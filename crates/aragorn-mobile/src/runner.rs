//! 에뮬레이터 스레드: 세션을 만들고, 명령을 받고, 실제 시간에 맞춰 프레임을 돌린다.
//!
//! `Session`과 오디오 스트림은 스레드 사이로 옮길 수 없어서 이 스레드 안에서 만든다. 앱의 요청은
//! 이 스레드에서 `State`를 받아 실행하는 함수로 온다.

use crate::types::Image;
use aragorn_app::{
    pacing::{BaseSpeed, FastForward, RunMode, SpeedControl, UNLIMITED_BUDGET},
    session::Session,
    video::{Frame, VideoConfig},
};
use aragorn_host::{CpalAudio, FsSaveStore, SystemClock};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicU64, Ordering},
        mpsc::{Receiver, RecvTimeoutError, SyncSender},
    },
    time::{Duration, Instant},
};

/// 보통 속도에서 명령을 기다리는 최대 시간. 이만큼마다 깨어나 밀린 프레임을 돌린다.
/// 프레임(약 16.7ms)보다 충분히 짧아야 화면이 고르게 나아간다.
const TICK: Duration = Duration::from_millis(4);

pub enum Command {
    /// 에뮬레이터 스레드에서 실행할 일.
    Run(Box<dyn FnOnce(&mut State) + Send>),
    Stop,
}

/// 에뮬레이터 스레드만 만지는 상태.
pub struct State {
    pub session: Session,
    /// 사용자 일시정지 (버튼, 키, 디버거 단계 실행, 브레이크포인트).
    pub speed: SpeedControl,
    /// 앱이 백그라운드에 있다.
    pub background: bool,
    /// 강제 업데이트 전까지 게임을 진행하지 않는다.
    pub blocked: bool,
    pub fast_held: bool,
    pub rewind_held: bool,
    pub base_speed: BaseSpeed,
    pub fast_forward: FastForward,
    pub screen: Arc<Screen>,
    /// 화면 필터 (팔레트, 색 보정).
    pub video: VideoConfig,
    /// 백그라운드로 가거나 끝날 때 이어하기를 남긴다.
    pub auto_resume: bool,
}

impl State {
    /// 지금 실행 모드. 백그라운드나 강제 업데이트 중에는 사용자 설정과 상관없이 멈춘다.
    pub fn mode(&self) -> RunMode {
        if self.background || self.blocked {
            RunMode::Paused
        } else {
            self.speed.mode(
                self.fast_held,
                self.rewind_held,
                self.fast_forward,
                self.base_speed,
            )
        }
    }

    /// 지금 화면에 필터를 적용해 앱에 보인다.
    pub fn publish(&self) {
        self.screen.publish(&self.display());
    }

    pub fn notice(&self, message: String) {
        self.screen.notice(message);
    }

    /// 켜져 있으면 이어하기를 남긴다. 실패하면 안내 문구로 알린다.
    pub fn save_resume_if_enabled(&mut self) {
        if self.auto_resume
            && let Err(message) = self.session.save_resume()
        {
            self.notice(message);
        }
    }

    /// 필터와 슈퍼 게임보이 색·테두리를 적용한 지금 화면.
    pub fn display(&self) -> Frame {
        self.session.display(&self.video)
    }

    /// 앱이 읽을 실행 상태를 갱신한다.
    fn update_status(&self) {
        let speed = match self.mode() {
            RunMode::Paused => "일시정지",
            RunMode::Normal => "1배",
            RunMode::Fast(fast) => fast.label(),
            RunMode::Rewind => "되감기",
        };
        self.screen.set_status(self.speed.is_paused(), speed);
        self.screen
            .set_link_text(self.session.link_status().map(|s| s.text()));
    }
}

/// 에뮬레이터 스레드가 쓰고 앱 UI 스레드가 읽는 화면과 안내 문구.
#[derive(Default)]
pub struct Screen {
    number: AtomicU64,
    image: Mutex<Option<Image>>,
    notices: Mutex<Vec<String>>,
    /// (사용자 일시정지, 실행 배율 문구). 앱이 매 프레임 읽어도 에뮬레이터 스레드를 기다리지 않게 둔다.
    status: Mutex<(bool, String)>,
    /// 링크 상태 문구. 링크를 쓰지 않으면 `None`.
    link: Mutex<Option<String>>,
}

impl Screen {
    pub fn frame_number(&self) -> u64 {
        self.number.load(Ordering::Acquire)
    }

    /// 마지막 화면. 아직 그리지 않았으면 검은 160×144.
    pub fn frame(&self) -> Image {
        self.image
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .unwrap_or_else(|| {
                let (w, h) = (crate::SCREEN_WIDTH as usize, crate::SCREEN_HEIGHT as usize);
                Image::from_pixels(w, h, &vec![0x0000_00FF; w * h])
            })
    }

