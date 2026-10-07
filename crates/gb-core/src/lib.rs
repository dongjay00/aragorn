//! Game Boy / Game Boy Color 에뮬레이션 코어.
//!
//! UI, OS, 네트워크에 의존하지 않는다. 이름은 Pan Docs 용어를 따른다.

pub mod bus;
pub mod cartridge;
pub mod cpu;
mod model;
pub mod ppu;
pub mod serial;
pub mod timer;

pub use cartridge::{CartError, Header};
pub use cpu::{IllegalOpcode, Registers};
pub use model::Model;
