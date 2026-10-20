//! 모바일(Android/iOS) 앱이 쓰는 에뮬레이터 API. UniFFI가 Kotlin·Swift 바인딩을 만든다.
//!
//! 에뮬레이션과 오디오는 전용 스레드에서 돈다. 앱은 명령(버튼, 속도, 스테이트, 디버거)을 보내고,
//! 화면을 그릴 때마다 마지막 프레임을 가져간다. 앱 UI는 표시와 입력만 맡는다.

uniffi::setup_scaffolding!();

#[cfg(target_os = "android")]
mod android;
mod debug;
mod runner;
mod settings;
mod types;
mod update;

pub use debug::{
    CpuView, DisasmLine, IoGroup, IoRow, MapView, PaletteGroup, PaletteRow, SpriteView, Swatch,
};
pub use settings::{PadLayout, PadOffsets, RecentRom, Settings};
pub use types::{
    FastSpeed, GbButton, Image, InputAction, InputDevice, Palette, Speed, VideoSettings,
};
pub use update::UpdateCheck;

use aragorn_app::session::{self, STATE_SLOTS, THUMBNAIL_HEIGHT, THUMBNAIL_WIDTH};
use aragorn_host::SystemClock;
use runner::{Command, Screen, State};
use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc, Mutex, PoisonError,
        mpsc::{self, Sender},
    },
    thread::{self, JoinHandle},
};

pub const SCREEN_WIDTH: u32 = 160;
pub const SCREEN_HEIGHT: u32 = 144;

/// 강제 업데이트 중에 스테이트를 다루려 할 때의 문구 (데스크톱과 같다).
const BLOCKED_NOTICE: &str = "업데이트가 필요해서 스테이트를 저장하거나 불러올 수 없습니다";

/// Kotlin에서는 `Throwable.message`와 겹치지 않게 필드 이름을 `reason`으로 둔다.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum EmulatorError {
    /// ROM을 불러올 수 없다.
    #[error("{reason}")]
    Rom { reason: String },
    /// 스테이트를 저장하거나 불러올 수 없다.
    #[error("{reason}")]
    State { reason: String },
    /// 파일을 읽거나 쓸 수 없다.
    #[error("{reason}")]
    Io { reason: String },
    /// 업데이트 정책을 검증할 수 없다.
    #[error("{reason}")]
    Update { reason: String },
    /// 에뮬레이터 스레드가 이미 끝났다.
    #[error("에뮬레이터가 이미 꺼졌습니다")]
    Stopped,
}

/// 화면 크기(픽셀). 프레임은 이 크기의 RGBA 바이트다.
#[uniffi::export]
pub fn screen_size() -> ScreenSize {
    ScreenSize {
        width: SCREEN_WIDTH,
        height: SCREEN_HEIGHT,
    }
}

#[derive(Debug, Clone, Copy, uniffi::Record)]
pub struct ScreenSize {
    pub width: u32,
    pub height: u32,
}

/// 스테이트 슬롯 하나 (저장된 것만).
#[derive(Debug, Clone, uniffi::Record)]
pub struct SlotSummary {
    /// 0부터. 화면에는 1부터 보인다.
    pub slot: u8,
    /// 저장 시각(유닉스 초)
    pub saved_at: u64,
    /// `방금`, `5분 전`
    pub ago: String,
    pub thumbnail: Option<Image>,
}

/// 링크 케이블 상태.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum LinkState {
    /// 상대를 기다린다. 상대는 `address`로 연결한다.
    Waiting {
        address: String,
    },
    Connecting {
        address: String,
    },
    Connected {
        peer: String,
    },
}

/// 이 기기의 LAN 주소. 모르면 `None`.
#[uniffi::export]
pub fn local_address() -> Option<String> {
    aragorn_host::local_ip()
}

/// 치트 목록의 한 줄.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CheatInfo {
    pub code: String,
    /// 이름이 없으면 코드
    pub label: String,
    pub enabled: bool,
}

