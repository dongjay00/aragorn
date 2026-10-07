//! PPU. M1에서는 VRAM/OAM 저장, LCDC, LY 타이밍, VBlank 인터럽트만 구현한다.
//! 모드, STAT, 렌더링은 M3에서 구현한다.

pub const LCDC: u16 = 0xFF40;
pub const LY: u16 = 0xFF44;

pub const DOTS_PER_LINE: u32 = 456;
pub const LINES_PER_FRAME: u8 = 154;
pub const DOTS_PER_FRAME: u32 = DOTS_PER_LINE * LINES_PER_FRAME as u32;
const VBLANK_LINE: u8 = 144;

#[derive(Debug, Clone)]
pub struct Ppu {
    vram: Box<[u8; 0x2000]>,
    oam: Box<[u8; 0xA0]>,
    lcdc: u8,
    ly: u8,
    /// 현재 줄(LCD가 꺼져 있으면 현재 프레임)에서 지난 dot 수.
    dot: u32,
    frame_ready: bool,
}

impl Default for Ppu {
    fn default() -> Self {
        Self {
            vram: Box::new([0; 0x2000]),
            oam: Box::new([0; 0xA0]),
            lcdc: 0x91,
            ly: 0,
            dot: 0,
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
            LY => self.ly,
            _ => 0xFF,
        }
    }

    /// LCD를 켜거나 끄면 LY와 dot이 0부터 다시 시작한다. LY는 읽기 전용이다.
    pub fn write_reg(&mut self, addr: u16, value: u8) {
        if addr == LCDC {
            let was_on = self.lcd_on();
            self.lcdc = value;
            if was_on != self.lcd_on() {
                self.ly = 0;
                self.dot = 0;
            }
        }
    }

    fn lcd_on(&self) -> bool {
        self.lcdc & 0x80 != 0
    }

    /// `dots` T-사이클 진행한다. VBlank에 진입하면 `true` (VBlank 인터럽트 요청).
    pub fn tick(&mut self, dots: u32) -> bool {
        self.dot += dots;
        if !self.lcd_on() {
            if self.dot >= DOTS_PER_FRAME {
                self.dot -= DOTS_PER_FRAME;
                self.frame_ready = true;
            }
            return false;
        }
        if self.dot < DOTS_PER_LINE {
            return false;
        }
        self.dot -= DOTS_PER_LINE;
        self.ly = (self.ly + 1) % LINES_PER_FRAME;
        let vblank = self.ly == VBLANK_LINE;
        self.frame_ready |= vblank;
        vblank
    }

    /// 프레임 경계를 지났으면 `true`. 읽으면 초기화된다.
    pub fn take_frame_ready(&mut self) -> bool {
        std::mem::take(&mut self.frame_ready)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `m_cycles`번 4 dot씩 진행하고 VBlank 인터럽트 요청 횟수를 반환한다.
    fn ticks(ppu: &mut Ppu, m_cycles: u32) -> usize {
        (0..m_cycles).filter(|_| ppu.tick(4)).count()
    }

    #[test]
    fn ly_advances_every_456_dots() {
        let mut p = Ppu::default();
        ticks(&mut p, 113);
        assert_eq!(p.read_reg(LY), 0);
        ticks(&mut p, 1);
        assert_eq!(p.read_reg(LY), 1);
    }

    #[test]
    fn vblank_starts_at_line_144() {
        let mut p = Ppu::default();
        assert_eq!(ticks(&mut p, 114 * 144 - 1), 0);
        assert!(!p.take_frame_ready());
        assert_eq!(ticks(&mut p, 1), 1);
        assert_eq!(p.read_reg(LY), 144);
        assert!(p.take_frame_ready());
        assert!(!p.take_frame_ready());
    }

    #[test]
    fn ly_wraps_after_line_153() {
        let mut p = Ppu::default();
        ticks(&mut p, 114 * 154);
        assert_eq!(p.read_reg(LY), 0);
    }

    #[test]
    fn lcd_off_resets_ly_and_still_paces_frames() {
        let mut p = Ppu::default();
        ticks(&mut p, 114 * 10);
        p.write_reg(LCDC, 0x11);
        assert_eq!(p.read_reg(LY), 0);
        assert_eq!(ticks(&mut p, DOTS_PER_FRAME / 4 - 1), 0);
        assert!(!p.take_frame_ready());
        ticks(&mut p, 1);
        assert_eq!(p.read_reg(LY), 0);
        assert!(p.take_frame_ready());
    }

    #[test]
    fn ly_is_read_only() {
        let mut p = Ppu::default();
        p.write_reg(LY, 99);
        assert_eq!(p.read_reg(LY), 0);
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
        assert_eq!(p.read_reg(LCDC), 0x91);
    }
}
