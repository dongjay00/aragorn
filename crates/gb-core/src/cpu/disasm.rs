//! SM83 디스어셈블러 (디버거용). 니모닉은 Pan Docs "CPU Instruction Set" 표기를 따른다.
//!
//! 옵코드를 x(7–6)·y(5–3)·z(2–0) 비트와 p = y >> 1, q = y & 1로 나눠 해석한다.

/// 명령 하나의 길이와 문자열.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction {
    /// 바이트 수 (1–3)
    pub len: u8,
    pub text: String,
}

const R: [&str; 8] = ["B", "C", "D", "E", "H", "L", "(HL)", "A"];
const RP: [&str; 4] = ["BC", "DE", "HL", "SP"];
const RP2: [&str; 4] = ["BC", "DE", "HL", "AF"];
const CC: [&str; 4] = ["NZ", "Z", "NC", "C"];
const ALU: [&str; 8] = [
    "ADD A,", "ADC A,", "SUB ", "SBC A,", "AND ", "XOR ", "OR ", "CP ",
];
const ROT: [&str; 8] = ["RLC", "RRC", "RL", "RR", "SLA", "SRA", "SWAP", "SRL"];
const ACC: [&str; 8] = ["RLCA", "RRCA", "RLA", "RRA", "DAA", "CPL", "SCF", "CCF"];

/// 부호 있는 8비트 값을 `+$02` / `-$02`로 쓴다.
fn signed(d: u8) -> String {
    let d = d as i8;
    if d < 0 {
        format!("-${:02X}", d.unsigned_abs())
    } else {
        format!("+${d:02X}")
    }
}

