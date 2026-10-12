//! VRAM 뷰어: 타일 시트, 배경 맵, 스프라이트(OAM), 팔레트 값. 결과는 0xRRGGBBAA 픽셀이다.
//!
//! 입력은 VRAM 뱅크(0x8000–0x9FFF에 해당하는 0x2000바이트)와 색을 정하는 함수라서
//! 코어 없이 테스트할 수 있다.

/// 타일 시트는 16×24 타일(뱅크 하나의 384개)이다.
pub const SHEET_COLUMNS: usize = 16;
pub const SHEET_WIDTH: usize = SHEET_COLUMNS * 8;
pub const SHEET_HEIGHT: usize = 384 / SHEET_COLUMNS * 8;
/// 배경 맵은 32×32 타일이다.
pub const MAP_SIZE: usize = 256;
pub const SCREEN_WIDTH: usize = 160;
pub const SCREEN_HEIGHT: usize = 144;
/// 스프라이트 미리보기에서 색 0(투명)을 칠할 값.
pub const TRANSPARENT: u32 = 0x0000_0000;
/// 타일 시트용 회색 4단계 (색 번호 0이 가장 밝다).
pub const GRAY: [u32; 4] = [0xFFFF_FFFF, 0xAAAA_AAFF, 0x5555_55FF, 0x0000_00FF];

/// 타일 한 줄(2바이트)의 색 번호 8개, 왼쪽부터.
pub fn tile_row(lo: u8, hi: u8) -> [u8; 8] {
    std::array::from_fn(|x| {
        let bit = 7 - x;
        ((hi >> bit) & 1) << 1 | ((lo >> bit) & 1)
    })
}

/// 타일 `index`(0–383)의 `y`번째 줄. `vram`은 뱅크 하나다.
fn row_of(vram: &[u8], index: usize, y: usize) -> [u8; 8] {
    let at = index * 16 + y * 2;
    tile_row(vram[at], vram[at + 1])
}

/// 뱅크 하나의 타일 384개를 16×24 격자로 그린다 (`SHEET_WIDTH`×`SHEET_HEIGHT`).
pub fn tile_sheet(vram: &[u8], shades: [u32; 4]) -> Vec<u32> {
    let mut out = vec![0; SHEET_WIDTH * SHEET_HEIGHT];
    for index in 0..384 {
        let (tx, ty) = (index % SHEET_COLUMNS * 8, index / SHEET_COLUMNS * 8);
        for y in 0..8 {
            for (x, color) in row_of(vram, index, y).into_iter().enumerate() {
                out[(ty + y) * SHEET_WIDTH + tx + x] = shades[usize::from(color)];
            }
        }
    }
    out
}

/// 배경 맵을 어디서 어떻게 읽을지 (LCDC 비트에서 정한다).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapSource {
    /// `true`면 0x9C00, 아니면 0x9800 맵
    pub high_map: bool,
    /// `true`면 0x8000부터 부호 없는 번호, 아니면 0x9000 기준 부호 있는 번호 (LCDC.4)
    pub unsigned_tiles: bool,
}

impl MapSource {
    /// LCDC가 고른 배경 맵(`window`면 창 맵)과 타일 주소 방식.
    pub fn from_lcdc(lcdc: u8, window: bool) -> Self {
        let map_bit = if window { 0x40 } else { 0x08 };
        Self {
            high_map: lcdc & map_bit != 0,
            unsigned_tiles: lcdc & 0x10 != 0,
        }
    }
}

/// 맵 타일 번호가 가리키는 타일(뱅크 안 0–383).
pub fn tile_index(number: u8, unsigned_tiles: bool) -> usize {
    if unsigned_tiles {
        usize::from(number)
    } else {
        (256 + i16::from(number as i8)) as usize
    }
}

/// 32×32 배경 맵 전체를 256×256으로 그린다.
/// - `bank1`이 있으면(CGB) 맵 속성의 팔레트·뱅크·뒤집기를 따른다.
/// - `color(palette, color)`는 BG 팔레트 색이다. DMG에서는 팔레트가 늘 0이다.
pub fn tile_map(
    bank0: &[u8],
    bank1: Option<&[u8]>,
    source: MapSource,
    color: impl Fn(u8, u8) -> u32,
) -> Vec<u32> {
    let base = if source.high_map { 0x1C00 } else { 0x1800 };
    let mut out = vec![0; MAP_SIZE * MAP_SIZE];
    for cell in 0..32 * 32 {
        let number = bank0[base + cell];
        let attr = bank1.map_or(0, |b| b[base + cell]);
        let tiles = if attr & 0x08 != 0 {
            bank1.unwrap_or(bank0)
        } else {
            bank0
        };
        let index = tile_index(number, source.unsigned_tiles);
        let (cx, cy) = (cell % 32 * 8, cell / 32 * 8);
        for y in 0..8 {
            let src_y = if attr & 0x40 != 0 { 7 - y } else { y };
            let row = row_of(tiles, index, src_y);
            for x in 0..8 {
                let src_x = if attr & 0x20 != 0 { 7 - x } else { x };
                out[(cy + y) * MAP_SIZE + cx + x] = color(attr & 0x07, row[src_x]);
            }
        }
    }
    out
}

