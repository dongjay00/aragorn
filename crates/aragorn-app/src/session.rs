//! 에뮬레이션 세션: 불러온 ROM 하나, 그 실행 속도, 배터리 세이브 (스펙 §5.1).

use crate::audio::{self, AudioSink};
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

/// 현재 시각 (포트). MBC3 RTC 세이브의 저장 시각과 앱이 꺼져 있던 시간 계산에 쓴다.
pub trait Clock {
    /// 유닉스 시각(초).
    fn now_unix(&self) -> u64;
}

pub struct Session {
    gb: GameBoy,
    pacer: FramePacer,
    title: String,
    store: Box<dyn SaveStore>,
    clock: Box<dyn Clock>,
    /// 아직 저장하지 않은 외부 RAM 쓰기가 있는지.
    unsaved: bool,
    /// 마지막 외부 RAM 쓰기 뒤 지난 프레임 수.
    idle_frames: u32,
    /// 직전 저장이 실패했는지. 실패하면 `SAVE_IDLE_FRAMES`마다 다시 시도한다.
    save_failed: bool,
    /// 기존 세이브를 읽지 못했으면 덮어쓰지 않는다. 새 게임으로 덮으면 원래 진행이 사라진다.
    store_blocked: bool,
    /// 상태 표시줄에 보일 오류 문구.
    errors: Vec<String>,
    /// `pump_audio`가 재사용하는 샘플 버퍼.
    audio: Vec<f32>,
}

