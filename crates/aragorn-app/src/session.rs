//! 에뮬레이션 세션: 불러온 ROM 하나와 그 실행 속도 (스펙 §5.1).

use crate::pacing::FramePacer;
use gb_core::{CartError, GameBoy, Model};
use std::time::Duration;

pub struct Session {
    gb: GameBoy,
    pacer: FramePacer,
    title: String,
}

impl Session {
    pub fn load(rom: Vec<u8>) -> Result<Session, CartError> {
        let gb = GameBoy::new(rom, Model::Auto)?;
        let title = gb.header().title.trim().to_string();
        Ok(Session {
            gb,
            pacer: FramePacer::default(),
            title,
        })
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    /// 실제로 `elapsed`가 지났을 때 필요한 만큼 프레임을 돌리고, 돌린 프레임 수를 반환한다.
    pub fn advance(&mut self, elapsed: Duration) -> u32 {
        let frames = self.pacer.frames_for(elapsed);
        for _ in 0..frames {
            self.gb.run_frame();
        }
        frames
    }

    /// 160×144, 각 픽셀은 0xRRGGBBAA.
    pub fn framebuffer(&self) -> &[u32] {
        self.gb.framebuffer()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pacing::FRAME_DURATION;

    /// 제목 "ARAGORN TEST", 0x0100에서 제자리 루프(JR -2)를 도는 32KB ROM.
    fn looping_rom() -> Vec<u8> {
        let mut rom = vec![0; 0x8000];
        rom[0x0100..0x0102].copy_from_slice(&[0x18, 0xFE]);
        rom[0x0134..0x0140].copy_from_slice(b"ARAGORN TEST");
        rom
    }

    #[test]
    fn loads_rom_and_exposes_title() {
        let session = Session::load(looping_rom()).unwrap();
        assert_eq!(session.title(), "ARAGORN TEST");
        assert_eq!(session.framebuffer().len(), 160 * 144);
    }

    #[test]
    fn rejects_invalid_rom() {
        assert_eq!(
            Session::load(vec![0; 16]).err(),
            Some(CartError::TooSmall(16))
        );
    }

    #[test]
    fn advance_runs_frames_for_elapsed_time() {
        let mut session = Session::load(looping_rom()).unwrap();
        assert_eq!(session.advance(FRAME_DURATION * 2), 2);
        assert_eq!(session.advance(Duration::ZERO), 0);
    }
}
