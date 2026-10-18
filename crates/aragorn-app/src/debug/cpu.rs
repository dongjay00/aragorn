//! CPU·메모리 디버거가 보일 내용: 레지스터 줄, 디스어셈블리, 메모리 줄, 주소 입력, 브레이크포인트 목록.

use gb_core::DebugView;

/// 디스어셈블리에서 PC 앞에 보일 최근 실행 명령 수.
pub const TRACE_SHOWN: usize = 4;
/// PC부터 보일 명령 수.
pub const AHEAD_SHOWN: usize = 20;
/// 메모리 뷰어의 줄 수 (한 줄 16바이트).
pub const MEMORY_ROWS: usize = 0x10000 / 16;

/// `0150`, `$0150`, `0x0150`, `c000` 같은 16진수 주소. 공백은 무시한다.
pub fn parse_address(text: &str) -> Option<u16> {
    let t = text.trim();
    let t = t
        .strip_prefix('$')
        .or_else(|| t.strip_prefix("0x"))
        .or_else(|| t.strip_prefix("0X"))
        .unwrap_or(t);
    if t.is_empty() || t.len() > 4 {
        return None;
    }
    u16::from_str_radix(t, 16).ok()
}

/// 메모리 한 줄: `C000  00 11 22 … FF  ..."...`
pub fn hex_row(addr: u16, bytes: &[u8]) -> String {
    let hex: Vec<String> = bytes.iter().map(|b| format!("{b:02X}")).collect();
    let ascii: String = bytes
        .iter()
        .map(|&b| {
            if (0x20..0x7F).contains(&b) {
                b as char
            } else {
                '.'
            }
        })
        .collect();
    format!("{addr:04X}  {}  {ascii}", hex.join(" "))
}

/// 메모리 뷰어의 `row`번째 줄 (`row * 16`번지부터 16바이트).
pub fn memory_row(debug: &DebugView<'_>, row: usize) -> String {
    let addr = (row * 16) as u16;
    let bytes: Vec<u8> = (0..16).map(|i| debug.peek(addr + i)).collect();
    hex_row(addr, &bytes)
}

/// F 레지스터의 플래그: 켜진 것은 글자, 꺼진 것은 `-` (예: `Z-H-`).
pub fn flags_text(f: u8) -> String {
    [(0x80, 'Z'), (0x40, 'N'), (0x20, 'H'), (0x10, 'C')]
        .iter()
        .map(|&(bit, c)| if f & bit != 0 { c } else { '-' })
        .collect()
}

/// 레지스터 두 줄.
pub fn register_lines(debug: &DebugView<'_>) -> [String; 2] {
    let r = debug.registers();
    [
        format!(
            "AF={:02X}{:02X} BC={:02X}{:02X} DE={:02X}{:02X} HL={:02X}{:02X}",
            r.a, r.f, r.b, r.c, r.d, r.e, r.h, r.l
        ),
        format!(
            "SP={:04X} PC={:04X} 플래그 {} IME {} HALT {}",
            r.sp,
            r.pc,
            flags_text(r.f),
            u8::from(debug.ime()),
            u8::from(debug.halted())
        ),
    ]
}

/// 목록에 있으면 빼고 없으면 넣는다. 목록은 정렬해 둔다.
pub fn toggle_breakpoint(list: &mut Vec<u16>, pc: u16) {
    match list.binary_search(&pc) {
        Ok(i) => {
            list.remove(i);
        }
        Err(i) => list.insert(i, pc),
    }
}

/// 디스어셈블리 한 줄.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisasmRow {
    pub pc: u16,
    /// `▶ 0150  C3 50 01   JP $0150` 형식. 지금 PC면 `▶`가 붙는다.
    pub text: String,
    /// PC부터의 명령이면 참, 최근 실행한 명령이면 거짓(흐리게 보인다).
    pub ahead: bool,
}

/// 최근 실행한 명령 `TRACE_SHOWN`개와 PC부터 `AHEAD_SHOWN`개.
pub fn disassembly(debug: &DebugView<'_>) -> Vec<DisasmRow> {
    let pc_now = debug.registers().pc;
    let recent: Vec<u16> = debug
        .trace()
        .iter()
        .rev()
        .skip_while(|&&pc| pc == pc_now)
        .take(TRACE_SHOWN)
        .copied()
        .collect();
    let mut rows: Vec<(u16, bool)> = recent.into_iter().rev().map(|pc| (pc, false)).collect();
    let mut addr = pc_now;
    for _ in 0..AHEAD_SHOWN {
        rows.push((addr, true));
        addr = addr.wrapping_add(u16::from(debug.disassemble(addr).len));
    }
    rows.into_iter()
        .map(|(pc, ahead)| {
            let instruction = debug.disassemble(pc);
            let bytes: Vec<String> = (0..instruction.len)
                .map(|i| format!("{:02X}", debug.peek(pc.wrapping_add(u16::from(i)))))
                .collect();
            let marker = if pc == pc_now { "▶" } else { " " };
            DisasmRow {
                pc,
                text: format!(
                    "{marker} {pc:04X}  {:<8}  {}",
                    bytes.join(" "),
                    instruction.text
                ),
                ahead,
            }
        })
        .collect()
}

/// CPU 창 위쪽에 보일 경고: 브레이크포인트 정지, 잘못된 옵코드.
pub fn stop_notices(debug: &DebugView<'_>) -> Vec<String> {
    let mut notices = Vec::new();
    if let Some(pc) = debug.breakpoint_hit() {
        notices.push(format!("브레이크포인트 ${pc:04X}에서 멈춤"));
    }
    if let Some(lock) = debug.illegal_opcode() {
        notices.push(format!(
            "잘못된 옵코드 ${:02X} (${:04X})에서 CPU 정지",
            lock.opcode, lock.pc
        ));
    }
    notices
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_accept_common_hex_forms() {
        assert_eq!(parse_address("0150"), Some(0x0150));
        assert_eq!(parse_address(" $c000 "), Some(0xC000));
        assert_eq!(parse_address("0xFF44"), Some(0xFF44));
        assert_eq!(parse_address("ff"), Some(0x00FF));
        assert_eq!(parse_address(""), None);
        assert_eq!(parse_address("10000"), None);
        assert_eq!(parse_address("zz"), None);
    }

    #[test]
    fn hex_row_shows_bytes_and_printable_ascii() {
        let bytes: Vec<u8> = (0x40..0x50).collect();
        assert_eq!(
            hex_row(0xC000, &bytes),
            "C000  40 41 42 43 44 45 46 47 48 49 4A 4B 4C 4D 4E 4F  @ABCDEFGHIJKLMNO"
        );
        assert!(hex_row(0, &[0x00, 0x7F, 0xFF]).ends_with("..."));
    }

    #[test]
    fn flags_show_set_bits() {
        assert_eq!(flags_text(0xB0), "Z-HC");
        assert_eq!(flags_text(0x00), "----");
        assert_eq!(flags_text(0xF0), "ZNHC");
    }

    #[test]
    fn toggling_keeps_breakpoints_sorted_and_unique() {
        let mut list = Vec::new();
        for pc in [0x0200, 0x0100, 0x0300, 0x0100] {
            toggle_breakpoint(&mut list, pc);
        }
        assert_eq!(list, [0x0200, 0x0300]);
    }
}
