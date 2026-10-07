# 마일스톤 4: 키 입력과 배터리 세이브 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 키보드로 게임을 조작하고, 배터리 세이브(`.sav`)를 ROM 옆에 저장하고 불러와서 포켓몬 레드/블루/피카츄를 실제로 플레이하고 이어서 할 수 있게 한다.

**Architecture:**
- `gb-core`:
  - `Joypad`가 P1(0xFF00)과 조이패드 인터럽트를 맡는다.
  - `Cartridge`가 배터리 여부, 외부 RAM 쓰기 표시(dirty), RAM 활성 상태를 노출한다.
  - `GameBoy`에 `set_button`, `battery_ram`, `load_battery_ram`, `battery_dirty`, `cartridge_ram_enabled`를 추가한다(스펙 §4.1).
- `aragorn-app`: 포트 `SaveStore`(ROM 하나의 세이브)를 둔다. `Session`은 다음 경우에 저장한다.
  - 프레임마다 확인해서, 게임이 외부 RAM을 끄면 바로 저장한다.
  - RAM을 켠 채 쓰기가 60프레임 멈추면 저장한다.
  - `flush()`를 부르면 저장한다(종료, ROM 교체, 업데이트 적용 전).
  - 실패하면 오류를 한 번만 알리고 60프레임마다 다시 시도한다.
- `aragorn-desktop`: ROM 옆 `.sav`에 쓰는 `FsSaveStore`(이전 세이브는 `.sav.bak`, 원자적 교체)와 기본 키 배치(스펙 §6.2)를 둔다.

**Tech Stack:** Rust 1.99 (edition 2024), eframe/egui 0.36.2, 새 의존성 없음

**Spec:** `docs/superpowers/specs/2026-10-07-gameboy-emulator-design.md` (§3.3 포트, §4.1 공개 API, §4.8 세이브 형식, §5.1 세션, §6.2 입력, §9의 마일스톤 4)

**작업 브랜치:** `feat/m4-input-save`

## Global Constraints

- `gb-core`에는 의존성을 추가하지 않는다. 게임 데이터 때문에 패닉하지 않는다.
- 이름은 Pan Docs 용어를 따른다. 사용자에게 보이는 문구, 문서, 커밋 메시지는 한국어로 쓴다.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `ARAGORN_REQUIRE_TEST_ROMS=1 cargo test --workspace --no-fail-fast`가 모든 커밋에서 통과해야 한다. 로컬과 CI 모두 Rust 1.99 stable이다.
- 커밋 메시지 끝에는 `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`를 붙인다.
- 릴리스(v0.3.0)는 병합 뒤 사용자 확인을 받고 한다.

## 계획 전 검증 (스파이크)

이 계획의 코드는 별도 worktree에 먼저 적용해서 확인했다.
- 단위 테스트는 core 155, app 62, desktop 39다.
- 인수 테스트(acid2 1, Blargg 15, mooneye 77)는 그대로 통과한다. clippy도 깨끗하다.
- 실제 파일 시스템을 쓰는 종단 테스트(`game_save_is_written_next_to_rom`)도 확인했다. ROM을 열고 한 프레임을 돌리면 `game.sav`가 생기고, 다시 열면 그 값을 읽는다.

## 스펙과 다른 결정 (리뷰어 확인용)

1. **세이브 위치는 ROM 옆 `<ROM 이름>.sav`다.** 사용자가 결정했다. BGB·mGBA·VBA-M과 같아서 기존 세이브를 그대로 쓸 수 있다.
   - 데이터 디렉터리를 고르는 설정(스펙 §5.2)은 설정 화면과 함께 M7에서 한다.
   - ROM 폴더에 쓸 수 없으면 오류를 상태 표시줄에 알린다.
2. **`SaveStore` 포트는 ROM 하나에 묶인 객체다.** 스펙 §3.3은 `rom_id`를 인자로 받는 형태다. 앱 계층이 경로나 ROM 식별 규칙을 몰라도 되도록, 데스크톱이 ROM마다 저장소를 만들어 세션에 넘긴다. 스테이트 슬롯 메서드는 M7에서 같은 trait에 추가한다.
3. **저장 시점의 보완.** 스펙의 "외부 RAM을 비활성화한 직후" 외에, RAM을 켠 채 두는 게임을 위해 "마지막 쓰기 뒤 60프레임(약 1초)"에도 저장한다.
4. **RTC 블록(48바이트)과 `now_unix` 보정은 M6에서 한다.** 지금은 `.sav`의 앞쪽 RAM 부분만 읽는다. RTC 블록이 붙은 다른 에뮬레이터의 세이브도 RAM은 읽는다.
5. **게임패드와 키 재설정은 스펙 마일스톤 7 그대로다.** M4는 기본 키보드 배치만 한다.

## Review Focus

1. **배터리가 없거나 RAM이 없는 카트리지**: 세이브 파일을 읽거나 쓰지 않아야 한다. → Task 3 `carts_without_battery_never_touch_the_store`
2. **크기가 다른 `.sav`(RTC 블록이 붙음, 잘림)**: 겹치는 부분만 싣고 패닉하지 않아야 한다. → Task 2 `loading_battery_ram_copies_overlapping_bytes`
3. **세이브 폴더에 쓸 수 없는 경우**: 오류를 한 번 알리고 게임은 계속 돌아야 하며 나중에 다시 시도해야 한다. → Task 3 `failed_save_is_reported_once_and_retried`, Task 4 `unwritable_location_reports_error`
4. **저장 직후 앱이 꺼지는 경우**: `.sav`가 깨지지 않아야 하고, 이전 세이브가 `.sav.bak`으로 남아야 한다. → Task 4 `overwriting_keeps_previous_save_as_backup`
5. **게임이 RAM을 켠 채 두는 경우**: 1초 안에 저장되고, 종료할 때는 반드시 저장되어야 한다. → Task 3 `saves_after_idle_frames_when_game_keeps_ram_enabled`, `flush_saves_pending_writes`

---

### Task 1: 조이패드

**Files:**
- Create: `crates/gb-core/src/joypad.rs`
- Modify: `crates/gb-core/src/lib.rs`, `crates/gb-core/src/bus.rs`, `crates/gb-core/src/gameboy.rs`

**Interfaces:**
- Produces:
  - `gb_core::Button { Right, Left, Up, Down, A, B, Select, Start }`(`Button::ALL`)
  - `gb_core::joypad::{P1, Joypad}`
  - `Bus::set_button(Button, bool)`
  - `GameBoy::set_button(&mut self, button: Button, pressed: bool)`

