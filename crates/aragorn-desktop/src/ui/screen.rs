//! 에뮬레이터 화면: 프레임버퍼에 화면 필터를 적용해 egui 텍스처로 올리고 정수 배율로 그린다.

use aragorn_app::video::VideoConfig;
use eframe::egui;
use gb_core::ppu::{SCREEN_HEIGHT, SCREEN_WIDTH};

/// 0xRRGGBBAA 픽셀을 RGBA 바이트로 바꾼다.
pub fn to_rgba(framebuffer: &[u32]) -> Vec<u8> {
    framebuffer.iter().flat_map(|p| p.to_be_bytes()).collect()
}

/// 주어진 영역에 들어가는 가장 큰 정수 배율 (최소 1).
pub fn integer_scale(available: egui::Vec2) -> f32 {
    let sx = (available.x / SCREEN_WIDTH as f32).floor();
    let sy = (available.y / SCREEN_HEIGHT as f32).floor();
    sx.min(sy).max(1.0)
}

/// LCD 격자를 그리는 최소 배율. 이보다 작으면 격자가 화면을 가린다.
pub const GRID_MIN_SCALE: f32 = 3.0;

#[derive(Default)]
pub struct ScreenView {
    texture: Option<egui::TextureHandle>,
    video: VideoConfig,
    /// 게임이 컬러로 그리는지. 필터가 다르다.
    cgb: bool,
}

impl ScreenView {
    /// 화면 필터를 바꾼다. 다음 `update`부터 적용한다.
    pub fn set_video(&mut self, video: VideoConfig, cgb: bool) {
        self.video = video;
        self.cgb = cgb;
    }

    pub fn update(&mut self, ctx: &egui::Context, framebuffer: &[u32]) {
        let filtered = self.video.apply(framebuffer, self.cgb);
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [SCREEN_WIDTH, SCREEN_HEIGHT],
            &to_rgba(&filtered),
        );
        match &mut self.texture {
            Some(texture) => texture.set(image, egui::TextureOptions::NEAREST),
            None => {
                self.texture =
                    Some(ctx.load_texture("screen", image, egui::TextureOptions::NEAREST));
            }
        }
    }

    pub fn show(&self, ui: &mut egui::Ui) {
        let Some(texture) = &self.texture else {
            return;
        };
        let scale = integer_scale(ui.available_size());
        let size = egui::vec2(SCREEN_WIDTH as f32, SCREEN_HEIGHT as f32) * scale;
        let grid = self.video.lcd_grid && scale >= GRID_MIN_SCALE;
        ui.centered_and_justified(|ui| {
            let response = ui.add(egui::Image::new((texture.id(), size)));
            if grid {
                paint_grid(ui.painter(), response.rect, scale);
            }
        });
    }
}

/// 픽셀 사이에 어두운 선을 그려 LCD처럼 보이게 한다.
fn paint_grid(painter: &egui::Painter, rect: egui::Rect, scale: f32) {
    let stroke = egui::Stroke::new(1.0, egui::Color32::from_black_alpha(60));
    for x in 1..SCREEN_WIDTH {
        let x = rect.left() + x as f32 * scale;
        painter.vline(x, rect.y_range(), stroke);
    }
    for y in 1..SCREEN_HEIGHT {
        let y = rect.top() + y as f32 * scale;
        painter.hline(rect.x_range(), y, stroke);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixels_become_rgba_bytes() {
        assert_eq!(
            to_rgba(&[0x1122_3344, 0xAABB_CCDD]),
            [0x11, 0x22, 0x33, 0x44, 0xAA, 0xBB, 0xCC, 0xDD]
        );
    }

    #[test]
    fn scale_is_largest_integer_that_fits() {
        assert_eq!(integer_scale(egui::vec2(640.0, 576.0)), 4.0);
        assert_eq!(integer_scale(egui::vec2(500.0, 1000.0)), 3.0);
        assert_eq!(integer_scale(egui::vec2(100.0, 100.0)), 1.0);
    }
}
