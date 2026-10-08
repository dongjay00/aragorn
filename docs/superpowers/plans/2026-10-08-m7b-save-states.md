# 마일스톤 7b: 세이브 스테이트 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:**
- 게임 상태 전체를 슬롯 10개에 저장하고 불러온다.
- F1–F10은 저장, Shift+F1–F10은 불러오기다. 툴바 "스테이트" 메뉴에서 썸네일과 저장 시각을 보고 고른다.

**Architecture:**
- `gb-core`:
  - 새 `state` 모듈이 파일 형식을 맡는다(스펙 §4.8).
    - 헤더: `"ARGN"` + 포맷 버전(u16) + 저장 시각(u64) + ROM 전역 체크섬(u16) + 제목
    - 본문: MessagePack(필드 이름 포함)
    - `StateError`: `NotAState`, `UnsupportedVersion`, `WrongRom`, `Corrupt`
    - `peek()`는 헤더만 읽는다.
    - 큰 배열은 `bytes`/`words` 도우미로 bin으로 저장한다.
  - 모든 하드웨어 상태 구조체에 serde derive를 붙인다.
    - 빼는 것: ROM, 헤더, 믹서의 샘플레이트·충전 계수·출력 버퍼, 시리얼 출력, `ram_dirty`
  - `GameBoy::save_state(now_unix)`, `load_state(data, now_unix)`:
    - ROM·기기·카트리지 구조가 다르면 거부하고 지금 상태를 그대로 둔다.
    - 불러온 뒤에는 ROM과 호스트 샘플레이트를 옮기고, 인덱스로 쓰는 값을 하드웨어 비트 폭으로 감싸고(`sanitize`), RTC를 지난 실제 시간만큼 진행한다.
- `aragorn-app`:
  - `SaveStore`에 `load_state`, `save_state(slot, data, thumbnail)`, `load_thumbnail`을 더한다.
  - `Session::save_state(slot)`, `load_state(slot)`, `slot_info(slot)`, `thumbnail()`(80×72, 2×2 평균)을 더한다.
  - 불러오기 전에는 아직 쓰지 않은 게임 세이브를 먼저 쓴다. 쓰지 못하면 불러오지 않는다.
- `aragorn-desktop`:
  - `FsSaveStore`는 ROM 옆에 `<ROM>.ss0`–`.ss9`와 `.ssN.thumb`를 원자적으로 쓴다.
  - `ui::state_slots`는 단축키 해석, "N분 전" 표시, 슬롯 메뉴를 맡는다.
  - `app.rs`는 툴바 "스테이트" 메뉴와 단축키를 연결하고, 결과를 상태 표시줄에 알린다.

**Tech Stack:** Rust 1.99 (edition 2024). `gb-core`에 새 의존성을 넣는다: `serde`(derive), `rmp-serde` 1.3, 개발용 `rmpv` 1.3.

**Spec:** `docs/superpowers/specs/2026-10-07-gameboy-emulator-design.md` (§3.3 `SaveStore`, §4.1 `save_state`/`load_state`, §4.8 세이브 스테이트 형식, §5.1 스테이트 슬롯, §6.2 단축키, §9의 마일스톤 7)

**작업 브랜치:** `feat/m7b-save-states`

## Global Constraints

- 게임 데이터나 스테이트 파일 때문에 패닉하지 않는다.
- 이름은 Pan Docs 용어를 따른다. 사용자에게 보이는 문구, 문서, 커밋 메시지는 한국어로 쓴다.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `ARAGORN_REQUIRE_TEST_ROMS=1 cargo test --workspace --no-fail-fast`가 모든 커밋에서 통과해야 한다. 로컬과 CI 모두 Rust 1.99 stable이다.
- 리눅스 로컬에서 데스크톱을 빌드할 때 linuxbrew pkg-config를 쓴다면 `PKG_CONFIG_PATH=/usr/lib/x86_64-linux-gnu/pkgconfig`가 필요하다(`libudev-dev`도 필요하다).
- 커밋 메시지 끝에는 `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`를 붙인다.
- 릴리스(v0.8.0)는 병합 뒤 사용자 확인을 받고 한다.

## 패치 적용 방법

각 태스크의 코드는 diff 블록이다. 블록 내용을 **한 글자도 바꾸지 말고**(빈 컨텍스트 줄의 앞 공백 포함) 파일로 저장한 뒤 `git apply <파일>`로 적용한다.

이 패치들은 재현 브랜치에서 main 위에 Task 1→4 순서로 적용해 확인했다. 결과는 아래와 같다.
- 적용 결과가 스파이크와 바이트 단위로 같다. Task 2에서 만드는 바이너리 고정 파일도 같다.
- 태스크마다 테스트 패치만 넣으면 실패하고, 구현 패치까지 넣으면 통과한다.
- clippy 경고는 0이다.
- 새 파일은 테스트 패치가 모듈 주석과 테스트만 만들고, 구현 패치가 그 위에 구현을 더한다.
- 패치를 같은 초 안에 연달아 적용하면 cargo가 바뀐 파일을 놓칠 수 있다. 구현 패치를 적용한 뒤에도 예전 코드 기준의 컴파일 오류가 나면 바뀐 파일을 `touch`한다.

## 계획 전 검증 (스파이크)

- 테스트 결과:

  | 범위 | 결과 |
  |---|---|
  | 단위 테스트 | core 244 (+12), app 102 (+7), desktop 70 (+6), xtask 14 |
  | 새 인수 테스트 | `save_state` 14 |
  | 기존 인수 테스트 | dmg/cgb-acid2, Blargg 15, dmg_sound 12, cgb_sound 12, mooneye 77 모두 그대로 통과 |

- **이어서 실행해도 같은지 (`tests/save_state.rs`)**
  - 테스트 ROM을 돌리다 저장한 뒤 세 경우를 비교한다: 그대로 계속, 같은 인스턴스에 불러와 계속, 새 인스턴스에 불러와 계속.
  - 프레임마다의 화면, 소리 샘플, 끝난 뒤의 전체 상태가 비트 단위로 같다.
  - 테스트 ROM만으로는 빠진 상태가 드러나지 않아서 합성 "부하 ROM"을 더했다.
    - 네 채널 모두 길이·엔벨로프·스윕을 쓴다.
    - 화면은 무늬 타일, 윈도우, 스프라이트, 타이머 인터럽트를 쓴다.
    - CGB에서는 팔레트, 2배속, WRAM 뱅크, HBlank HDMA까지 쓴다.
- **돌연변이 검사:** 필드 하나를 일부러 저장에서 빼고 이 테스트가 잡는지 봤다.
  - 잡는 것: `fs_step`, 믹서 필터, 2배속, 엔벨로프 타이머, LFSR, 스윕 shadow, 길이 카운터, PPU `dot`, `ime`, HDMA, WRAM 뱅크 (11개)
  - 잡지 못하는 것: `window_line`, `since_read`. 스테이트는 항상 VBlank 프레임 경계에서 저장되고, 그때 두 값은 결과에 영향이 없다.
- **손상된 스테이트 (`corrupted_integers_never_panic`):** 본문을 값 트리로 풀어 정수 값 하나하나를 0/1/0x7F/0xFF/0xFFFF/0xFFFFFFFF로 바꿔 불러온다. 처음에는 16개 필드에서 패닉이 났고, 모두 `sanitize`로 막았다.

## 스펙과 다른 결정 (리뷰어 확인용)

1. **본문은 bincode 대신 MessagePack(`rmp-serde`, 필드 이름 포함)이다.**
   - 스펙 §4.8은 "bincode 본문 + 새 필드는 `#[serde(default)]`"다. 그런데 bincode처럼 필드 이름이 없는 형식에서는 필드가 늘면 이전 데이터를 읽지 못해서 `#[serde(default)]`가 동작하지 않는다.
   - bincode는 2025년에 관리 중단이 공지됐다.
   - 사용자와 합의했다. `gb-core` 의존성은 스펙 §3.2의 `serde`에 `rmp-serde`가 더해진다.
2. **`save_state(now_unix)`, `load_state(data, now_unix)`로 시각 인자를 더한다.**
   - 스펙 §4.1은 시각 인자가 없다. M6b의 `battery_ram(now_unix)`와 같은 이유다.
   - 헤더에 저장 시각을 넣고, 불러올 때 RTC를 실제로 지난 시간만큼 진행한다(시계가 거꾸로 가면 진행하지 않는다).
3. **불러오면 외부 RAM(게임 세이브)도 스테이트의 것으로 되돌아간다.**
   - 사용자와 합의했다. `.sav`는 바로 덮어쓰지 않고 게임이 다시 저장할 때 기록한다.
   - 그래서 `ram_dirty`는 스테이트에서 뺀다.
   - 불러오기 전에는 아직 쓰지 않은 게임 세이브를 먼저 쓴다.
4. **`SaveStore`는 ROM별 인스턴스다.** 스펙 §3.3 시그니처의 `rom_id` 인자 대신 기존 `.sav` 저장소처럼 ROM 경로로 만든다. `load_thumbnail`을 더한다.
5. **F1–F10 단축키는 고정이다.** 설정 창에서 다시 지정하지 않는다. M7a의 키 설정은 게임 버튼, 배속, 일시정지만 다룬다.
6. **슬롯 번호는 사용자에게 1–10으로 보인다**(F1 = 슬롯 1). 파일 이름은 `.ss0`–`.ss9`다.

## Review Focus

1. **빠진 상태:** 저장해야 하는데 빠진 필드, 또는 호스트 설정인데 저장된 필드가 없는지 본다.
   - 각 구조체의 `#[serde(skip)]` 근거
   - 돌연변이 검사가 잡지 못한 `window_line`, `since_read`의 영향
   - → `tests/save_state.rs` 전체
2. **손상되었거나 악의적인 스테이트:** 패닉 없이 거부하거나 감싸야 한다.
   - 퍼즈 테스트가 바꾸지 않는 실수 값(믹서 위상·필터)과 배열 길이, 맵 구조
   - → `corrupted_integers_never_panic`, `truncated_or_garbled_data_is_corrupt_not_panic`, `wrong_array_length_is_corrupt`
3. **불러오기 실패 때 지금 게임이 그대로인지:** 다른 ROM, 다른 기기, 깨진 파일, 아직 쓰지 않은 세이브를 쓰지 못한 경우.
   - → `state_of_another_rom_is_rejected_and_nothing_changes`, `state_from_another_model_is_rejected`, `unsaved_game_progress_blocks_state_loading`
4. **게임 세이브와의 관계:**
   - 불러오기만으로 `.sav`를 쓰지 않아야 한다.
   - 불러오기 전에 쓰지 않은 진행을 먼저 써야 한다.
   - `.sav.bak` 정책과 충돌하지 않아야 한다.
   - → `state_slot_restores_game_ram_without_touching_save_file`, `loading_after_save_point_does_not_rewrite_save_file`
5. **호환성:** v1 고정 파일(`tests/fixtures/state-v1-cgb.argn`)을 다음 버전이 읽어야 한다. MessagePack named에서 `#[serde(default)]`로 필드를 더할 수 있는지 본다.
6. **UI:**
   - 단축키가 설정 창의 키 입력 대기나 키 반복과 충돌하지 않는지
   - 종료 중이나 강제 업데이트 중에는 동작하지 않는지
   - 메뉴 썸네일 텍스처가 저장할 때마다 새로 만들어지는지

---
### Task 1: 스테이트 파일 형식

**Files:** `crates/gb-core/Cargo.toml`, `Cargo.lock`, `crates/gb-core/src/lib.rs`, `crates/gb-core/src/state.rs`

**Interfaces:**
- Produces:
  - `gb_core::state::{MAGIC, FORMAT_VERSION, StateError, RomId, StateHeader, peek}`
  - crate 내부: `encode(rom, saved_at, body)`, `split(data, rom) -> (saved_at, body)`, `decode(body)`
  - serde `with` 도우미: `bytes`, `bytes::boxed`, `words`
- 이 태스크에서는 `encode`/`split`/`decode`를 테스트만 쓴다. 그래서 `#![allow(dead_code)]`를 두고, Task 2에서 지운다.

- [ ] **Step 1: 테스트 패치를 적용한다**

```bash
git checkout main && git pull --ff-only && git checkout -b feat/m7b-save-states
```

