//! 에뮬레이션 세션: 불러온 ROM 하나, 그 실행 속도, 배터리 세이브 (스펙 §5.1).

use crate::pacing::FramePacer;
use gb_core::{Button, CartError, GameBoy, Model};
use std::{io, time::Duration};

/// 게임이 외부 RAM을 켠 채로 두어도, 마지막 쓰기 뒤 이만큼(약 1초) 쓰기가 없으면 저장한다.
pub const SAVE_IDLE_FRAMES: u32 = 60;

/// ROM 하나의 배터리 세이브 저장소 (포트). 데스크톱에서는 ROM 옆 `.sav` 파일이다.
pub trait SaveStore {
    /// 저장된 세이브가 없으면 `Ok(None)`.
    fn load_battery(&self) -> io::Result<Option<Vec<u8>>>;
    /// 덮어쓰기 전에 이전 세이브를 백업 하나로 남긴다.
    fn save_battery(&self, data: &[u8]) -> io::Result<()>;
}

pub struct Session {
    gb: GameBoy,
    pacer: FramePacer,
    title: String,
    store: Box<dyn SaveStore>,
    /// 아직 저장하지 않은 외부 RAM 쓰기가 있는지.
    unsaved: bool,
    /// 마지막 외부 RAM 쓰기 뒤 지난 프레임 수.
    idle_frames: u32,
    /// 직전 저장이 실패했는지. 실패하면 `SAVE_IDLE_FRAMES`마다 다시 시도한다.
    save_failed: bool,
    /// 상태 표시줄에 보일 오류 문구.
    errors: Vec<String>,
}

