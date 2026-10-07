use aragorn_app::session::SaveStore;
use std::{
    cell::Cell,
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
};

/// ROM 옆의 `<ROM 이름>.sav` 파일. 다른 에뮬레이터(BGB, mGBA, VBA-M)와 같은 위치다.
pub struct FsSaveStore {
    path: PathBuf,
    /// 이번 실행에서 `.sav.bak`을 이미 만들었는지. 백업은 처음 저장하기 직전의 세이브 하나만 둔다.
    backed_up: Cell<bool>,
}

impl FsSaveStore {
    pub fn for_rom(rom_path: &Path) -> Self {
        Self {
            path: rom_path.with_extension("sav"),
            backed_up: Cell::new(false),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl SaveStore for FsSaveStore {
    fn load_battery(&self) -> io::Result<Option<Vec<u8>>> {
        match fs::read(&self.path) {
            Ok(data) => Ok(Some(data)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn save_battery(&self, data: &[u8]) -> io::Result<()> {
        // 이번 실행에서 처음 저장할 때만, 그 직전 세이브를 `.sav.bak`으로 남긴다.
        // 여러 번 저장해도 게임을 시작할 때의 세이브가 백업으로 유지된다.
        if !self.backed_up.get() {
            if self.path.exists() {
                fs::copy(&self.path, self.path.with_extension("sav.bak"))?;
            }
            self.backed_up.set(true);
        }
        // 쓰는 도중 꺼지거나 OS가 멈춰도 깨지지 않도록, 임시 파일을 디스크에 확실히 쓴 뒤 교체한다.
        let tmp = self.path.with_extension("sav.tmp");
        let mut file = File::create(&tmp)?;
        file.write_all(data)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, &self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_file_sits_next_to_rom() {
        let store = FsSaveStore::for_rom(Path::new("roms/pokemon red.gb"));
        assert_eq!(store.path(), Path::new("roms/pokemon red.sav"));
    }

    #[test]
    fn missing_save_loads_as_none() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsSaveStore::for_rom(&dir.path().join("game.gb"));
        assert_eq!(store.load_battery().unwrap(), None);
    }

    #[test]
    fn saved_data_loads_back() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsSaveStore::for_rom(&dir.path().join("game.gb"));
        store.save_battery(&[1, 2, 3]).unwrap();
        assert_eq!(store.load_battery().unwrap(), Some(vec![1, 2, 3]));
        assert!(!dir.path().join("game.sav.tmp").exists());
    }

    #[test]
    fn unwritable_location_reports_error() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsSaveStore::for_rom(&dir.path().join("없는 폴더").join("game.gb"));
        assert!(store.save_battery(&[1]).is_err());
    }

    #[test]
    fn backup_keeps_the_save_from_before_this_session() {
        // 백업은 이번 실행에서 처음 저장하기 직전의 세이브다. 여러 번 저장해도 바뀌지 않는다.
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("game.sav"), [0]).unwrap();
        let store = FsSaveStore::for_rom(&dir.path().join("game.gb"));
        store.save_battery(&[1]).unwrap();
        store.save_battery(&[2]).unwrap();
        assert_eq!(fs::read(dir.path().join("game.sav")).unwrap(), [2]);
        assert_eq!(fs::read(dir.path().join("game.sav.bak")).unwrap(), [0]);
    }
}
