//! 앱 설정: 키·게임패드 매핑, 속도, 음소거, 건너뛴 업데이트 버전. 데스크톱과 같은 `Config`를
//! JSON 파일 하나로 저장하고, 바꿀 때마다 바로 쓴다.
//!
//! 키 이름은 Android `KeyEvent.keyCodeToString`에서 `KEYCODE_`를 뗀 것이다 (`DPAD_UP`, `BUTTON_A`).
//! 데스크톱(egui·gilrs 이름)과 이름이 달라 기본값도 따로 둔다.

use crate::{
    EmulatorError,
    types::{FastSpeed, InputAction, InputDevice, Speed},
};
use aragorn_app::{
    config::Config,
    input::{Bindings, InputConfig},
};
use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

/// 키보드 기본값. 데스크톱 기본 키(방향키, X, Z, Backspace, Enter, Tab, Esc, M)와 같은 키다.
pub fn keyboard_default() -> Bindings {
    Bindings::from_names([
        "DPAD_RIGHT",
        "DPAD_LEFT",
        "DPAD_UP",
        "DPAD_DOWN",
        "X",
        "Z",
        "DEL",
        "ENTER",
        "TAB",
        "ESCAPE",
        "M",
    ])
}

/// 게임패드 기본값. 데스크톱처럼 아래 버튼이 A, 오른쪽 버튼이 B, 오른쪽 트리거가 배속이다.
pub fn gamepad_default() -> Bindings {
    Bindings::from_names([
        "DPAD_RIGHT",
        "DPAD_LEFT",
        "DPAD_UP",
        "DPAD_DOWN",
        "BUTTON_A",
        "BUTTON_B",
        "BUTTON_SELECT",
        "BUTTON_START",
        "BUTTON_R2",
        "",
        "",
    ])
}

fn input_default() -> InputConfig {
    InputConfig {
        keyboard: keyboard_default(),
        gamepad: gamepad_default(),
        ..InputConfig::default()
    }
}

fn config_default() -> Config {
    Config {
        input: input_default(),
        ..Config::default()
    }
}

#[derive(uniffi::Object)]
pub struct Settings {
    path: PathBuf,
    config: Mutex<Config>,
}

#[uniffi::export]
impl Settings {
    /// 설정 파일을 연다. 없거나 읽을 수 없으면 기본값으로 시작한다.
    #[uniffi::constructor]
    pub fn open(path: String) -> Arc<Self> {
        let path = PathBuf::from(path);
        let config = fs::read(&path)
            .ok()
            .and_then(|data| {
                serde_json::from_slice(&data)
                    .map_err(|e| log::warn!("설정 파일을 읽을 수 없어 기본값을 씁니다: {e}"))
                    .ok()
            })
            .unwrap_or_else(config_default);
        Arc::new(Self {
            path,
            config: Mutex::new(config),
        })
    }

    /// 기능에 지정된 키 이름. 지정하지 않았으면 빈 문자열.
    pub fn binding(&self, device: InputDevice, action: InputAction) -> String {
        bindings(&self.config(), device)
            .get(action.into())
            .to_string()
    }

    /// 기능에 키를 지정한다. 같은 키를 쓰던 다른 기능은 지정을 해제한다. 빈 이름은 지정 해제다.
    pub fn bind(
        &self,
        device: InputDevice,
        action: InputAction,
        name: String,
    ) -> Result<(), EmulatorError> {
        self.change(|config| bindings_mut(config, device).bind(action.into(), &name))
    }

    /// 키 이름 `name`에 지정된 기능들.
    pub fn actions_for(&self, device: InputDevice, name: String) -> Vec<InputAction> {
        bindings(&self.config(), device)
            .actions_for(&name)
            .map(InputAction::from)
            .collect()
    }

    /// 키 매핑과 속도를 기본값으로 되돌린다.
    pub fn reset_input(&self) -> Result<(), EmulatorError> {
        self.change(|config| config.input = input_default())
    }

    pub fn base_speed(&self) -> Speed {
        self.config().input.base_speed.into()
    }

    pub fn set_base_speed(&self, speed: Speed) -> Result<(), EmulatorError> {
        self.change(|config| config.input.base_speed = speed.into())
    }

    pub fn fast_speed(&self) -> FastSpeed {
        self.config().input.fast_forward.into()
    }

    pub fn set_fast_speed(&self, speed: FastSpeed) -> Result<(), EmulatorError> {
        self.change(|config| config.input.fast_forward = speed.into())
    }

    pub fn muted(&self) -> bool {
        self.config().audio.muted
    }

