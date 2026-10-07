//! 코어의 공개 진입점.

use crate::bus::Bus;
use crate::cartridge::{CartError, Cartridge, Header};
use crate::cpu::{Cpu, IllegalOpcode, Registers};
use crate::joypad::Button;
use crate::model::Model;
use crate::ppu;

pub struct GameBoy {
    cpu: Cpu,
    bus: Bus,
    model: Model,
}

impl GameBoy {
    /// 카트리지를 읽고 부트 ROM이 끝난 직후 상태로 시작한다 (부트 ROM은 실행하지 않는다).
    pub fn new(rom: Vec<u8>, model: Model) -> Result<GameBoy, CartError> {
        let cart = Cartridge::new(rom)?;
        let model = model.resolve(cart.header().cgb_flag);
        Ok(GameBoy {
            cpu: Cpu::new(Registers::post_boot(model)),
            bus: Bus::new(cart),
            model,
        })
    }

    pub fn model(&self) -> Model {
        self.model
    }

    pub fn header(&self) -> &Header {
        self.bus.cartridge().header()
    }

    /// 명령 하나(또는 인터럽트 디스패치, HALT 대기 1 M-사이클)를 실행한다.
    pub fn step(&mut self) {
        self.cpu.step(&mut self.bus);
    }

    /// 다음 VBlank 진입까지 실행한다. 프레임 경계가 오지 않아도(LCD가 꺼져 있거나 게임이
    /// LCD를 계속 껐다 켜는 경우) 한 프레임 분량(70224 T-사이클)이 지나면 반환한다.
    pub fn run_frame(&mut self) {
        let deadline = self.bus.cycles() + u64::from(ppu::DOTS_PER_FRAME / 4);
        loop {
            self.cpu.step(&mut self.bus);
            if self.bus.take_frame_ready() || self.bus.cycles() >= deadline {
                return;
            }
        }
    }

    /// 160×144, 각 픽셀은 0xRRGGBBAA.
    pub fn framebuffer(&self) -> &[u32] {
        self.bus.ppu().framebuffer()
    }

    /// 키를 누르거나 뗀다. 선택된 줄에서 새로 눌리면 조이패드 인터럽트를 요청한다.
    pub fn set_button(&mut self, button: Button, pressed: bool) {
        self.bus.set_button(button, pressed);
    }

    pub fn debug(&self) -> DebugView<'_> {
        DebugView { gb: self }
    }
}

/// 디버거와 테스트용 읽기 전용 상태.
pub struct DebugView<'a> {
    gb: &'a GameBoy,
}

impl<'a> DebugView<'a> {
    pub fn registers(&self) -> Registers {
        self.gb.cpu.regs
    }

    pub fn illegal_opcode(&self) -> Option<IllegalOpcode> {
        self.gb.cpu.lock()
    }

    pub fn serial_output(&self) -> &'a [u8] {
        self.gb.bus.serial_output()
    }

    pub fn peek(&self, addr: u16) -> u8 {
        self.gb.bus.peek(addr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::test_rom;

    fn gb_with_program(program: &[u8]) -> GameBoy {
        let mut rom = test_rom(0x00, 0x00, 0x00);
        rom[0x0100..0x0100 + program.len()].copy_from_slice(program);
        GameBoy::new(rom, Model::Dmg).unwrap()
    }

    #[test]
    fn auto_model_follows_cgb_flag() {
        let gb = GameBoy::new(test_rom(0x00, 0x00, 0x80), Model::Auto).unwrap();
        assert_eq!(gb.model(), Model::Cgb);
        assert_eq!(gb.debug().registers().a, 0x11);
        let gb = GameBoy::new(test_rom(0x00, 0x00, 0x80), Model::Dmg).unwrap();
        assert_eq!(gb.model(), Model::Dmg);
        assert_eq!(gb.debug().registers().a, 0x01);
        assert_eq!(gb.header().title, "TEST");
    }

    #[test]
    fn unsupported_cartridge_is_an_error() {
        assert_eq!(
            GameBoy::new(test_rom(0x05, 0x00, 0x00), Model::Auto).err(),
            Some(CartError::Unsupported(0x05))
        );
    }

    #[test]
    fn step_executes_one_instruction() {
        let mut gb = gb_with_program(&[0x3C]); // INC A
        gb.step();
        assert_eq!(gb.debug().registers().pc, 0x0101);
    }

    #[test]
    fn run_frame_stops_at_vblank() {
        let mut gb = gb_with_program(&[0x18, 0xFE]); // JR -2
        gb.run_frame();
        assert_eq!(gb.debug().peek(0xFF44), 144);
    }

    #[test]
    fn run_frame_returns_with_lcd_off() {
        // XOR A ; LDH (0x40),A ; JR -2
        let mut gb = gb_with_program(&[0xAF, 0xE0, 0x40, 0x18, 0xFE]);
        gb.run_frame();
        assert_eq!(gb.debug().peek(0xFF40), 0x00);
        assert_eq!(gb.debug().peek(0xFF44), 0);
    }

    #[test]
    fn run_frame_returns_when_lcd_toggles_every_frame() {
        // XOR A ; LDH (0x40),A ; LD A,0x91 ; LDH (0x40),A ; JR -9
        let mut gb = gb_with_program(&[0xAF, 0xE0, 0x40, 0x3E, 0x91, 0xE0, 0x40, 0x18, 0xF7]);
        gb.run_frame();
        gb.run_frame();
    }

    #[test]
    fn halt_forever_does_not_hang_run_frame() {
        // DI ; HALT, IE=0: 깨어날 수 없지만 프레임은 계속 진행해야 한다.
        let mut gb = gb_with_program(&[0xF3, 0x76]);
        gb.run_frame();
        gb.run_frame();
        assert_eq!(gb.debug().registers().pc, 0x0102);
    }

    #[test]
    fn illegal_opcode_does_not_hang_run_frame() {
        let mut gb = gb_with_program(&[0xD3]);
        gb.run_frame();
        assert_eq!(
            gb.debug().illegal_opcode(),
            Some(IllegalOpcode {
                pc: 0x0100,
                opcode: 0xD3
            })
        );
    }

    #[test]
    fn serial_output_is_visible_in_debug_view() {
        // LD A,'O' ; LDH (0x01),A ; LD A,0x81 ; LDH (0x02),A ; JR -2
        let mut gb = gb_with_program(&[0x3E, b'O', 0xE0, 0x01, 0x3E, 0x81, 0xE0, 0x02, 0x18, 0xFE]);
        gb.run_frame();
        assert_eq!(gb.debug().serial_output(), b"O");
    }

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
}
