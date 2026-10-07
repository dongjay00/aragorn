//! PPU (Pan Docs "Rendering", "LCD Control", "LCD Status", "Palettes", "OAM").
//!
//! 한 줄은 456 dot이다. 0–143번 줄은 모드 2(0–79) → 모드 3(80–251, 고정) → 모드 0(252–455),
//! 144–153번 줄은 모드 1이다. 모드 3에 들어갈 때 그 줄 전체를 한 번에 그린다(스캔라인 렌더러).

pub const LCDC: u16 = 0xFF40;
pub const STAT: u16 = 0xFF41;
pub const SCY: u16 = 0xFF42;
pub const SCX: u16 = 0xFF43;
pub const LY: u16 = 0xFF44;
pub const LYC: u16 = 0xFF45;
pub const BGP: u16 = 0xFF47;
pub const OBP0: u16 = 0xFF48;
pub const OBP1: u16 = 0xFF49;
pub const WY: u16 = 0xFF4A;
pub const WX: u16 = 0xFF4B;

pub const DOTS_PER_LINE: u32 = 456;
pub const LINES_PER_FRAME: u8 = 154;
pub const DOTS_PER_FRAME: u32 = DOTS_PER_LINE * LINES_PER_FRAME as u32;

/// PPU가 요청하는 인터럽트 (IF 비트와 같은 값).
pub const IRQ_VBLANK: u8 = 0x01;
pub const IRQ_STAT: u8 = 0x02;

const VBLANK_LINE: u8 = 144;
const MODE3_START: u32 = 80;
const MODE0_START: u32 = 252;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    HBlank = 0,
    VBlank = 1,
    OamScan = 2,
    Drawing = 3,
}

#[derive(Debug, Clone)]
pub struct Ppu {
    vram: Box<[u8; 0x2000]>,
    oam: Box<[u8; 0xA0]>,
    lcdc: u8,
    /// STAT의 쓰기 가능한 비트(3–6). 모드와 LYC 일치 비트는 읽을 때 만든다.
    stat: u8,
    scy: u8,
    scx: u8,
    ly: u8,
    lyc: u8,
    bgp: u8,
    obp0: u8,
    obp1: u8,
    wy: u8,
    wx: u8,
    mode: Mode,
    /// 현재 줄(LCD가 꺼져 있으면 현재 프레임)에서 지난 dot 수.
    dot: u32,
    /// LY == LYC 비교 결과(STAT 비트 2). LCD가 켜져 있을 때만 갱신되고, 꺼지면 마지막 값을 유지한다.
    lyc_match: bool,
    /// STAT 인터럽트 신호. 0→1로 바뀔 때만 인터럽트를 요청한다.
    stat_line: bool,
    frame_ready: bool,
}

impl Default for Ppu {
    fn default() -> Self {
        Self {
            vram: Box::new([0; 0x2000]),
            oam: Box::new([0; 0xA0]),
            lcdc: 0x91,
            stat: 0,
            scy: 0,
            scx: 0,
            ly: 0,
            lyc: 0,
            bgp: 0xFC,
            obp0: 0xFF,
            obp1: 0xFF,
            wy: 0,
            wx: 0,
            mode: Mode::OamScan,
            dot: 0,
            lyc_match: true,
            stat_line: false,
            frame_ready: false,
        }
    }
}

impl Ppu {
    pub fn read_vram(&self, addr: u16) -> u8 {
        self.vram[usize::from(addr & 0x1FFF)]
    }

    pub fn write_vram(&mut self, addr: u16, value: u8) {
        self.vram[usize::from(addr & 0x1FFF)] = value;
    }

    /// `addr`는 0xFE00–0xFE9F (버스가 보장한다).
    pub fn read_oam(&self, addr: u16) -> u8 {
        self.oam[usize::from(addr - 0xFE00)]
    }

    pub fn write_oam(&mut self, addr: u16, value: u8) {
        self.oam[usize::from(addr - 0xFE00)] = value;
    }

    pub fn read_reg(&self, addr: u16) -> u8 {
        match addr {
            LCDC => self.lcdc,
            STAT => {
                let mode = if self.lcd_on() { self.mode as u8 } else { 0 };
                0x80 | self.stat | (u8::from(self.lyc_match) << 2) | mode
            }
            SCY => self.scy,
            SCX => self.scx,
            LY => self.ly,
            LYC => self.lyc,
            BGP => self.bgp,
            OBP0 => self.obp0,
            OBP1 => self.obp1,
            WY => self.wy,
            WX => self.wx,
            _ => 0xFF,
        }
    }

