//! 디버거 창: CPU(레지스터·플래그·디스어셈블리·실행 제어·브레이크포인트)와 메모리 뷰어 (스펙 §6.3).

use aragorn_app::session::Session;
use eframe::egui;
use gb_core::DebugView;

/// 디스어셈블리에서 PC 앞에 보일 최근 실행 명령 수.
const TRACE_SHOWN: usize = 4;
/// PC부터 보일 명령 수.
const AHEAD_SHOWN: usize = 20;
const MEMORY_ROWS: usize = 0x10000 / 16;

/// 디버거 창이 앱에 요청하는 일.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DebugCommand {
    TogglePause,
    StepInstruction,
    StepFrame,
    SetBreakpoints(Vec<u16>),
}

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

/// F 레지스터의 플래그: 켜진 것은 글자, 꺼진 것은 `-` (예: `Z-H-`).
pub fn flags_text(f: u8) -> String {
    [(0x80, 'Z'), (0x40, 'N'), (0x20, 'H'), (0x10, 'C')]
        .iter()
        .map(|&(bit, c)| if f & bit != 0 { c } else { '-' })
        .collect()
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

#[derive(Default)]
pub struct DebuggerView {
    pub open: bool,
    breakpoints: Vec<u16>,
    new_breakpoint: String,
    memory_address: String,
    /// 다음에 메모리 창을 그릴 때 이 줄로 스크롤한다.
    memory_jump: Option<usize>,
}

impl DebuggerView {
    pub fn breakpoints(&self) -> &[u16] {
        &self.breakpoints
    }

    /// 창들을 그리고, 사용자가 요청한 일을 돌려준다.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        session: &Session,
        paused: bool,
    ) -> Vec<DebugCommand> {
        let mut commands = Vec::new();
        if !self.open {
            return commands;
        }
        let debug = session.debug();
        let mut open = self.open;
        egui::Window::new("CPU")
            .open(&mut open)
            .default_width(360.0)
            .show(ctx, |ui| self.cpu_window(ui, &debug, paused, &mut commands));
        egui::Window::new("메모리")
            .open(&mut open)
            .default_width(480.0)
            .show(ctx, |ui| self.memory_window(ui, &debug));
        self.open = open;
        commands
    }

    fn cpu_window(
        &mut self,
        ui: &mut egui::Ui,
        debug: &DebugView<'_>,
        paused: bool,
        commands: &mut Vec<DebugCommand>,
    ) {
        ui.horizontal(|ui| {
            if ui
                .button(if paused { "계속" } else { "일시정지" })
                .clicked()
            {
                commands.push(DebugCommand::TogglePause);
            }
            if ui.button("명령 1개").clicked() {
                commands.push(DebugCommand::StepInstruction);
            }
            if ui.button("프레임 1개").clicked() {
                commands.push(DebugCommand::StepFrame);
            }
        });
        let r = debug.registers();
        if let Some(pc) = debug.breakpoint_hit() {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!("브레이크포인트 ${pc:04X}에서 멈춤"),
            );
        }
        if let Some(lock) = debug.illegal_opcode() {
            ui.colored_label(
                ui.visuals().error_fg_color,
                format!(
                    "잘못된 옵코드 ${:02X} (${:04X})에서 CPU 정지",
                    lock.opcode, lock.pc
                ),
            );
        }
        ui.separator();
        let mono = |ui: &mut egui::Ui, text: String| {
            ui.label(egui::RichText::new(text).monospace());
        };
        mono(
            ui,
            format!(
                "AF={:02X}{:02X} BC={:02X}{:02X} DE={:02X}{:02X} HL={:02X}{:02X}",
                r.a, r.f, r.b, r.c, r.d, r.e, r.h, r.l
            ),
        );
        mono(
            ui,
            format!(
                "SP={:04X} PC={:04X} 플래그 {} IME {} HALT {}",
                r.sp,
                r.pc,
                flags_text(r.f),
                u8::from(debug.ime()),
                u8::from(debug.halted())
            ),
        );
        ui.separator();
        // 최근 실행한 명령(흐리게) + PC부터의 명령
        let trace = debug.trace();
        let recent: Vec<u16> = trace
            .iter()
            .rev()
            .skip_while(|&&pc| pc == r.pc)
            .take(TRACE_SHOWN)
            .copied()
            .collect();
        let mut rows: Vec<(u16, bool)> = recent.into_iter().rev().map(|pc| (pc, false)).collect();
        let mut addr = r.pc;
        for _ in 0..AHEAD_SHOWN {
            rows.push((addr, true));
            addr = addr.wrapping_add(u16::from(debug.disassemble(addr).len));
        }
        let mut changed = false;
        egui::Grid::new("disassembly")
            .spacing([6.0, 2.0])
            .show(ui, |ui| {
                for (pc, ahead) in rows {
                    let is_bp = self.breakpoints.binary_search(&pc).is_ok();
                    if ui
                        .selectable_label(is_bp, if is_bp { "●" } else { "○" })
                        .on_hover_text("브레이크포인트 켜기/끄기")
                        .clicked()
                    {
                        toggle_breakpoint(&mut self.breakpoints, pc);
                        changed = true;
                    }
                    let instruction = debug.disassemble(pc);
                    let bytes: Vec<String> = (0..instruction.len)
                        .map(|i| format!("{:02X}", debug.peek(pc.wrapping_add(u16::from(i)))))
                        .collect();
                    let marker = if pc == r.pc { "▶" } else { " " };
                    let text = egui::RichText::new(format!(
                        "{marker} {pc:04X}  {:<8}  {}",
                        bytes.join(" "),
                        instruction.text
                    ))
                    .monospace();
                    if ahead {
                        ui.label(text);
                    } else {
                        ui.label(text.weak());
                    }
                    ui.end_row();
                }
            });
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("브레이크포인트");
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.new_breakpoint)
                    .hint_text("0150")
                    .desired_width(60.0),
            );
            let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.button("추가").clicked() || enter)
                && let Some(pc) = parse_address(&self.new_breakpoint)
            {
                if self.breakpoints.binary_search(&pc).is_err() {
                    toggle_breakpoint(&mut self.breakpoints, pc);
                    changed = true;
                }
                self.new_breakpoint.clear();
            }
        });
        let mut remove = None;
        for &pc in &self.breakpoints {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("${pc:04X}")).monospace());
                if ui.small_button("✕").clicked() {
                    remove = Some(pc);
                }
            });
        }
        if let Some(pc) = remove {
            toggle_breakpoint(&mut self.breakpoints, pc);
            changed = true;
        }
        if changed {
            commands.push(DebugCommand::SetBreakpoints(self.breakpoints.clone()));
        }
    }

    fn memory_window(&mut self, ui: &mut egui::Ui, debug: &DebugView<'_>) {
        ui.horizontal(|ui| {
            ui.label("주소");
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.memory_address)
                    .hint_text("C000")
                    .desired_width(60.0),
            );
            let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.button("이동").clicked() || enter)
                && let Some(addr) = parse_address(&self.memory_address)
            {
                self.memory_jump = Some(usize::from(addr) / 16);
            }
        });
        let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
        let mut area = egui::ScrollArea::vertical().auto_shrink([false, false]);
        if let Some(row) = self.memory_jump.take() {
            let spacing = ui.spacing().item_spacing.y;
            area = area.vertical_scroll_offset(row as f32 * (row_height + spacing));
        }
        area.show_rows(ui, row_height, MEMORY_ROWS, |ui, range| {
            for row in range {
                let addr = (row * 16) as u16;
                let bytes: Vec<u8> = (0..16).map(|i| debug.peek(addr + i)).collect();
                ui.label(egui::RichText::new(hex_row(addr, &bytes)).monospace());
            }
        });
    }
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
