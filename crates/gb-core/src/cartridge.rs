//! 카트리지: 헤더 파싱과 ROM/RAM 매핑 (Pan Docs "The Cartridge Header").
//!
//! ROM-only, MBC1, MBC3, MBC5의 ROM/RAM 뱅크 전환을 지원한다. 배터리 세이브와 MBC3 RTC는 M4/M6에서 다룬다.

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
    /// `rom_bank`: 7비트(0은 1로 보정), `ram_select`: 0–3은 RAM 뱅크, 0x08–0x0C는 RTC 레지스터(M6).
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
        Ok(Cartridge {
            header,
            rom,
            ram,
            ram_enabled: false,
            mbc,
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
            // RTC 래치(0x6000–0x7FFF)는 M6에서 구현한다.
            (Mbc::Mbc3 { .. }, _) => {}
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

    /// 0xA000–0xBFFF. RAM이 꺼져 있거나 없으면 0xFF.
    pub fn read_ram(&self, addr: u16) -> u8 {
        self.ram_offset(addr).map_or(0xFF, |i| self.ram[i])
    }

    pub fn write_ram(&mut self, addr: u16, value: u8) {
        if let Some(i) = self.ram_offset(addr) {
            self.ram[i] = value;
        }
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
        assert_eq!(cart.read_ram(0xA123), 0xFF, "RTC 레지스터는 M6에서 구현");
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
}
