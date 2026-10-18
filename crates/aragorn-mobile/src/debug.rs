//! 디버거 화면이 그릴 내용. 데스크톱 디버거 창(CPU·메모리·VRAM·I/O)과 같은 계산을 `aragorn-app`에서
//! 가져와, 앱이 그대로 그릴 수 있는 값으로 만든다.

use crate::types::Image;
use aragorn_app::debug::{
    cpu, io,
    vram::{self, MapSource},
};
use gb_core::{DebugView, PaletteKind};

/// 배경 맵 위 지금 화면 테두리 색.
const VIEWPORT: u32 = 0xFF30_30FF;

#[derive(Debug, Clone, uniffi::Record)]
pub struct CpuView {
    /// 브레이크포인트 정지, 잘못된 옵코드 같은 경고.
    pub notices: Vec<String>,
    pub registers: Vec<String>,
    pub rows: Vec<DisasmLine>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct DisasmLine {
    pub pc: u16,
    pub text: String,
    /// PC부터의 명령이면 참, 최근 실행한 명령이면 거짓(흐리게 보인다).
    pub ahead: bool,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MapView {
    pub image: Image,
    /// `맵 $9800, 타일 $8000 · 빨간 테두리는 지금 화면`
    pub label: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct SpriteView {
    pub index: u8,
    pub image: Image,
    /// 화면 좌표 (OAM 값 - 8, - 16)
    pub x: i16,
    pub y: i16,
    pub tile: u8,
    pub attributes: String,
    /// 화면 밖이면 흐리게 보인다.
    pub on_screen: bool,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct PaletteGroup {
    pub title: String,
    pub rows: Vec<PaletteRow>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct PaletteRow {
    pub label: String,
    pub colors: Vec<Swatch>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct Swatch {
    /// 0xRRGGBBAA
    pub rgba: u32,
    pub label: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct IoGroup {
    pub name: String,
    pub rows: Vec<IoRow>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct IoRow {
    pub name: String,
    pub address: u16,
    pub value: u8,
    pub description: String,
}

/// `0150`, `$C000`, `0xFF44` 같은 16진수 주소. 읽을 수 없으면 `None`.
#[uniffi::export]
pub fn parse_address(text: String) -> Option<u16> {
    cpu::parse_address(&text)
}

/// 메모리 뷰어의 전체 줄 수.
#[uniffi::export]
pub fn memory_row_count() -> u32 {
    cpu::MEMORY_ROWS as u32
}

pub fn cpu_view(debug: &DebugView<'_>) -> CpuView {
    CpuView {
        notices: cpu::stop_notices(debug),
        registers: cpu::register_lines(debug).into(),
        rows: cpu::disassembly(debug)
            .into_iter()
            .map(|row| DisasmLine {
                pc: row.pc,
                text: row.text,
                ahead: row.ahead,
            })
            .collect(),
    }
}

pub fn memory_rows(debug: &DebugView<'_>, first: u32, count: u32) -> Vec<String> {
    let first = first as usize;
    let end = (first + count as usize).min(cpu::MEMORY_ROWS);
    (first..end)
        .map(|row| cpu::memory_row(debug, row))
        .collect()
}

/// 타일 시트. DMG에는 뱅크 1이 없어 뱅크 0을 보인다.
pub fn tiles(debug: &DebugView<'_>, bank: u8) -> Image {
    let bank = if debug.cgb() { bank.min(1) } else { 0 };
    let sheet = vram::tile_sheet(debug.vram(bank), vram::GRAY);
    Image::from_pixels(vram::SHEET_WIDTH, vram::SHEET_HEIGHT, &sheet)
}

pub fn tile_map(debug: &DebugView<'_>, window: bool) -> MapView {
    let source = MapSource::from_lcdc(debug.peek(0xFF40), window);
    let bank1 = debug.cgb().then(|| debug.vram(1));
    let mut map = vram::tile_map(debug.vram(0), bank1, source, |p, c| {
        debug.palette_color(PaletteKind::Bg, p, c)
    });
    if !window {
        vram::outline_viewport(&mut map, debug.peek(0xFF43), debug.peek(0xFF42), VIEWPORT);
    }
    let label = format!(
        "맵 {}, 타일 {}{}",
        if source.high_map { "$9C00" } else { "$9800" },
        if source.unsigned_tiles {
            "$8000"
        } else {
            "$8800"
        },
        if window {
            ""
        } else {
            " · 빨간 테두리는 지금 화면"
        }
    );
    MapView {
        image: Image::from_pixels(vram::MAP_SIZE, vram::MAP_SIZE, &map),
        label,
    }
}

pub fn sprites(debug: &DebugView<'_>) -> Vec<SpriteView> {
    let cgb = debug.cgb();
    let tall = debug.peek(0xFF40) & 0x04 != 0;
    let height = if tall { 16 } else { 8 };
    vram::sprites(debug.oam())
        .iter()
        .map(|sprite| {
            let pixels = vram::sprite_image(sprite, debug.vram(sprite.bank(cgb)), tall, |c| {
                debug.palette_color(PaletteKind::Obj, sprite.palette(cgb), c)
            });
            SpriteView {
                index: sprite.index,
                image: Image::from_pixels(8, height, &pixels),
                x: i16::from(sprite.x) - 8,
                y: i16::from(sprite.y) - 16,
                tile: sprite.tile,
                attributes: sprite.describe(cgb),
                on_screen: sprite.on_screen(tall),
            }
        })
        .collect()
}

/// CGB는 BG·OBJ 팔레트 8개씩, DMG는 BGP·OBP0·OBP1.
pub fn palettes(debug: &DebugView<'_>) -> Vec<PaletteGroup> {
    if debug.cgb() {
        [(PaletteKind::Bg, "BG"), (PaletteKind::Obj, "OBJ")]
            .into_iter()
            .map(|(kind, title)| PaletteGroup {
                title: title.to_string(),
                rows: (0..8u8)
                    .map(|palette| PaletteRow {
                        label: palette.to_string(),
                        colors: (0..4u8)
                            .map(|color| {
                                let word =
                                    vram::palette_word(debug.palette_ram(kind), palette, color);
                                Swatch {
                                    rgba: debug.palette_color(kind, palette, color),
                                    label: format!("색 {color}: ${word:04X}"),
                                }
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .collect()
    } else {
        let rows = [
            (PaletteKind::Bg, 0, "BGP", 0xFF47),
            (PaletteKind::Obj, 0, "OBP0", 0xFF48),
            (PaletteKind::Obj, 1, "OBP1", 0xFF49),
        ]
        .into_iter()
        .map(|(kind, palette, name, addr)| PaletteRow {
            label: format!("{name} ${:02X}", debug.peek(addr)),
            colors: (0..4u8)
                .map(|color| Swatch {
                    rgba: debug.palette_color(kind, palette, color),
                    label: format!("색 {color}"),
                })
                .collect(),
        })
        .collect();
        vec![PaletteGroup {
            title: "DMG".to_string(),
            rows,
        }]
    }
}

pub fn io_registers(debug: &DebugView<'_>) -> Vec<IoGroup> {
    io::GROUPS
        .iter()
        .map(|(group, registers)| IoGroup {
            name: group.to_string(),
            rows: registers
                .iter()
                .map(|&(address, name)| {
                    let value = debug.peek(address);
                    IoRow {
                        name: name.to_string(),
                        address,
                        value,
                        description: io::describe(address, value),
                    }
                })
                .collect(),
        })
        .collect()
}

pub fn channels_muted(debug: &DebugView<'_>) -> Vec<bool> {
    (0..4).map(|channel| debug.channel_muted(channel)).collect()
}
