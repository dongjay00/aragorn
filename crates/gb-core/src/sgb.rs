//! 슈퍼 게임보이 (Pan Docs "Super Game Boy").
//!
//! 게임은 P1(조이패드) 쓰기로 16바이트 명령 패킷을 보내 화면 영역별 4색 팔레트, 테두리, 여러 컨트롤러를
//! 쓴다. 기기는 DMG 그대로 두고 이 기능만 덧붙인다. 그래서 SGB를 지원하는 흑백 게임의 스테이트는 이전
//! 버전과 주고받을 수 있다.
//!
//! 화면에 색을 입히는 일은 코어가 그린 흑백 화면(4단계 색)을 받아 나중에 한다. 큰 데이터(팔레트 묶음,
//! 영역 파일, 테두리)는 게임이 화면에 띄운 타일을 그대로 읽어 가는 "VRAM 전송"으로 온다.

use crate::ppu::{DEFAULT_DMG_PALETTE, SCREEN_HEIGHT, SCREEN_WIDTH};

/// 테두리를 포함한 SGB 화면 크기.
pub const BORDER_WIDTH: usize = 256;
pub const BORDER_HEIGHT: usize = 224;
/// 테두리 안에서 게임 화면의 왼쪽 위.
pub const GAME_X: usize = 48;
pub const GAME_Y: usize = 40;

/// 영역 지정 단위는 8×8 픽셀 칸이다 (20×18칸).
const CELLS_X: usize = 20;
const CELLS_Y: usize = 18;
const CELLS: usize = CELLS_X * CELLS_Y;
/// VRAM 전송 한 번의 크기.
const TRANSFER: usize = 4096;