```diff
--- a/crates/gb-core/src/lib.rs
+++ b/crates/gb-core/src/lib.rs
@@ -13,6 +13,7 @@
 pub mod ppu;
 pub mod rtc;
 pub mod serial;
+pub mod state;
 pub mod timer;
 
 pub use cartridge::{CartError, Header};
@@ -20,3 +21,4 @@
 pub use gameboy::{DebugView, GameBoy};
 pub use joypad::Button;
 pub use model::Model;
+pub use state::{RomId, StateError};
--- /dev/null
+++ b/crates/gb-core/src/state.rs
@@ -0,0 +1,113 @@
+//! 세이브 스테이트 파일 형식 (스펙 §4.8).
+//!
+//! `"ARGN"` + 포맷 버전(u16 LE) + 저장 시각(u64 LE, 유닉스 초) + ROM 전역 체크섬(u16 LE)
+//! + 제목 길이(u8) + 제목(UTF-8) + 본문(MessagePack, 필드 이름 포함).
+//!
+//! 본문은 필드 이름을 함께 저장하므로 다음 버전에서 필드가 늘어도 `#[serde(default)]`로 이전 스테이트를
+//! 읽을 수 있다. ROM 데이터는 넣지 않는다.
+#[cfg(test)]
+mod tests {
+    use super::*;
+    use serde::{Deserialize, Serialize};
+
+    fn rom() -> RomId {
+        RomId {
+            global_checksum: 0xBEEF,
+            title: "POKEMON CRYSTAL".into(),
+        }
+    }
+
+    #[derive(Serialize, Deserialize, Debug, PartialEq)]
+    struct Big {
+        #[serde(with = "bytes")]
+        hram: [u8; 0x7F],
+        #[serde(with = "bytes::boxed")]
+        wram: Box<[u8; 0x8000]>,
+        #[serde(with = "words")]
+        screen: Box<[u32; 6]>,
+    }
+
+    fn big() -> Big {
+        let mut wram = Box::new([0u8; 0x8000]);
+        wram[0x7FFF] = 9;
+        Big {
+            hram: [7; 0x7F],
+            wram,
+            screen: Box::new([1, 2, 3, 0xFFFF_FFFF, 5, 6]),
+        }
+    }
+
+    #[test]
+    fn round_trips_header_and_body() {
+        let data = encode(&rom(), 1_700_000_000, &big());
+        assert_eq!(&data[..4], MAGIC);
+        let header = peek(&data).unwrap();
+        assert_eq!(header.saved_at, 1_700_000_000);
+        assert_eq!(header.rom, rom());
+        let (saved_at, body) = split(&data, &rom()).unwrap();
+        assert_eq!(saved_at, 1_700_000_000);
+        assert_eq!(decode::<Big>(body).unwrap(), big());
+    }
+
+    #[test]
+    fn big_arrays_are_stored_as_bytes() {
+        let data = encode(&rom(), 0, &big());
+        assert!(data.len() < 0x8000 + 0x7F + 24 + 200, "{}", data.len());
+    }
+
+    #[test]
+    fn rejects_other_files_versions_and_roms() {
+        assert_eq!(peek(b"").unwrap_err(), StateError::NotAState);
+        assert_eq!(peek(b"PNG\x00....").unwrap_err(), StateError::NotAState);
+        let mut data = encode(&rom(), 0, &big());
+        let other = RomId {
+            global_checksum: 0x1234,
+            title: "POKEMON GOLD".into(),
+        };
+        assert_eq!(
+            split(&data, &other).unwrap_err(),
+            StateError::WrongRom {
+                title: "POKEMON CRYSTAL".into()
+            }
+        );
+        data[4] = 2;
+        assert_eq!(peek(&data).unwrap_err(), StateError::UnsupportedVersion(2));
+    }
+
+    #[test]
+    fn truncated_or_garbled_data_is_corrupt_not_panic() {
+        let data = encode(&rom(), 0, &big());
+        for len in [5, 10, 16, 20, 40, data.len() - 1] {
+            let cut = &data[..len];
+            let result = split(cut, &rom()).and_then(|(_, body)| decode::<Big>(body));
+            assert!(matches!(result, Err(StateError::Corrupt(_))), "{len}");
+        }
+    }
+
+    #[test]
+    fn wrong_array_length_is_corrupt() {
+        #[derive(Serialize)]
+        struct Short {
+            #[serde(with = "serde_bytes_short")]
+            hram: Vec<u8>,
+            wram: (),
+            screen: (),
+        }
+        mod serde_bytes_short {
+            pub fn serialize<S: serde::Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
+                s.serialize_bytes(v)
+            }
+        }
+        let data = encode(
+            &rom(),
+            0,
+            &Short {
+                hram: vec![1; 3],
+                wram: (),
+                screen: (),
+            },
+        );
+        let (_, body) = split(&data, &rom()).unwrap();
+        assert!(matches!(decode::<Big>(body), Err(StateError::Corrupt(_))));
+    }
+}
```

- [ ] **Step 2: 실패하는지 확인한다**

Run: `ARAGORN_REQUIRE_TEST_ROMS=1 cargo test --workspace`
Expected: 컴파일 실패, ``unresolved imports `state::RomId`, `state::StateError` ``

- [ ] **Step 3: 구현 패치를 적용한다**

```diff
--- a/crates/gb-core/Cargo.toml
+++ b/crates/gb-core/Cargo.toml
@@ -5,6 +5,9 @@
 license.workspace = true
 
 [dependencies]
+serde = { version = "1", features = ["derive"] }
+rmp-serde = "1.3"
 
 [dev-dependencies]
 png = "0.17"
+rmpv = "1.3"
--- a/Cargo.lock
+++ b/Cargo.lock
@@ -1687,6 +1687,9 @@
 version = "0.7.0"
 dependencies = [
  "png 0.17.16",
+ "rmp-serde",
+ "rmpv",
+ "serde",
 ]
 
 [[package]]
@@ -3537,6 +3540,34 @@
  "libc",
  "untrusted",
  "windows-sys 0.52.0",
+]
+
+[[package]]
+name = "rmp"
+version = "0.8.15"
+source = "registry+https://github.com/rust-lang/crates.io-index"
+checksum = "4ba8be72d372b2c9b35542551678538b562e7cf86c3315773cae48dfbfe7790c"
+dependencies = [
+ "num-traits",
+]
+
+[[package]]
+name = "rmp-serde"
+version = "1.3.1"
+source = "registry+https://github.com/rust-lang/crates.io-index"
+checksum = "72f81bee8c8ef9b577d1681a70ebbc962c232461e397b22c208c43c04b67a155"
+dependencies = [
+ "rmp",
+ "serde",
+]
+
+[[package]]
+name = "rmpv"
+version = "1.3.1"
+source = "registry+https://github.com/rust-lang/crates.io-index"
+checksum = "7a4e1d4b9b938a26d2996af33229f0ca0956c652c1375067f0b45291c1df8417"
+dependencies = [
+ "rmp",
 ]
 
 [[package]]
--- a/crates/gb-core/src/state.rs
+++ b/crates/gb-core/src/state.rs
@@ -5,6 +5,221 @@
 //!
 //! 본문은 필드 이름을 함께 저장하므로 다음 버전에서 필드가 늘어도 `#[serde(default)]`로 이전 스테이트를
 //! 읽을 수 있다. ROM 데이터는 넣지 않는다.
+
+// Task 2에서 GameBoy가 쓰기 전까지는 테스트에서만 쓴다.
+#![allow(dead_code)]
+
+use std::fmt;
+
+pub const MAGIC: &[u8; 4] = b"ARGN";
+/// 본문 구조가 호환되지 않게 바뀔 때만 올린다.
+pub const FORMAT_VERSION: u16 = 1;
+
+/// 스테이트를 불러올 수 없는 이유. 불러오기에 실패하면 지금 상태는 바뀌지 않는다.
+#[derive(Debug, Clone, PartialEq, Eq)]
+pub enum StateError {
+    /// 세이브 스테이트 파일이 아니다.
+    NotAState,
+    /// 이 버전이 읽지 못하는 포맷 버전이다.
+    UnsupportedVersion(u16),
+    /// 다른 게임의 스테이트다.
+    WrongRom { title: String },
+    /// 형식은 맞지만 내용이 깨졌다.
+    Corrupt(String),
+}
+
+impl fmt::Display for StateError {
+    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
+        match self {
+            StateError::NotAState => write!(f, "세이브 스테이트 파일이 아닙니다"),
+            StateError::UnsupportedVersion(v) => {
+                write!(f, "지원하지 않는 스테이트 형식입니다 (버전 {v})")
+            }
+            StateError::WrongRom { title } => write!(f, "다른 게임({title})의 스테이트입니다"),
+            StateError::Corrupt(why) => write!(f, "스테이트 파일이 손상되었습니다: {why}"),
+        }
+    }
+}
+
+impl std::error::Error for StateError {}
+
+/// 스테이트가 어느 ROM의 것인지 가리는 값: 헤더의 전역 체크섬과 제목.
+#[derive(Debug, Clone, PartialEq, Eq)]
+pub struct RomId {
+    pub global_checksum: u16,
+    pub title: String,
+}
+
+/// 헤더를 붙여 스테이트 파일 내용을 만든다.
+pub(crate) fn encode<T: serde::Serialize>(rom: &RomId, saved_at: u64, body: &T) -> Vec<u8> {
+    let title = rom.title.as_bytes();
+    let title = &title[..title.len().min(255)];
+    let mut out = Vec::with_capacity(64 * 1024);
+    out.extend_from_slice(MAGIC);
+    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
+    out.extend_from_slice(&saved_at.to_le_bytes());
+    out.extend_from_slice(&rom.global_checksum.to_le_bytes());
+    out.push(title.len() as u8);
+    out.extend_from_slice(title);
+    rmp_serde::encode::write_named(&mut out, body).expect("메모리 버퍼 쓰기는 실패하지 않는다");
+    out
+}
+
+/// 헤더를 확인하고 `(저장 시각, 본문 바이트)`를 돌려준다.
+pub(crate) fn split<'a>(data: &'a [u8], rom: &RomId) -> Result<(u64, &'a [u8]), StateError> {
+    let header = peek(data)?;
+    if header.rom != *rom {
+        return Err(StateError::WrongRom {
+            title: header.rom.title,
+        });
+    }
+    Ok((header.saved_at, &data[header.body_offset..]))
+}
+
+/// 본문을 읽는다.
+pub(crate) fn decode<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T, StateError> {
+    rmp_serde::from_slice(body).map_err(|e| StateError::Corrupt(e.to_string()))
+}
+
+/// 스테이트 파일 헤더.
+#[derive(Debug, Clone, PartialEq, Eq)]
+pub struct StateHeader {
+    pub saved_at: u64,
+    pub rom: RomId,
+    body_offset: usize,
+}
+
+/// 본문을 풀지 않고 헤더만 읽는다 (슬롯 목록 표시용).
+pub fn peek(data: &[u8]) -> Result<StateHeader, StateError> {
+    if data.len() < 4 || &data[..4] != MAGIC {
+        return Err(StateError::NotAState);
+    }
+    let fixed = 4 + 2 + 8 + 2 + 1;
+    if data.len() < fixed {
+        return Err(StateError::Corrupt("헤더가 잘렸습니다".into()));
+    }
+    let version = u16::from_le_bytes([data[4], data[5]]);
+    if version != FORMAT_VERSION {
+        return Err(StateError::UnsupportedVersion(version));
+    }
+    let saved_at = u64::from_le_bytes(data[6..14].try_into().unwrap());
+    let global_checksum = u16::from_le_bytes([data[14], data[15]]);
+    let title_len = usize::from(data[16]);
+    let Some(title) = data.get(fixed..fixed + title_len) else {
+        return Err(StateError::Corrupt("헤더가 잘렸습니다".into()));
+    };
+    Ok(StateHeader {
+        saved_at,
+        rom: RomId {
+            global_checksum,
+            title: String::from_utf8_lossy(title).into_owned(),
+        },
+        body_offset: fixed + title_len,
+    })
+}
+
+/// 큰 바이트 배열(`[u8; N]`, `Box<[u8; N]>`)을 MessagePack bin으로 저장한다. serde는 33개 이상 원소의
+/// 배열을 지원하지 않는다. 길이가 다르면 손상으로 본다.
+pub(crate) mod bytes {
+    use serde::{Deserializer, Serializer, de::Error};
+
+    pub fn serialize<S: Serializer, const N: usize>(v: &[u8; N], s: S) -> Result<S::Ok, S::Error> {
+        s.serialize_bytes(v)
+    }
+
+    pub fn deserialize<'de, D: Deserializer<'de>, const N: usize>(
+        d: D,
+    ) -> Result<[u8; N], D::Error> {
+        let v: serde_bytes_buf::Buf = serde::Deserialize::deserialize(d)?;
+        v.0.try_into()
+            .map_err(|v: Vec<u8>| D::Error::invalid_length(v.len(), &"정해진 길이의 바이트 배열"))
+    }
+
+    pub mod boxed {
+        use serde::{Deserializer, Serializer};
+
+        #[allow(clippy::borrowed_box)] // serde `with`는 필드 타입(`Box<[u8; N]>`)의 참조를 넘긴다
+        pub fn serialize<S: Serializer, const N: usize>(
+            v: &Box<[u8; N]>,
+            s: S,
+        ) -> Result<S::Ok, S::Error> {
+            super::serialize(v, s)
+        }
+
+        pub fn deserialize<'de, D: Deserializer<'de>, const N: usize>(
+            d: D,
+        ) -> Result<Box<[u8; N]>, D::Error> {
+            super::deserialize(d).map(Box::new)
+        }
+    }
+
+    /// bin과 정수 배열 둘 다 받는 바이트 버퍼.
+    pub mod serde_bytes_buf {
+        use serde::de::{Deserialize, Deserializer, SeqAccess, Visitor};
+        use std::fmt;
+
+        pub struct Buf(pub Vec<u8>);
+
+        impl<'de> Deserialize<'de> for Buf {
+            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
+                struct V;
+                impl<'de> Visitor<'de> for V {
+                    type Value = Buf;
+                    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
+                        f.write_str("바이트 배열")
+                    }
+                    fn visit_bytes<E>(self, v: &[u8]) -> Result<Buf, E> {
+                        Ok(Buf(v.to_vec()))
+                    }
+                    fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Buf, E> {
+                        Ok(Buf(v))
+                    }
+                    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Buf, A::Error> {
+                        let mut out = Vec::new();
+                        while let Some(b) = seq.next_element()? {
+                            out.push(b);
+                        }
+                        Ok(Buf(out))
+                    }
+                }
+                d.deserialize_bytes(V)
+            }
+        }
+    }
+}
+
+/// 화면 버퍼(`Box<[u32; N]>`)를 리틀 엔디언 bin으로 저장한다.
+pub(crate) mod words {
+    use serde::{Deserializer, Serializer, de::Error};
+
+    #[allow(clippy::borrowed_box)] // serde `with`는 필드 타입(`Box<[u32; N]>`)의 참조를 넘긴다
+    pub fn serialize<S: Serializer, const N: usize>(
+        v: &Box<[u32; N]>,
+        s: S,
+    ) -> Result<S::Ok, S::Error> {
+        let bytes: Vec<u8> = v.iter().flat_map(|w| w.to_le_bytes()).collect();
+        s.serialize_bytes(&bytes)
+    }
+
+    pub fn deserialize<'de, D: Deserializer<'de>, const N: usize>(
+        d: D,
+    ) -> Result<Box<[u32; N]>, D::Error> {
+        let buf: super::bytes::serde_bytes_buf::Buf = serde::Deserialize::deserialize(d)?;
+        if buf.0.len() != N * 4 {
+            return Err(D::Error::invalid_length(
+                buf.0.len(),
+                &"화면 크기 × 4바이트",
+            ));
+        }
+        let mut out = Box::new([0u32; N]);
+        let (chunks, _) = buf.0.as_chunks::<4>();
+        for (w, chunk) in out.iter_mut().zip(chunks) {
+            *w = u32::from_le_bytes(*chunk);
+        }
+        Ok(out)
+    }
+}
+
 #[cfg(test)]
 mod tests {
     use super::*;
```

- [ ] **Step 4: 통과하는지 확인한다**

Run: `ARAGORN_REQUIRE_TEST_ROMS=1 cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: core lib 237 passed (기존 232 + state 5), clippy 경고 0

- [ ] **Step 5: 커밋한다**

```bash
cargo fmt --all --check
git add -A crates Cargo.lock
git commit -m "feat(core): 세이브 스테이트 파일 형식과 헤더 검사

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---
### Task 2: 코어 상태 직렬화

**Files:** `crates/gb-core/src/state.rs`, `crates/gb-core/src/apu/channel.rs`, `crates/gb-core/src/apu/mixer.rs`, `crates/gb-core/src/apu/mod.rs`, `crates/gb-core/src/apu/noise.rs`, `crates/gb-core/src/apu/square.rs`, `crates/gb-core/src/apu/wave.rs`, `crates/gb-core/src/bus.rs`, `crates/gb-core/src/cartridge.rs`, `crates/gb-core/src/cpu/mod.rs`, `crates/gb-core/src/cpu/registers.rs`, `crates/gb-core/src/gameboy.rs`, `crates/gb-core/src/joypad.rs`, `crates/gb-core/src/model.rs`, `crates/gb-core/src/ppu.rs`, `crates/gb-core/src/rtc.rs`, `crates/gb-core/src/serial.rs`, `crates/gb-core/src/timer.rs`, `crates/gb-core/tests/save_state.rs`, `crates/gb-core/tests/fixtures/state-v1-cgb.argn (생성)`

**Interfaces:**
- Consumes: Task 1 `state` 모듈
- Produces:
  - `GameBoy::rom_id()`, `GameBoy::save_state(now_unix) -> Vec<u8>`
  - `GameBoy::load_state(data, now_unix) -> Result<(), StateError>`
  - crate 내부: `Cartridge::{rom_id, adopt_rom, advance_rtc}`, `Bus::adopt_host`, `Apu::adopt_host`, `Mixer::adopt_host`, 채널·PPU `sanitize`
- 이 태스크의 GREEN 단계에서 바이너리 고정 파일 `tests/fixtures/state-v1-cgb.argn`을 만든다. 패치로는 담지 않는다.

- [ ] **Step 1: 테스트 패치를 적용한다**

```diff
--- a/crates/gb-core/src/gameboy.rs
+++ b/crates/gb-core/src/gameboy.rs
@@ -259,6 +259,119 @@
     }
 
     #[test]
