//! I/O 창: LCD·타이머·인터럽트·사운드 레지스터와 채널별 음소거 (스펙 §6.3).

use aragorn_app::debug::io;
use eframe::egui;
use gb_core::DebugView;

use super::debugger::DebugCommand;

#[derive(Default)]
pub struct IoView {
    pub open: bool,
    muted: [bool; 4],
}

impl IoView {
    /// 채널 1–4의 음소거. ROM을 바꾸면 새 세션에 다시 건다.
    pub fn muted(&self) -> [bool; 4] {
        self.muted
    }

    pub fn show(&mut self, ctx: &egui::Context, debug: &DebugView<'_>) -> Vec<DebugCommand> {
        let mut commands = Vec::new();
        let mut open = self.open;
        egui::Window::new("I/O")
            .open(&mut open)
            .default_width(520.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("음소거");
                    for (channel, muted) in self.muted.iter_mut().enumerate() {
                        if ui
                            .checkbox(muted, format!("채널 {}", channel + 1))
                            .changed()
                        {
                            commands.push(DebugCommand::SetChannelMuted(channel, *muted));
                        }
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (group, registers) in io::GROUPS {
                        egui::CollapsingHeader::new(*group)
                            .default_open(true)
                            .show(ui, |ui| {
                                egui::Grid::new(*group).striped(true).show(ui, |ui| {
                                    for &(addr, name) in *registers {
                                        let value = debug.peek(addr);
                                        let mono = |t: String| egui::RichText::new(t).monospace();
                                        ui.label(mono(format!("{name:<5}")));
                                        ui.label(mono(format!("${addr:04X}")));
                                        ui.label(mono(format!("{value:02X}")));
                                        ui.label(io::describe(addr, value));
                                        ui.end_row();
                                    }
                                });
                            });
                    }
                });
            });
        self.open = open;
        commands
    }
}
