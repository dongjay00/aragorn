//! MBC3 실시간 시계 (Pan Docs "MBC3", 스펙 §4.7·§4.8).
//!
//! 에뮬레이션 시간(4194304 dot = 1초)으로 진행한다. 배속하면 게임 시간도 빨라진다.
//! 앱이 꺼져 있던 시간은 세이브 파일의 타임스탬프와 현재 시각의 차이만큼 불러올 때 진행한다.

/// 1초에 진행하는 dot 수 (CGB 2배속에서도 같다).
const DOTS_PER_SECOND: u32 = 4_194_304;
/// 세이브 파일 뒤에 붙는 RTC 블록 길이 (BGB/VBA-M 형식, 타임스탬프 u64).
pub const SAVE_BLOCK_LEN: usize = 48;
/// 타임스탬프가 u32인 옛 형식의 길이.
const SAVE_BLOCK_LEN_32: usize = 44;

/// DH 비트: 일 카운터 비트 8, 정지, 일 카운터 넘침.
const DH_DAY_HIGH: u8 = 0x01;
const DH_HALT: u8 = 0x40;
const DH_CARRY: u8 = 0x80;

/// RTC 레지스터 5개 (S, M, H, DL, DH).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RtcRegs {
    pub seconds: u8,
    pub minutes: u8,
    pub hours: u8,
    pub day_low: u8,
    pub day_high: u8,
}

impl RtcRegs {
    /// `select`(0x08–0x0C)가 가리키는 레지스터. 쓰지 않는 비트는 0으로 읽힌다.
    fn get(&self, select: u8) -> u8 {
        match select {
            0x08 => self.seconds & 0x3F,
            0x09 => self.minutes & 0x3F,
            0x0A => self.hours & 0x1F,
            0x0B => self.day_low,
            _ => self.day_high & (DH_DAY_HIGH | DH_HALT | DH_CARRY),
        }
    }

    fn day(&self) -> u16 {
        u16::from(self.day_high & DH_DAY_HIGH) << 8 | u16::from(self.day_low)
    }

    fn set_day(&mut self, day: u16) {
        self.day_low = day as u8;
        self.day_high = (self.day_high & !DH_DAY_HIGH) | ((day >> 8) as u8 & DH_DAY_HIGH);
    }

    /// 1초 진행한다. 범위를 벗어난 값(게임이 직접 쓴 60초 등)은 6비트에서 감싸고 올림하지 않는다.
    fn tick_second(&mut self) {
        self.seconds = (self.seconds + 1) & 0x3F;
        if self.seconds != 60 {
            return;
        }
        self.seconds = 0;
        self.minutes = (self.minutes + 1) & 0x3F;
        if self.minutes != 60 {
            return;
        }
        self.minutes = 0;
        self.hours = (self.hours + 1) & 0x1F;
        if self.hours != 24 {
            return;
        }
        self.hours = 0;
        self.advance_days(1);
    }

    /// 일 카운터를 `days`만큼 올린다. 511을 넘으면 감싸고 넘침 비트를 켠다.
    fn advance_days(&mut self, days: u64) {
        let total = u64::from(self.day()) + days;
        if total > 511 {
            self.day_high |= DH_CARRY;
        }
        self.set_day((total % 512) as u16);
    }

    /// `seconds`초 진행한다. 레지스터가 정상 범위면 한 번에 계산한다.
    fn advance(&mut self, seconds: u64) {
        if self.seconds >= 60 || self.minutes >= 60 || self.hours >= 24 {
            // 비정상 값은 1초씩 진행해 하드웨어와 같은 감싸기를 따르되, 한 바퀴(1일) 안에서 정상 범위로 돌아온다.
            let mut left = seconds;
            while left > 0 && (self.seconds >= 60 || self.minutes >= 60 || self.hours >= 24) {
                self.tick_second();
                left -= 1;
            }
            if left > 0 {
                self.advance(left);
            }
            return;
        }
        let in_day = u64::from(self.hours) * 3600
            + u64::from(self.minutes) * 60
            + u64::from(self.seconds)
            + seconds;
        self.seconds = (in_day % 60) as u8;
        self.minutes = (in_day / 60 % 60) as u8;
        self.hours = (in_day / 3600 % 24) as u8;
        let days = in_day / 86_400;
        if days > 0 {
            self.advance_days(days);
        }
    }

    fn to_words(self) -> [u32; 5] {
        [
            self.seconds,
            self.minutes,
            self.hours,
            self.day_low,
            self.day_high,
        ]
        .map(u32::from)
    }