+    fn state_round_trips_ram_registers_and_screen() {
+        let mut gb = GameBoy::new(saving_rom(), Model::Dmg).unwrap();
+        gb.run_frame();
+        let state = gb.save_state(0);
+        let registers = gb.debug().registers();
+        let mut other = GameBoy::new(saving_rom(), Model::Dmg).unwrap();
+        other.load_state(&state, 0).unwrap();
+        assert_eq!(other.debug().registers(), registers);
+        assert_eq!(
+            other.battery_ram(0).unwrap()[0],
+            0x42,
+            "외부 RAM도 되돌린다"
+        );
+        assert_eq!(other.framebuffer(), gb.framebuffer());
+        assert_eq!(other.save_state(0), state);
+    }
+
+    #[test]
+    fn state_of_another_rom_is_rejected_and_nothing_changes() {
+        let mut other_rom = saving_rom();
+        other_rom[0x0134..0x0138].copy_from_slice(b"GOLD");
+        let other = GameBoy::new(other_rom, Model::Dmg).unwrap();
+        let state = other.save_state(0);
+        let mut gb = GameBoy::new(saving_rom(), Model::Dmg).unwrap();
+        gb.run_frame();
+        let before = gb.save_state(0);
+        assert_eq!(
+            gb.load_state(&state, 0),
+            Err(StateError::WrongRom {
+                title: "GOLD".into()
+            })
+        );
+        assert_eq!(gb.save_state(0), before);
+    }
+
+    #[test]
+    fn same_rom_with_different_global_checksum_is_rejected() {
+        let mut patched = saving_rom();
+        patched[0x014E] = 0x12;
+        let state = GameBoy::new(patched, Model::Dmg).unwrap().save_state(0);
+        let mut gb = GameBoy::new(saving_rom(), Model::Dmg).unwrap();
+        assert!(matches!(
+            gb.load_state(&state, 0),
+            Err(StateError::WrongRom { .. })
+        ));
+    }
+
+    #[test]
+    fn state_from_another_model_is_rejected() {
+        let rom = test_rom(0x00, 0x00, 0x80);
+        let cgb = GameBoy::new(rom.clone(), Model::Cgb).unwrap();
+        let mut dmg = GameBoy::new(rom, Model::Dmg).unwrap();
+        let before = dmg.save_state(0);
+        assert!(matches!(
+            dmg.load_state(&cgb.save_state(0), 0),
+            Err(StateError::Corrupt(_))
+        ));
+        assert_eq!(dmg.save_state(0), before);
+    }
+
+    #[test]
+    fn garbage_is_rejected_without_panic() {
+        let mut gb = GameBoy::new(saving_rom(), Model::Dmg).unwrap();
+        assert_eq!(gb.load_state(b"", 0), Err(StateError::NotAState));
+        let mut state = gb.save_state(0);
+        state.truncate(state.len() / 2);
+        assert!(matches!(
+            gb.load_state(&state, 0),
+            Err(StateError::Corrupt(_))
+        ));
+    }
+
+    #[test]
+    fn loading_advances_rtc_by_real_time_since_saving() {
+        let mut rom = test_rom(0x10, 0x00, 0x00);
+        rom[0x0149] = 0x03;
+        let mut gb = GameBoy::new(rom.clone(), Model::Dmg).unwrap();
+        let state = gb.save_state(1_000);
+        gb.load_state(&state, 1_000 + 2 * 3600 + 5).unwrap();
+        // 세이브 파일의 RTC 블록: RAM 뒤 현재 레지스터 초·분·시 (u32 LE)
+        let ram = gb.battery_ram(0).unwrap();
+        let rtc = &ram[0x8000..];
+        assert_eq!((rtc[0], rtc[4], rtc[8]), (5, 0, 2));
+        // 시계가 거꾸로 가면 진행하지 않는다
+        let mut gb = GameBoy::new(rom, Model::Dmg).unwrap();
+        gb.load_state(&state, 10).unwrap();
+        let rtc = &gb.battery_ram(0).unwrap()[0x8000..];
+        assert_eq!((rtc[0], rtc[4], rtc[8]), (0, 0, 0));
+    }
+
+    #[test]
+    fn loading_keeps_host_sample_rate_and_drops_pending_audio() {
+        let mut gb = GameBoy::new(saving_rom(), Model::Dmg).unwrap();
+        gb.set_sample_rate(48_000.0);
+        let state = gb.save_state(0);
+        gb.set_sample_rate(22_050.0);
+        for _ in 0..10 {
+            gb.run_frame();
+        }
+        gb.load_state(&state, 0).unwrap();
+        let mut pending = Vec::new();
+        gb.drain_audio(&mut pending);
+        assert!(pending.is_empty(), "불러오기 전 소리는 버린다");
+        for _ in 0..60 {
+            gb.run_frame();
+        }
+        let mut audio = Vec::new();
+        gb.drain_audio(&mut audio);
+        let per_second = audio.len() as f64 / 2.0 / (60.0 * 70_224.0 / 4_194_304.0);
+        assert!((per_second - 22_050.0).abs() < 100.0, "{per_second}");
+    }
+
+    #[test]
     fn pressed_button_is_visible_through_p1() {
         // LD A,0x10 ; LDH (0x00),A (버튼 줄 선택) ; JR -2
         let mut gb = gb_with_program(&[0x3E, 0x10, 0xE0, 0x00, 0x18, 0xFE]);
```

- [ ] **Step 2: 실패하는지 확인한다**

Run: `ARAGORN_REQUIRE_TEST_ROMS=1 cargo test --workspace`
Expected: 컴파일 실패, ``no method named `save_state` found for struct `gameboy::GameBoy` ``, ``cannot find type `StateError` in this scope``

- [ ] **Step 3: 구현 패치를 적용한다**

```diff
--- a/crates/gb-core/src/state.rs
+++ b/crates/gb-core/src/state.rs
@@ -5,9 +5,6 @@
 //!
 //! 본문은 필드 이름을 함께 저장하므로 다음 버전에서 필드가 늘어도 `#[serde(default)]`로 이전 스테이트를
 //! 읽을 수 있다. ROM 데이터는 넣지 않는다.
-
-// Task 2에서 GameBoy가 쓰기 전까지는 테스트에서만 쓴다.
-#![allow(dead_code)]
 
 use std::fmt;
 
--- a/crates/gb-core/src/apu/channel.rs
+++ b/crates/gb-core/src/apu/channel.rs
@@ -1,7 +1,7 @@
 //! 채널들이 함께 쓰는 길이 카운터와 볼륨 엔벨로프 (Pan Docs "Audio Registers").
 
 /// 길이 카운터. 켜져 있으면 프레임 시퀀서가 줄이고, 0이 되면 채널을 끈다.
-#[derive(Debug, Clone, Default)]
+#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
 pub struct Length {
     pub enabled: bool,
     pub counter: u16,
@@ -46,7 +46,7 @@
 }
 
 /// 볼륨 엔벨로프 (NRx2).
-#[derive(Debug, Clone, Default)]
+#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
 pub struct Envelope {
     initial: u8,
     up: bool,
@@ -56,6 +56,13 @@
 }
 
 impl Envelope {
+    /// 손상된 스테이트 때문에 패닉하지 않게 레지스터 비트 폭을 넘는 값을 감싼다.
+    pub(crate) fn sanitize(&mut self) {
+        self.initial &= 0x0F;
+        self.period &= 7;
+        self.volume &= 0x0F;
+    }
+
     pub fn write(&mut self, value: u8) {
         self.initial = value >> 4;
         self.up = value & 0x08 != 0;
--- a/crates/gb-core/src/apu/mixer.rs
+++ b/crates/gb-core/src/apu/mixer.rs
@@ -7,8 +7,10 @@
 /// (헤드리스 테스트) 메모리가 늘지 않게 한다.
 const MAX_PENDING: usize = 48_000 * 2;
 
-#[derive(Debug, Clone)]
+#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
 pub struct Mixer {
+    /// 호스트 설정이라 스테이트에 넣지 않는다. 불러올 때 지금 값을 유지한다.
+    #[serde(skip)]
     sample_rate: f64,
     /// 다음 출력 샘플까지 쌓인 위상(× M-사이클 1초).
     phase: f64,
@@ -16,7 +18,9 @@
     count: u32,
     /// 하이패스 필터 축전기 전압 (gbdev wiki "Game Boy Sound Hardware" Obscure Behavior).
     capacitor: (f32, f32),
+    #[serde(skip)]
     charge: f32,
+    #[serde(skip)]
     out: Vec<f32>,
 }
 
@@ -37,6 +41,22 @@
 }
 
 impl Mixer {
+    /// 스테이트에서 읽은 믹서에 호스트 설정(샘플레이트)을 옮긴다. 손상된 누적값은 비운다.
+    pub(crate) fn adopt_host(&mut self, host: &Mixer) {
+        self.set_sample_rate(host.sample_rate);
+        let finite = |v: (f32, f32)| v.0.is_finite() && v.1.is_finite();
+        if !(0.0..T_CYCLES_PER_SEC).contains(&self.phase)
+            || !finite(self.sum)
+            || !finite(self.capacitor)
+            || self.count > T_CYCLES_PER_SEC as u32
+        {
+            self.phase = 0.0;
+            self.sum = (0.0, 0.0);
+            self.count = 0;
+            self.capacitor = (0.0, 0.0);
+        }
+    }
+
     /// 출력 샘플레이트(Hz). 프론트엔드가 오디오 버퍼 상태에 따라 조금씩 바꾼다.
     pub fn set_sample_rate(&mut self, rate: f64) {
         let rate = rate.clamp(8_000.0, 192_000.0);
--- a/crates/gb-core/src/apu/mod.rs
+++ b/crates/gb-core/src/apu/mod.rs
@@ -32,7 +32,7 @@
     0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, // 빈 칸
 ];
 
-#[derive(Debug, Clone)]
+#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
 pub struct Apu {
     cgb: bool,
     power: bool,
@@ -313,6 +313,17 @@
         self.mixer.set_sample_rate(rate);
     }
 
+    /// 스테이트에서 읽은 APU에 호스트 설정을 옮기고, 레지스터 비트 폭을 넘는 값을 감싼다.
+    pub(crate) fn adopt_host(&mut self, host: &Apu) {
+        self.mixer.adopt_host(&host.mixer);
+        self.fs_step &= 7;
+        self.sweep.sanitize();
+        self.ch1.sanitize();
+        self.ch2.sanitize();
+        self.ch3.sanitize();
+        self.ch4.sanitize();
+    }
+
     /// 쌓인 인터리브 스테레오 샘플을 `out` 뒤에 붙인다.
     pub fn drain(&mut self, out: &mut Vec<f32>) {
         self.mixer.drain(out);
--- a/crates/gb-core/src/apu/noise.rs
+++ b/crates/gb-core/src/apu/noise.rs
@@ -2,7 +2,7 @@
 
 use super::channel::{Envelope, Length};
 
-#[derive(Debug, Clone, Default)]
+#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
 pub struct Noise {
     pub enabled: bool,
     pub length: Length,
@@ -16,6 +16,14 @@
 }
 
 impl Noise {
+    /// 손상된 스테이트 때문에 패닉하지 않게 레지스터 비트 폭을 넘는 값을 감싼다.
+    pub(crate) fn sanitize(&mut self) {
+        self.shift &= 0x0F;
+        self.divisor &= 7;
+        self.lfsr &= 0x7FFF;
+        self.env.sanitize();
+    }
+
     pub fn reg(&self) -> u8 {
         self.shift << 4 | u8::from(self.narrow) << 3 | self.divisor
     }
--- a/crates/gb-core/src/apu/square.rs
+++ b/crates/gb-core/src/apu/square.rs
@@ -5,7 +5,7 @@
 /// 듀티별 8단계 파형 (12.5%, 25%, 50%, 75%).
 const DUTY: [u8; 4] = [0b0000_0001, 0b1000_0001, 0b1000_0111, 0b0111_1110];
 
-#[derive(Debug, Clone, Default)]
+#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
 pub struct Square {
     pub enabled: bool,
     pub duty: u8,
@@ -18,6 +18,14 @@
 }
 
 impl Square {
+    /// 손상된 스테이트 때문에 패닉하지 않게 레지스터 비트 폭을 넘는 값을 감싼다.
+    pub(crate) fn sanitize(&mut self) {
+        self.duty &= 3;
+        self.step &= 7;
+        self.freq &= 0x7FF;
+        self.env.sanitize();
+    }
+
     /// 한 듀티 단계의 길이(T-사이클).
     fn period(&self) -> u32 {
         (2048 - u32::from(self.freq)) * 4
@@ -53,7 +61,7 @@
 }
 
 /// 채널 1 주파수 스윕 (NR10).
-#[derive(Debug, Clone, Default)]
+#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
 pub struct Sweep {
     period: u8,
     negate: bool,
@@ -66,6 +74,13 @@
 }
 
 impl Sweep {
+    /// 손상된 스테이트 때문에 패닉하지 않게 레지스터 비트 폭을 넘는 값을 감싼다.
+    pub(crate) fn sanitize(&mut self) {
+        self.period &= 7;
+        self.shift &= 7;
+        self.shadow &= 0x7FF;
+    }
+
     pub fn reg(&self) -> u8 {
         self.period << 4 | u8::from(self.negate) << 3 | self.shift
     }
--- a/crates/gb-core/src/apu/wave.rs
+++ b/crates/gb-core/src/apu/wave.rs
@@ -11,7 +11,7 @@
 /// 남은 타이머가 이 값 이하일 때(다음 2 MHz 클록에 읽을 때) 다시 트리거하면 웨이브 RAM이 망가진다.
 const CORRUPTION_WINDOW: u32 = 2;
 
-#[derive(Debug, Clone)]
+#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
 pub struct Wave {
     /// CGB는 채널이 켜져 있어도 웨이브 RAM(현재 바이트)에 언제나 접근할 수 있고, 다시 트리거해도 망가지지 않는다.
     pub cgb: bool,
@@ -50,6 +50,13 @@
 }
 
 impl Wave {
+    /// 손상된 스테이트 때문에 패닉하지 않게 레지스터 비트 폭을 넘는 값을 감싼다.
+    pub(crate) fn sanitize(&mut self) {
+        self.freq &= 0x7FF;
+        self.level &= 3;
+        self.position &= 31;
+    }
+
     /// 한 샘플의 길이(T-사이클).
     fn period(&self) -> u32 {
         (2048 - u32::from(self.freq)) * 2
--- a/crates/gb-core/src/bus.rs
+++ b/crates/gb-core/src/bus.rs
@@ -28,7 +28,7 @@
 const OAM_LEN: u16 = 0xA0;
 
 /// OAM DMA (Pan Docs "OAM DMA Transfer").
-#[derive(Debug, Clone, Default)]
+#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
 struct OamDma {
     /// 마지막으로 쓴 FF46 값.
     reg: u8,
@@ -39,7 +39,7 @@
 }
 
 /// CGB VRAM DMA (Pan Docs "VRAM DMA Transfers").
-#[derive(Debug, Clone, Default)]
+#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
 struct Hdma {
     source: u16,
     /// VRAM 안의 오프셋(0x0000–0x1FF0).
@@ -50,6 +50,7 @@
     hblank: bool,
 }
 
+#[derive(serde::Serialize, serde::Deserialize)]
 pub struct Bus {
     cgb: bool,
     cart: Cartridge,
@@ -59,11 +60,14 @@
     serial: Serial,
     joypad: Joypad,
     /// 뱅크 0–7 (DMG는 0과 1만 쓴다).
+    #[serde(with = "crate::state::bytes::boxed")]
     wram: Box<[u8; 0x8000]>,
     /// SVBK 하위 3비트.
     wram_bank: u8,
+    #[serde(with = "crate::state::bytes")]
     hram: [u8; 0x7F],
     /// 아직 구현하지 않은 I/O 레지스터(0xFF00–0xFF7F)는 쓴 값을 그대로 보관한다.
+    #[serde(with = "crate::state::bytes")]
     io: [u8; 0x80],
     ie: u8,
     if_: u8,
@@ -126,6 +130,27 @@
         }
     }
 
+    /// 스테이트에서 읽은 버스에 지금 버스의 ROM과 호스트 설정을 옮기고, 인덱스로 쓰는 값을 범위 안으로
+    /// 맞춘다. 다른 기기·카트리지의 상태면 거부한다. `host`는 바꾸지 않는다.
+    pub(crate) fn adopt_host(&mut self, host: &Bus) -> Result<(), String> {
+        if self.cgb != host.cgb {
+            return Err("기기 종류가 다릅니다".into());
+        }
+        self.cart.adopt_rom(&host.cart)?;
+        self.apu.adopt_host(&host.apu);
+        self.sanitize();
+        Ok(())
+    }
+
+    /// 손상된 스테이트 때문에 패닉하지 않게 인덱스로 쓰는 값을 하드웨어 범위로 감싼다.
+    fn sanitize(&mut self) {
+        self.wram_bank &= 0x07;
+        self.hdma.source &= 0xFFF0;
+        self.hdma.dest &= 0x1FF0;
+        self.hdma.remaining &= 0x7F;
+        self.ppu.sanitize();
+    }
+
     pub fn cartridge(&self) -> &Cartridge {
         &self.cart
     }
--- a/crates/gb-core/src/cartridge.rs
+++ b/crates/gb-core/src/cartridge.rs
@@ -3,6 +3,7 @@
 //! ROM-only, MBC1, MBC3(+RTC), MBC5의 ROM/RAM 뱅크 전환과 배터리 세이브를 지원한다.
 
 use crate::rtc::{self, Rtc};
+use crate::state::RomId;
 use std::fmt;
 
 const HEADER_END: usize = 0x0150;
@@ -28,7 +29,7 @@
 
 impl std::error::Error for CartError {}
 
-#[derive(Debug, Clone, PartialEq, Eq)]
+#[derive(Debug, Clone, PartialEq, Eq, Default)]
 pub struct Header {
     pub title: String,
     pub cgb_flag: u8,
@@ -60,7 +61,7 @@
 }
 
 /// 메모리 뱅크 컨트롤러 (Pan Docs "MBCs"). 직렬화를 위해 trait 객체 대신 enum으로 둔다.
-#[derive(Debug, Clone, Copy, PartialEq, Eq)]
+#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
 enum Mbc {
     /// ROM-only (+RAM). 뱅크 전환 없음.
     None,
@@ -72,15 +73,21 @@
     Mbc5 { rom_bank: u16, ram_bank: u8 },
 }
 
+#[derive(serde::Serialize, serde::Deserialize)]
 pub struct Cartridge {
+    /// ROM과 헤더는 스테이트에 넣지 않는다. 불러올 때 지금 카트리지의 것을 쓴다.
+    #[serde(skip)]
     header: Header,
+    #[serde(skip)]
     rom: Vec<u8>,
     ram: Vec<u8>,
     ram_enabled: bool,
     mbc: Mbc,
     /// 배터리가 있어 외부 RAM을 세이브 파일로 남겨야 하는 카트리지인지.
     battery: bool,
-    /// 마지막으로 확인한 뒤 외부 RAM(또는 RTC)에 쓰기가 있었는지.
+    /// 마지막으로 확인한 뒤 외부 RAM(또는 RTC)에 쓰기가 있었는지. 스테이트에는 넣지 않는다:
+    /// 불러온 직후 스테이트의 옛 외부 RAM이 세이브 파일을 덮어쓰지 않게 한다.
+    #[serde(skip)]
     ram_dirty: bool,
     /// MBC3+TIMER 카트리지(0x0F, 0x10)의 실시간 시계.
     rtc: Option<Rtc>,
@@ -139,6 +146,39 @@
         &self.header
     }
 
+    /// 세이브 스테이트가 어느 ROM의 것인지 가리는 값: 헤더 전역 체크섬(0x014E–0x014F)과 제목.
+    pub fn rom_id(&self) -> RomId {
+        RomId {
+            global_checksum: u16::from_be_bytes([self.rom[0x014E], self.rom[0x014F]]),
+            title: self.header.title.clone(),
+        }
+    }
+
+    /// 스테이트에서 읽은 카트리지에 지금 카트리지의 ROM·헤더를 붙인다. 구조(MBC 종류, 외부 RAM 크기,
+    /// RTC 유무)가 다르면 거부한다. `host`는 바꾸지 않는다.
+    pub(crate) fn adopt_rom(&mut self, host: &Cartridge) -> Result<(), String> {
+        if std::mem::discriminant(&self.mbc) != std::mem::discriminant(&host.mbc) {
+            return Err("MBC 종류가 다릅니다".into());
+        }
+        if self.ram.len() != host.ram.len() {
+            return Err("외부 RAM 크기가 다릅니다".into());
+        }
+        if self.rtc.is_some() != host.rtc.is_some() {
+            return Err("RTC 유무가 다릅니다".into());
+        }
+        self.battery = host.battery;
+        self.header = host.header.clone();
+        self.rom = host.rom.clone();
+        Ok(())
+    }
+
+    /// RTC가 있으면 `seconds`초 진행한다 (스테이트를 저장한 뒤 지난 실제 시간).
+    pub(crate) fn advance_rtc(&mut self, seconds: u64) {
+        if let Some(rtc) = &mut self.rtc {
+            rtc.advance_offline(seconds);
+        }
+    }
+
     fn rom_byte(&self, bank: usize, addr: u16) -> u8 {
         let banks = (self.rom.len() / 0x4000).max(1);
         let offset = (bank % banks) * 0x4000 + usize::from(addr & 0x3FFF);
--- a/crates/gb-core/src/cpu/mod.rs
+++ b/crates/gb-core/src/cpu/mod.rs
@@ -27,13 +27,13 @@
 pub const IE_ADDR: u16 = 0xFFFF;
 
 /// 정의되지 않은 옵코드를 만나 CPU가 멈춘 위치.
-#[derive(Debug, Clone, Copy, PartialEq, Eq)]
+#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
 pub struct IllegalOpcode {
     pub pc: u16,
     pub opcode: u8,
 }
 
-#[derive(Debug, Clone)]
+#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
 pub struct Cpu {
     pub regs: Registers,
     ime: bool,
--- a/crates/gb-core/src/cpu/registers.rs
+++ b/crates/gb-core/src/cpu/registers.rs
@@ -10,7 +10,7 @@
     pub const C: u8 = 0x10;
 }
 
-#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
+#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
 pub struct Registers {
     pub a: u8,
     pub f: u8,
--- a/crates/gb-core/src/gameboy.rs
+++ b/crates/gb-core/src/gameboy.rs
@@ -6,7 +6,9 @@
 use crate::joypad::Button;
 use crate::model::Model;
 use crate::ppu;
-
+use crate::state::{self, RomId, StateError};
+
+#[derive(serde::Serialize, serde::Deserialize)]
 pub struct GameBoy {
     cpu: Cpu,
     bus: Bus,
@@ -95,6 +97,38 @@
     /// 꺼내 가지 않으면 오래된 소리는 버린다(약 1초 분량까지만 남긴다).
     pub fn drain_audio(&mut self, out: &mut Vec<f32>) {
         self.bus.apu_mut().drain(out);
+    }
+
+    /// 세이브 스테이트가 어느 ROM의 것인지 가리는 값.
+    pub fn rom_id(&self) -> RomId {
+        self.bus.cartridge().rom_id()
+    }
+
+    /// 지금 상태 전체를 스테이트 파일 내용으로 만든다 (스펙 §4.8). ROM은 넣지 않는다.
+    /// `now_unix`는 저장 시각으로 기록되어, 불러올 때 RTC를 실제 시간에 맞추는 데 쓴다.
+    pub fn save_state(&self, now_unix: u64) -> Vec<u8> {
+        state::encode(&self.rom_id(), now_unix, self)
+    }
+
+    /// 스테이트를 불러온다. 다른 ROM·형식·손상된 내용이면 오류를 돌려주고 지금 상태를 그대로 둔다.
+    /// RTC가 있으면 저장 시각부터 `now_unix`까지 지난 시간만큼 시계를 진행한다.
+    /// 소리 출력 샘플레이트는 지금 값을 유지하고, 아직 꺼내지 않은 소리는 버린다.
+    pub fn load_state(&mut self, data: &[u8], now_unix: u64) -> Result<(), StateError> {
+        let (saved_at, body) = state::split(data, &self.rom_id())?;
+        let mut loaded: GameBoy = state::decode(body)?;
+        if loaded.model != self.model {
+            return Err(StateError::Corrupt("기기 종류가 다릅니다".into()));
+        }
+        loaded
+            .bus
+            .adopt_host(&self.bus)
+            .map_err(StateError::Corrupt)?;
+        loaded
+            .bus
+            .cartridge_mut()
+            .advance_rtc(now_unix.saturating_sub(saved_at));
+        *self = loaded;
+        Ok(())
     }
 
     pub fn debug(&self) -> DebugView<'_> {
--- a/crates/gb-core/src/joypad.rs
+++ b/crates/gb-core/src/joypad.rs
@@ -45,7 +45,7 @@
     }
 }
 
-#[derive(Debug, Clone)]
+#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
 pub struct Joypad {
     /// P1의 선택 비트(4–5). 부트 직후에는 둘 다 1(아무 줄도 선택하지 않음).
     select: u8,
--- a/crates/gb-core/src/model.rs
+++ b/crates/gb-core/src/model.rs
@@ -1,5 +1,5 @@
 /// 에뮬레이션할 기기.
-#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
+#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
 pub enum Model {
     /// 카트리지 헤더의 CGB 플래그로 고른다.
     #[default]
--- a/crates/gb-core/src/ppu.rs
+++ b/crates/gb-core/src/ppu.rs
@@ -40,7 +40,7 @@
 /// DMG 기본 팔레트(밝은 색부터). 각 값은 0xRRGGBBAA.
 pub const DEFAULT_DMG_PALETTE: [u32; 4] = [0xE0F8D0FF, 0x88C070FF, 0x346856FF, 0x081820FF];
 
-#[derive(Debug, Clone, Copy, PartialEq, Eq)]
+#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
 enum Mode {
     HBlank = 0,
     VBlank = 1,
@@ -48,22 +48,26 @@
     Drawing = 3,
 }
 
-#[derive(Debug, Clone)]
+#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
 pub struct Ppu {
     /// CGB 모드인지. DMG 모드에서는 VRAM 뱅크 1과 컬러 팔레트를 쓰지 않는다.
     cgb: bool,
     /// 뱅크 0(0x0000–0x1FFF)과 CGB 뱅크 1(0x2000–0x3FFF).
+    #[serde(with = "crate::state::bytes::boxed")]
     vram: Box<[u8; 0x4000]>,
     /// CPU가 보는 VRAM 뱅크(VBK 비트 0).
     vram_bank: u8,
     /// CGB 팔레트 RAM: 팔레트 8개 × 색 4개 × 2바이트(15비트 BGR, 리틀 엔디언).
+    #[serde(with = "crate::state::bytes")]
     bg_palettes: [u8; 64],
+    #[serde(with = "crate::state::bytes")]
     obj_palettes: [u8; 64],
     /// 팔레트 인덱스(비트 0–5)와 자동 증가(비트 7).
     bcps: u8,
     ocps: u8,
     /// 보이는 줄의 HBlank(모드 0)에 들어갔는지. HBlank DMA가 쓴다. 읽으면 초기화된다.
     hblank_started: bool,
+    #[serde(with = "crate::state::bytes::boxed")]
     oam: Box<[u8; 0xA0]>,
     lcdc: u8,
     /// STAT의 쓰기 가능한 비트(3–6). 모드와 LYC 일치 비트는 읽을 때 만든다.
@@ -90,6 +94,7 @@
     stat_line: bool,
     frame_ready: bool,
     palette: [u32; 4],
+    #[serde(with = "crate::state::words")]
     framebuffer: Box<[u32; SCREEN_WIDTH * SCREEN_HEIGHT]>,
 }
 
@@ -100,6 +105,21 @@
 }
 
 impl Ppu {
+    /// 손상된 스테이트 때문에 패닉하지 않게 인덱스로 쓰는 값을 하드웨어 범위로 감싼다.
+    pub(crate) fn sanitize(&mut self) {
+        self.vram_bank &= 0x01;
+        // LCD가 켜져 있으면 줄 안의 위치(2 dot 단위), 꺼져 있으면 프레임 안의 위치다.
+        self.dot = if self.lcdc & 0x80 != 0 {
+            (self.dot % DOTS_PER_LINE) & !1
+        } else {
+            self.dot % DOTS_PER_FRAME
+        };
+        self.ly %= LINES_PER_FRAME;
+        if self.ly >= SCREEN_HEIGHT as u8 {
+            self.mode = Mode::VBlank;
+        }
+    }
+
     /// 부트 ROM 직후 상태. CGB 팔레트 RAM은 흰색으로 채운다.
     pub fn new(cgb: bool) -> Self {
         Self {
--- a/crates/gb-core/src/rtc.rs
+++ b/crates/gb-core/src/rtc.rs
@@ -16,7 +16,7 @@
 const DH_CARRY: u8 = 0x80;
 
 /// RTC 레지스터 5개 (S, M, H, DL, DH).
-#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
+#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
 pub struct RtcRegs {
     pub seconds: u8,
     pub minutes: u8,
@@ -126,7 +126,7 @@
     }
 }
 
-#[derive(Debug, Clone, Default)]
+#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
 pub struct Rtc {
     regs: RtcRegs,
     latched: RtcRegs,
--- a/crates/gb-core/src/serial.rs
+++ b/crates/gb-core/src/serial.rs
@@ -9,10 +9,12 @@
 /// 보관하는 시리얼 출력의 최대 바이트 수. 넘치면 오래된 쪽부터 버린다.
 pub const OUTPUT_LIMIT: usize = 64 * 1024;
 
-#[derive(Debug, Clone, Default)]
+#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
 pub struct Serial {
     sb: u8,
     sc: u8,
+    /// 테스트 ROM 결과 출력용이라 스테이트에 넣지 않는다.
+    #[serde(skip)]
     output: Vec<u8>,
 }
 
--- a/crates/gb-core/src/timer.rs
+++ b/crates/gb-core/src/timer.rs
@@ -12,7 +12,7 @@
 pub const TAC: u16 = 0xFF07;
 
 /// TIMA 오버플로 후 TMA를 다시 싣는 과정.
-#[derive(Debug, Clone, Copy, PartialEq, Eq)]
+#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
 enum Reload {
     None,
     /// 오버플로가 났고 TIMA는 0이다. 다음 M-사이클에 TMA를 싣는다. 이때 TIMA를 쓰면 취소된다.
@@ -21,7 +21,7 @@
     Reloading,
 }
 
-#[derive(Debug, Clone)]
+#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
 pub struct Timer {
     counter: u16,
     tima: u8,
--- /dev/null
+++ b/crates/gb-core/tests/save_state.rs
@@ -0,0 +1,404 @@
+//! 세이브 스테이트 인수 테스트 (스펙 §4.8). 테스트 ROM을 실행하다 저장하고, 그대로 계속한 결과와
+//! 불러와서 계속한 결과가 화면·소리 모두 비트 단위로 같아야 한다. 상태 하나라도 빠지면 어긋난다.
+
+mod common;
+
+use gb_core::{GameBoy, Model};
+
+const SAMPLE_RATE: f64 = 48_000.0;
+const SAVED_AT: u64 = 1_000;
+
+/// `frames` 프레임을 돌리며 프레임마다의 화면, 전체 소리, 끝난 뒤의 전체 상태를 모은다.
+fn record(gb: &mut GameBoy, frames: u32) -> (Vec<Vec<u32>>, Vec<u32>, Vec<u8>) {
+    let mut screens = Vec::new();
+    let mut audio = Vec::new();
+    for _ in 0..frames {
+        gb.run_frame();
+        screens.push(gb.framebuffer().to_vec());
+        gb.drain_audio(&mut audio);
+    }
+    let audio = audio.iter().map(|s| s.to_bits()).collect();
+    (screens, audio, gb.save_state(SAVED_AT))
+}
+
+fn first_difference<T: PartialEq>(a: &[T], b: &[T]) -> Option<usize> {
+    a.iter()
+        .zip(b)
+        .position(|(x, y)| x != y)
+        .or_else(|| (a.len() != b.len()).then_some(a.len().min(b.len())))
+}
+
+/// 위치를 기억하는 아주 작은 어셈블러. 상대 점프만 라벨로 계산한다.
+struct Asm(Vec<u8>);
+
+impl Asm {
+    fn op(&mut self, bytes: &[u8]) -> &mut Self {
+        self.0.extend_from_slice(bytes);
+        self
+    }
+    fn here(&self) -> usize {
+        self.0.len()
+    }
+    /// `opcode`(JR 계열) 뒤에 `target`까지의 상대 거리.
+    fn jr(&mut self, opcode: u8, target: usize) -> &mut Self {
+        let offset = target as isize - (self.here() as isize + 2);
+        self.op(&[opcode, offset as i8 as u8])
+    }
+    /// LD A,value ; LDH (reg),A
+    fn set(&mut self, reg: u8, value: u8) -> &mut Self {
+        self.op(&[0x3E, value, 0xE0, reg])
+    }
+    /// `start`부터 `count`바이트(0이면 256)를 L xor H 무늬로 채운다.
+    fn fill(&mut self, start: u16, count: u8) -> &mut Self {
+        let [lo, hi] = start.to_le_bytes();
+        self.op(&[0x21, lo, hi, 0x06, count]); // LD HL,start; LD B,count
+        let top = self.here();
+        self.op(&[0x7D, 0xAC, 0x22, 0x05]); // LD A,L; XOR H; LD (HL+),A; DEC B
+        self.jr(0x20, top)
+    }
+}
+
+/// 스테이트에 들어가야 하는 하드웨어 상태를 한꺼번에 쓰는 합성 ROM.
+/// - 소리: 네 채널 모두 길이·엔벨로프·스윕을 켜고 약 60ms마다 다시 울린다.
+/// - 화면: 무늬 타일, 배경·윈도우·스프라이트, 타이머 인터럽트마다 SCX를, 루프마다 SCY·WX를 바꾼다.
+/// - CGB: 팔레트, 2배속, 루프마다 WRAM 뱅크를 바꿔 값을 쓰고 그 WRAM에서 HBlank HDMA를 다시 건다.
+fn stress_rom(cgb: bool) -> Vec<u8> {
+    let mut rom = vec![0u8; 0x8000];
+    rom[0x0100..0x0104].copy_from_slice(&[0x00, 0xC3, 0x50, 0x01]); // NOP; JP 0x0150
+    rom[0x0134..0x013F].copy_from_slice(b"STATESTRESS");
+    rom[0x0143] = if cgb { 0x80 } else { 0x00 };
+    // 타이머 인터럽트: PUSH AF; LDH A,(SCX); INC A; LDH (SCX),A; POP AF; RETI
+    rom[0x0050..0x0058].copy_from_slice(&[0xF5, 0xF0, 0x43, 0x3C, 0xE0, 0x43, 0xF1, 0xD9]);
+
+    let mut a = Asm(Vec::new());
+    a.op(&[0xF3]).set(0x40, 0x00); // DI; LCD 끄기
+    for page in 0..32u16 {
+        a.fill(0x8000 + page * 0x100, 0); // 타일 데이터와 두 타일맵
+    }
+    a.fill(0xFE00, 0xA0); // OAM
+    a.fill(0xFF30, 0x10); // 파형 RAM
+    if cgb {
+        a.set(0x68, 0x80).set(0x6A, 0x80); // BCPS·OCPS 자동 증가
+        a.op(&[0x06, 0x40]); // LD B,64
+        let top = a.here();
+        a.op(&[0x78, 0xE0, 0x69, 0x2F, 0xE0, 0x6B, 0x05]); // LD A,B; LDH (BCPD),A; CPL; LDH (OCPD),A; DEC B
+        a.jr(0x20, top);
+        // WRAM 뱅크 1–7의 0xD000–0xD7FF를 뱅크마다 다른 무늬(L xor H xor 뱅크)로 채운다
+        a.op(&[0x0E, 0x07]); // LD C,7
+        let bank = a.here();
+        a.op(&[0x79, 0xE0, 0x70, 0x21, 0x00, 0xD0, 0x16, 0x08]); // LD A,C; LDH (SVBK),A; LD HL,D000; LD D,8
+        let page = a.here();
+        a.op(&[0x06, 0x00]); // LD B,0
+        let byte = a.here();
+        a.op(&[0x7D, 0xA9, 0xAC, 0x22, 0x05]); // LD A,L; XOR C; XOR H; LD (HL+),A; DEC B
+        a.jr(0x20, byte);
+        a.op(&[0x15]).jr(0x20, page); // DEC D
+        a.op(&[0x0D]).jr(0x20, bank); // DEC C
+        a.set(0x4D, 0x01).op(&[0x10, 0x00]); // KEY1 준비; STOP → 2배속
+    }
+    // 소리
+    a.set(0x26, 0x80).set(0x24, 0x77).set(0x25, 0xFF);
+    a.set(0x10, 0x19)
+        .set(0x11, 0x80)
+        .set(0x12, 0xF3)
+        .set(0x13, 0x00);
+    a.set(0x16, 0x41).set(0x17, 0x82).set(0x18, 0x80);
+    a.set(0x1A, 0x80)
+        .set(0x1B, 0x00)
+        .set(0x1C, 0x20)
+        .set(0x1D, 0x40);
+    a.set(0x20, 0x00).set(0x21, 0xF1).set(0x22, 0x55);
+    // 화면: BGP, OBP0, WY, WX, LCDC(켜기, 윈도우 맵 9C00, 윈도우, 타일 8000, OBJ, BG)
+    a.set(0x47, 0xE4)
+        .set(0x48, 0xD2)
+        .set(0x4A, 0x20)
+        .set(0x4B, 0x30);
+    a.set(0x40, 0xF3);
+    // 타이머 인터럽트
+    a.set(0x07, 0x05).set(0xFF, 0x04).op(&[0xFB]); // TAC; IE=타이머; EI
+
+    let main = a.here();
+    a.op(&[0x0E, 0x10]); // LD C,16
+    let outer = a.here();
+    a.op(&[0x06, 0x00]); // LD B,0
+    let inner = a.here();
+    a.op(&[0x05]).jr(0x20, inner); // DEC B; JR NZ
+    a.op(&[0x0D]).jr(0x20, outer); // DEC C; JR NZ
+    a.set(0x14, 0xC7)
+        .set(0x19, 0xC6)
+        .set(0x1E, 0xC5)
+        .set(0x23, 0xC0); // 네 채널 다시 울리기
+    a.op(&[0xF0, 0x42, 0x3C, 0xE0, 0x42]); // SCY++
+    a.op(&[0xF0, 0x4B, 0x3C, 0xE0, 0x4B]); // WX++
+    if cgb {
+        // SVBK = (SVBK + 1) & 7; LD (0xD000),A — 뱅크마다 다른 값을 남긴다
+        a.op(&[0xF0, 0x70, 0x3C, 0xE6, 0x07, 0xE0, 0x70, 0xEA, 0x00, 0xD0]);
+        // 0xD000(바뀌는 WRAM)에서 VRAM 0x8800으로 HBlank HDMA 128블록
+        a.set(0x51, 0xD0)
+            .set(0x52, 0x00)
+            .set(0x53, 0x08)
+            .set(0x54, 0x00)
+            .set(0x55, 0xFF);
+    }
+    a.jr(0x18, main);
+
+    rom[0x0150..0x0150 + a.0.len()].copy_from_slice(&a.0);
+    rom
+}
+
+/// `rom`을 `warmup` 프레임 돌린 뒤 저장하고 이어서 비교한다 ([`resumes_identically`]와 같다).
+fn rom_resumes_identically(name: &str, rom: Vec<u8>, model: Model, warmup: u32, frames: u32) {
+    compare_resume(name, rom, model, warmup, frames);
+}
+
+/// `warmup` 프레임 뒤에 저장하고, 이어서 `frames` 프레임을 세 번 돌려 비교한다:
+/// 그대로 계속, 같은 인스턴스에 불러와서 계속, 새 인스턴스에 불러와서 계속.
+fn resumes_identically(rel: &str, model: Model, warmup: u32, frames: u32) {
+    let Some(rom) = common::load_rom(rel) else {
+        return;
+    };
+    compare_resume(rel, rom, model, warmup, frames);
+}
+
+fn compare_resume(rel: &str, rom: Vec<u8>, model: Model, warmup: u32, frames: u32) {
+    let mut gb = GameBoy::new(rom.clone(), model).expect("테스트 ROM 로드");
+    gb.set_sample_rate(SAMPLE_RATE);
+    for _ in 0..warmup {
+        gb.run_frame();
+    }
+    gb.drain_audio(&mut Vec::new());
+    let state = gb.save_state(SAVED_AT);
+
+    let expected = record(&mut gb, frames);
+
+    gb.load_state(&state, SAVED_AT)
+        .expect("같은 인스턴스에 불러오기");
+    let same = record(&mut gb, frames);
+
+    let mut fresh = GameBoy::new(rom, model).expect("테스트 ROM 로드");
+    fresh.set_sample_rate(SAMPLE_RATE);
+    fresh
+        .load_state(&state, SAVED_AT)
+        .expect("새 인스턴스에 불러오기");
+    let other = record(&mut fresh, frames);
+
+    for (name, got) in [("같은 인스턴스", same), ("새 인스턴스", other)] {
+        assert!(
+            got.0 == expected.0,
+            "{rel} ({name}): {}번째 프레임 화면이 다릅니다",
+            first_difference(&got.0, &expected.0).unwrap_or(0)
+        );
+        assert!(
+            got.1 == expected.1,
+            "{rel} ({name}): 소리 샘플 {}번째부터 다릅니다 (길이 {} / {})",
+            first_difference(&got.1, &expected.1).unwrap_or(0),
+            got.1.len(),
+            expected.1.len()
+        );
+        assert!(
+            got.2 == expected.2,
+            "{rel} ({name}): 끝난 뒤 상태가 {}번째 바이트부터 다릅니다",
+            first_difference(&got.2, &expected.2).unwrap_or(0)
+        );
+    }
+}
+
+#[test]
+fn dmg_acid2_ppu() {
+    resumes_identically("dmg-acid2/dmg-acid2.gb", Model::Dmg, 2, 30);
+}
+
+#[test]
+fn cgb_acid2_ppu() {
+    resumes_identically("cgb-acid2/cgb-acid2.gbc", Model::Cgb, 2, 30);
+}
+
+#[test]
+fn dmg_sound_wave_channel() {
+    resumes_identically(
+        "blargg/dmg_sound/rom_singles/09-wave read while on.gb",
+        Model::Dmg,
+        30,
+        120,
+    );
+}
+
+#[test]
+fn dmg_sound_sweep() {
+    resumes_identically(
+        "blargg/dmg_sound/rom_singles/05-sweep details.gb",
+        Model::Dmg,
+        20,
+        120,
+    );
+}
+
+#[test]
+fn cgb_sound_noise_and_length() {
+    resumes_identically(
+        "blargg/cgb_sound/rom_singles/08-len ctr during power.gb",
+        Model::Cgb,
+        20,
+        120,
+    );
+}
+
+#[test]
+fn cpu_interrupts_and_timer() {
+    resumes_identically(
+        "blargg/cpu_instrs/individual/02-interrupts.gb",
+        Model::Dmg,
+        10,
+        60,
+    );
+}
+
+#[test]
+fn mbc1_banking() {
+    resumes_identically(
+        "mooneye/emulator-only/mbc1/bits_bank2.gb",
+        Model::Dmg,
+        1,
+        30,
+    );
+}
+
+#[test]
+fn mbc5_banking() {
+    resumes_identically("mooneye/emulator-only/mbc5/rom_64Mb.gb", Model::Dmg, 1, 30);
+}
+
+#[test]
+fn timer_div() {
+    resumes_identically("mooneye/acceptance/timer/div_write.gb", Model::Dmg, 1, 30);
+}
+
+#[test]
+fn dmg_stress_rom() {
+    rom_resumes_identically("DMG 부하 ROM", stress_rom(false), Model::Dmg, 45, 180);
+}
+
+#[test]
+fn cgb_stress_rom() {
+    rom_resumes_identically("CGB 부하 ROM", stress_rom(true), Model::Cgb, 45, 180);
+}
+
+/// v1 스테이트 고정 파일. 형식이 바뀌어도 이전 스테이트를 읽을 수 있어야 한다(스펙 §4.8).
+/// 파일이 없을 때 `ARAGORN_WRITE_STATE_FIXTURE=1`로 실행하면 지금 코드로 만든다.
+#[test]
+fn version_1_state_still_loads() {
+    let path = concat!(
+        env!("CARGO_MANIFEST_DIR"),
+        "/tests/fixtures/state-v1-cgb.argn"
+    );
+    let rom = stress_rom(true);
+    if std::env::var_os("ARAGORN_WRITE_STATE_FIXTURE").is_some() {
+        let mut gb = GameBoy::new(rom.clone(), Model::Cgb).unwrap();
+        for _ in 0..30 {
+            gb.run_frame();
+        }
+        let path = std::path::Path::new(path);
+        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
+        std::fs::write(path, gb.save_state(SAVED_AT)).unwrap();
+    }
+    let data = std::fs::read(path).expect("v1 스테이트 고정 파일");
+    let mut gb = GameBoy::new(rom, Model::Cgb).unwrap();
+    gb.load_state(&data, SAVED_AT)
+        .expect("v1 스테이트는 앞으로도 읽을 수 있어야 한다");
+    for _ in 0..10 {
+        gb.run_frame();
+    }
+}
+
+#[test]
+fn stress_rom_actually_makes_sound_and_varied_screens() {
+    for (cgb, model) in [(false, Model::Dmg), (true, Model::Cgb)] {
+        let mut gb = GameBoy::new(stress_rom(cgb), model).unwrap();
+        let mut audio = Vec::new();
+        let mut screens = std::collections::HashSet::new();
+        for _ in 0..120 {
+            gb.run_frame();
+            gb.drain_audio(&mut audio);
+            screens.insert(gb.framebuffer().to_vec());
+        }
+        let loud = audio.iter().filter(|s| s.abs() > 0.01).count();
+        assert!(
+            loud > audio.len() / 4,
+            "{model:?}: 소리가 거의 없다 ({loud}/{})",
+            audio.len()
+        );
+        assert!(
+            screens.len() > 30,
+            "{model:?}: 화면이 거의 변하지 않는다 ({})",
+            screens.len()
+        );
+    }
+}
+
+/// 값 트리에서 정수 잎마다 경로를 모은다. 경로는 (맵 값 / 배열 원소) 인덱스의 나열이다.
+fn integer_paths(value: &rmpv::Value, path: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
+    match value {
+        rmpv::Value::Integer(_) => out.push(path.clone()),
+        rmpv::Value::Array(items) => {
+            for (i, item) in items.iter().enumerate() {
+                path.push(i);
+                integer_paths(item, path, out);
+                path.pop();
+            }
+        }
+        rmpv::Value::Map(entries) => {
+            for (i, (_, item)) in entries.iter().enumerate() {
+                path.push(i);
+                integer_paths(item, path, out);
+                path.pop();
+            }
+        }
+        _ => {}
+    }
+}
+
+fn at<'a>(value: &'a mut rmpv::Value, path: &[usize]) -> &'a mut rmpv::Value {
+    path.iter().fold(value, |v, &i| match v {
+        rmpv::Value::Array(items) => &mut items[i],
+        rmpv::Value::Map(entries) => &mut entries[i].1,
+        _ => unreachable!(),
+    })
+}
+
+/// 손상된 스테이트: 본문의 정수를 하나씩 극단값으로 바꿔 불러온다. 거부하거나, 받아들이면 몇 프레임
+/// 돌아야 한다. 어느 경우에도 패닉하면 안 된다.
+#[test]
+fn corrupted_integers_never_panic() {
+    for (cgb, model) in [(false, Model::Dmg), (true, Model::Cgb)] {
+        let rom = stress_rom(cgb);
+        let mut gb = GameBoy::new(rom.clone(), model).unwrap();
+        for _ in 0..30 {
+            gb.run_frame();
+        }
+        let state = gb.save_state(SAVED_AT);
+        let header_len = 17 + gb.rom_id().title.len();
+        let (header, body) = state.split_at(header_len);
+        let tree = rmpv::decode::read_value(&mut &body[..]).unwrap();
+        let mut paths = Vec::new();
+        integer_paths(&tree, &mut Vec::new(), &mut paths);
+        assert!(paths.len() > 100, "정수 필드가 너무 적다: {}", paths.len());
+        let mut accepted = 0;
+        for path in &paths {
+            for value in [0u64, 1, 0x7F, 0xFF, 0xFFFF, 0xFFFF_FFFF] {
+                let mut tree = tree.clone();
+                *at(&mut tree, path) = rmpv::Value::from(value);
+                let mut data = header.to_vec();
+                rmpv::encode::write_value(&mut data, &tree).unwrap();
+                let mut target = GameBoy::new(rom.clone(), model).unwrap();
+                if target.load_state(&data, SAVED_AT).is_ok() {
+                    accepted += 1;
+                    for _ in 0..2 {
+                        target.run_frame();
+                    }
+                    target.drain_audio(&mut Vec::new());
+                }
+            }
+        }
+        assert!(accepted > 0);
+    }
+}
```

- [ ] **Step 4: 고정 파일을 만들고 통과하는지 확인한다**

v1 스테이트 고정 파일은 바이너리라 패치에 넣지 않았다. 지금 코드로 만든다(결정적이라 스파이크의 파일과 바이트 단위로 같다).

Run: `ARAGORN_WRITE_STATE_FIXTURE=1 cargo test -q -p gb-core --test save_state version_1`
Expected: `1 passed`, `crates/gb-core/tests/fixtures/state-v1-cgb.argn` 생성 (143,253바이트)

Run: `ARAGORN_REQUIRE_TEST_ROMS=1 cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: core lib 244 passed (+ gameboy 7), save_state 14 passed, clippy 경고 0

- [ ] **Step 5: 커밋한다**

```bash
cargo fmt --all --check
git add -A crates
git commit -m "feat(core): 세이브 스테이트 저장·불러오기

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---
### Task 3: 세션 스테이트 슬롯과 파일 저장소

**Files:** `crates/aragorn-app/src/session.rs`, `crates/aragorn-desktop/src/adapters/fs_save_store.rs`

**Interfaces:**
- Consumes: Task 2 `GameBoy::{save_state, load_state}`, `gb_core::state::peek`
- Produces:
  - `SaveStore::{load_state(slot), save_state(slot, data, thumbnail), load_thumbnail(slot)}`
  - `aragorn_app::session::{STATE_SLOTS, THUMBNAIL_WIDTH, THUMBNAIL_HEIGHT, thumbnail, SlotInfo}`
  - `Session::{save_state(slot), load_state(slot), slot_info(slot)}`
  - `FsSaveStore::state_path(slot)`

- [ ] **Step 1: 테스트 패치를 적용한다**

```diff
--- a/crates/aragorn-app/src/session.rs
+++ b/crates/aragorn-app/src/session.rs
@@ -243,8 +243,11 @@
     use crate::pacing::{FRAME_DURATION, FastForward};
     use std::{
         cell::{Cell, RefCell},
+        collections::HashMap,
         rc::Rc,
     };
+
+    type Slot = (Vec<u8>, Vec<u32>);
 
     /// 테스트용 메모리 저장소. 저장 시도 횟수와 실패 여부를 조절할 수 있다.
     #[derive(Default)]
@@ -253,6 +256,8 @@
         save_attempts: Cell<u32>,
         fail_load: bool,
         fail_save: Cell<bool>,
+        /// 슬롯별 (스테이트, 썸네일)
+        states: RefCell<HashMap<u8, Slot>>,
     }
 
     struct Shared(Rc<MemoryStore>);
@@ -272,6 +277,22 @@
             }
             *self.0.data.borrow_mut() = Some(data.to_vec());
             Ok(())
+        }
+
+        fn load_state(&self, slot: u8) -> io::Result<Option<Vec<u8>>> {
+            Ok(self.0.states.borrow().get(&slot).map(|(d, _)| d.clone()))
+        }
+
+        fn save_state(&self, slot: u8, data: &[u8], thumbnail: &[u32]) -> io::Result<()> {
+            self.0
+                .states
+                .borrow_mut()
+                .insert(slot, (data.to_vec(), thumbnail.to_vec()));
+            Ok(())
+        }
+
+        fn load_thumbnail(&self, slot: u8) -> io::Result<Option<Vec<u32>>> {
+            Ok(self.0.states.borrow().get(&slot).map(|(_, t)| t.clone()))
         }
     }
 
@@ -637,6 +658,117 @@
         );
     }
 
+    #[test]
+    fn thumbnail_averages_2x2_blocks() {
+        let mut screen = vec![0u32; 160 * 144];
+        screen[0] = 0xFF00_0000;
+        screen[1] = 0x0000_00FF;
+        screen[160] = 0xFF00_0000;
+        screen[161] = 0x0000_00FF;
+        screen[160 * 144 - 1] = 0x0404_0404;
+        let thumb = thumbnail(&screen);
+        assert_eq!(thumb.len(), THUMBNAIL_WIDTH * THUMBNAIL_HEIGHT);
+        assert_eq!(thumb[0], 0x7F00_007F);
+        assert_eq!(thumb[thumb.len() - 1], 0x0101_0101);
+    }
+
+    #[test]
+    fn state_slot_restores_game_ram_without_touching_save_file() {
+        let (mut session, store) = session_with(saving_rom(true), MemoryStore::default());
+        session.save_state(3).unwrap();
+        run_frames(&mut session, 2);
+        assert_eq!(session.battery_ram().unwrap()[0], 0x42);
+        assert_eq!(store.save_attempts.get(), 1);
+        session.load_state(3).unwrap();
+        assert_eq!(
+            session.battery_ram().unwrap()[0],
+            0x00,
+            "스테이트의 외부 RAM"
+        );
+        assert_eq!(
+            store.data.borrow().as_ref().unwrap()[0],
+            0x42,
+            "세이브 파일은 그대로"
+        );
+        assert_eq!(
+            store.save_attempts.get(),
+            1,
+            "불러오기만으로는 저장하지 않는다"
+        );
+    }
+
+    #[test]
+    fn loading_after_save_point_does_not_rewrite_save_file() {
+        let (mut session, store) = session_with(saving_rom(true), MemoryStore::default());
+        run_frames(&mut session, 2);
+        session.save_state(0).unwrap();
+        session.load_state(0).unwrap();
+        run_frames(&mut session, 120);
+        assert_eq!(store.save_attempts.get(), 1);
+    }
+
+    #[test]
+    fn empty_or_foreign_slot_reports_and_keeps_game() {
+        let (mut session, _) = session_with(saving_rom(true), MemoryStore::default());
+        assert_eq!(
+            session.load_state(4).unwrap_err(),
+            "슬롯 5이(가) 비어 있습니다"
+        );
+        let store = MemoryStore::default();
+        let other = GameBoy::new(looping_rom(), Model::Auto).unwrap();
+        let mut foreign = other.save_state(0);
+        foreign[17] ^= 0xFF; // 제목 첫 글자를 바꾼다
+        store.states.borrow_mut().insert(0, (foreign, Vec::new()));
+        let (mut session, _) = session_with(saving_rom(true), store);
+        run_frames(&mut session, 2);
+        let err = session.load_state(0).unwrap_err();
+        assert!(err.starts_with("슬롯 1: 다른 게임"), "{err}");
+        assert_eq!(session.battery_ram().unwrap()[0], 0x42);
+    }
+
+    #[test]
+    fn unsaved_game_progress_blocks_state_loading() {
+        let store = MemoryStore::default();
+        store.fail_save.set(true);
+        let (mut session, store) = session_with(saving_rom(true), store);
+        session.save_state(0).unwrap();
+        run_frames(&mut session, 2);
+        assert!(session.load_state(0).is_err());
+        assert_eq!(session.battery_ram().unwrap()[0], 0x42, "불러오지 않았다");
+        store.fail_save.set(false);
+        session.load_state(0).unwrap();
+        assert_eq!(
+            store.data.borrow().as_ref().unwrap()[0],
+            0x42,
+            "먼저 저장했다"
+        );
+    }
+
+    #[test]
+    fn slot_info_shows_save_time_and_thumbnail() {
+        let (mut session, _, clock) = session_at(looping_rom(), MemoryStore::default(), 5_000);
+        assert_eq!(session.slot_info(2), None);
+        session.save_state(2).unwrap();
+        clock.0.set(9_000);
+        let info = session.slot_info(2).unwrap();
+        assert_eq!(info.saved_at, 5_000);
+        assert_eq!(
+            info.thumbnail.map(|t| t.len()),
+            Some(THUMBNAIL_WIDTH * THUMBNAIL_HEIGHT)
+        );
+    }
+
+    #[test]
+    fn state_loading_advances_rtc_by_clock() {
+        let (mut session, _, clock) = session_at(rtc_saving_rom(), MemoryStore::default(), 1_000);
+        session.save_state(0).unwrap();
+        clock.0.set(1_000 + 3 * 60);
+        session.load_state(0).unwrap();
+        let ram = session.battery_ram().unwrap();
+        let rtc = &ram[0x2000..];
+        assert_eq!((rtc[0], rtc[4]), (0, 3), "3분 진행");
+    }
+
     /// MBC3+TIMER+RAM+BATTERY(0x10) 판 `saving_rom(true)`.
     fn rtc_saving_rom() -> Vec<u8> {
         let mut rom = saving_rom(true);
--- a/crates/aragorn-desktop/src/adapters/fs_save_store.rs
+++ b/crates/aragorn-desktop/src/adapters/fs_save_store.rs
@@ -65,6 +65,41 @@
     }
 
     #[test]
+    fn state_slots_sit_next_to_rom() {
+        let store = FsSaveStore::for_rom(Path::new("roms/pokemon crystal.gbc"));
+        assert_eq!(store.state_path(0), Path::new("roms/pokemon crystal.ss0"));
+        assert_eq!(store.state_path(9), Path::new("roms/pokemon crystal.ss9"));
+    }
+
+    #[test]
+    fn state_and_thumbnail_round_trip() {
+        let dir = tempfile::tempdir().unwrap();
+        let store = FsSaveStore::for_rom(&dir.path().join("game.gbc"));
+        assert_eq!(store.load_state(4).unwrap(), None);
+        assert_eq!(store.load_thumbnail(4).unwrap(), None);
+        let thumb: Vec<u32> = (0..(THUMBNAIL_WIDTH * THUMBNAIL_HEIGHT) as u32).collect();
+        store.save_state(4, b"ARGN...", &thumb).unwrap();
+        assert_eq!(
+            store.load_state(4).unwrap().as_deref(),
+            Some(&b"ARGN..."[..])
+        );
+        assert_eq!(store.load_thumbnail(4).unwrap(), Some(thumb));
+        assert!(!dir.path().join("game.ss4.tmp").exists());
+        assert!(
+            !dir.path().join("game.sav").exists(),
+            "세이브 파일은 건드리지 않는다"
+        );
+    }
+
+    #[test]
+    fn wrong_size_thumbnail_is_ignored() {
+        let dir = tempfile::tempdir().unwrap();
+        let store = FsSaveStore::for_rom(&dir.path().join("game.gb"));
+        fs::write(dir.path().join("game.ss1.thumb"), [1, 2, 3]).unwrap();
+        assert_eq!(store.load_thumbnail(1).unwrap(), None);
+    }
+
+    #[test]
     fn missing_save_loads_as_none() {
         let dir = tempfile::tempdir().unwrap();
         let store = FsSaveStore::for_rom(&dir.path().join("game.gb"));
```

- [ ] **Step 2: 실패하는지 확인한다**

Run: `ARAGORN_REQUIRE_TEST_ROMS=1 cargo test --workspace`
Expected: 컴파일 실패, ``method `load_state` is not a member of trait `SaveStore` ``, ``cannot find function `thumbnail` in this scope``

- [ ] **Step 3: 구현 패치를 적용한다**

```diff
--- a/crates/aragorn-app/src/session.rs
+++ b/crates/aragorn-app/src/session.rs
@@ -14,6 +14,48 @@
     fn load_battery(&self) -> io::Result<Option<Vec<u8>>>;
     /// 덮어쓰기 전에 이전 세이브를 백업 하나로 남긴다.
     fn save_battery(&self, data: &[u8]) -> io::Result<()>;
+    /// 스테이트 슬롯(0–9)을 읽는다. 비어 있으면 `Ok(None)`.
+    fn load_state(&self, slot: u8) -> io::Result<Option<Vec<u8>>>;
+    /// 스테이트와 썸네일(`THUMBNAIL_WIDTH`×`THUMBNAIL_HEIGHT`, 0xRRGGBBAA)을 슬롯에 쓴다.
+    fn save_state(&self, slot: u8, data: &[u8], thumbnail: &[u32]) -> io::Result<()>;
+    /// 슬롯의 썸네일. 없거나 크기가 맞지 않으면 `Ok(None)`.
+    fn load_thumbnail(&self, slot: u8) -> io::Result<Option<Vec<u32>>>;
+}
+
+/// 스테이트 슬롯 수 (스펙 §5.1). 단축키 F1–F10이 슬롯 0–9다.
+pub const STATE_SLOTS: u8 = 10;
+pub const THUMBNAIL_WIDTH: usize = 80;
+pub const THUMBNAIL_HEIGHT: usize = 72;
+
+/// 화면(160×144)을 썸네일(80×72)로 줄인다. 2×2 픽셀마다 채널별 평균이다.
+pub fn thumbnail(framebuffer: &[u32]) -> Vec<u32> {
+    let width = THUMBNAIL_WIDTH * 2;
+    let mut out = Vec::with_capacity(THUMBNAIL_WIDTH * THUMBNAIL_HEIGHT);
+    for y in 0..THUMBNAIL_HEIGHT {
+        for x in 0..THUMBNAIL_WIDTH {
+            let at = |dx: usize, dy: usize| {
+                framebuffer
+                    .get((y * 2 + dy) * width + x * 2 + dx)
+                    .copied()
+                    .unwrap_or(0)
+            };
+            let pixels = [at(0, 0), at(1, 0), at(0, 1), at(1, 1)];
+            let channel = |shift: u32| {
+                let sum: u32 = pixels.iter().map(|p| (p >> shift) & 0xFF).sum();
+                (sum / 4) << shift
+            };
+            out.push(channel(24) | channel(16) | channel(8) | channel(0));
+        }
+    }
+    out
+}
+
+/// 슬롯 목록에 보일 정보.
+#[derive(Debug, Clone, PartialEq, Eq)]
+pub struct SlotInfo {
+    /// 저장 시각(유닉스 초)
+    pub saved_at: u64,
+    pub thumbnail: Option<Vec<u32>>,
 }
 
 /// 현재 시각 (포트). MBC3 RTC 세이브의 저장 시각과 앱이 꺼져 있던 시간 계산에 쓴다.
@@ -186,6 +228,47 @@
         !self.unsaved
     }
 
+    /// 지금 상태를 슬롯에 저장한다. 실패하면 상태 표시줄 문구를 돌려준다.
+    pub fn save_state(&mut self, slot: u8) -> Result<(), String> {
+        let data = self.gb.save_state(self.clock.now_unix());
+        let thumb = thumbnail(self.gb.framebuffer());
+        self.store
+            .save_state(slot, &data, &thumb)
+            .map_err(|e| format!("슬롯 {}에 저장하지 못했습니다: {e}", slot + 1))
+    }
+
+    /// 슬롯의 스테이트를 불러온다. 외부 RAM(게임 세이브)도 스테이트의 것으로 되돌아간다.
+    /// 아직 디스크에 쓰지 않은 게임 세이브가 있으면 먼저 쓰고, 쓰지 못하면 불러오지 않는다.
+    /// 실패하면 지금 게임은 그대로이고 상태 표시줄 문구를 돌려준다.
+    pub fn load_state(&mut self, slot: u8) -> Result<(), String> {
+        let n = slot + 1;
+        let data = match self.store.load_state(slot) {
+            Ok(Some(data)) => data,
+            Ok(None) => return Err(format!("슬롯 {n}이(가) 비어 있습니다")),
+            Err(e) => return Err(format!("슬롯 {n}을(를) 읽을 수 없습니다: {e}")),
+        };
+        if !self.flush() {
+            return Err("게임 세이브를 저장하지 못해 스테이트를 불러오지 않았습니다".to_string());
+        }
+        self.gb
+            .load_state(&data, self.clock.now_unix())
+            .map_err(|e| format!("슬롯 {n}: {e}"))?;
+        self.pacer.reset();
+        self.idle_frames = 0;
+        self.audio.clear();
+        Ok(())
+    }
+
+    /// 슬롯 목록에 보일 정보. 비었거나 읽을 수 없는 슬롯은 `None`.
+    pub fn slot_info(&self, slot: u8) -> Option<SlotInfo> {
+        let data = self.store.load_state(slot).ok()??;
+        let header = gb_core::state::peek(&data).ok()?;
+        Some(SlotInfo {
+            saved_at: header.saved_at,
+            thumbnail: self.store.load_thumbnail(slot).ok().flatten(),
+        })
+    }
+
     /// 쌓인 오류 문구를 꺼낸다.
     pub fn take_errors(&mut self) -> Vec<String> {
         std::mem::take(&mut self.errors)
--- a/crates/aragorn-desktop/src/adapters/fs_save_store.rs
+++ b/crates/aragorn-desktop/src/adapters/fs_save_store.rs
@@ -1,4 +1,4 @@
-use aragorn_app::session::SaveStore;
+use aragorn_app::session::{SaveStore, THUMBNAIL_HEIGHT, THUMBNAIL_WIDTH};
 use std::{
     cell::Cell,
     fs::{self, File},
@@ -7,8 +7,10 @@
 };
 
 /// ROM 옆의 `<ROM 이름>.sav` 파일. 다른 에뮬레이터(BGB, mGBA, VBA-M)와 같은 위치다.
+/// 스테이트는 같은 자리의 `<ROM 이름>.ss0`–`.ss9`, 썸네일은 `.ss0.thumb`(RGBA u32 LE)이다.
 pub struct FsSaveStore {
     path: PathBuf,
+    rom_path: PathBuf,
     /// 이번 실행에서 `.sav.bak`을 이미 만들었는지. 백업은 처음 저장하기 직전의 세이브 하나만 둔다.
     backed_up: Cell<bool>,
 }
@@ -17,6 +19,7 @@
     pub fn for_rom(rom_path: &Path) -> Self {
         Self {
             path: rom_path.with_extension("sav"),
+            rom_path: rom_path.to_path_buf(),
             backed_up: Cell::new(false),
         }
     }
@@ -24,15 +27,40 @@
     pub fn path(&self) -> &Path {
         &self.path
     }
+
+    pub fn state_path(&self, slot: u8) -> PathBuf {
+        self.rom_path.with_extension(format!("ss{slot}"))
+    }
+
+    fn thumbnail_path(&self, slot: u8) -> PathBuf {
+        self.rom_path.with_extension(format!("ss{slot}.thumb"))
+    }
+}
+
+/// 없으면 `Ok(None)`.
+fn read_optional(path: &Path) -> io::Result<Option<Vec<u8>>> {
+    match fs::read(path) {
+        Ok(data) => Ok(Some(data)),
+        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
+        Err(e) => Err(e),
+    }
+}
+
+/// 쓰는 도중 꺼지거나 OS가 멈춰도 깨지지 않도록, 임시 파일을 디스크에 확실히 쓴 뒤 교체한다.
+fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
+    let mut tmp = path.as_os_str().to_owned();
+    tmp.push(".tmp");
+    let tmp = PathBuf::from(tmp);
+    let mut file = File::create(&tmp)?;
+    file.write_all(data)?;
+    file.sync_all()?;
+    drop(file);
+    fs::rename(&tmp, path)
 }
 
 impl SaveStore for FsSaveStore {
     fn load_battery(&self) -> io::Result<Option<Vec<u8>>> {
-        match fs::read(&self.path) {
-            Ok(data) => Ok(Some(data)),
-            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
-            Err(e) => Err(e),
-        }
+        read_optional(&self.path)
     }
 
     fn save_battery(&self, data: &[u8]) -> io::Result<()> {
@@ -44,13 +72,31 @@
             }
             self.backed_up.set(true);
         }
-        // 쓰는 도중 꺼지거나 OS가 멈춰도 깨지지 않도록, 임시 파일을 디스크에 확실히 쓴 뒤 교체한다.
-        let tmp = self.path.with_extension("sav.tmp");
-        let mut file = File::create(&tmp)?;
-        file.write_all(data)?;
-        file.sync_all()?;
-        drop(file);
-        fs::rename(&tmp, &self.path)
+        write_atomic(&self.path, data)
+    }
+
+    fn load_state(&self, slot: u8) -> io::Result<Option<Vec<u8>>> {
+        read_optional(&self.state_path(slot))
+    }
+
+    /// 썸네일을 먼저 쓰고 스테이트를 쓴다. 중간에 실패해도 스테이트와 어긋난 썸네일만 남는다.
+    fn save_state(&self, slot: u8, data: &[u8], thumbnail: &[u32]) -> io::Result<()> {
+        let thumb: Vec<u8> = thumbnail.iter().flat_map(|p| p.to_le_bytes()).collect();
+        write_atomic(&self.thumbnail_path(slot), &thumb)?;
+        write_atomic(&self.state_path(slot), data)
+    }
+
+    fn load_thumbnail(&self, slot: u8) -> io::Result<Option<Vec<u32>>> {
+        let Some(bytes) = read_optional(&self.thumbnail_path(slot))? else {
+            return Ok(None);
+        };
+        if bytes.len() != THUMBNAIL_WIDTH * THUMBNAIL_HEIGHT * 4 {
+            return Ok(None);
+        }
+        let (chunks, _) = bytes.as_chunks::<4>();
+        Ok(Some(
+            chunks.iter().map(|c| u32::from_le_bytes(*c)).collect(),
+        ))
     }
 }
 
```

- [ ] **Step 4: 통과하는지 확인한다**

Run: `ARAGORN_REQUIRE_TEST_ROMS=1 cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: app 102 passed (+ session 7), desktop 67 passed (+ fs_save_store 3), clippy 경고 0

- [ ] **Step 5: 커밋한다**

```bash
cargo fmt --all --check
git add -A crates
git commit -m "feat: 세션 스테이트 슬롯과 ROM 옆 스테이트 파일

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---
### Task 4: 단축키와 스테이트 메뉴

**Files:** `crates/aragorn-desktop/src/ui/mod.rs`, `crates/aragorn-desktop/src/ui/state_slots.rs`, `crates/aragorn-desktop/src/app.rs`

**Interfaces:**
- Consumes: Task 3 `Session::{save_state, load_state, slot_info}`, `STATE_SLOTS`, 썸네일 크기
- Produces:
  - `ui::state_slots::{SlotCommand, hotkey, ago_text, SlotsView}`
  - 툴바 "스테이트" 메뉴, F1–F10 / Shift+F1–F10

- [ ] **Step 1: 테스트 패치를 적용한다**

```diff
--- a/crates/aragorn-desktop/src/ui/mod.rs
+++ b/crates/aragorn-desktop/src/ui/mod.rs
@@ -2,4 +2,5 @@
 pub mod input;
 pub mod screen;
 pub mod settings;
+pub mod state_slots;
 pub mod update_view;
--- /dev/null
+++ b/crates/aragorn-desktop/src/ui/state_slots.rs
@@ -0,0 +1,51 @@
+//! 세이브 스테이트 슬롯: F1–F10 저장, Shift+F1–F10 불러오기 (스펙 §6.2), 툴바 "스테이트" 메뉴.
+#[cfg(test)]
+mod tests {
+    use super::*;
+
+    fn key(key: Key, shift: bool, repeat: bool) -> Event {
+        Event::Key {
+            key,
+            physical_key: None,
+            pressed: true,
+            repeat,
+            modifiers: egui::Modifiers {
+                shift,
+                ..Default::default()
+            },
+        }
+    }
+
+    #[test]
+    fn function_keys_save_and_shift_loads() {
+        assert_eq!(
+            hotkey(&[key(Key::F1, false, false)]),
+            Some(SlotCommand::Save(0))
+        );
+        assert_eq!(
+            hotkey(&[key(Key::F10, false, false)]),
+            Some(SlotCommand::Save(9))
+        );
+        assert_eq!(
+            hotkey(&[key(Key::F3, true, false)]),
+            Some(SlotCommand::Load(2))
+        );
+    }
+
+    #[test]
+    fn other_keys_and_repeats_do_nothing() {
+        assert_eq!(hotkey(&[]), None);
+        assert_eq!(hotkey(&[key(Key::F11, false, false)]), None);
+        assert_eq!(hotkey(&[key(Key::A, false, false)]), None);
+        assert_eq!(hotkey(&[key(Key::F2, false, true)]), None);
+    }
+
+    #[test]
+    fn ago_text_uses_largest_unit() {
+        assert_eq!(ago_text(100, 100), "방금");
+        assert_eq!(ago_text(100, 200), "방금", "시계가 거꾸로 가도 방금");
+        assert_eq!(ago_text(1000, 1000 - 5 * 60), "5분 전");
+        assert_eq!(ago_text(100_000, 100_000 - 3 * 3600 - 1), "3시간 전");
+        assert_eq!(ago_text(1_000_000, 1_000_000 - 2 * 86_400), "2일 전");
+    }
+}
```

- [ ] **Step 2: 실패하는지 확인한다**

Run: `ARAGORN_REQUIRE_TEST_ROMS=1 cargo test --workspace`
Expected: 컴파일 실패, ``cannot find function `hotkey` in this scope``, ``cannot find function `ago_text` in this scope``

- [ ] **Step 3: 구현 패치를 적용한다**

```diff
--- a/crates/aragorn-desktop/src/ui/state_slots.rs
+++ b/crates/aragorn-desktop/src/ui/state_slots.rs
@@ -1,4 +1,141 @@
 //! 세이브 스테이트 슬롯: F1–F10 저장, Shift+F1–F10 불러오기 (스펙 §6.2), 툴바 "스테이트" 메뉴.
+
+use aragorn_app::session::{STATE_SLOTS, Session, THUMBNAIL_HEIGHT, THUMBNAIL_WIDTH};
+use eframe::egui::{self, Event, Key};
+
+const SLOT_KEYS: [Key; STATE_SLOTS as usize] = [
+    Key::F1,
+    Key::F2,
+    Key::F3,
+    Key::F4,
+    Key::F5,
+    Key::F6,
+    Key::F7,
+    Key::F8,
+    Key::F9,
+    Key::F10,
+];
+
+#[derive(Debug, Clone, Copy, PartialEq, Eq)]
+pub enum SlotCommand {
+    Save(u8),
+    Load(u8),
+}
+
+/// 이번 화면 갱신의 키 입력에서 슬롯 단축키를 찾는다. 키 반복은 세지 않는다.
+pub fn hotkey(events: &[Event]) -> Option<SlotCommand> {
+    events.iter().find_map(|e| match e {
+        Event::Key {
+            key,
+            pressed: true,
+            repeat: false,
+            modifiers,
+            ..
+        } => {
+            let slot = SLOT_KEYS.iter().position(|k| k == key)? as u8;
+            Some(if modifiers.shift {
+                SlotCommand::Load(slot)
+            } else {
+                SlotCommand::Save(slot)
+            })
+        }
+        _ => None,
+    })
+}
+
+/// 저장한 지 얼마나 지났는지.
+pub fn ago_text(now: u64, saved_at: u64) -> String {
+    let secs = now.saturating_sub(saved_at);
+    match secs {
+        0..60 => "방금".to_string(),
+        60..3600 => format!("{}분 전", secs / 60),
+        3600..86_400 => format!("{}시간 전", secs / 3600),
+        _ => format!("{}일 전", secs / 86_400),
+    }
+}
+
+struct SlotEntry {
+    saved_at: u64,
+    texture: Option<egui::TextureHandle>,
+}
+
+/// 메뉴에 보일 슬롯 목록. 메뉴를 열 때 비어 있으면 저장소에서 다시 읽는다.
+#[derive(Default)]
+pub struct SlotsView {
+    slots: Option<Vec<Option<SlotEntry>>>,
+}
+
+impl SlotsView {
+    /// ROM을 바꾸거나 슬롯에 저장하면 다음에 메뉴를 열 때 다시 읽는다.
+    pub fn invalidate(&mut self) {
+        self.slots = None;
+    }
+
+    fn load(ctx: &egui::Context, session: &Session) -> Vec<Option<SlotEntry>> {
+        (0..STATE_SLOTS)
+            .map(|slot| {
+                let info = session.slot_info(slot)?;
+                let texture = info.thumbnail.map(|thumb| {
+                    let image = egui::ColorImage::from_rgba_unmultiplied(
+                        [THUMBNAIL_WIDTH, THUMBNAIL_HEIGHT],
+                        &super::screen::to_rgba(&thumb),
+                    );
+                    ctx.load_texture(format!("slot{slot}"), image, egui::TextureOptions::LINEAR)
+                });
+                Some(SlotEntry {
+                    saved_at: info.saved_at,
+                    texture,
+                })
+            })
+            .collect()
+    }
+
+    /// 메뉴 안을 그린다. 누른 명령을 돌려준다.
+    pub fn show(&mut self, ui: &mut egui::Ui, session: &Session, now: u64) -> Option<SlotCommand> {
+        let slots = self
+            .slots
+            .get_or_insert_with(|| Self::load(ui.ctx(), session));
+        let mut command = None;
+        ui.label("F1–F10: 저장, Shift+F1–F10: 불러오기");
+        ui.separator();
+        egui::Grid::new("state_slots").striped(true).show(ui, |ui| {
+            for (slot, entry) in slots.iter().enumerate() {
+                let slot = slot as u8;
+                let size = egui::vec2(THUMBNAIL_WIDTH as f32, THUMBNAIL_HEIGHT as f32);
+                match entry.as_ref().and_then(|e| e.texture.as_ref()) {
+                    Some(texture) => {
+                        ui.add(egui::Image::new((texture.id(), size)));
+                    }
+                    None => {
+                        ui.allocate_space(size);
+                    }
+                }
+                ui.vertical(|ui| {
+                    ui.strong(format!("슬롯 {} (F{})", slot + 1, slot + 1));
+                    match entry {
+                        Some(e) => ui.label(ago_text(now, e.saved_at)),
+                        None => ui.weak("비어 있음"),
+                    };
+                });
+                if ui.button("저장").clicked() {
+                    command = Some(SlotCommand::Save(slot));
+                }
+                if ui
+                    .add_enabled(entry.is_some(), egui::Button::new("불러오기"))
+                    .clicked()
+                {
+                    command = Some(SlotCommand::Load(slot));
+                }
+                ui.end_row();
+            }
+        });
+        if command.is_some() {
+            ui.close();
+        }
+        command
+    }
+}
+
 #[cfg(test)]
 mod tests {
     use super::*;
--- a/crates/aragorn-desktop/src/app.rs
+++ b/crates/aragorn-desktop/src/app.rs
@@ -5,6 +5,7 @@
         input::{self, InputFrame, Keymap},
         screen::ScreenView,
         settings::SettingsView,
+        state_slots::{self, SlotCommand, SlotsView},
         update_view::{self, UpdateUiAction},
     },
     update_worker::UpdateWorker,
@@ -12,7 +13,7 @@
 use aragorn_app::{
     config::{Config, ConfigStore, UpdateConfig},
     pacing::{FastForward, RunMode, SpeedControl, UNLIMITED_BUDGET},
-    session::Session,
+    session::{Clock, Session},
     update::{UpdateCommand, UpdateEvent, UpdateFlow, UpdateSource, Updater},
 };
 use eframe::egui;
@@ -106,6 +107,7 @@
     /// 직전 화면 갱신의 실행 모드 (상태 표시줄용)
     mode: RunMode,
     settings: SettingsView,
+    slots: SlotsView,
 }
 
 impl AragornApp {
@@ -147,6 +149,7 @@
             speed: SpeedControl::default(),
             mode: RunMode::Normal,
             settings: SettingsView::default(),
+            slots: SlotsView::default(),
         };
         if app.worker.is_some() {
             app.dispatch(UpdateEvent::CheckRequested);
@@ -172,6 +175,7 @@
                 )));
                 self.screen.update(ctx, session.framebuffer());
                 self.notice = Some(format!("{} 실행 중", session.title()));