/// SGB를 켜고 처음 보이는 색 (SNES BGR555, 밝은 색부터).
const DEFAULT_PALETTE: [u16; 4] = [0x67BF, 0x265B, 0x10B5, 0x2866];

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum Transfer {
    /// 시스템 팔레트 512개 (`PAL_TRN`)
    Palettes,
    /// 영역 파일 45개 (`ATTR_TRF`)
    Attributes,
    /// 테두리 타일 128개. `true`면 뒤쪽 128개 (`CHR_TRN`)
    Tiles(bool),
    /// 테두리 타일 맵과 팔레트 (`PCT_TRN`)
    Picture,
    /// 소리·프로그램 전송. 쓰지 않는다.
    Ignored,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Sgb {
    /// 마지막으로 쓴 P1의 비트 4–5.
    last_lines: u8,
    /// 패킷을 받는 중이면 지금까지 받은 비트 수 (128이면 정지 비트를 기다린다).
    receiving: Option<u8>,
    /// 방금 정지 비트를 받았다. 그 뒤의 0x30은 컨트롤러를 바꾸지 않는다.
    #[serde(default)]
    stop_bit: bool,
    packet: [u8; 16],
    /// 여러 패킷으로 된 명령의 모인 바이트.
    command: Vec<u8>,
    /// 컨트롤러 수(1, 2, 4)와 지금 읽히는 컨트롤러.
    players: u8,
    player: u8,
    /// 화면에 쓰는 팔레트 4개. 색 0은 넷이 함께 쓴다.
    palettes: [[u16; 4]; 4],
    /// 칸마다 쓰는 팔레트 번호 (0–3).
    #[serde(with = "crate::state::bytes::boxed")]
    attributes: Box<[u8; CELLS]>,
    #[serde(with = "crate::state::bytes::boxed")]
    system_palettes: Box<[u8; TRANSFER]>,
    #[serde(with = "crate::state::bytes::boxed")]
    attribute_files: Box<[u8; TRANSFER]>,
    /// 테두리 타일 256개 (SNES 4bpp, 타일당 32바이트).
    #[serde(with = "crate::state::bytes::boxed")]
    border_tiles: Box<[u8; TRANSFER * 2]>,
    /// 테두리 타일 맵(32×32, 2바이트씩)과 팔레트 4–7 (0x800부터).
    #[serde(with = "crate::state::bytes::boxed")]
    border_picture: Box<[u8; TRANSFER]>,
    /// 테두리를 받았는지.
    has_border: bool,
    /// 화면 가리기: 0 없음, 1 멈춤, 2 검정, 3 색 0.
    mask: u8,
    /// 다음 프레임에 읽어 갈 VRAM 전송.
    pending: Option<Transfer>,
    /// 멈춤(`MASK_EN` 1)을 켰을 때의 화면. 스테이트에 넣지 않는다.
    #[serde(skip)]
    frozen: Option<Vec<u32>>,
}

impl Default for Sgb {
    fn default() -> Self {
        Self {
            last_lines: 0x30,
            receiving: None,
            stop_bit: false,
            packet: [0; 16],
            command: Vec::new(),
            players: 1,
            player: 0,
            palettes: [DEFAULT_PALETTE; 4],
            attributes: Box::new([0; CELLS]),
            system_palettes: Box::new([0; TRANSFER]),
            attribute_files: Box::new([0; TRANSFER]),
            border_tiles: Box::new([0; TRANSFER * 2]),
            border_picture: Box::new([0; TRANSFER]),
            has_border: false,
            mask: 0,
            pending: None,
            frozen: None,
        }
    }
}

/// SNES BGR555 → 0xRRGGBBAA.
fn rgba(color: u16) -> u32 {
    let channel = |shift: u16| {
        let v = u32::from((color >> shift) & 0x1F);
        (v << 3) | (v >> 2)
    };
    (channel(0) << 24) | (channel(5) << 16) | (channel(10) << 8) | 0xFF
}

fn word(data: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([data[at], data[at + 1]])
}

/// 코어가 그린 흑백 화면의 픽셀이 몇 번째 단계(0 밝음–3 어두움)인지.
fn shade(pixel: u32) -> usize {
    DEFAULT_DMG_PALETTE
        .iter()
        .position(|&c| c == pixel)
        .unwrap_or(0)
}

impl Sgb {
    /// P1 쓰기. 비트 4–5만 본다. 패킷이 끝나면 명령을 실행한다.
    /// `screen`은 지금 화면이다 (멈춤을 켤 때 보관한다).
    pub(crate) fn write_p1(&mut self, value: u8, screen: &[u32]) {
        let lines = value & 0x30;
        let previous = self.last_lines;
        self.last_lines = lines;
        match lines {
            // 리셋 펄스: 새 패킷을 시작한다.
            0x00 => {
                self.receiving = Some(0);
                self.packet = [0; 16];
            }
            // 0x20(P14 낮음)은 비트 0, 0x10(P15 낮음)은 비트 1. 사이에는 반드시 0x30이 온다.
            0x10 | 0x20 if previous == 0x30 => {
                if let Some(bit) = self.receiving {
                    let one = lines == 0x10;
                    if bit < 128 {
                        if one {
                            self.packet[usize::from(bit / 8)] |= 1 << (bit % 8);
                        }
                        self.receiving = Some(bit + 1);
                    } else {
                        // 정지 비트(0)
                        self.receiving = None;
                        self.stop_bit = true;
                        if !one {
                            self.packet_done(screen);
                        }
                    }
                }
            }
            _ => {}
        }
        // 여러 컨트롤러 모드에서는 P14가 낮음→높음으로 바뀔 때 다음 컨트롤러로 넘어간다.
        if lines == 0x30 && std::mem::take(&mut self.stop_bit) {
            return;
        }
        if self.players > 1 && self.receiving.is_none() && previous == 0x20 && lines == 0x30 {
            self.player = (self.player + 1) % self.players;
        }
    }

    /// P1 읽기를 바꿀 필요가 있으면 그 값의 하위 4비트. 두 줄을 모두 고르지 않으면 컨트롤러 번호
    /// (1P = 0xF, 2P = 0xE …), 2P 이후의 버튼 줄은 아무것도 누르지 않은 것으로 보인다.
    pub(crate) fn read_p1_low(&self, select: u8) -> Option<u8> {
        if self.players <= 1 {
            None
        } else if select & 0x30 == 0x30 {
            Some(0x0F - self.player)
        } else if self.player != 0 {
            Some(0x0F)
        } else {
            None
        }
    }

    fn packet_done(&mut self, screen: &[u32]) {
        if self.command.is_empty() && self.packet[0] & 0x07 == 0 {
            // 길이 0인 첫 패킷은 명령이 아니다.
            return;
        }
        self.command.extend_from_slice(&self.packet);
        let packets = usize::from(self.command[0] & 0x07);
        if self.command.len() >= packets * 16 {
            let command = std::mem::take(&mut self.command);
            self.run(&command, screen);
        }
    }

    fn run(&mut self, data: &[u8], screen: &[u32]) {
        match data[0] >> 3 {
            0x00 => self.set_palette_pair(data, 0, 1),
            0x01 => self.set_palette_pair(data, 2, 3),
            0x02 => self.set_palette_pair(data, 0, 3),
            0x03 => self.set_palette_pair(data, 1, 2),
            0x04 => self.attr_block(data),
            0x05 => self.attr_line(data),
            0x06 => self.attr_divide(data),
            0x07 => self.attr_char(data),
            0x09 | 0x10 => self.pending = Some(Transfer::Ignored),
            0x0A => self.palette_set(data, screen),
            0x0B => self.pending = Some(Transfer::Palettes),
            0x11 => {
                self.players = match data[1] & 0x03 {
                    1 => 2,
                    3 => 4,
                    _ => 1,
                };
                self.player = 0;
            }
            0x13 => self.pending = Some(Transfer::Tiles(data[1] & 1 != 0)),
            0x14 => self.pending = Some(Transfer::Picture),
            0x15 => self.pending = Some(Transfer::Attributes),
            0x16 => {
                self.apply_attribute_file(data[1] & 0x3F);
                if data[1] & 0x40 != 0 {
                    self.set_mask(0, screen);
                }
            }
            0x17 => self.set_mask(data[1] & 0x03, screen),
            // 소리, 키 보조, SNES 프로그램 등은 화면에 영향이 없다.
            _ => {}
        }
    }

    /// `PAL01` 등: 색 0과 팔레트 두 개의 색 1–3.
    fn set_palette_pair(&mut self, data: &[u8], a: usize, b: usize) {
        let color0 = word(data, 1);
        for palette in &mut self.palettes {
            palette[0] = color0;
        }
        for i in 0..3 {
            self.palettes[a][i + 1] = word(data, 3 + i * 2);
            self.palettes[b][i + 1] = word(data, 9 + i * 2);
        }
    }

    fn cell(&mut self, x: usize, y: usize) -> &mut u8 {
        &mut self.attributes[y * CELLS_X + x]
    }

    /// `ATTR_BLK`: 사각형 안·테두리·바깥에 팔레트를 지정한다.
    fn attr_block(&mut self, data: &[u8]) {
        let sets = usize::from(data[1]).min(18);
        for set in data[2..].as_chunks::<6>().0.iter().take(sets) {
            let control = set[0];
            let (inside, line, outside) = (set[1] & 3, (set[1] >> 2) & 3, (set[1] >> 4) & 3);
            let (x1, y1) = (usize::from(set[2] & 0x1F), usize::from(set[3] & 0x1F));
            let (x2, y2) = (usize::from(set[4] & 0x1F), usize::from(set[5] & 0x1F));
            // 안쪽만 바꾸면 테두리 줄도 안쪽 색, 바깥만 바꾸면 테두리 줄도 바깥 색이 된다.
            let line_palette = match control & 0x07 {
                c if c & 0x02 != 0 => Some(line),
                0x01 => Some(inside),
                0x04 => Some(outside),
                _ => None,
            };
            for y in 0..CELLS_Y {
                for x in 0..CELLS_X {
                    let within = (x1..=x2).contains(&x) && (y1..=y2).contains(&y);
                    let on_line = within && (x == x1 || x == x2 || y == y1 || y == y2);
                    let palette = if on_line {
                        line_palette
                    } else if within {
                        (control & 0x01 != 0).then_some(inside)
                    } else {
                        (control & 0x04 != 0).then_some(outside)
                    };
                    if let Some(p) = palette {
                        *self.cell(x, y) = p;
                    }
                }
            }
        }
    }

    /// `ATTR_LIN`: 가로줄(행)이나 세로줄(열) 하나씩 팔레트를 지정한다.
    fn attr_line(&mut self, data: &[u8]) {
        let count = usize::from(data[1]).min(110);
        for &entry in data[2..].iter().take(count) {
            let at = usize::from(entry & 0x1F);
            let palette = (entry >> 5) & 0x03;
            if entry & 0x80 != 0 {
                if at < CELLS_Y {
                    (0..CELLS_X).for_each(|x| *self.cell(x, at) = palette);
                }
            } else if at < CELLS_X {
                (0..CELLS_Y).for_each(|y| *self.cell(at, y) = palette);
            }
        }
    }

    /// `ATTR_DIV`: 줄 하나를 기준으로 화면을 둘로 나눈다.
    fn attr_divide(&mut self, data: &[u8]) {
        let (after, before, on) = (data[1] & 3, (data[1] >> 2) & 3, (data[1] >> 4) & 3);
        let horizontal = data[1] & 0x40 != 0;
        let at = usize::from(data[2] & 0x1F);
        for y in 0..CELLS_Y {
            for x in 0..CELLS_X {
                let position = if horizontal { y } else { x };
                *self.cell(x, y) = match position.cmp(&at) {
                    std::cmp::Ordering::Less => before,
                    std::cmp::Ordering::Equal => on,
                    std::cmp::Ordering::Greater => after,
                };
            }
        }
    }

    /// `ATTR_CHR`: 시작 칸부터 칸마다 2비트씩 팔레트를 지정한다.
    fn attr_char(&mut self, data: &[u8]) {
        let (mut x, mut y) = (usize::from(data[1]), usize::from(data[2]));
        let count = usize::from(word(data, 3)).min(CELLS);
        let vertical = data[5] & 1 != 0;
        for i in 0..count {
            let Some(&byte) = data.get(6 + i / 4) else {
                break;
            };
            if x >= CELLS_X || y >= CELLS_Y {
                break;
            }
            *self.cell(x, y) = (byte >> (6 - (i % 4) * 2)) & 0x03;
            if vertical {
                y += 1;
                if y == CELLS_Y {
                    y = 0;
                    x += 1;
                }
            } else {
                x += 1;
                if x == CELLS_X {
                    x = 0;
                    y += 1;
                }
            }
        }
    }

    /// `PAL_SET`: 시스템 팔레트 중 4개를 고르고, 원하면 영역 파일을 적용한다.
    fn palette_set(&mut self, data: &[u8], screen: &[u32]) {
        for p in 0..4 {
            let index = usize::from(word(data, 1 + p * 2) & 0x01FF);
            for c in 0..4 {
                self.palettes[p][c] = word(&self.system_palettes[..], index * 8 + c * 2);
            }
        }
        // 색 0은 넷이 함께 쓴다 (첫 팔레트의 것).
        let color0 = self.palettes[0][0];
        for palette in &mut self.palettes {
            palette[0] = color0;
        }
        let flags = data[9];
        if flags & 0x80 != 0 {
            self.apply_attribute_file(flags & 0x3F);
        }
        if flags & 0x40 != 0 {
            self.set_mask(0, screen);
        }
    }

    /// 영역 파일(90바이트, 칸마다 2비트) 하나를 화면 전체에 적용한다.
    fn apply_attribute_file(&mut self, file: u8) {
        let file = usize::from(file);
        if file >= 45 {
            return;
        }
        for i in 0..CELLS {
            let byte = self.attribute_files[file * 90 + i / 4];
            self.attributes[i] = (byte >> (6 - (i % 4) * 2)) & 0x03;
        }
    }

    fn set_mask(&mut self, mask: u8, screen: &[u32]) {
        if mask == 1 && self.mask != 1 {
            self.frozen = Some(self.colorize_live(screen));
        }
        self.mask = mask;
        if mask != 1 {
            self.frozen = None;
        }
    }

    /// 프레임이 끝날 때 부른다. VRAM 전송을 기다리고 있으면 이 화면에서 데이터를 읽는다.
    pub(crate) fn frame_done(&mut self, screen: &[u32]) {
        let Some(transfer) = self.pending.take() else {
            return;
        };
        let data = transfer_data(screen);
        match transfer {
            Transfer::Palettes => self.system_palettes.copy_from_slice(&data),
            Transfer::Attributes => self.attribute_files.copy_from_slice(&data),
            Transfer::Tiles(high) => {
                let at = if high { TRANSFER } else { 0 };
                self.border_tiles[at..at + TRANSFER].copy_from_slice(&data);
            }
            Transfer::Picture => {
                self.border_picture.copy_from_slice(&data);
                self.has_border = true;
            }
            Transfer::Ignored => {}
        }
    }

    /// 흑백 화면에 영역별 팔레트를 입힌다. 화면 가리기를 켰으면 그에 따른다.
    pub fn colorize(&self, screen: &[u32]) -> Vec<u32> {
        match self.mask {
            1 => self
                .frozen
                .clone()
                .unwrap_or_else(|| self.colorize_live(screen)),
            2 => vec![0x0000_00FF; SCREEN_WIDTH * SCREEN_HEIGHT],
            3 => vec![rgba(self.palettes[0][0]); SCREEN_WIDTH * SCREEN_HEIGHT],
            _ => self.colorize_live(screen),
        }
    }

    fn colorize_live(&self, screen: &[u32]) -> Vec<u32> {
        let colors = self.palettes.map(|p| p.map(rgba));
        screen
            .iter()
            .enumerate()
            .map(|(i, &pixel)| {
                let (x, y) = (i % SCREEN_WIDTH, i / SCREEN_WIDTH);
                let palette = usize::from(self.attributes[(y / 8) * CELLS_X + x / 8]);
                colors[palette][shade(pixel)]
            })
            .collect()
    }

    /// 게임이 테두리를 보냈는지.
    pub fn has_border(&self) -> bool {
        self.has_border
    }

    /// 테두리(256×224) 안 가운데에 `game`(160×144)을 놓은 화면. 테두리가 없으면 `None`.
    pub fn with_border(&self, game: &[u32]) -> Option<Vec<u32>> {
        if !self.has_border {
            return None;
        }
        let backdrop = rgba(self.palettes[0][0]);
        let mut out = vec![backdrop; BORDER_WIDTH * BORDER_HEIGHT];
        for y in 0..SCREEN_HEIGHT {
            let row = &game[y * SCREEN_WIDTH..][..SCREEN_WIDTH];
            out[(GAME_Y + y) * BORDER_WIDTH + GAME_X..][..SCREEN_WIDTH].copy_from_slice(row);
        }
        let border_palettes: [[u32; 16]; 4] = std::array::from_fn(|p| {
            std::array::from_fn(|c| rgba(word(&self.border_picture[..], 0x800 + p * 32 + c * 2)))
        });
        for ty in 0..BORDER_HEIGHT / 8 {
            for tx in 0..BORDER_WIDTH / 8 {
                let entry = word(&self.border_picture[..], (ty * 32 + tx) * 2);
                let tile = usize::from(entry & 0xFF);
                let palette = usize::from((entry >> 10) & 0x07).saturating_sub(4).min(3);
                let (flip_x, flip_y) = (entry & 0x4000 != 0, entry & 0x8000 != 0);
                let pixels = &self.border_tiles[tile * 32..][..32];
                for py in 0..8 {
                    let row = if flip_y { 7 - py } else { py };
                    for px in 0..8 {
                        let bit = if flip_x { px } else { 7 - px };
                        let plane = |n: usize| (pixels[n] >> bit) & 1;
                        let index = plane(row * 2)
                            | plane(row * 2 + 1) << 1
                            | plane(16 + row * 2) << 2
                            | plane(16 + row * 2 + 1) << 3;
                        if index != 0 {
                            out[(ty * 8 + py) * BORDER_WIDTH + tx * 8 + px] =
                                border_palettes[palette][usize::from(index)];
                        }
                    }
                }
            }
        }
        Some(out)
    }
}

/// VRAM 전송: 화면에 보이는 타일(가로 20개씩)을 차례로 다시 2bpp 타일 데이터로 바꾼 4KB.
/// 게임은 전송할 데이터를 타일로 깔고 BGP를 0xE4(그대로)로 두고 화면에 띄운다.
fn transfer_data(screen: &[u32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(TRANSFER);
    for tile in 0..TRANSFER / 16 {
        let (tx, ty) = (tile % CELLS_X, tile / CELLS_X);
        for row in 0..8 {
            let y = ty * 8 + row;
            let (mut low, mut high) = (0u8, 0u8);
            for px in 0..8 {
                let s = screen
                    .get(y * SCREEN_WIDTH + tx * 8 + px)
                    .map_or(0, |&p| shade(p));
                low |= ((s & 1) as u8) << (7 - px);
                high |= ((s >> 1) as u8) << (7 - px);
            }
            out.push(low);
            out.push(high);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    static BLANK: [u32; SCREEN_WIDTH * SCREEN_HEIGHT] = [DEFAULT_DMG_PALETTE[0]; 160 * 144];

    /// 패킷 첫 바이트: 명령 번호와 패킷 수.
    fn header(command: u8, packets: u8) -> u8 {
        (command << 3) | packets
    }

    /// 게임처럼 P1 쓰기로 패킷(16바이트씩)을 보낸다.
    fn send(sgb: &mut Sgb, bytes: &[u8]) {
        for packet in bytes.chunks(16) {
            sgb.write_p1(0x00, &BLANK);
            sgb.write_p1(0x30, &BLANK);
            let mut full = [0u8; 16];
            full[..packet.len()].copy_from_slice(packet);
            for bit in 0..128 {
                let one = full[bit / 8] >> (bit % 8) & 1 != 0;
                sgb.write_p1(if one { 0x10 } else { 0x20 }, &BLANK);
                sgb.write_p1(0x30, &BLANK);
            }
            sgb.write_p1(0x20, &BLANK);
            sgb.write_p1(0x30, &BLANK);
        }
    }

    #[test]
    fn pal01_sets_shared_color0_and_two_palettes() {
        let mut sgb = Sgb::default();
        let mut packet = vec![header(0x00, 1)];
        for color in [0x7FFFu16, 0x001F, 0x03E0, 0x7C00, 0x0001, 0x0002, 0x0003] {
            packet.extend(color.to_le_bytes());
        }
        send(&mut sgb, &packet);
        assert_eq!(sgb.palettes[0], [0x7FFF, 0x001F, 0x03E0, 0x7C00]);
        assert_eq!(sgb.palettes[1], [0x7FFF, 0x0001, 0x0002, 0x0003]);
        assert_eq!(sgb.palettes[3][0], 0x7FFF, "색 0은 모두 함께");
    }

    #[test]
    fn colorize_uses_the_cell_palette() {
        let mut sgb = Sgb::default();
        sgb.palettes[2] = [0x0000, 0x001F, 0x0000, 0x0000];
        // ATTR_BLK: 칸 (1,0)–(1,0) 안쪽을 팔레트 2로
        send(&mut sgb, &[header(0x04, 1), 1, 0x01, 0x02, 1, 0, 1, 0]);
        let mut screen = BLANK.to_vec();
        screen[8] = DEFAULT_DMG_PALETTE[1]; // 칸 (1,0)의 픽셀
        screen[0] = DEFAULT_DMG_PALETTE[1]; // 칸 (0,0)의 픽셀
        let out = sgb.colorize(&screen);
        assert_eq!(out[8], 0xFF00_00FF, "빨강 (팔레트 2의 색 1)");
        assert_eq!(out[0], rgba(DEFAULT_PALETTE[1]), "기본 팔레트 0");
    }

    #[test]
    fn mlt_req_cycles_controller_ids() {
        let mut sgb = Sgb::default();
        assert_eq!(sgb.read_p1_low(0x30), None);
        send(&mut sgb, &[header(0x11, 1), 0x01]);
        assert_eq!(sgb.read_p1_low(0x30), Some(0x0F));
        sgb.write_p1(0x20, &BLANK);
        sgb.write_p1(0x30, &BLANK);
        assert_eq!(sgb.read_p1_low(0x30), Some(0x0E), "2P");
        assert_eq!(
            sgb.read_p1_low(0x10),
            Some(0x0F),
            "2P는 아무것도 누르지 않았다"
        );
        sgb.write_p1(0x20, &BLANK);
        sgb.write_p1(0x30, &BLANK);
        assert_eq!(sgb.read_p1_low(0x30), Some(0x0F), "다시 1P");
    }

    #[test]
    fn vram_transfer_reads_the_next_frame() {
        let mut sgb = Sgb::default();
        send(&mut sgb, &[header(0x0B, 1)]); // PAL_TRN
        // 첫 타일 첫 줄: 픽셀 0만 가장 어두운 색(3) → 바이트 0x80, 0x80
        let mut screen = BLANK.to_vec();
        screen[0] = DEFAULT_DMG_PALETTE[3];
        sgb.frame_done(&screen);
        assert_eq!(&sgb.system_palettes[..3], &[0x80, 0x80, 0x00]);
        // PAL_SET: 시스템 팔레트 0을 네 팔레트에
        send(&mut sgb, &[header(0x0A, 1), 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(sgb.palettes[0][0], 0x8080);
    }

    #[test]
    fn mask_black_hides_the_game() {
        let mut sgb = Sgb::default();
        send(&mut sgb, &[header(0x17, 1), 2]);
        assert!(sgb.colorize(&BLANK).iter().all(|&p| p == 0x0000_00FF));
        send(&mut sgb, &[header(0x17, 1), 0]);
        assert_ne!(sgb.colorize(&BLANK)[0], 0x0000_00FF);
    }

    #[test]
    fn border_frames_the_game_and_skips_color0() {
        let mut sgb = Sgb::default();
        assert!(sgb.with_border(&BLANK).is_none());
        // 타일 1: 첫 픽셀만 색 1. 맵 (0,0)에 타일 1, 팔레트 4.
        sgb.border_tiles[32] = 0x80;
        sgb.border_picture[0] = 1;
        sgb.border_picture[1] = 4 << 2;
        sgb.border_picture[0x802..0x804].copy_from_slice(&0x001Fu16.to_le_bytes());
        sgb.has_border = true;
        let game = vec![0x1234_56FF; 160 * 144];
        let out = sgb.with_border(&game).unwrap();
        assert_eq!(out.len(), BORDER_WIDTH * BORDER_HEIGHT);
        assert_eq!(out[0], 0xFF00_00FF, "테두리 색");
        assert_eq!(out[1], rgba(DEFAULT_PALETTE[0]), "색 0은 배경");
        assert_eq!(
            out[GAME_Y * BORDER_WIDTH + GAME_X],
            0x1234_56FF,
            "게임 화면"
        );
    }
}