- [ ] **Step 1: 브랜치를 만들고 실패하는 테스트를 작성한다**

```bash
git checkout main && git pull --ff-only && git checkout -b feat/m4-input-save
```

`lib.rs`의 `mod gameboy;` 아래에 `pub mod joypad;`를 추가하고, `pub use gameboy::{DebugView, GameBoy};` 아래에 `pub use joypad::Button;`을 추가한다.

`crates/gb-core/src/joypad.rs`:

```rust
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
        todo!()
    }

    /// 선택 비트를 쓴다. 선택이 바뀌어 눌린 키가 새로 보이면 `true`(조이패드 인터럽트 요청).
    pub fn write(&mut self, value: u8) -> bool {
        todo!()
    }

    /// 키 상태를 바꾼다. 선택된 줄에서 새로 눌렸으면 `true`(조이패드 인터럽트 요청).
    pub fn set_button(&mut self, button: Button, pressed: bool) -> bool {
        todo!()
    }

    /// P1 하위 4비트: 선택된 줄에서 눌린 키가 0.
    fn lines(&self) -> u8 {
        todo!()
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
```

`bus.rs` tests 모듈 끝에 추가한다:

```rust
    #[test]
    fn pressed_button_requests_joypad_interrupt() {
        let mut b = bus();
        b.write(IF_ADDR, 0);
        b.write(joypad::P1, 0x10);
        b.set_button(Button::A, true);
        assert_eq!(b.read(IF_ADDR) & INT_JOYPAD, INT_JOYPAD);
        assert_eq!(b.read(joypad::P1) & 0x0F, 0x0E);
    }
```

`gameboy.rs` tests 모듈 끝에 추가한다:

```rust
    #[test]
    fn pressed_button_is_visible_through_p1() {
        // LD A,0x10 ; LDH (0x00),A (버튼 줄 선택) ; JR -2
        let mut gb = gb_with_program(&[0x3E, 0x10, 0xE0, 0x00, 0x18, 0xFE]);
        gb.run_frame();
        gb.set_button(Button::Start, true);
        assert_eq!(gb.debug().peek(0xFF00) & 0x0F, 0x07);
        assert_eq!(
            gb.debug().peek(0xFF0F) & 0x10,
            0x10,
            "조이패드 인터럽트 요청"
        );
    }
```

- [ ] **Step 2: 테스트가 실패하는지 확인한다**

Run: `cargo test -p gb-core --lib 2>&1 | grep -E "^error" | sort | uniq -c | head -5`
Expected: FAIL(컴파일 오류). `Bus::set_button`과 `GameBoy::set_button`이 아직 없다.

- [ ] **Step 3: 구현한다**

`joypad.rs`에서 `#[cfg(test)]` 앞까지를 아래로 교체한다:

```rust
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
```

`bus.rs`:
- `use crate::cpu::{CpuBus, IE_ADDR, IF_ADDR};` 아래에 `use crate::joypad::{self, Button, Joypad};`를 추가한다.
- `const JOYP: u16 = 0xFF00;` 줄을 지운다.
- `Bus` 구조체의 `serial: Serial,` 아래에 `joypad: Joypad,`를 추가한다.
- `Bus::new`의 `serial: Serial::default(),` 아래에 `joypad: Joypad::default(),`를 추가한다.

`read_io`의 아래 두 줄을:

```rust
            // 버튼 입력은 M4에서 구현한다. 지금은 아무 버튼도 눌리지 않은 상태다.
            JOYP => 0xC0 | (self.io[0] & 0x30) | 0x0F,
```

아래로 바꾼다:

```rust
            joypad::P1 => self.joypad.read(),
```

`write_io`의 `serial::SB | serial::SC => {` arm 위에 추가한다:

```rust
            joypad::P1 => {
                let irq = self.joypad.write(value);
                self.request(INT_JOYPAD, irq);
            }
```

`pub fn cartridge(&self)` 아래에 추가한다:

```rust

    pub fn set_button(&mut self, button: Button, pressed: bool) {
        let irq = self.joypad.set_button(button, pressed);
        self.request(INT_JOYPAD, irq);
    }
```

`bus.rs` tests의 `joypad_reports_no_buttons`에서 `JOYP`를 `joypad::P1`으로 바꾼다(두 곳).

`gameboy.rs`:
- `use crate::cpu::{Cpu, IllegalOpcode, Registers};` 아래에 `use crate::joypad::Button;`을 추가한다.
- `pub fn debug(&self)` 위에 추가한다:

```rust
    /// 키를 누르거나 뗀다. 선택된 줄에서 새로 눌리면 조이패드 인터럽트를 요청한다.
    pub fn set_button(&mut self, button: Button, pressed: bool) {
        self.bus.set_button(button, pressed);
    }
```

- [ ] **Step 4: 테스트가 통과하는지 확인한다**

Run: `cargo fmt --all && cargo test -p gb-core --lib 2>&1 | grep -E "FAILED|test result" && cargo clippy -p gb-core --all-targets -- -D warnings`
Expected: `148 passed`(141 + 조이패드 5 + 버스 1 + GameBoy 1). clippy 출력 없음.

- [ ] **Step 5: 커밋한다**

