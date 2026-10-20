//! 스크린샷: 화면(0xRRGGBBAA)을 정수 배율로 키운 PNG로 만든다.

/// 스크린샷 배율. 160×144는 사진 앱에서 너무 작게 보인다.
pub const SCALE: usize = 3;

/// `width`×`height` 픽셀을 `scale`배로 키운 PNG 파일 내용.
pub fn png(pixels: &[u32], width: usize, height: usize, scale: usize) -> Vec<u8> {
    let scale = scale.max(1);
    let (out_w, out_h) = (width * scale, height * scale);
    let mut rgba = Vec::with_capacity(out_w * out_h * 4);
    for y in 0..out_h {
        let row = &pixels[(y / scale) * width..][..width];
        for &p in row {
            for _ in 0..scale {
                rgba.extend_from_slice(&p.to_be_bytes());
            }
        }
    }
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, out_w as u32, out_h as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(&rgba))
        .expect("메모리에 쓰는 PNG 인코딩은 실패하지 않는다");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_is_scaled_and_keeps_colors() {
        let data = png(&[0xFF00_00FF, 0x00FF_00FF], 2, 1, 3);
        let decoder = png::Decoder::new(&data[..]);
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!((info.width, info.height), (6, 3));
        assert_eq!(&buf[..4], &[0xFF, 0, 0, 0xFF]);
        assert_eq!(&buf[3 * 4..3 * 4 + 4], &[0, 0xFF, 0, 0xFF]);
    }
}