impl Session {
    /// ROM을 불러오고, 배터리 카트리지면 `store`의 세이브를 `clock`의 현재 시각과 함께 싣는다(RTC가 꺼져
    /// 있던 시간을 반영한다). 세이브를 읽지 못해도 게임은 새로 시작하고 오류는 `take_errors`로 알린다.
    pub fn load(
        rom: Vec<u8>,
        store: Box<dyn SaveStore>,
        clock: Box<dyn Clock>,
    ) -> Result<Session, CartError> {
        // 헤더의 CGB 플래그로 기기를 고른다: 노랑·금·은·크리스탈은 CGB, 레드·블루는 DMG.
        let mut gb = GameBoy::new(rom, Model::Auto)?;
        let title = gb.header().title.trim().to_string();
        let mut errors = Vec::new();
        let mut store_blocked = false;
        if gb.battery_ram(0).is_some() {
            // RTC 블록이 없는 세이브(다른 에뮬레이터, M5 이전)도 RAM만 다 있으면 정상이다.
            let ram_len = gb.cartridge_ram_len();
            match store.load_battery() {
                Ok(Some(data)) => {
                    if data.len() < ram_len {
                        errors.push(format!(
                            "세이브 파일이 카트리지 RAM보다 작습니다 ({}/{ram_len}바이트). 앞부분만 불러왔습니다",
                            data.len()
                        ));
                    }
                    gb.load_battery_ram(&data, clock.now_unix());
                }
                Ok(None) => {}
                Err(e) => {
                    errors.push(format!("세이브 파일을 읽을 수 없습니다: {e}"));
                    store_blocked = true;
                }
            }
        }
        Ok(Session {
            gb,
            pacer: FramePacer::default(),
            title,
            store,
            clock,
            unsaved: false,
            idle_frames: 0,
            save_failed: false,
            store_blocked,
            errors,
            audio: Vec::new(),
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

    /// 지금까지 만든 소리를 `sink`로 보내고, 장치 버퍼에 쌓인 양에 맞춰 다음 프레임부터 쓸
    /// 샘플레이트를 보정한다. `advance` 뒤에 부른다.
    pub fn pump_audio(&mut self, sink: &mut dyn AudioSink) {
        self.gb.drain_audio(&mut self.audio);
        sink.push(&self.audio);
        self.audio.clear();
        let rate = sink.sample_rate();
        let adjust = audio::rate_adjust(sink.fill_ratio());
        self.gb.set_sample_rate(f64::from(rate) * adjust);
    }

    /// 지금 저장한다면 세이브 파일에 쓸 내용.
    pub fn battery_ram(&self) -> Option<Vec<u8>> {
        self.gb.battery_ram(self.clock.now_unix())
    }

    /// 저장하지 않은 세이브가 있으면 지금 저장한다. 앱 종료, ROM 교체, 업데이트 적용 전에 부른다.
    /// 저장하지 못한 진행이 남아 있으면 `false`.
    pub fn flush(&mut self) -> bool {
        if self.unsaved {
            self.save_battery();
        }
        !self.unsaved
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
        let Some(ram) = self.battery_ram() else {
            self.unsaved = false;
            return;
        };
        if self.store_blocked {
            if !self.save_failed {
                self.errors
                    .push("기존 세이브를 보호하려고 이번 실행에서는 저장하지 않습니다".to_string());
            }
            self.save_failed = true;
            self.idle_frames = 0;
            return;
        }
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

    /// 테스트가 바꿀 수 있는 시계.
    #[derive(Clone, Default)]
    struct TestClock(Rc<Cell<u64>>);

    impl Clock for TestClock {
        fn now_unix(&self) -> u64 {
            self.0.get()
        }
    }

    fn session_with(rom: Vec<u8>, store: MemoryStore) -> (Session, Rc<MemoryStore>) {
        let (session, store, _) = session_at(rom, store, 0);
        (session, store)
    }

    /// 시계가 `now`인 세션. 시계를 돌려주어 테스트가 시간을 옮길 수 있다.
    fn session_at(
        rom: Vec<u8>,
        store: MemoryStore,
        now: u64,
    ) -> (Session, Rc<MemoryStore>, TestClock) {
        let store = Rc::new(store);
        let clock = TestClock::default();
        clock.0.set(now);
        let session = Session::load(
            rom,
            Box::new(Shared(Rc::clone(&store))),
            Box::new(clock.clone()),
        )
        .unwrap();
        (session, store, clock)
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

    /// 받은 샘플을 모으는 오디오 장치. `fill`은 장치 버퍼가 찼다고 보고할 비율이다.
    struct FakeSink {
        rate: u32,
        fill: f32,
        received: Vec<f32>,
    }

    impl AudioSink for FakeSink {
        fn sample_rate(&self) -> u32 {
            self.rate
        }
        fn fill_ratio(&self) -> f32 {
            self.fill
        }
        fn push(&mut self, samples: &[f32]) {
            self.received.extend_from_slice(samples);
        }
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
    fn model_follows_cgb_flag() {
        // 노랑·금·은·크리스탈처럼 CGB 플래그가 있는 ROM은 CGB로, 레드·블루는 DMG로 돈다.
        for (flag, model) in [(0x00, Model::Dmg), (0x80, Model::Cgb), (0xC0, Model::Cgb)] {
            let mut rom = looping_rom();
            rom[0x0143] = flag;
            let (session, _) = session_with(rom, MemoryStore::default());
            assert_eq!(session.model(), model, "{flag:#04X}");
        }
    }

    #[test]
    fn rejects_invalid_rom() {
        let store = Box::new(Shared(Rc::new(MemoryStore::default())));
        assert_eq!(
            Session::load(vec![0; 16], store, Box::new(TestClock::default())).err(),
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

    #[test]
    fn unreadable_save_is_never_overwritten() {
        // 읽지 못한 세이브를 새 게임으로 덮어쓰면 원래 진행이 .sav와 .sav.bak 모두에서 사라진다.
        let store = MemoryStore {
            fail_load: true,
            ..MemoryStore::default()
        };
        let (mut session, store) = session_with(saving_rom(true), store);
        session.take_errors();
        run_frames(&mut session, SAVE_IDLE_FRAMES * 3);
        assert!(!session.flush());
        assert_eq!(store.save_attempts.get(), 0);
        assert_eq!(
            session.take_errors(),
            ["기존 세이브를 보호하려고 이번 실행에서는 저장하지 않습니다"]
        );
    }

    #[test]
    fn short_save_file_is_reported() {
        let store = MemoryStore {
            data: RefCell::new(Some(vec![0x11; 100])),
            ..MemoryStore::default()
        };
        let (mut session, _) = session_with(saving_rom(true), store);
        assert_eq!(
            session.take_errors(),
            ["세이브 파일이 카트리지 RAM보다 작습니다 (100/8192바이트). 앞부분만 불러왔습니다"]
        );
    }

    #[test]
    fn flush_reports_whether_progress_is_safe() {
        let (mut session, _) = session_with(saving_rom(false), MemoryStore::default());
        run_frames(&mut session, 1);
        assert!(session.flush());
        let store = MemoryStore::default();
        store.fail_save.set(true);
        let (mut session, _) = session_with(saving_rom(false), store);
        run_frames(&mut session, 1);
        assert!(!session.flush());
    }

    /// 1초(약 59.7프레임) 동안 만든 스테레오 프레임 수.
    fn frames_per_second(session: &mut Session, sink: &mut FakeSink) -> f64 {
        sink.received.clear();
        let frames = 597;
        for _ in 0..frames {
            session.advance(FRAME_DURATION);
            session.pump_audio(sink);
        }
        sink.received.len() as f64 / 2.0 / (f64::from(frames) * FRAME_DURATION.as_secs_f64())
    }

    #[test]
    fn audio_follows_device_sample_rate() {
        let (mut session, _) = session_with(looping_rom(), MemoryStore::default());
        let mut sink = FakeSink {
            rate: 44_100,
            fill: 1.0,
            received: Vec::new(),
        };
        session.pump_audio(&mut sink);
        let rate = frames_per_second(&mut session, &mut sink);
        assert!((rate - 44_100.0).abs() < 100.0, "{rate}");
        assert!(sink.received.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn empty_device_buffer_raises_sample_rate() {
        let (mut session, _) = session_with(looping_rom(), MemoryStore::default());
        let mut sink = FakeSink {
            rate: 48_000,
            fill: 0.0,
            received: Vec::new(),
        };
        session.pump_audio(&mut sink);
        let fast = frames_per_second(&mut session, &mut sink);
        sink.fill = 2.0;
        session.pump_audio(&mut sink);
        let slow = frames_per_second(&mut session, &mut sink);
        assert!(fast > 48_150.0 && slow < 47_850.0, "{fast} / {slow}");
    }

    /// MBC3+TIMER+RAM+BATTERY(0x10) 판 `saving_rom(true)`.
    fn rtc_saving_rom() -> Vec<u8> {
        let mut rom = saving_rom(true);
        rom[0x0147] = 0x10;
        rom
    }

    /// 세이브 끝 48바이트 RTC 블록의 (시 레지스터, 저장 시각).
    fn rtc_hours_and_timestamp(save: &[u8]) -> (u8, u64) {
        let block = &save[save.len() - 48..];
        (
            block[8],
            u64::from_le_bytes(block[40..48].try_into().unwrap()),
        )
    }

    #[test]
    fn rtc_save_records_clock_time() {
        let (mut session, store, clock) =
            session_at(rtc_saving_rom(), MemoryStore::default(), 1_000);
        clock.0.set(5_000);
        run_frames(&mut session, 1);
        let saved = store.data.borrow().clone().unwrap();
        assert_eq!(saved.len(), 0x2000 + 48);
        assert_eq!(rtc_hours_and_timestamp(&saved).1, 5_000, "저장할 때의 시각");
    }

    #[test]
    fn rtc_advances_by_time_the_app_was_closed() {
        let (mut session, store, _) = session_at(rtc_saving_rom(), MemoryStore::default(), 1_000);
        run_frames(&mut session, 1);
        let saved = store.data.borrow().clone().unwrap();
        let store = MemoryStore {
            data: RefCell::new(Some(saved)),
            ..MemoryStore::default()
        };
        let (reloaded, _, _) = session_at(rtc_saving_rom(), store, 1_000 + 3 * 3600);
        assert_eq!(
            rtc_hours_and_timestamp(&reloaded.battery_ram().unwrap()),
            (3, 1_000 + 3 * 3600)
        );
    }

    #[test]
    fn save_without_rtc_block_is_not_reported_short() {
        // M5까지 저장한 금·은·크리스탈 세이브는 RAM만 있다.
        let store = MemoryStore {
            data: RefCell::new(Some(vec![0; 0x2000])),
            ..MemoryStore::default()
        };
        let (mut session, _, _) = session_at(rtc_saving_rom(), store, 0);
        assert!(session.take_errors().is_empty());
    }
}
