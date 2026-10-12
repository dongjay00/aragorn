//! I/O 창: 레지스터 목록과 값 풀이.

/// 창에 보일 레지스터 묶음: (묶음 이름, [(주소, 이름)]).
pub const GROUPS: &[(&str, &[(u16, &str)])] = &[
    (
        "LCD",
        &[
            (0xFF40, "LCDC"),
            (0xFF41, "STAT"),
            (0xFF42, "SCY"),
            (0xFF43, "SCX"),
            (0xFF44, "LY"),
            (0xFF45, "LYC"),
            (0xFF47, "BGP"),
            (0xFF48, "OBP0"),
            (0xFF49, "OBP1"),
            (0xFF4A, "WY"),
            (0xFF4B, "WX"),
        ],
    ),
    (
        "타이머·인터럽트",
        &[
            (0xFF04, "DIV"),
            (0xFF05, "TIMA"),
            (0xFF06, "TMA"),
            (0xFF07, "TAC"),
            (0xFF0F, "IF"),
            (0xFFFF, "IE"),
        ],
    ),
    (
        "사운드",
        &[
            (0xFF10, "NR10"),
            (0xFF11, "NR11"),
            (0xFF12, "NR12"),
            (0xFF13, "NR13"),
            (0xFF14, "NR14"),
            (0xFF16, "NR21"),
            (0xFF17, "NR22"),
            (0xFF18, "NR23"),
            (0xFF19, "NR24"),
            (0xFF1A, "NR30"),
            (0xFF1B, "NR31"),
            (0xFF1C, "NR32"),
            (0xFF1D, "NR33"),
            (0xFF1E, "NR34"),
            (0xFF20, "NR41"),
            (0xFF21, "NR42"),
            (0xFF22, "NR43"),
            (0xFF23, "NR44"),
            (0xFF24, "NR50"),
            (0xFF25, "NR51"),
            (0xFF26, "NR52"),
        ],
    ),
];

fn on_off(value: u8, bit: u8) -> &'static str {
    if value & (1 << bit) != 0 {
        "켜짐"
    } else {
        "꺼짐"
    }
}

/// 켜진 비트의 이름을 공백으로 잇는다. 없으면 `-`.
fn bit_names(value: u8, names: &[(u8, &str)]) -> String {
    let set: Vec<&str> = names
        .iter()
        .filter(|&&(bit, _)| value & (1 << bit) != 0)
        .map(|&(_, name)| name)
        .collect();
    if set.is_empty() {
        "-".into()
    } else {
        set.join(" ")
    }
}

/// 채널 번호 1–4 중 `value`의 비트 `shift`..`shift+4`가 켜진 것.
fn channels(value: u8, shift: u8) -> String {
    bit_names(value >> shift, &[(0, "1"), (1, "2"), (2, "3"), (3, "4")])
}

/// 레지스터 값의 뜻. 풀이할 것이 없는 레지스터는 빈 문자열이다.
/// 사운드 레지스터는 읽은 값(쓰기 전용 비트는 1)을 풀이한다.
pub fn describe(addr: u16, value: u8) -> String {
    match addr {
        0xFF40 => format!(
            "LCD {}, 창 {} (맵 {}), 타일 {}, BG 맵 {}, OBJ {} {}, BG {}",
            on_off(value, 7),
            on_off(value, 5),
            if value & 0x40 != 0 { "$9C00" } else { "$9800" },
            if value & 0x10 != 0 { "$8000" } else { "$8800" },
            if value & 0x08 != 0 { "$9C00" } else { "$9800" },
            on_off(value, 1),
            if value & 0x04 != 0 { "8×16" } else { "8×8" },
            on_off(value, 0),
        ),
        0xFF41 => format!(
            "모드 {} {}, LYC=LY {}, 인터럽트 {}",
            value & 3,
            ["HBlank", "VBlank", "OAM 검색", "그리기"][usize::from(value & 3)],
            if value & 0x04 != 0 {
                "같음"
            } else {
                "다름"
            },
            bit_names(
                value,
                &[(6, "LYC"), (5, "OAM"), (4, "VBlank"), (3, "HBlank")]
            ),
        ),
        0xFF47..=0xFF49 => format!(
            "색 0–3 → 명암 {} {} {} {}",
            value & 3,
            (value >> 2) & 3,
            (value >> 4) & 3,
            (value >> 6) & 3
        ),
        0xFF07 => format!(
            "{}, {} Hz",
            if value & 0x04 != 0 {
                "켜짐"
            } else {
                "꺼짐"
            },
            ["4096", "262144", "65536", "16384"][usize::from(value & 3)],
        ),
        0xFF0F | 0xFFFF => bit_names(
            value,
            &[
                (0, "VBlank"),
                (1, "STAT"),
                (2, "타이머"),
                (3, "시리얼"),
                (4, "조이패드"),
            ],
        ),
        0xFF24 => format!("볼륨 왼쪽 {} 오른쪽 {}", (value >> 4) & 7, value & 7),
        0xFF25 => format!(
            "왼쪽 {} / 오른쪽 {}",
            channels(value, 4),
            channels(value, 0)
        ),
        0xFF26 => format!(
            "전원 {}, 소리 나는 채널 {}",
            on_off(value, 7),
            channels(value, 0)
        ),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_cover_lcd_timer_and_sound_registers() {
        let all: Vec<u16> = GROUPS
            .iter()
            .flat_map(|(_, regs)| regs.iter().map(|r| r.0))
            .collect();
        for addr in [0xFF40, 0xFF41, 0xFF04, 0xFF07, 0xFFFF, 0xFF10, 0xFF26] {
            assert!(all.contains(&addr), "{addr:04X}");
        }
        let mut sorted = all.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), all.len(), "중복이 없다");
    }

    #[test]
    fn lcdc_and_stat_are_spelled_out() {
        assert_eq!(
            describe(0xFF40, 0x91),
            "LCD 켜짐, 창 꺼짐 (맵 $9800), 타일 $8000, BG 맵 $9800, OBJ 꺼짐 8×8, BG 켜짐"
        );
        assert_eq!(
            describe(0xFF40, 0x6E),
            "LCD 꺼짐, 창 켜짐 (맵 $9C00), 타일 $8800, BG 맵 $9C00, OBJ 켜짐 8×16, BG 꺼짐"
        );
        assert_eq!(
            describe(0xFF41, 0xC5),
            "모드 1 VBlank, LYC=LY 같음, 인터럽트 LYC"
        );
        assert_eq!(
            describe(0xFF41, 0x80),
            "모드 0 HBlank, LYC=LY 다름, 인터럽트 -"
        );
    }

    #[test]
    fn timer_interrupt_and_palette_registers_are_spelled_out() {
        assert_eq!(describe(0xFF07, 0x05), "켜짐, 262144 Hz");
        assert_eq!(describe(0xFF07, 0xF8), "꺼짐, 4096 Hz");
        assert_eq!(describe(0xFFFF, 0x05), "VBlank 타이머");
        assert_eq!(describe(0xFF0F, 0xE0), "-");
        assert_eq!(describe(0xFF47, 0xE4), "색 0–3 → 명암 0 1 2 3");
    }

    #[test]
    fn sound_control_registers_are_spelled_out() {
        assert_eq!(describe(0xFF24, 0x73), "볼륨 왼쪽 7 오른쪽 3");
        assert_eq!(describe(0xFF25, 0xF3), "왼쪽 1 2 3 4 / 오른쪽 1 2");
        assert_eq!(describe(0xFF26, 0xF5), "전원 켜짐, 소리 나는 채널 1 3");
        assert_eq!(describe(0xFF10, 0x80), "");
    }
}
