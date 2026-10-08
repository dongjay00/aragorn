//! UI 스레드(에뮬레이터)와 오디오 콜백 스레드 사이의 샘플 큐.

use std::collections::VecDeque;

/// 인터리브 스테레오 샘플 큐. 비면 무음을 내고, `prime`만큼 다시 찰 때까지 기다렸다가 재생을 이어 간다.
/// 바로 이어 가면 콜백마다 조금씩 끊겨 지글거리는 소리가 난다.
pub struct SampleQueue {
    samples: VecDeque<f32>,
    /// 재생을 시작하거나 다시 시작할 때 필요한 샘플 수.
    prime: usize,
    /// 최대 샘플 수. 넘치면 오래된 샘플을 버리고 최근 `prime`만큼만 남긴다.
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

    /// 샘플을 넣는다. 스테레오 짝이 깨지지 않게 짝수 개 단위로 자른다.
    /// 넘치면(콜백이 멈췄다 돌아왔거나 장치를 다시 열어 밀린 소리가 한꺼번에 들어오면) 오래된 샘플을
    /// 버리고 최근 `prime`만큼만 남긴다. 그러지 않으면 늦은 소리가 1분 가까이 이어진다.
    pub fn push(&mut self, samples: &[f32]) {
        self.samples.extend(&samples[..samples.len() & !1]);
        if self.samples.len() > self.capacity {
            let excess = self.samples.len() - self.prime.min(self.capacity);
            self.samples.drain(..excess);
        }
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
    fn overflow_keeps_only_newest_prime_frames() {
        // 콜백이 멈췄거나 장치를 다시 연 뒤 밀린 소리가 들어오면, 오래된 소리를 버리고
        // 최근 `prime`만큼만 남겨 지연을 바로 목표로 되돌린다.
        let mut queue = SampleQueue::new(1, 2);
        queue.push(&[0.1, 0.2, 0.3, 0.4, 0.5, 0.6]);
        assert_eq!(queue.queued_frames(), 1);
        assert_eq!(popped(&mut queue, 2), [0.5, 0.6]);
    }

    #[test]
    fn filling_up_to_capacity_keeps_everything() {
        let mut queue = SampleQueue::new(1, 2);
        queue.push(&[0.1, 0.2]);
        queue.push(&[0.3, 0.4]);
        assert_eq!(queue.queued_frames(), 2);
        queue.push(&[0.5, 0.6]);
        assert_eq!(queue.queued_frames(), 1);
        assert_eq!(popped(&mut queue, 2), [0.5, 0.6]);
    }

    #[test]
    fn odd_sample_count_keeps_channels_aligned() {
        let mut queue = SampleQueue::new(1, 10);
        queue.push(&[0.1, 0.2, 0.3]);
        assert_eq!(queue.queued_frames(), 1);
        assert_eq!(popped(&mut queue, 2), [0.1, 0.2]);
    }
}
