//! 치트 코드 (게임 지니, 게임샤크).
//!
//! - 게임 지니는 카트리지 ROM 읽기를 바꾼다. 비교 값이 있으면 원래 값이 그 값일 때만 바꾼다
//!   (뱅크가 바뀌는 영역에서 다른 뱅크를 건드리지 않게).
//! - 게임샤크는 프레임마다(VBlank 진입 때) 메모리에 값을 쓴다.
//!
//! 치트는 스테이트에 넣지 않는다. 스테이트를 불러와도 지금 켠 치트가 그대로 적용된다.

use std::fmt;

/// 해석한 치트 코드 하나.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cheat {
    /// ROM `address`(0x0000–0x7FFF)를 읽으면 `value`를 돌려준다.
    GameGenie {
        address: u16,
        value: u8,
        compare: Option<u8>,
    },
    /// 프레임마다 `address`에 `value`를 쓴다. `bank`는 코드의 첫 바이트다
    /// (0x01이면 지금 뱅크, 0x80–0x87이면 CGB WRAM 뱅크 0–7).
    GameShark { bank: u8, address: u16, value: u8 },
}

/// 코드를 해석할 수 없는 이유.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheatError {
    /// 게임 지니(`ABC-DEF`, `ABC-DEF-GHI`)나 게임샤크(`01VVAAAA`) 형식이 아니다.
    Format,
    /// 게임 지니 주소가 ROM 영역(0x0000–0x7FFF) 밖이다.
    NotRom(u16),
}

impl fmt::Display for CheatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CheatError::Format => write!(
                f,
                "게임 지니(ABC-DEF-GHI)나 게임샤크(01VVAAAA) 코드가 아닙니다"
            ),
            CheatError::NotRom(address) => {
                write!(f, "게임 지니 주소 ${address:04X}가 ROM 영역 밖입니다")
            }
        }
    }
}

impl std::error::Error for CheatError {}

impl Cheat {
    /// 코드를 해석한다. 대소문자, 공백, `-`는 가리지 않는다.
    /// - 게임 지니: 16진수 6자리 또는 9자리 (`00A-17B-C49`)
    /// - 게임샤크: 16진수 8자리 (`010138CD`)
    pub fn parse(code: &str) -> Result<Cheat, CheatError> {
        let digits: Vec<u8> = code
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '-')
            .map(|c| c.to_digit(16).map(|d| d as u8).ok_or(CheatError::Format))
            .collect::<Result<_, _>>()?;
        match digits.len() {
            6 | 9 => Self::game_genie(&digits),
            8 => Ok(Self::game_shark(&digits)),
            _ => Err(CheatError::Format),
        }
    }

    /// `ABC-DEF-GHI`: AB = 새 값, 주소 = (F xor 0xF) C D E, 비교 값 = GI를 오른쪽으로 2비트 돌리고
    /// 0xBA와 xor. H는 쓰지 않는다.
    fn game_genie(d: &[u8]) -> Result<Cheat, CheatError> {
        let value = d[0] << 4 | d[1];
        let address = u16::from(d[5] ^ 0xF) << 12
            | u16::from(d[2]) << 8
            | u16::from(d[3]) << 4
            | u16::from(d[4]);
        if address >= 0x8000 {
            return Err(CheatError::NotRom(address));
        }
        let compare = (d.len() == 9).then(|| (d[6] << 4 | d[8]).rotate_right(2) ^ 0xBA);
        Ok(Cheat::GameGenie {
            address,
            value,
            compare,
        })
    }

    /// `TTVVLLHH`: TT = 뱅크, VV = 값, 주소 = HHLL.
    fn game_shark(d: &[u8]) -> Cheat {
        let byte = |i: usize| d[i] << 4 | d[i + 1];
        Cheat::GameShark {
            bank: byte(0),
            value: byte(2),
            address: u16::from(byte(6)) << 8 | u16::from(byte(4)),
        }
    }
}

/// 켜 둔 치트. 버스가 ROM 읽기와 프레임마다 쓰기에 쓴다.
#[derive(Debug, Clone, Default)]
pub(crate) struct Cheats {
    genie: Vec<(u16, u8, Option<u8>)>,
    shark: Vec<(u8, u16, u8)>,
}

impl Cheats {
    pub(crate) fn new(cheats: &[Cheat]) -> Self {
        let mut out = Self::default();
        for &cheat in cheats {
            match cheat {
                Cheat::GameGenie {
                    address,
                    value,
                    compare,
                } => out.genie.push((address, value, compare)),
                Cheat::GameShark {
                    bank,
                    address,
                    value,
                } => out.shark.push((bank, address, value)),
            }
        }
        out
    }

    /// ROM에서 읽은 `original`을 게임 지니 코드대로 바꾼다.
    pub(crate) fn patch_rom(&self, address: u16, original: u8) -> u8 {
        for &(at, value, compare) in &self.genie {
            if at == address && compare.is_none_or(|c| c == original) {
                return value;
            }
        }
        original
    }

    pub(crate) fn has_genie(&self) -> bool {
        !self.genie.is_empty()
    }

    pub(crate) fn writes(&self) -> &[(u8, u16, u8)] {
        &self.shark
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_genie_six_digits_has_no_compare() {
        // AB=00, 주소 = (F^F)CDE → C=A, D=1, E=7, F=B → 0x4A17
        assert_eq!(
            Cheat::parse("00A-17B").unwrap(),
            Cheat::GameGenie {
                address: 0x4A17,
                value: 0x00,
                compare: None
            }
        );
    }

    #[test]
    fn game_genie_nine_digits_decodes_compare() {
        // GI = 0xC9 → ror 2 = 0x72 → xor 0xBA = 0xC8
        assert_eq!(
            Cheat::parse("3ea-17b-c49").unwrap(),
            Cheat::GameGenie {
                address: 0x4A17,
                value: 0x3E,
                compare: Some(0xC8)
            }
        );
    }

    #[test]
    fn game_genie_outside_rom_is_rejected() {
        // F = 0 → 주소 최상위 니블 0xF
        assert_eq!(Cheat::parse("00A-170"), Err(CheatError::NotRom(0xFA17)));
    }

    #[test]
    fn game_shark_swaps_address_bytes() {
        assert_eq!(
            Cheat::parse(" 0163 38cd ").unwrap(),
            Cheat::GameShark {
                bank: 0x01,
                value: 0x63,
                address: 0xCD38
            }
        );
    }

    #[test]
    fn other_shapes_are_rejected() {
        for code in ["", "123", "0101C0D", "XYZ-123", "0101C0D0F0"] {
            assert_eq!(Cheat::parse(code), Err(CheatError::Format), "{code}");
        }
    }

    #[test]
    fn compare_limits_the_patch() {
        let cheats = Cheats::new(&[
            Cheat::GameGenie {
                address: 0x4000,
                value: 0x99,
                compare: Some(0x11),
            },
            Cheat::GameGenie {
                address: 0x0150,
                value: 0x77,
                compare: None,
            },
        ]);
        assert_eq!(cheats.patch_rom(0x4000, 0x11), 0x99);
        assert_eq!(cheats.patch_rom(0x4000, 0x22), 0x22, "다른 뱅크");
        assert_eq!(cheats.patch_rom(0x0150, 0x00), 0x77);
        assert_eq!(cheats.patch_rom(0x0151, 0x00), 0x00);
    }
}
