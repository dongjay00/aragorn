//! 바인딩으로 내보내는 값 타입. `aragorn-app`의 타입을 Kotlin·Swift에서 쓸 수 있는 모양으로 옮긴다.

use aragorn_app::{
    input::Action,
    pacing::{BaseSpeed, FastForward},
};

/// RGBA 바이트 이미지 (Android `Bitmap.Config.ARGB_8888`의 메모리 순서와 같다).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Image {
    /// 0xRRGGBBAA 픽셀을 R, G, B, A 바이트 순서로 바꾼다.
    pub fn from_pixels(width: usize, height: usize, pixels: &[u32]) -> Self {
        Self {
            width: width as u32,
            height: height as u32,
            rgba: rgba_bytes(pixels),
        }
    }
}

pub fn rgba_bytes(pixels: &[u32]) -> Vec<u8> {
    pixels.iter().flat_map(|p| p.to_be_bytes()).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum GbButton {
    Right,
    Left,
    Up,
    Down,
    A,
    B,
    Select,
    Start,
}

impl From<GbButton> for gb_core::Button {
    fn from(button: GbButton) -> Self {
        match button {
            GbButton::Right => Self::Right,
            GbButton::Left => Self::Left,
            GbButton::Up => Self::Up,
            GbButton::Down => Self::Down,
            GbButton::A => Self::A,
            GbButton::B => Self::B,
            GbButton::Select => Self::Select,
            GbButton::Start => Self::Start,
        }
    }
}

/// 배속 키를 누르지 않을 때의 기본 속도.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Speed {
    X1,
    X2,
    X4,
}

impl From<Speed> for BaseSpeed {
    fn from(speed: Speed) -> Self {
        match speed {
            Speed::X1 => Self::X1,
            Speed::X2 => Self::X2,
            Speed::X4 => Self::X4,
        }
    }
}

impl From<BaseSpeed> for Speed {
    fn from(speed: BaseSpeed) -> Self {
        match speed {
            BaseSpeed::X1 => Self::X1,
            BaseSpeed::X2 => Self::X2,
            BaseSpeed::X4 => Self::X4,
        }
    }
}

/// 배속 키를 누르고 있는 동안의 속도.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FastSpeed {
    X2,
    X4,
    Unlimited,
}

impl From<FastSpeed> for FastForward {
    fn from(speed: FastSpeed) -> Self {
        match speed {
            FastSpeed::X2 => Self::X2,
            FastSpeed::X4 => Self::X4,
            FastSpeed::Unlimited => Self::Unlimited,
        }
    }
}

impl From<FastForward> for FastSpeed {
    fn from(speed: FastForward) -> Self {
        match speed {
            FastForward::X2 => Self::X2,
            FastForward::X4 => Self::X4,
            FastForward::Unlimited => Self::Unlimited,
        }
    }
}

/// 키를 지정할 수 있는 기능. 순서는 설정 화면에 보이는 순서다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum InputAction {
    Up,
    Down,
    Left,
    Right,
    A,
    B,
    Start,
    Select,
    FastForward,
    Pause,
    Mute,
}

impl From<InputAction> for Action {
    fn from(action: InputAction) -> Self {
        match action {
            InputAction::Up => Self::Up,
            InputAction::Down => Self::Down,
            InputAction::Left => Self::Left,
            InputAction::Right => Self::Right,
            InputAction::A => Self::A,
            InputAction::B => Self::B,
            InputAction::Start => Self::Start,
            InputAction::Select => Self::Select,
            InputAction::FastForward => Self::FastForward,
            InputAction::Pause => Self::Pause,
            InputAction::Mute => Self::Mute,
        }
    }
}

impl From<Action> for InputAction {
    fn from(action: Action) -> Self {
        match action {
            Action::Up => Self::Up,
            Action::Down => Self::Down,
            Action::Left => Self::Left,
            Action::Right => Self::Right,
            Action::A => Self::A,
            Action::B => Self::B,
            Action::Start => Self::Start,
            Action::Select => Self::Select,
            Action::FastForward => Self::FastForward,
            Action::Pause => Self::Pause,
            Action::Mute => Self::Mute,
        }
    }
}

impl InputAction {
    /// 게임보이 버튼이면 그 버튼.
    pub fn button(self) -> Option<GbButton> {
        Action::from(self).button().map(|button| match button {
            gb_core::Button::Right => GbButton::Right,
            gb_core::Button::Left => GbButton::Left,
            gb_core::Button::Up => GbButton::Up,
            gb_core::Button::Down => GbButton::Down,
            gb_core::Button::A => GbButton::A,
            gb_core::Button::B => GbButton::B,
            gb_core::Button::Select => GbButton::Select,
            gb_core::Button::Start => GbButton::Start,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum InputDevice {
    Keyboard,
    Gamepad,
}

/// 설정 화면 순서대로 모든 기능.
#[uniffi::export]
pub fn all_actions() -> Vec<InputAction> {
    Action::ALL.into_iter().map(InputAction::from).collect()
}

#[uniffi::export]
pub fn action_label(action: InputAction) -> String {
    Action::from(action).label().to_string()
}

/// 기능이 게임보이 버튼이면 그 버튼.
#[uniffi::export]
pub fn action_button(action: InputAction) -> Option<GbButton> {
    action.button()
}

#[uniffi::export]
pub fn speed_label(speed: Speed) -> String {
    BaseSpeed::from(speed).label().to_string()
}

#[uniffi::export]
pub fn fast_speed_label(speed: FastSpeed) -> String {
    FastForward::from(speed).label().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixels_become_rgba_bytes() {
        assert_eq!(rgba_bytes(&[0x1122_33FF]), [0x11, 0x22, 0x33, 0xFF]);
    }

    #[test]
    fn actions_round_trip_in_settings_order() {
        let actions = all_actions();
        assert_eq!(actions.len(), Action::ALL.len());
        for (mobile, app) in actions.into_iter().zip(Action::ALL) {
            assert_eq!(Action::from(mobile), app);
            assert_eq!(InputAction::from(app), mobile);
        }
    }

    #[test]
    fn only_game_buttons_map_to_buttons() {
        assert_eq!(InputAction::A.button(), Some(GbButton::A));
        assert_eq!(InputAction::FastForward.button(), None);
    }
}
