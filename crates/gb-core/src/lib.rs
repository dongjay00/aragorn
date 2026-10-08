//! Game Boy / Game Boy Color 에뮬레이션 코어.
//!
//! UI, OS, 네트워크에 의존하지 않는다. 이름은 Pan Docs 용어를 따른다.
//! 앱 계층은 `GameBoy`와 `DebugView`만 쓴다. 하위 모듈은 디버거와 테스트를 위해 공개한다.

pub mod apu;
pub mod bus;
pub mod cartridge;
pub mod cpu;
mod gameboy;
pub mod joypad;
mod model;
pub mod ppu;
pub mod rtc;
pub mod serial;
pub mod timer;

pub use cartridge::{CartError, Header};
pub use cpu::{IllegalOpcode, Registers};
pub use gameboy::{DebugView, GameBoy};
pub use joypad::Button;
pub use model::Model;
