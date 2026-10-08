//! 키보드·게임패드 매핑 설정 (스펙 §5.2, §6.2).
//!
//! 키와 패드 버튼은 이름 문자열로 저장한다. 이름을 실제 키로 바꾸는 일은 데스크톱 계층이 한다.
//! 빈 문자열은 "지정 안 함"이다.

use crate::pacing::FastForward;
use gb_core::Button;
use serde::{Deserialize, Serialize};

/// 사용자가 키를 지정할 수 있는 기능.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    Right,
    Left,
    Up,
    Down,
    A,
    B,
    Select,
    Start,
    /// 누르고 있는 동안 배속
    FastForward,
    /// 누를 때마다 일시정지 켜기/끄기
    Pause,
}

impl Action {
    pub const ALL: [Action; 10] = [
        Action::Up,
        Action::Down,
        Action::Left,
        Action::Right,
        Action::A,
        Action::B,
        Action::Start,
        Action::Select,
        Action::FastForward,
        Action::Pause,
    ];

    /// 게임보이 버튼이면 그 버튼.
    pub fn button(self) -> Option<Button> {
        Some(match self {
            Action::Right => Button::Right,
            Action::Left => Button::Left,
            Action::Up => Button::Up,
            Action::Down => Button::Down,
            Action::A => Button::A,
            Action::B => Button::B,
            Action::Select => Button::Select,
            Action::Start => Button::Start,
            Action::FastForward | Action::Pause => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Action::Right => "오른쪽",
            Action::Left => "왼쪽",
            Action::Up => "위",
            Action::Down => "아래",
            Action::A => "A",
            Action::B => "B",
            Action::Select => "Select",
            Action::Start => "Start",
            Action::FastForward => "배속 (누르는 동안)",
            Action::Pause => "일시정지",
        }
    }
}

/// 기능마다 키(또는 패드 버튼) 이름 하나.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bindings {
    pub right: String,
    pub left: String,
    pub up: String,
    pub down: String,
    pub a: String,
    pub b: String,
    pub select: String,
    pub start: String,
    pub fast_forward: String,
    pub pause: String,
}

impl Bindings {
    /// 키보드 기본값 (스펙 §6.2). 이름은 egui `Key::name()`과 같다.
    pub fn keyboard_default() -> Self {
        Self::from_names([
            "Right",
            "Left",
            "Up",
            "Down",
            "X",
            "Z",
            "Backspace",
            "Enter",
            "Tab",
            "Escape",
        ])
    }

    /// 게임패드 기본값 (스펙 §6.2). 이름은 gilrs `Button`의 변형 이름과 같다.
    pub fn gamepad_default() -> Self {
        Self::from_names([
            "DPadRight",
            "DPadLeft",
            "DPadUp",
            "DPadDown",
            "South",
            "East",
            "Select",
            "Start",
            "RightTrigger",
            "",
        ])
    }

    /// 순서: 오른쪽, 왼쪽, 위, 아래, A, B, Select, Start, 배속, 일시정지.
    fn from_names(names: [&str; 10]) -> Self {
        let [
            right,
            left,
            up,
            down,
            a,
            b,
            select,
            start,
            fast_forward,
            pause,
        ] = names.map(str::to_string);
        Self {
            right,
            left,
            up,
            down,
            a,
            b,
            select,
            start,
            fast_forward,
            pause,
        }
    }

    pub fn get(&self, action: Action) -> &str {
        match action {
            Action::Right => &self.right,
            Action::Left => &self.left,
            Action::Up => &self.up,
            Action::Down => &self.down,
            Action::A => &self.a,
            Action::B => &self.b,
            Action::Select => &self.select,
            Action::Start => &self.start,
            Action::FastForward => &self.fast_forward,
            Action::Pause => &self.pause,
        }
    }

    fn slot(&mut self, action: Action) -> &mut String {
        match action {
            Action::Right => &mut self.right,
            Action::Left => &mut self.left,
            Action::Up => &mut self.up,
            Action::Down => &mut self.down,
            Action::A => &mut self.a,
            Action::B => &mut self.b,
            Action::Select => &mut self.select,
            Action::Start => &mut self.start,
            Action::FastForward => &mut self.fast_forward,
            Action::Pause => &mut self.pause,
        }
    }

