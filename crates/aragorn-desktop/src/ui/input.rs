//! 설정의 키 이름을 egui 키로 바꾸고, 키보드와 게임패드 입력을 합친다 (스펙 §6.2).

use aragorn_app::input::{Action, Bindings};
use eframe::egui::{Event, Key};
use gb_core::Button;
use std::collections::HashSet;

/// 설정의 키보드 매핑을 egui 키로 바꾼 것.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keymap {
    keys: [(Action, Option<Key>); 11],
}

impl Keymap {
    /// 빈 이름은 지정 안 함이다. 해석할 수 없는 이름은 그 기능의 기본 키를 쓴다.
    pub fn new(bindings: &Bindings) -> Self {
        let defaults = Bindings::keyboard_default();
        Self {
            keys: Action::ALL.map(|action| {
                let name = bindings.get(action);
                let key = if name.is_empty() {
                    None
                } else {
                    Key::from_name(name).or_else(|| Key::from_name(defaults.get(action)))
                };
                (action, key)
            }),
        }
    }

    pub fn key(&self, action: Action) -> Option<Key> {
        self.keys
            .iter()
            .find(|(a, _)| *a == action)
            .and_then(|(_, k)| *k)
    }
}

/// 글자 입력칸에 입력하는 동안 눌린 키를 뗄 때까지 게임에 넘기지 않는다. 입력칸에서 Enter를 누르면
/// 포커스가 풀린 뒤에도 키가 눌려 있어서 게임의 Start로 새는 것을 막는다.
#[derive(Debug, Default)]
pub struct TypingGuard {
    blocked: HashSet<Key>,
}

impl TypingGuard {
    /// 화면 갱신마다 부른다. `keys_down`은 지금 눌린 키 전부다.
    pub fn update(&mut self, typing: bool, keys_down: &HashSet<Key>) {
        if typing {
            self.blocked.extend(keys_down);
        } else {
            self.blocked.retain(|k| keys_down.contains(k));
        }
    }

    /// 이 키를 게임에 넘겨도 되는지.
    pub fn allows(&self, typing: bool, key: Key) -> bool {
        !typing && !self.blocked.contains(&key)
    }
}

/// 이번 화면 갱신에서 게임패드가 낸 입력.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PadInput {
    /// 지금 눌려 있는 기능
    pub held: Vec<Action>,
    /// 이번에 새로 눌린 기능
    pub pressed: Vec<Action>,
}

/// 키보드와 게임패드를 합친 이번 화면 갱신의 입력.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputFrame {
    pub buttons: [(Button, bool); 8],
    pub fast_forward: bool,
    pub pause_pressed: bool,
    pub mute_pressed: bool,
}

/// 이번 화면 갱신에 `key`가 새로 눌렸는지. 누르고 있을 때 OS가 보내는 반복 입력은 세지 않는다.
pub fn pressed_once(events: &[Event], key: Key) -> bool {
    events
        .iter()
        .any(|e| matches!(e, Event::Key { key: k, pressed: true, repeat: false, .. } if *k == key))
}