```bash
git add crates/gb-core
git commit -m "feat(core): 조이패드 입력과 조이패드 인터럽트

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: 배터리 RAM API

**Files:**
- Modify: `crates/gb-core/src/cartridge.rs`, `crates/gb-core/src/bus.rs`, `crates/gb-core/src/gameboy.rs`

**Interfaces:**
- Produces:
  - `Cartridge::{battery_ram() -> Option<&[u8]>, load_battery_ram(&[u8]), take_ram_dirty() -> bool, ram_enabled() -> bool}`
  - `Bus::cartridge_mut()`
  - `GameBoy` 메서드:
    - `battery_ram() -> Option<Vec<u8>>`
    - `load_battery_ram(&[u8], now_unix: u64)`
    - `battery_dirty() -> bool`: 읽으면 초기화된다.
    - `cartridge_ram_enabled() -> bool`
  - 배터리 타입: 0x03, 0x09, 0x0F, 0x10, 0x13, 0x1B, 0x1E. RAM이 있을 때만 `battery_ram`이 `Some`이다.

- [ ] **Step 1: 실패하는 테스트를 작성한다**

`cartridge.rs` tests 모듈 끝에 추가한다:

```rust
    #[test]
    fn only_battery_types_expose_battery_ram() {
        assert!(
            Cartridge::new(banked_rom(0x03, 0x01, 0x02))
                .unwrap()
                .battery_ram()
                .is_some()
        );
        assert!(
            Cartridge::new(banked_rom(0x13, 0x01, 0x03))
                .unwrap()
                .battery_ram()
                .is_some()
        );
        assert!(
            Cartridge::new(banked_rom(0x1B, 0x01, 0x03))
                .unwrap()
                .battery_ram()
                .is_some()
        );
        assert!(
            Cartridge::new(banked_rom(0x02, 0x01, 0x02))
                .unwrap()
                .battery_ram()
                .is_none()
        );
        assert!(
            Cartridge::new(banked_rom(0x0F, 0x01, 0x00))
                .unwrap()
                .battery_ram()
                .is_none()
        );
    }

    #[test]
    fn ram_writes_set_dirty_flag_until_taken() {
        let mut cart = Cartridge::new(banked_rom(0x03, 0x01, 0x02)).unwrap();
        cart.write_ram(0xA000, 0x01);
        assert!(!cart.take_ram_dirty(), "꺼진 RAM에 쓴 것은 무시된다");
        cart.write_rom(0x0000, 0x0A);
        cart.write_ram(0xA000, 0x01);
        assert!(cart.take_ram_dirty());
        assert!(!cart.take_ram_dirty());
    }

    #[test]
    fn loading_battery_ram_copies_overlapping_bytes() {
        let mut cart = Cartridge::new(banked_rom(0x03, 0x01, 0x02)).unwrap();
        let mut save = vec![0x5A; 0x2000 + 48];
        save[0x1FFF] = 0x77;
        cart.load_battery_ram(&save);
        assert_eq!(cart.battery_ram().unwrap().len(), 0x2000);
        assert_eq!(cart.battery_ram().unwrap()[0x1FFF], 0x77);
        cart.load_battery_ram(&[1, 2]);
        assert_eq!(&cart.battery_ram().unwrap()[..3], &[1, 2, 0x5A]);
    }

    #[test]
    fn rom_only_ram_counts_as_enabled() {
        assert!(
            Cartridge::new(banked_rom(0x09, 0x00, 0x02))
                .unwrap()
                .ram_enabled()
        );
        assert!(
            !Cartridge::new(banked_rom(0x03, 0x01, 0x02))
                .unwrap()
                .ram_enabled()
        );
    }
```

`gameboy.rs` tests 모듈의 `pressed_button_is_visible_through_p1` 위에 추가한다:

```rust
    /// MBC1+RAM+BATTERY(0x03), 8KB RAM. 0x0100: RAM 켜기 → 0xA000에 0x42 쓰기 → RAM 끄기 → 제자리 루프.
    fn saving_rom() -> Vec<u8> {
        let mut rom = test_rom(0x03, 0x00, 0x00);
        rom[0x0149] = 0x02;
        let program = [
            0x3E, 0x0A, 0xEA, 0x00, 0x00, // LD A,0x0A ; LD (0x0000),A
            0x3E, 0x42, 0xEA, 0x00, 0xA0, // LD A,0x42 ; LD (0xA000),A
            0xAF, 0xEA, 0x00, 0x00, // XOR A ; LD (0x0000),A
            0x18, 0xFE, // JR -2
        ];
        rom[0x0100..0x0100 + program.len()].copy_from_slice(&program);
        rom
    }

    #[test]
    fn battery_ram_is_exposed_only_for_battery_carts() {
        let gb = GameBoy::new(saving_rom(), Model::Dmg).unwrap();
        assert_eq!(gb.battery_ram().map(|r| r.len()), Some(0x2000));
        let gb = gb_with_program(&[0x00]);
        assert_eq!(gb.battery_ram(), None);
    }

    #[test]
    fn game_writes_mark_battery_dirty_and_disable_ram() {
        let mut gb = GameBoy::new(saving_rom(), Model::Dmg).unwrap();
        assert!(!gb.battery_dirty());
        gb.run_frame();
        assert!(gb.battery_dirty());
        assert!(!gb.battery_dirty(), "읽으면 초기화");
        assert!(!gb.cartridge_ram_enabled());
        assert_eq!(gb.battery_ram().unwrap()[0], 0x42);
    }

    #[test]
    fn loaded_battery_ram_is_visible_to_the_game() {
        let mut gb = GameBoy::new(saving_rom(), Model::Dmg).unwrap();
        gb.load_battery_ram(&[0x11, 0x22], 0);
        let ram = gb.battery_ram().unwrap();
        assert_eq!((ram[0], ram[1], ram[2]), (0x11, 0x22, 0x00));
        assert!(!gb.battery_dirty(), "불러오기는 게임의 쓰기가 아니다");
    }
```

- [ ] **Step 2: 테스트가 실패하는지 확인한다**

Run: `cargo test -p gb-core --lib 2>&1 | grep -E "^error" | sort | uniq -c | head -5`
Expected: FAIL(컴파일 오류). `battery_ram`, `take_ram_dirty`, `battery_dirty` 등이 아직 없다.

- [ ] **Step 3: 구현한다**

`cartridge.rs`의 `Cartridge` 구조체에서 `mbc: Mbc,` 아래에 필드를 추가한다:

```rust
    /// 배터리가 있어 외부 RAM을 세이브 파일로 남겨야 하는 카트리지인지.
    battery: bool,
    /// 마지막으로 확인한 뒤 외부 RAM에 쓰기가 있었는지.
    ram_dirty: bool,
```

`Cartridge::new`의 `let ram = vec![0; ram_size(header.ram_size_code)];` 아래에 추가한다:

```rust
        let battery = matches!(header.cart_type, 0x03 | 0x09 | 0x0F | 0x10 | 0x13 | 0x1B | 0x1E);
