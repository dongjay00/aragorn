//! 키보드 입력을 게임보이 버튼으로 바꾼다 (스펙 §6.2 기본 키). 키 설정 화면은 M7.

use eframe::egui::Key;
use gb_core::Button;

/// 기본 키 배치: 방향키, Z=B, X=A, Enter=Start, Backspace=Select.
pub const DEFAULT_KEYS: [(Key, Button); 8] = [
    (Key::ArrowRight, Button::Right),
    (Key::ArrowLeft, Button::Left),
    (Key::ArrowUp, Button::Up),
    (Key::ArrowDown, Button::Down),
    (Key::X, Button::A),
    (Key::Z, Button::B),
    (Key::Backspace, Button::Select),
    (Key::Enter, Button::Start),
];

/// 지금 눌린 키를 보고 각 버튼의 상태를 돌려준다.
pub fn button_states(key_down: impl Fn(Key) -> bool) -> [(Button, bool); 8] {
    DEFAULT_KEYS.map(|(key, button)| (button, key_down(key)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_button_has_exactly_one_default_key() {
        for button in Button::ALL {
            assert_eq!(
                DEFAULT_KEYS.iter().filter(|(_, b)| *b == button).count(),
                1,
                "{button:?}"
            );
        }
    }

    #[test]
    fn held_keys_become_pressed_buttons() {
        let states = button_states(|key| matches!(key, Key::X | Key::ArrowUp));
        let pressed: Vec<Button> = states.iter().filter(|(_, p)| *p).map(|(b, _)| *b).collect();
        assert_eq!(pressed, [Button::Up, Button::A]);
    }
}
