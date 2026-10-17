//! 에뮬레이터 스레드: 세션을 만들고, 명령을 받고, 실제 시간에 맞춰 프레임을 돌린다.
//!
//! `Session`과 오디오 스트림은 스레드 사이로 옮길 수 없어서 이 스레드 안에서 만든다.

use aragorn_app::{
    pacing::RunMode,
    session::{STATE_SLOTS, Session},
};
use aragorn_host::{CpalAudio, FsSaveStore, SystemClock};
use gb_core::Button;
use std::{
    path::PathBuf,
    sync::{
        Mutex, PoisonError,
        atomic::{AtomicU64, Ordering},
        mpsc::{Receiver, RecvTimeoutError, SyncSender},
    },
    time::{Duration, Instant},
};

/// 실행 중에 명령을 기다리는 최대 시간. 이만큼마다 깨어나 밀린 프레임을 돌린다.
/// 프레임(약 16.7ms)보다 충분히 짧아야 화면이 고르게 나아간다.
const TICK: Duration = Duration::from_millis(4);

pub enum Command {
    Button(Button, bool),
    SetPaused(bool),
    SetMuted(bool),
    SaveState(u8, SyncSender<Result<(), String>>),
    LoadState(u8, SyncSender<Result<(), String>>),
    Flush(SyncSender<bool>),
    Stop,
}

/// 에뮬레이터 스레드가 쓰고 앱 UI 스레드가 읽는 화면과 오류 문구.
#[derive(Default)]
pub struct Screen {
    number: AtomicU64,
    rgba: Mutex<Vec<u8>>,
    errors: Mutex<Vec<String>>,
}

impl Screen {
    pub fn frame_number(&self) -> u64 {
        self.number.load(Ordering::Acquire)
    }

    pub fn frame(&self) -> Vec<u8> {
        self.rgba
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn take_errors(&self) -> Vec<String> {
        std::mem::take(&mut *self.errors.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// 0xRRGGBBAA 픽셀을 R, G, B, A 바이트 순서로 바꿔 싣는다.
    fn publish(&self, framebuffer: &[u32]) {
        {
            let mut rgba = self.rgba.lock().unwrap_or_else(PoisonError::into_inner);
            rgba.clear();
            rgba.extend(framebuffer.iter().flat_map(|p| p.to_be_bytes()));
        }
        self.number.fetch_add(1, Ordering::Release);
    }

    fn report(&self, session: &mut Session) {
        let new = session.take_errors();
        if !new.is_empty() {
            for message in &new {
                log::warn!("{message}");
            }
            self.errors
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .extend(new);
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
    screen: std::sync::Arc<Screen>,
) {
    let store = FsSaveStore::for_rom(&save_path);
    let mut session = match Session::load(rom, Box::new(store), Box::new(SystemClock)) {
        Ok(session) => session,
        Err(e) => {
            let _ = ready.send(Err(e.to_string()));
            return;
        }
    };
    let _ = ready.send(Ok(session.title().to_string()));
    screen.publish(session.framebuffer());
    screen.report(&mut session);

    let mut audio = audio.then(CpalAudio::open);
    let mut paused = false;
    let mut last_tick = Instant::now();
    'run: loop {
        // 일시정지 중에는 명령이 올 때까지 잠든다.
        let received = if paused {
            commands.recv().map_err(|_| RecvTimeoutError::Disconnected)
        } else {
            commands.recv_timeout(TICK)
        };
        match received {
            Ok(command) => {
                // 밀린 명령까지 다 처리한 뒤에 프레임을 돌린다.
                for command in std::iter::once(command).chain(commands.try_iter()) {
                    if !handle(command, &mut session, &mut paused, &screen) {
                        break 'run;
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break 'run,
        }

        let now = Instant::now();
        let elapsed = now - last_tick;
        last_tick = now;
        let mode = if paused {
            RunMode::Paused
        } else {
            RunMode::Normal
        };
        if session.run(elapsed, mode, &mut || false) > 0 {
            screen.publish(session.framebuffer());
        }
        if let Some(sink) = audio.as_mut().and_then(CpalAudio::sink) {
            session.pump_audio(sink);
        }
        screen.report(&mut session);
    }
    session.flush();
    screen.report(&mut session);
}

/// 명령 하나를 처리한다. 스레드를 끝내야 하면 `false`.
fn handle(command: Command, session: &mut Session, paused: &mut bool, screen: &Screen) -> bool {
    match command {
        Command::Button(button, pressed) => session.set_button(button, pressed),
        Command::SetPaused(value) => {
            *paused = value;
            if value {
                // 백그라운드로 가면 앱이 언제 끝날지 모르니 미리 저장한다.
                session.flush();
            }
        }
        Command::SetMuted(muted) => session.set_sound_off(muted),
        Command::SaveState(slot, reply) => {
            let _ = reply.send(check_slot(slot).and_then(|()| session.save_state(slot)));
        }
        Command::LoadState(slot, reply) => {
            let result = check_slot(slot).and_then(|()| session.load_state(slot));
            if result.is_ok() {
                screen.publish(session.framebuffer());
            }
            let _ = reply.send(result);
        }
        Command::Flush(reply) => {
            let _ = reply.send(session.flush());
        }
        Command::Stop => return false,
    }
    true
}

fn check_slot(slot: u8) -> Result<(), String> {
    if slot < STATE_SLOTS {
        Ok(())
    } else {
        Err(format!("슬롯은 1–{STATE_SLOTS}번만 있습니다"))
    }
}
