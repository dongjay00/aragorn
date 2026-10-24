use crate::{
    adapters::{CpalAudio, FsSaveStore, Gamepads, SystemClock, UnavailableUpdater},
    build_info,
    ui::{
        cheats::{CheatCommand, CheatsView},
        debugger::{DebugCommand, DebuggerView},
        input::{self, InputFrame, Keymap, TypingGuard},
        io_view::IoView,
        link::{LinkCommand, LinkView},
        screen::ScreenView,
        settings::{self, SettingsView},
        state_slots::{self, SlotCommand, SlotsView},
        update_view::{self, UpdateUiAction},
        vram_view::VramView,
    },
    update_worker::UpdateWorker,
};
use aragorn_app::{
    config::{Config, ConfigStore, UpdateConfig},
    pacing::{FastForward, RunMode, SpeedControl, UNLIMITED_BUDGET},
    recent::{self, RecentGame},
    session::{Clock, Session},
    update::{UpdateCommand, UpdateEvent, UpdateFlow, UpdateSource, Updater},
};
use eframe::egui;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

const RECHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

pub struct AppDeps {
    pub config_store: Box<dyn ConfigStore>,
    /// `None`이면 업데이트 확인을 하지 않는다 (개발 빌드)
    pub source: Option<Arc<dyn UpdateSource>>,
    /// `None`이면 설치판이 아니다
    pub updater: Option<Arc<dyn Updater>>,
    pub release_page: Option<String>,
    /// 실행할 때 명령줄로 받은 ROM 경로
    pub initial_rom: Option<PathBuf>,
}

/// ROM 파일을 읽어 세션을 만든다. 배터리 세이브는 ROM 옆 `.sav`에서 읽고 쓰고, RTC는 시스템 시계를 쓴다.
/// 실패하면 상태 표시줄에 보일 문구를 돌려준다.
pub fn load_session(path: &Path) -> Result<Session, String> {
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let rom = std::fs::read(path).map_err(|e| format!("{name}을(를) 읽을 수 없습니다: {e}"))?;
    let store = Box::new(FsSaveStore::for_rom(path));
    Session::load(rom, store, Box::new(SystemClock))
        .map_err(|e| format!("{name}을(를) 열 수 없습니다: {e}"))
}

/// 스크린샷 폴더: 사진 폴더의 `Aragorn/`. 사진 폴더를 모르면 ROM 옆 `screenshots/`.
pub fn screenshot_dir(rom: Option<&Path>) -> PathBuf {
    directories::UserDirs::new()
        .and_then(|dirs| dirs.picture_dir().map(|d| d.join("Aragorn")))
        .or_else(|| rom.and_then(Path::parent).map(|d| d.join("screenshots")))
        .unwrap_or_else(|| PathBuf::from("screenshots"))
}

/// `<제목> 001.png`부터 비어 있는 첫 번호. 제목의 경로 구분자는 `_`로 바꾼다.
pub fn next_screenshot_path(dir: &Path, title: &str) -> PathBuf {
    let title: String = title
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let title = if title.trim().is_empty() {
        "Aragorn".to_string()
    } else {
        title
    };
    (1..)
        .map(|n| dir.join(format!("{title} {n:03}.png")))
        .find(|path| !path.exists())
        .expect("번호는 끝이 없다")
}

/// 위젯의 키보드 포커스를 없앤다. 게임 키(Tab=배속, 방향키, Enter=Start)가 egui 포커스 이동과
/// 버튼 누르기로 새지 않게 설정 창이 닫혀 있을 때 매 화면 갱신마다 부른다.
/// 글자 입력칸(디버거의 주소 입력)의 포커스는 남긴다. 그동안 키보드는 게임에 가지 않는다.
pub fn release_widget_focus(ctx: &egui::Context) {
    if ctx.text_edit_focused() {
        return;
    }
    ctx.memory_mut(|m| {
        if let Some(id) = m.focused() {
            m.surrender_focus(id);
        }
    });
}

/// 상태 표시줄에 보일 실행 모드. 보통 속도면 표시하지 않는다.
pub fn mode_text(mode: RunMode) -> Option<String> {
    match mode {
        RunMode::Normal => None,
        RunMode::Paused => Some("일시정지".to_string()),
        RunMode::Rewind => Some("되감기".to_string()),
        RunMode::Fast(FastForward::Unlimited) => Some("배속 (무제한)".to_string()),
        RunMode::Fast(speed) => Some(format!("배속 ({})", speed.label())),
    }
}

/// 다음 화면 갱신까지 기다릴 시간. 일시정지 중에는 게임 화면이 바뀌지 않으므로 천천히 다시 그린다.
/// 게임패드의 일시정지 버튼은 다시 그릴 때 읽으므로 완전히 멈추지는 않는다.
pub fn repaint_delay(mode: RunMode) -> Duration {
    match mode {
        RunMode::Paused => Duration::from_millis(100),
        RunMode::Normal | RunMode::Fast(_) | RunMode::Rewind => Duration::ZERO,
    }
}