    pub fn set_muted(&self, muted: bool) -> Result<(), EmulatorError> {
        self.change(|config| config.audio.muted = muted)
    }

    /// "이 버전 건너뛰기"로 고른 버전.
    pub fn skipped_version(&self) -> Option<String> {
        self.config().update.skipped_version.clone()
    }

    pub fn set_skipped_version(&self, version: Option<String>) -> Result<(), EmulatorError> {
        self.change(|config| config.update.skipped_version = version)
    }
}

impl Settings {
    fn config(&self) -> MutexGuard<'_, Config> {
        self.config.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// 설정을 바꾸고 파일에 쓴다. 쓰지 못해도 이번 실행 동안은 바뀐 설정을 쓴다.
    fn change(&self, edit: impl FnOnce(&mut Config)) -> Result<(), EmulatorError> {
        let mut config = self.config();
        edit(&mut config);
        let json = serde_json::to_vec_pretty(&*config).map_err(|e| EmulatorError::Io {
            reason: e.to_string(),
        })?;
        write_atomic(&self.path, &json).map_err(|e| EmulatorError::Io {
            reason: format!("설정을 저장하지 못했습니다: {e}"),
        })
    }
}

fn bindings(config: &Config, device: InputDevice) -> &Bindings {
    match device {
        InputDevice::Keyboard => &config.input.keyboard,
        InputDevice::Gamepad => &config.input.gamepad,
    }
}

fn bindings_mut(config: &mut Config, device: InputDevice) -> &mut Bindings {
    match device {
        InputDevice::Keyboard => &mut config.input.keyboard,
        InputDevice::Gamepad => &mut config.input.gamepad,
    }
}

/// 쓰는 도중 앱이 죽어도 설정 파일이 깨지지 않게 임시 파일에 쓴 뒤 바꾼다.
fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    let mut file = File::create(&tmp)?;
    file.write_all(data)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open(dir: &tempfile::TempDir) -> Arc<Settings> {
        Settings::open(
            dir.path()
                .join("config.json")
                .to_string_lossy()
                .into_owned(),
        )
    }

    #[test]
    fn starts_with_android_key_names() {
        let dir = tempfile::tempdir().unwrap();
        let settings = open(&dir);
        assert_eq!(settings.binding(InputDevice::Keyboard, InputAction::A), "X");
        assert_eq!(
            settings.binding(InputDevice::Gamepad, InputAction::FastForward),
            "BUTTON_R2"
        );
        assert_eq!(
            settings.actions_for(InputDevice::Gamepad, "DPAD_UP".into()),
            [InputAction::Up]
        );
        assert_eq!(settings.base_speed(), Speed::X1);
        assert!(!settings.muted());
    }

    #[test]
    fn changes_are_saved_and_reloaded() {
        let dir = tempfile::tempdir().unwrap();
        let settings = open(&dir);
        settings
            .bind(InputDevice::Keyboard, InputAction::B, "X".into())
            .unwrap();
        settings.set_base_speed(Speed::X2).unwrap();
        settings.set_fast_speed(FastSpeed::Unlimited).unwrap();
        settings.set_muted(true).unwrap();
        settings.set_skipped_version(Some("1.2.3".into())).unwrap();

        let reloaded = open(&dir);
        // 같은 키를 쓰던 A는 지정이 풀린다.
        assert_eq!(reloaded.binding(InputDevice::Keyboard, InputAction::A), "");
        assert_eq!(reloaded.binding(InputDevice::Keyboard, InputAction::B), "X");
        assert_eq!(reloaded.base_speed(), Speed::X2);
        assert_eq!(reloaded.fast_speed(), FastSpeed::Unlimited);
        assert!(reloaded.muted());
        assert_eq!(reloaded.skipped_version().as_deref(), Some("1.2.3"));
    }

    #[test]
    fn reset_restores_android_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let settings = open(&dir);
        settings
            .bind(InputDevice::Gamepad, InputAction::Pause, "BUTTON_Y".into())
            .unwrap();
        settings.reset_input().unwrap();
        assert_eq!(
            settings.binding(InputDevice::Gamepad, InputAction::Pause),
            ""
        );
        assert_eq!(
            settings.binding(InputDevice::Gamepad, InputAction::A),
            "BUTTON_A"
        );
    }

    #[test]
    fn corrupt_file_falls_back_to_defaults() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("config.json"), b"{not json").unwrap();
        let settings = open(&dir);
        assert_eq!(
            settings.binding(InputDevice::Keyboard, InputAction::Start),
            "ENTER"
        );
    }
}
