//! 치트 창: 게임 지니·게임샤크 코드를 더하고, 켜고 끄고, 지운다. 목록은 게임마다 따로 저장된다.

use aragorn_app::cheats::CheatEntry;
use eframe::egui;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheatCommand {
    Add { code: String, name: String },
    SetEnabled(usize, bool),
    Remove(usize),
}

#[derive(Debug, Default)]
pub struct CheatsView {
    pub open: bool,
    code: String,
    name: String,
}

impl CheatsView {
    /// 더하기에 성공하면 입력칸을 비운다.
    pub fn added(&mut self) {
        self.code.clear();
        self.name.clear();
    }

    pub fn show(&mut self, ctx: &egui::Context, entries: &[CheatEntry]) -> Vec<CheatCommand> {
        let mut commands = Vec::new();
        let mut open = self.open;
        egui::Window::new("치트")
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                if entries.is_empty() {
                    ui.label("아직 치트가 없습니다.");
                }
                egui::Grid::new("cheats").striped(true).show(ui, |ui| {
                    for (index, entry) in entries.iter().enumerate() {
                        let mut enabled = entry.enabled;
                        if ui.checkbox(&mut enabled, entry.label()).changed() {
                            commands.push(CheatCommand::SetEnabled(index, enabled));
                        }
                        ui.monospace(&entry.code);
                        if ui.small_button("지우기").clicked() {
                            commands.push(CheatCommand::Remove(index));
                        }
                        ui.end_row();
                    }
                });
                ui.separator();
                egui::Grid::new("new_cheat").show(ui, |ui| {
                    ui.label("코드");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.code)
                            .hint_text("00A-17B-C49 또는 010138CD")
                            .font(egui::TextStyle::Monospace),
                    );
                    ui.end_row();
                    ui.label("이름");
                    ui.add(egui::TextEdit::singleline(&mut self.name).hint_text("무한 체력"));
                    ui.end_row();
                });
                if ui.button("더하기").clicked() {
                    commands.push(CheatCommand::Add {
                        code: self.code.clone(),
                        name: self.name.clone(),
                    });
                }
                ui.label("여러 코드를 + 로 이어 쓰면 한 치트로 켜고 끕니다.");
            });
        self.open = open;
        commands
    }
}
