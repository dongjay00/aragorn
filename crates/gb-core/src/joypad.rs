//! 조이패드 (Pan Docs "Joypad Input").
//!
//! P1(0xFF00)의 비트 4가 0이면 방향키, 비트 5가 0이면 버튼 줄을 선택한다.
//! 하위 4비트는 선택된 줄에서 눌린 키가 0이다(active-low).
//! 선택된 줄의 비트가 1→0으로 바뀌면 조이패드 인터럽트를 요청한다.

pub const P1: u16 = 0xFF00;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Button {
    Right,
    Left,
    Up,
    Down,
    A,
    B,
    Select,
    Start,
}

impl Button {
    pub const ALL: [Button; 8] = [
        Button::Right,
        Button::Left,
        Button::Up,
        Button::Down,
        Button::A,
        Button::B,
        Button::Select,
        Button::Start,
    ];

    /// (방향키 줄인지, 하위 4비트 안의 비트 번호)
    fn line_bit(self) -> (bool, u8) {
        match self {
            Button::Right => (true, 0),
            Button::Left => (true, 1),
            Button::Up => (true, 2),
            Button::Down => (true, 3),
            Button::A => (false, 0),
            Button::B => (false, 1),
            Button::Select => (false, 2),
            Button::Start => (false, 3),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Joypad {
    /// P1의 선택 비트(4–5). 부트 직후에는 둘 다 1(아무 줄도 선택하지 않음).
    select: u8,
    /// 눌린 방향키 (비트 0–3 = 오른쪽, 왼쪽, 위, 아래)
    dpad: u8,
    /// 눌린 버튼 (비트 0–3 = A, B, Select, Start)
    buttons: u8,
}

impl Default for Joypad {
    fn default() -> Self {
        Self {
            select: 0x30,
            dpad: 0,
            buttons: 0,
        }
    }
}

impl Joypad {
    pub fn read(&self) -> u8 {
        0xC0 | self.select | self.lines()
    }

    /// 선택 비트를 쓴다. 선택이 바뀌어 눌린 키가 새로 보이면 `true`(조이패드 인터럽트 요청).
    pub fn write(&mut self, value: u8) -> bool {
        let before = self.lines();
        self.select = value & 0x30;
        falling(before, self.lines())
    }

    /// 키 상태를 바꾼다. 선택된 줄에서 새로 눌렸으면 `true`(조이패드 인터럽트 요청).
    pub fn set_button(&mut self, button: Button, pressed: bool) -> bool {
        let before = self.lines();
        let (dpad, bit) = button.line_bit();
        let set = if dpad {
            &mut self.dpad
        } else {
            &mut self.buttons
        };
        if pressed {
            *set |= 1 << bit;
        } else {
            *set &= !(1 << bit);
        }
        falling(before, self.lines())
    }

    /// P1 하위 4비트: 선택된 줄에서 눌린 키가 0.
    fn lines(&self) -> u8 {
        let mut pressed = 0;
        if self.select & 0x10 == 0 {
            pressed |= self.dpad;
        }
        if self.select & 0x20 == 0 {
            pressed |= self.buttons;
        }
        !pressed & 0x0F
    }
}

fn falling(before: u8, after: u8) -> bool {
    before & !after & 0x0F != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_selected_reads_all_released() {
        let p = Joypad::default();
        assert_eq!(p.read(), 0xFF);
    }

    #[test]
    fn selected_line_shows_pressed_keys_as_zero() {
        let mut p = Joypad::default();
        p.set_button(Button::Down, true);
        p.set_button(Button::Start, true);
        p.write(0x20); // 방향키 줄 선택
        assert_eq!(p.read(), 0xE7);
        p.write(0x10); // 버튼 줄 선택
        assert_eq!(p.read(), 0xD7);
    }

    #[test]
    fn released_key_reads_one_again() {
        let mut p = Joypad::default();
        p.write(0x10);
        p.set_button(Button::A, true);
        p.set_button(Button::A, false);
        assert_eq!(p.read() & 0x0F, 0x0F);
    }

    #[test]
    fn pressing_a_key_on_selected_line_requests_interrupt() {
        let mut p = Joypad::default();
        p.write(0x10);
        assert!(p.set_button(Button::B, true));
        assert!(
            !p.set_button(Button::B, true),
            "이미 눌린 키는 다시 요청하지 않는다"
        );
        assert!(!p.set_button(Button::Up, true), "선택되지 않은 줄");
    }

    #[test]
    fn selecting_a_line_with_held_key_requests_interrupt() {
        let mut p = Joypad::default();
        p.set_button(Button::Left, true);
        assert!(p.write(0x20));
    }
}
