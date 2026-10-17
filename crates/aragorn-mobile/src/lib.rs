//! 모바일(Android/iOS) 앱이 쓰는 에뮬레이터 API. UniFFI가 Kotlin·Swift 바인딩을 만든다.
//!
//! 에뮬레이션과 오디오는 전용 스레드에서 돈다. 앱은 명령(버튼, 일시정지, 스테이트)을 보내고,
//! 화면을 그릴 때마다 마지막 프레임을 가져간다. 앱 UI는 표시와 입력만 맡는다.

uniffi::setup_scaffolding!();

#[cfg(target_os = "android")]
mod android;
mod runner;

use runner::{Command, Screen};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum GbButton {
    Right,
    Left,
    Up,
    Down,
    A,
    B,
    Select,
    Start,
}

impl From<GbButton> for gb_core::Button {
    fn from(button: GbButton) -> Self {
        match button {
            GbButton::Right => Self::Right,
            GbButton::Left => Self::Left,
            GbButton::Up => Self::Up,
            GbButton::Down => Self::Down,
            GbButton::A => Self::A,
            GbButton::B => Self::B,
            GbButton::Select => Self::Select,
            GbButton::Start => Self::Start,
        }
    }
}

/// Kotlin에서는 `Throwable.message`와 겹치지 않게 필드 이름을 `reason`으로 둔다.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum EmulatorError {
    /// ROM을 불러올 수 없다.
    #[error("{reason}")]
    Rom { reason: String },
    /// 스테이트를 저장하거나 불러올 수 없다.
    #[error("{reason}")]
    State { reason: String },
    /// 세이브 폴더를 만들 수 없다.
    #[error("{reason}")]
    Io { reason: String },
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

    pub fn set_button(&self, button: GbButton, pressed: bool) {
        self.send(Command::Button(button.into(), pressed));
    }

    /// 앱이 백그라운드로 가면 일시정지한다. 일시정지 중에는 스레드가 CPU를 쓰지 않는다.
    pub fn set_paused(&self, paused: bool) {
        self.send(Command::SetPaused(paused));
    }

    pub fn set_muted(&self, muted: bool) {
        self.send(Command::SetMuted(muted));
    }

    /// 마지막으로 그린 프레임 번호. 바뀌었을 때만 `frame`을 가져가면 된다.
    pub fn frame_number(&self) -> u64 {
        self.screen.frame_number()
    }

    /// 마지막으로 그린 프레임. `SCREEN_WIDTH`×`SCREEN_HEIGHT` 픽셀의 RGBA 바이트다
    /// (Android `Bitmap.Config.ARGB_8888`의 메모리 순서와 같다).
    pub fn frame(&self) -> Vec<u8> {
        self.screen.frame()
    }

    pub fn save_state(&self, slot: u8) -> Result<(), EmulatorError> {
        self.request(|reply| Command::SaveState(slot, reply))?
            .map_err(|reason| EmulatorError::State { reason })
    }

    pub fn load_state(&self, slot: u8) -> Result<(), EmulatorError> {
        self.request(|reply| Command::LoadState(slot, reply))?
            .map_err(|reason| EmulatorError::State { reason })
    }

    /// 저장하지 않은 게임 세이브를 지금 저장한다. 앱이 백그라운드로 갈 때 부른다.
    /// 저장하지 못한 진행이 남아 있으면 `false`.
    pub fn flush(&self) -> bool {
        self.request(Command::Flush).unwrap_or(false)
    }

    /// 쌓인 오류 문구를 꺼낸다.
    pub fn take_errors(&self) -> Vec<String> {
        self.screen.take_errors()
    }

    /// 세이브를 저장하고 스레드를 끝낸다. 여러 번 불러도 된다.
    pub fn stop(&self) {
        self.send(Command::Stop);
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

    fn send(&self, command: Command) {
        // 스레드가 끝났으면 보낼 곳이 없다. `stop` 뒤의 입력은 버린다.
        let _ = self.commands.send(command);
    }

    /// 명령을 보내고 스레드의 답을 기다린다.
    fn request<T>(
        &self,
        command: impl FnOnce(mpsc::SyncSender<T>) -> Command,
    ) -> Result<T, EmulatorError> {
        let (reply, answer) = mpsc::sync_channel(1);
        self.commands
            .send(command(reply))
            .map_err(|_| EmulatorError::Stopped)?;
        answer.recv().map_err(|_| EmulatorError::Stopped)
    }
}

impl Drop for Emulator {
    fn drop(&mut self) {
        self.stop();
    }
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
        let deadline = Instant::now() + Duration::from_secs(5);
        while emulator.frame_number() < 3 {
            assert!(Instant::now() < deadline, "프레임이 진행되지 않습니다");
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            emulator.frame().len(),
            (SCREEN_WIDTH * SCREEN_HEIGHT * 4) as usize
        );
    }

    #[test]
    fn saves_and_loads_a_state_next_to_the_save_path() {
        let dir = tempfile::tempdir().unwrap();
        let emulator = start(&dir);
        assert!(matches!(
            emulator.load_state(3),
            Err(EmulatorError::State { .. })
        ));
        emulator.save_state(3).unwrap();
        assert!(dir.path().join("saves/test.ss3").exists());
        emulator.load_state(3).unwrap();
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
    }
}
