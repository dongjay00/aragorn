use aragorn_app::config::{Config, ConfigStore};
use std::{fs, io, path::PathBuf};

pub struct TomlConfigStore {
    path: PathBuf,
}

impl TomlConfigStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// OS 설정 디렉터리의 `Aragorn/config.toml`
    pub fn default_path() -> Option<PathBuf> {
        directories::ProjectDirs::from("", "", "Aragorn")
            .map(|d| d.config_dir().join("config.toml"))
    }
}

impl ConfigStore for TomlConfigStore {
    fn load(&self) -> Config {
        match fs::read_to_string(&self.path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                log::warn!(
                    "설정 파일을 읽을 수 없어 기본값을 씁니다 ({}): {e}",
                    self.path.display()
                );
                Config::default()
            }),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Config::default(),
            Err(e) => {
                log::warn!("설정 파일 열기 실패 ({}): {e}", self.path.display());
                Config::default()
            }
        }
    }

    fn save(&self, config: &Config) -> io::Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(config).map_err(io::Error::other)?;
        // 쓰는 도중 꺼져도 기존 파일이 깨지지 않도록 임시 파일에 쓰고 교체한다.
        let tmp = self.path.with_extension("toml.tmp");
        fs::write(&tmp, text)?;
        fs::rename(&tmp, &self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn store_in(dir: &Path) -> TomlConfigStore {
        TomlConfigStore::new(dir.join("nested/config.toml"))
    }

    #[test]
    fn missing_file_gives_default() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(store_in(dir.path()).load(), Config::default());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(dir.path());
        let mut config = Config::default();
        config.update.auto_download = false;
        config.update.skipped_version = Some("1.2.3".into());
        store.save(&config).unwrap();
        assert_eq!(store.load(), config);
    }

    #[test]
    fn corrupt_file_falls_back_to_default() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(dir.path());
        fs::create_dir_all(dir.path().join("nested")).unwrap();
        fs::write(
            dir.path().join("nested/config.toml"),
            "this is = = not toml",
        )
        .unwrap();
        assert_eq!(store.load(), Config::default());
    }

    #[test]
    fn partial_file_keeps_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(dir.path());
        fs::create_dir_all(dir.path().join("nested")).unwrap();
        fs::write(
            dir.path().join("nested/config.toml"),
            "[update]\nskipped_version = \"1.0.0\"\n",
        )
        .unwrap();
        let config = store.load();
        assert!(config.update.auto_download);
        assert_eq!(config.update.skipped_version.as_deref(), Some("1.0.0"));
    }

    #[test]
    fn input_bindings_round_trip_through_toml() {
        use aragorn_app::{input::Action, pacing::FastForward};
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(dir.path());
        let mut config = Config::default();
        config.input.keyboard.bind(Action::A, "K");
        config.input.gamepad.bind(Action::Pause, "Mode");
        config.input.fast_forward = FastForward::Unlimited;
        store.save(&config).unwrap();
        assert_eq!(store.load(), config);
    }

    #[test]
    fn wrong_input_value_type_keeps_rest_of_config() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(dir.path());
        fs::create_dir_all(dir.path().join("nested")).unwrap();
        fs::write(
            dir.path().join("nested/config.toml"),
            "[update]\nskipped_version = \"1.0.0\"\n\n[input]\nfast_forward = 4\n\n[input.keyboard]\na = 1\nstart = \"Space\"\n",
        )
        .unwrap();
        let config = store.load();
        assert_eq!(config.update.skipped_version.as_deref(), Some("1.0.0"));
        assert_eq!(config.input.keyboard.start, "Space");
        assert_eq!(config.input.keyboard.a, "X");
    }

    #[test]
    fn partial_input_section_keeps_other_bindings() {
        use aragorn_app::input::{Action, InputConfig};
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(dir.path());
        fs::create_dir_all(dir.path().join("nested")).unwrap();
        fs::write(
            dir.path().join("nested/config.toml"),
            "[input]\nfast_forward = \"x8\"\n\n[input.keyboard]\nstart = \"Space\"\n",
        )
        .unwrap();
        let input = store.load().input;
        assert_eq!(input.keyboard.get(Action::Start), "Space");
        assert_eq!(input.keyboard.get(Action::A), "X");
        assert_eq!(input.gamepad, InputConfig::default().gamepad);
        assert_eq!(input.fast_forward, InputConfig::default().fast_forward);
    }
}
