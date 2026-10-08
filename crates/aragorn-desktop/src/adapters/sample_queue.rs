//! UI 스레드(에뮬레이터)와 오디오 콜백 스레드 사이의 샘플 큐.

use std::collections::VecDeque;

/// 인터리브 스테레오 샘플 큐. 비면 무음을 내고, `prime`만큼 다시 찰 때까지 기다렸다가 재생을 이어 간다.
/// 바로 이어 가면 콜백마다 조금씩 끊겨 지글거리는 소리가 난다.
pub struct SampleQueue {
    samples: VecDeque<f32>,
    /// 재생을 시작하거나 다시 시작할 때 필요한 샘플 수.
    prime: usize,
    /// 최대 샘플 수. 넘치는 샘플은 버린다.
    capacity: usize,
    playing: bool,
}

impl SampleQueue {
    /// `prime_frames`, `capacity_frames`: 스테레오 프레임 수.
    pub fn new(prime_frames: usize, capacity_frames: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(capacity_frames * 2),
            prime: prime_frames * 2,
            capacity: capacity_frames * 2,
            playing: false,
        }
    }

    pub fn queued_frames(&self) -> usize {
        self.samples.len() / 2
    }

    /// 들어갈 자리만큼만 넣는다. 스테레오 짝이 깨지지 않게 짝수 개 단위로 자른다.
    pub fn push(&mut self, samples: &[f32]) {
        let room = self.capacity.saturating_sub(self.samples.len()) & !1;
        let take = samples.len().min(room) & !1;
        self.samples.extend(&samples[..take]);
    }

    /// `out`을 인터리브 스테레오 샘플로 채운다. 모자라면 나머지는 무음이다.
    pub fn pop_into(&mut self, out: &mut [f32]) {
        if !self.playing && self.samples.len() >= self.prime {
            self.playing = true;
        }
        for sample in out.iter_mut() {
            *sample = if self.playing {
                self.samples.pop_front().unwrap_or_else(|| {
                    self.playing = false;
                    0.0
                })
            } else {
                0.0
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn popped(queue: &mut SampleQueue, len: usize) -> Vec<f32> {
        let mut out = vec![9.0; len];
        queue.pop_into(&mut out);
        out
    }

    #[test]
    fn waits_until_primed() {
        let mut queue = SampleQueue::new(2, 10);
        queue.push(&[0.1, 0.2]);
        assert_eq!(popped(&mut queue, 2), [0.0, 0.0], "1프레임은 부족하다");
        queue.push(&[0.3, 0.4]);
        assert_eq!(popped(&mut queue, 4), [0.1, 0.2, 0.3, 0.4]);
    }

    #[test]
    fn underrun_plays_silence_and_reprimes() {
        let mut queue = SampleQueue::new(2, 10);
        queue.push(&[0.1, 0.2, 0.3, 0.4]);
        assert_eq!(popped(&mut queue, 6), [0.1, 0.2, 0.3, 0.4, 0.0, 0.0]);
        queue.push(&[0.5, 0.6]);
        assert_eq!(popped(&mut queue, 2), [0.0, 0.0], "다시 찰 때까지 기다린다");
        assert_eq!(queue.queued_frames(), 1);
    }

    #[test]
    fn overflow_is_dropped_in_whole_frames() {
        let mut queue = SampleQueue::new(1, 2);
        queue.push(&[0.1, 0.2, 0.3, 0.4, 0.5, 0.6]);
        assert_eq!(queue.queued_frames(), 2);
        queue.push(&[0.7]);
        assert_eq!(queue.queued_frames(), 2);
        assert_eq!(popped(&mut queue, 4), [0.1, 0.2, 0.3, 0.4]);
    }

    #[test]
    fn odd_sample_count_keeps_channels_aligned() {
        let mut queue = SampleQueue::new(1, 10);
        queue.push(&[0.1, 0.2, 0.3]);
        assert_eq!(queue.queued_frames(), 1);
        assert_eq!(popped(&mut queue, 2), [0.1, 0.2]);
    }
}
