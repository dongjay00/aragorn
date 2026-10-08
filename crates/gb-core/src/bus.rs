//! 메모리 버스: 주소 공간 매핑과 주변장치 진행 (Pan Docs "Memory Map").

use crate::apu::{self, Apu};
use crate::cartridge::Cartridge;
use crate::cpu::{CpuBus, IE_ADDR, IF_ADDR};
use crate::joypad::{self, Button, Joypad};
use crate::model::Model;
use crate::ppu::{self, Ppu};
use crate::serial::{self, Serial};
use crate::timer::{self, Timer};

pub const INT_VBLANK: u8 = 0x01;
pub const INT_STAT: u8 = 0x02;
pub const INT_TIMER: u8 = 0x04;
pub const INT_SERIAL: u8 = 0x08;
pub const INT_JOYPAD: u8 = 0x10;

const DMA: u16 = 0xFF46;
/// CGB: 속도 전환 준비(비트 0)와 현재 속도(비트 7).
pub const KEY1: u16 = 0xFF4D;
/// CGB: VRAM DMA 출발지(HDMA1–2), 도착지(HDMA3–4), 길이·모드·상태(HDMA5).
pub const HDMA1: u16 = 0xFF51;
pub const HDMA5: u16 = 0xFF55;
/// CGB: 오브젝트 우선순위 모드.
pub const OPRI: u16 = 0xFF6C;
/// CGB: 0xD000–0xDFFF에 보이는 WRAM 뱅크(1–7, 0은 1).
pub const SVBK: u16 = 0xFF70;
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

/// CGB VRAM DMA (Pan Docs "VRAM DMA Transfers").
#[derive(Debug, Clone, Default)]
struct Hdma {
    source: u16,
    /// VRAM 안의 오프셋(0x0000–0x1FF0).
    dest: u16,
    /// 남은 16바이트 블록 수 - 1. HDMA5를 읽으면 하위 7비트로 보인다.
    remaining: u8,
    /// HBlank 모드 전송이 진행 중인지.
    hblank: bool,
}

pub struct Bus {
    cgb: bool,
    cart: Cartridge,
    ppu: Ppu,
    apu: Apu,
    timer: Timer,
    serial: Serial,
    joypad: Joypad,
    /// 뱅크 0–7 (DMG는 0과 1만 쓴다).
    wram: Box<[u8; 0x8000]>,
    /// SVBK 하위 3비트.
    wram_bank: u8,
    hram: [u8; 0x7F],
    /// 아직 구현하지 않은 I/O 레지스터(0xFF00–0xFF7F)는 쓴 값을 그대로 보관한다.
    io: [u8; 0x80],
    ie: u8,
    if_: u8,
    dma: OamDma,
    hdma: Hdma,
    /// KEY1 비트 0: 다음 STOP에서 속도를 바꾼다.
    speed_switch_armed: bool,
    /// CGB 2배속. CPU와 타이머는 두 배로 빨라지고 PPU와 APU는 그대로다.
    double_speed: bool,
    opri: u8,
    /// 시작 후 진행한 M-사이클 수.
    cycles: u64,
    /// 시작 후 진행한 dot(PPU 클록) 수. 2배속에서는 M-사이클당 2다.
    dots: u64,
}

impl Bus {
    /// 부트 ROM이 끝난 직후 상태 (Pan Docs "Power Up Sequence"). `model`은 확정된 기기다.
    pub fn new(cart: Cartridge, model: Model) -> Self {
        let cgb = model == Model::Cgb;
        Self {
            cgb,
            cart,
            ppu: Ppu::new(cgb),
            apu: Apu::new(cgb),
            // mooneye boot_div-dmgABCmgb로 맞춘 값: PC=0x0100에서 DIV 내부 카운터 위상.
            timer: Timer::new(0xABC8),
            serial: Serial::default(),
            joypad: Joypad::default(),
            wram: Box::new([0; 0x8000]),
            wram_bank: 1,
            hram: [0; 0x7F],
            io: [0xFF; 0x80],
            ie: 0x00,
            if_: INT_VBLANK,
            dma: OamDma::default(),
            hdma: Hdma {
                remaining: 0x7F,
                ..Hdma::default()
            },
            speed_switch_armed: false,
            double_speed: false,
            opri: 0,
            cycles: 0,
            dots: 0,
        }
    }

