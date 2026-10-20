//! 화면 필터: 흑백 게임보이 팔레트와 게임보이 컬러 색 보정. LCD 격자는 앱이 화면 위에 그린다.

use gb_core::ppu::DEFAULT_DMG_PALETTE;
use serde::{Deserialize, Serialize};

/// 흑백(DMG) 게임의 4색. 컬러 게임에는 쓰지 않는다.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DmgPalette {
    /// 지금까지 쓰던 연두색 (BGB 기본값과 같다).
    #[default]
    Green,
    /// 원래 게임보이 화면의 탁한 녹색.
    Original,
    /// 게임보이 포켓의 회색.
    Pocket,
    /// 흰색과 검은색.
    Gray,
}

impl DmgPalette {
    pub const ALL: [DmgPalette; 4] = [
        DmgPalette::Green,
        DmgPalette::Original,
        DmgPalette::Pocket,
        DmgPalette::Gray,
    ];

    pub fn label(self) -> &'static str {
        match self {
            DmgPalette::Green => "연두",
            DmgPalette::Original => "원래 게임보이",
            DmgPalette::Pocket => "게임보이 포켓",
            DmgPalette::Gray => "흑백",
        }
    }

    /// 밝은 색부터 4색 (0xRRGGBBAA).
    pub fn colors(self) -> [u32; 4] {
        match self {
            DmgPalette::Green => DEFAULT_DMG_PALETTE,
            DmgPalette::Original => [0x9BBC0FFF, 0x8BAC0FFF, 0x306230FF, 0x0F380FFF],
            DmgPalette::Pocket => [0xC4CFA1FF, 0x8B956DFF, 0x4D533CFF, 0x1F1F1FFF],
            DmgPalette::Gray => [0xFFFFFFFF, 0xAAAAAAFF, 0x555555FF, 0x000000FF],
        }
    }
}

/// 설정 파일의 `[video]` 섹션.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VideoConfig {
    /// 모르는 이름(다음 버전에서 저장한 설정)이면 설정 전체를 버리지 않고 기본 팔레트를 쓴다.
    #[serde(deserialize_with = "lenient_palette")]
    pub palette: DmgPalette,
    /// 게임보이 컬러 화면처럼 색을 덜 진하게 한다. 컬러 게임에만 쓴다.
    pub color_correction: bool,
    /// 픽셀 사이에 LCD 격자를 그린다 (앱이 그린다).
    pub lcd_grid: bool,
    /// 슈퍼 게임보이를 지원하는 흑백 게임(포켓몬 레드·블루·옐로 등)에 게임이 정한 색을 입힌다.
    /// 끄면 흑백 게임 색(`palette`)을 쓴다.
    pub sgb_colors: bool,
    /// 슈퍼 게임보이 테두리를 그린다. 화면이 256×224가 된다.
    pub sgb_border: bool,
}

impl Default for VideoConfig {
    fn default() -> Self {
        Self {
            palette: DmgPalette::default(),
            color_correction: false,
            lcd_grid: false,
            sgb_colors: true,
            sgb_border: true,
        }
    }
}

/// 앱이 그릴 화면 한 장.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub width: usize,
    pub height: usize,
    /// 0xRRGGBBAA
    pub pixels: Vec<u32>,
}

impl Frame {
    /// 테두리를 포함했는지 (LCD 격자는 테두리가 없을 때만 그린다).
    pub fn has_border(&self) -> bool {
        self.width != gb_core::ppu::SCREEN_WIDTH
    }
}

/// 코어 화면에 필터와 슈퍼 게임보이 색·테두리를 적용한 화면.
pub fn display(gb: &gb_core::GameBoy, video: &VideoConfig) -> Frame {
    use gb_core::ppu::{SCREEN_HEIGHT, SCREEN_WIDTH};
    let sgb = gb.sgb();
    let game = match sgb {
        Some(sgb) if video.sgb_colors => sgb.colorize(gb.framebuffer()),
        _ => video.apply(gb.framebuffer(), gb.debug().cgb()),
    };
    match sgb
        .filter(|_| video.sgb_border)
        .and_then(|s| s.with_border(&game))
    {
        Some(pixels) => Frame {
            width: gb_core::sgb::BORDER_WIDTH,
            height: gb_core::sgb::BORDER_HEIGHT,
            pixels,
        },
        None => Frame {
            width: SCREEN_WIDTH,
            height: SCREEN_HEIGHT,
            pixels: game,
        },
    }
}

fn lenient_palette<'de, D: serde::Deserializer<'de>>(d: D) -> Result<DmgPalette, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}

impl VideoConfig {
    /// 아무것도 바꾸지 않는 설정인지. 그러면 필터를 건너뛴다.
    fn is_identity(&self, cgb: bool) -> bool {
        if cgb {
            !self.color_correction
        } else {
            self.palette == DmgPalette::Green
        }
    }

