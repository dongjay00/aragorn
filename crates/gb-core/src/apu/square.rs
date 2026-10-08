//! 채널 1·2: 구형파, 채널 1의 주파수 스윕 (Pan Docs "Sound Channel 1 — Pulse with period sweep").

use super::channel::{Envelope, Length};

/// 듀티별 8단계 파형 (12.5%, 25%, 50%, 75%).
const DUTY: [u8; 4] = [0b0000_0001, 0b1000_0001, 0b1000_0111, 0b0111_1110];

#[derive(Debug, Clone, Default)]
pub struct Square {
    pub enabled: bool,
    pub duty: u8,
    pub length: Length,
    pub env: Envelope,
    /// 11비트 주기 값 (NRx3, NRx4 하위 3비트).
    pub freq: u16,
    timer: u32,
    pub step: u8,
}

impl Square {
    /// 한 듀티 단계의 길이(T-사이클).
    fn period(&self) -> u32 {
        (2048 - u32::from(self.freq)) * 4
    }

    pub fn trigger(&mut self) {
        self.enabled = self.env.dac_on();
        self.env.trigger();
        self.timer = self.period();
    }

    /// 1 T-사이클 진행한다.
    pub fn tick(&mut self) {
        if self.timer <= 1 {
            self.timer = self.period();
            self.step = (self.step + 1) & 7;
        } else {
            self.timer -= 1;
        }
    }

    /// DAC 입력(0–15). DAC가 꺼져 있으면 `None`.
    pub fn output(&self) -> Option<u8> {
        self.env.dac_on().then(|| {
            let high = DUTY[usize::from(self.duty)] >> (7 - self.step) & 1 != 0;
            if self.enabled && high {
                self.env.volume
            } else {
                0
            }
        })
    }
}

/// 채널 1 주파수 스윕 (NR10).
#[derive(Debug, Clone, Default)]
pub struct Sweep {
    period: u8,
    negate: bool,
    shift: u8,
    timer: u8,
    enabled: bool,
    shadow: u16,
    /// 마지막 트리거 뒤 감소 모드로 계산한 적이 있는지.
    negate_used: bool,
}

impl Sweep {
    pub fn reg(&self) -> u8 {
        self.period << 4 | u8::from(self.negate) << 3 | self.shift
    }

    /// NR10 쓰기. 감소 모드로 계산한 뒤 증가 모드로 바꾸면 채널이 꺼진다.
    pub fn write(&mut self, value: u8, ch: &mut Square) {
        let negate = value & 0x08 != 0;
        if self.negate && !negate && self.negate_used {
            ch.enabled = false;
        }
        self.period = (value >> 4) & 7;
        self.negate = negate;
        self.shift = value & 7;
    }

    pub fn trigger(&mut self, ch: &mut Square) {
        self.shadow = ch.freq;
        self.reload_timer();
        self.enabled = self.period != 0 || self.shift != 0;
        self.negate_used = false;
        if self.shift != 0 {
            self.next_freq(ch);
        }
    }

    /// 프레임 시퀀서 2·6단계 (128 Hz).
    pub fn clock(&mut self, ch: &mut Square) {
        self.timer = self.timer.saturating_sub(1);
        if self.timer > 0 {
            return;
        }
        self.reload_timer();
        if !self.enabled || self.period == 0 {
            return;
        }
        let freq = self.next_freq(ch);
        if freq <= 2047 && self.shift != 0 {
            self.shadow = freq;
            ch.freq = freq;
            self.next_freq(ch);
        }
    }

    /// 주기 0은 8로 센다.
    fn reload_timer(&mut self) {
        self.timer = if self.period == 0 { 8 } else { self.period };
    }

    /// 다음 주파수를 계산한다. 2047을 넘으면 채널을 끈다.
    fn next_freq(&mut self, ch: &mut Square) -> u16 {
        let delta = self.shadow >> self.shift;
        let freq = if self.negate {
            self.negate_used = true;
            self.shadow - delta
        } else {
            self.shadow + delta
        };
        if freq > 2047 {
            ch.enabled = false;
        }
        freq
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(freq: u16, duty: u8) -> Square {
        let mut ch = Square {
            freq,
            duty,
            ..Square::default()
        };
        ch.env.write(0xF0);
        ch.trigger();
        ch
    }

    #[test]
    fn duty_step_advances_every_period() {
        let mut ch = square(2047, 2); // 주기 4 T-사이클
        for _ in 0..3 {
            ch.tick();
        }
        assert_eq!(ch.step, 0);
        ch.tick();
        assert_eq!(ch.step, 1);
    }

    #[test]
    fn output_follows_duty_waveform() {
        let mut ch = square(2047, 0); // 12.5%: 마지막 단계만 높다
        let mut highs = 0;
        for _ in 0..8 {
            for _ in 0..4 {
                ch.tick();
            }
            if ch.output() == Some(15) {
                highs += 1;
            }
        }
        assert_eq!(highs, 1);
    }

    #[test]
    fn dac_off_outputs_nothing() {
        let mut ch = square(0, 2);
        ch.env.write(0x00);
        assert_eq!(ch.output(), None);
    }

    #[test]
    fn sweep_overflow_on_trigger_disables_channel() {
        let mut ch = square(0x700, 2);
        let mut sweep = Sweep::default();
        sweep.write(0x11, &mut ch); // 주기 1, 증가, 시프트 1: 0x700 + 0x380 > 2047
        sweep.trigger(&mut ch);
        assert!(!ch.enabled);
    }

    #[test]
    fn sweep_updates_frequency_every_period() {
        let mut ch = square(0x100, 2);
        let mut sweep = Sweep::default();
        sweep.write(0x21, &mut ch); // 주기 2, 증가, 시프트 1
        sweep.trigger(&mut ch);
        sweep.clock(&mut ch);
        assert_eq!(ch.freq, 0x100);
        sweep.clock(&mut ch);
        assert_eq!(ch.freq, 0x180);
        assert!(ch.enabled);
    }

    #[test]
    fn leaving_negate_mode_after_use_disables_channel() {
        let mut ch = square(0x400, 2);
        let mut sweep = Sweep::default();
        sweep.write(0x19, &mut ch); // 주기 1, 감소, 시프트 1
        sweep.trigger(&mut ch);
        assert!(ch.enabled);
        sweep.write(0x11, &mut ch);
        assert!(!ch.enabled);
    }
}