/// 스테이트 저장·불러오기를 할 수 없을 때 상태 표시줄에 보일 이유.
pub fn slot_blocked_notice(exiting: bool, update_required: bool) -> Option<&'static str> {
    if exiting {
        Some("종료하는 중이라 스테이트를 저장하거나 불러올 수 없습니다")
    } else if update_required {
        Some("업데이트가 필요해서 스테이트를 저장하거나 불러올 수 없습니다")
    } else {
        None
    }
}

/// 저장하지 못한 진행이 있을 때 ROM 교체·종료를 계속할지 정한다.
/// 처음에는 멈추고 경고하며, 사용자가 같은 동작을 한 번 더 하면 저장 없이 진행한다.
pub fn proceed_after_flush(flushed: bool, warned: &mut bool) -> bool {
    if flushed || *warned {
        *warned = false;
        return true;
    }
    *warned = true;
    false
}

const UNSAVED_WARNING: &str =
    "세이브를 저장하지 못했습니다. 한 번 더 하면 저장하지 않고 진행합니다";

pub struct AragornApp {
    flow: UpdateFlow,
    worker: Option<UpdateWorker>,
    updater: Arc<dyn Updater>,
    config_store: Box<dyn ConfigStore>,
    config: Config,
    release_page: Option<String>,
    last_check: Instant,
    exit_handled: bool,
    session: Option<Session>,
    /// 지금 게임의 ROM 경로 ("처음부터 다시 시작"에 쓴다).
    rom_path: Option<PathBuf>,
    screen: ScreenView,
    last_tick: Instant,
    /// ROM 로드 결과 등 상태 표시줄에 보일 문구
    notice: Option<String>,
    /// 세이브 저장 실패를 이미 경고했는지 (`proceed_after_flush`)
    unsaved_warned: bool,
    audio: CpalAudio,
    gamepads: Gamepads,
    /// 설정의 키보드 매핑을 egui 키로 바꾼 것. 설정이 바뀌면 다시 만든다.
    keymap: Keymap,
    speed: SpeedControl,
    /// 직전 화면 갱신의 실행 모드 (상태 표시줄용)
    mode: RunMode,
    settings: SettingsView,
    slots: SlotsView,
    debugger: DebuggerView,
    vram: VramView,
    io: IoView,
    cheats: CheatsView,
    link: LinkView,
    typing_guard: TypingGuard,
}

impl AragornApp {
    pub fn new(ctx: &egui::Context, deps: AppDeps) -> Self {
        let config = deps.config_store.load();
        let keymap = Keymap::new(&config.input.keyboard);
        let flow = UpdateFlow::new(
            build_info::current_version(),
            config.update.to_prefs(),
            deps.updater.is_some(),
        );
        let updater: Arc<dyn Updater> =
            deps.updater.unwrap_or_else(|| Arc::new(UnavailableUpdater));
        let worker = deps.source.map(|source| {
            let ctx = ctx.clone();
            UpdateWorker::spawn(
                source,
                Arc::clone(&updater),
                Box::new(move || ctx.request_repaint()),
            )
        });
        let mut app = Self {
            flow,
            worker,
            updater,
            config_store: deps.config_store,
            config,
            release_page: deps.release_page,
            last_check: Instant::now(),
            exit_handled: false,
            session: None,
            rom_path: None,
            screen: ScreenView::default(),
            last_tick: Instant::now(),
            notice: None,
            unsaved_warned: false,
            audio: CpalAudio::open(),
            gamepads: Gamepads::open(),
            keymap,
            speed: SpeedControl::default(),
            mode: RunMode::Normal,
            settings: SettingsView::default(),
            slots: SlotsView::default(),
            debugger: DebuggerView::default(),
            vram: VramView::default(),
            io: IoView::default(),
            cheats: CheatsView::default(),
            link: LinkView::default(),
            typing_guard: TypingGuard::default(),
        };
        if app.worker.is_some() {
            app.dispatch(UpdateEvent::CheckRequested);
        }
        if let Some(path) = deps.initial_rom {
            app.open_rom(ctx, &path);
        }
        app
    }

    /// ROM을 연다. 이어하기를 켰으면 지난번에 닫은 자리에서 이어서 한다.
    fn open_rom(&mut self, ctx: &egui::Context, path: &Path) {
        self.open_rom_with(ctx, path, true);
    }

