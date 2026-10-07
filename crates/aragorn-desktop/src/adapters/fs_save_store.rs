use aragorn_app::session::SaveStore;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// ROM 옆의 `<ROM 이름>.sav` 파일. 다른 에뮬레이터(BGB, mGBA, VBA-M)와 같은 위치다.
pub struct FsSaveStore {
    path: PathBuf,
}

impl FsSaveStore {
    pub fn for_rom(rom_path: &Path) -> Self {
        Self {
            path: rom_path.with_extension("sav"),
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
        // 이전 세이브를 `.sav.bak` 하나로 남기고, 쓰는 도중 꺼져도 깨지지 않도록 임시 파일에 쓰고 교체한다.
        if self.path.exists() {
            fs::copy(&self.path, self.path.with_extension("sav.bak"))?;
        }
        let tmp = self.path.with_extension("sav.tmp");
        fs::write(&tmp, data)?;
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
    fn overwriting_keeps_previous_save_as_backup() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsSaveStore::for_rom(&dir.path().join("game.gb"));
        store.save_battery(&[1]).unwrap();
        store.save_battery(&[2]).unwrap();
        assert_eq!(fs::read(dir.path().join("game.sav")).unwrap(), [2]);
        assert_eq!(fs::read(dir.path().join("game.sav.bak")).unwrap(), [1]);
    }

    #[test]
    fn unwritable_location_reports_error() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsSaveStore::for_rom(&dir.path().join("없는 폴더").join("game.gb"));
        assert!(store.save_battery(&[1]).is_err());
    }
}
