use crate::{
    adapters::{FsSaveStore, UnavailableUpdater},
    build_info,
    ui::{
        input,
        screen::ScreenView,
        update_view::{self, UpdateUiAction},
    },
    update_worker::UpdateWorker,
};
use aragorn_app::{
    config::{Config, ConfigStore, UpdateConfig},
    session::Session,
    update::{UpdateCommand, UpdateEvent, UpdateFlow, UpdateSource, Updater},
};
use eframe::egui;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant, SystemTime},
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

/// ROM 파일을 읽어 세션을 만든다. 배터리 세이브는 ROM 옆 `.sav`에서 읽고 쓴다.
/// 실패하면 상태 표시줄에 보일 문구를 돌려준다.
pub fn load_session(path: &Path, now_unix: u64) -> Result<Session, String> {
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let rom = std::fs::read(path).map_err(|e| format!("{name}을(를) 읽을 수 없습니다: {e}"))?;
    let store = Box::new(FsSaveStore::for_rom(path));
    Session::load(rom, store, now_unix).map_err(|e| format!("{name}을(를) 열 수 없습니다: {e}"))
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

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
}

impl AragornApp {
    pub fn new(ctx: &egui::Context, deps: AppDeps) -> Self {
        let config = deps.config_store.load();
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
        self.flush_session();
        match load_session(path, now_unix()) {
            Ok(session) => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
                    "Aragorn v{} - {}",
                    self.flow.current(),
                    session.title()
                )));
                self.screen.update(ctx, session.framebuffer());
                self.notice = Some(format!("{} 실행 중", session.title()));
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

    /// 경과 시간만큼 에뮬레이션을 진행하고 화면을 갱신한다. 강제 업데이트 중에는 멈춘다.
    fn run_emulation(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        let elapsed = now - self.last_tick;
        self.last_tick = now;
        let Some(session) = &mut self.session else {
            return;
        };
        if self.flow.blocks_emulation() {
            return;
        }
        for (button, pressed) in input::button_states(|key| ctx.input(|i| i.key_down(key))) {
            session.set_button(button, pressed);
        }
        if session.advance(elapsed) > 0 {
            self.screen.update(ctx, session.framebuffer());
        }
        self.collect_session_errors();
        ctx.request_repaint();
    }

    /// 저장하지 않은 배터리 세이브를 디스크에 쓴다.
    fn flush_session(&mut self) {
        if let Some(session) = &mut self.session {
            session.flush();
        }
        self.collect_session_errors();
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
            self.exit_handled = true;
            self.flush_session();
            events.push(UpdateEvent::AppExiting);
        }
        for event in events {
            self.dispatch(event);
        }
        let dropped = ctx.input(|i| i.raw.dropped_files.first().map(|f| f.path().to_path_buf()));
        if let Some(path) = dropped {
            self.open_rom(ctx, &path);
        }
        self.run_emulation(ctx);
        // 6시간 재확인 타이머가 유휴 상태에서도 돌도록 주기적으로 깨운다.
        ctx.request_repaint_after(Duration::from_secs(60));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let mut events = Vec::new();

        let mut open_requested = false;
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("ROM 열기").clicked() {
                    open_requested = true;
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

    #[test]
    fn missing_rom_file_reports_read_error() {
        let err = load_session(Path::new("/없는/경로/pokemon.gb"), 0)
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
        let err = load_session(&path, 0).err().unwrap();
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

        let mut session = load_session(&path, 0).unwrap();
        session.advance(aragorn_app::pacing::FRAME_DURATION);
        let save = std::fs::read(dir.path().join("game.sav")).unwrap();
        assert_eq!((save.len(), save[0]), (0x2000, 0x42));

        let reloaded = load_session(&path, 0).unwrap();
        assert_eq!(reloaded.battery_ram().unwrap()[0], 0x42);
    }
}