    /// 코어가 그린 화면(0xRRGGBBAA)에 필터를 적용한 것. `cgb`는 컬러로 그린 화면인지다.
    pub fn apply(&self, framebuffer: &[u32], cgb: bool) -> Vec<u32> {
        if self.is_identity(cgb) {
            return framebuffer.to_vec();
        }
        if cgb {
            framebuffer.iter().map(|&p| correct_color(p)).collect()
        } else {
            let colors = self.palette.colors();
            framebuffer
                .iter()
                .map(|&p| {
                    DEFAULT_DMG_PALETTE
                        .iter()
                        .position(|&c| c == p)
                        .map_or(p, |i| colors[i])
                })
                .collect()
        }
    }
}

/// 게임보이 컬러 LCD 색에 가깝게 바꾼다 (gambatte 방식: 채널을 섞고 조금 어둡게).
fn correct_color(pixel: u32) -> u32 {
    let channel = |shift: u32| (pixel >> shift) & 0xFF;
    // 코어는 5비트 색을 8비트로 늘린 값을 준다.
    let (r, g, b) = (channel(24) >> 3, channel(16) >> 3, channel(8) >> 3);
    let r2 = (r * 13 + g * 2 + b) >> 1;
    let g2 = (g * 3 + b) << 1;
    let b2 = (r * 3 + g * 2 + b * 11) >> 1;
    (r2.min(255) << 24) | (g2.min(255) << 16) | (b2.min(255) << 8) | (pixel & 0xFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_changes_nothing() {
        let screen = vec![0x1234_56FF, DEFAULT_DMG_PALETTE[2]];
        let config = VideoConfig::default();
        assert_eq!(config.apply(&screen, false), screen);
        assert_eq!(config.apply(&screen, true), screen);
    }

    #[test]
    fn dmg_palette_swaps_the_four_shades() {
        let config = VideoConfig {
            palette: DmgPalette::Gray,
            ..VideoConfig::default()
        };
        let screen = DEFAULT_DMG_PALETTE.to_vec();
        assert_eq!(config.apply(&screen, false), DmgPalette::Gray.colors());
        assert_eq!(
            config.apply(&[0x1234_56FF], true),
            [0x1234_56FF],
            "컬러 게임은 그대로"
        );
    }

    #[test]
    fn color_correction_keeps_white_and_black_and_mutes_red() {
        let config = VideoConfig {
            color_correction: true,
            ..VideoConfig::default()
        };
        let out = config.apply(&[0xFFFF_FFFF, 0x0000_00FF, 0xFF00_00FF], true);
        assert_eq!(out[0], 0xF8F8_F8FF);
        assert_eq!(out[1], 0x0000_00FF);
        let red = out[2];
        assert!((red >> 24) > 150 && (red >> 8 & 0xFF) > 0, "{red:08X}");
    }

    #[test]
    fn missing_fields_use_defaults() {
        let config: VideoConfig = serde_json::from_str(r#"{"palette":"pocket"}"#).unwrap();
        assert_eq!(config.palette, DmgPalette::Pocket);
        assert!(!config.lcd_grid);
        let config: VideoConfig =
            serde_json::from_str(r#"{"palette":"neon","lcd_grid":true}"#).unwrap();
        assert_eq!(config.palette, DmgPalette::Green, "모르는 팔레트");
        assert!(config.lcd_grid);
        assert!(
            config.sgb_colors && config.sgb_border,
            "SGB는 기본으로 켠다"
        );
    }

    #[test]
    fn display_without_sgb_is_the_filtered_screen() {
        let mut rom = vec![0u8; 0x8000];
        rom[0x0100..0x0102].copy_from_slice(&[0x18, 0xFE]);
        let gb = gb_core::GameBoy::new(rom.clone(), gb_core::Model::Auto).unwrap();
        let frame = display(&gb, &VideoConfig::default());
        assert_eq!((frame.width, frame.height), (160, 144));
        assert!(!frame.has_border());

        // SGB 게임이지만 아직 테두리를 받지 않았다: 160×144에 SGB 기본 색.
        rom[0x0146] = 0x03;
        rom[0x014B] = 0x33;
        let gb = gb_core::GameBoy::new(rom, gb_core::Model::Auto).unwrap();
        let frame = display(&gb, &VideoConfig::default());
        assert_eq!(frame.width, 160);
        assert_ne!(frame.pixels[0], DEFAULT_DMG_PALETTE[0], "SGB 색");
        let off = VideoConfig {
            sgb_colors: false,
            ..VideoConfig::default()
        };
        assert_eq!(display(&gb, &off).pixels[0], DEFAULT_DMG_PALETTE[0]);
    }
}
