//! 카트리지: 헤더 파싱과 ROM/RAM 매핑 (Pan Docs "The Cartridge Header").
//!
//! M1은 뱅크 전환이 필요 없는 32KB ROM만 지원한다. MBC1/3/5는 M4에서 구현한다.

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

pub struct Cartridge {
    header: Header,
    rom: Vec<u8>,
}

impl Cartridge {
    pub fn new(rom: Vec<u8>) -> Result<Cartridge, CartError> {
        let header = Header::parse(&rom)?;
        // 32KB MBC1 ROM은 뱅크 1이 고정이라 ROM-only와 같게 동작한다.
        match header.cart_type {
            0x00..=0x03 if header.rom_size_code == 0 => Ok(Cartridge { header, rom }),
            other => Err(CartError::Unsupported(other)),
        }
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    /// 0x0000–0x7FFF. ROM 파일보다 뒤쪽은 0xFF(오픈 버스)다.
    pub fn read_rom(&self, addr: u16) -> u8 {
        self.rom.get(usize::from(addr)).copied().unwrap_or(0xFF)
    }

    /// MBC 레지스터 쓰기. M1의 카트리지에는 레지스터가 없다.
    pub fn write_rom(&mut self, _addr: u16, _value: u8) {}

    /// 0xA000–0xBFFF. M1의 카트리지에는 RAM이 없다.
    pub fn read_ram(&self, _addr: u16) -> u8 {
        0xFF
    }

    pub fn write_ram(&mut self, _addr: u16, _value: u8) {}
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

    #[test]
    fn accepts_32k_rom_only_and_mbc1() {
        for t in [0x00, 0x01, 0x02, 0x03] {
            assert!(Cartridge::new(test_rom(t, 0x00, 0x00)).is_ok(), "{t:#04X}");
        }
    }

    #[test]
    fn rejects_banked_or_unknown_cartridges_for_now() {
        assert_eq!(
            Cartridge::new(test_rom(0x01, 0x01, 0x00)).err(),
            Some(CartError::Unsupported(0x01))
        );
        assert_eq!(
            Cartridge::new(test_rom(0x13, 0x00, 0x00)).err(),
            Some(CartError::Unsupported(0x13))
        );
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
