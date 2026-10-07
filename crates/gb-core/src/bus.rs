//! 메모리 버스: 주소 공간 매핑과 주변장치 진행 (Pan Docs "Memory Map").

use crate::cartridge::Cartridge;
use crate::cpu::{CpuBus, IE_ADDR, IF_ADDR};
use crate::joypad::{self, Button, Joypad};
use crate::ppu::{self, Ppu};
use crate::serial::{self, Serial};
use crate::timer::{self, Timer};

pub const INT_VBLANK: u8 = 0x01;
pub const INT_STAT: u8 = 0x02;
pub const INT_TIMER: u8 = 0x04;
pub const INT_SERIAL: u8 = 0x08;
pub const INT_JOYPAD: u8 = 0x10;

const DMA: u16 = 0xFF46;
const OAM_LEN: u16 = 0xA0;

/// OAM DMA (Pan Docs "OAM DMA Transfer").
#[derive(Debug, Clone, Default)]
struct OamDma {
    /// 마지막으로 쓴 FF46 값.
    reg: u8,
    /// FF46 쓰기 후 시작까지 남은 M-사이클과 출발지. 진행 중인 전송은 시작할 때까지 계속된다.
    starting: Option<(u8, u16)>,
    /// 진행 중인 전송의 (출발지, 다음 바이트 번호). `Some`인 동안 CPU는 OAM에 접근할 수 없다.
    active: Option<(u16, u16)>,
}

pub struct Bus {
    cart: Cartridge,
    ppu: Ppu,
    timer: Timer,
    serial: Serial,
    joypad: Joypad,
    wram: Box<[u8; 0x2000]>,
    hram: [u8; 0x7F],
    /// 아직 구현하지 않은 I/O 레지스터(0xFF00–0xFF7F)는 쓴 값을 그대로 보관한다.
    io: [u8; 0x80],
    ie: u8,
    if_: u8,
    dma: OamDma,
    /// 시작 후 진행한 M-사이클 수.
    cycles: u64,
}

impl Bus {
    /// 부트 ROM이 끝난 직후의 DMG 상태 (Pan Docs "Power Up Sequence").
    pub fn new(cart: Cartridge) -> Self {
        Self {
            cart,
            ppu: Ppu::default(),
            // mooneye boot_div-dmgABCmgb로 맞춘 값: PC=0x0100에서 DIV 내부 카운터 위상.
            timer: Timer::new(0xABC8),
            serial: Serial::default(),
            joypad: Joypad::default(),
            wram: Box::new([0; 0x2000]),
            hram: [0; 0x7F],
            io: [0xFF; 0x80],
            ie: 0x00,
            if_: INT_VBLANK,
            dma: OamDma::default(),
            cycles: 0,
        }
    }

    pub fn cartridge(&self) -> &Cartridge {
        &self.cart
    }

    pub fn set_button(&mut self, button: Button, pressed: bool) {
        let irq = self.joypad.set_button(button, pressed);
        self.request(INT_JOYPAD, irq);
    }

    pub fn serial_output(&self) -> &[u8] {
        self.serial.output()
    }

    pub fn cycles(&self) -> u64 {
        self.cycles
    }

