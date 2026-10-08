//! 카트리지: 헤더 파싱과 ROM/RAM 매핑 (Pan Docs "The Cartridge Header").
//!
//! ROM-only, MBC1, MBC3(+RTC), MBC5의 ROM/RAM 뱅크 전환과 배터리 세이브를 지원한다.

use crate::rtc::{self, Rtc};
use std::fmt;

const HEADER_END: usize = 0x0150;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CartError {
    /// 헤더(0x0150바이트)보다 작은 ROM.
    TooSmall(usize),
    /// 아직 지원하지 않는 카트리지 타입 (0x0147).
    Unsupported(u8),
}

impl fmt::Display for CartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CartError::TooSmall(len) => write!(f, "ROM 파일이 너무 작습니다 ({len}바이트)"),
            CartError::Unsupported(t) => {
                write!(f, "지원하지 않는 카트리지 타입입니다: 0x{t:02X}")
            }
        }
    }
}

impl std::error::Error for CartError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub title: String,
    pub cgb_flag: u8,
    pub cart_type: u8,
    pub rom_size_code: u8,
    pub ram_size_code: u8,
    pub header_checksum_valid: bool,
}

impl Header {
    pub fn parse(rom: &[u8]) -> Result<Header, CartError> {
        if rom.len() < HEADER_END {
            return Err(CartError::TooSmall(rom.len()));
        }
        let title = rom[0x0134..0x0143]
            .iter()
            .take_while(|&&b| b != 0)
            .map(|&b| char::from(b))
            .collect();
        Ok(Header {
            title,
            cgb_flag: rom[0x0143],
            cart_type: rom[0x0147],
            rom_size_code: rom[0x0148],
            ram_size_code: rom[0x0149],
            header_checksum_valid: header_checksum(rom) == rom[0x014D],
        })
    }
}

/// 메모리 뱅크 컨트롤러 (Pan Docs "MBCs"). 직렬화를 위해 trait 객체 대신 enum으로 둔다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mbc {
    /// ROM-only (+RAM). 뱅크 전환 없음.
    None,
    /// `bank1`: 5비트 ROM 뱅크(0은 1로 보정), `bank2`: 2비트(ROM 상위 비트 또는 RAM 뱅크), `mode`: 뱅킹 모드.
    Mbc1 { bank1: u8, bank2: u8, mode: bool },
    /// `rom_bank`: 7비트(0은 1로 보정), `ram_select`: 0–3은 RAM 뱅크, 0x08–0x0C는 RTC 레지스터.
    Mbc3 { rom_bank: u8, ram_select: u8 },
    /// `rom_bank`: 9비트(0도 그대로), `ram_bank`: 4비트.
    Mbc5 { rom_bank: u16, ram_bank: u8 },
}

pub struct Cartridge {
    header: Header,
    rom: Vec<u8>,
    ram: Vec<u8>,
    ram_enabled: bool,
    mbc: Mbc,
    /// 배터리가 있어 외부 RAM을 세이브 파일로 남겨야 하는 카트리지인지.
    battery: bool,
    /// 마지막으로 확인한 뒤 외부 RAM(또는 RTC)에 쓰기가 있었는지.
    ram_dirty: bool,
    /// MBC3+TIMER 카트리지(0x0F, 0x10)의 실시간 시계.
    rtc: Option<Rtc>,
}

/// 헤더의 RAM 크기 코드(0x0149)를 바이트 수로 바꾼다.
fn ram_size(code: u8) -> usize {
    match code {
        0x02 => 0x2000,
        0x03 => 0x8000,
        0x04 => 0x20000,
        0x05 => 0x10000,
        _ => 0,
    }
}