    /// `resume`이 거짓이면 이어하기 없이 게임 세이브에서 시작한다 ("처음부터 다시 시작").
    fn open_rom_with(&mut self, ctx: &egui::Context, path: &Path, resume: bool) {
        let flushed = self.flush_session();
        if !proceed_after_flush(flushed, &mut self.unsaved_warned) {
            self.notice = Some(UNSAVED_WARNING.to_string());
            return;
        }
        // 다른 게임으로 바꾸기 전에 지금 게임의 자리를 남긴다. 같은 게임을 처음부터 다시 시작할 때는 남기지 않는다.
        if resume {
            self.save_resume();
        }
        match load_session(path) {
            Ok(session) => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
                    "Aragorn v{} - {}",
                    self.flow.current(),
                    session.title()
                )));
                self.screen.set_video(self.config.video);
                self.screen.update(ctx, &session);
                self.remember_recent(path, session.title());
                self.notice = Some(format!("{} 실행 중", session.title()));
                // 일시정지한 채 다른 게임을 열어도 바로 시작한다.
                self.speed.resume();
                self.slots.invalidate();
                let mut session = session;
                session.set_breakpoints(self.debugger.breakpoints());
                for (channel, muted) in self.io.muted().into_iter().enumerate() {
                    session.set_channel_muted(channel, muted);
                }
                session.set_sound_off(self.config.audio.muted);
                session.set_rewind_enabled(self.config.rewind.enabled);
                if resume && self.config.resume.enabled {
                    match session.resume() {
                        Ok(true) => {
                            self.notice = Some(format!(
                                "{} — 지난번에 닫은 자리에서 이어서 합니다",
                                session.title()
                            ));
                            self.screen.update(ctx, &session);
                        }
                        Ok(false) => {}
                        Err(message) => {
                            log::warn!("{message}");
                            self.notice = Some(message);
                        }
                    }
                }
                self.session = Some(session);
                self.rom_path = Some(path.to_path_buf());
                self.last_tick = Instant::now();
                self.collect_session_errors();
            }
            Err(message) => {
                log::warn!("{message}");
                self.notice = Some(message);
                // 지우거나 옮긴 파일은 최근 목록에서 뺀다.
                if !path.exists() {
                    recent::remove(&mut self.config.recent, &path.to_string_lossy());
                    self.save_config();
                }
            }
        }
    }

    /// 이어하기를 켰으면 지금 게임의 상태를 남긴다. 실패하면 상태 표시줄에 알리고 계속한다.
    fn save_resume(&mut self) {
        if !self.config.resume.enabled {
            return;
        }
        let Some(session) = &mut self.session else {
            return;
        };
        if let Err(message) = session.save_resume() {
            log::error!("{message}");
            self.notice = Some(message);
        }
    }

    /// 지금 게임을 게임 세이브에서 다시 시작한다 (이어하기를 쓰지 않는다).
    fn restart(&mut self, ctx: &egui::Context) {
        if let Some(path) = self.rom_path.clone() {
            self.open_rom_with(ctx, &path, false);
        }
    }

    /// 지금 화면(화면 필터 적용)을 사진 폴더의 `Aragorn/`에 PNG로 저장한다.
    fn save_screenshot(&mut self) {
        let Some(session) = &self.session else {
            return;
        };
        let frame = session.display(&self.config.video);
        let png = aragorn_app::screenshot::png(
            &frame.pixels,
            frame.width,
            frame.height,
            aragorn_app::screenshot::SCALE,
        );
        let dir = screenshot_dir(self.rom_path.as_deref());
        let result = std::fs::create_dir_all(&dir).and_then(|()| {
            let path = next_screenshot_path(&dir, session.title());
            std::fs::write(&path, png).map(|()| path)
        });
        self.notice = Some(match result {
            Ok(path) => format!("스크린샷을 저장했습니다: {}", path.display()),
            Err(e) => format!("스크린샷을 저장하지 못했습니다: {e}"),
        });
    }

    /// 최근 게임 목록 맨 앞에 넣는다. 헤더에 제목이 없으면 파일 이름을 쓴다.
    fn remember_recent(&mut self, path: &Path, title: &str) {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let title = if title.is_empty() {
            path.file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
        } else {
            title.to_string()
        };
        recent::add(
            &mut self.config.recent,
            RecentGame {
                path: path.to_string_lossy().into_owned(),
                title,
                opened_at: SystemClock.now_unix(),
            },
        );
        self.save_config();
    }

    /// 링크 창의 요청을 처리한다.
    fn run_link_command(&mut self, command: LinkCommand) {
        let Some(session) = &mut self.session else {
            return;
        };
        match command {
            LinkCommand::Host => match aragorn_host::TcpLink::host(aragorn_host::DEFAULT_PORT) {
                Ok(link) => session.set_link(Box::new(link)),
                Err(e) => self.notice = Some(format!("링크를 열 수 없습니다: {e}")),
            },
            LinkCommand::Connect(address) => {
                session.set_link(Box::new(aragorn_host::TcpLink::connect(&address)));
            }
            LinkCommand::Disconnect => session.disconnect_link(),
        }
    }

    /// 치트 창의 요청을 처리한다.
    fn run_cheat_command(&mut self, command: CheatCommand) {
        let Some(session) = &mut self.session else {
            return;
        };
        let added = matches!(command, CheatCommand::Add { .. });
        let result = match command {
            CheatCommand::Add { code, name } => session.add_cheat(&code, &name),
            CheatCommand::SetEnabled(index, enabled) => session.set_cheat_enabled(index, enabled),
            CheatCommand::Remove(index) => session.remove_cheat(index),
        };
        match result {
            Ok(()) if added => {
                self.cheats.added();
                self.notice = Some("치트를 켰습니다".to_string());
            }
            Ok(()) => {}
            Err(message) => {
                log::warn!("{message}");
                self.notice = Some(message);
            }
        }
    }

    /// 화면 필터와 되감기 설정을 지금 게임에 적용한다.
    fn apply_play_settings(&mut self, ctx: &egui::Context) {
        if let Some(session) = &mut self.session {
            session.set_rewind_enabled(self.config.rewind.enabled);
            self.screen.set_video(self.config.video);
            self.screen.update(ctx, session);
        }
    }

    /// 게임패드와 키보드 입력을 모은다. 설정 창이 키를 기다리는 중이면 그 키를 지정하고
    /// 게임에는 입력을 넘기지 않는다(`None`).
    fn gather_input(&mut self, ctx: &egui::Context) -> Option<InputFrame> {
        let pad = self.gamepads.poll(&self.config.input.gamepad);
        for notice in pad.notices {
            log::info!("{notice}");
            self.notice = Some(notice);
        }
        if self.settings.is_capturing() {
            let key = ctx.input(|i| settings::pressed_key(&i.events));
            if self
                .settings
                .capture(&mut self.config.input, key, pad.first_pressed)
            {
                self.input_config_changed();
            }
            return None;
        }
        // 글자 입력칸에 입력하는 동안, 그리고 그때 눌린 키를 뗄 때까지 키보드는 게임에 넘기지 않는다
        // (게임패드는 그대로).
        let typing = ctx.text_edit_focused();
        self.typing_guard
            .update(typing, &ctx.input(|i| i.keys_down.clone()));
        let guard = &self.typing_guard;
        Some(input::gather(
            &self.keymap,
            |key| guard.allows(typing, key) && ctx.input(|i| i.key_down(key)),
            |key| guard.allows(typing, key) && ctx.input(|i| input::pressed_once(&i.events, key)),
            &pad.input,
        ))
    }

    /// 스테이트 슬롯에 저장하거나 불러오고 결과를 상태 표시줄에 알린다. 종료 중이거나 강제 업데이트
    /// 중에는 하지 않는다.
    fn run_slot_command(&mut self, ctx: &egui::Context, command: SlotCommand) {
        if let Some(reason) = slot_blocked_notice(self.exit_handled, self.flow.blocks_emulation()) {
            self.notice = Some(reason.to_string());
            return;
        }
        let Some(session) = &mut self.session else {
            return;
        };
        let result = match command {
            SlotCommand::Save(slot) => {
                self.slots.invalidate();
                session
                    .save_state(slot)
                    .map(|()| format!("슬롯 {}에 저장했습니다", slot + 1))
            }
            SlotCommand::Load(slot) => session.load_state(slot).map(|()| {
                self.screen.update(ctx, session);
                format!("슬롯 {}을(를) 불러왔습니다", slot + 1)
            }),
        };
        match result {
            Ok(message) => self.notice = Some(message),
            Err(message) => {
                log::warn!("{message}");
                self.notice = Some(message);
            }
        }
        self.collect_session_errors();
    }

    /// 디버거 창의 요청을 처리한다. 단계 실행은 일시정지한 상태에서 한다. 종료 중이거나 강제 업데이트
    /// 중에는 하지 않는다.
    fn run_debug_command(&mut self, ctx: &egui::Context, command: DebugCommand) {
        let Some(session) = &mut self.session else {
            return;
        };
        // 음소거는 게임을 진행하지 않으니 언제든 바꾼다. 창의 체크 상태와 어긋나지 않게 한다.
        if let DebugCommand::SetChannelMuted(channel, muted) = command {
            session.set_channel_muted(channel, muted);
            return;
        }
        if self.exit_handled || self.flow.blocks_emulation() {
            return;
        }
        match command {
            DebugCommand::TogglePause => self.speed.toggle_pause(),
            DebugCommand::StepInstruction | DebugCommand::StepFrame => {
                self.speed.pause();
                if command == DebugCommand::StepInstruction {
                    session.step_instruction();
                } else {
                    session.step_frame();
                }
                self.screen.update(ctx, session);
            }
            DebugCommand::SetBreakpoints(pcs) => session.set_breakpoints(&pcs),
            DebugCommand::SetChannelMuted(..) => unreachable!("위에서 처리한다"),
        }
        self.collect_session_errors();
    }

    /// 음소거를 바꾸고 설정에 저장한다.
    fn set_muted(&mut self, muted: bool) {
        self.config.audio.muted = muted;
        if let Some(session) = &mut self.session {
            session.set_sound_off(muted);
        }
        self.save_config();
    }

    fn input_config_changed(&mut self) {
        self.keymap = Keymap::new(&self.config.input.keyboard);
        self.save_config();
    }

    /// 경과 시간만큼 에뮬레이션을 진행하고 화면을 갱신한다. 강제 업데이트 중에는 멈춘다.
    fn run_emulation(&mut self, ctx: &egui::Context, input: Option<InputFrame>) {
        let now = Instant::now();
        let elapsed = now - self.last_tick;
        self.last_tick = now;
        // 음소거는 게임을 진행하지 않으니 ROM이 없거나 멈춘 동안에도 바꾼다.
        if input.as_ref().is_some_and(|f| f.mute_pressed) {
            self.set_muted(!self.config.audio.muted);
        }
        let Some(session) = &mut self.session else {
            return;
        };
        // 종료 중에는 마지막 저장 뒤로 게임이 진행되지 않게 멈춘다.
        if self.exit_handled || self.flow.blocks_emulation() {
            return;
        }
        let (fast_held, rewind_held, pause_pressed) =
            input.as_ref().map_or((false, false, false), |f| {
                (f.fast_forward, f.rewind, f.pause_pressed)
            });
        if pause_pressed {
            self.speed.toggle_pause();
        }
        let buttons = input.map_or(gb_core::Button::ALL.map(|b| (b, false)), |f| f.buttons);
        for (button, pressed) in buttons {
            session.set_button(button, pressed);
        }
        let input_config = &self.config.input;
        self.mode = self.speed.mode(
            fast_held,
            rewind_held,
            input_config.fast_forward,
            input_config.base_speed,
        );
        let started = Instant::now();
        let mut within_budget = || started.elapsed() < UNLIMITED_BUDGET;
        if session.run(elapsed, self.mode, &mut within_budget) > 0 {
            self.screen.update(ctx, session);
        }
        if let Some(pc) = session.take_breakpoint_stop() {
            self.speed.pause();
            self.mode = RunMode::Paused;
            self.debugger.cpu_open = true;
            self.notice = Some(format!("브레이크포인트 ${pc:04X}에서 멈췄습니다"));
        }
        if let Some(sink) = self.audio.sink() {
            session.pump_audio(sink);
        }
        self.collect_session_errors();
        ctx.request_repaint_after(repaint_delay(self.mode));
    }

    /// 저장하지 않은 배터리 세이브를 디스크에 쓴다. 저장하지 못한 진행이 남아 있으면 `false`.
    fn flush_session(&mut self) -> bool {
        let flushed = self.session.as_mut().is_none_or(Session::flush);
        self.collect_session_errors();
        flushed
    }

    /// 세션의 세이브 오류를 로그와 상태 표시줄로 옮긴다.
    fn collect_session_errors(&mut self) {
        let Some(session) = &mut self.session else {
            return;
        };
        for message in session.take_errors() {
            log::error!("{message}");
            self.notice = Some(message);
        }
    }

    fn dispatch(&mut self, event: UpdateEvent) {
        for command in self.flow.handle(event) {
            match command {
                UpdateCommand::SavePrefs(prefs) => {
                    self.config.update = UpdateConfig::from_prefs(&prefs);
                    self.save_config();
                }
                UpdateCommand::ApplyOnExit => {
                    self.flush_persistent_state();
                    if let Err(e) = self.updater.apply_on_exit() {
                        log::error!("종료 후 업데이트 예약 실패: {e}");
                    }
                }
                UpdateCommand::ApplyAndRestart => {
                    self.flush_persistent_state();
                    self.send(command);
                }
                UpdateCommand::FetchPolicy | UpdateCommand::Download(_) => self.send(command),
            }
        }
    }

    fn send(&self, command: UpdateCommand) {
        if let Some(worker) = &self.worker {
            worker.send(command);
        }
    }

    fn save_config(&self) {
        if let Err(e) = self.config_store.save(&self.config) {
            log::error!("설정 저장 실패: {e}");
        }
    }

    /// 업데이트를 적용하기 전에 디스크에 남겨야 하는 상태(설정, 배터리 세이브)를 저장한다.
    fn flush_persistent_state(&mut self) {
        self.save_config();
        self.flush_session();
        self.save_resume();
    }
}

