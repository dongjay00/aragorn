//! 산술/논리/회전 연산. 결과와 함께 플래그(Z N H C)를 갱신한다 (Pan Docs "CPU Instruction Set").

use super::registers::{Registers, flag};

pub const ADD: u8 = 0;
pub const ADC: u8 = 1;
pub const SUB: u8 = 2;
pub const SBC: u8 = 3;
pub const AND: u8 = 4;
pub const XOR: u8 = 5;
pub const OR: u8 = 6;
pub const CP: u8 = 7;

pub const RLC: u8 = 0;
pub const RRC: u8 = 1;
pub const RL: u8 = 2;
pub const RR: u8 = 3;
pub const SLA: u8 = 4;
pub const SRA: u8 = 5;
pub const SWAP: u8 = 6;
pub const SRL: u8 = 7;

/// 옵코드 비트 3–5가 고르는 8비트 연산을 A와 `v`에 적용한다.
pub fn alu(r: &mut Registers, op: u8, v: u8) {
    match op {
        ADD => add8(r, v, false),
        ADC => add8(r, v, true),
        SUB => {
            let result = sub_flags(r, v, false);
            r.a = result;
        }
        SBC => {
            let result = sub_flags(r, v, true);
            r.a = result;
        }
        AND => {
            r.a &= v;
            let z = r.a == 0;
            r.set_flags(z, false, true, false);
        }
        XOR => {
            r.a ^= v;
            let z = r.a == 0;
            r.set_flags(z, false, false, false);
        }
        OR => {
            r.a |= v;
            let z = r.a == 0;
            r.set_flags(z, false, false, false);
        }
        _ => {
            sub_flags(r, v, false);
        }
    }
}

fn add8(r: &mut Registers, v: u8, with_carry: bool) {
    let carry = u8::from(with_carry && r.flag(flag::C));
    let a = r.a;
    let result = a.wrapping_add(v).wrapping_add(carry);
    let h = (a & 0x0F) + (v & 0x0F) + carry > 0x0F;
    let c = u16::from(a) + u16::from(v) + u16::from(carry) > 0xFF;
    r.a = result;
    r.set_flags(result == 0, false, h, c);
}

/// A - v (- C)의 결과를 반환하고 플래그를 설정한다. A는 바꾸지 않는다 (CP가 그대로 쓴다).
fn sub_flags(r: &mut Registers, v: u8, with_carry: bool) -> u8 {
    let carry = u8::from(with_carry && r.flag(flag::C));
    let a = r.a;
    let result = a.wrapping_sub(v).wrapping_sub(carry);
    let h = (a & 0x0F) < (v & 0x0F) + carry;
    let c = u16::from(a) < u16::from(v) + u16::from(carry);
    r.set_flags(result == 0, true, h, c);
    result
}

pub fn inc8(r: &mut Registers, v: u8) -> u8 {
    let result = v.wrapping_add(1);
    let c = r.flag(flag::C);
    r.set_flags(result == 0, false, (v & 0x0F) == 0x0F, c);
    result
}

pub fn dec8(r: &mut Registers, v: u8) -> u8 {
    let result = v.wrapping_sub(1);
    let c = r.flag(flag::C);
    r.set_flags(result == 0, true, (v & 0x0F) == 0, c);
    result
}

/// ADD HL,rr: H는 비트 11, C는 비트 15에서의 올림이다.
pub fn add_hl(r: &mut Registers, v: u16) {
    let hl = r.hl();
    let (result, c) = hl.overflowing_add(v);
    let h = (hl & 0x0FFF) + (v & 0x0FFF) > 0x0FFF;
    let z = r.flag(flag::Z);
    r.set_hl(result);
    r.set_flags(z, false, h, c);
}

/// ADD SP,e / LD HL,SP+e: H/C는 하위 바이트의 부호 없는 덧셈 기준이다.
pub fn add_sp_e(r: &mut Registers, e: i8) -> u16 {
    let sp = r.sp;
    let operand = u16::from(e as u8);
    let h = (sp & 0x000F) + (operand & 0x000F) > 0x000F;
    let c = (sp & 0x00FF) + operand > 0x00FF;
    r.set_flags(false, false, h, c);
    sp.wrapping_add_signed(i16::from(e))
}

