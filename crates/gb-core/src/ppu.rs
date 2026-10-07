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

pub const SCREEN_WIDTH: usize = 160;
pub const SCREEN_HEIGHT: usize = 144;
pub const DOTS_PER_LINE: u32 = 456;
pub const LINES_PER_FRAME: u8 = 154;
pub const DOTS_PER_FRAME: u32 = DOTS_PER_LINE * LINES_PER_FRAME as u32;

/// PPU가 요청하는 인터럽트 (IF 비트와 같은 값).
pub const IRQ_VBLANK: u8 = 0x01;
pub const IRQ_STAT: u8 = 0x02;

const VBLANK_LINE: u8 = 144;
const MODE3_START: u32 = 80;
const MODE0_START: u32 = 252;

/// DMG 기본 팔레트(밝은 색부터). 각 값은 0xRRGGBBAA.
pub const DEFAULT_DMG_PALETTE: [u32; 4] = [0xE0F8D0FF, 0x88C070FF, 0x346856FF, 0x081820FF];

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
    /// 이번 프레임에서 LY == WY인 줄을 지났는지 (윈도우 표시 조건).
    wy_triggered: bool,
    /// 윈도우 내부 줄 카운터. 윈도우를 그린 줄에서만 증가한다.
    window_line: u8,
    /// LY == LYC 비교 결과(STAT 비트 2). LCD가 켜져 있을 때만 갱신되고, 꺼지면 마지막 값을 유지한다.
    lyc_match: bool,
    /// STAT 인터럽트 신호. 0→1로 바뀔 때만 인터럽트를 요청한다.
    stat_line: bool,
    frame_ready: bool,
    palette: [u32; 4],
    framebuffer: Box<[u32; SCREEN_WIDTH * SCREEN_HEIGHT]>,
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
            wy_triggered: false,
            window_line: 0,
            lyc_match: true,
            stat_line: false,
            frame_ready: false,
            palette: DEFAULT_DMG_PALETTE,
            framebuffer: Box::new([DEFAULT_DMG_PALETTE[0]; SCREEN_WIDTH * SCREEN_HEIGHT]),
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
                    self.framebuffer.fill(self.palette[0]);
                } else if !was_on && self.lcd_on() {
                    self.ly = 0;
                    self.dot = 0;
                    self.mode = Mode::OamScan;
                    self.wy_triggered = false;
                    self.window_line = 0;
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

    pub fn set_palette(&mut self, palette: [u32; 4]) {
        self.palette = palette;
    }

    /// 160×144, 각 픽셀은 0xRRGGBBAA.
    pub fn framebuffer(&self) -> &[u32] {
        &self.framebuffer[..]
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
                MODE3_START => {
                    self.mode = Mode::Drawing;
                    self.render_line();
                }
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
                if self.ly == 0 {
                    self.wy_triggered = false;
                    self.window_line = 0;
                }
                self.mode = Mode::OamScan;
            }
        }
        irq | self.update_stat_line()
    }

    /// 프레임 경계를 지났으면 `true`. 읽으면 초기화된다.
    pub fn take_frame_ready(&mut self) -> bool {
        std::mem::take(&mut self.frame_ready)
    }

    fn tile_row(&self, tile_addr: usize, row: usize) -> (u8, u8) {
        let i = (tile_addr + row * 2) & 0x1FFF;
        (self.vram[i], self.vram[i + 1])
    }

    /// BG/윈도우 타일 번호가 가리키는 타일 데이터 주소 (VRAM 기준 오프셋).
    fn bg_tile_addr(&self, index: u8) -> usize {
        if self.lcdc & 0x10 != 0 {
            usize::from(index) * 16
        } else {
            (0x1000 + i32::from(index as i8) * 16) as usize
        }
    }

    fn render_line(&mut self) {
        if self.ly == self.wy {
            self.wy_triggered = true;
        }
        let y = usize::from(self.ly);
        // BG/윈도우의 색 번호(0–3). 스프라이트 우선순위 판정에 쓴다.
        let mut bg_color = [0u8; SCREEN_WIDTH];
        let window_visible = self.lcdc & 0x20 != 0 && self.wy_triggered && self.wx <= 166;
        let mut window_drawn = false;
        if self.lcdc & 0x01 != 0 {
            for (x, color) in bg_color.iter_mut().enumerate() {
                let in_window = window_visible && x + 7 >= usize::from(self.wx);
                let (map, px, py) = if in_window {
                    window_drawn = true;
                    let map = if self.lcdc & 0x40 != 0 {
                        0x1C00
                    } else {
                        0x1800
                    };
                    (
                        map,
                        x + 7 - usize::from(self.wx),
                        usize::from(self.window_line),
                    )
                } else {
                    let map = if self.lcdc & 0x08 != 0 {
                        0x1C00
                    } else {
                        0x1800
                    };
                    (
                        map,
                        (x + usize::from(self.scx)) & 0xFF,
                        (y + usize::from(self.scy)) & 0xFF,
                    )
                };
                let index = self.vram[map + (py / 8) * 32 + px / 8];
                let (lo, hi) = self.tile_row(self.bg_tile_addr(index), py % 8);
                let bit = 7 - (px % 8);
                *color = (((hi >> bit) & 1) << 1) | ((lo >> bit) & 1);
            }
        }
        if window_drawn {
            self.window_line = self.window_line.wrapping_add(1);
        }
        let row = &mut self.framebuffer[y * SCREEN_WIDTH..(y + 1) * SCREEN_WIDTH];
        for (pixel, &color) in row.iter_mut().zip(&bg_color) {
            *pixel = self.palette[usize::from((self.bgp >> (color * 2)) & 3)];
        }
        if self.lcdc & 0x02 != 0 {
            self.render_sprites(y, &bg_color);
        }
    }

    fn render_sprites(&mut self, y: usize, bg_color: &[u8; SCREEN_WIDTH]) {
        let height = if self.lcdc & 0x04 != 0 { 16 } else { 8 };
        // 줄당 최대 10개, OAM 순서대로 고른다.
        let mut sprites: Vec<(usize, &[u8; 4])> = self
            .oam
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                let top = i32::from(s[0]) - 16;
                (top..top + height).contains(&(y as i32))
            })
            .take(10)
            .collect();
        // DMG: X 좌표가 작은 것이 우선이고, 같으면 OAM 앞쪽이 우선이다.
        sprites.sort_by_key(|&(i, s)| (s[1], i));
        for (x, &bg) in bg_color.iter().enumerate() {
            for &(_, s) in &sprites {
                let left = i32::from(s[1]) - 8;
                let col = x as i32 - left;
                if !(0..8).contains(&col) {
                    continue;
                }
                let attr = s[3];
                let mut line = y as i32 - (i32::from(s[0]) - 16);
                if attr & 0x40 != 0 {
                    line = height - 1 - line;
                }
                let tile = if height == 16 { s[2] & 0xFE } else { s[2] };
                let (lo, hi) = self.tile_row(usize::from(tile) * 16, line as usize);
                let bit = if attr & 0x20 != 0 { col } else { 7 - col };
                let color = (((hi >> bit) & 1) << 1) | ((lo >> bit) & 1);
                if color == 0 {
                    continue;
                }
                // 우선순위가 가장 높은 불투명 스프라이트 픽셀만 본다. BG 우선이면 BG 색 1–3이 이긴다.
                if attr & 0x80 == 0 || bg == 0 {
                    let palette = if attr & 0x10 != 0 {
                        self.obp1
                    } else {
                        self.obp0
                    };
                    self.framebuffer[y * SCREEN_WIDTH + x] =
                        self.palette[usize::from((palette >> (color * 2)) & 3)];
                }
                break;
            }
        }
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

    /// (x, y) 픽셀의 음영 번호 (0 = 가장 밝음).
    fn shade(ppu: &Ppu, x: usize, y: usize) -> usize {
        let pixel = ppu.framebuffer()[y * SCREEN_WIDTH + x];
        DEFAULT_DMG_PALETTE
            .iter()
            .position(|&c| c == pixel)
            .unwrap()
    }

    /// 타일 `index`(0x8000 기준)의 모든 줄을 색 3으로 채운다.
    fn solid_tile(ppu: &mut Ppu, index: u16) {
        for i in 0..16 {
            ppu.write_vram(0x8000 + index * 16 + i, 0xFF);
        }
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
    fn background_tile_uses_bgp() {
        let mut p = ppu();
        solid_tile(&mut p, 1);
        p.write_vram(0x9800, 1);
        ticks(&mut p, 20);
        assert_eq!(
            (shade(&p, 0, 0), shade(&p, 7, 0), shade(&p, 8, 0)),
            (3, 3, 0)
        );
        p.write_reg(BGP, 0x00);
        ticks(&mut p, 114);
        assert_eq!(shade(&p, 0, 1), 0);
    }

    #[test]
    fn signed_tile_addressing_uses_0x9000_base() {
        let mut p = ppu();
        p.write_reg(LCDC, 0x81);
        for i in 0..16 {
            p.write_vram(0x8800 + i, 0xFF);
        }
        p.write_vram(0x9800, 0x80);
        ticks(&mut p, 20);
        assert_eq!(shade(&p, 0, 0), 3);
    }

    #[test]
    fn scx_scrolls_background() {
        let mut p = ppu();
        solid_tile(&mut p, 1);
        p.write_vram(0x9800, 1);
        p.write_reg(SCX, 4);
        ticks(&mut p, 20);
        assert_eq!((shade(&p, 3, 0), shade(&p, 4, 0)), (3, 0));
    }

    #[test]
    fn window_starts_at_wx_minus_7() {
        let mut p = ppu();
        solid_tile(&mut p, 1);
        for i in 0..32 {
            p.write_vram(0x9800 + i, 1);
        }
        // BG는 0x9C00 맵(타일 0), 윈도우는 0x9800 맵(타일 1).
        p.write_reg(LCDC, 0x91 | 0x08 | 0x20);
        p.write_reg(WY, 0);
        p.write_reg(WX, 7 + 80);
        ticks(&mut p, 20);
        assert_eq!((shade(&p, 79, 0), shade(&p, 80, 0)), (0, 3));
    }

    #[test]
    fn sprite_draws_over_background_unless_bg_priority() {
        let mut p = ppu();
        p.write_reg(LCDC, 0x93);
        solid_tile(&mut p, 1);
        p.write_oam(0xFE00, 16);
        p.write_oam(0xFE01, 8);
        p.write_oam(0xFE02, 1);
        ticks(&mut p, 20);
        assert_eq!(shade(&p, 0, 0), 3);
        // BG 우선 속성이면 BG 색 0 위에만 그린다. 여기서 BG는 색 3(BGP로 음영 1)이라 BG가 보인다.
        p.write_oam(0xFE03, 0x80);
        p.write_vram(0x9800, 1);
        p.write_reg(BGP, 0x40);
        ticks(&mut p, 114);
        assert_eq!(shade(&p, 0, 1), 1);
    }

    #[test]
    fn lower_x_sprite_wins_on_dmg() {
        let mut p = ppu();
        p.write_reg(LCDC, 0x93);
        solid_tile(&mut p, 1);
        p.write_reg(OBP0, 0xC0);
        p.write_reg(OBP1, 0x40);
        // OAM 0: x=12, OBP0(색3→음영3). OAM 1: x=8, OBP1(색3→음영1). 겹치는 x=4..8은 x가 작은 OAM 1.
        for (i, (x, attr)) in [(12u8, 0x00u8), (8, 0x10)].into_iter().enumerate() {
            let base = 0xFE00 + 4 * i as u16;
            p.write_oam(base, 16);
            p.write_oam(base + 1, x);
            p.write_oam(base + 2, 1);
            p.write_oam(base + 3, attr);
        }
        ticks(&mut p, 20);
        assert_eq!((shade(&p, 4, 0), shade(&p, 8, 0)), (1, 3));
    }

    #[test]
    fn only_ten_sprites_per_line() {
        let mut p = ppu();
        p.write_reg(LCDC, 0x93);
        solid_tile(&mut p, 1);
        for i in 0..11u16 {
            p.write_oam(0xFE00 + 4 * i, 16);
            p.write_oam(0xFE01 + 4 * i, 8 + 8 * i as u8);
            p.write_oam(0xFE02 + 4 * i, 1);
        }
        ticks(&mut p, 20);
        assert_eq!((shade(&p, 72, 0), shade(&p, 80, 0)), (3, 0));
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
