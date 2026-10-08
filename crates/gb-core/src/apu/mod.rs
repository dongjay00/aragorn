//! APU: 소리 채널 4개, 프레임 시퀀서, 믹서 (Pan Docs "Audio", gbdev wiki "Game Boy Sound Hardware").
//!
//! 채널 타이머는 T-사이클 단위로 진행하고, 믹서는 M-사이클마다 값을 하나 받는다.
//! 프레임 시퀀서는 타이머 DIV 비트 4의 하강 에지(512 Hz)에 맞춰 버스가 부른다.

mod channel;
mod mixer;
mod noise;
mod square;
mod wave;

use mixer::Mixer;
use noise::Noise;
use square::{Square, Sweep};
use wave::Wave;

pub const NR10: u16 = 0xFF10;
pub const NR52: u16 = 0xFF26;
pub const WAVE_RAM: u16 = 0xFF30;
pub const WAVE_RAM_END: u16 = 0xFF3F;
/// CGB: 채널 1·2, 3·4의 현재 디지털 출력(각 4비트).
pub const PCM12: u16 = 0xFF76;
pub const PCM34: u16 = 0xFF77;

/// FF10–FF2F를 읽을 때 1로 보이는 비트 (쓰기 전용 비트와 빈 레지스터).
const READ_MASK: [u8; 0x20] = [
    0x80, 0x3F, 0x00, 0xFF, 0xBF, // NR10–NR14
    0xFF, 0x3F, 0x00, 0xFF, 0xBF, // 빈 칸, NR21–NR24
    0x7F, 0xFF, 0x9F, 0xFF, 0xBF, // NR30–NR34
    0xFF, 0xFF, 0x00, 0x00, 0xBF, // 빈 칸, NR41–NR44
    0x00, 0x00, 0x70, // NR50–NR52
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, // 빈 칸
];

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Apu {
    cgb: bool,
    power: bool,
    sweep: Sweep,
    ch1: Square,
    ch2: Square,
    ch3: Wave,
    ch4: Noise,
    /// 다음에 실행할 프레임 시퀀서 단계 (0–7).
    fs_step: u8,
    nr50: u8,
    nr51: u8,
    mixer: Mixer,
}

impl Default for Apu {
    fn default() -> Self {
        Self::new(false)
    }
}

