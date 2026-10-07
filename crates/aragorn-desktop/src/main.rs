// 릴리스 빌드에서는 Windows가 앱과 함께 콘솔 창을 띄우지 않게 한다. 개발 빌드는 로그를 보려고 콘솔을 유지한다.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use aragorn_app::update::{UpdateSource, Updater};
use aragorn_desktop::{
    adapters::{GithubPolicySource, TomlConfigStore, VelopackUpdater},
    app::{AppDeps, AragornApp},
    build_info, ui,
};
use eframe::egui;
use std::{path::PathBuf, sync::Arc, time::Duration};

const POLICY_TIMEOUT: Duration = Duration::from_secs(5);

fn main() -> eframe::Result {
    // Velopack 설치/제거 훅 처리. 반드시 가장 먼저 호출한다.
    velopack::VelopackApp::build().run();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let config_path =
        TomlConfigStore::default_path().unwrap_or_else(|| PathBuf::from("config.toml"));
    let (source, updater, release_page) = match build_info::GITHUB_REPO {
        Some(repo) => {
            let source: Arc<dyn UpdateSource> = Arc::new(GithubPolicySource::new(
                build_info::policy_base_url(repo),
                *build_info::POLICY_PUBLIC_KEY,
                POLICY_TIMEOUT,
            ));
            let updater = VelopackUpdater::new(&build_info::repo_url(repo))
                .map(|u| Arc::new(u) as Arc<dyn Updater>);
            (
                Some(source),
                updater,
                Some(build_info::releases_page_url(repo)),
            )
        }
        None => (None, None, None),
    };

    // `aragorn <ROM 경로>`로 실행하면 바로 그 ROM을 연다.
    let initial_rom = std::env::args_os().nth(1).map(PathBuf::from);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(format!("Aragorn v{}", build_info::current_version()))
            .with_inner_size([640.0, 576.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Aragorn",
        options,
        Box::new(move |cc| {
            ui::fonts::install(&cc.egui_ctx);
            let deps = AppDeps {
                config_store: Box::new(TomlConfigStore::new(config_path)),
                source,
                updater,
                release_page,
                initial_rom,
            };
            Ok(Box::new(AragornApp::new(&cc.egui_ctx, deps)))
        }),
    )
}
