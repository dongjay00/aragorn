//! 링크 케이블 창: 이 컴퓨터에서 기다리거나 상대 주소로 연결한다.

use aragorn_app::link::LinkStatus;
use aragorn_host::DEFAULT_PORT;
use eframe::egui;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkCommand {
    Host,
    Connect(String),
    Disconnect,
}

#[derive(Debug, Default)]
pub struct LinkView {
    pub open: bool,
    address: String,
}

impl LinkView {
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        status: Option<&LinkStatus>,
    ) -> Option<LinkCommand> {
        let mut command = None;
        let mut open = self.open;
        egui::Window::new("링크 케이블")
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("같은 네트워크(와이파이)의 다른 Aragorn과 통신 교환·대전을 합니다.");
                ui.label("한쪽이 \"기다리기\"를 누르고, 다른 쪽이 보이는 주소로 연결하세요.");
                ui.separator();
                match status {
                    Some(status) if is_active(status) => {
                        ui.strong(status.text());
                        if ui.button("끊기").clicked() {
                            command = Some(LinkCommand::Disconnect);
                        }
                    }
                    closed => {
                        if let Some(status) = closed {
                            ui.label(status.text());
                        }
                        self.idle(ui, &mut command);
                    }
                }
                ui.separator();
                ui.label("교환·대전 중에는 되감기를 쓸 수 없습니다. 스테이트를 불러오면 상대와 어긋날 수 있습니다.");
            });
        self.open = open;
        command
    }

    fn idle(&mut self, ui: &mut egui::Ui, command: &mut Option<LinkCommand>) {
        if ui
            .button(format!("기다리기 (포트 {DEFAULT_PORT})"))
            .clicked()
        {
            *command = Some(LinkCommand::Host);
        }
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.address)
                    .hint_text("192.168.0.5")
                    .desired_width(160.0),
            );
            if ui
                .add_enabled(!self.address.trim().is_empty(), egui::Button::new("연결"))
                .clicked()
            {
                *command = Some(LinkCommand::Connect(self.address.clone()));
            }
        });
    }
}

/// 기다리는 중이거나 연결된 상태.
fn is_active(status: &LinkStatus) -> bool {
    !matches!(status, LinkStatus::Closed { .. })
}