/// 직전 덧셈/뺄셈(N 플래그) 결과를 BCD로 보정한다.
pub fn daa(r: &mut Registers) {
    let mut a = r.a;
    let mut carry = r.flag(flag::C);
    let n = r.flag(flag::N);
    if n {
        if carry {
            a = a.wrapping_sub(0x60);
        }
        if r.flag(flag::H) {
            a = a.wrapping_sub(0x06);
        }
    } else {
        if carry || a > 0x99 {
            a = a.wrapping_add(0x60);
            carry = true;
        }
        if r.flag(flag::H) || (a & 0x0F) > 0x09 {
            a = a.wrapping_add(0x06);
        }
    }
    r.a = a;
    r.set_flags(a == 0, n, false, carry);
}

pub fn cpl(r: &mut Registers) {
    r.a = !r.a;
    r.f |= flag::N | flag::H;
}

pub fn scf(r: &mut Registers) {
    r.f = (r.f & flag::Z) | flag::C;
}

pub fn ccf(r: &mut Registers) {
    r.f = (r.f & (flag::Z | flag::C)) ^ flag::C;
}

/// CB 접두 회전/시프트. RLCA 같은 A 전용 명령은 호출한 쪽에서 Z를 지운다.
pub fn shift(r: &mut Registers, op: u8, v: u8) -> u8 {
    let carry_in = u8::from(r.flag(flag::C));
    let (result, c) = match op {
        RLC => (v.rotate_left(1), v & 0x80 != 0),
        RRC => (v.rotate_right(1), v & 0x01 != 0),
        RL => ((v << 1) | carry_in, v & 0x80 != 0),
        RR => ((v >> 1) | (carry_in << 7), v & 0x01 != 0),
        SLA => (v << 1, v & 0x80 != 0),
        SRA => ((v >> 1) | (v & 0x80), v & 0x01 != 0),
        SWAP => (v.rotate_left(4), false),
        _ => (v >> 1, v & 0x01 != 0),
    };
    r.set_flags(result == 0, false, false, c);
    result
}

pub fn bit(r: &mut Registers, b: u8, v: u8) {
    let c = r.flag(flag::C);
    r.set_flags(v & (1 << b) == 0, false, true, c);
}

#[cfg(test)]
mod tests {
    use super::flag::{C, H, N, Z};
    use super::*;

    fn regs(a: u8, f: u8) -> Registers {
        Registers {
            a,
            f,
            ..Registers::default()
        }
    }

    fn apply(a: u8, f: u8, op: u8, v: u8) -> (u8, u8) {
        let mut r = regs(a, f);
        alu(&mut r, op, v);
        (r.a, r.f)
    }

    #[test]
    fn add_sets_zero_half_and_carry() {
        assert_eq!(apply(0x3A, 0, ADD, 0xC6), (0x00, Z | H | C));
        assert_eq!(apply(0x0F, 0, ADD, 0x01), (0x10, H));
    }

    #[test]
    fn adc_adds_carry_flag() {
        assert_eq!(apply(0xE1, C, ADC, 0x0F), (0xF1, H));
    }

    #[test]
    fn sub_sets_n_and_borrows() {
        assert_eq!(apply(0x3E, 0, SUB, 0x3E), (0x00, Z | N));
        assert_eq!(apply(0x3E, 0, SUB, 0x0F), (0x2F, N | H));
        assert_eq!(apply(0x3E, 0, SUB, 0x40), (0xFE, N | C));
    }

    #[test]
    fn sbc_subtracts_carry_flag() {
        assert_eq!(apply(0x3B, C, SBC, 0x2A), (0x10, N));
        assert_eq!(apply(0x3B, C, SBC, 0x4F), (0xEB, N | H | C));
    }

    #[test]
    fn and_sets_half_carry() {
        assert_eq!(apply(0x5A, 0, AND, 0x3F), (0x1A, H));
        assert_eq!(apply(0x5A, 0, AND, 0x00), (0x00, Z | H));
    }

    #[test]
    fn xor_and_or_clear_other_flags() {
        assert_eq!(apply(0xFF, 0xF0, XOR, 0xFF), (0x00, Z));
        assert_eq!(apply(0x5A, 0xF0, OR, 0x03), (0x5B, 0));
    }

    #[test]
    fn cp_keeps_a() {
        assert_eq!(apply(0x3C, 0, CP, 0x2F), (0x3C, N | H));
        assert_eq!(apply(0x3C, 0, CP, 0x3C), (0x3C, Z | N));
    }

