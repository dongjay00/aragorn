use super::{UpdateCommand, UpdateError, UpdateEvent, UpdatePolicy};
use semver::Version;

/// 서명 검증까지 끝낸 업데이트 정책을 가져온다.
pub trait UpdateSource: Send + Sync {
    fn fetch_policy(&self) -> Result<UpdatePolicy, UpdateError>;
}

/// 업데이트 패키지를 내려받고 설치한다.
pub trait Updater: Send + Sync {
    fn download(&self, version: &Version) -> Result<(), UpdateError>;
    /// 성공하면 프로세스가 재시작되므로 반환하지 않을 수 있다.
    fn apply_and_restart(&self) -> Result<(), UpdateError>;
    /// 앱이 종료된 뒤 업데이트가 적용되도록 예약한다.
    fn apply_on_exit(&self) -> Result<(), UpdateError>;
}

/// 입출력 명령을 실행하고, 상태 머신에 돌려줄 이벤트를 만든다.
/// `SavePrefs`는 설정 저장소를 가진 호출자가 처리한다.
pub fn execute(
    command: &UpdateCommand,
    source: &dyn UpdateSource,
    updater: &dyn Updater,
) -> Option<UpdateEvent> {
    match command {
        UpdateCommand::FetchPolicy => Some(UpdateEvent::PolicyFetched(source.fetch_policy())),
        UpdateCommand::Download(version) => {
            Some(UpdateEvent::DownloadFinished(updater.download(version)))
        }
        UpdateCommand::ApplyAndRestart => updater
            .apply_and_restart()
            .err()
            .map(UpdateEvent::ApplyFailed),
        UpdateCommand::ApplyOnExit => updater.apply_on_exit().err().map(UpdateEvent::ApplyFailed),
        UpdateCommand::SavePrefs(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct FakeSource(Result<UpdatePolicy, UpdateError>);

    impl UpdateSource for FakeSource {
        fn fetch_policy(&self) -> Result<UpdatePolicy, UpdateError> {
            self.0.clone()
        }
    }

    struct FakeUpdater {
        result: Result<(), UpdateError>,
        calls: Mutex<Vec<String>>,
    }

    impl FakeUpdater {
        fn new(result: Result<(), UpdateError>) -> Self {
            Self {
                result,
                calls: Mutex::new(vec![]),
            }
        }
        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl Updater for FakeUpdater {
        fn download(&self, version: &Version) -> Result<(), UpdateError> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("download {version}"));
            self.result.clone()
        }
        fn apply_and_restart(&self) -> Result<(), UpdateError> {
            self.calls.lock().unwrap().push("apply_and_restart".into());
            self.result.clone()
        }
        fn apply_on_exit(&self) -> Result<(), UpdateError> {
            self.calls.lock().unwrap().push("apply_on_exit".into());
            self.result.clone()
        }
    }

    fn policy() -> UpdatePolicy {
        UpdatePolicy {
            latest: Version::new(1, 1, 0),
            minimum_supported: Version::new(1, 0, 0),
            message: String::new(),
        }
    }

    #[test]
    fn fetch_policy_returns_policy_fetched() {
        let source = FakeSource(Ok(policy()));
        let updater = FakeUpdater::new(Ok(()));
        let ev = execute(&UpdateCommand::FetchPolicy, &source, &updater);
        assert_eq!(ev, Some(UpdateEvent::PolicyFetched(Ok(policy()))));
    }

    #[test]
    fn download_reports_result() {
        let source = FakeSource(Ok(policy()));
        let updater = FakeUpdater::new(Err(UpdateError::Download("x".into())));
        let ev = execute(
            &UpdateCommand::Download(Version::new(1, 1, 0)),
            &source,
            &updater,
        );
        assert_eq!(
            ev,
            Some(UpdateEvent::DownloadFinished(Err(UpdateError::Download(
                "x".into()
            ))))
        );
        assert_eq!(updater.calls(), vec!["download 1.1.0"]);
    }

    #[test]
    fn successful_apply_produces_no_event() {
        let source = FakeSource(Ok(policy()));
        let updater = FakeUpdater::new(Ok(()));
        assert_eq!(
            execute(&UpdateCommand::ApplyOnExit, &source, &updater),
            None
        );
        assert_eq!(updater.calls(), vec!["apply_on_exit"]);
    }

    #[test]
    fn failed_apply_produces_apply_failed() {
        let source = FakeSource(Ok(policy()));
        let updater = FakeUpdater::new(Err(UpdateError::Apply("locked".into())));
        let ev = execute(&UpdateCommand::ApplyAndRestart, &source, &updater);
        assert_eq!(
            ev,
            Some(UpdateEvent::ApplyFailed(UpdateError::Apply(
                "locked".into()
            )))
        );
    }

    #[test]
    fn save_prefs_is_left_to_caller() {
        let source = FakeSource(Ok(policy()));
        let updater = FakeUpdater::new(Ok(()));
        let cmd = UpdateCommand::SavePrefs(Default::default());
        assert_eq!(execute(&cmd, &source, &updater), None);
        assert!(updater.calls().is_empty());
    }
}
