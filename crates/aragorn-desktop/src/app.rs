use crate::{
    adapters::UnavailableUpdater,
    build_info,
    ui::update_view::{self, UpdateUiAction},
    update_worker::UpdateWorker,
};
use aragorn_app::{
    config::{Config, ConfigStore, UpdateConfig},
    update::{UpdateCommand, UpdateEvent, UpdateFlow, UpdateSource, Updater},
};
use eframe::egui;
use std::{
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
        };
        if app.worker.is_some() {
            app.dispatch(UpdateEvent::CheckRequested);
        }
        app
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

    /// 업데이트를 적용하기 전에 디스크에 남겨야 하는 상태를 저장한다.
    /// 배터리 세이브는 마일스톤 4에서 여기에 추가한다.
    fn flush_persistent_state(&self) {
        self.save_config();
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
            events.push(UpdateEvent::AppExiting);
        }
        for event in events {
            self.dispatch(event);
        }
        // 6시간 재확인 타이머가 유휴 상태에서도 돌도록 주기적으로 깨운다.
        ctx.request_repaint_after(Duration::from_secs(60));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let mut events = Vec::new();

        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal(|ui| {
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
            ui.label(update_view::status_text(
                self.flow.state(),
                self.worker.is_some(),
            ));
        });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(120.0);
                ui.heading(format!("Aragorn v{}", self.flow.current()));
                ui.label("ROM 실행은 다음 마일스톤에서 지원됩니다.");
            });
        });

        for event in events {
            self.dispatch(event);
        }
    }
}
