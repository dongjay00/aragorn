//! 채널 4: LFSR 노이즈 (Pan Docs "Sound Channel 4 — Noise").

use super::channel::{Envelope, Length};

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Noise {
    pub enabled: bool,
    pub length: Length,
    pub env: Envelope,
    shift: u8,
    /// 7비트 LFSR 모드.
    narrow: bool,
    divisor: u8,
    timer: u32,
    lfsr: u16,
}

impl Noise {
    /// 손상된 스테이트 때문에 패닉하지 않게 레지스터 비트 폭을 넘는 값을 감싼다.
    pub(crate) fn sanitize(&mut self) {
        self.shift &= 0x0F;
        self.divisor &= 7;
        self.lfsr &= 0x7FFF;
        self.env.sanitize();
    }

    pub fn reg(&self) -> u8 {
        self.shift << 4 | u8::from(self.narrow) << 3 | self.divisor
    }

    /// NR43 쓰기.
    pub fn write(&mut self, value: u8) {
        self.shift = value >> 4;
        self.narrow = value & 0x08 != 0;
        self.divisor = value & 0x07;
    }

    /// LFSR 클록 간격(T-사이클).
    fn period(&self) -> u32 {
        let base = if self.divisor == 0 {
            8
        } else {
            u32::from(self.divisor) * 16
        };
        base << self.shift
    }

    pub fn trigger(&mut self) {
        self.enabled = self.env.dac_on();
        self.env.trigger();
        self.timer = self.period();
        self.lfsr = 0x7FFF;
    }

    /// 1 T-사이클 진행한다. 시프트 14·15는 LFSR을 클록하지 않는다.
    pub fn tick(&mut self) {
        if self.timer > 1 {
            self.timer -= 1;
            return;
        }
        self.timer = self.period();
        if self.shift >= 14 {
            return;
        }
        let bit = (self.lfsr ^ (self.lfsr >> 1)) & 1;
        self.lfsr = (self.lfsr >> 1) | (bit << 14);
        if self.narrow {
            self.lfsr = (self.lfsr & !0x40) | (bit << 6);
        }
    }

    /// DAC 입력(0–15). DAC가 꺼져 있으면 `None`.
    pub fn output(&self) -> Option<u8> {
        let high = self.enabled && self.lfsr & 1 == 0;
        self.env
            .dac_on()
            .then_some(if high { self.env.volume } else { 0 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noise(nr43: u8) -> Noise {
        let mut ch = Noise::default();
        ch.env.write(0xF0);
        ch.write(nr43);
        ch.trigger();
        ch
    }

    /// LFSR이 한 바퀴 도는 데 필요한 클록 수.
    fn lfsr_cycle(ch: &mut Noise) -> u32 {
        let start = ch.lfsr;
        let period = ch.period();
        for n in 1..=40_000 {
            for _ in 0..period {
                ch.tick();
            }
            if ch.lfsr == start {
                return n;
            }
        }
        panic!("한 바퀴를 돌지 않는다");
    }

    #[test]
    fn wide_lfsr_has_period_32767() {
        assert_eq!(lfsr_cycle(&mut noise(0x00)), 32767);
    }

    #[test]
    fn narrow_lfsr_has_period_127() {
        let mut ch = noise(0x08);
        // 처음 몇 클록은 15비트 상태에서 7비트 주기로 들어가는 과정이다.
        for _ in 0..8 * 20 {
            ch.tick();
        }
        assert_eq!(lfsr_cycle(&mut ch), 127);
    }

    #[test]
    fn shift_14_freezes_lfsr() {
        let mut ch = noise(0xE0);
        for _ in 0..100_000 {
            ch.tick();
        }
        assert_eq!(ch.lfsr, 0x7FFF);
    }

    #[test]
    fn output_is_volume_when_lfsr_bit0_is_clear() {
        let mut ch = noise(0x00);
        assert_eq!(ch.output(), Some(0), "0x7FFF: 비트 0이 1");
        while ch.lfsr & 1 != 0 {
            ch.tick();
        }
        assert_eq!(ch.output(), Some(15));
    }
}
