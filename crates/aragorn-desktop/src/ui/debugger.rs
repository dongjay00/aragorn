//! 디버거 창: CPU(레지스터·플래그·디스어셈블리·실행 제어·브레이크포인트)와 메모리 뷰어 (스펙 §6.3).

use aragorn_app::debug::cpu::{self, parse_address, toggle_breakpoint};
use aragorn_app::session::Session;
use eframe::egui;
use gb_core::DebugView;

/// 디버거 창이 앱에 요청하는 일.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DebugCommand {
    TogglePause,
    StepInstruction,
    StepFrame,
    SetBreakpoints(Vec<u16>),
    /// 채널(0–3)의 음소거를 켜거나 끈다.
    SetChannelMuted(usize, bool),
}

#[derive(Default)]
pub struct DebuggerView {
    pub cpu_open: bool,
    pub memory_open: bool,
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
        let debug = session.debug();
        let mut open = self.cpu_open;
        egui::Window::new("CPU")
            .open(&mut open)
            .default_width(360.0)
            .show(ctx, |ui| self.cpu_window(ui, &debug, paused, &mut commands));
        self.cpu_open = open;
        let mut open = self.memory_open;
        egui::Window::new("메모리")
            .open(&mut open)
            .default_width(480.0)
            .show(ctx, |ui| self.memory_window(ui, &debug));
        self.memory_open = open;
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
        for (i, notice) in cpu::stop_notices(debug).into_iter().enumerate() {
            // 브레이크포인트 정지는 경고, 잘못된 옵코드는 오류 색이다.
            let color = if i == 0 && debug.breakpoint_hit().is_some() {
                ui.visuals().warn_fg_color
            } else {
                ui.visuals().error_fg_color
            };
            ui.colored_label(color, notice);
        }
        ui.separator();
        for line in cpu::register_lines(debug) {
            ui.label(egui::RichText::new(line).monospace());
        }
        ui.separator();
        let mut changed = false;
        egui::Grid::new("disassembly")
            .spacing([6.0, 2.0])
            .show(ui, |ui| {
                for row in cpu::disassembly(debug) {
                    let is_bp = self.breakpoints.binary_search(&row.pc).is_ok();
                    if ui
                        .selectable_label(is_bp, if is_bp { "●" } else { "○" })
                        .on_hover_text("브레이크포인트 켜기/끄기")
                        .clicked()
                    {
                        toggle_breakpoint(&mut self.breakpoints, row.pc);
                        changed = true;
                    }
                    let text = egui::RichText::new(row.text).monospace();
                    if row.ahead {
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
        area.show_rows(ui, row_height, cpu::MEMORY_ROWS, |ui, range| {
            for row in range {
                ui.label(egui::RichText::new(cpu::memory_row(debug, row)).monospace());
            }
        });
    }
}