impl Session {
    /// ROM을 불러오고, 배터리 카트리지면 `store`의 세이브를 싣는다.
    /// 세이브를 읽지 못해도 게임은 새로 시작하고 오류는 `take_errors`로 알린다.
    pub fn load(
        rom: Vec<u8>,
        store: Box<dyn SaveStore>,
        now_unix: u64,
    ) -> Result<Session, CartError> {
        // CGB 하드웨어(VRAM/WRAM 뱅크, 팔레트)는 M6에서 구현한다. 그 전까지 CGB 플래그가 있는 ROM을
        // CGB 모드로 시작하면 화면과 메모리가 망가지므로 모두 DMG로 돌린다.
        let mut gb = GameBoy::new(rom, Model::Dmg)?;
        let title = gb.header().title.trim().to_string();
        let mut errors = Vec::new();
        if gb.battery_ram().is_some() {
            match store.load_battery() {
                Ok(Some(data)) => gb.load_battery_ram(&data, now_unix),
                Ok(None) => {}
                Err(e) => errors.push(format!("세이브 파일을 읽을 수 없습니다: {e}")),
            }
        }
        Ok(Session {
            gb,
            pacer: FramePacer::default(),
            title,
            store,
            unsaved: false,
            idle_frames: 0,
            save_failed: false,
            errors,
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
            self.track_battery();
        }
        frames
    }

    pub fn set_button(&mut self, button: Button, pressed: bool) {
        self.gb.set_button(button, pressed);
    }

    /// 160×144, 각 픽셀은 0xRRGGBBAA.
    pub fn framebuffer(&self) -> &[u32] {
        self.gb.framebuffer()
    }

    pub fn battery_ram(&self) -> Option<Vec<u8>> {
        self.gb.battery_ram()
    }

    /// 저장하지 않은 세이브가 있으면 지금 저장한다. 앱 종료, ROM 교체, 업데이트 적용 전에 부른다.
    pub fn flush(&mut self) {
        if self.unsaved {
            self.save_battery();
        }
    }

    /// 쌓인 오류 문구를 꺼낸다.
    pub fn take_errors(&mut self) -> Vec<String> {
        std::mem::take(&mut self.errors)
    }

    /// 프레임마다 부른다. 게임이 저장을 마치고 외부 RAM을 끄면 바로, 켜 둔 채라면
    /// 쓰기가 멈추고 `SAVE_IDLE_FRAMES`가 지나면 저장한다.
    fn track_battery(&mut self) {
        if self.gb.battery_dirty() {
            self.unsaved = true;
            self.idle_frames = 0;
        } else if self.unsaved {
            self.idle_frames += 1;
        }
        let settled = !self.save_failed && !self.gb.cartridge_ram_enabled();
        if self.unsaved && (settled || self.idle_frames >= SAVE_IDLE_FRAMES) {
            self.save_battery();
        }
    }

    fn save_battery(&mut self) {
        let Some(ram) = self.gb.battery_ram() else {
            self.unsaved = false;
            return;
        };
        match self.store.save_battery(&ram) {
            Ok(()) => {
                self.unsaved = false;
                self.save_failed = false;
            }
            Err(e) => {
                if !self.save_failed {
                    self.errors
                        .push(format!("세이브 파일을 저장할 수 없습니다: {e}"));
                }
                self.save_failed = true;
                self.idle_frames = 0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pacing::FRAME_DURATION;
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };

    /// 테스트용 메모리 저장소. 저장 시도 횟수와 실패 여부를 조절할 수 있다.
    #[derive(Default)]
    struct MemoryStore {
        data: RefCell<Option<Vec<u8>>>,
        save_attempts: Cell<u32>,
        fail_load: bool,
        fail_save: Cell<bool>,
    }

    struct Shared(Rc<MemoryStore>);

    impl SaveStore for Shared {
        fn load_battery(&self) -> io::Result<Option<Vec<u8>>> {
            if self.0.fail_load {
                return Err(io::Error::other("디스크 오류"));
            }
            Ok(self.0.data.borrow().clone())
        }

        fn save_battery(&self, data: &[u8]) -> io::Result<()> {
            self.0.save_attempts.set(self.0.save_attempts.get() + 1);
            if self.0.fail_save.get() {
                return Err(io::Error::other("쓰기 금지"));
            }
            *self.0.data.borrow_mut() = Some(data.to_vec());
            Ok(())
        }
    }

    fn session_with(rom: Vec<u8>, store: MemoryStore) -> (Session, Rc<MemoryStore>) {
        let store = Rc::new(store);
        let session = Session::load(rom, Box::new(Shared(Rc::clone(&store))), 0).unwrap();
        (session, store)
    }

    /// 제목 "ARAGORN TEST", 0x0100에 `program`을 둔 32KB ROM (기본은 제자리 루프 JR -2).
    fn rom_with(cart_type: u8, ram_size_code: u8, program: &[u8]) -> Vec<u8> {
        let mut rom = vec![0; 0x8000];
        rom[0x0100..0x0100 + program.len()].copy_from_slice(program);
        rom[0x0134..0x0140].copy_from_slice(b"ARAGORN TEST");
        rom[0x0147] = cart_type;
        rom[0x0149] = ram_size_code;
        rom
    }

    fn looping_rom() -> Vec<u8> {
        rom_with(0x00, 0x00, &[0x18, 0xFE])
    }

    /// MBC1+RAM+BATTERY, 8KB. RAM 켜기 → 0xA000에 0x42 쓰기 → (`disable`이면) RAM 끄기 → 제자리 루프.
    fn saving_rom(disable: bool) -> Vec<u8> {
        let mut program = vec![
            0x3E, 0x0A, 0xEA, 0x00, 0x00, // LD A,0x0A ; LD (0x0000),A
            0x3E, 0x42, 0xEA, 0x00, 0xA0, // LD A,0x42 ; LD (0xA000),A
        ];
        if disable {
            program.extend([0xAF, 0xEA, 0x00, 0x00]); // XOR A ; LD (0x0000),A
        }
        program.extend([0x18, 0xFE]);
        rom_with(0x03, 0x02, &program)
    }

    fn run_frames(session: &mut Session, frames: u32) {
        for _ in 0..frames {
            session.advance(FRAME_DURATION);
        }
    }

    #[test]
    fn loads_rom_and_exposes_title() {
        let (session, _) = session_with(looping_rom(), MemoryStore::default());
        assert_eq!(session.title(), "ARAGORN TEST");
        assert_eq!(session.framebuffer().len(), 160 * 144);
    }

    #[test]
    fn cgb_flagged_rom_runs_as_dmg_until_cgb_support() {
        // 노랑·금·은처럼 CGB 플래그가 있는 ROM도 CGB 하드웨어(M6)가 생기기 전까지는 DMG로 돌린다.
        for flag in [0x80, 0xC0] {
            let mut rom = looping_rom();
            rom[0x0143] = flag;
            let (session, _) = session_with(rom, MemoryStore::default());
            assert_eq!(session.model(), Model::Dmg, "{flag:#04X}");
        }
    }

    #[test]
    fn rejects_invalid_rom() {
        let store = Box::new(Shared(Rc::new(MemoryStore::default())));
        assert_eq!(
            Session::load(vec![0; 16], store, 0).err(),
            Some(CartError::TooSmall(16))
        );
    }

    #[test]
    fn advance_runs_frames_for_elapsed_time() {
        let (mut session, _) = session_with(looping_rom(), MemoryStore::default());
        assert_eq!(session.advance(FRAME_DURATION * 2), 2);
        assert_eq!(session.advance(Duration::ZERO), 0);
    }

    #[test]
    fn existing_save_is_loaded_into_battery_ram() {
        let store = MemoryStore {
            data: RefCell::new(Some(vec![0x99, 0x88])),
            ..MemoryStore::default()
        };
        let (session, _) = session_with(saving_rom(true), store);
        assert_eq!(&session.battery_ram().unwrap()[..3], &[0x99, 0x88, 0x00]);
    }

    #[test]
    fn saves_when_game_disables_ram() {
        let (mut session, store) = session_with(saving_rom(true), MemoryStore::default());
        run_frames(&mut session, 1);
        assert_eq!(store.save_attempts.get(), 1);
        assert_eq!(store.data.borrow().as_ref().unwrap()[0], 0x42);
        run_frames(&mut session, 100);
        assert_eq!(
            store.save_attempts.get(),
            1,
            "새 쓰기가 없으면 다시 저장하지 않는다"
        );
    }

    #[test]
    fn saves_after_idle_frames_when_game_keeps_ram_enabled() {
        let (mut session, store) = session_with(saving_rom(false), MemoryStore::default());
        run_frames(&mut session, SAVE_IDLE_FRAMES);
        assert_eq!(store.save_attempts.get(), 0);
        run_frames(&mut session, 1);
        assert_eq!(store.save_attempts.get(), 1);
    }

    #[test]
    fn flush_saves_pending_writes() {
        let (mut session, store) = session_with(saving_rom(false), MemoryStore::default());
        run_frames(&mut session, 1);
        session.flush();
        assert_eq!(store.save_attempts.get(), 1);
        session.flush();
        assert_eq!(store.save_attempts.get(), 1);
    }

    #[test]
    fn carts_without_battery_never_touch_the_store() {
        let store = MemoryStore {
            fail_load: true,
            ..MemoryStore::default()
        };
        let (mut session, store) = session_with(looping_rom(), store);
        run_frames(&mut session, 3);
        session.flush();
        assert!(session.take_errors().is_empty());
        assert_eq!(store.save_attempts.get(), 0);
    }

    #[test]
    fn unreadable_save_is_reported_and_game_starts_fresh() {
        let store = MemoryStore {
            fail_load: true,
            ..MemoryStore::default()
        };
        let (mut session, _) = session_with(saving_rom(true), store);
        assert_eq!(
            session.take_errors(),
            ["세이브 파일을 읽을 수 없습니다: 디스크 오류"]
        );
        assert_eq!(session.battery_ram().unwrap()[0], 0x00);
    }

    #[test]
    fn failed_save_is_reported_once_and_retried() {
        let store = MemoryStore::default();
        store.fail_save.set(true);
        let (mut session, store) = session_with(saving_rom(true), store);
        run_frames(&mut session, 10);
        assert_eq!(store.save_attempts.get(), 1);
        assert_eq!(
            session.take_errors(),
            ["세이브 파일을 저장할 수 없습니다: 쓰기 금지"]
        );
        store.fail_save.set(false);
        run_frames(&mut session, SAVE_IDLE_FRAMES);
        assert_eq!(store.save_attempts.get(), 2);
        assert!(session.take_errors().is_empty());
        assert_eq!(store.data.borrow().as_ref().unwrap()[0], 0x42);
    }
}
