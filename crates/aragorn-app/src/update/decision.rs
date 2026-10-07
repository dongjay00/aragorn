use semver::Version;
use std::collections::BTreeMap;

/// 채널(win, osx, linux)별 전체 패키지 SHA256 (소문자 16진수).
pub type PackageHashes = BTreeMap<String, String>;

/// 서명 검증을 통과한 업데이트 정책.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdatePolicy {
    pub latest: Version,
    pub minimum_supported: Version,
    pub message: String,
    pub packages: PackageHashes,
}

/// 내려받을 패키지와, 서명된 정책이 보증하는 채널별 해시.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageSpec {
    pub version: Version,
    pub sha256_by_channel: PackageHashes,
}

/// 업데이트에 관한 사용자 선택.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpdatePrefs {
    pub auto_download: bool,
    pub skipped_version: Option<Version>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateDecision {
    UpToDate,
    Soft {
        version: Version,
        auto_download: bool,
    },
    Forced {
        version: Version,
    },
}

pub fn decide(current: &Version, policy: &UpdatePolicy, prefs: &UpdatePrefs) -> UpdateDecision {
    if !policy.latest.pre.is_empty() {
        return UpdateDecision::UpToDate;
    }
    if *current < policy.minimum_supported {
        return UpdateDecision::Forced {
            version: policy.latest.clone(),
        };
    }
    let skipped = prefs.skipped_version.as_ref() == Some(&policy.latest);
    if *current < policy.latest && !skipped {
        return UpdateDecision::Soft {
            version: policy.latest.clone(),
            auto_download: prefs.auto_download,
        };
    }
    UpdateDecision::UpToDate
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    fn policy(latest: &str, min: &str) -> UpdatePolicy {
        UpdatePolicy {
            latest: v(latest),
            minimum_supported: v(min),
            message: String::new(),
            packages: PackageHashes::new(),
        }
    }

    fn prefs(auto_download: bool, skipped: Option<&str>) -> UpdatePrefs {
        UpdatePrefs {
            auto_download,
            skipped_version: skipped.map(v),
        }
    }

    #[test]
    fn same_version_is_up_to_date() {
        let d = decide(&v("1.0.0"), &policy("1.0.0", "1.0.0"), &prefs(true, None));
        assert_eq!(d, UpdateDecision::UpToDate);
    }

    #[test]
    fn newer_latest_is_soft_update() {
        let d = decide(&v("1.0.0"), &policy("1.1.0", "1.0.0"), &prefs(false, None));
        assert_eq!(
            d,
            UpdateDecision::Soft {
                version: v("1.1.0"),
                auto_download: false
            }
        );
    }

    #[test]
    fn soft_update_carries_auto_download_pref() {
        let d = decide(&v("1.0.0"), &policy("1.1.0", "1.0.0"), &prefs(true, None));
        assert_eq!(
            d,
            UpdateDecision::Soft {
                version: v("1.1.0"),
                auto_download: true
            }
        );
    }

    #[test]
    fn below_minimum_is_forced_to_latest() {
        let d = decide(&v("1.0.0"), &policy("1.2.0", "1.1.0"), &prefs(true, None));
        assert_eq!(
            d,
            UpdateDecision::Forced {
                version: v("1.2.0")
            }
        );
    }

    #[test]
    fn skipped_version_is_not_offered_again() {
        let d = decide(
            &v("1.0.0"),
            &policy("1.1.0", "1.0.0"),
            &prefs(true, Some("1.1.0")),
        );
        assert_eq!(d, UpdateDecision::UpToDate);
    }

    #[test]
    fn newer_release_after_skipped_one_is_offered() {
        let d = decide(
            &v("1.0.0"),
            &policy("1.2.0", "1.0.0"),
            &prefs(false, Some("1.1.0")),
        );
        assert_eq!(
            d,
            UpdateDecision::Soft {
                version: v("1.2.0"),
                auto_download: false
            }
        );
    }

    #[test]
    fn skipping_cannot_bypass_forced_update() {
        let d = decide(
            &v("1.0.0"),
            &policy("1.1.0", "1.1.0"),
            &prefs(true, Some("1.1.0")),
        );
        assert_eq!(
            d,
            UpdateDecision::Forced {
                version: v("1.1.0")
            }
        );
    }

    #[test]
    fn local_build_newer_than_latest_is_up_to_date() {
        let d = decide(&v("1.3.0"), &policy("1.2.0", "1.0.0"), &prefs(true, None));
        assert_eq!(d, UpdateDecision::UpToDate);
    }

    #[test]
    fn prerelease_latest_is_ignored() {
        let d = decide(
            &v("1.0.0"),
            &policy("1.1.0-beta.1", "1.0.0"),
            &prefs(true, None),
        );
        assert_eq!(d, UpdateDecision::UpToDate);
    }
}
