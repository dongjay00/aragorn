//! 채널 3: 웨이브 RAM 재생 (Pan Docs "Sound Channel 3 — Wave output").
//!
//! DMG에서 채널이 켜져 있는 동안 CPU가 웨이브 RAM에 접근하면, 채널이 바로 그 T-사이클에
//! 읽은 바이트에만 닿는다. 다른 때에는 읽기는 0xFF, 쓰기는 무시된다.
//! 아래 타이밍 상수는 Blargg dmg_sound 09·10·12로 맞춘 값이다.

use super::channel::Length;

/// 트리거 뒤 첫 샘플을 읽기까지 추가로 걸리는 T-사이클.
const TRIGGER_DELAY: u32 = 6;
/// 남은 타이머가 이 값 이하일 때(다음 2 MHz 클록에 읽을 때) 다시 트리거하면 웨이브 RAM이 망가진다.
const CORRUPTION_WINDOW: u32 = 2;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Wave {
    /// CGB는 채널이 켜져 있어도 웨이브 RAM(현재 바이트)에 언제나 접근할 수 있고, 다시 트리거해도 망가지지 않는다.
    pub cgb: bool,
    pub enabled: bool,
    pub dac: bool,
    pub length: Length,
    /// NR32 출력 레벨 (0: 무음, 1: 100%, 2: 50%, 3: 25%).
    pub level: u8,
    pub freq: u16,
    timer: u32,
    /// 재생 중인 4비트 샘플 위치 (0–31).
    position: u8,
    /// 마지막으로 웨이브 RAM에서 읽은 바이트.
    buffer: u8,
    /// 마지막 웨이브 RAM 읽기 뒤 지난 T-사이클.
    since_read: u32,
    ram: [u8; 16],
}

impl Default for Wave {
    fn default() -> Self {
        Self {
            cgb: false,
            enabled: false,
            dac: false,
            length: Length::default(),
            level: 0,
            freq: 0,
            timer: 0,
            position: 0,
            buffer: 0,
            since_read: u32::MAX,
            ram: [0; 16],
        }
    }
}

impl Wave {
    /// 손상된 스테이트 때문에 패닉하지 않게 레지스터 비트 폭을 넘는 값을 감싼다.
    pub(crate) fn sanitize(&mut self) {
        self.freq &= 0x7FF;
        self.level &= 3;
        self.position &= 31;
    }

    /// 한 샘플의 길이(T-사이클).
    fn period(&self) -> u32 {
        (2048 - u32::from(self.freq)) * 2
    }

    /// 전원을 끌 때: 웨이브 RAM만 남기고 모두 지운다.
    pub fn power_off(&mut self) {
        *self = Wave {
            cgb: self.cgb,
            ram: self.ram,
            ..Wave::default()
        };
    }

    /// 전원을 켤 때 샘플 버퍼를 비운다.
    pub fn clear_buffer(&mut self) {
        self.buffer = 0;
    }

    /// CPU가 보는 웨이브 RAM 칸. DMG에서 채널이 켜져 있고 지금 읽는 순간이 아니면 `None`.
    fn cpu_index(&self, index: usize) -> Option<usize> {
        if !self.enabled {
            return Some(index);
        }
        (self.cgb || self.since_read == 0).then_some(usize::from(self.position / 2))
    }

    pub fn read_ram(&self, index: usize) -> u8 {
        self.cpu_index(index).map_or(0xFF, |i| self.ram[i])
    }

    pub fn write_ram(&mut self, index: usize, value: u8) {
        if let Some(i) = self.cpu_index(index) {
            self.ram[i] = value;
        }
    }

    pub fn trigger(&mut self) {
        if !self.cgb && self.enabled && self.timer <= CORRUPTION_WINDOW {
            self.corrupt_ram();
        }
        self.enabled = self.dac;
        self.position = 0;
        self.timer = self.period() + TRIGGER_DELAY;
    }

