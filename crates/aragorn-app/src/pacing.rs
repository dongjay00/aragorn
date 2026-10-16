//! 실제 경과 시간을 에뮬레이션 프레임 수로 바꾼다 (스펙 §5.1 속도 제어 계산).

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// DMG 한 프레임: 70224 T-사이클 / 4194304 Hz ≈ 16.74ms (59.73fps).
pub const FRAME_DURATION: Duration = Duration::from_nanos(16_742_706);

/// 한 번에 따라잡는 최대 프레임 수. 창이 멈췄다 돌아와도 한꺼번에 몰아서 돌리지 않는다.
pub const MAX_FRAMES_PER_TICK: u32 = 3;

/// 무제한 배속에서 화면 갱신 한 번에 에뮬레이션에 쓰는 최대 시간. 나머지는 UI 몫이다.
pub const UNLIMITED_BUDGET: Duration = Duration::from_millis(12);

/// 무제한 배속에서 화면 갱신 한 번에 돌리는 프레임 상한. 시간 확인이 고장 나도 UI를 붙잡지 않는다.
pub const UNLIMITED_MAX_FRAMES: u32 = 120;

/// 배속 키를 누르고 있는 동안의 속도 (스펙 §5.1: 2×/4×/무제한). 빠른 순서로 비교한다.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FastForward {
    X2,
    #[default]
    X4,
    Unlimited,
}

impl FastForward {
    pub const ALL: [FastForward; 3] = [FastForward::X2, FastForward::X4, FastForward::Unlimited];

    /// 실제 시간 대비 배율. 무제한이면 `None`.
    pub fn multiplier(self) -> Option<u32> {
        match self {
            FastForward::X2 => Some(2),
            FastForward::X4 => Some(4),
            FastForward::Unlimited => None,
        }
    }

    /// 설정 파일에 쓰는 이름(`x2`, `x4`, `unlimited`)으로 찾는다.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "x2" => Some(FastForward::X2),
            "x4" => Some(FastForward::X4),
            "unlimited" => Some(FastForward::Unlimited),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            FastForward::X2 => "2배",
            FastForward::X4 => "4배",
            FastForward::Unlimited => "무제한",
        }
    }
}

/// 배속 키를 누르지 않을 때의 기본 속도. 무제한은 CPU를 계속 붙잡으므로 고를 수 없다.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BaseSpeed {
    #[default]
    X1,
    X2,
    X4,
}

impl BaseSpeed {
    pub const ALL: [BaseSpeed; 3] = [BaseSpeed::X1, BaseSpeed::X2, BaseSpeed::X4];

    /// 보통 속도보다 빠르면 그에 해당하는 배속.
    pub fn fast(self) -> Option<FastForward> {
        match self {
            BaseSpeed::X1 => None,
            BaseSpeed::X2 => Some(FastForward::X2),
            BaseSpeed::X4 => Some(FastForward::X4),
        }
    }

    /// 설정 파일에 쓰는 이름(`x1`, `x2`, `x4`)으로 찾는다.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "x1" => Some(BaseSpeed::X1),
            "x2" => Some(BaseSpeed::X2),
            "x4" => Some(BaseSpeed::X4),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            BaseSpeed::X1 => "1배",
            BaseSpeed::X2 => "2배",
            BaseSpeed::X4 => "4배",
        }
    }
}

/// 이번 화면 갱신에서 에뮬레이션을 어떻게 돌릴지.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    Paused,
    Normal,
    Fast(FastForward),
}

/// 일시정지 토글과 배속 키 상태로 실행 모드를 정한다.
#[derive(Debug, Default, Clone)]
pub struct SpeedControl {
    paused: bool,
}

