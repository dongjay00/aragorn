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
        // CGB 하드웨어(VRAM/WRAM 뱅크, 팔레트)는 M6에서 구현한다. 그 전까지 CGB 플래그가 있는 ROM을
        // CGB 모드로 시작하면 화면과 메모리가 망가지므로 모두 DMG로 돌린다.
        let gb = GameBoy::new(rom, Model::Dmg)?;
        let title = gb.header().title.trim().to_string();
        Ok(Session {
            gb,
            pacer: FramePacer::default(),
            title,
        })
    }

    /// 실제로 에뮬레이션하는 기기.
    pub fn model(&self) -> Model {
        self.gb.model()
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
    fn cgb_flagged_rom_runs_as_dmg_until_cgb_support() {
        // 노랑·금·은처럼 CGB 플래그가 있는 ROM도 CGB 하드웨어(M6)가 생기기 전까지는 DMG로 돌린다.
        for flag in [0x80, 0xC0] {
            let mut rom = looping_rom();
            rom[0x0143] = flag;
            assert_eq!(
                Session::load(rom).unwrap().model(),
                Model::Dmg,
                "{flag:#04X}"
            );
        }
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
