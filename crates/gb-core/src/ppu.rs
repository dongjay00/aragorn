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
/// CGB: VRAM 뱅크 선택.
pub const VBK: u16 = 0xFF4F;
/// CGB: BG 팔레트 인덱스와 데이터.
pub const BCPS: u16 = 0xFF68;
pub const BCPD: u16 = 0xFF69;
/// CGB: 스프라이트 팔레트 인덱스와 데이터.
pub const OCPS: u16 = 0xFF6A;
pub const OCPD: u16 = 0xFF6B;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum Mode {
    HBlank = 0,
    VBlank = 1,
    OamScan = 2,
    Drawing = 3,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Ppu {
    /// CGB 모드인지. DMG 모드에서는 VRAM 뱅크 1과 컬러 팔레트를 쓰지 않는다.
    cgb: bool,
    /// 뱅크 0(0x0000–0x1FFF)과 CGB 뱅크 1(0x2000–0x3FFF).
    #[serde(with = "crate::state::bytes::boxed")]
    vram: Box<[u8; 0x4000]>,
    /// CPU가 보는 VRAM 뱅크(VBK 비트 0).
    vram_bank: u8,
    /// CGB 팔레트 RAM: 팔레트 8개 × 색 4개 × 2바이트(15비트 BGR, 리틀 엔디언).
    #[serde(with = "crate::state::bytes")]
    bg_palettes: [u8; 64],
    #[serde(with = "crate::state::bytes")]
    obj_palettes: [u8; 64],
    /// 팔레트 인덱스(비트 0–5)와 자동 증가(비트 7).
    bcps: u8,
    ocps: u8,
    /// 보이는 줄의 HBlank(모드 0)에 들어갔는지. HBlank DMA가 쓴다. 읽으면 초기화된다.
    hblank_started: bool,
    #[serde(with = "crate::state::bytes::boxed")]
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
    #[serde(with = "crate::state::words")]
    framebuffer: Box<[u32; SCREEN_WIDTH * SCREEN_HEIGHT]>,
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new(false)
    }
}

impl Ppu {
    /// 손상된 스테이트 때문에 패닉하지 않게 인덱스로 쓰는 값을 하드웨어 범위로 감싼다.
    pub(crate) fn sanitize(&mut self) {
        self.vram_bank &= 0x01;
        // LCD가 켜져 있으면 줄 안의 위치(2 dot 단위), 꺼져 있으면 프레임 안의 위치다.
        self.dot = if self.lcdc & 0x80 != 0 {
            (self.dot % DOTS_PER_LINE) & !1
        } else {
            self.dot % DOTS_PER_FRAME
        };
        self.ly %= LINES_PER_FRAME;
        if self.ly >= SCREEN_HEIGHT as u8 {
            self.mode = Mode::VBlank;
        }
    }

