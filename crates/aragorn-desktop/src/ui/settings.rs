//! 설정 창: 키보드·게임패드 매핑과 배속 속도 (스펙 §6.2).

use aragorn_app::{
    input::{Action, InputConfig},
    pacing::FastForward,
};
use eframe::egui::{self, Key};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Device {
    Keyboard,
    Gamepad,
}

#[derive(Debug, Default)]
pub struct SettingsView {
    pub open: bool,
    /// 다음 입력을 기다리는 칸
    capturing: Option<(Device, Action)>,
}

impl SettingsView {
    /// 키 입력을 기다리는 중이면 게임과 단축키에 입력을 넘기지 않는다.
    pub fn is_capturing(&self) -> bool {
        self.capturing.is_some()
    }

    /// 입력을 기다리는 칸이 있으면 이번에 눌린 키나 패드 버튼을 지정한다. 지정했으면 `true`.
    pub fn capture(
        &mut self,
        input: &mut InputConfig,
        key: Option<Key>,
        pad_button: Option<&str>,
    ) -> bool {
        let name = match self.capturing {
            Some((Device::Keyboard, _)) => key.map(Key::name),
            Some((Device::Gamepad, _)) => pad_button,
            None => None,
        };
        let (Some(name), Some((device, action))) = (name, self.capturing) else {
            return false;
        };
        bindings_mut(input, device).bind(action, name);
        self.capturing = None;
        true
    }

    /// 설정 창을 그린다. 설정이 바뀌었으면 `true`.
    pub fn show(&mut self, ctx: &egui::Context, input: &mut InputConfig) -> bool {
        let mut open = self.open;
        let mut changed = false;
        egui::Window::new("설정")
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("칸을 누른 뒤 지정할 키나 게임패드 버튼을 누르세요. 오른쪽 클릭하면 지정을 해제합니다.");
                ui.add_space(4.0);
                egui::Grid::new("bindings").striped(true).show(ui, |ui| {
                    ui.strong("기능");
                    ui.strong("키보드");
                    ui.strong("게임패드");
                    ui.end_row();
                    for action in Action::ALL {
                        ui.label(action.label());
                        for device in [Device::Keyboard, Device::Gamepad] {
                            changed |= self.binding_cell(ui, input, device, action);
                        }
                        ui.end_row();
                    }
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label("배속 속도:");
                    for speed in FastForward::ALL {
                        changed |= ui
                            .radio_value(&mut input.fast_forward, speed, speed.label())
                            .changed();
                    }
                });
                ui.add_space(8.0);
                if ui.button("기본값으로").clicked() {
                    *input = InputConfig::default();
                    self.capturing = None;
                    changed = true;
                }
            });
        self.open = open;
        if !open {
            self.capturing = None;
        }
        changed
    }

    fn binding_cell(
        &mut self,
        ui: &mut egui::Ui,
        input: &mut InputConfig,
        device: Device,
        action: Action,
    ) -> bool {
        let waiting = self.capturing == Some((device, action));
        let name = bindings_mut(input, device).get(action);
        let text = if waiting {
            "입력 대기…"
        } else if name.is_empty() {
            "없음"
        } else {
            name
        };
        let response = ui.selectable_label(waiting, text);
        if response.clicked() {
            self.capturing = if waiting {
                None
            } else {
                Some((device, action))
            };
        }
        if response.secondary_clicked() {
            bindings_mut(input, device).bind(action, "");
            self.capturing = None;
            return true;
        }
        false
    }
}

fn bindings_mut(input: &mut InputConfig, device: Device) -> &mut aragorn_app::input::Bindings {
    match device {
        Device::Keyboard => &mut input.keyboard,
        Device::Gamepad => &mut input.gamepad,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn waiting_for(device: Device, action: Action) -> SettingsView {
        SettingsView {
            open: true,
            capturing: Some((device, action)),
        }
    }

    #[test]
    fn captured_key_is_bound_and_capture_ends() {
        let mut input = InputConfig::default();
        let mut view = waiting_for(Device::Keyboard, Action::A);
        assert!(
            !view.capture(&mut input, None, Some("South")),
            "패드 입력은 무시"
        );
        assert!(view.capture(&mut input, Some(Key::K), None));
        assert_eq!(input.keyboard.get(Action::A), "K");
        assert!(!view.is_capturing());
    }

    #[test]
    fn captured_pad_button_is_bound() {
        let mut input = InputConfig::default();
        let mut view = waiting_for(Device::Gamepad, Action::Pause);
        assert!(
            !view.capture(&mut input, Some(Key::K), None),
            "키보드 입력은 무시"
        );
        assert!(view.capture(&mut input, None, Some("North")));
        assert_eq!(input.gamepad.get(Action::Pause), "North");
    }

    #[test]
    fn escape_can_be_bound_like_any_key() {
        let mut input = InputConfig::default();
        let mut view = waiting_for(Device::Keyboard, Action::Start);
        assert!(view.capture(&mut input, Some(Key::Escape), None));
        assert_eq!(input.keyboard.get(Action::Start), "Escape");
        assert_eq!(
            input.keyboard.get(Action::Pause),
            "",
            "일시정지에서 해제된다"
        );
    }

    #[test]
    fn not_capturing_ignores_input() {
        let mut input = InputConfig::default();
        let mut view = SettingsView::default();
        assert!(!view.capture(&mut input, Some(Key::K), Some("North")));
        assert_eq!(input, InputConfig::default());
    }
}
