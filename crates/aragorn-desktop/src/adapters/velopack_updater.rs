use aragorn_app::update::{PackageSpec, UpdateError, Updater};
use std::sync::Mutex;
use velopack::{UpdateCheck, UpdateInfo, UpdateManager, sources::GithubSource};

/// `vpk pack`이 채널을 지정하지 않았을 때 쓰는 OS별 기본 채널 이름.
pub const PACKAGE_CHANNEL: &str = if cfg!(windows) {
    "win"
} else if cfg!(target_os = "macos") {
    "osx"
} else {
    "linux"
};

/// 피드가 제시한 패키지가 서명된 정책이 지정한 버전, 해시와 같은지 확인한다.
/// 피드(releases.{channel}.json)는 서명되지 않았으므로 이 검사가 패키지 무결성의 근거다.
fn check_offered_package(
    spec: &PackageSpec,
    channel: &str,
    offered_version: &str,
    offered_sha256: &str,
) -> Result<(), UpdateError> {
    let expected = spec.sha256_by_channel.get(channel).ok_or_else(|| {
        UpdateError::Download(format!("서명된 정책에 {channel} 패키지 정보가 없습니다"))
    })?;
    if offered_version != spec.version.to_string() {
        return Err(UpdateError::Download(format!(
            "피드의 버전 {offered_version}이(가) 정책의 {}와 다릅니다",
            spec.version
        )));
    }
    if !offered_sha256.eq_ignore_ascii_case(expected) {
        return Err(UpdateError::Download(
            "패키지 해시가 서명된 정책과 다릅니다".into(),
        ));
    }
    Ok(())
}

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
    fn download(&self, spec: &PackageSpec) -> Result<(), UpdateError> {
        let check = self
            .manager
            .check_for_updates()
            .map_err(|e| UpdateError::Download(e.to_string()))?;
        let UpdateCheck::UpdateAvailable(info) = check else {
            return Err(UpdateError::Download(format!(
                "{} 패키지를 찾을 수 없습니다",
                spec.version
            )));
        };
        let mut info = *info;
        check_offered_package(
            spec,
            PACKAGE_CHANNEL,
            &info.TargetFullRelease.Version,
            &info.TargetFullRelease.SHA256,
        )?;
        // 델타로 재조립한 패키지는 서명된 해시로 검증되지 않으므로 전체 패키지만 받는다.
        info.BaseRelease = None;
        info.DeltasToTarget.clear();
        self.manager
            .download_updates(&info, None)
            .map_err(|e| UpdateError::Download(e.to_string()))?;
        *self.pending.lock().expect("pending lock") = Some(info);
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
    fn download(&self, _spec: &PackageSpec) -> Result<(), UpdateError> {
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
    use aragorn_app::update::PackageHashes;
    use semver::Version;

    #[test]
    fn unavailable_updater_reports_not_installed() {
        let u = UnavailableUpdater;
        assert_eq!(u.download(&spec()), Err(UpdateError::NotInstalled));
        assert_eq!(u.apply_and_restart(), Err(UpdateError::NotInstalled));
        assert_eq!(u.apply_on_exit(), Err(UpdateError::NotInstalled));
    }

    fn spec() -> PackageSpec {
        PackageSpec {
            version: Version::new(1, 1, 0),
            sha256_by_channel: PackageHashes::from([("win".to_string(), "ab".repeat(32))]),
        }
    }

    #[test]
    fn accepts_package_matching_signed_policy() {
        assert_eq!(
            check_offered_package(&spec(), "win", "1.1.0", &"AB".repeat(32)),
            Ok(())
        );
    }

    #[test]
    fn rejects_channel_missing_from_policy() {
        let result = check_offered_package(&spec(), "linux", "1.1.0", &"ab".repeat(32));
        assert!(
            matches!(result, Err(UpdateError::Download(_))),
            "{result:?}"
        );
    }

    #[test]
    fn rejects_version_other_than_policy() {
        let result = check_offered_package(&spec(), "win", "9.9.9", &"ab".repeat(32));
        assert!(
            matches!(result, Err(UpdateError::Download(_))),
            "{result:?}"
        );
    }

    #[test]
    fn rejects_hash_other_than_policy() {
        let result = check_offered_package(&spec(), "win", "1.1.0", &"cd".repeat(32));
        assert!(
            matches!(result, Err(UpdateError::Download(_))),
            "{result:?}"
        );
    }

    #[test]
    fn rejects_feed_without_sha256() {
        let result = check_offered_package(&spec(), "win", "1.1.0", "");
        assert!(
            matches!(result, Err(UpdateError::Download(_))),
            "{result:?}"
        );
    }

    #[test]
    fn package_channel_is_a_velopack_default_channel() {
        assert!(["win", "osx", "linux"].contains(&PACKAGE_CHANNEL));
    }

    #[test]
    fn velopack_is_unavailable_outside_installed_app() {
        assert!(VelopackUpdater::new("https://github.com/example/aragorn").is_none());
    }
}