+                self.slots.invalidate();
                 self.session = Some(session);
                 self.last_tick = Instant::now();
                 self.collect_session_errors();
@@ -219,6 +223,37 @@
         ))
     }
 
+    /// 스테이트 슬롯에 저장하거나 불러오고 결과를 상태 표시줄에 알린다. 종료 중이거나 강제 업데이트
+    /// 중에는 하지 않는다.
+    fn run_slot_command(&mut self, ctx: &egui::Context, command: SlotCommand) {
+        if self.exit_handled || self.flow.blocks_emulation() {
+            return;
+        }
+        let Some(session) = &mut self.session else {
+            return;
+        };
+        let result = match command {
+            SlotCommand::Save(slot) => {
+                self.slots.invalidate();
+                session
+                    .save_state(slot)
+                    .map(|()| format!("슬롯 {}에 저장했습니다", slot + 1))
+            }
+            SlotCommand::Load(slot) => session.load_state(slot).map(|()| {
+                self.screen.update(ctx, session.framebuffer());
+                format!("슬롯 {}을(를) 불러왔습니다", slot + 1)
+            }),
+        };
+        match result {
+            Ok(message) => self.notice = Some(message),
+            Err(message) => {
+                log::warn!("{message}");
+                self.notice = Some(message);
+            }
+        }
+        self.collect_session_errors();
+    }
+
     fn input_config_changed(&mut self) {
         self.keymap = Keymap::new(&self.config.input.keyboard);
         self.save_config();
@@ -351,6 +386,11 @@
             release_widget_focus(ctx);
         }
         let input = self.gather_input(ctx);
