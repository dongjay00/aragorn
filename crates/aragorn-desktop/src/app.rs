use crate::{
    adapters::{CpalAudio, FsSaveStore, Gamepads, SystemClock, UnavailableUpdater},
    build_info,
    ui::{
        input::{self, InputFrame, Keymap},
        screen::ScreenView,
        settings::SettingsView,
        state_slots::{self, SlotCommand, SlotsView},
        update_view::{self, UpdateUiAction},
    },
    update_worker::UpdateWorker,
};
use aragorn_app::{
    config::{Config, ConfigStore, UpdateConfig},
    pacing::{FastForward, RunMode, SpeedControl, UNLIMITED_BUDGET},
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

/// 위젯의 키보드 포커스를 없앤다. 게임 키(Tab=배속, 방향키, Enter=Start)가 egui 포커스 이동과
/// 버튼 누르기로 새지 않게 설정 창이 닫혀 있을 때 매 화면 갱신마다 부른다.
pub fn release_widget_focus(ctx: &egui::Context) {
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
        RunMode::Fast(FastForward::Unlimited) => Some("배속 (무제한)".to_string()),
        RunMode::Fast(speed) => Some(format!("배속 ({})", speed.label())),
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
        };
        if app.worker.is_some() {
            app.dispatch(UpdateEvent::CheckRequested);
        }
        if let Some(path) = deps.initial_rom {
            app.open_rom(ctx, &path);
        }
        app
    }

    fn open_rom(&mut self, ctx: &egui::Context, path: &Path) {
        let flushed = self.flush_session();
        if !proceed_after_flush(flushed, &mut self.unsaved_warned) {
            self.notice = Some(UNSAVED_WARNING.to_string());
            return;
        }
        match load_session(path) {
            Ok(session) => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
                    "Aragorn v{} - {}",
                    self.flow.current(),
                    session.title()
                )));
                self.screen.update(ctx, session.framebuffer());
                self.notice = Some(format!("{} 실행 중", session.title()));
                self.slots.invalidate();
                self.session = Some(session);
                self.last_tick = Instant::now();
                self.collect_session_errors();
            }
            Err(message) => {
                log::warn!("{message}");
                self.notice = Some(message);
            }
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
            let key = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        repeat: false,
                        ..
                    } => Some(*key),
                    _ => None,
                })
            });
            if self
                .settings
                .capture(&mut self.config.input, key, pad.first_pressed)
            {
                self.input_config_changed();
            }
            return None;
        }
        Some(input::gather(
            &self.keymap,
            |key| ctx.input(|i| i.key_down(key)),
            |key| ctx.input(|i| input::pressed_once(&i.events, key)),
            &pad.input,
        ))
    }

    /// 스테이트 슬롯에 저장하거나 불러오고 결과를 상태 표시줄에 알린다. 종료 중이거나 강제 업데이트
    /// 중에는 하지 않는다.
    fn run_slot_command(&mut self, ctx: &egui::Context, command: SlotCommand) {
        if self.exit_handled || self.flow.blocks_emulation() {
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
                self.screen.update(ctx, session.framebuffer());
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

    fn input_config_changed(&mut self) {
        self.keymap = Keymap::new(&self.config.input.keyboard);
        self.save_config();
    }

    /// 경과 시간만큼 에뮬레이션을 진행하고 화면을 갱신한다. 강제 업데이트 중에는 멈춘다.
    fn run_emulation(&mut self, ctx: &egui::Context, input: Option<InputFrame>) {
        let now = Instant::now();
        let elapsed = now - self.last_tick;
        self.last_tick = now;
        let Some(session) = &mut self.session else {
            return;
        };
        // 종료 중에는 마지막 저장 뒤로 게임이 진행되지 않게 멈춘다.
        if self.exit_handled || self.flow.blocks_emulation() {
            return;
        }
        let (fast_held, pause_pressed) = input
            .as_ref()
            .map_or((false, false), |f| (f.fast_forward, f.pause_pressed));
        if pause_pressed {
            self.speed.toggle_pause();
        }
        let buttons = input.map_or(gb_core::Button::ALL.map(|b| (b, false)), |f| f.buttons);
        for (button, pressed) in buttons {
            session.set_button(button, pressed);
        }
        self.mode = self.speed.mode(fast_held, self.config.input.fast_forward);
        let started = Instant::now();
        let mut within_budget = || started.elapsed() < UNLIMITED_BUDGET;
        if session.run(elapsed, self.mode, &mut within_budget) > 0 {
            self.screen.update(ctx, session.framebuffer());
        }
        if let Some(sink) = self.audio.sink() {
            session.pump_audio(sink);
        }
        self.collect_session_errors();
        ctx.request_repaint();
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
            && let Some(command) = ctx.input(|i| state_slots::hotkey(&i.events))
        {
            self.run_slot_command(ctx, command);
        }
        self.run_emulation(ctx, input);
        // 6시간 재확인 타이머가 유휴 상태에서도 돌도록 주기적으로 깨운다.
        ctx.request_repaint_after(Duration::from_secs(60));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let mut events = Vec::new();

        let mut open_requested = false;
        let mut slot_command = None;
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("ROM 열기").clicked() {
                    open_requested = true;
                }
                if ui.button("설정").clicked() {
                    self.settings.open = !self.settings.open;
                }
                if let Some(session) = &self.session {
                    let now = SystemClock.now_unix();
                    ui.menu_button("스테이트", |ui| {
                        slot_command = self.slots.show(ui, session, now);
                    });
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
                });
            }
        });

        if let Some(command) = slot_command {
            let ctx = ui.ctx().clone();
            self.run_slot_command(&ctx, command);
        }
        if self.settings.show(ui.ctx(), &mut self.config.input) {
            self.input_config_changed();
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
