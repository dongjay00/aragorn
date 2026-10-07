//! 실제 경과 시간을 에뮬레이션 프레임 수로 바꾼다 (스펙 §5.1 속도 제어 계산).

use std::time::Duration;

/// DMG 한 프레임: 70224 T-사이클 / 4194304 Hz ≈ 16.74ms (59.73fps).
pub const FRAME_DURATION: Duration = Duration::from_nanos(16_742_706);

/// 한 번에 따라잡는 최대 프레임 수. 창이 멈췄다 돌아와도 한꺼번에 몰아서 돌리지 않는다.
pub const MAX_FRAMES_PER_TICK: u32 = 3;

#[derive(Debug, Default, Clone)]
pub struct FramePacer {
    /// 아직 프레임으로 바꾸지 못한 경과 시간.
    carry: Duration,
}

impl FramePacer {
    /// `elapsed`만큼 시간이 지났을 때 돌릴 프레임 수. 상한을 넘은 시간은 버린다.
    pub fn frames_for(&mut self, elapsed: Duration) -> u32 {
        let total = self.carry + elapsed;
        let frames = (total.as_nanos() / FRAME_DURATION.as_nanos()) as u32;
        if frames > MAX_FRAMES_PER_TICK {
            self.carry = Duration::ZERO;
            return MAX_FRAMES_PER_TICK;
        }
        self.carry = total - FRAME_DURATION * frames;
        frames
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
}