```

같은 함수의 `Ok(Cartridge { ... mbc, })` 필드 목록에서 `mbc,` 다음에 `battery,`와 `ram_dirty: false,`를 추가한다.

`write_ram`의 `self.ram[i] = value;` 다음 줄에 `self.ram_dirty = true;`를 추가하고, `write_ram` 함수 아래에 메서드를 추가한다:

```rust
    /// 세이브 파일로 남길 외부 RAM. 배터리가 없거나 RAM이 없는 카트리지는 `None`.
    pub fn battery_ram(&self) -> Option<&[u8]> {
        (self.battery && !self.ram.is_empty()).then_some(&self.ram[..])
    }

    /// 세이브 파일 내용을 외부 RAM에 싣는다. 길이가 다르면 앞쪽만 겹치는 만큼 복사한다
    /// (RTC 블록이 붙은 .sav도 RAM 부분만 읽는다. RTC는 M6).
    pub fn load_battery_ram(&mut self, data: &[u8]) {
        let len = data.len().min(self.ram.len());
        self.ram[..len].copy_from_slice(&data[..len]);
    }

    /// 마지막 호출 뒤 외부 RAM에 쓰기가 있었으면 `true`. 읽으면 초기화된다.
    pub fn take_ram_dirty(&mut self) -> bool {
        std::mem::take(&mut self.ram_dirty)
    }

    /// 게임이 외부 RAM을 켜 두었는지. RAM 활성화 레지스터가 없는 ROM+RAM 카트리지는 항상 `true`.
    pub fn ram_enabled(&self) -> bool {
        self.ram_enabled || self.mbc == Mbc::None
    }
```

`bus.rs`의 `pub fn cartridge(&self)` 아래에 추가한다:

```rust

    pub fn cartridge_mut(&mut self) -> &mut Cartridge {
        &mut self.cart
    }
```

`gameboy.rs`의 `pub fn debug(&self)` 위(`set_button` 아래)에 추가한다:

```rust
    /// 세이브 파일로 남길 외부 RAM. 배터리가 없는 카트리지는 `None`.
    pub fn battery_ram(&self) -> Option<Vec<u8>> {
        self.bus.cartridge().battery_ram().map(<[u8]>::to_vec)
    }

    /// 세이브 파일 내용을 싣는다. `now_unix`는 MBC3 RTC의 오프라인 경과 보정용이다(M6).
    pub fn load_battery_ram(&mut self, data: &[u8], _now_unix: u64) {
        self.bus.cartridge_mut().load_battery_ram(data);
    }

    /// 마지막 호출 뒤 외부 RAM에 쓰기가 있었으면 `true`. 읽으면 초기화된다.
    pub fn battery_dirty(&mut self) -> bool {
        self.bus.cartridge_mut().take_ram_dirty()
    }

    /// 게임이 외부 RAM을 켜 두었는지. 게임은 보통 저장을 마치면 RAM을 끈다.
    pub fn cartridge_ram_enabled(&self) -> bool {
        self.bus.cartridge().ram_enabled()
    }
```

- [ ] **Step 4: 테스트가 통과하는지 확인한다**

Run: `cargo fmt --all && cargo test -p gb-core --lib 2>&1 | grep -E "FAILED|test result" && cargo clippy -p gb-core --all-targets -- -D warnings`
Expected: `155 passed`. clippy 출력 없음.

- [ ] **Step 5: 커밋한다**

```bash
git add crates/gb-core
git commit -m "feat(core): 배터리 RAM 읽기·싣기와 쓰기 감지

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: 세션의 배터리 세이브

**Files:**
- Modify: `crates/aragorn-app/src/session.rs`(전체 교체)

**Interfaces:**
- Consumes: Task 1–2의 `GameBoy` API, `gb_core::Button`
- Produces:
  - `aragorn_app::session::{SaveStore, SAVE_IDLE_FRAMES}`
  - `Session::load(rom: Vec<u8>, store: Box<dyn SaveStore>, now_unix: u64) -> Result<Session, CartError>`
  - `Session::set_button(Button, bool)`, `battery_ram() -> Option<Vec<u8>>`, `flush()`, `take_errors() -> Vec<String>`
  - 기존 `model()`, `title()`, `advance()`, `framebuffer()`는 그대로다.

- [ ] **Step 1: 실패하는 테스트를 작성한다**

`crates/aragorn-app/src/session.rs`를 아래로 교체한다(구현은 `todo!()`):