/// 실행 중인 게임 하나.
#[derive(uniffi::Object)]
pub struct Emulator {
    title: String,
    commands: Sender<Command>,
    screen: Arc<Screen>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

#[uniffi::export]
impl Emulator {
    /// ROM을 불러와 바로 실행한다.
    ///
    /// `save_path`는 ROM이 그 자리에 있다고 치는 경로다. 세이브는 데스크톱과 같은 규칙으로
    /// 그 옆에 저장한다(`<이름>.sav`, `.ss0`–`.ss9`). 폴더가 없으면 만든다.
    #[uniffi::constructor]
    pub fn new(rom: Vec<u8>, save_path: String) -> Result<Arc<Self>, EmulatorError> {
        init_logging();
        Self::start(rom, PathBuf::from(save_path), true).map(Arc::new)
    }

    pub fn title(&self) -> String {
        self.title.clone()
    }

    /// 스테이트 슬롯 수.
    pub fn slot_count(&self) -> u8 {
        STATE_SLOTS
    }

    // ---- 화면과 안내 ----

    /// 마지막으로 그린 프레임 번호. 바뀌었을 때만 `frame`을 가져가면 된다.
    pub fn frame_number(&self) -> u64 {
        self.screen.frame_number()
    }

    /// 마지막으로 그린 화면. 보통 160×144이고, 슈퍼 게임보이 테두리를 그리면 256×224다.
    pub fn frame(&self) -> Image {
        self.screen.frame()
    }

    /// 쌓인 안내·오류 문구를 꺼낸다 (세이브 오류, 브레이크포인트 정지).
    pub fn take_notices(&self) -> Vec<String> {
        self.screen.take_notices()
    }

    // ---- 입력과 실행 ----

    pub fn set_button(&self, button: GbButton, pressed: bool) {
        let button = button.into();
        self.post(move |state| state.session.set_button(button, pressed));
    }

    /// 앱이 백그라운드로 가면 멈추고 게임 세이브를 저장한다. 멈춘 동안 스레드는 CPU를 쓰지 않는다.
    pub fn set_background(&self, background: bool) {
        self.post(move |state| {
            state.background = background;
            if background {
                state.session.flush();
                // 앱이 백그라운드에서 꺼져도 이어서 할 수 있게 지금 자리를 남긴다.
                state.save_resume_if_enabled();
            }
        });
    }

    /// 백그라운드로 가거나 게임을 닫을 때 이어하기를 남길지.
    pub fn set_auto_resume(&self, on: bool) {
        self.post(move |state| state.auto_resume = on);
    }

    /// 지금 자리를 이어하기로 남긴다 (다른 게임을 열기 전에).
    pub fn save_resume(&self) -> Result<(), EmulatorError> {
        self.call(|state| state.session.save_resume())?
            .map_err(|reason| EmulatorError::State { reason })
    }

    /// 이어하기가 있으면 그 자리로 간다. 갔으면 `true`. 이어하기 뒤로 세이브 파일이 바뀌었으면
    /// 세이브 파일을 지키려고 가지 않고 이유를 오류로 돌려준다.
    pub fn resume(&self) -> Result<bool, EmulatorError> {
        self.call(|state| {
            let resumed = state.session.resume()?;
            if resumed {
                state.publish();
            }
            Ok(resumed)
        })?
        .map_err(|reason| EmulatorError::State { reason })
    }

    /// 지금 화면(필터, 테두리 적용)을 3배로 키운 PNG.
    pub fn screenshot(&self) -> Result<Vec<u8>, EmulatorError> {
        self.call(|state| {
            let frame = state.display();
            aragorn_app::screenshot::png(
                &frame.pixels,
                frame.width,
                frame.height,
                aragorn_app::screenshot::SCALE,
            )
        })
    }

    /// 강제 업데이트가 필요하면 게임을 멈추고 스테이트를 막는다.
    pub fn set_blocked(&self, blocked: bool) {
        self.post(move |state| {
            state.blocked = blocked;
            if blocked {
                state.session.flush();
            }
        });
    }