impl SpeedControl {
    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
    }

    /// 일시정지한다 (디버거의 브레이크포인트·단계 실행).
    pub fn pause(&mut self) {
        self.paused = true;
    }

    /// 일시정지를 푼다 (새 ROM을 열 때).
    pub fn resume(&mut self) {
        self.paused = false;
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// 일시정지 중에는 배속 키를 눌러도 멈춰 있다. 배속 키를 누르면 기본 속도와 배속 속도 중
    /// 빠른 쪽으로 돈다. 누르는 동안 오히려 느려지지 않게 한다.
    pub fn mode(&self, fast_held: bool, fast_forward: FastForward, base: BaseSpeed) -> RunMode {
        if self.paused {
            RunMode::Paused
        } else if fast_held {
            RunMode::Fast(base.fast().map_or(fast_forward, |b| b.max(fast_forward)))
        } else {
            base.fast().map_or(RunMode::Normal, RunMode::Fast)
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct FramePacer {
    /// 아직 프레임으로 바꾸지 못한 경과 시간.
    carry: Duration,
}

impl FramePacer {
    /// `elapsed`만큼 시간이 지났을 때 돌릴 프레임 수. 상한을 넘은 시간은 버린다.
    pub fn frames_for(&mut self, elapsed: Duration) -> u32 {
        self.frames_scaled(elapsed, 1)
    }

    /// 실제 시간의 `multiplier`배 속도로 돌릴 프레임 수. 상한도 같은 배율로 늘어난다.
    pub fn frames_scaled(&mut self, elapsed: Duration, multiplier: u32) -> u32 {
        let total = self.carry + elapsed * multiplier;
        let frames = (total.as_nanos() / FRAME_DURATION.as_nanos()) as u32;
        let cap = MAX_FRAMES_PER_TICK * multiplier;
        if frames > cap {
            self.carry = Duration::ZERO;
            return cap;
        }
        self.carry = total - FRAME_DURATION * frames;
        frames
    }

    /// 남은 시간을 버린다. 일시정지나 무제한 배속 뒤에 몰아서 돌리지 않게 한다.
    pub fn reset(&mut self) {
        self.carry = Duration::ZERO;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_frame_duration_runs_one_frame() {
        let mut p = FramePacer::default();
        assert_eq!(p.frames_for(FRAME_DURATION), 1);
        assert_eq!(p.frames_for(Duration::ZERO), 0);
    }

    #[test]
    fn leftover_time_carries_over() {
        let mut p = FramePacer::default();
        assert_eq!(p.frames_for(FRAME_DURATION / 2), 0);
        assert_eq!(
            p.frames_for(FRAME_DURATION / 2 + Duration::from_nanos(1)),
            1
        );
    }

    #[test]
    fn long_pause_is_capped_and_forgotten() {
        let mut p = FramePacer::default();
        assert_eq!(p.frames_for(Duration::from_secs(1)), MAX_FRAMES_PER_TICK);
        assert_eq!(p.frames_for(FRAME_DURATION / 2), 0);
    }

    #[test]
    fn fast_forward_scales_frames_and_cap() {
        let mut p = FramePacer::default();
        assert_eq!(p.frames_scaled(FRAME_DURATION, 4), 4);
        assert_eq!(p.frames_scaled(FRAME_DURATION / 2, 2), 1);
        assert_eq!(
            p.frames_scaled(Duration::from_secs(1), 4),
            MAX_FRAMES_PER_TICK * 4
        );
    }

    #[test]
    fn reset_forgets_leftover_time() {
        let mut p = FramePacer::default();
        assert_eq!(p.frames_for(FRAME_DURATION / 2), 0);
        p.reset();
        assert_eq!(
            p.frames_for(FRAME_DURATION / 2 + Duration::from_nanos(1)),
            0
        );
    }

    #[test]
    fn fast_forward_multipliers() {
        assert_eq!(FastForward::X2.multiplier(), Some(2));
        assert_eq!(FastForward::X4.multiplier(), Some(4));
        assert_eq!(FastForward::Unlimited.multiplier(), None);
        assert_eq!(FastForward::default(), FastForward::X4);
    }

    #[test]
    fn fast_forward_names_match_serde() {
        for f in FastForward::ALL {
            let name = serde_json::to_value(f).unwrap();
            assert_eq!(FastForward::from_name(name.as_str().unwrap()), Some(f));
        }
        assert_eq!(FastForward::from_name("x8"), None);
    }

    #[test]
    fn speed_control_modes() {
        let mut c = SpeedControl::default();
        assert_eq!(
            c.mode(false, FastForward::X2, BaseSpeed::X1),
            RunMode::Normal
        );
        assert_eq!(
            c.mode(true, FastForward::X2, BaseSpeed::X1),
            RunMode::Fast(FastForward::X2)
        );
        c.toggle_pause();
        assert!(c.is_paused());
        assert_eq!(
            c.mode(true, FastForward::X2, BaseSpeed::X1),
            RunMode::Paused
        );
        c.toggle_pause();
        assert_eq!(
            c.mode(false, FastForward::X2, BaseSpeed::X1),
            RunMode::Normal
        );
        c.pause();
        c.pause();
        assert!(c.is_paused(), "pause는 토글이 아니다");
        c.resume();
        c.resume();
        assert!(!c.is_paused(), "resume도 토글이 아니다");
    }

    #[test]
    fn base_speed_sets_mode_without_key() {
        let c = SpeedControl::default();
        assert_eq!(BaseSpeed::default(), BaseSpeed::X1);
        assert_eq!(
            c.mode(false, FastForward::X4, BaseSpeed::X1),
            RunMode::Normal
        );
        assert_eq!(
            c.mode(false, FastForward::X4, BaseSpeed::X2),
            RunMode::Fast(FastForward::X2)
        );
    }

    #[test]
    fn fast_key_never_slows_base_speed() {
        let c = SpeedControl::default();
        assert_eq!(
            c.mode(true, FastForward::X2, BaseSpeed::X4),
            RunMode::Fast(FastForward::X4)
        );
        assert_eq!(
            c.mode(true, FastForward::Unlimited, BaseSpeed::X2),
            RunMode::Fast(FastForward::Unlimited)
        );
    }

    #[test]
    fn paused_ignores_base_speed() {
        let mut c = SpeedControl::default();
        c.pause();
        assert_eq!(
            c.mode(false, FastForward::X4, BaseSpeed::X4),
            RunMode::Paused
        );
    }

    #[test]
    fn base_speed_names_match_serde() {
        for b in BaseSpeed::ALL {
            let name = serde_json::to_value(b).unwrap();
            assert_eq!(BaseSpeed::from_name(name.as_str().unwrap()), Some(b));
        }
        assert_eq!(BaseSpeed::from_name("unlimited"), None);
    }
}
