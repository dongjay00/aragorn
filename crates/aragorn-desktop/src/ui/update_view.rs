use aragorn_app::update::{UpdateEvent, UpdateFlow, UpdateState};
use eframe::egui;

pub enum UpdateUiAction {
    Event(UpdateEvent),
    Quit,
}

pub fn status_text(state: &UpdateState, updates_enabled: bool) -> String {
    if !updates_enabled {
        return "개발 빌드: 업데이트 확인 안 함".into();
    }
    match state {
        UpdateState::Idle => String::new(),
        UpdateState::Checking => "업데이트 확인 중…".into(),
        UpdateState::UpToDate => "최신 버전입니다".into(),
        UpdateState::CheckFailed { error } => format!("업데이트 확인 실패: {error}"),
        UpdateState::SoftAvailable { version, .. } => format!("새 버전 {version} 사용 가능"),
        UpdateState::Forced { version, .. } => format!("필수 업데이트: {version}"),
        UpdateState::Downloading { version, .. } => format!("다운로드 중: {version}"),
        UpdateState::ReadyToRestart { version, .. } => format!("업데이트 준비 완료: {version}"),
        UpdateState::Failed { error, .. } => format!("업데이트 실패: {error}"),
    }
}

/// 강제 업데이트는 모달, 소프트 업데이트는 상단 배너로 보여준다.
pub fn show(
    ui: &mut egui::Ui,
    flow: &UpdateFlow,
    release_page: Option<&str>,
) -> Vec<UpdateUiAction> {
    let mut actions = Vec::new();
    match flow.state() {
        UpdateState::Forced { version, message } => forced_modal(ui, |ui| {
            ui.heading("필수 업데이트");
            ui.label(format!(
                "현재 버전 {}은(는) 더 이상 지원되지 않습니다. {version}(으)로 업데이트해야 계속할 수 있습니다.",
                flow.current()
            ));
            if !message.is_empty() {
                ui.label(message);
            }
            ui.add_space(8.0);
            if flow.updater_available() {
                if ui.button("업데이트").clicked() {
                    actions.push(UpdateUiAction::Event(UpdateEvent::UpdateNow));
                }
            } else {
                ui.label("설치판이 아니어서 자동으로 업데이트할 수 없습니다. 새 버전을 직접 내려받아 주세요.");
                if let Some(url) = release_page {
                    ui.hyperlink_to("다운로드 페이지 열기", url);
                }
            }
            if ui.button("종료").clicked() {
                actions.push(UpdateUiAction::Quit);
            }
        }),
        UpdateState::Downloading {
            version,
            forced: true,
        } => forced_modal(ui, |ui| {
            ui.heading("필수 업데이트");
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(format!("{version} 다운로드 중…"));
            });
        }),
        UpdateState::ReadyToRestart { forced: true, .. } => forced_modal(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("업데이트를 적용하고 다시 시작합니다…");
            });
        }),
        UpdateState::Failed {
            error,
            forced: true,
            ..
        } => forced_modal(ui, |ui| {
            ui.heading("업데이트 실패");
            ui.label(error.to_string());
            ui.horizontal(|ui| {
                if ui.button("다시 시도").clicked() {
                    actions.push(UpdateUiAction::Event(UpdateEvent::UpdateNow));
                }
                if ui.button("종료").clicked() {
                    actions.push(UpdateUiAction::Quit);
                }
            });
        }),
        UpdateState::SoftAvailable { version, message } => banner(ui, |ui| {
            ui.label(format!("새 버전 {version}이(가) 있습니다."));
            if !message.is_empty() {
                ui.label(message);
            }
            if flow.updater_available() {
                if ui.button("지금 업데이트").clicked() {
                    actions.push(UpdateUiAction::Event(UpdateEvent::UpdateNow));
                }
            } else if let Some(url) = release_page {
                ui.hyperlink_to("다운로드 페이지", url);
            }
            if ui.button("나중에").clicked() {
                actions.push(UpdateUiAction::Event(UpdateEvent::Later));
            }
            if ui.button("이 버전 건너뛰기").clicked() {
                actions.push(UpdateUiAction::Event(UpdateEvent::SkipVersion));
            }
        }),
        UpdateState::Downloading {
            version,
            forced: false,
        } => banner(ui, |ui| {
            ui.spinner();
            ui.label(format!("업데이트 {version} 다운로드 중…"));
        }),
        UpdateState::ReadyToRestart {
            version,
            forced: false,
        } => banner(ui, |ui| {
            ui.label(format!(
                "업데이트 {version} 준비 완료. 앱을 종료하면 적용됩니다."
            ));
            if ui.button("재시작하여 업데이트").clicked() {
                actions.push(UpdateUiAction::Event(UpdateEvent::RestartNow));
            }
        }),
        UpdateState::Failed {
            error,
            forced: false,
            ..
        } => banner(ui, |ui| {
            ui.label(format!("업데이트 실패: {error}"));
            if ui.button("다시 시도").clicked() {
                actions.push(UpdateUiAction::Event(UpdateEvent::UpdateNow));
            }
            if ui.button("나중에").clicked() {
                actions.push(UpdateUiAction::Event(UpdateEvent::Later));
            }
        }),
        UpdateState::Idle
        | UpdateState::Checking
        | UpdateState::UpToDate
        | UpdateState::CheckFailed { .. } => {}
    }
    actions
}

fn forced_modal(ui: &egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Modal::new(egui::Id::new("forced-update")).show(ui.ctx(), |ui| {
        ui.set_max_width(360.0);
        add_contents(ui);
    });
}

fn banner(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Panel::top("update-banner").show(ui, |ui| {
        ui.horizontal_wrapped(add_contents);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use aragorn_app::update::UpdateError;
    use semver::Version;

    #[test]
    fn status_text_for_disabled_updates() {
        assert_eq!(
            status_text(&UpdateState::Idle, false),
            "개발 빌드: 업데이트 확인 안 함"
        );
    }

    #[test]
    fn status_text_for_common_states() {
        assert_eq!(
            status_text(&UpdateState::Checking, true),
            "업데이트 확인 중…"
        );
        assert_eq!(status_text(&UpdateState::UpToDate, true), "최신 버전입니다");
        assert_eq!(
            status_text(
                &UpdateState::CheckFailed {
                    error: UpdateError::Network("x".into())
                },
                true
            ),
            "업데이트 확인 실패: 네트워크 오류: x"
        );
        assert_eq!(
            status_text(
                &UpdateState::Downloading {
                    version: Version::new(1, 1, 0),
                    forced: false
                },
                true
            ),
            "다운로드 중: 1.1.0"
        );
    }
}