    /// 사용자 일시정지를 켜거나 끄고, 바뀐 상태를 돌려준다.
    pub fn toggle_pause(&self) -> bool {
        self.call(|state| {
            state.speed.toggle_pause();
            state.speed.is_paused()
        })
        .unwrap_or(true)
    }

    /// 사용자가 일시정지했는지 (백그라운드 정지는 세지 않는다). 기다리지 않고 바로 읽는다.
    pub fn is_paused(&self) -> bool {
        self.screen.status().0
    }

    /// 배속 버튼(키)을 누르고 있는지.
    pub fn set_fast_held(&self, held: bool) {
        self.post(move |state| state.fast_held = held);
    }

    pub fn set_speed(&self, base: Speed, fast: FastSpeed) {
        self.post(move |state| {
            state.base_speed = base.into();
            state.fast_forward = fast.into();
        });
    }

    /// 지금 실행 배율 문구: `일시정지`, `1배`, `4배`, `무제한`. 기다리지 않고 바로 읽는다.
    pub fn speed_text(&self) -> String {
        self.screen.status().1
    }

    pub fn set_muted(&self, muted: bool) {
        self.post(move |state| state.session.set_sound_off(muted));
    }

    /// 되감기 버튼(키)을 누르고 있는지.
    pub fn set_rewind_held(&self, held: bool) {
        self.post(move |state| state.rewind_held = held);
    }

    /// 되감기를 쓸지. 끄면 쌓아 둔 스테이트를 버린다.
    pub fn set_rewind_enabled(&self, enabled: bool) {
        self.post(move |state| state.session.set_rewind_enabled(enabled));
    }

    /// 화면 필터를 바꾸고 지금 화면에 바로 적용한다.
    pub fn set_video(&self, video: VideoSettings) {
        self.post(move |state| {
            state.video = video.into();
            state.publish();
        });
    }

    // ---- 링크 케이블 ----

    /// 이 기기에서 상대를 기다린다. 상대가 입력할 주소를 돌려준다.
    pub fn link_host(&self) -> Result<String, EmulatorError> {
        let link = aragorn_host::TcpLink::host(aragorn_host::DEFAULT_PORT).map_err(|e| {
            EmulatorError::Io {
                reason: format!("링크를 열 수 없습니다: {e}"),
            }
        })?;
        let address = match aragorn_app::link::LinkTransport::status(&link) {
            aragorn_app::link::LinkStatus::Waiting { address } => address,
            _ => String::new(),
        };
        self.post(move |state| state.session.set_link(Box::new(link)));
        Ok(address)
    }

    /// 상대 주소(`192.168.0.5`, 포트를 빼면 기본 포트)로 연결한다. 결과는 `link_state`로 본다.
    pub fn link_connect(&self, address: String) {
        self.post(move |state| {
            state
                .session
                .set_link(Box::new(aragorn_host::TcpLink::connect(&address)))
        });
    }

    pub fn link_disconnect(&self) {
        self.post(|state| state.session.disconnect_link());
    }

    /// 링크 상태. 쓰지 않거나 끊겼으면 `None` (끊긴 이유는 안내 문구로 온다).
    pub fn link_state(&self) -> Option<LinkState> {
        use aragorn_app::link::LinkStatus;
        self.call(|state| match state.session.link_status()? {
            LinkStatus::Waiting { address } => Some(LinkState::Waiting { address }),
            LinkStatus::Connecting { address } => Some(LinkState::Connecting { address }),
            LinkStatus::Connected { peer } => Some(LinkState::Connected { peer }),
            LinkStatus::Closed { .. } => None,
        })
        .ok()
        .flatten()
    }

    /// 상태 줄에 보일 링크 문구. 기다리지 않고 바로 읽는다.
    pub fn link_text(&self) -> Option<String> {
        self.screen.link_text()
    }

    // ---- 치트 ----

