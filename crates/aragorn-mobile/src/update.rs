//! 업데이트 확인: GitHub Releases의 서명된 정책(데스크톱과 같은 파일)을 검증하고 판단한다.
//!
//! 정책 파일은 앱(Kotlin·Swift)이 받아 온다. 정책에 이 플랫폼의 패키지(`android`, `ios`)가 있을 때만
//! 새 버전이 있다고 본다. 데스크톱만 낸 릴리스는 모바일에 알리지 않는다.
//!
//! 모바일 앱은 스스로 설치할 수 없어 릴리스 페이지로 안내한다. Android는 같은 키로 서명한 APK만
//! 기존 앱 위에 설치되므로, 받은 APK의 무결성은 Android가 확인한다.

use crate::EmulatorError;
use aragorn_app::update::{UpdateDecision, UpdatePolicy, UpdatePrefs, decide, verify_and_parse};
use semver::Version;

/// 업데이트 정책 서명 검증용 공개키 (데스크톱과 같다).
const POLICY_PUBLIC_KEY: &[u8; 32] = include_bytes!("../../../release/policy.pub");
/// 정책의 패키지 채널 이름.
pub const PACKAGE_CHANNEL: &str = if cfg!(target_os = "ios") {
    "ios"
} else {
    "android"
};

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum UpdateCheck {
    UpToDate,
    /// 새 버전이 있다. 나중에 해도 된다.
    Available {
        version: String,
        message: String,
    },
    /// 이 버전은 더 지원하지 않는다. 업데이트 전까지 게임을 진행하지 않는다.
    Required {
        version: String,
        message: String,
    },
}

/// 앱 버전 (워크스페이스 버전, 데스크톱과 같다).
#[uniffi::export]
pub fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// 받아 온 정책 파일과 서명을 검증하고 판단한다.
#[uniffi::export]
pub fn check_update(
    policy: Vec<u8>,
    signature: Vec<u8>,
    skipped_version: Option<String>,
) -> Result<UpdateCheck, EmulatorError> {
    let policy = verify_and_parse(&policy, &signature, POLICY_PUBLIC_KEY).map_err(|e| {
        EmulatorError::Update {
            reason: e.to_string(),
        }
    })?;
    Ok(judge(
        &current_version(),
        &policy,
        skipped_version.as_deref(),
    ))
}

fn current_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).expect("CARGO_PKG_VERSION은 semver다")
}

fn judge(current: &Version, policy: &UpdatePolicy, skipped: Option<&str>) -> UpdateCheck {
    if !policy.packages.contains_key(PACKAGE_CHANNEL) {
        return UpdateCheck::UpToDate;
    }
    let prefs = UpdatePrefs {
        auto_download: false,
        skipped_version: skipped.and_then(|s| Version::parse(s).ok()),
    };
    let message = policy.message.clone();
    match decide(current, policy, &prefs) {
        UpdateDecision::UpToDate => UpdateCheck::UpToDate,
        UpdateDecision::Soft { version, .. } => UpdateCheck::Available {
            version: version.to_string(),
            message,
        },
        UpdateDecision::Forced { version } => UpdateCheck::Required {
            version: version.to_string(),
            message,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aragorn_app::update::PackageHashes;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    fn policy(latest: &str, min: &str, channels: &[&str]) -> UpdatePolicy {
        UpdatePolicy {
            latest: v(latest),
            minimum_supported: v(min),
            message: "새 기능".into(),
            packages: channels
                .iter()
                .map(|c| (c.to_string(), "ab".repeat(32)))
                .collect::<PackageHashes>(),
        }
    }

    #[test]
    fn desktop_only_release_is_not_offered() {
        let p = policy("2.0.0", "1.5.0", &["win", "osx", "linux"]);
        assert_eq!(judge(&v("1.0.0"), &p, None), UpdateCheck::UpToDate);
    }

    #[test]
    fn newer_android_release_is_offered_unless_skipped() {
        let p = policy("1.1.0", "1.0.0", &["android"]);
        assert_eq!(
            judge(&v("1.0.0"), &p, None),
            UpdateCheck::Available {
                version: "1.1.0".into(),
                message: "새 기능".into()
            }
        );
        assert_eq!(judge(&v("1.0.0"), &p, Some("1.1.0")), UpdateCheck::UpToDate);
    }

    #[test]
    fn unsupported_version_must_update() {
        let p = policy("1.2.0", "1.1.0", &["android"]);
        assert!(matches!(
            judge(&v("1.0.0"), &p, Some("1.2.0")),
            UpdateCheck::Required { .. }
        ));
    }

    #[test]
    fn bad_signature_is_an_error() {
        let result = check_update(br#"{"latest":"9.9.9"}"#.to_vec(), vec![0; 64], None);
        assert!(matches!(result, Err(EmulatorError::Update { .. })));
    }
}