    pub fn ppu(&self) -> &Ppu {
        &self.ppu
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
            0xFE00..=0xFE9F if self.dma.active.is_some() => 0xFF,
            0xFE00..=0xFE9F => self.ppu.read_oam(addr),
            0xFEA0..=0xFEFF => 0x00,
            0xFF00..=0xFF7F => self.read_io(addr),
            0xFF80..=0xFFFE => self.hram[usize::from(addr - 0xFF80)],
            IE_ADDR => self.ie,
        }
    }

    fn read_io(&self, addr: u16) -> u8 {
        match addr {
            joypad::P1 => self.joypad.read(),
            serial::SB | serial::SC => self.serial.read(addr),
            timer::DIV..=timer::TAC => self.timer.read(addr),
            IF_ADDR => self.if_ | 0xE0,
            DMA => self.dma.reg,
            ppu::LCDC..=ppu::LYC | ppu::BGP..=ppu::WX => self.ppu.read_reg(addr),
            _ => self.io[usize::from(addr - 0xFF00)],
        }
    }

    fn write_io(&mut self, addr: u16, value: u8) {
        match addr {
            joypad::P1 => {
                let irq = self.joypad.write(value);
                self.request(INT_JOYPAD, irq);
            }
            serial::SB | serial::SC => {
                let irq = self.serial.write(addr, value);
                self.request(INT_SERIAL, irq);
            }
            timer::DIV..=timer::TAC => self.timer.write(addr, value),
            IF_ADDR => self.if_ = value & 0x1F,
            DMA => {
                self.dma.reg = value;
                self.dma.starting = Some((2, u16::from(value) << 8));
            }
            ppu::LCDC..=ppu::LYC | ppu::BGP..=ppu::WX => {
                let irq = self.ppu.write_reg(addr, value);
                self.if_ |= irq;
            }
            _ => self.io[usize::from(addr - 0xFF00)] = value,
        }
    }

    /// DMA가 출발지에서 읽는 값. 0xE000 이상은 WRAM 에코로 본다.
    fn dma_source_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => self.cart.read_rom(addr),
            0x8000..=0x9FFF => self.ppu.read_vram(addr),
            0xA000..=0xBFFF => self.cart.read_ram(addr),
            0xC000..=0xDFFF => self.wram[usize::from(addr - 0xC000)],
            _ => self.wram[usize::from((addr - 0xE000) & 0x1FFF)],
        }
    }

    /// 진행 중인 전송은 M-사이클마다 1바이트를 옮긴다. FF46 쓰기 후 2번째 M-사이클에 새 전송이 시작된다.
    fn tick_dma(&mut self) {
        if let Some((source, index)) = self.dma.active {
            let byte = self.dma_source_read(source.wrapping_add(index));
            self.ppu.write_oam(0xFE00 + index, byte);
            self.dma.active = (index + 1 < OAM_LEN).then_some((source, index + 1));
        }
        if let Some((delay, source)) = self.dma.starting {
            if delay <= 1 {
                self.dma.starting = None;
                self.dma.active = Some((source, 0));
            } else {
                self.dma.starting = Some((delay - 1, source));
            }
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
            0xFE00..=0xFE9F if self.dma.active.is_some() => {}
            0xFE00..=0xFE9F => self.ppu.write_oam(addr, value),
            0xFEA0..=0xFEFF => {}
            0xFF00..=0xFF7F => self.write_io(addr, value),
            0xFF80..=0xFFFE => self.hram[usize::from(addr - 0xFF80)] = value,
            IE_ADDR => self.ie = value,
        }
    }

    fn tick(&mut self) {
        self.cycles += 1;
        self.tick_dma();
        let timer_irq = self.timer.tick();
        self.request(INT_TIMER, timer_irq);
        self.if_ |= self.ppu.tick(4);
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
        b.write(joypad::P1, 0x20);
        assert_eq!(b.read(joypad::P1), 0xEF);
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

    /// WRAM 0xC100–0xC19F에 0..160을 채우고 OAM 0xFE00에 0x55를 둔 버스.
    fn bus_with_dma_source() -> Bus {
        let mut b = bus();
        for i in 0..0xA0u16 {
            b.write(0xC100 + i, i as u8);
        }
        b.write(0xFE00, 0x55);
        b
    }

    #[test]
    fn dma_register_reads_back() {
        let mut b = bus();
        b.write(0xFF46, 0xC1);
        assert_eq!(b.read(0xFF46), 0xC1);
    }

    #[test]
    fn dma_blocks_oam_from_second_cycle_until_transfer_ends() {
        let mut b = bus_with_dma_source();
        b.write(0xFF46, 0xC1);
        b.tick();
        assert_eq!(b.read(0xFE00), 0x55, "쓰기 직후 1 M-사이클은 접근 가능");
        b.tick();
        assert_eq!(b.read(0xFE00), 0xFF, "2번째 M-사이클부터 막힌다");
        for _ in 2..161 {
            b.tick();
        }
        assert_eq!(b.read(0xFE00), 0xFF, "161번째 M-사이클까지 막혀 있다");
        b.tick();
        assert_eq!((b.read(0xFE00), b.read(0xFE9F)), (0x00, 0x9F));
    }

    #[test]
    fn oam_writes_are_ignored_during_dma() {
        let mut b = bus_with_dma_source();
        b.write(0xFF46, 0xC1);
        b.tick();
        b.tick();
        b.write(0xFE10, 0xEE);
        for _ in 0..160 {
            b.tick();
        }
        assert_eq!(b.read(0xFE10), 0x10);
    }

    #[test]
    fn restarted_dma_keeps_oam_blocked() {
        let mut b = bus_with_dma_source();
        b.write(0xFF46, 0xC1);
        for _ in 0..10 {
            b.tick();
        }
        b.write(0xFF46, 0xC1);
        b.tick();
        assert_eq!(b.read(0xFE00), 0xFF);
        b.tick();
        assert_eq!(b.read(0xFE00), 0xFF);
    }

    #[test]
    fn dma_from_high_source_reads_echo_ram() {
        let mut b = bus();
        b.write(0xDE00, 0x77);
        b.write(0xFF46, 0xFE);
        for _ in 0..162 {
            b.tick();
        }
        assert_eq!(b.read(0xFE00), 0x77);
    }

    #[test]
    fn post_boot_div_phase_matches_dmg() {
        let mut b = bus();
        for _ in 0..13 {
            b.tick();
        }
        assert_eq!(b.read(timer::DIV), 0xAB);
        b.tick();
        assert_eq!(b.read(timer::DIV), 0xAC);
    }

    #[test]
    fn pressed_button_requests_joypad_interrupt() {
        let mut b = bus();
        b.write(IF_ADDR, 0);
        b.write(joypad::P1, 0x10);
        b.set_button(Button::A, true);
        assert_eq!(b.read(IF_ADDR) & INT_JOYPAD, INT_JOYPAD);
        assert_eq!(b.read(joypad::P1) & 0x0F, 0x0E);
    }
}