/// 키보드나 게임패드 중 하나라도 눌려 있으면 눌린 것으로 본다.
pub fn gather(
    keymap: &Keymap,
    key_down: impl Fn(Key) -> bool,
    key_pressed: impl Fn(Key) -> bool,
    pad: &PadInput,
) -> InputFrame {
    let held =
        |action: Action| keymap.key(action).is_some_and(&key_down) || pad.held.contains(&action);
    InputFrame {
        buttons: Button::ALL.map(|button| {
            let action = Action::ALL
                .into_iter()
                .find(|a| a.button() == Some(button))
                .expect("모든 버튼에 기능이 있다");
            (button, held(action))
        }),
        fast_forward: held(Action::FastForward),
        pause_pressed: keymap.key(Action::Pause).is_some_and(&key_pressed)
            || pad.pressed.contains(&Action::Pause),
        mute_pressed: keymap.key(Action::Mute).is_some_and(&key_pressed)
            || pad.pressed.contains(&Action::Mute),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui;

    fn pressed(frame: &InputFrame) -> Vec<Button> {
        frame
            .buttons
            .iter()
            .filter(|(_, p)| *p)
            .map(|(b, _)| *b)
            .collect()
    }

    #[test]
    fn default_keyboard_names_are_egui_keys() {
        let keymap = Keymap::new(&Bindings::keyboard_default());
        for action in Action::ALL {
            assert!(keymap.key(action).is_some(), "{action:?}");
        }
        assert_eq!(keymap.key(Action::Right), Some(Key::ArrowRight));
        assert_eq!(keymap.key(Action::FastForward), Some(Key::Tab));
        assert_eq!(keymap.key(Action::Pause), Some(Key::Escape));
    }

    #[test]
    fn unknown_name_falls_back_to_default_and_empty_unbinds() {
        let mut bindings = Bindings::keyboard_default();
        bindings.a = "NoSuchKey".into();
        bindings.bind(Action::B, "");
        let keymap = Keymap::new(&bindings);
        assert_eq!(keymap.key(Action::A), Some(Key::X));
        assert_eq!(keymap.key(Action::B), None);
    }

    #[test]
    fn held_keys_become_pressed_buttons() {
        let keymap = Keymap::new(&Bindings::keyboard_default());
        let frame = gather(
            &keymap,
            |key| matches!(key, Key::X | Key::ArrowUp),
            |_| false,
            &PadInput::default(),
        );
        assert_eq!(pressed(&frame), [Button::Up, Button::A]);
        assert!(!frame.fast_forward && !frame.pause_pressed);
    }

    #[test]
    fn keyboard_and_gamepad_are_merged() {
        let keymap = Keymap::new(&Bindings::keyboard_default());
        let pad = PadInput {
            held: vec![Action::B, Action::FastForward],
            pressed: vec![Action::Pause],
        };
        let frame = gather(&keymap, |key| key == Key::Enter, |_| false, &pad);
        assert_eq!(pressed(&frame), [Button::B, Button::Start]);
        assert!(frame.fast_forward);
        assert!(frame.pause_pressed);
    }

    fn key_event(key: Key, repeat: bool) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn key_repeat_is_not_a_new_press() {
        assert!(pressed_once(&[key_event(Key::Escape, false)], Key::Escape));
        assert!(!pressed_once(&[key_event(Key::Escape, true)], Key::Escape));
        assert!(!pressed_once(&[key_event(Key::Tab, false)], Key::Escape));
        let released = egui::Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        assert!(!pressed_once(&[released], Key::Escape));
    }

    #[test]
    fn keys_held_while_typing_stay_blocked_until_released() {
        let mut guard = TypingGuard::default();
        let down = |keys: &[Key]| {
            keys.iter()
                .copied()
                .collect::<std::collections::HashSet<_>>()
        };
        // 입력칸에서 Enter를 누른다
        guard.update(true, &down(&[Key::Enter]));
        assert!(!guard.allows(true, Key::X), "입력 중에는 모두 막는다");
        // 입력칸이 포커스를 잃었지만 Enter는 아직 눌려 있다
        guard.update(false, &down(&[Key::Enter, Key::X]));
        assert!(!guard.allows(false, Key::Enter));
        assert!(
            guard.allows(false, Key::X),
            "입력이 끝난 뒤 새로 누른 키는 통과"
        );
        // Enter를 뗐다가 다시 누르면 통과
        guard.update(false, &down(&[]));
        guard.update(false, &down(&[Key::Enter]));
        assert!(guard.allows(false, Key::Enter));
    }

    #[test]
    fn pause_toggles_on_press_not_hold() {
        let keymap = Keymap::new(&Bindings::keyboard_default());
        let frame = gather(
            &keymap,
            |k| k == Key::Escape,
            |_| false,
            &PadInput::default(),
        );
        assert!(!frame.pause_pressed);
        let frame = gather(
            &keymap,
            |_| false,
            |k| k == Key::Escape,
            &PadInput::default(),
        );
        assert!(frame.pause_pressed);
    }

    #[test]
    fn mute_toggles_on_press_from_keyboard_or_pad() {
        let keymap = Keymap::new(&Bindings::keyboard_default());
        let frame = gather(&keymap, |k| k == Key::M, |_| false, &PadInput::default());
        assert!(!frame.mute_pressed, "누르고 있는 것만으로는 바꾸지 않는다");
        let frame = gather(&keymap, |_| false, |k| k == Key::M, &PadInput::default());
        assert!(frame.mute_pressed);
        let pad = PadInput {
            held: vec![Action::Mute],
            pressed: vec![Action::Mute],
        };
        assert!(gather(&keymap, |_| false, |_| false, &pad).mute_pressed);
    }
}
