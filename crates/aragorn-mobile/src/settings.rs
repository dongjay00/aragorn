//! 앱 설정: 키·게임패드 매핑, 속도, 음소거, 건너뛴 업데이트 버전, 버튼 햅틱. 데스크톱과 같은 `Config`에
//! 모바일 전용 항목을 더해 JSON 파일 하나로 저장하고, 바꿀 때마다 바로 쓴다.
//!
//! 키 이름은 플랫폼마다 다르다. 데스크톱(egui·gilrs 이름)과도 달라 기본값을 플랫폼마다 둔다.
//! - Android 키보드·게임패드: `KeyEvent.keyCodeToString`에서 `KEYCODE_`를 뗀 것 (`DPAD_UP`, `BUTTON_A`)
//! - iOS 키보드: 앱이 `GCKeyCode`에 붙인 이름 (`UP`, `X`, `RETURN`)
//! - iOS 게임패드: Android와 같은 이름을 쓴다 (앱이 GameController 버튼을 이 이름으로 바꾼다)

use crate::{
    EmulatorError,
    types::{FastSpeed, InputAction, InputDevice, Speed},
};
use aragorn_app::{
    config::Config,
    input::{Bindings, InputConfig},
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

/// 키보드 기본값. 데스크톱 기본 키(방향키, X, Z, Backspace, Enter, Tab, Esc, M)와 같은 키다.
pub fn keyboard_default() -> Bindings {
    if cfg!(target_os = "ios") {
        keyboard_default_ios()
    } else {
        keyboard_default_android()
    }
}

fn keyboard_default_ios() -> Bindings {
    Bindings::from_names([
        "RIGHT", "LEFT", "UP", "DOWN", "X", "Z", "DELETE", "RETURN", "TAB", "ESCAPE", "M",
    ])
}

fn keyboard_default_android() -> Bindings {
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

/// 게임패드 기본값 (Android와 iOS가 같다). 데스크톱처럼 아래 버튼이 A, 오른쪽 버튼이 B, 오른쪽 트리거가 배속이다.
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

/// 설정 파일 내용: 데스크톱과 같은 `Config` 항목 + 모바일 전용 항목.
/// 모바일 전용 항목이 없는 예전 파일은 그 항목만 기본값으로 읽는다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct MobileConfig {
    #[serde(flatten)]
    app: Config,
    /// 가상 패드를 누를 때 햅틱(진동)을 낸다.
    haptics: bool,
}

impl Default for MobileConfig {
    fn default() -> Self {
        Self {
            app: config_default(),
            haptics: true,
        }
    }
}

#[derive(uniffi::Object)]
pub struct Settings {
    path: PathBuf,
    config: Mutex<MobileConfig>,
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
            .unwrap_or_default();
        Arc::new(Self {
            path,
            config: Mutex::new(config),
        })
    }

    /// 기능에 지정된 키 이름. 지정하지 않았으면 빈 문자열.
    pub fn binding(&self, device: InputDevice, action: InputAction) -> String {
        bindings(&self.config().app, device)
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
        self.change(|config| bindings_mut(&mut config.app, device).bind(action.into(), &name))
    }

    /// 키 이름 `name`에 지정된 기능들.
    pub fn actions_for(&self, device: InputDevice, name: String) -> Vec<InputAction> {
        bindings(&self.config().app, device)
            .actions_for(&name)
            .map(InputAction::from)
            .collect()
    }

    /// 키 매핑과 속도를 기본값으로 되돌린다.
    pub fn reset_input(&self) -> Result<(), EmulatorError> {
        self.change(|config| config.app.input = input_default())
    }

    pub fn base_speed(&self) -> Speed {
        self.config().app.input.base_speed.into()
    }

    pub fn set_base_speed(&self, speed: Speed) -> Result<(), EmulatorError> {
        self.change(|config| config.app.input.base_speed = speed.into())
    }

    pub fn fast_speed(&self) -> FastSpeed {
        self.config().app.input.fast_forward.into()
    }

    pub fn set_fast_speed(&self, speed: FastSpeed) -> Result<(), EmulatorError> {
        self.change(|config| config.app.input.fast_forward = speed.into())
    }

    pub fn muted(&self) -> bool {
        self.config().app.audio.muted
    }

    pub fn set_muted(&self, muted: bool) -> Result<(), EmulatorError> {
        self.change(|config| config.app.audio.muted = muted)
    }

    /// "이 버전 건너뛰기"로 고른 버전.
    pub fn skipped_version(&self) -> Option<String> {
        self.config().app.update.skipped_version.clone()
    }

    pub fn set_skipped_version(&self, version: Option<String>) -> Result<(), EmulatorError> {
        self.change(|config| config.app.update.skipped_version = version)
    }

    /// 가상 패드를 누를 때 햅틱을 낼지. 기본은 켜짐.
    pub fn haptics(&self) -> bool {
        self.config().haptics
    }

    pub fn set_haptics(&self, on: bool) -> Result<(), EmulatorError> {
        self.change(|config| config.haptics = on)
    }
}

impl Settings {
    fn config(&self) -> MutexGuard<'_, MobileConfig> {
        self.config.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// 설정을 바꾸고 파일에 쓴다. 쓰지 못해도 이번 실행 동안은 바뀐 설정을 쓴다.
    fn change(&self, edit: impl FnOnce(&mut MobileConfig)) -> Result<(), EmulatorError> {
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
    use aragorn_app::input::Action;

    fn open(dir: &tempfile::TempDir) -> Arc<Settings> {
        Settings::open(
            dir.path()
                .join("config.json")
                .to_string_lossy()
                .into_owned(),
        )
    }

    #[test]
    fn ios_keyboard_uses_the_same_keys_as_desktop() {
        let ios = keyboard_default_ios();
        let desktop = Bindings::keyboard_default();
        assert_eq!(ios.get(Action::A), "X");
        assert_eq!(ios.get(Action::Start), "RETURN");
        assert_eq!(desktop.get(Action::Start), "Enter");
        assert_eq!(ios.get(Action::Mute), desktop.get(Action::Mute));
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
    fn haptics_default_on_and_persist() {
        let dir = tempfile::tempdir().unwrap();
        let settings = open(&dir);
        assert!(settings.haptics());
        settings.set_haptics(false).unwrap();
        assert!(!open(&dir).haptics());
    }

    #[test]
    fn file_from_before_haptics_keeps_its_settings() {
        // 햅틱 항목이 생기기 전(0.13.0)에 쓴 파일: 다른 설정은 그대로, 햅틱은 기본값(켜짐).
        let dir = tempfile::tempdir().unwrap();
        let old = Settings::open(dir.path().join("old.json").to_string_lossy().into_owned());
        old.set_muted(true).unwrap();
        old.bind(InputDevice::Keyboard, InputAction::A, "Q".into())
            .unwrap();
        let mut json: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.path().join("old.json")).unwrap()).unwrap();
        json.as_object_mut().unwrap().remove("haptics");
        fs::write(
            dir.path().join("config.json"),
            serde_json::to_vec(&json).unwrap(),
        )
        .unwrap();

        let settings = open(&dir);
        assert!(settings.muted());
        assert_eq!(settings.binding(InputDevice::Keyboard, InputAction::A), "Q");
        assert!(settings.haptics());
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
