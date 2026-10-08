//! 채널들이 함께 쓰는 길이 카운터와 볼륨 엔벨로프 (Pan Docs "Audio Registers").

/// 길이 카운터. 켜져 있으면 프레임 시퀀서가 줄이고, 0이 되면 채널을 끈다.
#[derive(Debug, Clone, Default)]
pub struct Length {
    pub enabled: bool,
    pub counter: u16,
}

impl Length {
    /// NRx1 쓰기. `max`는 채널 3이 256, 나머지가 64.
    pub fn load(&mut self, value: u8, max: u16) {
        self.counter = max - u16::from(value) % max;
    }

    /// 프레임 시퀀서 길이 클록. 카운터가 0이 되면 `true`(채널을 꺼야 한다).
    pub fn clock(&mut self) -> bool {
        if self.enabled && self.counter > 0 {
            self.counter -= 1;
            return self.counter == 0;
        }
        false
    }

    /// NRx4 쓰기의 길이 부분 (gbdev wiki "Game Boy Sound Hardware" Obscure Behavior).
    /// `quiet_step`: 다음 프레임 시퀀서 단계가 길이를 클록하지 않는다.
    /// 채널을 꺼야 하면 `true`.
    pub fn write_nrx4(&mut self, value: u8, max: u16, quiet_step: bool) -> bool {
        let was_enabled = self.enabled;
        self.enabled = value & 0x40 != 0;
        let trigger = value & 0x80 != 0;
        let mut disable = false;
        // 길이를 새로 켜면, 이번 길이 주기에 이미 지난 클록 하나를 바로 반영한다.
        if quiet_step && !was_enabled && self.enabled && self.counter > 0 {
            self.counter -= 1;
            disable = self.counter == 0 && !trigger;
        }
        if trigger && self.counter == 0 {
            self.counter = max;
            if self.enabled && quiet_step {
                self.counter -= 1;
            }
        }
        disable
    }
}

/// 볼륨 엔벨로프 (NRx2).
#[derive(Debug, Clone, Default)]
pub struct Envelope {
    initial: u8,
    up: bool,
    period: u8,
    pub volume: u8,
    timer: u8,
}

impl Envelope {
    pub fn write(&mut self, value: u8) {
        self.initial = value >> 4;
        self.up = value & 0x08 != 0;
        self.period = value & 0x07;
    }

    pub fn reg(&self) -> u8 {
        self.initial << 4 | u8::from(self.up) << 3 | self.period
    }

    /// 상위 5비트가 모두 0이면 DAC가 꺼진다.
    pub fn dac_on(&self) -> bool {
        self.reg() & 0xF8 != 0
    }

    pub fn trigger(&mut self) {
        self.volume = self.initial;
        self.timer = self.period;
    }

    /// 프레임 시퀀서 7단계 (64 Hz).
    pub fn clock(&mut self) {
        if self.period == 0 {
            return;
        }
        self.timer = self.timer.saturating_sub(1);
        if self.timer > 0 {
            return;
        }
        self.timer = self.period;
        if self.up && self.volume < 15 {
            self.volume += 1;
        } else if !self.up && self.volume > 0 {
            self.volume -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_counts_down_only_when_enabled() {
        let mut len = Length::default();
        len.load(62, 64);
        assert!(!len.clock());
        assert_eq!(len.counter, 2);
        len.enabled = true;
        assert!(!len.clock());
        assert!(len.clock(), "0이 되면 채널을 끈다");
        assert!(!len.clock(), "0에서는 더 줄지 않는다");
    }

    #[test]
    fn trigger_reloads_empty_length() {
        let mut len = Length::default();
        assert!(!len.write_nrx4(0x80, 256, false));
        assert_eq!(len.counter, 256);
    }

    #[test]
    fn enabling_length_on_quiet_step_clocks_once() {
        let mut len = Length::default();
        len.load(63, 64);
        assert!(len.write_nrx4(0x40, 64, true), "1→0이면 채널을 끈다");
        assert_eq!(len.counter, 0);
        len.enabled = false;
        len.load(60, 64);
        assert!(!len.write_nrx4(0x40, 64, false));
        assert_eq!(len.counter, 4, "다음 단계가 길이를 클록하면 그대로");
    }

    #[test]
    fn trigger_on_quiet_step_reloads_one_less() {
        let mut len = Length::default();
        assert!(!len.write_nrx4(0xC0, 64, true));
        assert_eq!(len.counter, 63);
    }

    #[test]
    fn envelope_steps_volume_every_period() {
        let mut env = Envelope::default();
        env.write(0x2A); // 볼륨 2, 증가, 주기 2
        env.trigger();
        env.clock();
        assert_eq!(env.volume, 2);
        env.clock();
        assert_eq!(env.volume, 3);
        env.write(0x10); // 주기 0: 멈춘다
        env.clock();
        env.clock();
        assert_eq!(env.volume, 3);
    }

    #[test]
    fn envelope_stops_at_limits() {
        let mut env = Envelope::default();
        env.write(0xF9); // 볼륨 15, 증가, 주기 1
        env.trigger();
        env.clock();
        assert_eq!(env.volume, 15);
        env.write(0x01); // 볼륨 0, 감소
        env.trigger();
        env.clock();
        assert_eq!(env.volume, 0);
    }

    #[test]
    fn dac_follows_upper_five_bits() {
        let mut env = Envelope::default();
        env.write(0x07);
        assert!(!env.dac_on());
        env.write(0x08);
        assert!(env.dac_on());
    }
}