    fn from_words(words: &[u8]) -> Self {
        let byte = |i: usize| words[i * 4];
        Self {
            seconds: byte(0),
            minutes: byte(1),
            hours: byte(2),
            day_low: byte(3),
            day_high: byte(4),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Rtc {
    regs: RtcRegs,
    latched: RtcRegs,
    /// 다음 초까지 쌓인 dot.
    sub_second: u32,
    /// 래치 레지스터에 마지막으로 쓴 값. 0 → 1이면 현재 값을 래치한다.
    latch_armed: bool,
}

impl Rtc {
    /// 래치된 레지스터를 읽는다 (`select` 0x08–0x0C).
    pub fn read(&self, select: u8) -> u8 {
        self.latched.get(select)
    }

    /// 현재 레지스터에 쓴다. 초를 쓰면 1초 미만 카운터도 지운다.
    pub fn write(&mut self, select: u8, value: u8) {
        let r = &mut self.regs;
        match select {
            0x08 => {
                r.seconds = value & 0x3F;
                self.sub_second = 0;
            }
            0x09 => r.minutes = value & 0x3F,
            0x0A => r.hours = value & 0x1F,
            0x0B => r.day_low = value,
            _ => r.day_high = value & (DH_DAY_HIGH | DH_HALT | DH_CARRY),
        }
    }

    /// 0x6000–0x7FFF 쓰기. 0을 쓴 뒤 1을 쓰면 현재 값을 래치 레지스터에 복사한다.
    pub fn write_latch(&mut self, value: u8) {
        if self.latch_armed && value == 0x01 {
            self.latched = self.regs;
        }
        self.latch_armed = value == 0x00;
    }

    fn halted(&self) -> bool {
        self.regs.day_high & DH_HALT != 0
    }

    /// `dots` dot 진행한다.
    pub fn tick(&mut self, dots: u32) {
        if self.halted() {
            return;
        }
        self.sub_second += dots;
        if self.sub_second >= DOTS_PER_SECOND {
            self.sub_second -= DOTS_PER_SECOND;
            self.regs.tick_second();
        }
    }

    /// 앱이 꺼져 있던 `seconds`초를 반영한다. 정지 중이면 진행하지 않는다.
    pub fn advance_offline(&mut self, seconds: u64) {
        if !self.halted() {
            self.regs.advance(seconds);
        }
    }

    /// 세이브 파일 뒤에 붙일 48바이트: 현재 레지스터 5개, 래치 레지스터 5개(각 u32 LE), 유닉스 시각(u64 LE).
    pub fn save_block(&self, now_unix: u64) -> [u8; SAVE_BLOCK_LEN] {
        let mut block = [0u8; SAVE_BLOCK_LEN];
        let words = self
            .regs
            .to_words()
            .into_iter()
            .chain(self.latched.to_words());
        for (chunk, word) in block.as_chunks_mut::<4>().0.iter_mut().zip(words) {
            chunk.copy_from_slice(&word.to_le_bytes());
        }
        block[40..48].copy_from_slice(&now_unix.to_le_bytes());
        block
    }

    /// 세이브 파일의 RTC 블록(48바이트, 또는 타임스탬프가 u32인 44바이트)을 싣고, 저장 시각부터 `now_unix`까지
    /// 지난 시간만큼 진행한다. 길이가 맞지 않으면 `false`를 반환하고 아무것도 바꾸지 않는다.
    pub fn load_block(&mut self, block: &[u8], now_unix: u64) -> bool {
        let saved_at = match block.len() {
            SAVE_BLOCK_LEN => u64::from_le_bytes(block[40..48].try_into().unwrap()),
            SAVE_BLOCK_LEN_32 => u64::from(u32::from_le_bytes(block[40..44].try_into().unwrap())),
            _ => return false,
        };
        self.regs = RtcRegs::from_words(&block[0..20]);
        self.latched = RtcRegs::from_words(&block[20..40]);
        self.sub_second = 0;
        // 시계가 뒤로 간 경우(저장 시각이 미래)는 진행하지 않는다.
        self.advance_offline(now_unix.saturating_sub(saved_at));
        true
    }

    #[cfg(test)]
    pub fn regs(&self) -> RtcRegs {
        self.regs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rtc_at(hours: u8, minutes: u8, seconds: u8, day: u16) -> Rtc {
        let mut rtc = Rtc::default();
        rtc.write(0x08, seconds);
        rtc.write(0x09, minutes);
        rtc.write(0x0A, hours);
        rtc.write(0x0B, day as u8);
        rtc.write(0x0C, (day >> 8) as u8);
        rtc
    }

    fn latched(rtc: &mut Rtc) -> [u8; 5] {
        rtc.write_latch(0);
        rtc.write_latch(1);
        [0x08, 0x09, 0x0A, 0x0B, 0x0C].map(|s| rtc.read(s))
    }

    #[test]
    fn one_second_takes_4194304_dots() {
        let mut rtc = Rtc::default();
        for _ in 0..DOTS_PER_SECOND / 4 - 1 {
            rtc.tick(4);
        }
        assert_eq!(rtc.regs().seconds, 0);
        rtc.tick(4);
        assert_eq!(rtc.regs().seconds, 1);
    }

    #[test]
    fn rollover_carries_through_minutes_hours_and_days() {
        let mut rtc = rtc_at(23, 59, 59, 0x1FF);
        rtc.tick(DOTS_PER_SECOND);
        assert_eq!(latched(&mut rtc), [0, 0, 0, 0x00, DH_CARRY]);
    }

    #[test]
    fn halt_stops_the_clock() {
        let mut rtc = rtc_at(1, 2, 3, 0);
        rtc.write(0x0C, DH_HALT);
        rtc.tick(DOTS_PER_SECOND);
        rtc.advance_offline(3600);
        assert_eq!(rtc.regs().seconds, 3);
        assert_eq!(rtc.regs().hours, 1);
    }

    #[test]
    fn reads_return_latched_values_until_next_latch() {
        let mut rtc = rtc_at(0, 0, 10, 0);
        assert_eq!(latched(&mut rtc)[0], 10);
        rtc.tick(DOTS_PER_SECOND);
        assert_eq!(rtc.read(0x08), 10, "래치하기 전에는 그대로");
        rtc.write_latch(1);
        assert_eq!(rtc.read(0x08), 10, "0 없이 1만 쓰면 래치하지 않는다");
        assert_eq!(latched(&mut rtc)[0], 11);
    }

    #[test]
    fn unused_bits_read_as_zero() {
        let mut rtc = Rtc::default();
        rtc.write(0x08, 0xFF);
        rtc.write(0x0A, 0xFF);
        rtc.write(0x0C, 0xFF);
        assert_eq!(latched(&mut rtc), [0x3F, 0x00, 0x1F, 0x00, 0xC1]);
    }

    #[test]
    fn writing_seconds_resets_sub_second_counter() {
        let mut rtc = Rtc::default();
        rtc.tick(DOTS_PER_SECOND - 4);
        rtc.write(0x08, 5);
        rtc.tick(4);
        assert_eq!(rtc.regs().seconds, 5);
    }

    #[test]
    fn out_of_range_seconds_wrap_without_carry() {
        let mut rtc = rtc_at(0, 0, 63, 0);
        rtc.tick(DOTS_PER_SECOND);
        assert_eq!((rtc.regs().seconds, rtc.regs().minutes), (0, 0));
    }

    #[test]
    fn offline_time_advances_by_whole_days() {
        let mut rtc = rtc_at(22, 30, 0, 100);
        rtc.advance_offline(3 * 86_400 + 2 * 3600 + 15);
        assert_eq!(latched(&mut rtc), [15, 30, 0, 104, 0]);
    }

    #[test]
    fn offline_time_past_day_511_sets_carry() {
        let mut rtc = rtc_at(0, 0, 0, 500);
        rtc.advance_offline(20 * 86_400);
        assert_eq!(latched(&mut rtc), [0, 0, 0, 8, DH_CARRY]);
    }

    #[test]
    fn save_block_round_trips_and_applies_elapsed_time() {
        let mut rtc = rtc_at(1, 2, 3, 0x123);
        latched(&mut rtc);
        let block = rtc.save_block(1_000);
        assert_eq!(block.len(), SAVE_BLOCK_LEN);
        assert_eq!(&block[0..4], &[3, 0, 0, 0], "초, u32 LE");
        assert_eq!(&block[16..20], &[0x01, 0, 0, 0], "DH");
        assert_eq!(&block[40..48], &1_000u64.to_le_bytes());

        let mut loaded = Rtc::default();
        assert!(loaded.load_block(&block, 1_000 + 60));
        assert_eq!(loaded.regs().minutes, 3, "앱이 꺼진 1분을 반영");
        assert_eq!(loaded.read(0x08), 3, "래치 레지스터도 복원");
    }

    #[test]
    fn legacy_44_byte_block_is_accepted() {
        let rtc = rtc_at(0, 0, 0, 0);
        let mut block = rtc.save_block(0)[..44].to_vec();
        block[40..44].copy_from_slice(&500u32.to_le_bytes());
        let mut loaded = Rtc::default();
        assert!(loaded.load_block(&block, 510));
        assert_eq!(loaded.regs().seconds, 10);
    }

    #[test]
    fn clock_going_backwards_does_not_advance() {
        let rtc = rtc_at(0, 0, 0, 0);
        let mut loaded = Rtc::default();
        assert!(loaded.load_block(&rtc.save_block(1_000), 10));
        assert_eq!(loaded.regs(), RtcRegs::default());
    }

    #[test]
    fn wrong_length_block_is_ignored() {
        let mut rtc = rtc_at(0, 0, 7, 0);
        assert!(!rtc.load_block(&[0; 10], 100));
        assert_eq!(rtc.regs().seconds, 7);
    }
}
