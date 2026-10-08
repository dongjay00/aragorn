//! 오디오 출력 포트와 샘플레이트 보정 (스펙 §5.1 "오디오 버퍼 채움 비율 → 리샘플링 비율 보정").
//!
//! 화면은 실제 시간에 맞춰 프레임을 돌리고, 오디오 장치는 자기 클록으로 샘플을 가져간다.
//! 두 클록의 차이는 장치 버퍼에 쌓인 양을 보고 에뮬레이터 샘플레이트를 ±0.5% 안에서 바꿔 메운다.

/// 장치 버퍼에 쌓아 둘 목표 분량(초).
pub const TARGET_LATENCY_SECS: f64 = 0.1;
/// 샘플레이트 보정 한도(±0.5%).
pub const MAX_RATE_ADJUST: f64 = 0.005;

/// 오디오 출력 장치 (포트). 샘플은 인터리브 스테레오 f32(-1.0–1.0).
pub trait AudioSink {
    /// 장치 샘플레이트(Hz).
    fn sample_rate(&self) -> u32;
    /// 장치 버퍼에 쌓인 양 ÷ 목표 분량(`target_frames`). 1.0이 목표다.
    fn fill_ratio(&self) -> f32;
    fn push(&mut self, samples: &[f32]);
}

/// 장치 버퍼에 쌓아 둘 목표 스테레오 프레임 수.
pub fn target_frames(sample_rate: u32) -> usize {
    (f64::from(sample_rate) * TARGET_LATENCY_SECS) as usize
}

/// 샘플레이트에 곱할 보정 비율. 버퍼가 목표보다 비어 있으면 조금 더 만들고, 넘치면 조금 덜 만든다.
pub fn rate_adjust(fill_ratio: f32) -> f64 {
    let fill = f64::from(fill_ratio);
    (1.0 + (1.0 - fill) * MAX_RATE_ADJUST).clamp(1.0 - MAX_RATE_ADJUST, 1.0 + MAX_RATE_ADJUST)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_is_a_tenth_of_a_second() {
        assert_eq!(target_frames(48_000), 4_800);
        assert_eq!(target_frames(44_100), 4_410);
    }

    #[test]
    fn rate_is_unchanged_at_target() {
        assert_eq!(rate_adjust(1.0), 1.0);
    }

    #[test]
    fn empty_buffer_speeds_up_and_full_buffer_slows_down() {
        assert_eq!(rate_adjust(0.0), 1.0 + MAX_RATE_ADJUST);
        assert_eq!(rate_adjust(2.0), 1.0 - MAX_RATE_ADJUST);
        assert!(rate_adjust(0.5) > 1.0);
        assert!(rate_adjust(1.5) < 1.0);
    }

    #[test]
    fn adjustment_is_capped() {
        assert_eq!(rate_adjust(100.0), 1.0 - MAX_RATE_ADJUST);
        assert_eq!(rate_adjust(-1.0), 1.0 + MAX_RATE_ADJUST);
    }
}
