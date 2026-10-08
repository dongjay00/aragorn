//! SM83 레지스터 파일 (Pan Docs "CPU Registers and Flags").

use crate::Model;

/// 플래그 레지스터(F)의 비트. 하위 4비트는 항상 0이다.
pub mod flag {
    pub const Z: u8 = 0x80;
    pub const N: u8 = 0x40;
    pub const H: u8 = 0x20;
    pub const C: u8 = 0x10;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Registers {
    pub a: u8,
    pub f: u8,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    pub sp: u16,
    pub pc: u16,
}

impl Registers {
    /// 부트 ROM이 끝난 직후의 값 (Pan Docs "Power Up Sequence"). `Auto`는 DMG로 취급한다.
    pub fn post_boot(model: Model) -> Self {
        let mut r = Self {
            sp: 0xFFFE,
            pc: 0x0100,
            ..Self::default()
        };
        match model {
            Model::Cgb => {
                r.set_af(0x1180);
                r.set_bc(0x0000);
                r.set_de(0xFF56);
                r.set_hl(0x000D);
            }
            Model::Dmg | Model::Auto => {
                r.set_af(0x01B0);
                r.set_bc(0x0013);
                r.set_de(0x00D8);
                r.set_hl(0x014D);
            }
        }
        r
    }

    pub fn af(&self) -> u16 {
        u16::from_be_bytes([self.a, self.f])
    }

    pub fn bc(&self) -> u16 {
        u16::from_be_bytes([self.b, self.c])
    }

    pub fn de(&self) -> u16 {
        u16::from_be_bytes([self.d, self.e])
    }

    pub fn hl(&self) -> u16 {
        u16::from_be_bytes([self.h, self.l])
    }

    /// F의 하위 4비트는 항상 0이다 (POP AF도 이 규칙을 따른다).
    pub fn set_af(&mut self, value: u16) {
        let [a, f] = value.to_be_bytes();
        self.a = a;
        self.f = f & 0xF0;
    }

    pub fn set_bc(&mut self, value: u16) {
        [self.b, self.c] = value.to_be_bytes();
    }

    pub fn set_de(&mut self, value: u16) {
        [self.d, self.e] = value.to_be_bytes();
    }

    pub fn set_hl(&mut self, value: u16) {
        [self.h, self.l] = value.to_be_bytes();
    }

    pub fn flag(&self, mask: u8) -> bool {
        self.f & mask != 0
    }

    pub fn set_flags(&mut self, z: bool, n: bool, h: bool, c: bool) {
        self.f = (u8::from(z) << 7) | (u8::from(n) << 6) | (u8::from(h) << 5) | (u8::from(c) << 4);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_pairs_are_high_byte_first() {
        let mut r = Registers::default();
        r.set_bc(0x1234);
        r.set_de(0x5678);
        r.set_hl(0x9ABC);
        assert_eq!(
            (r.b, r.c, r.d, r.e, r.h, r.l),
            (0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC)
        );
        assert_eq!((r.bc(), r.de(), r.hl()), (0x1234, 0x5678, 0x9ABC));
    }

    #[test]
    fn low_nibble_of_f_is_always_zero() {
        let mut r = Registers::default();
        r.set_af(0x12FF);
        assert_eq!((r.a, r.f, r.af()), (0x12, 0xF0, 0x12F0));
    }

    #[test]
    fn set_flags_packs_znhc_into_high_nibble() {
        let mut r = Registers::default();
        r.set_flags(true, false, true, false);
        assert_eq!(r.f, flag::Z | flag::H);
        assert!(r.flag(flag::Z) && !r.flag(flag::N) && r.flag(flag::H) && !r.flag(flag::C));
    }

    #[test]
    fn post_boot_dmg_matches_pan_docs() {
        let r = Registers::post_boot(Model::Dmg);
        assert_eq!(
            (r.af(), r.bc(), r.de(), r.hl(), r.sp, r.pc),
            (0x01B0, 0x0013, 0x00D8, 0x014D, 0xFFFE, 0x0100)
        );
    }

    #[test]
    fn post_boot_cgb_matches_pan_docs() {
        let r = Registers::post_boot(Model::Cgb);
        assert_eq!(
            (r.af(), r.bc(), r.de(), r.hl(), r.sp, r.pc),
            (0x1180, 0x0000, 0xFF56, 0x000D, 0xFFFE, 0x0100)
        );
    }
}
