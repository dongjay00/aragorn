//! 인수 테스트 공용 도우미. ROM은 `scripts/fetch-test-roms.sh`로 `tests/roms/`에 내려받는다.

use std::path::PathBuf;

/// `tests/roms/` 기준 상대 경로의 ROM을 읽는다. 없으면 경고 후 건너뛰고,
/// `ARAGORN_REQUIRE_TEST_ROMS`가 있으면(CI) 실패시킨다.
pub fn load_rom(rel: &str) -> Option<Vec<u8>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/roms")
        .join(rel);
    match std::fs::read(&path) {
        Ok(rom) => Some(rom),
        Err(e) if std::env::var_os("ARAGORN_REQUIRE_TEST_ROMS").is_none() => {
            eprintln!(
                "경고: {rel}을(를) 읽을 수 없어 건너뜁니다 ({e}). scripts/fetch-test-roms.sh로 내려받으세요."
            );
            None
        }
        Err(e) => panic!("{rel}을(를) 읽을 수 없습니다: {e}"),
    }
}