impl Apu {
    /// 부트 ROM이 끝난 직후 상태 (Pan Docs "Power Up Sequence"): 채널 1이 부트 효과음을 마친 채 켜져 있다.
    pub fn new(cgb: bool) -> Self {
        let mut apu = Self {
            cgb,
            power: true,
            sweep: Sweep::default(),
            ch1: Square::default(),
            ch2: Square::default(),
            ch3: Wave::default(),
            ch4: Noise::default(),
            fs_step: 0,
            nr50: 0x77,
            nr51: 0xF3,
            mixer: Mixer::default(),
        };
        apu.write(0xFF11, 0xBF);
        apu.write(0xFF12, 0xF3);
        apu.ch1.enabled = true;
        apu.ch3.cgb = cgb;
        apu
    }

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            WAVE_RAM..=WAVE_RAM_END => self.ch3.read_ram(usize::from(addr - WAVE_RAM)),
            PCM12 => digital(self.ch2.output()) << 4 | digital(self.ch1.output()),
            PCM34 => digital(self.ch4.output()) << 4 | digital(self.ch3.output()),
            NR52 => {
                u8::from(self.power) << 7
                    | 0x70
                    | u8::from(self.ch1.enabled)
                    | u8::from(self.ch2.enabled) << 1
                    | u8::from(self.ch3.enabled) << 2
                    | u8::from(self.ch4.enabled) << 3
            }
            _ => self.reg(addr) | READ_MASK[usize::from(addr - NR10)],
        }
    }

    /// 읽을 수 있는 비트만 담은 레지스터 값.
    fn reg(&self, addr: u16) -> u8 {
        match addr {
            0xFF10 => self.sweep.reg(),
            0xFF11 => self.ch1.duty << 6,
            0xFF12 => self.ch1.env.reg(),
            0xFF14 => u8::from(self.ch1.length.enabled) << 6,
            0xFF16 => self.ch2.duty << 6,
            0xFF17 => self.ch2.env.reg(),
            0xFF19 => u8::from(self.ch2.length.enabled) << 6,
            0xFF1A => u8::from(self.ch3.dac) << 7,
            0xFF1C => self.ch3.level << 5,
            0xFF1E => u8::from(self.ch3.length.enabled) << 6,
            0xFF21 => self.ch4.env.reg(),
            0xFF22 => self.ch4.reg(),
            0xFF23 => u8::from(self.ch4.length.enabled) << 6,
            0xFF24 => self.nr50,
            0xFF25 => self.nr51,
            _ => 0,
        }
    }

    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            WAVE_RAM..=WAVE_RAM_END => self.ch3.write_ram(usize::from(addr - WAVE_RAM), value),
            NR52 => self.write_power(value & 0x80 != 0),
            _ if self.power => self.write_reg(addr, value),
            // DMG: 전원이 꺼져 있어도 길이 카운터는 쓸 수 있다. CGB는 무시한다.
            0xFF11 if !self.cgb => self.ch1.length.load(value, 64),
            0xFF16 if !self.cgb => self.ch2.length.load(value, 64),
            0xFF1B if !self.cgb => self.ch3.length.load(value, 256),
            0xFF20 if !self.cgb => self.ch4.length.load(value, 64),
            _ => {}
        }
    }

    fn write_reg(&mut self, addr: u16, value: u8) {
        // 다음 단계가 길이를 클록하지 않으면 NRx4 쓰기가 길이를 한 번 더 줄일 수 있다.
        let quiet_step = self.fs_step & 1 == 1;
        match addr {
            0xFF10 => self.sweep.write(value, &mut self.ch1),
            0xFF11 => {
                self.ch1.duty = value >> 6;
                self.ch1.length.load(value, 64);
            }
            0xFF12 => {
                self.ch1.env.write(value);
                self.ch1.enabled &= self.ch1.env.dac_on();
            }
            0xFF13 => self.ch1.freq = self.ch1.freq & 0x700 | u16::from(value),
            0xFF14 => {
                self.ch1.freq = self.ch1.freq & 0xFF | u16::from(value & 7) << 8;
                if self.ch1.length.write_nrx4(value, 64, quiet_step) {
                    self.ch1.enabled = false;
                }
                if value & 0x80 != 0 {
                    self.ch1.trigger();
                    self.sweep.trigger(&mut self.ch1);
                }
            }
            0xFF16 => {
                self.ch2.duty = value >> 6;
                self.ch2.length.load(value, 64);
            }
            0xFF17 => {
                self.ch2.env.write(value);
                self.ch2.enabled &= self.ch2.env.dac_on();
            }
            0xFF18 => self.ch2.freq = self.ch2.freq & 0x700 | u16::from(value),
            0xFF19 => {
                self.ch2.freq = self.ch2.freq & 0xFF | u16::from(value & 7) << 8;
                if self.ch2.length.write_nrx4(value, 64, quiet_step) {
                    self.ch2.enabled = false;
                }
                if value & 0x80 != 0 {
                    self.ch2.trigger();
                }
            }
            0xFF1A => {
                self.ch3.dac = value & 0x80 != 0;
                self.ch3.enabled &= self.ch3.dac;
            }
            0xFF1B => self.ch3.length.load(value, 256),
            0xFF1C => self.ch3.level = (value >> 5) & 3,
            0xFF1D => self.ch3.freq = self.ch3.freq & 0x700 | u16::from(value),
            0xFF1E => {
                self.ch3.freq = self.ch3.freq & 0xFF | u16::from(value & 7) << 8;
                if self.ch3.length.write_nrx4(value, 256, quiet_step) {
                    self.ch3.enabled = false;
                }
                if value & 0x80 != 0 {
                    self.ch3.trigger();
                }
            }
            0xFF20 => self.ch4.length.load(value, 64),
            0xFF21 => {
                self.ch4.env.write(value);
                self.ch4.enabled &= self.ch4.env.dac_on();
            }
            0xFF22 => self.ch4.write(value),
            0xFF23 => {
                if self.ch4.length.write_nrx4(value, 64, quiet_step) {
                    self.ch4.enabled = false;
                }
                if value & 0x80 != 0 {
                    self.ch4.trigger();
                }
            }
            0xFF24 => self.nr50 = value,
            0xFF25 => self.nr51 = value,
            _ => {}
        }
    }

    /// NR52 비트 7. 끄면 레지스터가 모두 지워지고(웨이브 RAM은 남고, DMG는 길이 카운터도 남는다),
    /// 켜면 프레임 시퀀서가 0단계부터 다시 시작한다.
    fn write_power(&mut self, on: bool) {
        if self.power && !on {
            let lengths = [
                self.ch1.length.counter,
                self.ch2.length.counter,
                self.ch3.length.counter,
                self.ch4.length.counter,
            ];
            self.sweep = Sweep::default();
            self.ch1 = Square::default();
            self.ch2 = Square::default();
            self.ch3.power_off();
            self.ch4 = Noise::default();
            self.nr50 = 0;
            self.nr51 = 0;
            if !self.cgb {
                self.ch1.length.counter = lengths[0];
                self.ch2.length.counter = lengths[1];
                self.ch3.length.counter = lengths[2];
                self.ch4.length.counter = lengths[3];
            }
        } else if !self.power && on {
            self.fs_step = 0;
            self.ch1.step = 0;
            self.ch2.step = 0;
            self.ch3.clear_buffer();
        }
        self.power = on;
    }

    /// DIV 비트 4의 하강 에지(512 Hz). 0·2·4·6단계는 길이, 2·6단계는 스윕, 7단계는 엔벨로프를 클록한다.
    pub fn frame_sequencer(&mut self) {
        if !self.power {
            return;
        }
        let step = self.fs_step;
        self.fs_step = (step + 1) & 7;
        if step.is_multiple_of(2) {
            if self.ch1.length.clock() {
                self.ch1.enabled = false;
            }
            if self.ch2.length.clock() {
                self.ch2.enabled = false;
            }
            if self.ch3.length.clock() {
                self.ch3.enabled = false;
            }
            if self.ch4.length.clock() {
                self.ch4.enabled = false;
            }
        }
        if step == 2 || step == 6 {
            self.sweep.clock(&mut self.ch1);
        }
        if step == 7 {
            self.ch1.env.clock();
            self.ch2.env.clock();
            self.ch4.env.clock();
        }
    }

    /// `t_cycles` T-사이클(1 M-사이클: 보통 4, CGB 2배속은 2) 진행하고 믹서에 출력 값을 하나 넘긴다.
    pub fn tick(&mut self, t_cycles: u32) {
        for _ in 0..t_cycles {
            self.ch3.tick();
            if self.power {
                self.ch1.tick();
                self.ch2.tick();
                self.ch4.tick();
            }
        }
        let (left, right) = self.output();
        self.mixer.push(left, right, t_cycles);
    }

    /// NR51 패닝과 NR50 마스터 볼륨을 적용한 왼쪽·오른쪽 값. 채널 4개가 모두 최대일 때 ±1.0 안에 든다.
    fn output(&self) -> (f32, f32) {
        let channels = [
            self.ch1.output(),
            self.ch2.output(),
            self.ch3.output(),
            self.ch4.output(),
        ];
        let (mut left, mut right) = (0.0, 0.0);
        for (i, digital) in channels.into_iter().enumerate() {
            let Some(digital) = digital else { continue };
            let analog = f32::from(digital) / 15.0;
            if self.nr51 & (0x10 << i) != 0 {
                left += analog;
            }
            if self.nr51 & (0x01 << i) != 0 {
                right += analog;
            }
        }
        let left_volume = f32::from((self.nr50 >> 4) & 7) + 1.0;
        let right_volume = f32::from(self.nr50 & 7) + 1.0;
        (left * left_volume / 32.0, right * right_volume / 32.0)
    }

    pub fn set_sample_rate(&mut self, rate: f64) {
        self.mixer.set_sample_rate(rate);
    }

    /// 스테이트에서 읽은 APU에 호스트 설정을 옮기고, 레지스터 비트 폭을 넘는 값을 감싼다.
    pub(crate) fn adopt_host(&mut self, host: &Apu) {
        self.mixer.adopt_host(&host.mixer);
        self.fs_step &= 7;
        self.sweep.sanitize();
        self.ch1.sanitize();
        self.ch2.sanitize();
        self.ch3.sanitize();
        self.ch4.sanitize();
    }

    /// 쌓인 인터리브 스테레오 샘플을 `out` 뒤에 붙인다.
    pub fn drain(&mut self, out: &mut Vec<f32>) {
        self.mixer.drain(out);
    }
}