    pub fn dots(&self) -> u64 {
        self.dots
    }

    /// 0xD000–0xDFFF(와 에코)가 가리키는 WRAM 오프셋.
    fn wram_index(&self, addr: u16) -> usize {
        let offset = usize::from(addr & 0x0FFF);
        if addr & 0x1000 == 0 {
            offset
        } else {
            usize::from(self.wram_bank.max(1)) * 0x1000 + offset
        }
    }

    pub fn cartridge(&self) -> &Cartridge {
        &self.cart
    }

    pub fn cartridge_mut(&mut self) -> &mut Cartridge {
        &mut self.cart
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

    pub fn apu_mut(&mut self) -> &mut Apu {
        &mut self.apu
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
            0xC000..=0xFDFF => self.wram[self.wram_index(addr)],
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
            KEY1 if self.cgb => {
                0x7E | u8::from(self.double_speed) << 7 | u8::from(self.speed_switch_armed)
            }
            HDMA5 if self.cgb => u8::from(!self.hdma.hblank) << 7 | self.hdma.remaining,
            OPRI if self.cgb => 0xFE | self.opri,
            SVBK if self.cgb => 0xF8 | self.wram_bank,
            ppu::VBK | ppu::BCPS..=ppu::OCPD if self.cgb => self.ppu.read_reg(addr),
            apu::PCM12 | apu::PCM34 if self.cgb => self.apu.read(addr),
            apu::NR10..=apu::WAVE_RAM_END => self.apu.read(addr),
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
            timer::DIV..=timer::TAC => {
                let before = self.timer.apu_clock_bit(self.double_speed);
                self.timer.write(addr, value);
                if before && !self.timer.apu_clock_bit(self.double_speed) {
                    self.apu.frame_sequencer();
                }
            }
            apu::NR10..=apu::WAVE_RAM_END => self.apu.write(addr, value),
            IF_ADDR => self.if_ = value & 0x1F,
            KEY1 if self.cgb => self.speed_switch_armed = value & 0x01 != 0,
            HDMA1..=HDMA5 if self.cgb => self.write_hdma(addr, value),
            OPRI if self.cgb => self.opri = value & 0x01,
            SVBK if self.cgb => self.wram_bank = value & 0x07,
            ppu::VBK | ppu::BCPS..=ppu::OCPD if self.cgb => {
                self.ppu.write_reg(addr, value);
            }
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
            _ => self.wram[self.wram_index(addr)],
        }
    }

    fn write_hdma(&mut self, addr: u16, value: u8) {
        let h = &mut self.hdma;
        match addr {
            0xFF51 => h.source = u16::from(value) << 8 | (h.source & 0x00F0),
            0xFF52 => h.source = (h.source & 0xFF00) | u16::from(value & 0xF0),
            0xFF53 => h.dest = u16::from(value & 0x1F) << 8 | (h.dest & 0x00F0),
            0xFF54 => h.dest = (h.dest & 0x1F00) | u16::from(value & 0xF0),
            _ => {
                if h.hblank && value & 0x80 == 0 {
                    // 진행 중인 HBlank 전송을 멈춘다. 남은 길이는 그대로 읽힌다.
                    h.hblank = false;
                    return;
                }
                h.remaining = value & 0x7F;
                if value & 0x80 != 0 {
                    h.hblank = true;
                } else {
                    // 범용 전송: 한 번에 모두 옮기고, 16바이트마다 CPU가 8 M-사이클(2배속은 16) 멈춘다.
                    let blocks = u32::from(h.remaining) + 1;
                    for _ in 0..blocks {
                        self.hdma_block();
                    }
                    let stall = blocks * if self.double_speed { 16 } else { 8 };
                    for _ in 0..stall {
                        self.tick();
                    }
                }
            }
        }
    }