+        if input.is_some()
+            && let Some(command) = ctx.input(|i| state_slots::hotkey(&i.events))
+        {
+            self.run_slot_command(ctx, command);
+        }
         self.run_emulation(ctx, input);
         // 6시간 재확인 타이머가 유휴 상태에서도 돌도록 주기적으로 깨운다.
         ctx.request_repaint_after(Duration::from_secs(60));
@@ -360,6 +400,7 @@
         let mut events = Vec::new();
 
         let mut open_requested = false;
+        let mut slot_command = None;
         egui::Panel::top("toolbar").show(ui, |ui| {
             ui.horizontal(|ui| {
                 if ui.button("ROM 열기").clicked() {
@@ -367,6 +408,12 @@
                 }
                 if ui.button("설정").clicked() {
                     self.settings.open = !self.settings.open;
+                }
+                if let Some(session) = &self.session {
+                    let now = SystemClock.now_unix();
+                    ui.menu_button("스테이트", |ui| {
+                        slot_command = self.slots.show(ui, session, now);
+                    });
                 }
                 ui.separator();
                 let mut auto_download = self.flow.prefs().auto_download;
@@ -422,6 +469,10 @@
             }
         });
 
+        if let Some(command) = slot_command {
+            let ctx = ui.ctx().clone();
+            self.run_slot_command(&ctx, command);
+        }
         if self.settings.show(ui.ctx(), &mut self.config.input) {
             self.input_config_changed();
         }
```

- [ ] **Step 4: 통과하는지 확인한다**

Run: `ARAGORN_REQUIRE_TEST_ROMS=1 cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: desktop 70 passed (+ state_slots 3), clippy 경고 0

- [ ] **Step 5: 커밋한다**

```bash
cargo fmt --all --check
git add -A crates
git commit -m "feat(desktop): F1–F10 스테이트 단축키와 스테이트 메뉴

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---
### Task 5: 전체 확인, PR

- [ ] **Step 1: 전체 검사를 실행한다**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && ARAGORN_REQUIRE_TEST_ROMS=1 cargo test --workspace --no-fail-fast 2>&1 | grep "test result" | grep -v " 0 passed"`
Expected: 모든 결과가 `ok`다.

| 테스트 | 개수 |
|---|---|
| gb-core lib | 244 |
| acid2 (DMG) | 1 |
| blargg | 15 |
| cgb_acid2 | 1 |
| cgb_sound | 12 |
| dmg_sound | 12 |
| mooneye | 77 |
| save_state | 14 |
| app | 102 |
| desktop | 70 |
| xtask | 14 |

- [ ] **Step 2: PR을 만들고 CI를 확인한다** (외부 공개 작업이므로 사용자 확인 후)

```bash
git push -u origin feat/m7b-save-states
gh pr create --repo dongjay00/aragorn --base main --title "M7b: 세이브 스테이트" --body-file <작성한 본문>
gh pr checks --watch
```

- [ ] **Step 3: 플레이 확인을 요청한다** (사용자 작업, 릴리스 뒤)

사용자에게 아래 확인을 요청한다.
- 게임 중에 F1을 누르면 "슬롯 1에 저장했습니다"가 뜨고, Shift+F1로 그 순간으로 돌아간다. 화면과 소리가 바로 이어진다.
- 툴바 "스테이트" 메뉴에 썸네일과 "N분 전"이 보이고, 메뉴에서도 저장·불러오기가 된다.
- 금·은·크리스탈: 스테이트를 저장하고 몇 분 뒤 불러오면 게임 안 시계가 실제 시간에 맞다.
- 게임 안에서 저장한 뒤 그 이전 스테이트를 불러와도, 앱을 다시 켜기 전까지 `.sav`는 게임 안 저장 시점의 것이다.