impl Cartridge {
    pub fn new(rom: Vec<u8>) -> Result<Cartridge, CartError> {
        let header = Header::parse(&rom)?;
        let mbc = match header.cart_type {
            0x00 | 0x08 | 0x09 => Mbc::None,
            0x01..=0x03 => Mbc::Mbc1 {
                bank1: 1,
                bank2: 0,
                mode: false,
            },
            0x0F..=0x13 => Mbc::Mbc3 {
                rom_bank: 1,
                ram_select: 0,
            },
            0x19..=0x1E => Mbc::Mbc5 {
                rom_bank: 1,
                ram_bank: 0,
            },
            other => return Err(CartError::Unsupported(other)),
        };
        let ram = vec![0; ram_size(header.ram_size_code)];
        let battery = matches!(
            header.cart_type,
            0x03 | 0x09 | 0x0F | 0x10 | 0x13 | 0x1B | 0x1E
        );
        let rtc = matches!(header.cart_type, 0x0F | 0x10).then(Rtc::default);
        Ok(Cartridge {
            header,
            rom,
            ram,
            ram_enabled: false,
            mbc,
            battery,
            ram_dirty: false,
            rtc,
        })
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    fn rom_byte(&self, bank: usize, addr: u16) -> u8 {
        let banks = (self.rom.len() / 0x4000).max(1);
        let offset = (bank % banks) * 0x4000 + usize::from(addr & 0x3FFF);
        self.rom.get(offset).copied().unwrap_or(0xFF)
    }

    /// 0x0000–0x7FFF. ROM 파일보다 뒤쪽은 0xFF(오픈 버스)다.
    pub fn read_rom(&self, addr: u16) -> u8 {
        let bank = match (self.mbc, addr) {
            (Mbc::None, _) => return self.rom.get(usize::from(addr)).copied().unwrap_or(0xFF),
            (Mbc::Mbc1 { bank2, mode, .. }, 0x0000..=0x3FFF) => {
                if mode {
                    usize::from(bank2) << 5
                } else {
                    0
                }
            }
            (_, 0x0000..=0x3FFF) => 0,
            (Mbc::Mbc1 { bank1, bank2, .. }, _) => (usize::from(bank2) << 5) | usize::from(bank1),
            (Mbc::Mbc3 { rom_bank, .. }, _) => usize::from(rom_bank),
            (Mbc::Mbc5 { rom_bank, .. }, _) => usize::from(rom_bank),
        };
        self.rom_byte(bank, addr)
    }

    /// MBC 레지스터 쓰기 (0x0000–0x7FFF).
    pub fn write_rom(&mut self, addr: u16, value: u8) {
        match (&mut self.mbc, addr) {
            (Mbc::None, _) => {}
            (_, 0x0000..=0x1FFF) => self.ram_enabled = value & 0x0F == 0x0A,
            (Mbc::Mbc1 { bank1, .. }, 0x2000..=0x3FFF) => *bank1 = (value & 0x1F).max(1),
            (Mbc::Mbc1 { bank2, .. }, 0x4000..=0x5FFF) => *bank2 = value & 0x03,
            (Mbc::Mbc1 { mode, .. }, _) => *mode = value & 0x01 != 0,
            (Mbc::Mbc3 { rom_bank, .. }, 0x2000..=0x3FFF) => *rom_bank = (value & 0x7F).max(1),
            (Mbc::Mbc3 { ram_select, .. }, 0x4000..=0x5FFF) => *ram_select = value & 0x0F,
            (Mbc::Mbc3 { .. }, _) => {
                if let Some(rtc) = &mut self.rtc {
                    rtc.write_latch(value);
                }
            }
            (Mbc::Mbc5 { rom_bank, .. }, 0x2000..=0x2FFF) => {
                *rom_bank = (*rom_bank & 0x100) | u16::from(value);
            }
            (Mbc::Mbc5 { rom_bank, .. }, 0x3000..=0x3FFF) => {
                *rom_bank = (*rom_bank & 0xFF) | (u16::from(value & 0x01) << 8);
            }
            (Mbc::Mbc5 { ram_bank, .. }, 0x4000..=0x5FFF) => *ram_bank = value & 0x0F,
            (Mbc::Mbc5 { .. }, _) => {}
        }
    }

    /// 외부 RAM 안의 바이트 위치. RAM이 꺼져 있거나 없거나 RTC가 선택되었으면 `None`.
    fn ram_offset(&self, addr: u16) -> Option<usize> {
        if self.ram.is_empty() || (!self.ram_enabled && self.mbc != Mbc::None) {
            return None;
        }
        let bank = match self.mbc {
            Mbc::None => 0,
            Mbc::Mbc1 { bank2, mode, .. } => {
                if mode {
                    usize::from(bank2)
                } else {
                    0
                }
            }
            Mbc::Mbc3 { ram_select, .. } if ram_select <= 0x03 => usize::from(ram_select),
            Mbc::Mbc3 { .. } => return None,
            Mbc::Mbc5 { ram_bank, .. } => usize::from(ram_bank),
        };
        Some((bank * 0x2000 + usize::from(addr & 0x1FFF)) % self.ram.len())
    }

    /// RAM이 켜져 있고 MBC3가 RTC 레지스터(0x08–0x0C)를 고르고 있으면 그 번호.
    fn rtc_select(&self) -> Option<u8> {
        match self.mbc {
            Mbc::Mbc3 { ram_select, .. }
                if self.ram_enabled
                    && self.rtc.is_some()
                    && (0x08..=0x0C).contains(&ram_select) =>
            {
                Some(ram_select)
            }
            _ => None,
        }
    }

    /// 0xA000–0xBFFF. RAM이 꺼져 있거나 없으면 0xFF.
    pub fn read_ram(&self, addr: u16) -> u8 {
        if let (Some(select), Some(rtc)) = (self.rtc_select(), &self.rtc) {
            return rtc.read(select);
        }
        self.ram_offset(addr).map_or(0xFF, |i| self.ram[i])
    }

    pub fn write_ram(&mut self, addr: u16, value: u8) {
        if let Some(select) = self.rtc_select() {
            if let Some(rtc) = &mut self.rtc {
                rtc.write(select, value);
                self.ram_dirty = true;
            }
            return;
        }
        if let Some(i) = self.ram_offset(addr) {
            self.ram[i] = value;
            self.ram_dirty = true;
        }
    }

    /// RTC를 `dots` dot 진행한다 (RTC가 없으면 아무것도 하지 않는다).
    pub fn tick(&mut self, dots: u32) {
        if let Some(rtc) = &mut self.rtc {
            rtc.tick(dots);
        }
    }

    /// 외부 RAM 크기(바이트). 세이브 파일의 RAM 부분 길이다.
    pub fn ram_len(&self) -> usize {
        self.ram.len()
    }

    /// 세이브 파일 내용: 외부 RAM, 그리고 RTC가 있으면 48바이트 RTC 블록(`now_unix`는 저장 시각).
    /// 배터리가 없거나 남길 것이 없는 카트리지는 `None`.
    pub fn battery_ram(&self, now_unix: u64) -> Option<Vec<u8>> {
        if !self.battery || (self.ram.is_empty() && self.rtc.is_none()) {
            return None;
        }
        let mut data = self.ram.clone();
        if let Some(rtc) = &self.rtc {
            data.extend_from_slice(&rtc.save_block(now_unix));
        }
        Some(data)
    }

    /// 세이브 파일 내용을 싣는다. RAM 부분은 겹치는 만큼 복사하고, RTC 카트리지는 뒤따르는 RTC 블록을
    /// 읽어 저장 시각부터 `now_unix`까지 지난 시간을 반영한다. RTC 블록이 없거나 깨졌으면 시계는 0부터 간다.
    pub fn load_battery_ram(&mut self, data: &[u8], now_unix: u64) {
        let len = data.len().min(self.ram.len());
        self.ram[..len].copy_from_slice(&data[..len]);
        if let Some(rtc) = &mut self.rtc
            && let Some(block) = data.get(self.ram.len()..)
        {
            rtc.load_block(&block[..block.len().min(rtc::SAVE_BLOCK_LEN)], now_unix);
        }
    }

    /// 마지막 호출 뒤 외부 RAM에 쓰기가 있었으면 `true`. 읽으면 초기화된다.
    pub fn take_ram_dirty(&mut self) -> bool {
        std::mem::take(&mut self.ram_dirty)
    }

    /// 게임이 외부 RAM을 켜 두었는지. RAM 활성화 레지스터가 없는 ROM+RAM 카트리지는 항상 `true`.
    pub fn ram_enabled(&self) -> bool {
        self.ram_enabled || self.mbc == Mbc::None
    }
}

fn header_checksum(rom: &[u8]) -> u8 {
    rom[0x0134..=0x014C]
        .iter()
        .fold(0u8, |x, &b| x.wrapping_sub(b).wrapping_sub(1))
}

/// 제목 "TEST", 헤더 체크섬이 맞는 32KB ROM.
#[cfg(test)]
pub(crate) fn test_rom(cart_type: u8, rom_size_code: u8, cgb_flag: u8) -> Vec<u8> {
    let mut rom = vec![0; 0x8000];
    rom[0x0134..0x0138].copy_from_slice(b"TEST");
    rom[0x0143] = cgb_flag;
    rom[0x0147] = cart_type;
    rom[0x0148] = rom_size_code;
    rom[0x014D] = header_checksum(&rom);
    rom
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_header_fields() {
        let h = Header::parse(&test_rom(0x01, 0x00, 0x80)).unwrap();
        assert_eq!(h.title, "TEST");
        assert_eq!(
            (h.cgb_flag, h.cart_type, h.rom_size_code, h.ram_size_code),
            (0x80, 0x01, 0x00, 0x00)
        );
        assert!(h.header_checksum_valid);
    }

    #[test]
    fn detects_bad_header_checksum() {
        let mut rom = test_rom(0x00, 0x00, 0x00);
        rom[0x014D] ^= 0xFF;
        assert!(!Header::parse(&rom).unwrap().header_checksum_valid);
    }

    #[test]
    fn rejects_rom_smaller_than_header() {
        assert_eq!(Header::parse(&[0; 0x100]), Err(CartError::TooSmall(0x100)));
    }

    /// 각 뱅크 첫 두 바이트에 (뱅크 번호 하위, 상위)를 적은 ROM. 0x4000 읽기로 매핑된 뱅크를 알 수 있다.
    fn banked_rom(cart_type: u8, rom_size_code: u8, ram_size_code: u8) -> Vec<u8> {
        let banks = 2usize << rom_size_code;
        let mut rom = vec![0; banks * 0x4000];
        for bank in 1..banks {
            rom[bank * 0x4000] = bank as u8;
            rom[bank * 0x4000 + 1] = (bank >> 8) as u8;
        }
        rom[0x0134..0x0138].copy_from_slice(b"TEST");
        rom[0x0147] = cart_type;
        rom[0x0148] = rom_size_code;
        rom[0x0149] = ram_size_code;
        rom[0x014D] = header_checksum(&rom);
        rom
    }

    fn mapped_bank(cart: &Cartridge, base: u16) -> u16 {
        u16::from_le_bytes([cart.read_rom(base), cart.read_rom(base + 1)])
    }

    #[test]
    fn accepts_rom_only_mbc1_mbc3_and_mbc5_types() {
        let types = [
            0x00, 0x01, 0x02, 0x03, 0x08, 0x09, 0x0F, 0x10, 0x11, 0x12, 0x13, 0x19, 0x1A, 0x1B,
            0x1C, 0x1D, 0x1E,
        ];
        for t in types {
            assert!(
                Cartridge::new(banked_rom(t, 0x01, 0x00)).is_ok(),
                "{t:#04X}"
            );
        }
    }

    #[test]
    fn rejects_unsupported_cartridge_types() {
        for t in [0x05, 0x06, 0x0B, 0x20, 0xFC, 0xFF] {
            assert_eq!(
                Cartridge::new(banked_rom(t, 0x01, 0x00)).err(),
                Some(CartError::Unsupported(t))
            );
        }
    }

    #[test]
    fn mbc1_switches_rom_bank_and_maps_zero_to_one() {
        let mut cart = Cartridge::new(banked_rom(0x01, 0x05, 0x00)).unwrap();
        assert_eq!(mapped_bank(&cart, 0x4000), 1);
        cart.write_rom(0x2000, 0x05);
        assert_eq!(mapped_bank(&cart, 0x4000), 5);
        cart.write_rom(0x2000, 0x00);
        assert_eq!(mapped_bank(&cart, 0x4000), 1);
        cart.write_rom(0x3FFF, 0x21);
        assert_eq!(mapped_bank(&cart, 0x4000), 1);
    }

    #[test]
    fn mbc1_upper_bits_select_large_banks_and_mode_maps_bank0_area() {
        let mut cart = Cartridge::new(banked_rom(0x01, 0x06, 0x00)).unwrap();
        cart.write_rom(0x2000, 0x02);
        cart.write_rom(0x4000, 0x01);
        assert_eq!(mapped_bank(&cart, 0x4000), 0x22);
        assert_eq!(mapped_bank(&cart, 0x0000), 0);
        cart.write_rom(0x6000, 0x01);
        assert_eq!(mapped_bank(&cart, 0x0000), 0x20);
    }

    #[test]
    fn rom_bank_wraps_to_rom_size() {
        let mut cart = Cartridge::new(banked_rom(0x01, 0x03, 0x00)).unwrap();
        cart.write_rom(0x2000, 0x13);
        assert_eq!(mapped_bank(&cart, 0x4000), 0x13 % 16);
    }

    #[test]
    fn mbc1_ram_needs_enable_and_banks_only_in_mode1() {
        let mut cart = Cartridge::new(banked_rom(0x03, 0x01, 0x03)).unwrap();
        cart.write_ram(0xA000, 0x11);
        assert_eq!(cart.read_ram(0xA000), 0xFF, "꺼진 RAM은 0xFF");
        cart.write_rom(0x0000, 0x0A);
        cart.write_ram(0xA000, 0x11);
        cart.write_rom(0x4000, 0x01);
        assert_eq!(cart.read_ram(0xA000), 0x11, "모드 0에서는 RAM 뱅크 0 고정");
        cart.write_rom(0x6000, 0x01);
        assert_eq!(cart.read_ram(0xA000), 0x00);
        cart.write_ram(0xA000, 0x22);
        cart.write_rom(0x6000, 0x00);
        assert_eq!(cart.read_ram(0xA000), 0x11);
        cart.write_rom(0x0000, 0x00);
        assert_eq!(cart.read_ram(0xA000), 0xFF);
    }

    #[test]
    fn mbc3_selects_7bit_rom_bank_ram_banks_and_hides_rtc() {
        let mut cart = Cartridge::new(banked_rom(0x13, 0x06, 0x03)).unwrap();
        cart.write_rom(0x2000, 0x45);
        assert_eq!(mapped_bank(&cart, 0x4000), 0x45);
        cart.write_rom(0x2000, 0x00);
        assert_eq!(mapped_bank(&cart, 0x4000), 1);
        cart.write_rom(0x0000, 0x0A);
        cart.write_rom(0x4000, 0x02);
        cart.write_ram(0xA123, 0x33);
        cart.write_rom(0x4000, 0x00);
        assert_eq!(cart.read_ram(0xA123), 0x00);
        cart.write_rom(0x4000, 0x02);
        assert_eq!(cart.read_ram(0xA123), 0x33);
        cart.write_rom(0x4000, 0x08);
        assert_eq!(cart.read_ram(0xA123), 0xFF, "RTC가 없는 MBC3(0x13)");
    }

    #[test]
    fn mbc5_uses_9bit_rom_bank_including_zero() {
        let mut cart = Cartridge::new(banked_rom(0x19, 0x08, 0x00)).unwrap();
        cart.write_rom(0x2000, 0x00);
        assert_eq!(mapped_bank(&cart, 0x4000), 0);
        cart.write_rom(0x3000, 0x01);
        cart.write_rom(0x2000, 0x05);
        assert_eq!(mapped_bank(&cart, 0x4000), 0x105);
    }

    #[test]
    fn mbc5_banks_ram() {
        let mut cart = Cartridge::new(banked_rom(0x1B, 0x01, 0x04)).unwrap();
        cart.write_rom(0x0000, 0x0A);
        cart.write_rom(0x4000, 0x0F);
        cart.write_ram(0xBFFF, 0x44);
        cart.write_rom(0x4000, 0x00);
        assert_eq!(cart.read_ram(0xBFFF), 0x00);
        cart.write_rom(0x4000, 0x0F);
        assert_eq!(cart.read_ram(0xBFFF), 0x44);
    }

    #[test]
    fn rom_only_ram_is_always_enabled() {
        let mut cart = Cartridge::new(banked_rom(0x08, 0x00, 0x02)).unwrap();
        cart.write_ram(0xA000, 0x55);
        assert_eq!(cart.read_ram(0xA000), 0x55);
    }

    #[test]
    fn short_rom_reads_open_bus() {
        let mut rom = test_rom(0x00, 0x00, 0x00);
        rom.truncate(0x4000);
        let cart = Cartridge::new(rom).unwrap();
        assert_eq!(cart.read_rom(0x0134), b'T');
        assert_eq!(cart.read_rom(0x4000), 0xFF);
        assert_eq!(cart.read_rom(0x7FFF), 0xFF);
    }

    #[test]
    fn rom_writes_are_ignored_and_ram_is_absent() {
        let mut cart = Cartridge::new(test_rom(0x01, 0x00, 0x00)).unwrap();
        cart.write_rom(0x0134, 0x00);
        cart.write_ram(0xA000, 0x12);
        assert_eq!(cart.read_rom(0x0134), b'T');
        assert_eq!(cart.read_ram(0xA000), 0xFF);
    }

    #[test]
    fn error_messages_are_korean() {
        assert_eq!(
            CartError::Unsupported(0x13).to_string(),
            "지원하지 않는 카트리지 타입입니다: 0x13"
        );
        assert_eq!(
            CartError::TooSmall(16).to_string(),
            "ROM 파일이 너무 작습니다 (16바이트)"
        );
    }

    #[test]
    fn only_battery_types_expose_battery_ram() {
        let saved = |cart_type, ram_code| {
            Cartridge::new(banked_rom(cart_type, 0x01, ram_code))
                .unwrap()
                .battery_ram(0)
                .map(|data| data.len())
        };
        assert_eq!(saved(0x03, 0x02), Some(0x2000));
        assert_eq!(saved(0x13, 0x03), Some(0x8000));
        assert_eq!(saved(0x1B, 0x03), Some(0x8000));
        assert_eq!(saved(0x02, 0x02), None, "배터리 없음");
        assert_eq!(
            saved(0x0F, 0x00),
            Some(48),
            "MBC3+TIMER+BATTERY: RAM 없이 RTC 블록만"
        );
        assert_eq!(saved(0x10, 0x03), Some(0x8000 + 48));
    }

    #[test]
    fn ram_writes_set_dirty_flag_until_taken() {
        let mut cart = Cartridge::new(banked_rom(0x03, 0x01, 0x02)).unwrap();
        cart.write_ram(0xA000, 0x01);
        assert!(!cart.take_ram_dirty(), "꺼진 RAM에 쓴 것은 무시된다");
        cart.write_rom(0x0000, 0x0A);
        cart.write_ram(0xA000, 0x01);
        assert!(cart.take_ram_dirty());
        assert!(!cart.take_ram_dirty());
    }

    #[test]
    fn loading_battery_ram_copies_overlapping_bytes() {
        let mut cart = Cartridge::new(banked_rom(0x03, 0x01, 0x02)).unwrap();
        let mut save = vec![0x5A; 0x2000 + 48];
        save[0x1FFF] = 0x77;
        cart.load_battery_ram(&save, 0);
        assert_eq!(cart.battery_ram(0).unwrap().len(), 0x2000);
        assert_eq!(cart.battery_ram(0).unwrap()[0x1FFF], 0x77);
        cart.load_battery_ram(&[1, 2], 0);
        assert_eq!(&cart.battery_ram(0).unwrap()[..3], &[1, 2, 0x5A]);
    }

    /// MBC3+TIMER+RAM+BATTERY(0x10), RAM 32KB, RAM 켜짐.
    fn rtc_cart() -> Cartridge {
        let mut cart = Cartridge::new(banked_rom(0x10, 0x06, 0x03)).unwrap();
        cart.write_rom(0x0000, 0x0A);
        cart
    }

    fn latch(cart: &mut Cartridge) {
        cart.write_rom(0x6000, 0x00);
        cart.write_rom(0x6000, 0x01);
    }

    /// 1초(4194304 dot) 진행한다.
    fn tick_second(cart: &mut Cartridge) {
        for _ in 0..4_194_304 / 4 {
            cart.tick(4);
        }
    }

    #[test]
    fn mbc3_timer_maps_rtc_registers_through_latch() {
        let mut cart = rtc_cart();
        cart.write_rom(0x4000, 0x08);
        cart.write_ram(0xA000, 30);
        assert!(cart.take_ram_dirty(), "시계 설정도 저장 대상이다");
        latch(&mut cart);
        assert_eq!(cart.read_ram(0xA000), 30);
        tick_second(&mut cart);
        assert_eq!(cart.read_ram(0xA000), 30, "다시 래치하기 전에는 그대로");
        latch(&mut cart);
        assert_eq!(
            cart.read_ram(0xBFFF),
            31,
            "0xA000–0xBFFF 어디서나 같은 레지스터"
        );
        cart.write_rom(0x4000, 0x00);
        cart.write_ram(0xA000, 0x44);
        assert_eq!(cart.read_ram(0xA000), 0x44, "RAM 뱅크는 그대로 쓴다");
        cart.write_rom(0x0000, 0x00);
        cart.write_rom(0x4000, 0x08);
        assert_eq!(
            cart.read_ram(0xA000),
            0xFF,
            "RAM이 꺼져 있으면 RTC도 막힌다"
        );
    }

    #[test]
    fn rtc_block_follows_ram_and_restores_with_elapsed_time() {
        let mut cart = rtc_cart();
        cart.write_ram(0xA000, 0x42);
        cart.write_rom(0x4000, 0x0A);
        cart.write_ram(0xA000, 5); // 5시
        let save = cart.battery_ram(1_000).unwrap();
        assert_eq!(save.len(), 0x8000 + 48);

        let mut loaded = rtc_cart();
        loaded.load_battery_ram(&save, 1_000 + 2 * 3600);
        loaded.write_rom(0x4000, 0x0A);
        latch(&mut loaded);
        assert_eq!(loaded.read_ram(0xA000), 7, "꺼져 있던 2시간을 반영");
        loaded.write_rom(0x4000, 0x00);
        assert_eq!(loaded.read_ram(0xA000), 0x42);
    }

    #[test]
    fn save_without_rtc_block_starts_clock_from_zero() {
        let mut cart = rtc_cart();
        cart.load_battery_ram(&vec![0x11; 0x8000], 1_000_000);
        cart.write_rom(0x4000, 0x0A);
        latch(&mut cart);
        assert_eq!(cart.read_ram(0xA000), 0);
    }

    #[test]
    fn rom_only_ram_counts_as_enabled() {
        assert!(
            Cartridge::new(banked_rom(0x09, 0x00, 0x02))
                .unwrap()
                .ram_enabled()
        );
        assert!(
            !Cartridge::new(banked_rom(0x03, 0x01, 0x02))
                .unwrap()
                .ram_enabled()
        );
    }
}