    /// 레지스터 쓰기. STAT 신호가 새로 켜지면 STAT 인터럽트 비트를 반환한다.
    pub fn write_reg(&mut self, addr: u16, value: u8) -> u8 {
        match addr {
            LCDC => {
                let was_on = self.lcd_on();
                self.lcdc = value;
                if was_on && !self.lcd_on() {
                    self.ly = 0;
                    self.dot = 0;
                    self.mode = Mode::HBlank;
                } else if !was_on && self.lcd_on() {
                    self.ly = 0;
                    self.dot = 0;
                    self.mode = Mode::OamScan;
                }
            }
            STAT => self.stat = value & 0x78,
            SCY => self.scy = value,
            SCX => self.scx = value,
            LYC => self.lyc = value,
            BGP => self.bgp = value,
            OBP0 => self.obp0 = value,
            OBP1 => self.obp1 = value,
            WY => self.wy = value,
            WX => self.wx = value,
            // LY는 읽기 전용이다.
            _ => {}
        }
        if self.lcd_on() {
            self.lyc_match = self.ly == self.lyc;
        }
        self.update_stat_line()
    }

    fn lcd_on(&self) -> bool {
        self.lcdc & 0x80 != 0
    }

    /// STAT 신호를 다시 계산해 0→1로 바뀌었으면 `IRQ_STAT`을 반환한다.
    fn update_stat_line(&mut self) -> u8 {
        // DMG는 144번 줄이 시작될 때 모드 2 조건도 한 번 켠다.
        let oam_condition = self.mode == Mode::OamScan || (self.ly == VBLANK_LINE && self.dot == 0);
        let line = (self.stat & 0x40 != 0 && self.lyc_match)
            || (self.lcd_on()
                && ((self.stat & 0x08 != 0 && self.mode == Mode::HBlank)
                    || (self.stat & 0x10 != 0 && self.mode == Mode::VBlank)
                    || (self.stat & 0x20 != 0 && oam_condition)));
        let rising = line && !self.stat_line;
        self.stat_line = line;
        if rising { IRQ_STAT } else { 0 }
    }

    /// `dots` T-사이클(4의 배수) 진행한다. 요청할 인터럽트 비트(`IRQ_VBLANK`, `IRQ_STAT`)를 반환한다.
    pub fn tick(&mut self, dots: u32) -> u8 {
        if !self.lcd_on() {
            self.dot += dots;
            if self.dot >= DOTS_PER_FRAME {
                self.dot -= DOTS_PER_FRAME;
                self.frame_ready = true;
            }
            return 0;
        }
        let mut irq = 0;
        for _ in 0..dots / 4 {
            irq |= self.step4();
        }
        irq
    }

    /// 4 dot 진행한다. 모드 경계(80, 252, 456)는 모두 4의 배수다.
    fn step4(&mut self) -> u8 {
        self.dot += 4;
        let mut irq = 0;
        if self.ly < VBLANK_LINE {
            match self.dot {
                MODE3_START => self.mode = Mode::Drawing,
                MODE0_START => self.mode = Mode::HBlank,
                _ => {}
            }
        }
        if self.dot == DOTS_PER_LINE {
            self.dot = 0;
            self.ly = (self.ly + 1) % LINES_PER_FRAME;
            self.lyc_match = self.ly == self.lyc;
            if self.ly == VBLANK_LINE {
                self.mode = Mode::VBlank;
                self.frame_ready = true;
                irq |= IRQ_VBLANK;
            } else if self.ly < VBLANK_LINE {
                self.mode = Mode::OamScan;
            }
        }
        irq | self.update_stat_line()
    }