    pub fn cheats(&self) -> Vec<CheatInfo> {
        self.call(|state| {
            state
                .session
                .cheats()
                .iter()
                .map(|entry| CheatInfo {
                    code: entry.code.clone(),
                    label: entry.label().to_string(),
                    enabled: entry.enabled,
                })
                .collect()
        })
        .unwrap_or_default()
    }

    /// 치트를 켠 채로 더한다. 코드가 틀리면 오류다. 목록은 세이브 옆 `.cht`에 저장한다.
    pub fn add_cheat(&self, code: String, name: String) -> Result<(), EmulatorError> {
        self.call(move |state| state.session.add_cheat(&code, &name))?
            .map_err(|reason| EmulatorError::State { reason })
    }

    pub fn set_cheat_enabled(&self, index: u32, enabled: bool) -> Result<(), EmulatorError> {
        self.call(move |state| state.session.set_cheat_enabled(index as usize, enabled))?
            .map_err(|reason| EmulatorError::State { reason })
    }

    pub fn remove_cheat(&self, index: u32) -> Result<(), EmulatorError> {
        self.call(move |state| state.session.remove_cheat(index as usize))?
            .map_err(|reason| EmulatorError::State { reason })
    }

    // ---- 세이브 ----

    pub fn save_state(&self, slot: u8) -> Result<(), EmulatorError> {
        self.call(move |state| {
            check_slot(state, slot)?;
            state.session.save_state(slot)
        })?
        .map_err(|reason| EmulatorError::State { reason })
    }

    pub fn load_state(&self, slot: u8) -> Result<(), EmulatorError> {
        self.call(move |state| {
            check_slot(state, slot)?;
            state.session.load_state(slot)?;
            state.publish();
            Ok(())
        })?
        .map_err(|reason| EmulatorError::State { reason })
    }

