//! cgb-acid2 인수 테스트 (스펙 §8-3): CGB 모드 프레임버퍼를 기준 스크린샷과 픽셀 단위로 비교한다.

mod common;

use gb_core::{GameBoy, Model};

const WIDTH: usize = 160;
const HEIGHT: usize = 144;

/// 기준 PNG를 픽셀마다 0xRRGGBB로 바꾼다.
fn reference_rgb(png_bytes: &[u8]) -> Vec<u32> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().expect("PNG 헤더");
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).expect("PNG 본문");
    assert_eq!((info.width, info.height), (WIDTH as u32, HEIGHT as u32));
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    let channels = info.color_type.samples();
    (0..WIDTH * HEIGHT)
        .map(|i| {
            let p = &buf[(i / WIDTH) * info.line_size + (i % WIDTH) * channels..];
            u32::from(p[0]) << 16 | u32::from(p[1]) << 8 | u32::from(p[2])
        })
        .collect()
}

#[test]
fn cgb_acid2_matches_reference() {
    let (Some(rom), Some(reference)) = (
        common::load_rom("cgb-acid2/cgb-acid2.gbc"),
        common::load_rom("cgb-acid2/reference-cgb.png"),
    ) else {
        return;
    };
    let mut gb = GameBoy::new(rom, Model::Cgb).expect("테스트 ROM 로드");
    for _ in 0..60 {
        gb.run_frame();
    }
    let expected = reference_rgb(&reference);
    let wrong: Vec<(usize, usize, u32, u32)> = gb
        .framebuffer()
        .iter()
        .zip(&expected)
        .enumerate()
        .filter(|(_, (pixel, rgb))| **pixel >> 8 != **rgb)
        .map(|(i, (pixel, rgb))| (i % WIDTH, i / WIDTH, *pixel >> 8, *rgb))
        .collect();
    assert!(
        wrong.is_empty(),
        "{}개 픽셀이 다릅니다. 처음 몇 개 (x, y, 실제, 기대): {:x?}",
        wrong.len(),
        &wrong[..wrong.len().min(8)]
    );
}