/// `addr`의 명령을 해석한다. `read`는 부작용 없이 메모리를 읽어야 한다.
pub fn disassemble(addr: u16, read: impl Fn(u16) -> u8) -> Instruction {
    let op = read(addr);
    let n = read(addr.wrapping_add(1));
    let nn = u16::from_le_bytes([n, read(addr.wrapping_add(2))]);
    let (x, y, z) = (op >> 6, usize::from((op >> 3) & 7), usize::from(op & 7));
    let (p, q) = (y >> 1, y & 1);
    let one = |text: String| Instruction { len: 1, text };
    let two = |text: String| Instruction { len: 2, text };
    let three = |text: String| Instruction { len: 3, text };
    // JR의 도착 주소: 다음 명령 위치 + 부호 있는 오프셋
    let jr_target = addr.wrapping_add(2).wrapping_add_signed(i16::from(n as i8));
    let illegal = || one(format!("DB ${op:02X}"));
    match x {
        0 => match z {
            0 => match y {
                0 => one("NOP".into()),
                1 => three(format!("LD (${nn:04X}),SP")),
                2 => two("STOP".into()),
                3 => two(format!("JR ${jr_target:04X}")),
                _ => two(format!("JR {},${jr_target:04X}", CC[y - 4])),
            },
            1 if q == 0 => three(format!("LD {},${nn:04X}", RP[p])),
            1 => one(format!("ADD HL,{}", RP[p])),
            2 => {
                let mem = ["(BC)", "(DE)", "(HL+)", "(HL-)"][p];
                one(if q == 0 {
                    format!("LD {mem},A")
                } else {
                    format!("LD A,{mem}")
                })
            }
            3 => one(format!("{} {}", ["INC", "DEC"][q], RP[p])),
            4 => one(format!("INC {}", R[y])),
            5 => one(format!("DEC {}", R[y])),
            6 => two(format!("LD {},${n:02X}", R[y])),
            _ => one(ACC[y].into()),
        },
        1 if y == 6 && z == 6 => one("HALT".into()),
        1 => one(format!("LD {},{}", R[y], R[z])),
        2 => one(format!("{}{}", ALU[y], R[z])),
        _ => match z {
            0 => match y {
                0..=3 => one(format!("RET {}", CC[y])),
                4 => two(format!("LDH ($FF{n:02X}),A")),
                5 => two(format!("ADD SP,{}", signed(n))),
                6 => two(format!("LDH A,($FF{n:02X})")),
                _ => two(format!("LD HL,SP{}", signed(n))),
            },
            1 if q == 0 => one(format!("POP {}", RP2[p])),
            1 => one(["RET", "RETI", "JP HL", "LD SP,HL"][p].into()),
            2 => match y {
                0..=3 => three(format!("JP {},${nn:04X}", CC[y])),
                4 => one("LD ($FF00+C),A".into()),
                5 => three(format!("LD (${nn:04X}),A")),
                6 => one("LD A,($FF00+C)".into()),
                _ => three(format!("LD A,(${nn:04X})")),
            },
            3 => match y {
                0 => three(format!("JP ${nn:04X}")),
                1 => {
                    let (x, y, z) = (n >> 6, usize::from((n >> 3) & 7), usize::from(n & 7));
                    two(match x {
                        0 => format!("{} {}", ROT[y], R[z]),
                        1 => format!("BIT {y},{}", R[z]),
                        2 => format!("RES {y},{}", R[z]),
                        _ => format!("SET {y},{}", R[z]),
                    })
                }
                6 => one("DI".into()),
                7 => one("EI".into()),
                _ => illegal(),
            },
            4 if y <= 3 => three(format!("CALL {},${nn:04X}", CC[y])),
            4 => illegal(),
            5 if q == 0 => one(format!("PUSH {}", RP2[p])),
            5 if p == 0 => three(format!("CALL ${nn:04X}")),
            5 => illegal(),
            6 => two(format!("{}${n:02X}", ALU[y])),
            _ => one(format!("RST ${:02X}", y * 8)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::FlatBus;
    use super::super::{Cpu, Registers};
    use super::*;

    fn text(bytes: &[u8], at: u16) -> Instruction {
        disassemble(at, |a| {
            bytes
                .get(usize::from(a.wrapping_sub(at)))
                .copied()
                .unwrap_or(0)
        })
    }

    #[test]
    fn mnemonics_follow_pan_docs() {
        let cases: [(&[u8], &str); 24] = [
            (&[0x00], "NOP"),
            (&[0x08, 0x34, 0x12], "LD ($1234),SP"),
            (&[0x01, 0xCD, 0xAB], "LD BC,$ABCD"),
            (&[0x22], "LD (HL+),A"),
            (&[0x3A], "LD A,(HL-)"),
            (&[0x36, 0x7F], "LD (HL),$7F"),
            (&[0x76], "HALT"),
            (&[0x7E], "LD A,(HL)"),
            (&[0x8E], "ADC A,(HL)"),
            (&[0x97], "SUB A"),
            (&[0xFE, 0x90], "CP $90"),
            (&[0xE0, 0x44], "LDH ($FF44),A"),
            (&[0xF0, 0x40], "LDH A,($FF40)"),
            (&[0xE2], "LD ($FF00+C),A"),
            (&[0xEA, 0x00, 0xC0], "LD ($C000),A"),
            (&[0xE8, 0xFE], "ADD SP,-$02"),
            (&[0xF8, 0x05], "LD HL,SP+$05"),
            (&[0xC2, 0x50, 0x01], "JP NZ,$0150"),
            (&[0xCD, 0x00, 0x40], "CALL $4000"),
            (&[0xD8], "RET C"),
            (&[0xFF], "RST $38"),
            (&[0xCB, 0x37], "SWAP A"),
            (&[0xCB, 0x7E], "BIT 7,(HL)"),
            (&[0xD3], "DB $D3"),
        ];
        for (bytes, expected) in cases {
            assert_eq!(text(bytes, 0x0100).text, expected, "{bytes:02X?}");
        }
    }

    #[test]
    fn relative_jumps_show_absolute_target() {
        assert_eq!(text(&[0x18, 0xFE], 0x0150).text, "JR $0150");
        assert_eq!(text(&[0x20, 0x05], 0x0150).text, "JR NZ,$0157");
        assert_eq!(
            text(&[0x38, 0x80], 0x0000).text,
            "JR C,$FF82",
            "주소가 감싼다"
        );
    }

    #[test]
    fn every_cb_opcode_is_two_bytes_and_distinct() {
        let mut seen = std::collections::HashSet::new();
        for op in 0..=255u8 {
            let i = text(&[0xCB, op], 0);
            assert_eq!(i.len, 2);
            assert!(seen.insert(i.text.clone()), "{}", i.text);
        }
    }

    /// 실제 CPU가 명령 하나를 실행하고 PC가 얼마나 나아가는지와 길이를 대조한다. 피연산자는 0이라
    /// 분기 명령은 주소 0으로 가거나(점프·호출·복귀) 다음 명령으로 간다(JR 0). 0으로 간 경우는 빼고 본다.
    #[test]
    fn lengths_match_cpu_execution() {
        const ILLEGAL: [u8; 11] = [
            0xD3, 0xDB, 0xDD, 0xE3, 0xE4, 0xEB, 0xEC, 0xED, 0xF4, 0xFC, 0xFD,
        ];
        for op in 0..=255u8 {
            if ILLEGAL.contains(&op) || op == 0x76 {
                continue;
            }
            let program: &[u8] = if op == 0xCB { &[0xCB, 0x00] } else { &[op] };
            let expected = text(program, 0x0100).len;
            let mut lengths = Vec::new();
            for f in [0x00, 0xF0] {
                let mut bus = FlatBus::with_program(program);
                let mut cpu = Cpu::new(Registers {
                    f,
                    sp: 0xFFF0,
                    pc: 0x0100,
                    ..Registers::default()
                });
                cpu.step(&mut bus);
                let pc = cpu.regs.pc;
                // RST·JP HL 같은 무조건 분기는 0번지 근처로 간다
                if !(0x0000..0x0100).contains(&pc) {
                    lengths.push(pc - 0x0100);
                }
            }
            if lengths.is_empty() {
                continue;
            }
            for len in lengths {
                assert_eq!(u16::from(expected), len, "{op:#04X}");
            }
        }
    }
}