    /// 프레임 경계를 지났으면 `true`. 읽으면 초기화된다.
    pub fn take_frame_ready(&mut self) -> bool {
        std::mem::take(&mut self.frame_ready)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `m_cycles`번 4 dot씩 진행하고 요청된 인터럽트 비트를 모두 OR해서 반환한다.
    fn ticks(ppu: &mut Ppu, m_cycles: u32) -> u8 {
        (0..m_cycles).fold(0, |irq, _| irq | ppu.tick(4))
    }

    fn mode(ppu: &Ppu) -> u8 {
        ppu.read_reg(STAT) & 0x03
    }

    /// LCD 켜짐, BG 켜짐, 0x8000 타일 데이터, BGP=0xE4(색 번호 = 음영)인 PPU.
    fn ppu() -> Ppu {
        let mut p = Ppu::default();
        p.write_reg(LCDC, 0x91);
        p.write_reg(BGP, 0xE4);
        p.write_reg(OBP0, 0xE4);
        p
    }

    #[test]
    fn ly_advances_every_456_dots() {
        let mut p = ppu();
        ticks(&mut p, 113);
        assert_eq!(p.read_reg(LY), 0);
        ticks(&mut p, 1);
        assert_eq!(p.read_reg(LY), 1);
    }

    #[test]
    fn modes_follow_line_timing() {
        let mut p = ppu();
        assert_eq!(mode(&p), 2);
        ticks(&mut p, 20);
        assert_eq!(mode(&p), 3);
        ticks(&mut p, 43);
        assert_eq!(mode(&p), 0);
        ticks(&mut p, 51);
        assert_eq!((p.read_reg(LY), mode(&p)), (1, 2));
    }

    #[test]
    fn vblank_starts_at_line_144() {
        let mut p = ppu();
        assert_eq!(ticks(&mut p, 114 * 144 - 1) & IRQ_VBLANK, 0);
        assert!(!p.take_frame_ready());
        assert_eq!(ticks(&mut p, 1) & IRQ_VBLANK, IRQ_VBLANK);
        assert_eq!((p.read_reg(LY), mode(&p)), (144, 1));
        assert!(p.take_frame_ready());
        assert!(!p.take_frame_ready());
    }

    #[test]
    fn ly_wraps_after_line_153() {
        let mut p = ppu();
        ticks(&mut p, 114 * 154);
        assert_eq!((p.read_reg(LY), mode(&p)), (0, 2));
    }

    #[test]
    fn lcd_off_resets_ly_and_still_paces_frames() {
        let mut p = ppu();
        ticks(&mut p, 114 * 10);
        p.write_reg(LCDC, 0x11);
        assert_eq!((p.read_reg(LY), mode(&p)), (0, 0));
        assert_eq!(ticks(&mut p, DOTS_PER_FRAME / 4 - 1), 0);
        assert!(!p.take_frame_ready());
        ticks(&mut p, 1);
        assert!(p.take_frame_ready());
    }

    #[test]
    fn ly_is_read_only_and_stat_bit7_reads_one() {
        let mut p = ppu();
        p.write_reg(LY, 99);
        p.write_reg(STAT, 0xFF);
        assert_eq!(p.read_reg(LY), 0);
        assert_eq!(p.read_reg(STAT) & 0xF8, 0xF8);
    }

    #[test]
    fn hblank_stat_interrupt_fires_on_rising_edge_only() {
        let mut p = ppu();
        p.write_reg(STAT, 0x08);
        assert_eq!(ticks(&mut p, 62), 0);
        assert_eq!(ticks(&mut p, 1), IRQ_STAT);
        assert_eq!(ticks(&mut p, 50), 0);
    }

    #[test]
    fn lyc_match_sets_flag_and_requests_stat_interrupt() {
        let mut p = ppu();
        p.write_reg(LYC, 1);
        p.write_reg(STAT, 0x40);
        assert_eq!(p.read_reg(STAT) & 0x04, 0);
        assert_eq!(ticks(&mut p, 114), IRQ_STAT);
        assert_eq!(p.read_reg(STAT) & 0x04, 0x04);
    }

    #[test]
    fn writing_lyc_to_current_line_requests_stat_interrupt() {
        let mut p = ppu();
        p.write_reg(LYC, 5);
        p.write_reg(STAT, 0x40);
        assert_eq!(p.write_reg(LYC, 0), IRQ_STAT);
    }

    #[test]
    fn line_144_also_raises_mode2_stat_interrupt() {
        let mut p = ppu();
        p.write_reg(STAT, 0x20);
        ticks(&mut p, 114 * 144 - 1);
        assert_eq!(ticks(&mut p, 1), IRQ_VBLANK | IRQ_STAT);
    }

    #[test]
    fn lyc_flag_is_kept_while_lcd_off() {
        let mut p = ppu();
        assert_eq!(p.read_reg(STAT) & 0x04, 0x04);
        p.write_reg(LCDC, 0x11);
        p.write_reg(LYC, 5);
        assert_eq!(p.read_reg(STAT) & 0x04, 0x04);
        p.write_reg(LCDC, 0x91);
        assert_eq!(p.read_reg(STAT) & 0x04, 0x00);
    }

    #[test]
    fn vram_and_oam_store_bytes() {
        let mut p = Ppu::default();
        p.write_vram(0x8000, 1);
        p.write_vram(0x9FFF, 2);
        p.write_oam(0xFE00, 3);
        p.write_oam(0xFE9F, 4);
        assert_eq!(
            (
                p.read_vram(0x8000),
                p.read_vram(0x9FFF),
                p.read_oam(0xFE00),
                p.read_oam(0xFE9F)
            ),
            (1, 2, 3, 4)
        );
    }
}