/// PCM12/PCM34에 보이는 4비트 값. DAC가 꺼진 채널은 0이다.
fn digital(output: Option<u8>) -> u8 {
    output.unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_boot_registers() {
        let apu = Apu::new(false);
        assert_eq!(apu.read(NR52), 0xF1);
        assert_eq!(apu.read(0xFF11), 0xBF);
        assert_eq!(apu.read(0xFF12), 0xF3);
        assert_eq!(apu.read(0xFF24), 0x77);
        assert_eq!(apu.read(0xFF25), 0xF3);
    }

    #[test]
    fn write_only_bits_read_as_one() {
        let mut apu = Apu::new(false);
        apu.write(0xFF13, 0x12);
        apu.write(0xFF1C, 0x60);
        assert_eq!(apu.read(0xFF13), 0xFF);
        assert_eq!(apu.read(0xFF1C), 0xFF);
        assert_eq!(apu.read(0xFF27), 0xFF);
    }

    #[test]
    fn power_off_clears_registers_and_ignores_writes() {
        let mut apu = Apu::new(false);
        apu.write(0xFF30, 0x5A);
        apu.write(NR52, 0x00);
        assert_eq!(apu.read(NR52), 0x70);
        assert_eq!(apu.read(0xFF24), 0x00);
        apu.write(0xFF24, 0x77);
        assert_eq!(apu.read(0xFF24), 0x00, "꺼져 있으면 쓰기를 무시한다");
        assert_eq!(apu.read(0xFF30), 0x5A, "웨이브 RAM은 남는다");
    }

    #[test]
    fn length_can_be_written_while_powered_off() {
        let mut apu = Apu::new(false);
        apu.write(NR52, 0x00);
        apu.write(0xFF16, 0x3E); // 길이 2
        apu.write(NR52, 0x80);
        apu.write(0xFF17, 0xF0);
        apu.write(0xFF19, 0xC0); // 길이 켜고 트리거
        assert_eq!(apu.read(NR52) & 0x02, 0x02);
        apu.frame_sequencer();
        apu.frame_sequencer();
        apu.frame_sequencer();
        assert_eq!(apu.read(NR52) & 0x02, 0x00, "길이 2가 다 되면 꺼진다");
    }

    #[test]
    fn trigger_with_dac_off_does_not_enable_channel() {
        let mut apu = Apu::new(false);
        apu.write(0xFF21, 0x00);
        apu.write(0xFF23, 0x80);
        assert_eq!(apu.read(NR52) & 0x08, 0);
    }

    #[test]
    fn panning_routes_channel_to_one_side() {
        let mut apu = Apu::new(false);
        apu.write(0xFF25, 0x02); // 채널 2 → 오른쪽만
        apu.write(0xFF16, 0xC0); // 듀티 75%
        apu.write(0xFF17, 0xF0);
        apu.write(0xFF18, 0xFF);
        apu.write(0xFF19, 0x87); // 주기 값 2047: 듀티 단계당 4 T-사이클
        for _ in 0..100 {
            apu.tick(4);
        }
        let (left, right) = apu.output();
        assert_eq!(left, 0.0);
        assert!(right > 0.0);
    }

    #[test]
    fn square_wave_pitch_matches_frequency_register() {
        // 주기 값 1750: 131072 / (2048 - 1750) ≈ 439.8 Hz
        let mut apu = Apu::new(false);
        apu.write(0xFF25, 0x22);
        apu.write(0xFF16, 0x80); // 50%
        apu.write(0xFF17, 0xF0);
        apu.write(0xFF18, (1750 & 0xFF) as u8);
        apu.write(0xFF19, 0x80 | (1750 >> 8) as u8);
        let mut rising = 0;
        let mut last = 0.0;
        for _ in 0..1_048_576 {
            apu.tick(4);
            let (_, right) = apu.output();
            if right > 0.0 && last == 0.0 {
                rising += 1;
            }
            last = right;
        }
        assert!((439..=441).contains(&rising), "{rising}");
    }

    #[test]
    fn cgb_power_off_clears_lengths_and_ignores_length_writes() {
        let mut apu = Apu::new(true);
        apu.write(0xFF16, 0x3E); // 길이 2
        apu.write(NR52, 0x00);
        apu.write(0xFF16, 0x3F); // 꺼져 있으면 무시
        apu.write(NR52, 0x80);
        apu.write(0xFF17, 0xF0);
        apu.write(0xFF19, 0xC0); // 길이 0이라 트리거가 64로 채운다
        for _ in 0..2 * 63 - 1 {
            apu.frame_sequencer();
        }
        assert_eq!(
            apu.read(NR52) & 0x02,
            0x02,
            "64번째 길이 클록 전까지 켜져 있다"
        );
    }

    #[test]
    fn pcm_registers_show_digital_outputs() {
        let mut apu = Apu::new(true);
        apu.write(0xFF16, 0xC0); // 듀티 75%
        apu.write(0xFF17, 0xF0);
        apu.write(0xFF18, 0xFF);
        apu.write(0xFF19, 0x87);
        for _ in 0..100 {
            apu.tick(4);
        }
        assert_eq!(
            apu.read(PCM12),
            0xF0,
            "채널 2 볼륨 15, 채널 1은 부트 직후 볼륨 0"
        );
        assert_eq!(apu.read(PCM34), 0x00);
    }

    #[test]
    fn double_speed_ticks_keep_pitch() {
        // 2배속 M-사이클(2 T-사이클)로 1초를 돌려도 같은 높이(약 440 Hz)가 나온다.
        let mut apu = Apu::new(true);
        apu.write(0xFF25, 0x22);
        apu.write(0xFF16, 0x80);
        apu.write(0xFF17, 0xF0);
        apu.write(0xFF18, (1750 & 0xFF) as u8);
        apu.write(0xFF19, 0x80 | (1750 >> 8) as u8);
        let mut rising = 0;
        let mut last = 0.0;
        for _ in 0..2 * 1_048_576 {
            apu.tick(2);
            let (_, right) = apu.output();
            if right > 0.0 && last == 0.0 {
                rising += 1;
            }
            last = right;
        }
        assert!((439..=441).contains(&rising), "{rising}");
    }
}