/// 맵 위에 지금 화면(160×144, SCX/SCY에서 시작) 테두리를 그린다. 맵 끝에서 감싸 돈다.
pub fn outline_viewport(map: &mut [u32], scx: u8, scy: u8, color: u32) {
    let mut put = |x: usize, y: usize| {
        map[(usize::from(scy) + y) % MAP_SIZE * MAP_SIZE + (usize::from(scx) + x) % MAP_SIZE] =
            color;
    };
    for x in 0..SCREEN_WIDTH {
        put(x, 0);
        put(x, SCREEN_HEIGHT - 1);
    }
    for y in 0..SCREEN_HEIGHT {
        put(0, y);
        put(SCREEN_WIDTH - 1, y);
    }
}

/// OAM 항목 하나.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sprite {
    pub index: u8,
    /// OAM에 적힌 값 그대로 (화면 좌표 + 16, + 8)
    pub y: u8,
    pub x: u8,
    pub tile: u8,
    pub attr: u8,
}

impl Sprite {
    pub fn flip_x(&self) -> bool {
        self.attr & 0x20 != 0
    }

    pub fn flip_y(&self) -> bool {
        self.attr & 0x40 != 0
    }

    /// BG 색 1–3 뒤에 그려지는지 (속성 비트 7).
    pub fn behind_bg(&self) -> bool {
        self.attr & 0x80 != 0
    }

    /// 쓰는 팔레트: CGB는 0–7, DMG는 OBP0/OBP1의 0/1.
    pub fn palette(&self, cgb: bool) -> u8 {
        if cgb {
            self.attr & 0x07
        } else {
            (self.attr >> 4) & 1
        }
    }

    /// CGB에서 타일을 읽을 VRAM 뱅크.
    pub fn bank(&self, cgb: bool) -> u8 {
        u8::from(cgb && self.attr & 0x08 != 0)
    }

    /// 화면 안에 조금이라도 보이는지.
    pub fn on_screen(&self, tall: bool) -> bool {
        let height: u16 = if tall { 16 } else { 8 };
        self.x > 0 && self.x < 168 && u16::from(self.y) + height > 16 && self.y < 160
    }
}

/// OAM 160바이트를 스프라이트 40개로 나눈다.
pub fn sprites(oam: &[u8; 0xA0]) -> Vec<Sprite> {
    let (entries, _) = oam.as_chunks::<4>();
    entries
        .iter()
        .enumerate()
        .map(|(i, &[y, x, tile, attr])| Sprite {
            index: i as u8,
            y,
            x,
            tile,
            attr,
        })
        .collect()
}

/// 스프라이트 하나를 8×8(또는 8×16)로 그린다. 색 0은 `TRANSPARENT`다.
/// `tiles`는 스프라이트가 쓰는 뱅크, `color(color)`는 그 스프라이트 팔레트 색이다.
pub fn sprite_image(
    sprite: &Sprite,
    tiles: &[u8],
    tall: bool,
    color: impl Fn(u8) -> u32,
) -> Vec<u32> {
    let height = if tall { 16 } else { 8 };
    let first = if tall {
        sprite.tile & 0xFE
    } else {
        sprite.tile
    };
    let mut out = vec![TRANSPARENT; 8 * height];
    for y in 0..height {
        let src_y = if sprite.flip_y() { height - 1 - y } else { y };
        let index = usize::from(first) + src_y / 8;
        let row = row_of(tiles, index, src_y % 8);
        for x in 0..8 {
            let src_x = if sprite.flip_x() { 7 - x } else { x };
            let c = row[src_x];
            if c != 0 {
                out[y * 8 + x] = color(c);
            }
        }
    }
    out
}

