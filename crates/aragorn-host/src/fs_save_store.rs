use aragorn_app::session::{SaveStore, THUMBNAIL_HEIGHT, THUMBNAIL_WIDTH};
use std::{
    cell::Cell,
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
};

/// ROM 옆의 `<ROM 이름>.sav` 파일. 다른 에뮬레이터(BGB, mGBA, VBA-M)와 같은 위치다.
/// 스테이트는 같은 자리의 `<ROM 이름>.ss0`–`.ss9`, 썸네일은 `.ss0.thumb`(RGBA u32 LE)이다.
pub struct FsSaveStore {
    path: PathBuf,
    rom_path: PathBuf,
    /// 이번 실행에서 `.sav.bak`을 이미 만들었는지. 백업은 처음 저장하기 직전의 세이브 하나만 둔다.
    backed_up: Cell<bool>,
}

impl FsSaveStore {
    pub fn for_rom(rom_path: &Path) -> Self {
        Self {
            path: rom_path.with_extension("sav"),
            rom_path: rom_path.to_path_buf(),
            backed_up: Cell::new(false),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn state_path(&self, slot: u8) -> PathBuf {
        self.rom_path.with_extension(format!("ss{slot}"))
    }

    fn thumbnail_path(&self, slot: u8) -> PathBuf {
        self.rom_path.with_extension(format!("ss{slot}.thumb"))
    }
}

/// 없으면 `Ok(None)`.
fn read_optional(path: &Path) -> io::Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(data) => Ok(Some(data)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// 쓰는 도중 꺼지거나 OS가 멈춰도 깨지지 않도록, 임시 파일을 디스크에 확실히 쓴 뒤 교체한다.
fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let mut file = File::create(&tmp)?;
    file.write_all(data)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&tmp, path)
}

impl SaveStore for FsSaveStore {
    fn load_battery(&self) -> io::Result<Option<Vec<u8>>> {
        read_optional(&self.path)
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
        write_atomic(&self.path, data)
    }

    fn load_state(&self, slot: u8) -> io::Result<Option<Vec<u8>>> {
        read_optional(&self.state_path(slot))
    }

    /// 썸네일을 먼저 쓰고 스테이트를 쓴다. 중간에 실패해도 스테이트와 어긋난 썸네일만 남는다.
    fn save_state(&self, slot: u8, data: &[u8], thumbnail: &[u32]) -> io::Result<()> {
        let thumb: Vec<u8> = thumbnail.iter().flat_map(|p| p.to_le_bytes()).collect();
        write_atomic(&self.thumbnail_path(slot), &thumb)?;
        write_atomic(&self.state_path(slot), data)
    }

    fn load_thumbnail(&self, slot: u8) -> io::Result<Option<Vec<u32>>> {
        let Some(bytes) = read_optional(&self.thumbnail_path(slot))? else {
            return Ok(None);
        };
        if bytes.len() != THUMBNAIL_WIDTH * THUMBNAIL_HEIGHT * 4 {
            return Ok(None);
        }
        let (chunks, _) = bytes.as_chunks::<4>();
        Ok(Some(
            chunks.iter().map(|c| u32::from_le_bytes(*c)).collect(),
        ))
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
    fn state_slots_sit_next_to_rom() {
        let store = FsSaveStore::for_rom(Path::new("roms/pokemon crystal.gbc"));
        assert_eq!(store.state_path(0), Path::new("roms/pokemon crystal.ss0"));
        assert_eq!(store.state_path(9), Path::new("roms/pokemon crystal.ss9"));
    }

    #[test]
    fn state_and_thumbnail_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsSaveStore::for_rom(&dir.path().join("game.gbc"));
        assert_eq!(store.load_state(4).unwrap(), None);
        assert_eq!(store.load_thumbnail(4).unwrap(), None);
        let thumb: Vec<u32> = (0..(THUMBNAIL_WIDTH * THUMBNAIL_HEIGHT) as u32).collect();
        store.save_state(4, b"ARGN...", &thumb).unwrap();
        assert_eq!(
            store.load_state(4).unwrap().as_deref(),
            Some(&b"ARGN..."[..])
        );
        assert_eq!(store.load_thumbnail(4).unwrap(), Some(thumb));
        assert!(!dir.path().join("game.ss4.tmp").exists());
        assert!(
            !dir.path().join("game.sav").exists(),
            "세이브 파일은 건드리지 않는다"
        );
    }

    #[test]
    fn wrong_size_thumbnail_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsSaveStore::for_rom(&dir.path().join("game.gb"));
        fs::write(dir.path().join("game.ss1.thumb"), [1, 2, 3]).unwrap();
        assert_eq!(store.load_thumbnail(1).unwrap(), None);
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
