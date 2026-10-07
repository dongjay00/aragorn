//! 메모리 버스: 주소 공간 매핑과 주변장치 진행 (Pan Docs "Memory Map").

use crate::cartridge::Cartridge;
use crate::cpu::{CpuBus, IE_ADDR, IF_ADDR};
use crate::ppu::{self, Ppu};
use crate::serial::{self, Serial};
use crate::timer::{self, Timer};

pub const INT_VBLANK: u8 = 0x01;
pub const INT_STAT: u8 = 0x02;
pub const INT_TIMER: u8 = 0x04;
pub const INT_SERIAL: u8 = 0x08;
pub const INT_JOYPAD: u8 = 0x10;

const JOYP: u16 = 0xFF00;

pub struct Bus {
    cart: Cartridge,
    ppu: Ppu,
    timer: Timer,
    serial: Serial,
    wram: Box<[u8; 0x2000]>,
    hram: [u8; 0x7F],
    /// 아직 구현하지 않은 I/O 레지스터(0xFF00–0xFF7F)는 쓴 값을 그대로 보관한다.
    io: [u8; 0x80],
    ie: u8,
    if_: u8,
    /// 시작 후 진행한 M-사이클 수.
    cycles: u64,
}

impl Bus {
    /// 부트 ROM이 끝난 직후의 DMG 상태 (Pan Docs "Power Up Sequence").
    pub fn new(cart: Cartridge) -> Self {
        Self {
            cart,
            ppu: Ppu::default(),
            timer: Timer::new(0xABCC),
            serial: Serial::default(),
            wram: Box::new([0; 0x2000]),
            hram: [0; 0x7F],
            io: [0xFF; 0x80],
            ie: 0x00,
            if_: INT_VBLANK,
            cycles: 0,
        }
    }

    pub fn cartridge(&self) -> &Cartridge {
        &self.cart
    }

    pub fn serial_output(&self) -> &[u8] {
        self.serial.output()
    }

    pub fn cycles(&self) -> u64 {
        self.cycles
    }

    pub fn take_frame_ready(&mut self) -> bool {
        self.ppu.take_frame_ready()
    }

    /// 부수 효과 없는 읽기 (디버거도 쓴다).
    pub fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => self.cart.read_rom(addr),
            0x8000..=0x9FFF => self.ppu.read_vram(addr),
            0xA000..=0xBFFF => self.cart.read_ram(addr),
            0xC000..=0xDFFF => self.wram[usize::from(addr - 0xC000)],
            0xE000..=0xFDFF => self.wram[usize::from(addr - 0xE000)],
            0xFE00..=0xFE9F => self.ppu.read_oam(addr),
            0xFEA0..=0xFEFF => 0x00,
            0xFF00..=0xFF7F => self.read_io(addr),
            0xFF80..=0xFFFE => self.hram[usize::from(addr - 0xFF80)],
            IE_ADDR => self.ie,
        }
    }

    fn read_io(&self, addr: u16) -> u8 {
        match addr {
            // 버튼 입력은 M4에서 구현한다. 지금은 아무 버튼도 눌리지 않은 상태다.
            JOYP => 0xC0 | (self.io[0] & 0x30) | 0x0F,
            serial::SB | serial::SC => self.serial.read(addr),
            timer::DIV..=timer::TAC => self.timer.read(addr),
            IF_ADDR => self.if_ | 0xE0,
            ppu::LCDC | ppu::LY => self.ppu.read_reg(addr),
            _ => self.io[usize::from(addr - 0xFF00)],
        }
    }

    fn write_io(&mut self, addr: u16, value: u8) {
        match addr {
            serial::SB | serial::SC => {
                let irq = self.serial.write(addr, value);
                self.request(INT_SERIAL, irq);
            }
            timer::DIV..=timer::TAC => {
                let irq = self.timer.write(addr, value);
                self.request(INT_TIMER, irq);
            }
            IF_ADDR => self.if_ = value & 0x1F,
            ppu::LCDC | ppu::LY => self.ppu.write_reg(addr, value),
            _ => self.io[usize::from(addr - 0xFF00)] = value,
        }
    }

    fn request(&mut self, interrupt: u8, requested: bool) {
        if requested {
            self.if_ |= interrupt;
        }
    }
}

impl CpuBus for Bus {
    fn read(&mut self, addr: u16) -> u8 {
        self.peek(addr)
    }

    fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x7FFF => self.cart.write_rom(addr, value),
            0x8000..=0x9FFF => self.ppu.write_vram(addr, value),
            0xA000..=0xBFFF => self.cart.write_ram(addr, value),
            0xC000..=0xDFFF => self.wram[usize::from(addr - 0xC000)] = value,
            0xE000..=0xFDFF => self.wram[usize::from(addr - 0xE000)] = value,
            0xFE00..=0xFE9F => self.ppu.write_oam(addr, value),
            0xFEA0..=0xFEFF => {}
            0xFF00..=0xFF7F => self.write_io(addr, value),
            0xFF80..=0xFFFE => self.hram[usize::from(addr - 0xFF80)] = value,
            IE_ADDR => self.ie = value,
        }
    }

    fn tick(&mut self) {
        self.cycles += 1;
        let timer_irq = self.timer.tick();
        self.request(INT_TIMER, timer_irq);
        let vblank_irq = self.ppu.tick(4);
        self.request(INT_VBLANK, vblank_irq);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::test_rom;

    fn bus() -> Bus {
        let mut rom = test_rom(0x00, 0x00, 0x00);
        rom[0x0200] = 0xAB;
        Bus::new(Cartridge::new(rom).unwrap())
    }

    #[test]
    fn reads_rom_and_ignores_rom_writes() {
        let mut b = bus();
        b.write(0x0200, 0x00);
        assert_eq!(b.read(0x0200), 0xAB);
    }

    #[test]
    fn echo_ram_mirrors_wram() {
        let mut b = bus();
        b.write(0xC123, 0x42);
        assert_eq!(b.read(0xE123), 0x42);
        b.write(0xFDFF, 7);
        assert_eq!(b.read(0xDDFF), 7);
    }

    #[test]
    fn hram_and_ie_store_bytes() {
        let mut b = bus();
        b.write(0xFF80, 1);
        b.write(0xFFFE, 2);
        b.write(IE_ADDR, 0x1F);
        assert_eq!(
            (b.read(0xFF80), b.read(0xFFFE), b.read(IE_ADDR)),
            (1, 2, 0x1F)
        );
    }

    #[test]
    fn if_upper_bits_read_as_one() {
        let mut b = bus();
        b.write(IF_ADDR, 0x04);
        assert_eq!(b.read(IF_ADDR), 0xE4);
    }

    #[test]
    fn timer_overflow_requests_interrupt() {
        let mut b = bus();
        b.write(IF_ADDR, 0);
        b.write(timer::TAC, 0x05);
        b.write(timer::TIMA, 0xFF);
        for _ in 0..4 {
            b.tick();
        }
        assert_eq!(b.read(IF_ADDR) & INT_TIMER, INT_TIMER);
    }

    #[test]
    fn serial_transfer_captures_output_and_requests_interrupt() {
        let mut b = bus();
        b.write(IF_ADDR, 0);
        b.write(serial::SB, b'P');
        b.write(serial::SC, 0x81);
        assert_eq!(b.serial_output(), b"P");
        assert_eq!(b.read(IF_ADDR) & INT_SERIAL, INT_SERIAL);
    }

    #[test]
    fn vblank_requests_interrupt_and_marks_frame() {
        let mut b = bus();
        b.write(IF_ADDR, 0);
        for _ in 0..114 * 144 {
            b.tick();
        }
        assert_eq!(b.read(IF_ADDR) & INT_VBLANK, INT_VBLANK);
        assert_eq!(b.read(ppu::LY), 144);
        assert!(b.take_frame_ready());
    }

    #[test]
    fn unusable_area_reads_zero() {
        let mut b = bus();
        b.write(0xFEA0, 5);
        assert_eq!(b.read(0xFEA0), 0);
    }

    #[test]
    fn joypad_reports_no_buttons() {
        let mut b = bus();
        b.write(JOYP, 0x20);
        assert_eq!(b.read(JOYP), 0xEF);
    }

    #[test]
    fn unimplemented_io_keeps_written_value() {
        let mut b = bus();
        b.write(0xFF42, 0x33);
        assert_eq!(b.read(0xFF42), 0x33);
    }

    #[test]
    fn every_address_is_readable_and_writable() {
        let mut b = bus();
        for addr in 0..=0xFFFF {
            let value = b.read(addr);
            b.write(addr, value);
        }
    }

    #[test]
    fn post_boot_io_state() {
        let b = bus();
        assert_eq!(b.peek(ppu::LCDC), 0x91);
        assert_eq!(b.peek(IF_ADDR), 0xE1);
        assert_eq!(b.peek(timer::DIV), 0xAB);
    }
}