```rust
//! 에뮬레이션 세션: 불러온 ROM 하나, 그 실행 속도, 배터리 세이브 (스펙 §5.1).

use crate::pacing::FramePacer;
use gb_core::{Button, CartError, GameBoy, Model};
use std::{io, time::Duration};

/// 게임이 외부 RAM을 켠 채로 두어도, 마지막 쓰기 뒤 이만큼(약 1초) 쓰기가 없으면 저장한다.
pub const SAVE_IDLE_FRAMES: u32 = 60;

/// ROM 하나의 배터리 세이브 저장소 (포트). 데스크톱에서는 ROM 옆 `.sav` 파일이다.
pub trait SaveStore {
    /// 저장된 세이브가 없으면 `Ok(None)`.
    fn load_battery(&self) -> io::Result<Option<Vec<u8>>>;
    /// 덮어쓰기 전에 이전 세이브를 백업 하나로 남긴다.
    fn save_battery(&self, data: &[u8]) -> io::Result<()>;
}

pub struct Session {
        todo!()
    }

impl Session {
    /// ROM을 불러오고, 배터리 카트리지면 `store`의 세이브를 싣는다.
    /// 세이브를 읽지 못해도 게임은 새로 시작하고 오류는 `take_errors`로 알린다.
    pub fn load(
        rom: Vec<u8>,
        store: Box<dyn SaveStore>,
        now_unix: u64,
    ) -> Result<Session, CartError> {
        todo!()
    }

    /// 실제로 에뮬레이션하는 기기.
    pub fn model(&self) -> Model {
        self.gb.model()
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    /// 실제로 `elapsed`가 지났을 때 필요한 만큼 프레임을 돌리고, 돌린 프레임 수를 반환한다.
    pub fn advance(&mut self, elapsed: Duration) -> u32 {
        todo!()
    }

    pub fn set_button(&mut self, button: Button, pressed: bool) {
        todo!()
    }

    /// 160×144, 각 픽셀은 0xRRGGBBAA.
    pub fn framebuffer(&self) -> &[u32] {
        self.gb.framebuffer()
    }

    pub fn battery_ram(&self) -> Option<Vec<u8>> {
        todo!()
    }

    /// 저장하지 않은 세이브가 있으면 지금 저장한다. 앱 종료, ROM 교체, 업데이트 적용 전에 부른다.
    pub fn flush(&mut self) {
        todo!()
    }

    /// 쌓인 오류 문구를 꺼낸다.
    pub fn take_errors(&mut self) -> Vec<String> {
        todo!()
    }

    /// 프레임마다 부른다. 게임이 저장을 마치고 외부 RAM을 끄면 바로, 켜 둔 채라면
    /// 쓰기가 멈추고 `SAVE_IDLE_FRAMES`가 지나면 저장한다.
    fn track_battery(&mut self) {
        todo!()
    }

    fn save_battery(&mut self) {
        let Some(ram) = self.gb.battery_ram() else {
            self.unsaved = false;
            return;
        };
        match self.store.save_battery(&ram) {
            Ok(()) => {
                self.unsaved = false;
                self.save_failed = false;
            }
            Err(e) => {
                if !self.save_failed {
                    self.errors
                        .push(format!("세이브 파일을 저장할 수 없습니다: {e}"));
                }
                self.save_failed = true;
                self.idle_frames = 0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pacing::FRAME_DURATION;
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };

    /// 테스트용 메모리 저장소. 저장 시도 횟수와 실패 여부를 조절할 수 있다.
    #[derive(Default)]
    struct MemoryStore {
        data: RefCell<Option<Vec<u8>>>,
        save_attempts: Cell<u32>,
        fail_load: bool,
        fail_save: Cell<bool>,
    }

    struct Shared(Rc<MemoryStore>);

    impl SaveStore for Shared {
        fn load_battery(&self) -> io::Result<Option<Vec<u8>>> {
            if self.0.fail_load {
                return Err(io::Error::other("디스크 오류"));
            }
            Ok(self.0.data.borrow().clone())
        }

        fn save_battery(&self, data: &[u8]) -> io::Result<()> {
            self.0.save_attempts.set(self.0.save_attempts.get() + 1);
            if self.0.fail_save.get() {
                return Err(io::Error::other("쓰기 금지"));
            }
            *self.0.data.borrow_mut() = Some(data.to_vec());
            Ok(())
        }
    }

    fn session_with(rom: Vec<u8>, store: MemoryStore) -> (Session, Rc<MemoryStore>) {
        let store = Rc::new(store);
        let session = Session::load(rom, Box::new(Shared(Rc::clone(&store))), 0).unwrap();
        (session, store)
    }

    /// 제목 "ARAGORN TEST", 0x0100에 `program`을 둔 32KB ROM (기본은 제자리 루프 JR -2).
    fn rom_with(cart_type: u8, ram_size_code: u8, program: &[u8]) -> Vec<u8> {
        let mut rom = vec![0; 0x8000];
        rom[0x0100..0x0100 + program.len()].copy_from_slice(program);
        rom[0x0134..0x0140].copy_from_slice(b"ARAGORN TEST");
        rom[0x0147] = cart_type;
        rom[0x0149] = ram_size_code;
        rom
    }

    fn looping_rom() -> Vec<u8> {
        rom_with(0x00, 0x00, &[0x18, 0xFE])
    }

    /// MBC1+RAM+BATTERY, 8KB. RAM 켜기 → 0xA000에 0x42 쓰기 → (`disable`이면) RAM 끄기 → 제자리 루프.
    fn saving_rom(disable: bool) -> Vec<u8> {
        let mut program = vec![
            0x3E, 0x0A, 0xEA, 0x00, 0x00, // LD A,0x0A ; LD (0x0000),A
            0x3E, 0x42, 0xEA, 0x00, 0xA0, // LD A,0x42 ; LD (0xA000),A
        ];
        if disable {
            program.extend([0xAF, 0xEA, 0x00, 0x00]); // XOR A ; LD (0x0000),A
        }
        program.extend([0x18, 0xFE]);
        rom_with(0x03, 0x02, &program)
    }

    fn run_frames(session: &mut Session, frames: u32) {
        for _ in 0..frames {
            session.advance(FRAME_DURATION);
        }
    }

    #[test]
    fn loads_rom_and_exposes_title() {
        let (session, _) = session_with(looping_rom(), MemoryStore::default());
        assert_eq!(session.title(), "ARAGORN TEST");
        assert_eq!(session.framebuffer().len(), 160 * 144);
    }

    #[test]
    fn cgb_flagged_rom_runs_as_dmg_until_cgb_support() {
        // 노랑·금·은처럼 CGB 플래그가 있는 ROM도 CGB 하드웨어(M6)가 생기기 전까지는 DMG로 돌린다.
        for flag in [0x80, 0xC0] {
            let mut rom = looping_rom();
            rom[0x0143] = flag;
            let (session, _) = session_with(rom, MemoryStore::default());
            assert_eq!(session.model(), Model::Dmg, "{flag:#04X}");
        }
    }

    #[test]
    fn rejects_invalid_rom() {
        let store = Box::new(Shared(Rc::new(MemoryStore::default())));
        assert_eq!(
            Session::load(vec![0; 16], store, 0).err(),
            Some(CartError::TooSmall(16))
        );
    }

    #[test]
    fn advance_runs_frames_for_elapsed_time() {
        let (mut session, _) = session_with(looping_rom(), MemoryStore::default());
        assert_eq!(session.advance(FRAME_DURATION * 2), 2);
        assert_eq!(session.advance(Duration::ZERO), 0);
    }

    #[test]
    fn existing_save_is_loaded_into_battery_ram() {
        let store = MemoryStore {
            data: RefCell::new(Some(vec![0x99, 0x88])),
            ..MemoryStore::default()
        };
        let (session, _) = session_with(saving_rom(true), store);
        assert_eq!(&session.battery_ram().unwrap()[..3], &[0x99, 0x88, 0x00]);
    }

    #[test]
    fn saves_when_game_disables_ram() {
        let (mut session, store) = session_with(saving_rom(true), MemoryStore::default());
        run_frames(&mut session, 1);
        assert_eq!(store.save_attempts.get(), 1);
        assert_eq!(store.data.borrow().as_ref().unwrap()[0], 0x42);
        run_frames(&mut session, 100);
        assert_eq!(
            store.save_attempts.get(),
            1,
            "새 쓰기가 없으면 다시 저장하지 않는다"
        );
    }

    #[test]
    fn saves_after_idle_frames_when_game_keeps_ram_enabled() {
        let (mut session, store) = session_with(saving_rom(false), MemoryStore::default());
        run_frames(&mut session, SAVE_IDLE_FRAMES);
        assert_eq!(store.save_attempts.get(), 0);
        run_frames(&mut session, 1);
        assert_eq!(store.save_attempts.get(), 1);
    }

    #[test]
    fn flush_saves_pending_writes() {
        let (mut session, store) = session_with(saving_rom(false), MemoryStore::default());
        run_frames(&mut session, 1);
        session.flush();
        assert_eq!(store.save_attempts.get(), 1);
        session.flush();
        assert_eq!(store.save_attempts.get(), 1);
    }

    #[test]
    fn carts_without_battery_never_touch_the_store() {
        let store = MemoryStore {
            fail_load: true,
            ..MemoryStore::default()
        };
        let (mut session, store) = session_with(looping_rom(), store);
        run_frames(&mut session, 3);
        session.flush();
        assert!(session.take_errors().is_empty());
        assert_eq!(store.save_attempts.get(), 0);
    }

    #[test]
    fn unreadable_save_is_reported_and_game_starts_fresh() {
        let store = MemoryStore {
            fail_load: true,
            ..MemoryStore::default()
        };
        let (mut session, _) = session_with(saving_rom(true), store);
        assert_eq!(
            session.take_errors(),
            ["세이브 파일을 읽을 수 없습니다: 디스크 오류"]
        );
        assert_eq!(session.battery_ram().unwrap()[0], 0x00);
    }

    #[test]
    fn failed_save_is_reported_once_and_retried() {
        let store = MemoryStore::default();
        store.fail_save.set(true);
        let (mut session, store) = session_with(saving_rom(true), store);
        run_frames(&mut session, 10);
        assert_eq!(store.save_attempts.get(), 1);
        assert_eq!(
            session.take_errors(),
            ["세이브 파일을 저장할 수 없습니다: 쓰기 금지"]
        );
        store.fail_save.set(false);
        run_frames(&mut session, SAVE_IDLE_FRAMES);
        assert_eq!(store.save_attempts.get(), 2);
        assert!(session.take_errors().is_empty());
        assert_eq!(store.data.borrow().as_ref().unwrap()[0], 0x42);
    }
}
```