    pub fn take_notices(&self) -> Vec<String> {
        std::mem::take(&mut *self.notices.lock().unwrap_or_else(PoisonError::into_inner))
    }

    pub fn status(&self) -> (bool, String) {
        self.status
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn link_text(&self) -> Option<String> {
        self.link
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn set_link_text(&self, text: Option<String>) {
        let mut link = self.link.lock().unwrap_or_else(PoisonError::into_inner);
        if *link != text {
            *link = text;
        }
    }

    fn set_status(&self, paused: bool, speed: &str) {
        let mut status = self.status.lock().unwrap_or_else(PoisonError::into_inner);
        if status.0 != paused || status.1 != speed {
            *status = (paused, speed.to_string());
        }
    }

    fn publish(&self, frame: &Frame) {
        let image = Image::from_pixels(frame.width, frame.height, &frame.pixels);
        *self.image.lock().unwrap_or_else(PoisonError::into_inner) = Some(image);
        self.number.fetch_add(1, Ordering::Release);
    }

    fn notice(&self, message: String) {
        log::info!("{message}");
        self.notices
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(message);
    }

    fn report(&self, session: &mut Session) {
        for message in session.take_errors() {
            self.notice(message);
        }
    }
}

/// 스레드 본체. 세션을 만든 결과(게임 제목 또는 오류 문구)를 `ready`로 한 번 알린다.
/// `Stop`을 받거나 `Emulator`가 사라지면 세이브를 저장하고 끝난다.
pub fn run(
    rom: Vec<u8>,
    save_path: PathBuf,
    audio: bool,
    commands: Receiver<Command>,
    ready: SyncSender<Result<String, String>>,
    screen: Arc<Screen>,
) {
    let store = FsSaveStore::for_rom(&save_path);
    let session = match Session::load(rom, Box::new(store), Box::new(SystemClock)) {
        Ok(session) => session,
        Err(e) => {
            let _ = ready.send(Err(e.to_string()));
            return;
        }
    };
    let _ = ready.send(Ok(session.title().to_string()));
    let mut state = State {
        session,
        speed: SpeedControl::default(),
        background: false,
        blocked: false,
        fast_held: false,
        rewind_held: false,
        base_speed: BaseSpeed::default(),
        fast_forward: FastForward::default(),
        screen,
        video: VideoConfig::default(),
        auto_resume: false,
    };
    state.publish();
    state.update_status();
    state.screen.report(&mut state.session);

    let mut audio = audio.then(CpalAudio::open);
    let mut last_tick = Instant::now();
    'run: loop {
        // 멈춰 있으면 명령이 올 때까지 잠들고, 무제한 배속이면 기다리지 않는다.
        // 되감기는 보통 속도처럼 프레임마다 깨어난다.
        // 링크를 쓰는 동안에는 멈춰 있어도 깨어나 상대와 메시지를 주고받는다.
        let linked = state.session.link_status().is_some();
        let received = match state.mode() {
            RunMode::Paused if !linked => {
                commands.recv().map_err(|_| RecvTimeoutError::Disconnected)
            }
            RunMode::Fast(FastForward::Unlimited) => commands.try_recv().map_err(|e| match e {
                std::sync::mpsc::TryRecvError::Empty => RecvTimeoutError::Timeout,
                std::sync::mpsc::TryRecvError::Disconnected => RecvTimeoutError::Disconnected,
            }),
            _ => commands.recv_timeout(TICK),
        };
        match received {
            Ok(command) => {
                // 밀린 명령까지 다 처리한 뒤에 프레임을 돌린다.
                for command in std::iter::once(command).chain(commands.try_iter()) {
                    match command {
                        Command::Run(task) => task(&mut state),
                        Command::Stop => break 'run,
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break 'run,
        }

        let now = Instant::now();
        let elapsed = now - last_tick;
        last_tick = now;
        let started = Instant::now();
        let mut within_budget = || started.elapsed() < UNLIMITED_BUDGET;
        if state.session.run(elapsed, state.mode(), &mut within_budget) > 0 {
            state.publish();
        }
        if let Some(pc) = state.session.take_breakpoint_stop() {
            state.speed.pause();
            state.notice(format!("브레이크포인트 ${pc:04X}에서 멈췄습니다"));
        }
        if let Some(sink) = audio.as_mut().and_then(CpalAudio::sink) {
            state.session.pump_audio(sink);
        }
        state.update_status();
        state.screen.report(&mut state.session);
    }
    state.session.flush();
    state.save_resume_if_enabled();
    state.screen.report(&mut state.session);
}