    #[test]
    fn inc_keeps_carry() {
        let mut r = regs(0, C);
        assert_eq!(inc8(&mut r, 0xFF), 0x00);
        assert_eq!(r.f, Z | H | C);
        let mut r = regs(0, 0);
        assert_eq!(inc8(&mut r, 0x0F), 0x10);
        assert_eq!(r.f, H);
    }

    #[test]
    fn dec_keeps_carry() {
        let mut r = regs(0, C);
        assert_eq!(dec8(&mut r, 0x01), 0x00);
        assert_eq!(r.f, Z | N | C);
        let mut r = regs(0, 0);
        assert_eq!(dec8(&mut r, 0x00), 0xFF);
        assert_eq!(r.f, N | H);
    }

    #[test]
    fn add_hl_keeps_zero_flag() {
        let mut r = regs(0, Z);
        r.set_hl(0x8A23);
        add_hl(&mut r, 0x0605);
        assert_eq!((r.hl(), r.f), (0x9028, Z | H));
        let mut r = regs(0, 0);
        r.set_hl(0x8A23);
        add_hl(&mut r, 0x8A23);
        assert_eq!((r.hl(), r.f), (0x1446, H | C));
    }

    #[test]
    fn add_sp_e_uses_low_byte_carries_and_clears_zero() {
        let mut r = regs(0, Z | N);
        r.sp = 0x00FF;
        assert_eq!(add_sp_e(&mut r, 1), 0x0100);
        assert_eq!((r.f, r.sp), (H | C, 0x00FF));
        let mut r = regs(0, 0);
        r.sp = 0x0000;
        assert_eq!(add_sp_e(&mut r, -1), 0xFFFF);
        assert_eq!(r.f, 0);
        let mut r = regs(0, 0);
        r.sp = 0xFFF8;
        assert_eq!(add_sp_e(&mut r, 2), 0xFFFA);
        assert_eq!(r.f, 0);
    }

    #[test]
    fn daa_after_addition() {
        let mut r = regs(0x7D, 0);
        daa(&mut r);
        assert_eq!((r.a, r.f), (0x83, 0));
        let mut r = regs(0x9A, 0);
        daa(&mut r);
        assert_eq!((r.a, r.f), (0x00, Z | C));
    }

    #[test]
    fn daa_after_subtraction() {
        let mut r = regs(0x4B, N | H);
        daa(&mut r);
        assert_eq!((r.a, r.f), (0x45, N));
    }

    #[test]
    fn cpl_scf_ccf() {
        let mut r = regs(0x35, Z | C);
        cpl(&mut r);
        assert_eq!((r.a, r.f), (0xCA, Z | N | H | C));
        let mut r = regs(0, Z | N | H);
        scf(&mut r);
        assert_eq!(r.f, Z | C);
        let mut r = regs(0, Z | N | H | C);
        ccf(&mut r);
        assert_eq!(r.f, Z);
        let mut r = regs(0, 0);
        ccf(&mut r);
        assert_eq!(r.f, C);
    }

    #[test]
    fn rotates_and_shifts() {
        let cases = [
            (RLC, 0x85, false, 0x0B, C),
            (RRC, 0x01, false, 0x80, C),
            (RL, 0x80, false, 0x00, Z | C),
            (RL, 0x11, true, 0x23, 0),
            (RR, 0x01, false, 0x00, Z | C),
            (RR, 0x8A, true, 0xC5, 0),
            (SLA, 0xFF, false, 0xFE, C),
            (SRA, 0x8A, false, 0xC5, 0),
            (SWAP, 0xF0, true, 0x0F, 0),
            (SRL, 0x01, false, 0x00, Z | C),
        ];
        for (op, input, carry_in, value, f) in cases {
            let mut r = regs(0, if carry_in { C } else { 0 });
            assert_eq!(
                shift(&mut r, op, input),
                value,
                "op {op} input {input:#04X}"
            );
            assert_eq!(r.f, f, "op {op} input {input:#04X}");
        }
    }

    #[test]
    fn bit_sets_zero_when_bit_clear_and_keeps_carry() {
        let mut r = regs(0, C);
        bit(&mut r, 7, 0x80);
        assert_eq!(r.f, H | C);
        bit(&mut r, 0, 0x80);
        assert_eq!(r.f, Z | H | C);
    }
}