    /// 16바이트를 VRAM으로 옮긴다. 마지막 블록이면 전송을 끝낸다(HDMA5가 0xFF로 읽힌다).
    fn hdma_block(&mut self) {
        for i in 0..16 {
            let byte = self.dma_source_read(self.hdma.source.wrapping_add(i));
            let dest = 0x8000 | ((self.hdma.dest + i) & 0x1FFF);
            self.ppu.write_vram(dest, byte);
        }
        self.hdma.source = self.hdma.source.wrapping_add(16);
        self.hdma.dest = (self.hdma.dest + 16) & 0x1FF0;
        if self.hdma.remaining == 0 {
            self.hdma.remaining = 0x7F;
            self.hdma.hblank = false;
        } else {
            self.hdma.remaining -= 1;
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
            0xC000..=0xFDFF => {
                let i = self.wram_index(addr);
                self.wram[i] = value;
            }
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
        let dots = if self.double_speed { 2 } else { 4 };
        self.dots += u64::from(dots);
        self.tick_dma();
        let before = self.timer.apu_clock_bit(self.double_speed);
        let timer_irq = self.timer.tick();
        self.request(INT_TIMER, timer_irq);
        if before && !self.timer.apu_clock_bit(self.double_speed) {
            self.apu.frame_sequencer();
        }
        self.apu.tick(dots);
        self.if_ |= self.ppu.tick(dots);
        if self.ppu.take_hblank_started() && self.hdma.hblank {
            self.hdma_block();
        }
    }

    /// STOP. CGB에서 KEY1로 준비했으면 속도를 바꾸고 DIV를 지운다.
    fn stop(&mut self) {
        if self.cgb && self.speed_switch_armed {
            self.speed_switch_armed = false;
            self.double_speed = !self.double_speed;
            self.write_io(timer::DIV, 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::test_rom;

    fn bus() -> Bus {
        let mut rom = test_rom(0x00, 0x00, 0x00);
        rom[0x0200] = 0xAB;
        Bus::new(Cartridge::new(rom).unwrap(), Model::Dmg)
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

    #[test]
    fn apu_registers_are_mapped() {
        let mut b = bus();
        assert_eq!(b.read(apu::NR52), 0xF1);
        b.write(0xFF30, 0x5A);
        assert_eq!(b.read(0xFF30), 0x5A);
    }

    /// 채널 2를 길이 1로 켠다. 다음 길이 클록에 꺼진다.
    fn bus_with_short_channel_2() -> Bus {
        let mut b = bus();
        b.write(timer::DIV, 0);
        b.write(0xFF17, 0xF0);
        b.write(0xFF16, 0x3F);
        b.write(0xFF19, 0xC0);
        assert_eq!(b.read(apu::NR52) & 0x02, 0x02);
        b
    }

    #[test]
    fn div_bit_4_falling_edge_clocks_frame_sequencer() {
        let mut b = bus_with_short_channel_2();
        // DIV 비트 4(카운터 비트 12)는 8192 T-사이클(2048 M-사이클)마다 1→0으로 떨어진다.
        for _ in 0..2047 {
            b.tick();
        }
        assert_eq!(b.read(apu::NR52) & 0x02, 0x02);
        b.tick();
        assert_eq!(b.read(apu::NR52) & 0x02, 0x00);
    }

    #[test]
    fn div_reset_while_bit_4_is_set_clocks_frame_sequencer() {
        let mut b = bus_with_short_channel_2();
        for _ in 0..1024 {
            b.tick();
        }
        assert_eq!(b.read(apu::NR52) & 0x02, 0x02);
        b.write(timer::DIV, 0);
        assert_eq!(b.read(apu::NR52) & 0x02, 0x00);
    }

    fn cgb_bus() -> Bus {
        Bus::new(
            Cartridge::new(test_rom(0x00, 0x00, 0xC0)).unwrap(),
            Model::Cgb,
        )
    }

    #[test]
    fn cgb_switches_wram_banks() {
        let mut b = cgb_bus();
        b.write(0xD000, 1);
        b.write(SVBK, 2);
        assert_eq!(b.read(SVBK), 0xFA);
        assert_eq!(b.read(0xD000), 0);
        b.write(0xD000, 2);
        b.write(0xC000, 9);
        b.write(SVBK, 0);
        assert_eq!(b.read(0xD000), 1, "뱅크 0은 1로 본다");
        b.write(SVBK, 2);
        assert_eq!(
            (b.read(0xF000), b.read(0xC000)),
            (2, 9),
            "에코도 뱅크를 따른다"
        );
    }

    #[test]
    fn dmg_ignores_cgb_registers() {
        let mut b = bus();
        b.write(0xD000, 1);
        b.write(SVBK, 2);
        assert_eq!(b.read(0xD000), 1);
        assert_eq!(b.read(KEY1), 0xFF);
    }

    #[test]
    fn armed_stop_switches_to_double_speed() {
        let mut b = cgb_bus();
        assert_eq!(b.read(KEY1), 0x7E);
        b.write(KEY1, 0x01);
        assert_eq!(b.read(KEY1), 0x7F);
        b.stop();
        assert_eq!(b.read(KEY1), 0xFE);
        assert_eq!(b.read(timer::DIV), 0, "DIV를 지운다");
        // 2배속에서는 M-사이클당 2 dot이라 한 줄(456 dot)에 228 M-사이클이 걸린다.
        for _ in 0..228 {
            b.tick();
        }
        assert_eq!((b.read(ppu::LY), b.dots()), (1, 456));
        b.write(KEY1, 0x01);
        b.stop();
        assert_eq!(b.read(KEY1), 0x7E, "다시 보통 속도");
    }

    #[test]
    fn unarmed_stop_keeps_speed() {
        let mut b = cgb_bus();
        b.stop();
        assert_eq!(b.read(KEY1), 0x7E);
    }

    /// WRAM 0xC000부터 0, 1, 2, …를 채운 CGB 버스.
    fn cgb_bus_with_hdma_source() -> Bus {
        let mut b = cgb_bus();
        for i in 0..0x40u16 {
            b.write(0xC000 + i, i as u8);
        }
        b.write(HDMA1, 0xC0);
        b.write(0xFF52, 0x00);
        b.write(0xFF53, 0x81);
        b.write(0xFF54, 0x00);
        b
    }

    #[test]
    fn general_hdma_copies_all_blocks_at_once() {
        let mut b = cgb_bus_with_hdma_source();
        b.write(HDMA5, 0x01); // 2블록
        assert_eq!((b.read(0x8100), b.read(0x811F)), (0x00, 0x1F));
        assert_eq!(b.read(0x8120), 0x00, "3번째 블록은 옮기지 않는다");
        assert_eq!(b.read(HDMA5), 0xFF, "끝나면 0xFF");
    }

    #[test]
    fn hblank_hdma_copies_one_block_per_hblank() {
        let mut b = cgb_bus_with_hdma_source();
        b.write(HDMA5, 0x81); // HBlank 모드 2블록
        assert_eq!(b.read(HDMA5), 0x01);
        // 0번 줄 HBlank는 252 dot(63 M-사이클)에 시작한다.
        for _ in 0..63 {
            b.tick();
        }
        assert_eq!((b.read(0x810F), b.read(0x8110)), (0x0F, 0x00));
        assert_eq!(b.read(HDMA5), 0x00);
        for _ in 0..114 {
            b.tick();
        }
        assert_eq!(b.read(0x811F), 0x1F);
        assert_eq!(b.read(HDMA5), 0xFF);
    }

    #[test]
    fn hblank_hdma_can_be_cancelled() {
        let mut b = cgb_bus_with_hdma_source();
        b.write(HDMA5, 0x83);
        b.write(HDMA5, 0x00);
        assert_eq!(
            b.read(HDMA5),
            0x83,
            "멈추면 비트 7이 켜지고 남은 길이가 보인다"
        );
        for _ in 0..63 {
            b.tick();
        }
        assert_eq!(b.read(0x8100), 0x00, "멈춘 뒤에는 옮기지 않는다");
    }
}