impl eframe::App for AragornApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut events = Vec::new();
        if let Some(worker) = &self.worker {
            while let Some(event) = worker.try_recv() {
                events.push(event);
            }
            if self.last_check.elapsed() >= RECHECK_INTERVAL {
                self.last_check = Instant::now();
                events.push(UpdateEvent::CheckRequested);
            }
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.exit_handled {
            let flushed = self.flush_session();
            if proceed_after_flush(flushed, &mut self.unsaved_warned) {
                self.save_resume();
                self.exit_handled = true;
                events.push(UpdateEvent::AppExiting);
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.notice = Some(UNSAVED_WARNING.to_string());
            }
        }
        for event in events {
            self.dispatch(event);
        }
        let dropped = ctx.input(|i| i.raw.dropped_files.first().map(|f| f.path().to_path_buf()));
        if let Some(path) = dropped {
            self.open_rom(ctx, &path);
        }
        if !self.settings.open {
            release_widget_focus(ctx);
        }
        let input = self.gather_input(ctx);
        if input.is_some()
            && !ctx.text_edit_focused()
            && let Some(command) = ctx.input(|i| state_slots::hotkey(&i.events))
        {
            self.run_slot_command(ctx, command);
        }
        if input.is_some()
            && !ctx.text_edit_focused()
            && ctx.input(|i| input::pressed_once(&i.events, egui::Key::F12))
        {
            self.save_screenshot();
        }
        self.run_emulation(ctx, input);
        // 6시간 재확인 타이머가 유휴 상태에서도 돌도록 주기적으로 깨운다.
        ctx.request_repaint_after(Duration::from_secs(60));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let mut events = Vec::new();

        let mut open_requested = false;
        let mut recent_requested: Option<PathBuf> = None;
        let mut restart_requested = false;
        let mut screenshot_requested = false;
        let mut slot_command = None;
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("ROM 열기").clicked() {
                    open_requested = true;
                }
                ui.menu_button("디버거", |ui| {
                    ui.checkbox(&mut self.debugger.cpu_open, "CPU");
                    ui.checkbox(&mut self.debugger.memory_open, "메모리");
                    ui.checkbox(&mut self.vram.open, "VRAM");
                    ui.checkbox(&mut self.io.open, "I/O");
                });
                if !self.config.recent.is_empty() {
                    ui.menu_button("최근 게임", |ui| {
                        for game in &self.config.recent {
                            if ui.button(&game.title).on_hover_text(&game.path).clicked() {
                                recent_requested = Some(PathBuf::from(&game.path));
                                ui.close();
                            }
                        }
                    });
                }
                if ui.button("설정").clicked() {
                    self.settings.open = !self.settings.open;
                }
                if self.session.is_some() && ui.button("치트").clicked() {
                    self.cheats.open = !self.cheats.open;
                }
                if self.session.is_some() && ui.button("링크").clicked() {
                    self.link.open = !self.link.open;
                }
                if let Some(session) = &self.session {
                    let now = SystemClock.now_unix();
                    ui.menu_button("스테이트", |ui| {
                        slot_command = self.slots.show(ui, session, now);
                        ui.separator();
                        if ui
                            .button("처음부터 다시 시작")
                            .on_hover_text("이어하기 없이 게임 세이브에서 다시 시작합니다")
                            .clicked()
                        {
                            restart_requested = true;
                            ui.close();
                        }
                    });
                    if ui.button("스크린샷").on_hover_text("F12").clicked() {
                        screenshot_requested = true;
                    }
                }
                let mut muted = self.config.audio.muted;
                if ui.checkbox(&mut muted, "음소거").changed() {
                    self.set_muted(muted);
                }
                ui.separator();
                let mut auto_download = self.flow.prefs().auto_download;
                if ui
                    .checkbox(&mut auto_download, "자동으로 업데이트 다운로드")
                    .changed()
                {
                    events.push(UpdateEvent::SetAutoDownload(auto_download));
                }
                if self.worker.is_some() && ui.button("업데이트 확인").clicked() {
                    events.push(UpdateEvent::CheckRequested);
                }
            });
        });

        for action in update_view::show(ui, &self.flow, self.release_page.as_deref()) {
            match action {
                UpdateUiAction::Event(event) => events.push(event),
                UpdateUiAction::Quit => ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close),
            }
        }

        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                if self.session.is_some()
                    && let Some(text) = mode_text(self.mode)
                {
                    ui.strong(text);
                    ui.separator();
                }
                if let Some(status) = self.session.as_ref().and_then(Session::link_status) {
                    ui.strong(status.text());
                    ui.separator();
                }
                if self.session.is_some() && self.config.audio.muted {
                    ui.strong("음소거");
                    ui.separator();
                }
                if let Some(notice) = &self.notice {
                    ui.label(notice);
                    ui.separator();
                }
                ui.label(update_view::status_text(
                    self.flow.state(),
                    self.worker.is_some(),
                ));
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            if self.session.is_some() {
                self.screen.show(ui);
            } else {
                ui.vertical_centered(|ui| {
                    ui.add_space(120.0);
                    ui.heading(format!("Aragorn v{}", self.flow.current()));
                    ui.label(
                        "ROM 열기 버튼을 누르거나 ROM 파일(.gb, .gbc)을 창에 끌어다 놓으세요.",
                    );
                    if !self.config.recent.is_empty() {
                        ui.add_space(16.0);
                        ui.strong("최근 게임");
                        let now = SystemClock.now_unix();
                        for game in &self.config.recent {
                            let text = format!(
                                "{} · {}",
                                game.title,
                                aragorn_app::session::ago_text(now, game.opened_at)
                            );
                            if ui.button(text).on_hover_text(&game.path).clicked() {
                                recent_requested = Some(PathBuf::from(&game.path));
                            }
                        }
                    }
                });
            }
        });

        let debug_commands = match &self.session {
            Some(session) => {
                let mut commands = self
                    .debugger
                    .show(ui.ctx(), session, self.speed.is_paused());
                self.vram.show(ui.ctx(), &session.debug());
                commands.extend(self.io.show(ui.ctx(), &session.debug()));
                commands
            }
            None => Vec::new(),
        };
        for command in debug_commands {
            let ctx = ui.ctx().clone();
            self.run_debug_command(&ctx, command);
        }
        if let Some(command) = slot_command {
            let ctx = ui.ctx().clone();
            self.run_slot_command(&ctx, command);
        }
        if self.settings.show(ui.ctx(), &mut self.config) {
            self.input_config_changed();
            let ctx = ui.ctx().clone();
            self.apply_play_settings(&ctx);
        }
        let cheat_commands = match &self.session {
            Some(session) => self.cheats.show(ui.ctx(), session.cheats()),
            None => Vec::new(),
        };
        for command in cheat_commands {
            self.run_cheat_command(command);
        }
        let link_command = match &self.session {
            Some(session) => self.link.show(ui.ctx(), session.link_status().as_ref()),
            None => None,
        };
        if let Some(command) = link_command {
            self.run_link_command(command);
        }
        if restart_requested {
            let ctx = ui.ctx().clone();
            self.restart(&ctx);
        }
        if screenshot_requested {
            self.save_screenshot();
        }
        if let Some(path) = recent_requested {
            let ctx = ui.ctx().clone();
            self.open_rom(&ctx, &path);
        }

        for event in events {
            self.dispatch(event);
        }
        if open_requested
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("Game Boy ROM", &["gb", "gbc"])
                .pick_file()
        {
            let ctx = ui.ctx().clone();
            self.open_rom(&ctx, &path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 툴바 버튼 하나를 그리는 화면 갱신을 `key` 입력으로 한 번 돌리고, 버튼이 눌렸는지 돌려준다.
    /// `guard`면 앱의 `logic()`처럼 그리기 전에 포커스를 없앤다.
    fn toolbar_frame(ctx: &egui::Context, key: egui::Key, guard: bool) -> bool {
        let input = egui::RawInput {
            events: vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        let mut clicked = false;
        let mut output = ctx.run_ui(input, |ui| {
            if guard {
                release_widget_focus(ui.ctx());
            }
            clicked |= ui.button("ROM 열기").clicked();
        });
        output.textures_delta.clear();
        clicked
    }

    #[test]
    fn text_field_keeps_focus_while_typing() {
        let ctx = egui::Context::default();
        let mut text = String::new();
        let id = egui::Id::new("address");
        // 첫 화면 갱신에서 입력칸을 누른 것처럼 포커스를 주고, 이후 갱신마다 앱처럼 포커스를 정리한다.
        for frame in 0..3 {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                release_widget_focus(ui.ctx());
                let response = ui.add(egui::TextEdit::singleline(&mut text).id(id));
                if frame == 0 {
                    response.request_focus();
                }
            });
            output.textures_delta.clear();
        }
        assert!(ctx.memory(|m| m.has_focus(id)), "입력칸 포커스는 남는다");
    }

    #[test]
    fn tab_then_enter_does_not_press_toolbar_button() {
        // 막지 않으면 Tab(배속)이 버튼에 포커스를 주고 Enter(Start)가 버튼을 누른다.
        let ctx = egui::Context::default();
        toolbar_frame(&ctx, egui::Key::Tab, false);
        assert!(toolbar_frame(&ctx, egui::Key::Enter, false), "재현 확인");

        let ctx = egui::Context::default();
        toolbar_frame(&ctx, egui::Key::Tab, true);
        assert!(!toolbar_frame(&ctx, egui::Key::Enter, true));
    }

    #[test]
    fn paused_emulation_repaints_slowly() {
        assert_eq!(repaint_delay(RunMode::Paused), Duration::from_millis(100));
        assert_eq!(repaint_delay(RunMode::Normal), Duration::ZERO);
        assert_eq!(
            repaint_delay(RunMode::Fast(FastForward::Unlimited)),
            Duration::ZERO
        );
    }

    #[test]
    fn blocked_state_commands_explain_why() {
        assert_eq!(slot_blocked_notice(false, false), None);
        assert!(slot_blocked_notice(true, false).unwrap().contains("종료"));
        assert!(
            slot_blocked_notice(false, true)
                .unwrap()
                .contains("업데이트")
        );
    }

    #[test]
    fn mode_text_shows_only_unusual_speeds() {
        assert_eq!(mode_text(RunMode::Normal), None);
        assert_eq!(mode_text(RunMode::Paused).as_deref(), Some("일시정지"));
        assert_eq!(
            mode_text(RunMode::Fast(FastForward::X4)).as_deref(),
            Some("배속 (4배)")
        );
        assert_eq!(
            mode_text(RunMode::Fast(FastForward::Unlimited)).as_deref(),
            Some("배속 (무제한)")
        );
    }

    #[test]
    fn missing_rom_file_reports_read_error() {
        let err = load_session(Path::new("/없는/경로/pokemon.gb"))
            .err()
            .unwrap();
        assert!(
            err.starts_with("pokemon.gb을(를) 읽을 수 없습니다"),
            "{err}"
        );
    }

    #[test]
    fn invalid_rom_reports_cartridge_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.gb");
        std::fs::write(&path, [0u8; 16]).unwrap();
        let err = load_session(&path).err().unwrap();
        assert_eq!(
            err,
            "broken.gb을(를) 열 수 없습니다: ROM 파일이 너무 작습니다 (16바이트)"
        );
    }

    #[test]
    fn game_save_is_written_next_to_rom() {
        // MBC1+RAM+BATTERY ROM: RAM 켜기 → 0xA000에 0x42 → RAM 끄기 → 제자리 루프
        let mut rom = vec![0u8; 0x8000];
        let program = [
            0x3E, 0x0A, 0xEA, 0x00, 0x00, 0x3E, 0x42, 0xEA, 0x00, 0xA0, 0xAF, 0xEA, 0x00, 0x00,
            0x18, 0xFE,
        ];
        rom[0x0100..0x0100 + program.len()].copy_from_slice(&program);
        rom[0x0147] = 0x03;
        rom[0x0149] = 0x02;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("game.gb");
        std::fs::write(&path, &rom).unwrap();

        let mut session = load_session(&path).unwrap();
        session.advance(aragorn_app::pacing::FRAME_DURATION);
        let save = std::fs::read(dir.path().join("game.sav")).unwrap();
        assert_eq!((save.len(), save[0]), (0x2000, 0x42));

        let reloaded = load_session(&path).unwrap();
        assert_eq!(reloaded.battery_ram().unwrap()[0], 0x42);
    }

    /// 저장한 설정을 테스트가 볼 수 있는 저장소.
    #[derive(Clone, Default)]
    struct MemoryConfig(std::rc::Rc<std::cell::RefCell<Option<Config>>>);

    impl ConfigStore for MemoryConfig {
        fn load(&self) -> Config {
            self.0.borrow().clone().unwrap_or_default()
        }
        fn save(&self, config: &Config) -> std::io::Result<()> {
            *self.0.borrow_mut() = Some(config.clone());
            Ok(())
        }
    }

    #[test]
    fn opened_rom_goes_to_recent_games_and_missing_one_leaves() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("game.gb");
        let mut rom = vec![0u8; 0x8000];
        rom[0x0100..0x0102].copy_from_slice(&[0x18, 0xFE]);
        rom[0x0134..0x0138].copy_from_slice(b"GAME");
        std::fs::write(&path, &rom).unwrap();
        let store = MemoryConfig::default();
        let ctx = egui::Context::default();
        let mut app = AragornApp::new(
            &ctx,
            AppDeps {
                config_store: Box::new(store.clone()),
                source: None,
                updater: None,
                release_page: None,
                initial_rom: Some(path.clone()),
            },
        );
        let saved = store.load();
        assert_eq!(saved.recent.len(), 1);
        assert_eq!(saved.recent[0].title, "GAME");

        std::fs::remove_file(&path).unwrap();
        let canonical = PathBuf::from(&saved.recent[0].path);
        app.open_rom(&ctx, &canonical);
        assert!(store.load().recent.is_empty(), "지운 파일은 목록에서 뺀다");
    }

    #[test]
    fn screenshot_names_count_up_and_avoid_separators() {
        let dir = tempfile::tempdir().unwrap();
        let first = next_screenshot_path(dir.path(), "PM/RED");
        assert_eq!(first.file_name().unwrap(), "PM_RED 001.png");
        std::fs::write(&first, b"").unwrap();
        assert_eq!(
            next_screenshot_path(dir.path(), "PM/RED")
                .file_name()
                .unwrap(),
            "PM_RED 002.png"
        );
        assert_eq!(
            next_screenshot_path(dir.path(), " ").file_name().unwrap(),
            "Aragorn 001.png"
        );
    }

    #[test]
    fn closing_another_game_leaves_a_resume_point() {
        let dir = tempfile::tempdir().unwrap();
        let rom = |name: &str| {
            let path = dir.path().join(name);
            let mut data = vec![0u8; 0x8000];
            // LD HL,0xC000 ; INC (HL) ; JR -3
            data[0x0100..0x0106].copy_from_slice(&[0x21, 0x00, 0xC0, 0x34, 0x18, 0xFD]);
            std::fs::write(&path, &data).unwrap();
            path
        };
        let (first, second) = (rom("a.gb"), rom("b.gb"));
        let ctx = egui::Context::default();
        let mut app = AragornApp::new(
            &ctx,
            AppDeps {
                config_store: Box::new(MemoryConfig::default()),
                source: None,
                updater: None,
                release_page: None,
                initial_rom: Some(first.clone()),
            },
        );
        for _ in 0..5 {
            app.session
                .as_mut()
                .unwrap()
                .advance(aragorn_app::pacing::FRAME_DURATION);
        }
        let at_close = app.session.as_ref().unwrap().debug().peek(0xC000);
        app.open_rom(&ctx, &second);
        assert!(dir.path().join("a.ssr").exists());
        app.open_rom(&ctx, &first);
        assert_eq!(app.session.as_ref().unwrap().debug().peek(0xC000), at_close);
        app.restart(&ctx);
        assert_ne!(
            app.session.as_ref().unwrap().debug().peek(0xC000),
            at_close,
            "처음부터"
        );
    }

    #[test]
    fn failed_save_warns_once_before_discarding() {
        let mut warned = false;
        assert!(proceed_after_flush(true, &mut warned));
        assert!(
            !proceed_after_flush(false, &mut warned),
            "처음에는 멈추고 알린다"
        );
        assert!(warned);
        assert!(
            proceed_after_flush(false, &mut warned),
            "한 번 더 하면 저장 없이 진행한다"
        );
        assert!(proceed_after_flush(true, &mut warned));
        assert!(!warned, "저장에 성공하면 경고 상태를 지운다");
    }
}
