//! gilrs 게임패드 어댑터 (스펙 §6.2). 연결된 패드를 자동으로 인식하고 설정의 매핑으로 입력을 바꾼다.

use crate::ui::input::PadInput;
use aragorn_app::input::{Action, Bindings};
use gilrs::{Axis, Button, EventType, Gilrs};

/// 왼쪽 스틱을 이만큼 넘게 기울이면 방향 버튼을 누른 것으로 본다.
pub const STICK_DEADZONE: f32 = 0.5;

/// 설정에 저장하는 패드 버튼 이름. gilrs `Button` 변형 이름과 같다.
const BUTTON_NAMES: [(Button, &str); 19] = [
    (Button::South, "South"),
    (Button::East, "East"),
    (Button::North, "North"),
    (Button::West, "West"),
    (Button::C, "C"),
    (Button::Z, "Z"),
    (Button::LeftTrigger, "LeftTrigger"),
    (Button::LeftTrigger2, "LeftTrigger2"),
    (Button::RightTrigger, "RightTrigger"),
    (Button::RightTrigger2, "RightTrigger2"),
    (Button::Select, "Select"),
    (Button::Start, "Start"),
    (Button::Mode, "Mode"),
    (Button::LeftThumb, "LeftThumb"),
    (Button::RightThumb, "RightThumb"),
    (Button::DPadUp, "DPadUp"),
    (Button::DPadDown, "DPadDown"),
    (Button::DPadLeft, "DPadLeft"),
    (Button::DPadRight, "DPadRight"),
];

pub fn button_name(button: Button) -> Option<&'static str> {
    BUTTON_NAMES
        .iter()
        .find(|(b, _)| *b == button)
        .map(|(_, n)| *n)
}

pub fn button_from_name(name: &str) -> Option<Button> {
    BUTTON_NAMES
        .iter()
        .find(|(_, n)| *n == name)
        .map(|(b, _)| *b)
}

/// 왼쪽 스틱 값(x: 오른쪽이 +, y: 위가 +)으로 눌린 방향.
pub fn stick_directions(x: f32, y: f32) -> Vec<Action> {
    [
        (x > STICK_DEADZONE, Action::Right),
        (x < -STICK_DEADZONE, Action::Left),
        (y > STICK_DEADZONE, Action::Up),
        (y < -STICK_DEADZONE, Action::Down),
    ]
    .into_iter()
    .filter_map(|(on, action)| on.then_some(action))
    .collect()
}

/// 이번 화면 갱신에서 패드가 낸 것.
#[derive(Debug, Default)]
pub struct PadPoll {
    pub input: PadInput,
    /// 이번에 새로 눌린 버튼 하나의 이름 (설정 창의 버튼 지정용)
    pub first_pressed: Option<&'static str>,
    /// 상태 표시줄에 보일 연결·해제 알림
    pub notices: Vec<String>,
}

pub struct Gamepads {
    /// 패드를 쓸 수 없는 환경이면 `None`. 키보드만으로 계속 실행한다.
    gilrs: Option<Gilrs>,
}

impl Gamepads {
    pub fn open() -> Self {
        let gilrs = match Gilrs::new() {
            Ok(g) => Some(g),
            Err(gilrs::Error::NotImplemented(g)) => {
                log::warn!("이 플랫폼에서는 게임패드를 지원하지 않습니다");
                Some(g)
            }
            Err(e) => {
                log::warn!("게임패드를 쓸 수 없습니다: {e}");
                None
            }
        };
        Self { gilrs }
    }

    /// 쌓인 이벤트를 처리하고 지금 패드 상태를 `bindings`로 바꾼다.
    pub fn poll(&mut self, bindings: &Bindings) -> PadPoll {
        let mut poll = PadPoll::default();
        let Some(gilrs) = &mut self.gilrs else {
            return poll;
        };
        while let Some(event) = gilrs.next_event() {
            let name = gilrs.gamepad(event.id).name().to_string();
            match event.event {
                EventType::ButtonPressed(button, _) => {
                    if let Some(button) = button_name(button) {
                        poll.first_pressed.get_or_insert(button);
                        poll.input.pressed.extend(bindings.actions_for(button));
                    }
                }
                EventType::Connected => poll.notices.push(format!("게임패드 연결됨: {name}")),
                EventType::Disconnected => {
                    poll.notices.push(format!("게임패드 연결 해제됨: {name}"))
                }
                _ => {}
            }
        }
        for (_, pad) in gilrs.gamepads() {
            for action in Action::ALL {
                let held =
                    button_from_name(bindings.get(action)).is_some_and(|b| pad.is_pressed(b));
                if held && !poll.input.held.contains(&action) {
                    poll.input.held.push(action);
                }
            }
            for action in stick_directions(pad.value(Axis::LeftStickX), pad.value(Axis::LeftStickY))
            {
                if !poll.input.held.contains(&action) {
                    poll.input.held.push(action);
                }
            }
        }
        poll
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for (button, name) in BUTTON_NAMES {
            assert_eq!(button_name(button), Some(name));
            assert_eq!(button_from_name(name), Some(button));
        }
        assert_eq!(button_from_name(""), None);
        assert_eq!(button_from_name("Nope"), None);
        assert_eq!(button_name(Button::Unknown), None);
    }

    #[test]
    fn default_gamepad_bindings_are_known_buttons() {
        let defaults = Bindings::gamepad_default();
        for action in Action::ALL {
            let name = defaults.get(action);
            assert!(
                name.is_empty() || button_from_name(name).is_some(),
                "{name}"
            );
        }
    }

    #[test]
    fn stick_beyond_deadzone_presses_directions() {
        assert_eq!(stick_directions(0.0, 0.0), []);
        assert_eq!(stick_directions(0.4, -0.4), []);
        assert_eq!(stick_directions(0.9, 0.0), [Action::Right]);
        assert_eq!(stick_directions(-0.7, 0.8), [Action::Left, Action::Up]);
        assert_eq!(stick_directions(0.0, -1.0), [Action::Down]);
    }
}