    /// `action`에 `name`을 지정한다. 같은 이름을 쓰던 다른 기능은 지정을 해제한다.
    pub fn bind(&mut self, action: Action, name: &str) {
        for other in Action::ALL {
            if other != action && self.get(other) == name {
                self.slot(other).clear();
            }
        }
        *self.slot(action) = name.to_string();
    }

    /// 이름이 `name`인 키에 지정된 기능들.
    pub fn actions_for<'a>(&'a self, name: &'a str) -> impl Iterator<Item = Action> + 'a {
        Action::ALL
            .into_iter()
            .filter(move |&a| !name.is_empty() && self.get(a) == name)
    }
}

/// 설정 파일의 `[input]` 섹션. 읽기는 아래 `Deserialize` 구현을 따른다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InputConfig {
    pub keyboard: Bindings,
    pub gamepad: Bindings,
    pub fast_forward: FastForward,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            keyboard: Bindings::keyboard_default(),
            gamepad: Bindings::gamepad_default(),
            fast_forward: FastForward::default(),
        }
    }
}

/// 항목이 빠진 매핑은 그 항목만 기본값으로 채운다. 키보드와 패드의 기본값이 달라서
/// `#[serde(default)]` 대신 직접 구현한다.
mod partial {
    use super::Bindings;
    use serde::{Deserialize, Deserializer, de::IgnoredAny};

    /// 타입이 틀린 값(문자열 자리에 숫자 등)은 설정 전체를 버리지 않고 "없음"으로 본다.
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Lenient<T> {
        Value(T),
        Other(IgnoredAny),
    }

    fn lenient<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
        d: D,
    ) -> Result<Option<T>, D::Error> {
        Ok(match Lenient::deserialize(d)? {
            Lenient::Value(v) => Some(v),
            Lenient::Other(_) => None,
        })
    }

    #[derive(Deserialize, Default)]
    #[serde(default)]
    pub struct PartialBindings {
        #[serde(deserialize_with = "lenient")]
        right: Option<String>,
        #[serde(deserialize_with = "lenient")]
        left: Option<String>,
        #[serde(deserialize_with = "lenient")]
        up: Option<String>,
        #[serde(deserialize_with = "lenient")]
        down: Option<String>,
        #[serde(deserialize_with = "lenient")]
        a: Option<String>,
        #[serde(deserialize_with = "lenient")]
        b: Option<String>,
        #[serde(deserialize_with = "lenient")]
        select: Option<String>,
        #[serde(deserialize_with = "lenient")]
        start: Option<String>,
        #[serde(deserialize_with = "lenient")]
        fast_forward: Option<String>,
        #[serde(deserialize_with = "lenient")]
        pause: Option<String>,
    }

    impl PartialBindings {
        pub fn fill(self, mut base: Bindings) -> Bindings {
            let fields = [
                (self.right, &mut base.right),
                (self.left, &mut base.left),
                (self.up, &mut base.up),
                (self.down, &mut base.down),
                (self.a, &mut base.a),
                (self.b, &mut base.b),
                (self.select, &mut base.select),
                (self.start, &mut base.start),
                (self.fast_forward, &mut base.fast_forward),
                (self.pause, &mut base.pause),
            ];
            for (value, slot) in fields {
                if let Some(value) = value {
                    *slot = value;
                }
            }
            base
        }
    }

    #[derive(Deserialize, Default)]
    #[serde(default)]
    pub struct PartialInput {
        #[serde(deserialize_with = "lenient")]
        pub keyboard: Option<PartialBindings>,
        #[serde(deserialize_with = "lenient")]
        pub gamepad: Option<PartialBindings>,
        /// 모르는 값이면 설정 전체를 버리지 않고 기본 배속을 쓴다.
        #[serde(deserialize_with = "lenient")]
        pub fast_forward: Option<String>,
    }
}

