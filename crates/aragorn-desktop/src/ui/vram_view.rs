//! VRAM 창: 타일, 배경·윈도우 맵, OAM, 팔레트 (스펙 §6.3).

use std::collections::HashMap;

use aragorn_app::debug::vram::{self, MapSource, Sprite};
use eframe::egui;
use gb_core::{DebugView, PaletteKind};

use super::screen::to_rgba;

/// 타일 시트와 맵을 그릴 배율.
const ZOOM: f32 = 2.0;
/// 스프라이트 미리보기 배율.
const SPRITE_ZOOM: f32 = 3.0;
/// 맵 위 화면 테두리 색.
const VIEWPORT: u32 = 0xFF30_30FF;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Tiles,
    Map,
    Oam,
    Palettes,
}

/// 배율 `zoom`으로 그린 타일 시트에서 `pos`(시트 왼쪽 위 기준)에 있는 타일 번호(0–383).
pub fn tile_at(pos: egui::Vec2, zoom: f32) -> Option<usize> {
    if pos.x < 0.0 || pos.y < 0.0 {
        return None;
    }
    let (x, y) = ((pos.x / zoom) as usize / 8, (pos.y / zoom) as usize / 8);
    (x < vram::SHEET_COLUMNS && y < vram::SHEET_HEIGHT / 8).then_some(y * vram::SHEET_COLUMNS + x)
}

#[derive(Default)]
pub struct VramView {
    pub open: bool,
    tab: Tab,
    bank: u8,
    /// 윈도우 레이어 맵을 볼지 (아니면 배경 맵)
    window_map: bool,
    textures: HashMap<String, egui::TextureHandle>,
}

impl VramView {
    pub fn show(&mut self, ctx: &egui::Context, debug: &DebugView<'_>) {
        let mut open = self.open;
        egui::Window::new("VRAM")
            .open(&mut open)
            .default_width(540.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.tab, Tab::Tiles, "타일");
                    ui.selectable_value(&mut self.tab, Tab::Map, "배경 맵");
                    ui.selectable_value(&mut self.tab, Tab::Oam, "OAM");
                    ui.selectable_value(&mut self.tab, Tab::Palettes, "팔레트");
                });
                ui.separator();
                // 창이 이미지보다 작아도 끝까지 볼 수 있게 한다.
                egui::ScrollArea::both().show(ui, |ui| match self.tab {
                    Tab::Tiles => self.tiles(ui, debug),
                    Tab::Map => self.map(ui, debug),
                    Tab::Oam => self.oam(ui, debug),
                    Tab::Palettes => palettes(ui, debug),
                });
            });
        self.open = open;
    }

    /// 텍스처를 `pixels`로 갱신하고 그 id를 돌려준다.
    fn texture(
        &mut self,
        ctx: &egui::Context,
        name: &str,
        size: [usize; 2],
        pixels: &[u32],
    ) -> egui::TextureId {
        let image = egui::ColorImage::from_rgba_unmultiplied(size, &to_rgba(pixels));
        let options = egui::TextureOptions::NEAREST;
        match self.textures.get_mut(name) {
            Some(texture) => {
                texture.set(image, options);
                texture.id()
            }
            None => {
                let texture = ctx.load_texture(name, image, options);
                let id = texture.id();
                self.textures.insert(name.to_string(), texture);
                id
            }
        }
    }

    fn tiles(&mut self, ui: &mut egui::Ui, debug: &DebugView<'_>) {
        if debug.cgb() {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.bank, 0, "뱅크 0");
                ui.selectable_value(&mut self.bank, 1, "뱅크 1");
            });
        } else {
            self.bank = 0;
        }
        let sheet = vram::tile_sheet(debug.vram(self.bank), vram::GRAY);
        let id = self.texture(
            ui.ctx(),
            "vram-tiles",
            [vram::SHEET_WIDTH, vram::SHEET_HEIGHT],
            &sheet,
        );
        let size = egui::vec2(vram::SHEET_WIDTH as f32, vram::SHEET_HEIGHT as f32) * ZOOM;
        let response = ui.add(egui::Image::new((id, size)).sense(egui::Sense::hover()));
        if let Some(pos) = response.hover_pos()
            && let Some(tile) = tile_at(pos - response.rect.min, ZOOM)
        {
            let addr = 0x8000 + tile * 16;
            response.on_hover_text(format!("뱅크 {} 타일 {tile} (${addr:04X})", self.bank));
        }
    }

    fn map(&mut self, ui: &mut egui::Ui, debug: &DebugView<'_>) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.window_map, false, "배경");
            ui.selectable_value(&mut self.window_map, true, "윈도우");
        });
        let lcdc = debug.peek(0xFF40);
        let source = MapSource::from_lcdc(lcdc, self.window_map);
        let bank1 = debug.cgb().then(|| debug.vram(1));
        let mut map = vram::tile_map(debug.vram(0), bank1, source, |p, c| {
            debug.palette_color(PaletteKind::Bg, p, c)
        });
        if !self.window_map {
            vram::outline_viewport(&mut map, debug.peek(0xFF43), debug.peek(0xFF42), VIEWPORT);
        }
        ui.label(format!(
            "맵 {}, 타일 {}{}",
            if source.high_map { "$9C00" } else { "$9800" },
            if source.unsigned_tiles {
                "$8000"
            } else {
                "$8800"
            },
            if self.window_map {
                ""
            } else {
                " · 빨간 테두리는 지금 화면"
            }
        ));
        let id = self.texture(ui.ctx(), "vram-map", [vram::MAP_SIZE, vram::MAP_SIZE], &map);
        let size = egui::Vec2::splat(vram::MAP_SIZE as f32 * ZOOM);
        ui.add(egui::Image::new((id, size)));
    }

    fn oam(&mut self, ui: &mut egui::Ui, debug: &DebugView<'_>) {
        let cgb = debug.cgb();
        let tall = debug.peek(0xFF40) & 0x04 != 0;
        let height = if tall { 16 } else { 8 };
        let sprites = vram::sprites(debug.oam());
        egui::Grid::new("oam").striped(true).show(ui, |ui| {
            for title in ["#", "", "X", "Y", "타일", "속성"] {
                ui.strong(title);
            }
            ui.end_row();
            for sprite in &sprites {
                let image = vram::sprite_image(sprite, debug.vram(sprite.bank(cgb)), tall, |c| {
                    debug.palette_color(PaletteKind::Obj, sprite.palette(cgb), c)
                });
                let id = self.texture(
                    ui.ctx(),
                    &format!("oam-{}", sprite.index),
                    [8, height],
                    &image,
                );
                let mono = |text: String| {
                    let text = egui::RichText::new(text).monospace();
                    if sprite.on_screen(tall) {
                        text
                    } else {
                        text.weak()
                    }
                };
                ui.label(mono(sprite.index.to_string()));
                ui.add(egui::Image::new((
                    id,
                    egui::vec2(8.0, height as f32) * SPRITE_ZOOM,
                )));
                ui.label(mono(format!("{:3}", i16::from(sprite.x) - 8)));
                ui.label(mono(format!("{:3}", i16::from(sprite.y) - 16)));
                ui.label(mono(format!("${:02X}", sprite.tile)));
                ui.label(mono(attributes(sprite, cgb)));
                ui.end_row();
            }
        });
    }
}