- [ ] **Step 2: 테스트가 실패하는지 확인한다**

Run: `cargo test -p aragorn-app --lib session:: 2>&1 | grep -E "test result"`
Expected: FAIL. 11개 세션 테스트가 모두 `not yet implemented`로 패닉한다(데스크톱 크레이트는 Task 4 전까지 컴파일되지 않는다).

- [ ] **Step 3: 구현한다**

`session.rs`에서 `#[cfg(test)]` 앞까지를 아래로 교체한다:

```rust
//! 에뮬레이션 세션: 불러온 ROM 하나, 그 실행 속도, 배터리 세이브 (스펙 §5.1).

use crate::pacing::FramePacer;
use gb_core::{Button, CartError, GameBoy, Model};
use std::{io, time::Duration};

/// 게임이 외부 RAM을 켠 채로 두어도, 마지막 쓰기 뒤 이만큼(약 1초) 쓰기가 없으면 저장한다.
pub const SAVE_IDLE_FRAMES: u32 = 60;

/// ROM 하나의 배터리 세이브 저장소 (포트). 데스크톱에서는 ROM 옆 `.sav` 파일이다.
pub trait SaveStore {
    /// 저장된 세이브가 없으면 `Ok(None)`.
    fn load_battery(&self) -> io::Result<Option<Vec<u8>>>;
    /// 덮어쓰기 전에 이전 세이브를 백업 하나로 남긴다.
    fn save_battery(&self, data: &[u8]) -> io::Result<()>;
}

pub struct Session {
    gb: GameBoy,
    pacer: FramePacer,
    title: String,
    store: Box<dyn SaveStore>,
    /// 아직 저장하지 않은 외부 RAM 쓰기가 있는지.
    unsaved: bool,
    /// 마지막 외부 RAM 쓰기 뒤 지난 프레임 수.
    idle_frames: u32,
    /// 직전 저장이 실패했는지. 실패하면 `SAVE_IDLE_FRAMES`마다 다시 시도한다.
    save_failed: bool,
    /// 상태 표시줄에 보일 오류 문구.
    errors: Vec<String>,
}

impl Session {
    /// ROM을 불러오고, 배터리 카트리지면 `store`의 세이브를 싣는다.
    /// 세이브를 읽지 못해도 게임은 새로 시작하고 오류는 `take_errors`로 알린다.
    pub fn load(
        rom: Vec<u8>,
        store: Box<dyn SaveStore>,
        now_unix: u64,
    ) -> Result<Session, CartError> {
        // CGB 하드웨어(VRAM/WRAM 뱅크, 팔레트)는 M6에서 구현한다. 그 전까지 CGB 플래그가 있는 ROM을
        // CGB 모드로 시작하면 화면과 메모리가 망가지므로 모두 DMG로 돌린다.
        let mut gb = GameBoy::new(rom, Model::Dmg)?;
        let title = gb.header().title.trim().to_string();
        let mut errors = Vec::new();
        if gb.battery_ram().is_some() {
            match store.load_battery() {
                Ok(Some(data)) => gb.load_battery_ram(&data, now_unix),
                Ok(None) => {}
                Err(e) => errors.push(format!("세이브 파일을 읽을 수 없습니다: {e}")),
            }
        }
        Ok(Session {
            gb,
            pacer: FramePacer::default(),
            title,
            store,
            unsaved: false,
            idle_frames: 0,
            save_failed: false,
            errors,
        })
    }

    /// 실제로 에뮬레이션하는 기기.
    pub fn model(&self) -> Model {
        self.gb.model()
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    /// 실제로 `elapsed`가 지났을 때 필요한 만큼 프레임을 돌리고, 돌린 프레임 수를 반환한다.
    pub fn advance(&mut self, elapsed: Duration) -> u32 {
        let frames = self.pacer.frames_for(elapsed);
        for _ in 0..frames {
            self.gb.run_frame();
            self.track_battery();
        }
        frames
    }

    pub fn set_button(&mut self, button: Button, pressed: bool) {
        self.gb.set_button(button, pressed);
    }

    /// 160×144, 각 픽셀은 0xRRGGBBAA.
    pub fn framebuffer(&self) -> &[u32] {
        self.gb.framebuffer()
    }

    pub fn battery_ram(&self) -> Option<Vec<u8>> {
        self.gb.battery_ram()
    }

    /// 저장하지 않은 세이브가 있으면 지금 저장한다. 앱 종료, ROM 교체, 업데이트 적용 전에 부른다.
    pub fn flush(&mut self) {
        if self.unsaved {
            self.save_battery();
        }
    }

    /// 쌓인 오류 문구를 꺼낸다.
    pub fn take_errors(&mut self) -> Vec<String> {
        std::mem::take(&mut self.errors)
    }

    /// 프레임마다 부른다. 게임이 저장을 마치고 외부 RAM을 끄면 바로, 켜 둔 채라면
    /// 쓰기가 멈추고 `SAVE_IDLE_FRAMES`가 지나면 저장한다.
    fn track_battery(&mut self) {
        if self.gb.battery_dirty() {
            self.unsaved = true;
            self.idle_frames = 0;
        } else if self.unsaved {
            self.idle_frames += 1;
        }
        let settled = !self.save_failed && !self.gb.cartridge_ram_enabled();
        if self.unsaved && (settled || self.idle_frames >= SAVE_IDLE_FRAMES) {
            self.save_battery();
        }
    }

    fn save_battery(&mut self) {
        let Some(ram) = self.gb.battery_ram() else {
            self.unsaved = false;
            return;
        };
        match self.store.save_battery(&ram) {
            Ok(()) => {
                self.unsaved = false;
                self.save_failed = false;
            }
            Err(e) => {
                if !self.save_failed {
                    self.errors
                        .push(format!("세이브 파일을 저장할 수 없습니다: {e}"));
                }
                self.save_failed = true;
                self.idle_frames = 0;
            }
        }
    }
}
```

