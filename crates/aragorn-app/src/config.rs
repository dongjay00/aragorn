use crate::update::UpdatePrefs;
use semver::Version;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub update: UpdateConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateConfig {
    pub auto_download: bool,
    pub skipped_version: Option<String>,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            auto_download: true,
            skipped_version: None,
        }
    }
}

impl UpdateConfig {
    /// 해석할 수 없는 버전 문자열은 무시한다.
    pub fn to_prefs(&self) -> UpdatePrefs {
        UpdatePrefs {
            auto_download: self.auto_download,
            skipped_version: self
                .skipped_version
                .as_deref()
                .and_then(|s| Version::parse(s).ok()),
        }
    }

    pub fn from_prefs(prefs: &UpdatePrefs) -> Self {
        Self {
            auto_download: prefs.auto_download,
            skipped_version: prefs.skipped_version.as_ref().map(Version::to_string),
        }
    }
}

/// 설정 영속화 포트. `load`는 실패하면 기본값을 돌려준다.
pub trait ConfigStore {
    fn load(&self) -> Config;
    fn save(&self, config: &Config) -> std::io::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_download_defaults_to_on() {
        assert!(Config::default().update.auto_download);
    }

    #[test]
    fn missing_fields_use_defaults() {
        let c: Config = serde_json::from_str("{}").unwrap();
        assert_eq!(c, Config::default());
        let c: Config = serde_json::from_str(r#"{"update":{}}"#).unwrap();
        assert_eq!(c, Config::default());
    }

    #[test]
    fn invalid_skipped_version_is_ignored() {
        let c: Config =
            serde_json::from_str(r#"{"update":{"skipped_version":"garbage"}}"#).unwrap();
        let prefs = c.update.to_prefs();
        assert_eq!(prefs.skipped_version, None);
        assert!(prefs.auto_download);
    }

    #[test]
    fn prefs_round_trip() {
        let prefs = UpdatePrefs {
            auto_download: false,
            skipped_version: Some(Version::new(1, 2, 3)),
        };
        let cfg = UpdateConfig::from_prefs(&prefs);
        assert_eq!(cfg.skipped_version.as_deref(), Some("1.2.3"));
        assert_eq!(cfg.to_prefs(), prefs);
    }
}