    /// 부트 ROM 직후 상태. CGB 팔레트 RAM은 흰색으로 채운다.
    pub fn new(cgb: bool) -> Self {
        Self {
            cgb,
            vram: Box::new([0; 0x4000]),
            vram_bank: 0,
            bg_palettes: [0xFF; 64],
            obj_palettes: [0xFF; 64],
            bcps: 0,
            ocps: 0,
            hblank_started: false,
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

    /// CPU가 고른 뱅크의 VRAM 오프셋.
    fn vram_index(&self, addr: u16) -> usize {
        usize::from(self.vram_bank) * 0x2000 + usize::from(addr & 0x1FFF)
    }

    pub fn read_vram(&self, addr: u16) -> u8 {
        self.vram[self.vram_index(addr)]
    }

    pub fn write_vram(&mut self, addr: u16, value: u8) {
        let i = self.vram_index(addr);
        self.vram[i] = value;
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
            VBK if self.cgb => 0xFE | self.vram_bank,
            BCPS if self.cgb => self.bcps | 0x40,
            BCPD if self.cgb => self.bg_palettes[usize::from(self.bcps & 0x3F)],
            OCPS if self.cgb => self.ocps | 0x40,
            OCPD if self.cgb => self.obj_palettes[usize::from(self.ocps & 0x3F)],
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
                    let blank = if self.cgb { WHITE } else { self.palette[0] };
                    self.framebuffer.fill(blank);
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
            VBK if self.cgb => self.vram_bank = value & 0x01,
            BCPS if self.cgb => self.bcps = value & 0xBF,
            BCPD if self.cgb => write_palette(&mut self.bg_palettes, &mut self.bcps, value),
            OCPS if self.cgb => self.ocps = value & 0xBF,
            OCPD if self.cgb => write_palette(&mut self.obj_palettes, &mut self.ocps, value),
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

    /// `dots` dot(2의 배수, CGB 2배속이면 M-사이클당 2) 진행한다.
    /// 요청할 인터럽트 비트(`IRQ_VBLANK`, `IRQ_STAT`)를 반환한다.
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
        for _ in 0..dots / 2 {
            irq |= self.step2();
        }
        irq
    }

    /// 2 dot 진행한다. 모드 경계(80, 252, 456)는 모두 짝수다.
    fn step2(&mut self) -> u8 {
        self.dot += 2;
        let mut irq = 0;
        if self.ly < VBLANK_LINE {
            match self.dot {
                MODE3_START => {
                    self.mode = Mode::Drawing;
                    self.render_line();
                }
                MODE0_START => {
                    self.mode = Mode::HBlank;
                    self.hblank_started = true;
                }
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

    /// HBlank에 들어갔으면 `true`. 읽으면 초기화된다.
    pub fn take_hblank_started(&mut self) -> bool {
        std::mem::take(&mut self.hblank_started)
    }

    /// `bank`(0 또는 0x2000)의 타일 한 줄의 두 바이트.
    fn tile_row(&self, bank: usize, tile_addr: usize, row: usize) -> (u8, u8) {
        let i = bank + ((tile_addr + row * 2) & 0x1FFF);
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
        // BG/윈도우의 색 번호(0–3)와 CGB BG 우선순위 속성. 스프라이트 우선순위 판정에 쓴다.
        let mut bg_color = [0u8; SCREEN_WIDTH];
        let mut bg_priority = [false; SCREEN_WIDTH];
        let mut pixels = [0u32; SCREEN_WIDTH];
        let window_visible = self.lcdc & 0x20 != 0 && self.wy_triggered && self.wx <= 166;
        let mut window_drawn = false;
        // CGB의 LCDC 비트 0은 BG를 끄지 않고 BG 우선순위만 끈다.
        if self.cgb || self.lcdc & 0x01 != 0 {
            for x in 0..SCREEN_WIDTH {
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
                let map_index = map + (py / 8) * 32 + px / 8;
                let index = self.vram[map_index];
                // CGB 타일 속성(VRAM 뱅크 1의 같은 위치): 팔레트 0–2, 뱅크 3, 좌우 5, 상하 6, 우선 7.
                let attr = if self.cgb {
                    self.vram[0x2000 + map_index]
                } else {
                    0
                };
                let bank = if attr & 0x08 != 0 { 0x2000 } else { 0 };
                let row = if attr & 0x40 != 0 { 7 - py % 8 } else { py % 8 };
                let (lo, hi) = self.tile_row(bank, self.bg_tile_addr(index), row);
                let bit = if attr & 0x20 != 0 { px % 8 } else { 7 - px % 8 };
                let color = (((hi >> bit) & 1) << 1) | ((lo >> bit) & 1);
                bg_color[x] = color;
                bg_priority[x] = attr & 0x80 != 0;
                if self.cgb {
                    pixels[x] = cgb_color(&self.bg_palettes, attr & 0x07, color);
                }
            }
        }
        if window_drawn {
            self.window_line = self.window_line.wrapping_add(1);
        }
        if !self.cgb {
            for (pixel, &color) in pixels.iter_mut().zip(&bg_color) {
                *pixel = self.palette[usize::from((self.bgp >> (color * 2)) & 3)];
            }
        }
        self.framebuffer[y * SCREEN_WIDTH..(y + 1) * SCREEN_WIDTH].copy_from_slice(&pixels);
        if self.lcdc & 0x02 != 0 {
            self.render_sprites(y, &bg_color, &bg_priority);
        }
    }

    fn render_sprites(
        &mut self,
        y: usize,
        bg_color: &[u8; SCREEN_WIDTH],
        bg_priority: &[bool; SCREEN_WIDTH],
    ) {
        let height = if self.lcdc & 0x04 != 0 { 16 } else { 8 };
        // 줄당 최대 10개, OAM 순서대로 고른다.
        let mut sprites: Vec<(usize, [u8; 4])> = self
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
            .map(|(i, s)| (i, *s))
            .collect();
        // DMG: X 좌표가 작은 것이 우선이고, 같으면 OAM 앞쪽이 우선이다. CGB: OAM 앞쪽이 우선이다.
        if !self.cgb {
            sprites.sort_by_key(|&(i, s)| (s[1], i));
        }
        // CGB에서 LCDC 비트 0이 꺼져 있으면 스프라이트가 항상 BG 위에 그려진다.
        let bg_master_priority = !self.cgb || self.lcdc & 0x01 != 0;
        for x in 0..SCREEN_WIDTH {
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
                let bank = if self.cgb && attr & 0x08 != 0 {
                    0x2000
                } else {
                    0
                };
                let (lo, hi) = self.tile_row(bank, usize::from(tile) * 16, line as usize);
                let bit = if attr & 0x20 != 0 { col } else { 7 - col };
                let color = (((hi >> bit) & 1) << 1) | ((lo >> bit) & 1);
                if color == 0 {
                    continue;
                }
                // 우선순위가 가장 높은 불투명 스프라이트 픽셀만 본다. BG 우선이면 BG 색 1–3이 이긴다.
                let behind_bg = attr & 0x80 != 0 || bg_priority[x];
                if !bg_master_priority || bg_color[x] == 0 || !behind_bg {
                    self.framebuffer[y * SCREEN_WIDTH + x] = if self.cgb {
                        cgb_color(&self.obj_palettes, attr & 0x07, color)
                    } else {
                        let palette = if attr & 0x10 != 0 {
                            self.obp1
                        } else {
                            self.obp0
                        };
                        self.palette[usize::from((palette >> (color * 2)) & 3)]
                    };
                }
                break;
            }
        }
    }
}

const WHITE: u32 = 0xFFFF_FFFF;

/// 팔레트 데이터 레지스터(BCPD/OCPD) 쓰기. 자동 증가가 켜져 있으면 인덱스를 1 올린다.
fn write_palette(ram: &mut [u8; 64], spec: &mut u8, value: u8) {
    ram[usize::from(*spec & 0x3F)] = value;
    if *spec & 0x80 != 0 {
        *spec = 0x80 | ((*spec + 1) & 0x3F);
    }
}

/// CGB 팔레트 RAM의 15비트 색을 0xRRGGBBAA로 바꾼다. 5비트 채널은 (c << 3) | (c >> 2)로 늘린다.
fn cgb_color(ram: &[u8; 64], palette: u8, color: u8) -> u32 {
    let i = usize::from(palette) * 8 + usize::from(color) * 2;
    let bgr = u16::from_le_bytes([ram[i], ram[i + 1]]);
    let channel = |shift: u16| {
        let c = u32::from((bgr >> shift) & 0x1F);
        (c << 3) | (c >> 2)
    };
    channel(0) << 24 | channel(5) << 16 | channel(10) << 8 | 0xFF
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

    /// LCD 켜짐, BG·스프라이트 켜짐, 0x8000 타일 데이터인 CGB PPU.
    fn cgb_ppu() -> Ppu {
        let mut p = Ppu::new(true);
        p.write_reg(LCDC, 0x93);
        p
    }

    /// CGB 팔레트 RAM에 15비트 색 하나를 쓴다.
    fn set_cgb_color(p: &mut Ppu, spec: u16, data: u16, palette: u8, color: u8, bgr: u16) {
        p.write_reg(spec, palette * 8 + color * 2);
        let [lo, hi] = bgr.to_le_bytes();
        p.write_reg(data, lo);
        p.write_reg(spec, palette * 8 + color * 2 + 1);
        p.write_reg(data, hi);
    }

    const RED: u16 = 0x001F;
    const GREEN: u16 = 0x03E0;
    const BLACK: u16 = 0x0000;

    fn pixel(p: &Ppu, x: usize, y: usize) -> u32 {
        p.framebuffer()[y * SCREEN_WIDTH + x]
    }

    #[test]
    fn cgb_vram_has_two_banks() {
        let mut p = Ppu::new(true);
        p.write_vram(0x8000, 1);
        p.write_reg(VBK, 0x01);
        assert_eq!(p.read_reg(VBK), 0xFF);
        assert_eq!(p.read_vram(0x8000), 0);
        p.write_vram(0x8000, 2);
        p.write_reg(VBK, 0x00);
        assert_eq!((p.read_reg(VBK), p.read_vram(0x8000)), (0xFE, 1));
    }

    #[test]
    fn dmg_has_no_vram_bank_or_color_palettes() {
        let mut p = Ppu::default();
        p.write_reg(VBK, 0x01);
        p.write_vram(0x8000, 7);
        p.write_reg(VBK, 0x00);
        assert_eq!(p.read_vram(0x8000), 7);
        assert_eq!((p.read_reg(VBK), p.read_reg(BCPD)), (0xFF, 0xFF));
    }

    #[test]
    fn palette_data_auto_increments_and_wraps() {
        let mut p = Ppu::new(true);
        p.write_reg(BCPS, 0x80 | 0x3E);
        p.write_reg(BCPD, 0x12);
        p.write_reg(BCPD, 0x34);
        assert_eq!(p.read_reg(BCPS), 0xC0, "0x3F 다음은 0이다");
        p.write_reg(BCPS, 0x3F);
        assert_eq!(p.read_reg(BCPD), 0x34);
        p.write_reg(BCPD, 0x56);
        assert_eq!(p.read_reg(BCPS), 0x7F, "자동 증가가 꺼져 있으면 그대로다");
    }

    #[test]
    fn cgb_colors_expand_five_bit_channels() {
        let mut ram = [0u8; 64];
        for (i, bgr) in [0x7FFFu16, RED, GREEN, 0x7C00].into_iter().enumerate() {
            ram[i * 2..i * 2 + 2].copy_from_slice(&bgr.to_le_bytes());
        }
        assert_eq!(cgb_color(&ram, 0, 0), 0xFFFF_FFFF);
        assert_eq!(cgb_color(&ram, 0, 1), 0xFF00_00FF);
        assert_eq!(cgb_color(&ram, 0, 2), 0x00FF_00FF);
        assert_eq!(cgb_color(&ram, 0, 3), 0x0000_FFFF);
    }

    #[test]
    fn cgb_bg_uses_attribute_palette_bank_and_flip() {
        let mut p = cgb_ppu();
        set_cgb_color(&mut p, BCPS, BCPD, 1, 0, BLACK);
        set_cgb_color(&mut p, BCPS, BCPD, 1, 1, RED);
        p.write_reg(VBK, 0x01);
        p.write_vram(0x8000, 0x80); // 뱅크 1 타일 0, 0번 줄 맨 왼쪽 픽셀만 색 1
        p.write_vram(0x9800, 0x29); // 뱅크 1, 좌우 반전, 팔레트 1
        p.write_reg(VBK, 0x00);
        ticks(&mut p, 21);
        assert_eq!(pixel(&p, 7, 0), 0xFF00_00FF, "반전되어 맨 오른쪽");
        assert_eq!(pixel(&p, 0, 0), 0x0000_00FF);
    }

    /// 타일 1을 색 1로 채우고, BG는 색 0인 CGB PPU. 스프라이트 팔레트 0은 빨강, 1은 초록이다.
    fn cgb_ppu_with_sprite_tile() -> Ppu {
        let mut p = cgb_ppu();
        for row in 0..8 {
            p.write_vram(0x8010 + row * 2, 0xFF);
        }
        set_cgb_color(&mut p, OCPS, OCPD, 0, 1, RED);
        set_cgb_color(&mut p, OCPS, OCPD, 1, 1, GREEN);
        p
    }

    #[test]
    fn cgb_sprites_prefer_oam_order_over_x() {
        let mut p = cgb_ppu_with_sprite_tile();
        p.write_oam(0xFE00, 16);
        p.write_oam(0xFE01, 10);
        p.write_oam(0xFE02, 1);
        p.write_oam(0xFE03, 0x00);
        p.write_oam(0xFE04, 16);
        p.write_oam(0xFE05, 8);
        p.write_oam(0xFE06, 1);
        p.write_oam(0xFE07, 0x01);
        ticks(&mut p, 21);
        assert_eq!(pixel(&p, 4, 0), 0xFF00_00FF, "겹치면 OAM 앞쪽(빨강)");
        assert_eq!(pixel(&p, 0, 0), 0x00FF_00FF);
    }

    #[test]
    fn cgb_bg_priority_attribute_hides_sprites_unless_master_priority_is_off() {
        for (lcdc, expected) in [(0x93, 0xFFFF_FFFF), (0x92, 0xFF00_00FF)] {
            let mut p = cgb_ppu_with_sprite_tile();
            p.write_reg(LCDC, lcdc);
            p.write_vram(0x9800, 1); // BG 타일 1(색 1, 팔레트 0 = 흰색)
            p.write_reg(VBK, 0x01);
            p.write_vram(0x9800, 0x80); // BG 우선
            p.write_reg(VBK, 0x00);
            p.write_oam(0xFE00, 16);
            p.write_oam(0xFE01, 8);
            p.write_oam(0xFE02, 1);
            ticks(&mut p, 21);
            assert_eq!(pixel(&p, 0, 0), expected, "LCDC {lcdc:#04X}");
        }
    }

    #[test]
    fn hblank_start_is_signalled_once_per_visible_line() {
        let mut p = ppu();
        ticks(&mut p, 62);
        assert!(!p.take_hblank_started());
        ticks(&mut p, 1);
        assert!(p.take_hblank_started());
        assert!(!p.take_hblank_started(), "읽으면 초기화");
    }

    #[test]
    fn two_dot_steps_keep_line_timing() {
        let mut p = ppu();
        for _ in 0..228 {
            p.tick(2);
        }
        assert_eq!(p.read_reg(LY), 1);
    }
}