- [ ] **Step 4: 테스트가 통과하는지 확인한다**

Run: `cargo fmt --all && cargo test -p aragorn-app 2>&1 | grep -E "FAILED|test result" | head -1 && cargo clippy -p aragorn-app --all-targets -- -D warnings`
Expected: `62 passed`. clippy 출력 없음.

- [ ] **Step 5: 커밋한다**

```bash
git add crates/aragorn-app
git commit -m "feat(app): 세션의 배터리 세이브 불러오기·자동 저장과 SaveStore 포트

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

이 커밋에서는 데스크톱 크레이트가 아직 옛 `Session::load`를 불러서 워크스페이스 전체 빌드가 깨진다. Task 4에서 바로 고친다.

---

### Task 4: ROM 옆 `.sav` 저장소, 키보드 입력, 앱 연결

**Files:**
- Create: `crates/aragorn-desktop/src/adapters/fs_save_store.rs`, `crates/aragorn-desktop/src/ui/input.rs`
- Modify: `crates/aragorn-desktop/src/adapters/mod.rs`, `crates/aragorn-desktop/src/ui/mod.rs`, `crates/aragorn-desktop/src/app.rs`

**Interfaces:**
- Consumes: Task 3 `Session`, `SaveStore`
- Produces:
  - `adapters::FsSaveStore::for_rom(&Path)`
  - `ui::input::{DEFAULT_KEYS, button_states}`
  - `app::load_session(&Path, now_unix: u64)`

- [ ] **Step 1: 실패하는 테스트를 작성한다**

`adapters/mod.rs`의 `mod github_policy;` 위에 `mod fs_save_store;`를, `pub use github_policy::GithubPolicySource;` 위에 `pub use fs_save_store::FsSaveStore;`를 추가한다. `ui/mod.rs`의 `pub mod fonts;` 아래에 `pub mod input;`을 추가한다.

`crates/aragorn-desktop/src/adapters/fs_save_store.rs`:

```rust
use aragorn_app::session::SaveStore;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// ROM 옆의 `<ROM 이름>.sav` 파일. 다른 에뮬레이터(BGB, mGBA, VBA-M)와 같은 위치다.
pub struct FsSaveStore {
    path: PathBuf,
}

impl FsSaveStore {
    pub fn for_rom(rom_path: &Path) -> Self {
        Self {
            path: rom_path.with_extension("sav"),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl SaveStore for FsSaveStore {
    fn load_battery(&self) -> io::Result<Option<Vec<u8>>> {
        todo!()
    }

    fn save_battery(&self, data: &[u8]) -> io::Result<()> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_file_sits_next_to_rom() {
        let store = FsSaveStore::for_rom(Path::new("roms/pokemon red.gb"));
        assert_eq!(store.path(), Path::new("roms/pokemon red.sav"));
    }

    #[test]
    fn missing_save_loads_as_none() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsSaveStore::for_rom(&dir.path().join("game.gb"));
        assert_eq!(store.load_battery().unwrap(), None);
    }

    #[test]
    fn saved_data_loads_back() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsSaveStore::for_rom(&dir.path().join("game.gb"));
        store.save_battery(&[1, 2, 3]).unwrap();
        assert_eq!(store.load_battery().unwrap(), Some(vec![1, 2, 3]));
        assert!(!dir.path().join("game.sav.tmp").exists());
    }

    #[test]
    fn overwriting_keeps_previous_save_as_backup() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsSaveStore::for_rom(&dir.path().join("game.gb"));
        store.save_battery(&[1]).unwrap();
        store.save_battery(&[2]).unwrap();
        assert_eq!(fs::read(dir.path().join("game.sav")).unwrap(), [2]);
        assert_eq!(fs::read(dir.path().join("game.sav.bak")).unwrap(), [1]);
    }

    #[test]
    fn unwritable_location_reports_error() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsSaveStore::for_rom(&dir.path().join("없는 폴더").join("game.gb"));
        assert!(store.save_battery(&[1]).is_err());
    }
}
```

`crates/aragorn-desktop/src/ui/input.rs`:

```rust
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
    todo!()
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
```

`app.rs` tests 모듈의 두 `load_session` 호출에 두 번째 인자 `0`을 넣는다(`load_session(Path::new("/없는/경로/pokemon.gb"), 0)`, `load_session(&path, 0)`). 그리고 tests 모듈 끝에 추가한다:

```rust
    #[test]
    fn game_save_is_written_next_to_rom() {
        // MBC1+RAM+BATTERY ROM: RAM 켜기 → 0xA000에 0x42 → RAM 끄기 → 제자리 루프
        let mut rom = vec![0u8; 0x8000];
        let program = [
            0x3E, 0x0A, 0xEA, 0x00, 0x00, 0x3E, 0x42, 0xEA, 0x00, 0xA0, 0xAF, 0xEA, 0x00, 0x00,
            0x18, 0xFE,
        ];
        rom[0x0100..0x0100 + program.len()].copy_from_slice(&program);
        rom[0x0147] = 0x03;
        rom[0x0149] = 0x02;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("game.gb");
        std::fs::write(&path, &rom).unwrap();

        let mut session = load_session(&path, 0).unwrap();
        session.advance(aragorn_app::pacing::FRAME_DURATION);
        let save = std::fs::read(dir.path().join("game.sav")).unwrap();
        assert_eq!((save.len(), save[0]), (0x2000, 0x42));

        let reloaded = load_session(&path, 0).unwrap();
        assert_eq!(reloaded.battery_ram().unwrap()[0], 0x42);
    }