impl<'de> Deserialize<'de> for InputConfig {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let p = partial::PartialInput::deserialize(d)?;
        Ok(Self {
            keyboard: p
                .keyboard
                .unwrap_or_default()
                .fill(Bindings::keyboard_default()),
            gamepad: p
                .gamepad
                .unwrap_or_default()
                .fill(Bindings::gamepad_default()),
            fast_forward: p
                .fast_forward
                .as_deref()
                .and_then(FastForward::from_name)
                .unwrap_or_default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_button_action_maps_to_its_button() {
        let buttons: Vec<Button> = Action::ALL.iter().filter_map(|a| a.button()).collect();
        assert_eq!(buttons.len(), 8);
        for b in Button::ALL {
            assert!(buttons.contains(&b), "{b:?}");
        }
    }

    #[test]
    fn keyboard_defaults_follow_spec() {
        let k = Bindings::keyboard_default();
        assert_eq!(k.get(Action::A), "X");
        assert_eq!(k.get(Action::B), "Z");
        assert_eq!(k.get(Action::Start), "Enter");
        assert_eq!(k.get(Action::Select), "Backspace");
        assert_eq!(k.get(Action::FastForward), "Tab");
        assert_eq!(k.get(Action::Pause), "Escape");
    }

    #[test]
    fn default_bindings_have_no_duplicates() {
        for bindings in [Bindings::keyboard_default(), Bindings::gamepad_default()] {
            for a in Action::ALL {
                let name = bindings.get(a);
                if !name.is_empty() {
                    assert_eq!(bindings.actions_for(name).count(), 1, "{name}");
                }
            }
        }
    }

    #[test]
    fn binding_a_used_key_unbinds_the_old_action() {
        let mut k = Bindings::keyboard_default();
        k.bind(Action::A, "Z");
        assert_eq!(k.get(Action::A), "Z");
        assert_eq!(k.get(Action::B), "", "B에 쓰던 Z는 해제된다");
        assert_eq!(k.actions_for("Z").collect::<Vec<_>>(), [Action::A]);
    }

    #[test]
    fn rebinding_the_same_key_keeps_it() {
        let mut k = Bindings::keyboard_default();
        k.bind(Action::A, "X");
        assert_eq!(k, Bindings::keyboard_default());
    }

    #[test]
    fn empty_name_matches_nothing() {
        let g = Bindings::gamepad_default();
        assert_eq!(g.actions_for("").count(), 0);
    }

    #[test]
    fn missing_entries_fall_back_to_their_own_defaults() {
        let c: InputConfig = serde_json::from_str(
            r#"{"keyboard":{"a":"K"},"gamepad":{"pause":"Mode"},"fast_forward":"x2"}"#,
        )
        .unwrap();
        let mut keyboard = Bindings::keyboard_default();
        keyboard.a = "K".into();
        let mut gamepad = Bindings::gamepad_default();
        gamepad.pause = "Mode".into();
        assert_eq!(c.keyboard, keyboard);
        assert_eq!(c.gamepad, gamepad);
        assert_eq!(c.fast_forward, FastForward::X2);
    }

    #[test]
    fn unknown_fast_forward_falls_back_to_default() {
        let c: InputConfig = serde_json::from_str(r#"{"fast_forward":"x8"}"#).unwrap();
        assert_eq!(c, InputConfig::default());
    }

    #[test]
    fn wrong_value_types_fall_back_per_item() {
        let c: InputConfig =
            serde_json::from_str(r#"{"keyboard":{"a":1,"b":"K"},"gamepad":5,"fast_forward":4}"#)
                .unwrap();
        let mut keyboard = Bindings::keyboard_default();
        keyboard.b = "K".into();
        assert_eq!(c.keyboard, keyboard);
        assert_eq!(c.gamepad, Bindings::gamepad_default());
        assert_eq!(c.fast_forward, FastForward::default());
    }

    #[test]
    fn round_trips_through_serde() {
        let mut c = InputConfig::default();
        c.keyboard.bind(Action::Pause, "P");
        c.fast_forward = FastForward::Unlimited;
        let json = serde_json::to_string(&c).unwrap();
        assert_eq!(serde_json::from_str::<InputConfig>(&json).unwrap(), c);
    }
}