/// 속성 바이트 풀이: `팔레트 3 뱅크 1 ↔ ↕ BG뒤`
fn attributes(sprite: &Sprite, cgb: bool) -> String {
    let mut parts = vec![if cgb {
        format!("팔레트 {} 뱅크 {}", sprite.palette(true), sprite.bank(true))
    } else {
        format!("OBP{}", sprite.palette(false))
    }];
    if sprite.flip_x() {
        parts.push("↔".into());
    }
    if sprite.flip_y() {
        parts.push("↕".into());
    }
    if sprite.behind_bg() {
        parts.push("BG뒤".into());
    }
    parts.join(" ")
}

fn swatch(ui: &mut egui::Ui, rgba: u32, hover: String) {
    let [r, g, b, _] = rgba.to_be_bytes();
    let (rect, response) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 2.0, egui::Color32::from_rgb(r, g, b));
    response.on_hover_text(hover);
}

fn palettes(ui: &mut egui::Ui, debug: &DebugView<'_>) {
    if debug.cgb() {
        for (kind, title) in [(PaletteKind::Bg, "BG"), (PaletteKind::Obj, "OBJ")] {
            ui.strong(title);
            egui::Grid::new(title).show(ui, |ui| {
                for palette in 0..8u8 {
                    ui.label(format!("{palette}"));
                    for color in 0..4u8 {
                        let word = vram::palette_word(debug.palette_ram(kind), palette, color);
                        let rgba = debug.palette_color(kind, palette, color);
                        swatch(ui, rgba, format!("색 {color}: ${word:04X}"));
                    }
                    ui.end_row();
                }
            });
        }
    } else {
        egui::Grid::new("dmg-palettes").show(ui, |ui| {
            for (kind, palette, name, addr) in [
                (PaletteKind::Bg, 0, "BGP", 0xFF47),
                (PaletteKind::Obj, 0, "OBP0", 0xFF48),
                (PaletteKind::Obj, 1, "OBP1", 0xFF49),
            ] {
                ui.label(format!("{name} ${:02X}", debug.peek(addr)));
                for color in 0..4u8 {
                    swatch(
                        ui,
                        debug.palette_color(kind, palette, color),
                        format!("색 {color}"),
                    );
                }
                ui.end_row();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hovered_tile_follows_zoomed_grid() {
        assert_eq!(tile_at(egui::vec2(0.0, 0.0), 2.0), Some(0));
        assert_eq!(tile_at(egui::vec2(15.9, 15.9), 2.0), Some(0));
        assert_eq!(tile_at(egui::vec2(16.0, 16.0), 2.0), Some(17));
        assert_eq!(tile_at(egui::vec2(255.0, 383.0), 2.0), Some(383));
        assert_eq!(tile_at(egui::vec2(256.0, 0.0), 2.0), None);
        assert_eq!(tile_at(egui::vec2(0.0, 384.0), 2.0), None);
        assert_eq!(tile_at(egui::vec2(-1.0, 0.0), 2.0), None);
    }

    #[test]
    fn sprite_attributes_are_spelled_out() {
        let s = Sprite {
            index: 0,
            y: 16,
            x: 8,
            tile: 0,
            attr: 0x80 | 0x20 | 0x10 | 0x0B,
        };
        assert_eq!(attributes(&s, true), "팔레트 3 뱅크 1 ↔ BG뒤");
        assert_eq!(attributes(&s, false), "OBP1 ↔ BG뒤");
        assert_eq!(attributes(&Sprite { attr: 0x40, ..s }, false), "OBP0 ↕");
    }
}
