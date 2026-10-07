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

/// 강제 업데이트 모달에 보여줄 선택지. 어떤 상태에서도 "종료"로 빠져나갈 수 있어야 한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForcedOption {
    UpdateNow,
    Retry,
    OpenDownloadPage,
    Quit,
}

pub fn forced_options(
    state: &UpdateState,
    updater_available: bool,
    has_release_page: bool,
) -> Vec<ForcedOption> {
    use ForcedOption as O;
    let mut options = match state {
        UpdateState::Forced { .. } if updater_available => vec![O::UpdateNow],
        UpdateState::Forced { .. } => vec![O::OpenDownloadPage],
        UpdateState::Downloading { forced: true, .. } => vec![],
        UpdateState::Failed { forced: true, .. } => vec![O::Retry, O::OpenDownloadPage],
        _ => return vec![],
    };
    if !has_release_page {
        options.retain(|o| *o != O::OpenDownloadPage);
    }
    options.push(O::Quit);
    options
}

/// 강제 업데이트는 모달, 소프트 업데이트는 상단 배너로 보여준다.
pub fn show(
    ui: &mut egui::Ui,
    flow: &UpdateFlow,
    release_page: Option<&str>,
) -> Vec<UpdateUiAction> {
    let mut actions = Vec::new();
    match flow.state() {
        UpdateState::Forced { .. }
        | UpdateState::Downloading { forced: true, .. }
        | UpdateState::Failed { forced: true, .. } => forced_modal(ui, |ui| {
            forced_message(ui, flow);
            ui.add_space(8.0);
            let options = forced_options(
                flow.state(),
                flow.updater_available(),
                release_page.is_some(),
            );
            ui.horizontal(|ui| {
                for option in options {
                    match option {
                        ForcedOption::UpdateNow => {
                            if ui.button("업데이트").clicked() {
                                actions.push(UpdateUiAction::Event(UpdateEvent::UpdateNow));
                            }
                        }
                        ForcedOption::Retry => {
                            if ui.button("다시 시도").clicked() {
                                actions.push(UpdateUiAction::Event(UpdateEvent::UpdateNow));
                            }
                        }
                        ForcedOption::OpenDownloadPage => {
                            if let Some(url) = release_page {
                                ui.hyperlink_to("다운로드 페이지 열기", url);
                            }
                        }
                        ForcedOption::Quit => {
                            if ui.button("종료").clicked() {
                                actions.push(UpdateUiAction::Quit);
                            }
                        }
                    }
                }
            });
        }),
        UpdateState::ReadyToRestart { forced: true, .. } => forced_modal(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("업데이트를 적용하고 다시 시작합니다…");
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

fn forced_message(ui: &mut egui::Ui, flow: &UpdateFlow) {
    match flow.state() {
        UpdateState::Forced { version, message } => {
            ui.heading("필수 업데이트");
            ui.label(format!(
                "현재 버전 {}은(는) 더 이상 지원되지 않습니다. {version}(으)로 업데이트해야 계속할 수 있습니다.",
                flow.current()
            ));
            if !message.is_empty() {
                ui.label(message);
            }
            if !flow.updater_available() {
                ui.label("설치판이 아니어서 자동으로 업데이트할 수 없습니다. 새 버전을 직접 내려받아 주세요.");
            }
        }
        UpdateState::Downloading { version, .. } => {
            ui.heading("필수 업데이트");
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(format!("{version} 다운로드 중…"));
            });
        }
        UpdateState::Failed { error, .. } => {
            ui.heading("업데이트 실패");
            ui.label(error.to_string());
        }
        _ => {}
    }
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

    use ForcedOption as O;

    fn v() -> Version {
        Version::new(1, 2, 0)
    }

    fn forced() -> UpdateState {
        UpdateState::Forced {
            version: v(),
            message: String::new(),
        }
    }

    #[test]
    fn forced_with_updater_offers_update_and_quit() {
        assert_eq!(
            forced_options(&forced(), true, true),
            vec![O::UpdateNow, O::Quit]
        );
    }

    #[test]
    fn forced_without_updater_offers_download_page_and_quit() {
        assert_eq!(
            forced_options(&forced(), false, true),
            vec![O::OpenDownloadPage, O::Quit]
        );
        assert_eq!(forced_options(&forced(), false, false), vec![O::Quit]);
    }

    #[test]
    fn stalled_forced_download_can_still_quit() {
        let state = UpdateState::Downloading {
            version: v(),
            forced: true,
        };
        assert_eq!(forced_options(&state, true, true), vec![O::Quit]);
    }

    #[test]
    fn forced_failure_offers_retry_download_page_and_quit() {
        let state = UpdateState::Failed {
            version: v(),
            forced: true,
            error: UpdateError::Download("x".into()),
        };
        assert_eq!(
            forced_options(&state, true, true),
            vec![O::Retry, O::OpenDownloadPage, O::Quit]
        );
    }

    #[test]
    fn non_forced_states_have_no_modal_options() {
        assert!(forced_options(&UpdateState::Idle, true, true).is_empty());
        let soft = UpdateState::Downloading {
            version: v(),
            forced: false,
        };
        assert!(forced_options(&soft, true, true).is_empty());
    }

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