```

- [ ] **Step 2: 테스트가 실패하는지 확인한다**

Run: `cargo test -p aragorn-desktop --lib 2>&1 | grep -E "^error" | sort | uniq -c | head -5`
Expected: FAIL(컴파일 오류). `load_session`이 아직 인자 하나를 받고 옛 `Session::load`를 부르기 때문이다.

- [ ] **Step 3: 구현한다**

`fs_save_store.rs`와 `input.rs`에서 `#[cfg(test)]` 앞까지를 각각 아래로 교체한다.

```rust
use aragorn_app::session::SaveStore;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// ROM 옆의 `<ROM 이름>.sav` 파일. 다른 에뮬레이터(BGB, mGBA, VBA-M)와 같은 위치다.
pub struct FsSaveStore {
    path: PathBuf,
}

impl FsSaveStore {
    pub fn for_rom(rom_path: &Path) -> Self {
        Self {
            path: rom_path.with_extension("sav"),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl SaveStore for FsSaveStore {
    fn load_battery(&self) -> io::Result<Option<Vec<u8>>> {
        match fs::read(&self.path) {
            Ok(data) => Ok(Some(data)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn save_battery(&self, data: &[u8]) -> io::Result<()> {
        // 이전 세이브를 `.sav.bak` 하나로 남기고, 쓰는 도중 꺼져도 깨지지 않도록 임시 파일에 쓰고 교체한다.
        if self.path.exists() {
            fs::copy(&self.path, self.path.with_extension("sav.bak"))?;
        }
        let tmp = self.path.with_extension("sav.tmp");
        fs::write(&tmp, data)?;
        fs::rename(&tmp, &self.path)
    }
}
```
```rust
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
```

`app.rs` import를 바꾼다:
- `adapters::UnavailableUpdater,` → `adapters::{FsSaveStore, UnavailableUpdater},`
- `ui::{` 묶음의 `screen::ScreenView,` 위에 `input,`을 추가한다.
- `time::{Duration, Instant},` → `time::{Duration, Instant, SystemTime},`

`load_session` 함수를 아래로 교체한다(`now_unix` 도우미 포함):

```rust
/// ROM 파일을 읽어 세션을 만든다. 배터리 세이브는 ROM 옆 `.sav`에서 읽고 쓴다.
/// 실패하면 상태 표시줄에 보일 문구를 돌려준다.
pub fn load_session(path: &Path, now_unix: u64) -> Result<Session, String> {
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let rom = std::fs::read(path).map_err(|e| format!("{name}을(를) 읽을 수 없습니다: {e}"))?;
    let store = Box::new(FsSaveStore::for_rom(path));
    Session::load(rom, store, now_unix).map_err(|e| format!("{name}을(를) 열 수 없습니다: {e}"))
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
```

`open_rom`:
- 첫 줄에 `self.flush_session();`을 추가한다.
- `match load_session(path) {`를 `match load_session(path, now_unix()) {`로 바꾼다.
- `Ok` 분기의 `self.last_tick = Instant::now();` 다음에 `self.collect_session_errors();`를 추가한다.

`run_emulation`을 아래로 교체하고, 그 아래에 `flush_session`, `collect_session_errors`를 추가한다:

```rust
    /// 경과 시간만큼 에뮬레이션을 진행하고 화면을 갱신한다. 강제 업데이트 중에는 멈춘다.
    fn run_emulation(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        let elapsed = now - self.last_tick;
        self.last_tick = now;
        let Some(session) = &mut self.session else {
            return;
        };
        if self.flow.blocks_emulation() {
            return;
        }
        for (button, pressed) in input::button_states(|key| ctx.input(|i| i.key_down(key))) {
            session.set_button(button, pressed);
        }
        if session.advance(elapsed) > 0 {
            self.screen.update(ctx, session.framebuffer());
        }
        self.collect_session_errors();
        ctx.request_repaint();
    }

    /// 저장하지 않은 배터리 세이브를 디스크에 쓴다.
    fn flush_session(&mut self) {
        if let Some(session) = &mut self.session {
            session.flush();
        }
        self.collect_session_errors();
    }

    /// 세션의 세이브 오류를 로그와 상태 표시줄로 옮긴다.
    fn collect_session_errors(&mut self) {
        let Some(session) = &mut self.session else {
            return;
        };
        for message in session.take_errors() {
            log::error!("{message}");
            self.notice = Some(message);
        }
    }
```

`flush_persistent_state`를 아래로 교체한다:

```rust
    /// 업데이트를 적용하기 전에 디스크에 남겨야 하는 상태(설정, 배터리 세이브)를 저장한다.
    fn flush_persistent_state(&mut self) {
        self.save_config();
        self.flush_session();
    }
```

`logic()`의 종료 처리에서 `self.exit_handled = true;` 다음 줄에 `self.flush_session();`을 추가한다.

- [ ] **Step 4: 테스트가 통과하는지 확인한다**

Run: `cargo fmt --all && cargo test -p aragorn-desktop 2>&1 | grep -E "FAILED|test result" | head -1 && cargo clippy --workspace --all-targets -- -D warnings`
Expected: `39 passed`. clippy 출력 없음.

- [ ] **Step 5: 커밋한다**

```bash
git add crates/aragorn-desktop
git commit -m "feat(desktop): ROM 옆 .sav 저장, 기본 키보드 입력, 종료·교체 시 세이브 저장

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: 전체 확인, PR

- [ ] **Step 1: 전체 검사를 실행한다**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && ARAGORN_REQUIRE_TEST_ROMS=1 cargo test --workspace --no-fail-fast 2>&1 | grep "test result" | grep -v " 0 passed"`
Expected: 모든 결과가 `ok`다.
- gb-core lib: 155
- acid2: 1
- blargg: 15
- mooneye: 77
- app: 62
- desktop: 39
- xtask: 14

- [ ] **Step 2: PR을 만들고 CI를 확인한다** (외부 공개 작업이므로 사용자 확인 후)

```bash
git push -u origin feat/m4-input-save
gh pr create --repo dongjay00/aragorn --base main --title "M4: 키 입력과 배터리 세이브" --body-file <작성한 본문>
gh pr checks --watch
```

- [ ] **Step 3: 플레이 확인을 요청한다** (사용자 작업, 릴리스 뒤)

사용자에게 아래 확인을 요청한다.
- 레드/블루로 새 게임을 시작해 방향키와 Z/X/Enter로 조작해 본다.
- 게임 안에서 저장하고 앱을 껐다 켜서 이어하기가 되는지 확인한다.
- ROM 옆에 `.sav`(두 번째 저장부터 `.sav.bak`)가 생기는지 확인한다.