    /// 저장된 슬롯 목록. 비었거나 읽을 수 없는 슬롯은 빠진다.
    pub fn slots(&self) -> Vec<SlotSummary> {
        self.call(|state| {
            let now = aragorn_app::session::Clock::now_unix(&SystemClock);
            (0..STATE_SLOTS)
                .filter_map(|slot| {
                    let info = state.session.slot_info(slot)?;
                    Some(SlotSummary {
                        slot,
                        saved_at: info.saved_at,
                        ago: session::ago_text(now, info.saved_at),
                        thumbnail: info.thumbnail.map(|pixels| {
                            Image::from_pixels(THUMBNAIL_WIDTH, THUMBNAIL_HEIGHT, &pixels)
                        }),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
    }

    /// 저장하지 않은 게임 세이브를 지금 저장한다. 저장하지 못한 진행이 남아 있으면 `false`.
    pub fn flush(&self) -> bool {
        self.call(|state| state.session.flush()).unwrap_or(false)
    }

    // ---- 디버거 ----

    pub fn is_cgb(&self) -> bool {
        self.call(|state| state.session.debug().cgb())
            .unwrap_or(false)
    }

    pub fn cpu(&self) -> Result<CpuView, EmulatorError> {
        self.call(|state| debug::cpu_view(&state.session.debug()))
    }

    /// 메모리 뷰어의 `first`번째 줄부터 `count`줄.
    pub fn memory_rows(&self, first: u32, count: u32) -> Result<Vec<String>, EmulatorError> {
        self.call(move |state| debug::memory_rows(&state.session.debug(), first, count))
    }

    pub fn tiles(&self, bank: u8) -> Result<Image, EmulatorError> {
        self.call(move |state| debug::tiles(&state.session.debug(), bank))
    }

    pub fn tile_map(&self, window: bool) -> Result<MapView, EmulatorError> {
        self.call(move |state| debug::tile_map(&state.session.debug(), window))
    }

    pub fn sprites(&self) -> Result<Vec<SpriteView>, EmulatorError> {
        self.call(|state| debug::sprites(&state.session.debug()))
    }

    pub fn palettes(&self) -> Result<Vec<PaletteGroup>, EmulatorError> {
        self.call(|state| debug::palettes(&state.session.debug()))
    }

    pub fn io_registers(&self) -> Result<Vec<IoGroup>, EmulatorError> {
        self.call(|state| debug::io_registers(&state.session.debug()))
    }

    pub fn channels_muted(&self) -> Vec<bool> {
        self.call(|state| debug::channels_muted(&state.session.debug()))
            .unwrap_or_default()
    }

    /// 채널(0–3 = 채널 1–4)을 음소거한다. 게임은 그대로 돈다.
    pub fn set_channel_muted(&self, channel: u8, muted: bool) {
        self.post(move |state| {
            state
                .session
                .set_channel_muted(usize::from(channel.min(3)), muted)
        });
    }

    pub fn set_breakpoints(&self, pcs: Vec<u16>) {
        self.post(move |state| state.session.set_breakpoints(&pcs));
    }

    /// 일시정지하고 명령 하나를 실행한다. 강제 업데이트 중에는 하지 않는다.
    pub fn step_instruction(&self) {
        self.post(|state| step(state, |session| session.step_instruction()));
    }

    /// 일시정지하고 프레임 하나를 실행한다. 브레이크포인트에 걸리면 그 자리에서 멈춘다.
    pub fn step_frame(&self) {
        self.post(|state| step(state, |session| session.step_frame()));
    }

    /// 세이브를 저장하고 스레드를 끝낸다. 여러 번 불러도 된다.
    pub fn stop(&self) {
        let _ = self.commands.send(Command::Stop);
        let handle = self
            .thread
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(handle) = handle {
            let _ = handle.join();
        }
    }
}

impl Emulator {
    /// `audio`가 거짓이면 오디오 장치를 열지 않는다(테스트용).
    fn start(rom: Vec<u8>, save_path: PathBuf, audio: bool) -> Result<Self, EmulatorError> {
        if let Some(dir) = save_path.parent() {
            fs::create_dir_all(dir).map_err(|e| EmulatorError::Io {
                reason: format!("세이브 폴더를 만들 수 없습니다: {e}"),
            })?;
        }
        let (commands, receiver) = mpsc::channel();
        let (ready, started) = mpsc::sync_channel(1);
        let screen = Arc::new(Screen::default());
        let thread = thread::Builder::new()
            .name("aragorn-emulator".into())
            .spawn({
                let screen = Arc::clone(&screen);
                move || runner::run(rom, save_path, audio, receiver, ready, screen)
            })
            .map_err(|e| EmulatorError::Io {
                reason: format!("에뮬레이터 스레드를 시작할 수 없습니다: {e}"),
            })?;
        let title = started
            .recv()
            .map_err(|_| EmulatorError::Stopped)?
            .map_err(|reason| EmulatorError::Rom { reason })?;
        Ok(Self {
            title,
            commands,
            screen,
            thread: Mutex::new(Some(thread)),
        })
    }

    /// 에뮬레이터 스레드에서 실행하고 기다리지 않는다. 스레드가 끝났으면 버린다.
    fn post(&self, task: impl FnOnce(&mut State) + Send + 'static) {
        let _ = self.commands.send(Command::Run(Box::new(task)));
    }

    /// 에뮬레이터 스레드에서 실행하고 결과를 기다린다.
    fn call<T: Send + 'static>(
        &self,
        task: impl FnOnce(&mut State) -> T + Send + 'static,
    ) -> Result<T, EmulatorError> {
        let (reply, answer) = mpsc::sync_channel(1);
        self.commands
            .send(Command::Run(Box::new(move |state| {
                let _ = reply.send(task(state));
            })))
            .map_err(|_| EmulatorError::Stopped)?;
        answer.recv().map_err(|_| EmulatorError::Stopped)
    }
}

impl Drop for Emulator {
    fn drop(&mut self) {
        self.stop();
    }
}

fn check_slot(state: &State, slot: u8) -> Result<(), String> {
    if state.blocked {
        Err(BLOCKED_NOTICE.to_string())
    } else if slot >= STATE_SLOTS {
        Err(format!("슬롯은 1–{STATE_SLOTS}번만 있습니다"))
    } else {
        Ok(())
    }
}

/// 디버거 단계 실행. 일시정지한 상태에서 하고, 그린 화면을 바로 보인다.
fn step(state: &mut State, run: impl FnOnce(&mut aragorn_app::session::Session)) {
    if state.blocked {
        return;
    }
    state.speed.pause();
    run(&mut state.session);
    state.publish();
}

/// Android에서는 stderr가 버려지므로 로그와 패닉 메시지를 logcat(`aragorn` 태그)으로 보낸다.
fn init_logging() {
    #[cfg(target_os = "android")]
    {
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| {
            android_logger::init_once(
                android_logger::Config::default()
                    .with_max_level(log::LevelFilter::Info)
                    .with_tag("aragorn"),
            );
            let default_hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                log::error!("{info}");
                default_hook(info);
            }));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// 제자리 점프(`JR -2`)만 하는 ROM.
    fn looping_rom() -> Vec<u8> {
        let mut rom = vec![0; 0x8000];
        rom[0x0100..0x0102].copy_from_slice(&[0x18, 0xFE]);
        rom[0x0134..0x0140].copy_from_slice(b"ARAGORN TEST");
        rom
    }

    fn start(dir: &tempfile::TempDir) -> Emulator {
        Emulator::start(looping_rom(), dir.path().join("saves/test.gb"), false).unwrap()
    }

    fn wait_for_frames(emulator: &Emulator, at_least: u64) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while emulator.frame_number() < at_least {
            assert!(Instant::now() < deadline, "프레임이 진행되지 않습니다");
            thread::sleep(Duration::from_millis(10));
        }
    }

    /// 상태 문구는 에뮬레이터 스레드가 다음 차례에 갱신한다.
    fn wait_for_speed(emulator: &Emulator, expected: &str) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while emulator.speed_text() != expected {
            assert!(
                Instant::now() < deadline,
                "{expected}이(가) 아니라 {}",
                emulator.speed_text()
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn rejects_a_rom_smaller_than_the_header() {
        let dir = tempfile::tempdir().unwrap();
        let result = Emulator::start(vec![0; 16], dir.path().join("x.gb"), false);
        assert!(matches!(result, Err(EmulatorError::Rom { .. })));
    }

    #[test]
    fn runs_frames_in_the_background() {
        let dir = tempfile::tempdir().unwrap();
        let emulator = start(&dir);
        assert_eq!(emulator.title(), "ARAGORN TEST");
        wait_for_frames(&emulator, 3);
        let frame = emulator.frame();
        assert_eq!((frame.width, frame.height), (SCREEN_WIDTH, SCREEN_HEIGHT));
        assert_eq!(
            frame.rgba.len(),
            (SCREEN_WIDTH * SCREEN_HEIGHT * 4) as usize
        );
    }

    #[test]
    fn saves_and_lists_states_next_to_the_save_path() {
        let dir = tempfile::tempdir().unwrap();
        let emulator = start(&dir);
        assert!(emulator.slots().is_empty());
        assert!(matches!(
            emulator.load_state(3),
            Err(EmulatorError::State { .. })
        ));
        emulator.save_state(3).unwrap();
        assert!(dir.path().join("saves/test.ss3").exists());
        let slots = emulator.slots();
        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].slot, 3);
        assert_eq!(slots[0].ago, "방금");
        let thumbnail = slots[0].thumbnail.as_ref().unwrap();
        assert_eq!(thumbnail.rgba.len(), THUMBNAIL_WIDTH * THUMBNAIL_HEIGHT * 4);
        emulator.load_state(3).unwrap();
    }

    #[test]
    fn pause_stops_frames_and_step_advances_one() {
        let dir = tempfile::tempdir().unwrap();
        let emulator = start(&dir);
        assert!(emulator.toggle_pause());
        wait_for_speed(&emulator, "일시정지");
        let before = emulator.frame_number();
        thread::sleep(Duration::from_millis(100));
        assert_eq!(emulator.frame_number(), before);
        emulator.step_frame();
        wait_for_frames(&emulator, before + 1);
        assert!(emulator.is_paused());
        assert!(!emulator.toggle_pause());
    }

    #[test]
    fn speed_text_follows_base_speed_and_fast_button() {
        let dir = tempfile::tempdir().unwrap();
        let emulator = start(&dir);
        wait_for_speed(&emulator, "1배");
        emulator.set_speed(Speed::X2, FastSpeed::Unlimited);
        wait_for_speed(&emulator, "2배");
        emulator.set_fast_held(true);
        wait_for_speed(&emulator, "무제한");
        emulator.set_background(true);
        wait_for_speed(&emulator, "일시정지");
    }

    #[test]
    fn forced_update_blocks_states() {
        let dir = tempfile::tempdir().unwrap();
        let emulator = start(&dir);
        emulator.set_blocked(true);
        match emulator.save_state(0) {
            Err(EmulatorError::State { reason }) => assert_eq!(reason, BLOCKED_NOTICE),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn breakpoint_pauses_and_reports() {
        let dir = tempfile::tempdir().unwrap();
        let emulator = start(&dir);
        emulator.set_breakpoints(vec![0x0100]);
        let deadline = Instant::now() + Duration::from_secs(5);
        let notices = loop {
            let notices = emulator.take_notices();
            if !notices.is_empty() {
                break notices;
            }
            assert!(
                Instant::now() < deadline,
                "브레이크포인트에서 멈추지 않습니다"
            );
            thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(notices, ["브레이크포인트 $0100에서 멈췄습니다"]);
        assert!(emulator.is_paused());
        let cpu = emulator.cpu().unwrap();
        assert!(cpu.registers[1].contains("PC=0100"));
        assert!(cpu.rows.iter().any(|row| row.pc == 0x0100 && row.ahead));
    }

    #[test]
    fn debugger_views_have_expected_sizes() {
        let dir = tempfile::tempdir().unwrap();
        let emulator = start(&dir);
        assert!(!emulator.is_cgb());
        let tiles = emulator.tiles(1).unwrap();
        assert_eq!((tiles.width, tiles.height), (128, 192));
        assert_eq!(emulator.tile_map(false).unwrap().image.width, 256);
        assert_eq!(emulator.sprites().unwrap().len(), 40);
        assert_eq!(emulator.palettes().unwrap()[0].rows.len(), 3);
        assert!(!emulator.io_registers().unwrap().is_empty());
        let rows = emulator.memory_rows(0x0C00, 2).unwrap();
        assert!(rows[0].starts_with("C000"));
        assert_eq!(emulator.memory_rows(4095, 10).unwrap().len(), 1);
        emulator.set_channel_muted(2, true);
        assert_eq!(emulator.channels_muted(), [false, false, true, false]);
    }

    #[test]
    fn rewind_button_shows_rewind_and_goes_back() {
        let dir = tempfile::tempdir().unwrap();
        let emulator = start(&dir);
        wait_for_frames(&emulator, 20);
        emulator.set_rewind_held(true);
        wait_for_speed(&emulator, "되감기");
        emulator.set_rewind_held(false);
        wait_for_speed(&emulator, "1배");
    }

    #[test]
    fn cheats_are_listed_and_saved_next_to_the_save() {
        let dir = tempfile::tempdir().unwrap();
        let emulator = start(&dir);
        assert!(matches!(
            emulator.add_cheat("nope".into(), "".into()),
            Err(EmulatorError::State { .. })
        ));
        emulator
            .add_cheat("010138cd".into(), "체력".into())
            .unwrap();
        emulator.set_cheat_enabled(0, false).unwrap();
        assert_eq!(
            emulator.cheats(),
            [CheatInfo {
                code: "010138CD".into(),
                label: "체력".into(),
                enabled: false,
            }]
        );
        assert!(dir.path().join("saves/test.cht").exists());
        emulator.remove_cheat(0).unwrap();
        assert!(emulator.cheats().is_empty());
    }

    #[test]
    fn video_filter_changes_the_published_frame() {
        let dir = tempfile::tempdir().unwrap();
        let emulator = start(&dir);
        wait_for_frames(&emulator, 2);
        emulator.set_speed(Speed::X1, FastSpeed::X4);
        assert!(emulator.toggle_pause());
        wait_for_speed(&emulator, "일시정지");
        let before = emulator.frame();
        let number = emulator.frame_number();
        emulator.set_video(VideoSettings {
            palette: Palette::Gray,
            color_correction: false,
            lcd_grid: false,
            sgb_colors: true,
            sgb_border: true,
        });
        wait_for_frames(&emulator, number + 1);
        let after = emulator.frame();
        assert_ne!(before, after);
        assert_eq!(&after.rgba[..4], &[0xFF, 0xFF, 0xFF, 0xFF], "흰색 배경");
    }

    #[test]
    fn closing_leaves_a_resume_point_when_enabled() {
        let dir = tempfile::tempdir().unwrap();
        let emulator = start(&dir);
        assert!(!emulator.resume().unwrap(), "아직 없다");
        emulator.stop();
        assert!(
            !dir.path().join("saves/test.ssr").exists(),
            "꺼져 있으면 남기지 않는다"
        );

        let emulator = start(&dir);
        emulator.set_auto_resume(true);
        emulator.stop();
        assert!(dir.path().join("saves/test.ssr").exists());
        let emulator = start(&dir);
        assert!(emulator.resume().unwrap());
    }

    #[test]
    fn screenshot_is_a_scaled_png() {
        let dir = tempfile::tempdir().unwrap();
        let emulator = start(&dir);
        let png = emulator.screenshot().unwrap();
        assert_eq!(&png[1..4], b"PNG");
        // IHDR의 폭과 높이
        assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 480);
        assert_eq!(u32::from_be_bytes(png[20..24].try_into().unwrap()), 432);
    }

    #[test]
    fn two_emulators_link_over_tcp() {
        let dir = tempfile::tempdir().unwrap();
        let host = start(&dir);
        let other = tempfile::tempdir().unwrap();
        let client = start(&other);
        let address = host.link_host().unwrap();
        assert!(address.ends_with(":5151"), "{address}");
        assert!(matches!(host.link_state(), Some(LinkState::Waiting { .. })));
        client.link_connect("127.0.0.1".into());
        let deadline = Instant::now() + Duration::from_secs(5);
        while !matches!(client.link_state(), Some(LinkState::Connected { .. })) {
            assert!(Instant::now() < deadline, "{:?}", client.link_state());
            thread::sleep(Duration::from_millis(20));
        }
        assert!(host.link_text().is_some());
        client.link_disconnect();
        assert_eq!(client.link_state(), None);
        let deadline = Instant::now() + Duration::from_secs(5);
        while host.link_state().is_some() {
            assert!(Instant::now() < deadline, "상대가 끊은 것을 알아야 한다");
            thread::sleep(Duration::from_millis(20));
        }
        let notices = host.take_notices();
        assert!(
            notices.iter().any(|n| n.starts_with("링크가 끊겼습니다")),
            "{notices:?}"
        );
    }

    #[test]
    fn commands_after_stop_report_stopped() {
        let dir = tempfile::tempdir().unwrap();
        let emulator = start(&dir);
        emulator.stop();
        emulator.stop();
        assert!(matches!(
            emulator.save_state(0),
            Err(EmulatorError::Stopped)
        ));
        emulator.set_button(GbButton::A, true);
        assert!(!emulator.flush());
        assert!(emulator.slots().is_empty());
    }
}
