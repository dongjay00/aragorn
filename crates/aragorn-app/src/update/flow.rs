use super::{PackageSpec, UpdateDecision, UpdateError, UpdatePolicy, UpdatePrefs, decide};
use semver::Version;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateState {
    Idle,
    Checking,
    UpToDate,
    CheckFailed {
        error: UpdateError,
    },
    SoftAvailable {
        version: Version,
        message: String,
    },
    Forced {
        version: Version,
        message: String,
    },
    Downloading {
        version: Version,
        forced: bool,
    },
    ReadyToRestart {
        version: Version,
        forced: bool,
    },
    Failed {
        version: Version,
        forced: bool,
        error: UpdateError,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateEvent {
    /// 앱 시작, 6시간 주기, 사용자 "업데이트 확인"
    CheckRequested,
    PolicyFetched(Result<UpdatePolicy, UpdateError>),
    /// "지금 업데이트", 강제 모달의 "업데이트", "다시 시도"
    UpdateNow,
    Later,
    SkipVersion,
    SetAutoDownload(bool),
    DownloadFinished(Result<(), UpdateError>),
    RestartNow,
    ApplyFailed(UpdateError),
    AppExiting,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateCommand {
    FetchPolicy,
    Download(PackageSpec),
    ApplyAndRestart,
    ApplyOnExit,
    SavePrefs(UpdatePrefs),
}

/// 업데이트 흐름의 순수 상태 머신. 입출력은 하지 않고, 실행할 명령만 돌려준다.
pub struct UpdateFlow {
    current: Version,
    prefs: UpdatePrefs,
    updater_available: bool,
    state: UpdateState,
    /// 마지막으로 서명 검증된 정책이 지정한 패키지 (재시도에도 쓴다)
    target: Option<PackageSpec>,
    /// 재시작을 요청해 업데이트 적용을 기다리는 중. 직전에 저장한 세이브 뒤로 게임이 진행되지 않게 멈춘다.
    restarting: bool,
}

impl UpdateFlow {
    pub fn new(current: Version, prefs: UpdatePrefs, updater_available: bool) -> Self {
        Self {
            current,
            prefs,
            updater_available,
            state: UpdateState::Idle,
            target: None,
            restarting: false,
        }
    }

    pub fn state(&self) -> &UpdateState {
        &self.state
    }

    pub fn prefs(&self) -> &UpdatePrefs {
        &self.prefs
    }

    pub fn current(&self) -> &Version {
        &self.current
    }

    pub fn updater_available(&self) -> bool {
        self.updater_available
    }

    /// 강제 업데이트가 끝나기 전까지 게임을 진행하면 안 되는 상태인가.
    pub fn blocks_emulation(&self) -> bool {
        self.restarting
            || matches!(
                self.state,
                UpdateState::Forced { .. }
                    | UpdateState::Downloading { forced: true, .. }
                    | UpdateState::ReadyToRestart { forced: true, .. }
                    | UpdateState::Failed { forced: true, .. }
            )
    }

    pub fn handle(&mut self, event: UpdateEvent) -> Vec<UpdateCommand> {
        use UpdateState as S;
        match event {
            UpdateEvent::CheckRequested => {
                let busy = matches!(
                    self.state,
                    S::Checking | S::Downloading { .. } | S::ReadyToRestart { .. }
                );
                if busy || self.blocks_emulation() {
                    return vec![];
                }
                self.state = S::Checking;
                vec![UpdateCommand::FetchPolicy]
            }
            UpdateEvent::PolicyFetched(result) => {
                if self.state != S::Checking {
                    return vec![];
                }
                match result {
                    Ok(policy) => self.apply_decision(&policy),
                    Err(error) => {
                        self.state = S::CheckFailed { error };
                        vec![]
                    }
                }
            }
            UpdateEvent::UpdateNow => match self.state.clone() {
                S::SoftAvailable { version, .. } => self.start_download(version, false),
                S::Forced { version, .. } => self.start_download(version, true),
                S::Failed {
                    version, forced, ..
                } => self.start_download(version, forced),
                _ => vec![],
            },
            UpdateEvent::Later => {
                if matches!(
                    self.state,
                    S::SoftAvailable { .. } | S::Failed { forced: false, .. }
                ) {
                    self.state = S::Idle;
                }
                vec![]
            }
            UpdateEvent::SkipVersion => {
                let S::SoftAvailable { version, .. } = &self.state else {
                    return vec![];
                };
                self.prefs.skipped_version = Some(version.clone());
                self.state = S::Idle;
                vec![UpdateCommand::SavePrefs(self.prefs.clone())]
            }
            UpdateEvent::SetAutoDownload(on) => {
                self.prefs.auto_download = on;
                vec![UpdateCommand::SavePrefs(self.prefs.clone())]
            }
            UpdateEvent::DownloadFinished(result) => {
                let S::Downloading { version, forced } = self.state.clone() else {
                    return vec![];
                };
                match result {
                    Ok(()) => {
                        self.state = S::ReadyToRestart { version, forced };
                        if forced {
                            vec![UpdateCommand::ApplyAndRestart]
                        } else {
                            vec![]
                        }
                    }
                    Err(error) => {
                        self.state = S::Failed {
                            version,
                            forced,
                            error,
                        };
                        vec![]
                    }
                }
            }
            UpdateEvent::RestartNow => {
                if matches!(self.state, S::ReadyToRestart { .. }) {
                    self.restarting = true;
                    vec![UpdateCommand::ApplyAndRestart]
                } else {
                    vec![]
                }
            }
            UpdateEvent::ApplyFailed(error) => {
                self.restarting = false;
                let S::ReadyToRestart { version, forced } = self.state.clone() else {
                    return vec![];
                };
                self.state = S::Failed {
                    version,
                    forced,
                    error,
                };
                vec![]
            }
            UpdateEvent::AppExiting => {
                if matches!(self.state, S::ReadyToRestart { forced: false, .. }) {
                    vec![UpdateCommand::ApplyOnExit]
                } else {
                    vec![]
                }
            }
        }
    }

    fn apply_decision(&mut self, policy: &UpdatePolicy) -> Vec<UpdateCommand> {
        let decision = decide(&self.current, policy, &self.prefs);
        if let UpdateDecision::Soft { version, .. } | UpdateDecision::Forced { version } = &decision
        {
            self.target = Some(PackageSpec {
                version: version.clone(),
                sha256_by_channel: policy.packages.clone(),
            });
        }
        match decision {
            UpdateDecision::UpToDate => {
                self.state = UpdateState::UpToDate;
                vec![]
            }
            UpdateDecision::Soft {
                version,
                auto_download,
            } => {
                if auto_download && self.updater_available {
                    self.start_download(version, false)
                } else {
                    self.state = UpdateState::SoftAvailable {
                        version,
                        message: policy.message.clone(),
                    };
                    vec![]
                }
            }
            UpdateDecision::Forced { version } => {
                self.state = UpdateState::Forced {
                    version,
                    message: policy.message.clone(),
                };
                vec![]
            }
        }
    }

    fn start_download(&mut self, version: Version, forced: bool) -> Vec<UpdateCommand> {
        let Some(target) = self.target.clone().filter(|t| t.version == version) else {
            return vec![];
        };
        if !self.updater_available {
            return vec![];
        }
        self.state = UpdateState::Downloading { version, forced };
        vec![UpdateCommand::Download(target)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::update::PackageHashes;
    use UpdateCommand as C;
    use UpdateEvent as E;
    use UpdateState as S;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    fn hashes() -> PackageHashes {
        PackageHashes::from([("win".to_string(), "ab".repeat(32))])
    }

    fn policy(latest: &str, min: &str) -> UpdatePolicy {
        UpdatePolicy {
            latest: v(latest),
            minimum_supported: v(min),
            message: "msg".into(),
            packages: hashes(),
        }
    }

    fn spec(version: &str) -> PackageSpec {
        PackageSpec {
            version: v(version),
            sha256_by_channel: hashes(),
        }
    }

    fn flow(auto_download: bool, updater_available: bool) -> UpdateFlow {
        UpdateFlow::new(
            v("1.0.0"),
            UpdatePrefs {
                auto_download,
                skipped_version: None,
            },
            updater_available,
        )
    }

    fn checked(f: &mut UpdateFlow, latest: &str, min: &str) -> Vec<UpdateCommand> {
        assert_eq!(f.handle(E::CheckRequested), vec![C::FetchPolicy]);
        f.handle(E::PolicyFetched(Ok(policy(latest, min))))
    }

    #[test]
    fn check_requested_starts_fetch() {
        let mut f = flow(true, true);
        assert_eq!(f.handle(E::CheckRequested), vec![C::FetchPolicy]);
        assert_eq!(f.state(), &S::Checking);
    }

    #[test]
    fn up_to_date_policy() {
        let mut f = flow(true, true);
        assert_eq!(checked(&mut f, "1.0.0", "1.0.0"), vec![]);
        assert_eq!(f.state(), &S::UpToDate);
    }

    #[test]
    fn soft_without_auto_download_shows_banner() {
        let mut f = flow(false, true);
        assert_eq!(checked(&mut f, "1.1.0", "1.0.0"), vec![]);
        assert_eq!(
            f.state(),
            &S::SoftAvailable {
                version: v("1.1.0"),
                message: "msg".into()
            }
        );
        assert!(!f.blocks_emulation());
    }

    #[test]
    fn soft_with_auto_download_starts_download() {
        let mut f = flow(true, true);
        assert_eq!(
            checked(&mut f, "1.1.0", "1.0.0"),
            vec![C::Download(spec("1.1.0"))]
        );
        assert_eq!(
            f.state(),
            &S::Downloading {
                version: v("1.1.0"),
                forced: false
            }
        );
    }

    #[test]
    fn soft_with_auto_download_but_no_updater_shows_banner() {
        let mut f = flow(true, false);
        assert_eq!(checked(&mut f, "1.1.0", "1.0.0"), vec![]);
        assert!(matches!(f.state(), S::SoftAvailable { .. }));
    }

    #[test]
    fn forced_waits_for_user_then_downloads_and_restarts() {
        let mut f = flow(true, true);
        assert_eq!(checked(&mut f, "1.2.0", "1.1.0"), vec![]);
        assert_eq!(
            f.state(),
            &S::Forced {
                version: v("1.2.0"),
                message: "msg".into()
            }
        );
        assert!(f.blocks_emulation());

        assert_eq!(f.handle(E::UpdateNow), vec![C::Download(spec("1.2.0"))]);
        assert!(f.blocks_emulation());

        assert_eq!(
            f.handle(E::DownloadFinished(Ok(()))),
            vec![C::ApplyAndRestart]
        );
        assert_eq!(
            f.state(),
            &S::ReadyToRestart {
                version: v("1.2.0"),
                forced: true
            }
        );
    }

    #[test]
    fn forced_download_failure_keeps_blocking_and_allows_retry() {
        let mut f = flow(true, true);
        checked(&mut f, "1.2.0", "1.1.0");
        f.handle(E::UpdateNow);
        let err = UpdateError::Download("boom".into());
        assert_eq!(f.handle(E::DownloadFinished(Err(err.clone()))), vec![]);
        assert_eq!(
            f.state(),
            &S::Failed {
                version: v("1.2.0"),
                forced: true,
                error: err
            }
        );
        assert!(f.blocks_emulation());

        assert_eq!(f.handle(E::Later), vec![]);
        assert!(
            f.blocks_emulation(),
            "강제 업데이트 실패는 '나중에'로 넘길 수 없다"
        );

        assert_eq!(f.handle(E::UpdateNow), vec![C::Download(spec("1.2.0"))]);
    }

    #[test]
    fn forced_without_updater_ignores_update_now() {
        let mut f = flow(true, false);
        checked(&mut f, "1.2.0", "1.1.0");
        assert_eq!(f.handle(E::UpdateNow), vec![]);
        assert!(matches!(f.state(), S::Forced { .. }));
        assert!(f.blocks_emulation());
    }

    #[test]
    fn check_failure_does_not_block() {
        let mut f = flow(true, true);
        f.handle(E::CheckRequested);
        let err = UpdateError::Network("offline".into());
        assert_eq!(f.handle(E::PolicyFetched(Err(err.clone()))), vec![]);
        assert_eq!(f.state(), &S::CheckFailed { error: err });
        assert!(!f.blocks_emulation());
    }

    #[test]
    fn skip_version_saves_prefs() {
        let mut f = flow(false, true);
        checked(&mut f, "1.1.0", "1.0.0");
        let expected = UpdatePrefs {
            auto_download: false,
            skipped_version: Some(v("1.1.0")),
        };
        assert_eq!(
            f.handle(E::SkipVersion),
            vec![C::SavePrefs(expected.clone())]
        );
        assert_eq!(f.prefs(), &expected);
        assert_eq!(f.state(), &S::Idle);
    }

    #[test]
    fn later_hides_banner() {
        let mut f = flow(false, true);
        checked(&mut f, "1.1.0", "1.0.0");
        assert_eq!(f.handle(E::Later), vec![]);
        assert_eq!(f.state(), &S::Idle);
    }

    #[test]
    fn soft_download_ready_applies_on_exit() {
        let mut f = flow(true, true);
        checked(&mut f, "1.1.0", "1.0.0");
        assert_eq!(f.handle(E::DownloadFinished(Ok(()))), vec![]);
        assert_eq!(
            f.state(),
            &S::ReadyToRestart {
                version: v("1.1.0"),
                forced: false
            }
        );
        assert_eq!(f.handle(E::AppExiting), vec![C::ApplyOnExit]);
    }

    #[test]
    fn restart_now_applies_ready_update() {
        let mut f = flow(true, true);
        checked(&mut f, "1.1.0", "1.0.0");
        f.handle(E::DownloadFinished(Ok(())));
        assert_eq!(f.handle(E::RestartNow), vec![C::ApplyAndRestart]);
    }

    #[test]
    fn restarting_blocks_emulation_until_apply_fails() {
        let mut f = flow(true, true);
        checked(&mut f, "1.1.0", "1.0.0");
        f.handle(E::UpdateNow);
        f.handle(E::DownloadFinished(Ok(())));
        assert!(!f.blocks_emulation());
        f.handle(E::RestartNow);
        assert!(
            f.blocks_emulation(),
            "재시작 직전 저장한 세이브 뒤로 게임이 진행되면 안 된다"
        );
        f.handle(E::ApplyFailed(UpdateError::Apply("x".into())));
        assert!(!f.blocks_emulation());
    }

    #[test]
    fn apply_failure_moves_to_failed() {
        let mut f = flow(true, true);
        checked(&mut f, "1.1.0", "1.0.0");
        f.handle(E::DownloadFinished(Ok(())));
        let err = UpdateError::Apply("locked".into());
        assert_eq!(f.handle(E::ApplyFailed(err.clone())), vec![]);
        assert_eq!(
            f.state(),
            &S::Failed {
                version: v("1.1.0"),
                forced: false,
                error: err
            }
        );
    }

    #[test]
    fn exiting_without_ready_update_does_nothing() {
        let mut f = flow(true, true);
        assert_eq!(f.handle(E::AppExiting), vec![]);
    }

    #[test]
    fn check_ignored_while_downloading() {
        let mut f = flow(true, true);
        checked(&mut f, "1.1.0", "1.0.0");
        assert_eq!(f.handle(E::CheckRequested), vec![]);
        assert!(matches!(f.state(), S::Downloading { .. }));
    }

    #[test]
    fn stale_policy_result_is_ignored() {
        let mut f = flow(true, true);
        assert_eq!(
            f.handle(E::PolicyFetched(Ok(policy("1.1.0", "1.0.0")))),
            vec![]
        );
        assert_eq!(f.state(), &S::Idle);
    }

    #[test]
    fn set_auto_download_saves_prefs() {
        let mut f = flow(true, true);
        let expected = UpdatePrefs {
            auto_download: false,
            skipped_version: None,
        };
        assert_eq!(
            f.handle(E::SetAutoDownload(false)),
            vec![C::SavePrefs(expected)]
        );
        assert!(!f.prefs().auto_download);
    }
}
