//! dmg-acid2 인수 테스트 (스펙 §8-3): 프레임버퍼를 기준 스크린샷과 픽셀 단위로 비교한다.

mod common;

use gb_core::{GameBoy, Model, ppu::DEFAULT_DMG_PALETTE};

const WIDTH: usize = 160;
const HEIGHT: usize = 144;

/// 기준 PNG(2비트 그레이스케일, 3 = 흰색)를 음영 번호(0 = 가장 밝음)로 바꾼다.
fn reference_shades(png_bytes: &[u8]) -> Vec<usize> {
    let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    let mut reader = decoder.read_info().expect("PNG 헤더");
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).expect("PNG 본문");
    assert_eq!((info.width, info.height), (WIDTH as u32, HEIGHT as u32));
    assert_eq!(
        (info.color_type, info.bit_depth),
        (png::ColorType::Grayscale, png::BitDepth::Two)
    );
    (0..HEIGHT)
        .flat_map(|y| (0..WIDTH).map(move |x| (x, y)))
        .map(|(x, y)| {
            let byte = buf[y * info.line_size + x / 4];
            3 - usize::from((byte >> (6 - 2 * (x % 4))) & 0x03)
        })
        .collect()
}

#[test]
fn dmg_acid2_matches_reference() {
    let (Some(rom), Some(reference)) = (
        common::load_rom("dmg-acid2/dmg-acid2.gb"),
        common::load_rom("dmg-acid2/reference-dmg.png"),
    ) else {
        return;
    };
    let mut gb = GameBoy::new(rom, Model::Dmg).expect("테스트 ROM 로드");
    for _ in 0..60 {
        gb.run_frame();
    }
    let expected = reference_shades(&reference);
    let wrong: Vec<(usize, usize)> = gb
        .framebuffer()
        .iter()
        .zip(&expected)
        .enumerate()
        .filter(|(_, (pixel, shade))| DEFAULT_DMG_PALETTE[**shade] != **pixel)
        .map(|(i, _)| (i % WIDTH, i / WIDTH))
        .collect();
    assert!(
        wrong.is_empty(),
        "{}개 픽셀이 다릅니다. 처음 몇 개: {:?}",
        wrong.len(),
        &wrong[..wrong.len().min(10)]
    );
}