    /// DMG 버그: 읽으려던 바이트가 앞 4바이트 안이면 0번에, 아니면 그 바이트가 속한 4바이트 묶음을
    /// 0–3번에 덮어쓴다.
    fn corrupt_ram(&mut self) {
        let next = usize::from(((self.position + 1) & 31) / 2);
        if next < 4 {
            self.ram[0] = self.ram[next];
        } else {
            let block = next & !3;
            self.ram.copy_within(block..block + 4, 0);
        }
    }

    /// 1 T-사이클 진행한다.
    pub fn tick(&mut self) {
        self.since_read = self.since_read.saturating_add(1);
        if !self.enabled {
            return;
        }
        if self.timer <= 1 {
            self.timer = self.period();
            self.position = (self.position + 1) & 31;
            self.buffer = self.ram[usize::from(self.position / 2)];
            self.since_read = 0;
        } else {
            self.timer -= 1;
        }
    }

    /// DAC 입력(0–15). DAC가 꺼져 있으면 `None`.
    pub fn output(&self) -> Option<u8> {
        self.dac.then(|| {
            if !self.enabled || self.level == 0 {
                return 0;
            }
            let sample = if self.position & 1 == 0 {
                self.buffer >> 4
            } else {
                self.buffer & 0x0F
            };
            sample >> (self.level - 1)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn playing(freq: u16) -> Wave {
        let mut ch = Wave {
            dac: true,
            level: 1,
            freq,
            ..Wave::default()
        };
        for (i, byte) in ch.ram.iter_mut().enumerate() {
            *byte = (i as u8) << 4 | i as u8;
        }
        ch.trigger();
        ch
    }

    #[test]
    fn ram_is_freely_accessible_while_off() {
        let mut ch = Wave::default();
        ch.write_ram(3, 0x42);
        assert_eq!(ch.read_ram(3), 0x42);
    }

    #[test]
    fn first_sample_is_read_after_trigger_delay() {
        let mut ch = playing(2047); // 샘플당 2 T-사이클
        for _ in 0..2 + TRIGGER_DELAY - 1 {
            ch.tick();
        }
        assert_eq!(ch.read_ram(5), 0xFF, "읽기 전에는 막혀 있다");
        ch.tick();
        assert_eq!(ch.read_ram(5), 0x00, "position 1 → 0번 바이트");
        assert_eq!(ch.output(), Some(0x00));
        ch.tick();
        assert_eq!(ch.read_ram(5), 0xFF, "읽은 다음 T-사이클에는 다시 막힌다");
    }

    #[test]
    fn output_level_shifts_sample() {
        let mut ch = playing(2047);
        for _ in 0..2 + TRIGGER_DELAY + 4 {
            ch.tick();
        }
        // position 3 → 1번 바이트 0x11의 하위 니블
        assert_eq!(ch.output(), Some(1));
        ch.level = 0;
        assert_eq!(ch.output(), Some(0));
        ch.dac = false;
        assert_eq!(ch.output(), None);
    }

    #[test]
    fn retrigger_while_reading_corrupts_first_bytes() {
        let mut ch = playing(2047);
        // position 9까지 진행: 다음 읽기는 position 10 → 5번 바이트(4–7 묶음)
        for _ in 0..2 + TRIGGER_DELAY + 2 * 8 {
            ch.tick();
        }
        ch.tick();
        ch.trigger();
        assert_eq!(&ch.ram[0..4], &[0x44, 0x55, 0x66, 0x77]);
    }

    #[test]
    fn cgb_ram_is_accessible_while_playing() {
        let mut ch = playing(2047);
        ch.cgb = true;
        assert_eq!(ch.read_ram(9), 0x00, "언제나 현재 바이트(0번)");
        ch.write_ram(9, 0xAB);
        assert_eq!(ch.ram[0], 0xAB);
    }

    #[test]
    fn cgb_retrigger_does_not_corrupt_ram() {
        let mut ch = playing(2047);
        ch.cgb = true;
        for _ in 0..2 + TRIGGER_DELAY + 2 * 8 + 1 {
            ch.tick();
        }
        ch.trigger();
        assert_eq!(&ch.ram[0..4], &[0x00, 0x11, 0x22, 0x33]);
    }
}
