//! 되감기: 몇 프레임마다 스테이트를 압축해 쌓아 두고, 되감는 동안 하나씩 꺼내 불러온다.

use std::collections::VecDeque;

/// 보통 속도에서 스테이트를 남기는 간격(프레임). 되감을 때는 프레임마다 하나씩 꺼내므로
/// 실제 시간의 이 배율로 거꾸로 간다.
pub const CAPTURE_INTERVAL: u32 = 2;
/// 되감을 수 있는 최대 길이(보통 속도 기준 약 20초).
pub const MAX_SNAPSHOTS: usize = 600;
/// 압축한 스테이트를 모두 합쳐 이만큼을 넘으면 오래된 것부터 버린다.
pub const MAX_BYTES: usize = 48 * 1024 * 1024;

/// 압축한 스테이트를 시간 순서로 쌓는다.
#[derive(Default)]
pub struct RewindBuffer {
    snapshots: VecDeque<Vec<u8>>,
    bytes: usize,
    /// 마지막으로 남긴 뒤 지난 프레임 수.
    since_capture: u32,
}

impl RewindBuffer {
    /// 프레임 하나가 지났다. 남길 차례면 `true`. `speed`배로 돌고 있으면 간격도 그만큼 늘린다
    /// (배속 중에 남긴 스테이트도 실제 시간으로 비슷한 간격이 된다).
    pub fn frame_passed(&mut self, speed: u32) -> bool {
        self.since_capture += 1;
        if self.since_capture >= CAPTURE_INTERVAL * speed.max(1) {
            self.since_capture = 0;
            true
        } else {
            false
        }
    }

    /// 스테이트를 압축해 쌓는다.
    pub fn push(&mut self, state: &[u8]) {
        let packed = lz4_flex::compress_prepend_size(state);
        self.bytes += packed.len();
        self.snapshots.push_back(packed);
        while self.snapshots.len() > MAX_SNAPSHOTS || self.bytes > MAX_BYTES {
            match self.snapshots.pop_front() {
                Some(old) => self.bytes -= old.len(),
                None => break,
            }
        }
    }

    /// 가장 최근 스테이트를 꺼낸다. 다 꺼냈으면 `None`.
    pub fn pop(&mut self) -> Option<Vec<u8>> {
        let packed = self.snapshots.pop_back()?;
        self.bytes -= packed.len();
        self.since_capture = 0;
        // 직접 압축한 것이라 풀리지 않을 수 없지만, 깨졌으면 버리고 다음 것으로 넘어가게 한다.
        lz4_flex::decompress_size_prepended(&packed).ok()
    }

    pub fn len(&self) -> usize {
        self.snapshots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }

    /// 압축한 크기의 합(바이트).
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// 모두 버린다 (스테이트를 불러오거나 되감기를 끌 때).
    pub fn clear(&mut self) {
        self.snapshots.clear();
        self.bytes = 0;
        self.since_capture = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_every_interval_scaled_by_speed() {
        let mut buffer = RewindBuffer::default();
        let captured = (0..12).filter(|_| buffer.frame_passed(1)).count();
        assert_eq!(captured, 12 / CAPTURE_INTERVAL as usize);
        let captured = (0..16).filter(|_| buffer.frame_passed(4)).count();
        assert_eq!(captured, 16 / (CAPTURE_INTERVAL as usize * 4));
    }

    #[test]
    fn pops_newest_first() {
        let mut buffer = RewindBuffer::default();
        buffer.push(&[1; 100]);
        buffer.push(&[2; 100]);
        assert_eq!(buffer.pop().unwrap(), [2; 100]);
        assert_eq!(buffer.pop().unwrap(), [1; 100]);
        assert_eq!(buffer.pop(), None);
        assert_eq!(buffer.bytes(), 0);
    }

    #[test]
    fn drops_oldest_beyond_the_limit() {
        let mut buffer = RewindBuffer::default();
        for i in 0..MAX_SNAPSHOTS + 5 {
            buffer.push(&(i as u32).to_le_bytes());
        }
        assert_eq!(buffer.len(), MAX_SNAPSHOTS);
        let mut oldest = None;
        while let Some(state) = buffer.pop() {
            oldest = Some(state);
        }
        assert_eq!(oldest.unwrap(), 5u32.to_le_bytes(), "앞의 5개는 버렸다");
    }

    #[test]
    fn similar_states_compress_well() {
        let mut buffer = RewindBuffer::default();
        buffer.push(&vec![0x11; 150_000]);
        assert!(buffer.bytes() < 2_000, "{}", buffer.bytes());
    }
}