/// CGB 팔레트 RAM에 적힌 15비트 BGR 값.
pub fn palette_word(ram: &[u8; 64], palette: u8, color: u8) -> u16 {
    let i = usize::from(palette & 7) * 8 + usize::from(color & 3) * 2;
    u16::from_le_bytes([ram[i], ram[i + 1]])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 뱅크 하나 크기의 빈 VRAM에 타일 `index`를 `rows`(2바이트씩 8줄)로 채운다.
    fn vram_with_tile(index: usize, rows: [[u8; 2]; 8]) -> Vec<u8> {
        let mut vram = vec![0; 0x2000];
        for (y, [lo, hi]) in rows.into_iter().enumerate() {
            vram[index * 16 + y * 2] = lo;
            vram[index * 16 + y * 2 + 1] = hi;
        }
        vram
    }

    /// 첫 줄만 색 0,1,2,3,0,0,0,0인 타일
    const STRIPE: [[u8; 2]; 8] = [
        [0b0101_0000, 0b0011_0000],
        [0; 2],
        [0; 2],
        [0; 2],
        [0; 2],
        [0; 2],
        [0; 2],
        [0; 2],
    ];

    #[test]
    fn tile_rows_combine_low_and_high_bit_planes() {
        assert_eq!(tile_row(0b0101_0000, 0b0011_0000), [0, 1, 2, 3, 0, 0, 0, 0]);
        assert_eq!(tile_row(0xFF, 0xFF), [3; 8]);
    }

    #[test]
    fn tile_sheet_lays_out_16_tiles_per_row() {
        let vram = vram_with_tile(17, STRIPE);
        let sheet = tile_sheet(&vram, GRAY);
        assert_eq!((SHEET_WIDTH, SHEET_HEIGHT), (128, 192));
        assert_eq!(sheet.len(), 128 * 192);
        // 타일 17은 둘째 줄 둘째 칸: (8, 8)
        let at = |x: usize, y: usize| sheet[y * SHEET_WIDTH + x];
        assert_eq!([at(8, 8), at(9, 8), at(10, 8), at(11, 8)], GRAY);
        assert_eq!(at(9, 9), GRAY[0]);
        assert_eq!(at(1, 0), GRAY[0]);
    }

    #[test]
    fn signed_tile_numbers_start_at_0x9000() {
        assert_eq!(tile_index(0x00, true), 0);
        assert_eq!(tile_index(0xFF, true), 255);
        assert_eq!(tile_index(0x00, false), 256);
        assert_eq!(tile_index(0x7F, false), 383);
        assert_eq!(tile_index(0x80, false), 128);
    }

    #[test]
    fn map_source_follows_lcdc() {
        let both_high = MapSource {
            high_map: true,
            unsigned_tiles: true,
        };
        assert_eq!(MapSource::from_lcdc(0x08 | 0x10, false), both_high);
        assert_eq!(
            MapSource::from_lcdc(0x08 | 0x10, true),
            MapSource {
                high_map: false,
                ..both_high
            }
        );
        assert_eq!(
            MapSource::from_lcdc(0x40, true),
            MapSource {
                high_map: true,
                unsigned_tiles: false
            }
        );
    }

    #[test]
    fn map_follows_tile_numbers_and_addressing_mode() {
        let mut vram = vram_with_tile(256, STRIPE);
        vram[0x1800 + 33] = 0x00; // 0x9800 맵 (1, 1) 칸
        let source = MapSource {
            high_map: false,
            unsigned_tiles: false,
        };
        let map = tile_map(&vram, None, source, |p, c| u32::from(p) << 8 | u32::from(c));
        assert_eq!(map.len(), 256 * 256);
        let row = &map[8 * MAP_SIZE + 8..8 * MAP_SIZE + 12];
        assert_eq!(row, [0, 1, 2, 3]);
        let unsigned = tile_map(
            &vram,
            None,
            MapSource {
                unsigned_tiles: true,
                ..source
            },
            |_, c| u32::from(c),
        );
        assert_eq!(unsigned[8 * MAP_SIZE + 9], 0, "타일 0은 비어 있다");

        let mut high = vram.clone();
        high[0x1C00] = 0x00;
        high[0x1800] = 0x01; // 0x9800 맵은 빈 타일 257
        let map = tile_map(
            &high,
            None,
            MapSource {
                high_map: true,
                ..source
            },
            |_, c| u32::from(c),
        );
        assert_eq!(map[1], 1, "0x9C00 맵의 첫 칸");
    }

    #[test]
    fn cgb_map_attributes_pick_palette_bank_and_flip() {
        let bank0 = vec![0; 0x2000];
        let bank1 = {
            let mut b = vram_with_tile(0, STRIPE);
            b[0x1800] = 0x08 | 0x20 | 0x05; // 뱅크 1, 가로 뒤집기, 팔레트 5
            b[0x1800 + 1] = 0x08 | 0x40; // 뱅크 1, 세로 뒤집기
            b
        };
        let source = MapSource {
            high_map: false,
            unsigned_tiles: true,
        };
        let map = tile_map(&bank0, Some(&bank1), source, |p, c| {
            u32::from(p) << 8 | u32::from(c)
        });
        assert_eq!(&map[4..8], [0x503, 0x502, 0x501, 0x500]);
        assert_eq!(
            map[7 * MAP_SIZE + 8 + 1],
            1,
            "세로로 뒤집혀 첫 줄이 맨 아래로 간다"
        );
        assert_eq!(map[8 + 1], 0);
    }

    #[test]
    fn viewport_outline_wraps_around_the_map() {
        let mut map = vec![0; MAP_SIZE * MAP_SIZE];
        outline_viewport(&mut map, 200, 250, 9);
        let at = |x: usize, y: usize| map[y * MAP_SIZE + x];
        assert_eq!(at(200, 250), 9);
        assert_eq!(at((200 + 159) % 256, (250 + 143) % 256), 9);
        assert_eq!(at(0, 250), 9, "위쪽 테두리가 오른쪽 끝을 넘어 이어진다");
        assert_eq!(at(201, 251), 0, "안쪽은 그대로다");
        assert_eq!(
            map.iter().filter(|&&p| p == 9).count(),
            2 * 160 + 2 * 144 - 4
        );
    }

    #[test]
    fn sprites_decode_oam_entries() {
        let mut oam = [0u8; 0xA0];
        oam[4..8].copy_from_slice(&[16, 8, 0x42, 0x80 | 0x40 | 0x20 | 0x10 | 0x08 | 0x03]);
        let list = sprites(&oam);
        assert_eq!(list.len(), 40);
        let s = list[1];
        assert_eq!((s.index, s.y, s.x, s.tile), (1, 16, 8, 0x42));
        assert!(s.flip_x() && s.flip_y() && s.behind_bg());
        assert_eq!((s.palette(false), s.palette(true)), (1, 3));
        assert_eq!((s.bank(false), s.bank(true)), (0, 1));
        assert!(s.on_screen(false));
        assert!(!list[0].on_screen(false), "좌표 0은 화면 밖이다");
        // 게임이 스프라이트를 숨길 때 쓰는 큰 Y
        let hidden = Sprite {
            y: 0xF8,
            x: 20,
            ..s
        };
        assert!(!hidden.on_screen(false) && !hidden.on_screen(true));
        let tall_top = Sprite { y: 2, x: 8, ..s };
        assert!(!tall_top.on_screen(false) && tall_top.on_screen(true));
    }

    #[test]
    fn sprite_images_flip_and_stack_tall_tiles() {
        let mut tiles = vram_with_tile(4, STRIPE);
        tiles[5 * 16] = 0xFF; // 타일 5 첫 줄: 색 1
        let color = |c: u8| u32::from(c);
        let plain = Sprite {
            index: 0,
            y: 16,
            x: 8,
            tile: 4,
            attr: 0,
        };
        let img = sprite_image(&plain, &tiles, false, color);
        assert_eq!(img.len(), 64);
        assert_eq!(&img[..4], [TRANSPARENT, 1, 2, 3]);

        let flipped = Sprite {
            attr: 0x20,
            ..plain
        };
        assert_eq!(
            &sprite_image(&flipped, &tiles, false, color)[4..8],
            [3, 2, 1, TRANSPARENT]
        );

        let tall = Sprite { tile: 5, ..plain }; // 8×16에서는 번호의 비트 0을 무시한다
        let img = sprite_image(&tall, &tiles, true, color);
        assert_eq!(img.len(), 128);
        assert_eq!(img[1], 1, "위 타일 4");
        assert_eq!(img[8 * 8], 1, "아래 타일 5");
        let tall_flipped = Sprite { attr: 0x40, ..tall };
        let img = sprite_image(&tall_flipped, &tiles, true, color);
        assert_eq!(img[15 * 8 + 1], 1, "세로 뒤집기는 8×16 전체를 뒤집는다");
        assert_eq!(img[7 * 8], 1);
    }

    #[test]
    fn palette_words_are_little_endian_bgr555() {
        let mut ram = [0u8; 64];
        ram[7 * 8 + 3 * 2] = 0x1F;
        ram[7 * 8 + 3 * 2 + 1] = 0x7C;
        assert_eq!(palette_word(&ram, 7, 3), 0x7C1F);
        assert_eq!(palette_word(&ram, 0, 0), 0);
    }
}
