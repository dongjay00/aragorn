use aragorn_app::update::{UpdateError, Updater};
use semver::Version;
use std::sync::Mutex;
use velopack::{UpdateCheck, UpdateInfo, UpdateManager, sources::GithubSource};

/// Velopack으로 설치된 앱에서만 동작하는 업데이터.
pub struct VelopackUpdater {
    manager: UpdateManager,
    pending: Mutex<Option<UpdateInfo>>,
}

impl VelopackUpdater {
    /// Velopack 설치판이 아니면(`cargo run`, 압축 해제 실행 등) `None`.
    pub fn new(repo_url: &str) -> Option<Self> {
        let source = GithubSource::new(repo_url, None, false);
        match UpdateManager::new(source, None, None) {
            Ok(manager) => Some(Self {
                manager,
                pending: Mutex::new(None),
            }),
            Err(e) => {
                log::info!("자동 업데이트 비활성화 (설치판 아님): {e}");
                None
            }
        }
    }

    fn with_pending<T>(
        &self,
        f: impl FnOnce(&UpdateInfo) -> Result<T, UpdateError>,
    ) -> Result<T, UpdateError> {
        let pending = self.pending.lock().expect("pending lock");
        let info = pending
            .as_ref()
            .ok_or_else(|| UpdateError::Apply("다운로드된 업데이트가 없습니다".into()))?;
        f(info)
    }
}

impl Updater for VelopackUpdater {
    fn download(&self, version: &Version) -> Result<(), UpdateError> {
        let check = self
            .manager
            .check_for_updates()
            .map_err(|e| UpdateError::Download(e.to_string()))?;
        let UpdateCheck::UpdateAvailable(info) = check else {
            return Err(UpdateError::Download(format!(
                "{version} 패키지를 찾을 수 없습니다"
            )));
        };
        self.manager
            .download_updates(&info, None)
            .map_err(|e| UpdateError::Download(e.to_string()))?;
        *self.pending.lock().expect("pending lock") = Some(*info);
        Ok(())
    }

    fn apply_and_restart(&self) -> Result<(), UpdateError> {
        self.with_pending(|info| {
            self.manager
                .apply_updates_and_restart(info)
                .map_err(|e| UpdateError::Apply(e.to_string()))
        })
    }

    fn apply_on_exit(&self) -> Result<(), UpdateError> {
        self.with_pending(|info| {
            self.manager
                .wait_exit_then_apply_updates(info, true, false, Vec::<String>::new())
                .map_err(|e| UpdateError::Apply(e.to_string()))
        })
    }
}

/// 설치판이 아닐 때 쓰는 업데이터. 상태 머신은 `updater_available = false`로 이 경로를 막지만,
/// 실수로 호출되더라도 안전하게 실패한다.
pub struct UnavailableUpdater;

impl Updater for UnavailableUpdater {
    fn download(&self, _version: &Version) -> Result<(), UpdateError> {
        Err(UpdateError::NotInstalled)
    }
    fn apply_and_restart(&self) -> Result<(), UpdateError> {
        Err(UpdateError::NotInstalled)
    }
    fn apply_on_exit(&self) -> Result<(), UpdateError> {
        Err(UpdateError::NotInstalled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_updater_reports_not_installed() {
        let u = UnavailableUpdater;
        assert_eq!(
            u.download(&Version::new(1, 0, 0)),
            Err(UpdateError::NotInstalled)
        );
        assert_eq!(u.apply_and_restart(), Err(UpdateError::NotInstalled));
        assert_eq!(u.apply_on_exit(), Err(UpdateError::NotInstalled));
    }

    #[test]
    fn velopack_is_unavailable_outside_installed_app() {
        assert!(VelopackUpdater::new("https://github.com/example/aragorn").is_none());
    }
}
