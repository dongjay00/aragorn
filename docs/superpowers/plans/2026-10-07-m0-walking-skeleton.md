# 마일스톤 0: 걸어다니는 뼈대 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 3계층 Rust 워크스페이스, 빈 eframe 창, CI/릴리스 파이프라인, 그리고 강제/소프트/자동 업데이트가 동작하는 Aragorn v0.1.0을 GitHub Releases로 배포한다.

**Architecture:** `gb-core`(빈 도메인 크레이트) ← `aragorn-app`(업데이트 정책 판단, 서명 검증, 업데이트 상태 머신, 포트 trait, 설정 모델) ← `aragorn-desktop`(eframe UI, HTTP 정책 소스, Velopack 업데이터, TOML 설정 저장소). 업데이트 로직은 `aragorn-app`의 순수 상태 머신(`UpdateFlow`: 이벤트 → 명령)으로 만들고, 데스크톱은 명령을 백그라운드 스레드에서 실행해 결과를 이벤트로 되돌려준다. `xtask`는 서명 키 생성, 정책 파일 생성과 서명, 태그 검증을 맡는다.

**Tech Stack:** Rust 1.97 (edition 2024), semver, serde/serde_json, ed25519-dalek 2, thiserror 2, eframe(egui), ureq 2, toml, directories, velopack, GitHub Actions, vpk CLI(.NET tool)

**Spec:** `docs/superpowers/specs/2026-10-07-gameboy-emulator-design.md` (이 계획은 §2, §3, §5.2, §5.3, §7, §9의 마일스톤 0을 구현한다)

**작업 브랜치:** `feat/m0-walking-skeleton`

## Global Constraints

- Rust stable 1.97 이상, 모든 크레이트 `edition = "2024"`, 버전은 워크스페이스 `Cargo.toml`의 `workspace.package.version` 한 곳에서만 정의한다 (초기값 `0.1.0`).
- 의존 방향은 `aragorn-desktop → aragorn-app → gb-core`만 허용한다. `aragorn-app`과 `gb-core`는 eframe, ureq, velopack, 파일시스템 경로 크레이트에 의존하지 않는다.
- 버저닝은 Semantic Versioning(major.minor.patch)이다. 릴리스 태그 형식은 `vX.Y.Z`이고 워크스페이스 버전과 같아야 한다.
- 정책 파일 URL은 `https://github.com/<owner>/<repo>/releases/latest/download/update-policy.json`, 서명은 같은 위치의 `update-policy.json.sig`(ed25519 원시 서명 64바이트)이다.
- 서명이 검증되지 않은 정책이나 패키지는 절대 적용하지 않는다.
- 정책 확인 타임아웃은 5초, 실행 중 재확인 주기는 6시간이다. 자동 다운로드 기본값은 켜짐이다.
- 정책 확인 실패(오프라인 포함)는 앱 실행을 막지 않는다.
- 사용자에게 보이는 문구는 한국어다.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`가 모든 커밋에서 통과해야 한다.
- eframe과 velopack은 `cargo add`로 최신 버전을 쓴다. 이 계획의 코드와 해당 버전의 API 이름이 다르면(deprecation 경고 포함) docs.rs의 해당 버전 문서에 맞춰 **그 파일 안에서만** 조정하고, 포트 시그니처와 동작은 바꾸지 않는다.
- 커밋 메시지 끝에는 `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`를 붙인다.

## 스펙과 다른 결정 (리뷰어 확인용)

1. 서명/검증 함수(`sign_policy`, `verify_and_parse`)를 `aragorn-desktop`이 아닌 `aragorn-app`에 둔다. 순수 로직이라 TDD가 쉽고, `xtask`와 같은 코드를 공유할 수 있다. 그래서 `ed25519-dalek`은 `aragorn-app`의 의존성이 된다.
2. `Updater` 포트에 `apply_on_exit()`를 추가한다. 스펙 §7.2-4의 "다음 실행 시 적용"을 구현하기 위한 메서드로, Velopack의 "종료 후 적용" 기능에 대응한다.
3. 강제 업데이트 다운로드나 적용에 실패하면 "다시 시도 / 종료" 모달을 유지한다. 스펙 §7.2-6의 "실패해도 계속 실행"은 정책 확인 실패에 적용하고, 이미 서명 검증된 강제 정책이 있는 경우에는 적용하지 않는다고 해석했다.
4. GitHub 저장소는 빌드 시 환경 변수 `ARAGORN_REPO`(`owner/repo`)로 주입한다. 로컬 개발 빌드처럼 이 값이 없으면 업데이트 확인을 하지 않는다.
5. 한국어 표시를 위해 나눔고딕(OFL) 폰트를 바이너리에 내장한다. egui 기본 폰트에는 한글 글리프가 없다.

## Review Focus

1. **Velopack으로 설치하지 않은 실행**(`cargo run`, 압축 해제한 바이너리): 앱은 정상 실행되어야 하고, 강제 업데이트 상황에서는 "업데이트" 버튼 대신 다운로드 페이지 링크와 "종료"를 보여야 한다. → Task 3 `forced_without_updater_ignores_update_now`, Task 6 `unavailable_updater_reports_not_installed`
2. **깨진 정책 응답**(GitHub 404 HTML, 잘린 JSON, `"latest"` 같은 비semver 값, `minimum_supported > latest`): 확인 실패로 처리하고 앱은 계속 실행되어야 한다. → Task 2 `rejects_*` 테스트들, Task 6 `missing_policy_is_network_error`
3. **서버 무응답**: 5초 안에 포기하고 UI를 막지 않아야 한다. → Task 6 `hanging_server_times_out`, Task 7 `worker_returns_events_asynchronously`
4. **강제 업데이트 다운로드 실패**: 게임으로 넘어가지 않고 "다시 시도 / 종료"를 유지해야 한다. → Task 3 `forced_download_failure_keeps_blocking_and_allows_retry`
5. **손상된 설정 파일 / 일부 필드만 있는 설정 / 잘못된 건너뛴 버전 문자열**: 기본값으로 실행되어야 한다. → Task 4 `invalid_skipped_version_is_ignored`, Task 6 `corrupt_file_falls_back_to_default`, `partial_file_keeps_defaults`

---

## 파일 구조

```
aragorn/
├── Cargo.toml                                   # workspace (Task 1, 5, 6에서 members 추가)
├── .cargo/config.toml                           # xtask alias (Task 5)
├── release/
│   ├── policy.toml                              # minimum_supported, message (Task 5)
│   └── policy.pub                               # ed25519 공개키 32바이트 (Task 5)
├── crates/
│   ├── gb-core/
│   │   ├── Cargo.toml
│   │   └── src/lib.rs                           # 빈 도메인 크레이트 (Task 1)
│   ├── aragorn-app/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── config.rs                        # Config, UpdateConfig, ConfigStore (Task 4)
│   │       └── update/
│   │           ├── mod.rs
│   │           ├── decision.rs                  # UpdatePolicy, UpdatePrefs, UpdateDecision, decide (Task 1)
│   │           ├── error.rs                     # UpdateError (Task 2)
│   │           ├── policy_doc.rs                # parse_policy, verify_and_parse, sign_policy (Task 2)
│   │           ├── flow.rs                      # UpdateFlow 상태 머신 (Task 3)
│   │           └── ports.rs                     # UpdateSource, Updater, execute (Task 4)
│   └── aragorn-desktop/
│       ├── Cargo.toml
│       ├── assets/fonts/{NanumGothic-Regular.ttf, OFL.txt}   # (Task 7)
│       └── src/
│           ├── lib.rs
│           ├── main.rs                          # 조립 (Task 7)
│           ├── build_info.rs                    # 버전, 저장소, 공개키 (Task 6)
│           ├── adapters/
│           │   ├── mod.rs
│           │   ├── github_policy.rs             # GithubPolicySource (Task 6)
│           │   ├── toml_config.rs               # TomlConfigStore (Task 6)
│           │   └── velopack_updater.rs          # VelopackUpdater, UnavailableUpdater (Task 6)
│           ├── update_worker.rs                 # 백그라운드 명령 실행 (Task 7)
│           ├── app.rs                           # eframe::App (Task 7)
│           └── ui/{mod.rs, fonts.rs, update_view.rs}           # (Task 7)
├── xtask/
│   ├── Cargo.toml
│   └── src/{main.rs, policy.rs}                 # (Task 5)
└── .github/workflows/{ci.yml, release.yml}      # (Task 8)
```

---

### Task 1: 워크스페이스와 업데이트 판단 `decide()`

**Files:**
- Create: `Cargo.toml`
- Create: `crates/gb-core/Cargo.toml`, `crates/gb-core/src/lib.rs`
- Create: `crates/aragorn-app/Cargo.toml`, `crates/aragorn-app/src/lib.rs`
- Create: `crates/aragorn-app/src/update/mod.rs`, `crates/aragorn-app/src/update/decision.rs`

**Interfaces:**
- Consumes: 없음
- Produces:
  - `aragorn_app::update::UpdatePolicy { pub latest: Version, pub minimum_supported: Version, pub message: String }` (Debug, Clone, PartialEq, Eq)
  - `aragorn_app::update::UpdatePrefs { pub auto_download: bool, pub skipped_version: Option<Version> }` (Debug, Clone, Default, PartialEq, Eq)
  - `aragorn_app::update::UpdateDecision { UpToDate, Soft { version: Version, auto_download: bool }, Forced { version: Version } }`
  - `aragorn_app::update::decide(current: &Version, policy: &UpdatePolicy, prefs: &UpdatePrefs) -> UpdateDecision`
  - `Version`은 `semver::Version`

- [ ] **Step 0: 브랜치 생성**

```bash
git checkout -b feat/m0-walking-skeleton
```

- [ ] **Step 1: 워크스페이스와 크레이트 뼈대 작성**

`Cargo.toml`:
```toml
[workspace]
resolver = "3"
members = ["crates/gb-core", "crates/aragorn-app"]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "MIT"
```

`crates/gb-core/Cargo.toml`:
```toml
[package]
name = "gb-core"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
```

`crates/gb-core/src/lib.rs`:
```rust
//! Game Boy / Game Boy Color 에뮬레이션 코어.
//!
//! UI, OS, 네트워크에 의존하지 않는다. 하드웨어 구현은 마일스톤 1부터 추가한다.
```

`crates/aragorn-app/Cargo.toml`:
```toml
[package]
name = "aragorn-app"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
gb-core = { path = "../gb-core" }
semver = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
ed25519-dalek = "2"
thiserror = "2"
```

`crates/aragorn-app/src/lib.rs`:
```rust
//! 유스케이스 계층: 세션, 설정, 업데이트 정책. 외부 세계와는 포트 trait으로만 대화한다.

pub mod update;
```

`crates/aragorn-app/src/update/mod.rs`:
```rust
mod decision;

pub use decision::{UpdateDecision, UpdatePolicy, UpdatePrefs, decide};
```

- [ ] **Step 2: 실패하는 테스트 작성**

`crates/aragorn-app/src/update/decision.rs`:
```rust
use semver::Version;

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
            UpdateDecision::Soft { version: v("1.1.0"), auto_download: false }
        );
    }

    #[test]
    fn soft_update_carries_auto_download_pref() {
        let d = decide(&v("1.0.0"), &policy("1.1.0", "1.0.0"), &prefs(true, None));
        assert_eq!(
            d,
            UpdateDecision::Soft { version: v("1.1.0"), auto_download: true }
        );
    }

    #[test]
    fn below_minimum_is_forced_to_latest() {
        let d = decide(&v("1.0.0"), &policy("1.2.0", "1.1.0"), &prefs(true, None));
        assert_eq!(d, UpdateDecision::Forced { version: v("1.2.0") });
    }

    #[test]
    fn skipped_version_is_not_offered_again() {
        let d = decide(&v("1.0.0"), &policy("1.1.0", "1.0.0"), &prefs(true, Some("1.1.0")));
        assert_eq!(d, UpdateDecision::UpToDate);
    }

    #[test]
    fn newer_release_after_skipped_one_is_offered() {
        let d = decide(&v("1.0.0"), &policy("1.2.0", "1.0.0"), &prefs(false, Some("1.1.0")));
        assert_eq!(
            d,
            UpdateDecision::Soft { version: v("1.2.0"), auto_download: false }
        );
    }

    #[test]
    fn skipping_cannot_bypass_forced_update() {
        let d = decide(&v("1.0.0"), &policy("1.1.0", "1.1.0"), &prefs(true, Some("1.1.0")));
        assert_eq!(d, UpdateDecision::Forced { version: v("1.1.0") });
    }

    #[test]
    fn local_build_newer_than_latest_is_up_to_date() {
        let d = decide(&v("1.3.0"), &policy("1.2.0", "1.0.0"), &prefs(true, None));
        assert_eq!(d, UpdateDecision::UpToDate);
    }

    #[test]
    fn prerelease_latest_is_ignored() {
        let d = decide(&v("1.0.0"), &policy("1.1.0-beta.1", "1.0.0"), &prefs(true, None));
        assert_eq!(d, UpdateDecision::UpToDate);
    }
}
```

- [ ] **Step 3: 테스트가 실패하는지 확인**

Run: `cargo test -p aragorn-app update::decision`
Expected: 컴파일 실패 — `cannot find type UpdatePolicy`, `cannot find function decide`

- [ ] **Step 4: 최소 구현 작성**

`decision.rs`의 `use semver::Version;` 아래, `#[cfg(test)]` 위에 추가:
```rust
/// 서명 검증을 통과한 업데이트 정책.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdatePolicy {
    pub latest: Version,
    pub minimum_supported: Version,
    pub message: String,
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
    Soft { version: Version, auto_download: bool },
    Forced { version: Version },
}

pub fn decide(current: &Version, policy: &UpdatePolicy, prefs: &UpdatePrefs) -> UpdateDecision {
    if !policy.latest.pre.is_empty() {
        return UpdateDecision::UpToDate;
    }
    if *current < policy.minimum_supported {
        return UpdateDecision::Forced { version: policy.latest.clone() };
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
```

- [ ] **Step 5: 테스트 통과 확인**

Run: `cargo test -p aragorn-app update::decision`
Expected: 9 passed

- [ ] **Step 6: 포맷, 린트, 커밋**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add Cargo.toml Cargo.lock crates/
git commit -m "feat(app): add workspace and update decision logic

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: 정책 문서 파싱과 서명 검증

**Files:**
- Create: `crates/aragorn-app/src/update/error.rs`
- Create: `crates/aragorn-app/src/update/policy_doc.rs`
- Modify: `crates/aragorn-app/src/update/mod.rs`

**Interfaces:**
- Consumes: `UpdatePolicy` (Task 1)
- Produces:
  - `aragorn_app::update::UpdateError { Network(String), BadSignature, Malformed(String), NotInstalled, Download(String), Apply(String) }` (Debug, Clone, PartialEq, Eq, Display)
  - `aragorn_app::update::parse_policy(json: &[u8]) -> Result<UpdatePolicy, UpdateError>`
  - `aragorn_app::update::verify_and_parse(json: &[u8], signature: &[u8], public_key: &[u8; 32]) -> Result<UpdatePolicy, UpdateError>`
  - `aragorn_app::update::sign_policy(json: &[u8], secret: &[u8; 32]) -> [u8; 64]`
  - `aragorn_app::update::public_key_for(secret: &[u8; 32]) -> [u8; 32]`

- [ ] **Step 1: 에러 타입과 모듈 등록**

`crates/aragorn-app/src/update/error.rs`:
```rust
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UpdateError {
    #[error("네트워크 오류: {0}")]
    Network(String),
    #[error("서명 검증 실패")]
    BadSignature,
    #[error("정책 형식 오류: {0}")]
    Malformed(String),
    #[error("설치판에서만 자동 업데이트를 할 수 있습니다")]
    NotInstalled,
    #[error("다운로드 실패: {0}")]
    Download(String),
    #[error("업데이트 적용 실패: {0}")]
    Apply(String),
}
```

`crates/aragorn-app/src/update/mod.rs`:
```rust
mod decision;
mod error;
mod policy_doc;

pub use decision::{UpdateDecision, UpdatePolicy, UpdatePrefs, decide};
pub use error::UpdateError;
pub use policy_doc::{parse_policy, public_key_for, sign_policy, verify_and_parse};
```

- [ ] **Step 2: 실패하는 테스트 작성**

`crates/aragorn-app/src/update/policy_doc.rs`:
```rust
use super::{UpdateError, UpdatePolicy};
use semver::Version;

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: [u8; 32] = [7; 32];
    const OTHER_SECRET: [u8; 32] = [9; 32];
    const VALID: &[u8] = br#"{"latest":"1.4.2","minimum_supported":"1.2.0","message":"fix"}"#;

    fn pk() -> [u8; 32] {
        public_key_for(&SECRET)
    }

    #[test]
    fn accepts_validly_signed_policy() {
        let sig = sign_policy(VALID, &SECRET);
        let p = verify_and_parse(VALID, &sig, &pk()).unwrap();
        assert_eq!(p.latest, Version::new(1, 4, 2));
        assert_eq!(p.minimum_supported, Version::new(1, 2, 0));
        assert_eq!(p.message, "fix");
    }

    #[test]
    fn rejects_tampered_body() {
        let sig = sign_policy(VALID, &SECRET);
        let tampered = br#"{"latest":"9.9.9","minimum_supported":"1.2.0","message":"fix"}"#;
        assert_eq!(verify_and_parse(tampered, &sig, &pk()), Err(UpdateError::BadSignature));
    }

    #[test]
    fn rejects_signature_from_other_key() {
        let sig = sign_policy(VALID, &OTHER_SECRET);
        assert_eq!(verify_and_parse(VALID, &sig, &pk()), Err(UpdateError::BadSignature));
    }

    #[test]
    fn rejects_signature_of_wrong_length() {
        let html = b"<html>404 Not Found</html>";
        assert_eq!(verify_and_parse(VALID, html, &pk()), Err(UpdateError::BadSignature));
    }

    #[test]
    fn rejects_html_body_even_if_signed() {
        let body = b"<!DOCTYPE html><html>Not Found</html>";
        let sig = sign_policy(body, &SECRET);
        assert!(matches!(verify_and_parse(body, &sig, &pk()), Err(UpdateError::Malformed(_))));
    }

    #[test]
    fn rejects_non_semver_version() {
        let body = br#"{"latest":"latest","minimum_supported":"1.0.0"}"#;
        assert!(matches!(parse_policy(body), Err(UpdateError::Malformed(_))));
    }

    #[test]
    fn rejects_truncated_json() {
        let body = br#"{"latest":"1.0.0","minimum_su"#;
        assert!(matches!(parse_policy(body), Err(UpdateError::Malformed(_))));
    }

    #[test]
    fn rejects_minimum_above_latest() {
        let body = br#"{"latest":"1.0.0","minimum_supported":"1.1.0"}"#;
        assert!(matches!(parse_policy(body), Err(UpdateError::Malformed(_))));
    }

    #[test]
    fn message_defaults_to_empty() {
        let body = br#"{"latest":"1.0.0","minimum_supported":"1.0.0"}"#;
        assert_eq!(parse_policy(body).unwrap().message, "");
    }
}
```

- [ ] **Step 3: 테스트가 실패하는지 확인**

Run: `cargo test -p aragorn-app update::policy_doc`
Expected: 컴파일 실패 — `cannot find function sign_policy` 등

- [ ] **Step 4: 구현 작성**

`policy_doc.rs`의 `use` 아래, `#[cfg(test)]` 위에 추가 (`use` 줄도 아래처럼 교체):
```rust
use super::{UpdateError, UpdatePolicy};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use semver::Version;
use serde::Deserialize;

#[derive(Deserialize)]
struct PolicyDoc {
    latest: String,
    minimum_supported: String,
    #[serde(default)]
    message: String,
}

/// 서명을 검증한 뒤에만 본문을 해석한다.
pub fn verify_and_parse(
    json: &[u8],
    signature: &[u8],
    public_key: &[u8; 32],
) -> Result<UpdatePolicy, UpdateError> {
    let key = VerifyingKey::from_bytes(public_key).map_err(|_| UpdateError::BadSignature)?;
    let signature = Signature::from_slice(signature).map_err(|_| UpdateError::BadSignature)?;
    key.verify(json, &signature).map_err(|_| UpdateError::BadSignature)?;
    parse_policy(json)
}

pub fn parse_policy(json: &[u8]) -> Result<UpdatePolicy, UpdateError> {
    let doc: PolicyDoc =
        serde_json::from_slice(json).map_err(|e| UpdateError::Malformed(e.to_string()))?;
    let latest = parse_version("latest", &doc.latest)?;
    let minimum_supported = parse_version("minimum_supported", &doc.minimum_supported)?;
    if minimum_supported > latest {
        return Err(UpdateError::Malformed(format!(
            "minimum_supported({minimum_supported})가 latest({latest})보다 큽니다"
        )));
    }
    Ok(UpdatePolicy { latest, minimum_supported, message: doc.message })
}

fn parse_version(field: &str, value: &str) -> Result<Version, UpdateError> {
    Version::parse(value).map_err(|e| UpdateError::Malformed(format!("{field}: {e}")))
}

pub fn sign_policy(json: &[u8], secret: &[u8; 32]) -> [u8; 64] {
    SigningKey::from_bytes(secret).sign(json).to_bytes()
}

pub fn public_key_for(secret: &[u8; 32]) -> [u8; 32] {
    SigningKey::from_bytes(secret).verifying_key().to_bytes()
}
```

- [ ] **Step 5: 테스트 통과 확인**

Run: `cargo test -p aragorn-app`
Expected: 18 passed (decision 9 + policy_doc 9)

- [ ] **Step 6: 포맷, 린트, 커밋**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add crates/aragorn-app
git commit -m "feat(app): parse and verify signed update policy

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: 업데이트 상태 머신 `UpdateFlow`

**Files:**
- Create: `crates/aragorn-app/src/update/flow.rs`
- Modify: `crates/aragorn-app/src/update/mod.rs`

**Interfaces:**
- Consumes: `decide`, `UpdateDecision`, `UpdatePolicy`, `UpdatePrefs` (Task 1), `UpdateError` (Task 2)
- Produces:
  - `UpdateState { Idle, Checking, UpToDate, CheckFailed { error }, SoftAvailable { version, message }, Forced { version, message }, Downloading { version, forced }, ReadyToRestart { version, forced }, Failed { version, forced, error } }`
  - `UpdateEvent { CheckRequested, PolicyFetched(Result<UpdatePolicy, UpdateError>), UpdateNow, Later, SkipVersion, SetAutoDownload(bool), DownloadFinished(Result<(), UpdateError>), RestartNow, ApplyFailed(UpdateError), AppExiting }`
  - `UpdateCommand { FetchPolicy, Download(Version), ApplyAndRestart, ApplyOnExit, SavePrefs(UpdatePrefs) }`
  - `UpdateFlow::new(current: Version, prefs: UpdatePrefs, updater_available: bool) -> UpdateFlow`
  - `UpdateFlow::handle(&mut self, event: UpdateEvent) -> Vec<UpdateCommand>`
  - `UpdateFlow::{state() -> &UpdateState, prefs() -> &UpdatePrefs, current() -> &Version, updater_available() -> bool, blocks_emulation() -> bool}`

- [ ] **Step 1: 모듈 등록**

`crates/aragorn-app/src/update/mod.rs`:
```rust
mod decision;
mod error;
mod flow;
mod policy_doc;

pub use decision::{UpdateDecision, UpdatePolicy, UpdatePrefs, decide};
pub use error::UpdateError;
pub use flow::{UpdateCommand, UpdateEvent, UpdateFlow, UpdateState};
pub use policy_doc::{parse_policy, public_key_for, sign_policy, verify_and_parse};
```

- [ ] **Step 2: 실패하는 테스트 작성**

`crates/aragorn-app/src/update/flow.rs`:
```rust
use super::{UpdateDecision, UpdateError, UpdatePolicy, UpdatePrefs, decide};
use semver::Version;

#[cfg(test)]
mod tests {
    use super::*;
    use UpdateCommand as C;
    use UpdateEvent as E;
    use UpdateState as S;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    fn policy(latest: &str, min: &str) -> UpdatePolicy {
        UpdatePolicy { latest: v(latest), minimum_supported: v(min), message: "msg".into() }
    }

    fn flow(auto_download: bool, updater_available: bool) -> UpdateFlow {
        UpdateFlow::new(
            v("1.0.0"),
            UpdatePrefs { auto_download, skipped_version: None },
            updater_available,
        )
    }

    fn checked(f: &mut UpdateFlow, latest: &str, min: &str) -> Vec<UpdateCommand> {
        assert_eq!(f.handle(E::CheckRequested), vec![C::FetchPolicy]);
        f.handle(E::PolicyFetched(Ok(policy(latest, min))))
    }

    #[test]
    fn check_requested_starts_fetch() {
        let mut f = flow(true, true);
        assert_eq!(f.handle(E::CheckRequested), vec![C::FetchPolicy]);
        assert_eq!(f.state(), &S::Checking);
    }

    #[test]
    fn up_to_date_policy() {
        let mut f = flow(true, true);
        assert_eq!(checked(&mut f, "1.0.0", "1.0.0"), vec![]);
        assert_eq!(f.state(), &S::UpToDate);
    }

    #[test]
    fn soft_without_auto_download_shows_banner() {
        let mut f = flow(false, true);
        assert_eq!(checked(&mut f, "1.1.0", "1.0.0"), vec![]);
        assert_eq!(f.state(), &S::SoftAvailable { version: v("1.1.0"), message: "msg".into() });
        assert!(!f.blocks_emulation());
    }

    #[test]
    fn soft_with_auto_download_starts_download() {
        let mut f = flow(true, true);
        assert_eq!(checked(&mut f, "1.1.0", "1.0.0"), vec![C::Download(v("1.1.0"))]);
        assert_eq!(f.state(), &S::Downloading { version: v("1.1.0"), forced: false });
    }

    #[test]
    fn soft_with_auto_download_but_no_updater_shows_banner() {
        let mut f = flow(true, false);
        assert_eq!(checked(&mut f, "1.1.0", "1.0.0"), vec![]);
        assert!(matches!(f.state(), S::SoftAvailable { .. }));
    }

    #[test]
    fn forced_waits_for_user_then_downloads_and_restarts() {
        let mut f = flow(true, true);
        assert_eq!(checked(&mut f, "1.2.0", "1.1.0"), vec![]);
        assert_eq!(f.state(), &S::Forced { version: v("1.2.0"), message: "msg".into() });
        assert!(f.blocks_emulation());

        assert_eq!(f.handle(E::UpdateNow), vec![C::Download(v("1.2.0"))]);
        assert!(f.blocks_emulation());

        assert_eq!(f.handle(E::DownloadFinished(Ok(()))), vec![C::ApplyAndRestart]);
        assert_eq!(f.state(), &S::ReadyToRestart { version: v("1.2.0"), forced: true });
    }

    #[test]
    fn forced_download_failure_keeps_blocking_and_allows_retry() {
        let mut f = flow(true, true);
        checked(&mut f, "1.2.0", "1.1.0");
        f.handle(E::UpdateNow);
        let err = UpdateError::Download("boom".into());
        assert_eq!(f.handle(E::DownloadFinished(Err(err.clone()))), vec![]);
        assert_eq!(f.state(), &S::Failed { version: v("1.2.0"), forced: true, error: err });
        assert!(f.blocks_emulation());

        assert_eq!(f.handle(E::Later), vec![]);
        assert!(f.blocks_emulation(), "강제 업데이트 실패는 '나중에'로 넘길 수 없다");

        assert_eq!(f.handle(E::UpdateNow), vec![C::Download(v("1.2.0"))]);
    }

    #[test]
    fn forced_without_updater_ignores_update_now() {
        let mut f = flow(true, false);
        checked(&mut f, "1.2.0", "1.1.0");
        assert_eq!(f.handle(E::UpdateNow), vec![]);
        assert!(matches!(f.state(), S::Forced { .. }));
        assert!(f.blocks_emulation());
    }

    #[test]
    fn check_failure_does_not_block() {
        let mut f = flow(true, true);
        f.handle(E::CheckRequested);
        let err = UpdateError::Network("offline".into());
        assert_eq!(f.handle(E::PolicyFetched(Err(err.clone()))), vec![]);
        assert_eq!(f.state(), &S::CheckFailed { error: err });
        assert!(!f.blocks_emulation());
    }

    #[test]
    fn skip_version_saves_prefs() {
        let mut f = flow(false, true);
        checked(&mut f, "1.1.0", "1.0.0");
        let expected = UpdatePrefs { auto_download: false, skipped_version: Some(v("1.1.0")) };
        assert_eq!(f.handle(E::SkipVersion), vec![C::SavePrefs(expected.clone())]);
        assert_eq!(f.prefs(), &expected);
        assert_eq!(f.state(), &S::Idle);
    }

    #[test]
    fn later_hides_banner() {
        let mut f = flow(false, true);
        checked(&mut f, "1.1.0", "1.0.0");
        assert_eq!(f.handle(E::Later), vec![]);
        assert_eq!(f.state(), &S::Idle);
    }

    #[test]
    fn soft_download_ready_applies_on_exit() {
        let mut f = flow(true, true);
        checked(&mut f, "1.1.0", "1.0.0");
        assert_eq!(f.handle(E::DownloadFinished(Ok(()))), vec![]);
        assert_eq!(f.state(), &S::ReadyToRestart { version: v("1.1.0"), forced: false });
        assert_eq!(f.handle(E::AppExiting), vec![C::ApplyOnExit]);
    }

    #[test]
    fn restart_now_applies_ready_update() {
        let mut f = flow(true, true);
        checked(&mut f, "1.1.0", "1.0.0");
        f.handle(E::DownloadFinished(Ok(())));
        assert_eq!(f.handle(E::RestartNow), vec![C::ApplyAndRestart]);
    }

    #[test]
    fn apply_failure_moves_to_failed() {
        let mut f = flow(true, true);
        checked(&mut f, "1.1.0", "1.0.0");
        f.handle(E::DownloadFinished(Ok(())));
        let err = UpdateError::Apply("locked".into());
        assert_eq!(f.handle(E::ApplyFailed(err.clone())), vec![]);
        assert_eq!(f.state(), &S::Failed { version: v("1.1.0"), forced: false, error: err });
    }

    #[test]
    fn exiting_without_ready_update_does_nothing() {
        let mut f = flow(true, true);
        assert_eq!(f.handle(E::AppExiting), vec![]);
    }

    #[test]
    fn check_ignored_while_downloading() {
        let mut f = flow(true, true);
        checked(&mut f, "1.1.0", "1.0.0");
        assert_eq!(f.handle(E::CheckRequested), vec![]);
        assert!(matches!(f.state(), S::Downloading { .. }));
    }

    #[test]
    fn stale_policy_result_is_ignored() {
        let mut f = flow(true, true);
        assert_eq!(f.handle(E::PolicyFetched(Ok(policy("1.1.0", "1.0.0")))), vec![]);
        assert_eq!(f.state(), &S::Idle);
    }

    #[test]
    fn set_auto_download_saves_prefs() {
        let mut f = flow(true, true);
        let expected = UpdatePrefs { auto_download: false, skipped_version: None };
        assert_eq!(f.handle(E::SetAutoDownload(false)), vec![C::SavePrefs(expected)]);
        assert!(!f.prefs().auto_download);
    }
}
```

- [ ] **Step 3: 테스트가 실패하는지 확인**

Run: `cargo test -p aragorn-app update::flow`
Expected: 컴파일 실패 — `cannot find type UpdateFlow`

- [ ] **Step 4: 구현 작성**

`flow.rs`의 `use` 아래, `#[cfg(test)]` 위에 추가:
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateState {
    Idle,
    Checking,
    UpToDate,
    CheckFailed { error: UpdateError },
    SoftAvailable { version: Version, message: String },
    Forced { version: Version, message: String },
    Downloading { version: Version, forced: bool },
    ReadyToRestart { version: Version, forced: bool },
    Failed { version: Version, forced: bool, error: UpdateError },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateEvent {
    /// 앱 시작, 6시간 주기, 사용자 "업데이트 확인"
    CheckRequested,
    PolicyFetched(Result<UpdatePolicy, UpdateError>),
    /// "지금 업데이트", 강제 모달의 "업데이트", "다시 시도"
    UpdateNow,
    Later,
    SkipVersion,
    SetAutoDownload(bool),
    DownloadFinished(Result<(), UpdateError>),
    RestartNow,
    ApplyFailed(UpdateError),
    AppExiting,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateCommand {
    FetchPolicy,
    Download(Version),
    ApplyAndRestart,
    ApplyOnExit,
    SavePrefs(UpdatePrefs),
}

/// 업데이트 흐름의 순수 상태 머신. 입출력은 하지 않고, 실행할 명령만 돌려준다.
pub struct UpdateFlow {
    current: Version,
    prefs: UpdatePrefs,
    updater_available: bool,
    state: UpdateState,
}

impl UpdateFlow {
    pub fn new(current: Version, prefs: UpdatePrefs, updater_available: bool) -> Self {
        Self { current, prefs, updater_available, state: UpdateState::Idle }
    }

    pub fn state(&self) -> &UpdateState {
        &self.state
    }

    pub fn prefs(&self) -> &UpdatePrefs {
        &self.prefs
    }

    pub fn current(&self) -> &Version {
        &self.current
    }

    pub fn updater_available(&self) -> bool {
        self.updater_available
    }

    /// 강제 업데이트가 끝나기 전까지 게임을 진행하면 안 되는 상태인가.
    pub fn blocks_emulation(&self) -> bool {
        matches!(
            self.state,
            UpdateState::Forced { .. }
                | UpdateState::Downloading { forced: true, .. }
                | UpdateState::ReadyToRestart { forced: true, .. }
                | UpdateState::Failed { forced: true, .. }
        )
    }

    pub fn handle(&mut self, event: UpdateEvent) -> Vec<UpdateCommand> {
        use UpdateState as S;
        match event {
            UpdateEvent::CheckRequested => {
                let busy = matches!(
                    self.state,
                    S::Checking | S::Downloading { .. } | S::ReadyToRestart { .. }
                );
                if busy || self.blocks_emulation() {
                    return vec![];
                }
                self.state = S::Checking;
                vec![UpdateCommand::FetchPolicy]
            }
            UpdateEvent::PolicyFetched(result) => {
                if self.state != S::Checking {
                    return vec![];
                }
                match result {
                    Ok(policy) => self.apply_decision(&policy),
                    Err(error) => {
                        self.state = S::CheckFailed { error };
                        vec![]
                    }
                }
            }
            UpdateEvent::UpdateNow => match self.state.clone() {
                S::SoftAvailable { version, .. } => self.start_download(version, false),
                S::Forced { version, .. } => self.start_download(version, true),
                S::Failed { version, forced, .. } => self.start_download(version, forced),
                _ => vec![],
            },
            UpdateEvent::Later => {
                if matches!(self.state, S::SoftAvailable { .. } | S::Failed { forced: false, .. }) {
                    self.state = S::Idle;
                }
                vec![]
            }
            UpdateEvent::SkipVersion => {
                let S::SoftAvailable { version, .. } = &self.state else {
                    return vec![];
                };
                self.prefs.skipped_version = Some(version.clone());
                self.state = S::Idle;
                vec![UpdateCommand::SavePrefs(self.prefs.clone())]
            }
            UpdateEvent::SetAutoDownload(on) => {
                self.prefs.auto_download = on;
                vec![UpdateCommand::SavePrefs(self.prefs.clone())]
            }
            UpdateEvent::DownloadFinished(result) => {
                let S::Downloading { version, forced } = self.state.clone() else {
                    return vec![];
                };
                match result {
                    Ok(()) => {
                        self.state = S::ReadyToRestart { version, forced };
                        if forced { vec![UpdateCommand::ApplyAndRestart] } else { vec![] }
                    }
                    Err(error) => {
                        self.state = S::Failed { version, forced, error };
                        vec![]
                    }
                }
            }
            UpdateEvent::RestartNow => {
                if matches!(self.state, S::ReadyToRestart { .. }) {
                    vec![UpdateCommand::ApplyAndRestart]
                } else {
                    vec![]
                }
            }
            UpdateEvent::ApplyFailed(error) => {
                let S::ReadyToRestart { version, forced } = self.state.clone() else {
                    return vec![];
                };
                self.state = S::Failed { version, forced, error };
                vec![]
            }
            UpdateEvent::AppExiting => {
                if matches!(self.state, S::ReadyToRestart { forced: false, .. }) {
                    vec![UpdateCommand::ApplyOnExit]
                } else {
                    vec![]
                }
            }
        }
    }

    fn apply_decision(&mut self, policy: &UpdatePolicy) -> Vec<UpdateCommand> {
        match decide(&self.current, policy, &self.prefs) {
            UpdateDecision::UpToDate => {
                self.state = UpdateState::UpToDate;
                vec![]
            }
            UpdateDecision::Soft { version, auto_download } => {
                if auto_download && self.updater_available {
                    self.start_download(version, false)
                } else {
                    self.state = UpdateState::SoftAvailable { version, message: policy.message.clone() };
                    vec![]
                }
            }
            UpdateDecision::Forced { version } => {
                self.state = UpdateState::Forced { version, message: policy.message.clone() };
                vec![]
            }
        }
    }

    fn start_download(&mut self, version: Version, forced: bool) -> Vec<UpdateCommand> {
        if !self.updater_available {
            return vec![];
        }
        self.state = UpdateState::Downloading { version: version.clone(), forced };
        vec![UpdateCommand::Download(version)]
    }
}
```

- [ ] **Step 5: 테스트 통과 확인**

Run: `cargo test -p aragorn-app`
Expected: 36 passed (기존 18 + flow 18)

- [ ] **Step 6: 포맷, 린트, 커밋**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add crates/aragorn-app
git commit -m "feat(app): add update flow state machine

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: 업데이트 포트, 명령 실행기, 설정 모델

**Files:**
- Create: `crates/aragorn-app/src/update/ports.rs`
- Create: `crates/aragorn-app/src/config.rs`
- Modify: `crates/aragorn-app/src/update/mod.rs`, `crates/aragorn-app/src/lib.rs`

**Interfaces:**
- Consumes: `UpdateCommand`, `UpdateEvent` (Task 3), `UpdatePolicy`, `UpdatePrefs`, `UpdateError`
- Produces:
  - `trait UpdateSource: Send + Sync { fn fetch_policy(&self) -> Result<UpdatePolicy, UpdateError>; }`
  - `trait Updater: Send + Sync { fn download(&self, version: &Version) -> Result<(), UpdateError>; fn apply_and_restart(&self) -> Result<(), UpdateError>; fn apply_on_exit(&self) -> Result<(), UpdateError>; }`
  - `aragorn_app::update::execute(command: &UpdateCommand, source: &dyn UpdateSource, updater: &dyn Updater) -> Option<UpdateEvent>` (`SavePrefs`는 호출자가 처리하므로 `None`)
  - `aragorn_app::config::{Config { pub update: UpdateConfig }, UpdateConfig { pub auto_download: bool, pub skipped_version: Option<String> }, ConfigStore}`
  - `UpdateConfig::to_prefs(&self) -> UpdatePrefs`, `UpdateConfig::from_prefs(prefs: &UpdatePrefs) -> UpdateConfig`
  - `trait ConfigStore { fn load(&self) -> Config; fn save(&self, config: &Config) -> std::io::Result<()>; }`

- [ ] **Step 1: 모듈 등록**

`crates/aragorn-app/src/update/mod.rs`:
```rust
mod decision;
mod error;
mod flow;
mod policy_doc;
mod ports;

pub use decision::{UpdateDecision, UpdatePolicy, UpdatePrefs, decide};
pub use error::UpdateError;
pub use flow::{UpdateCommand, UpdateEvent, UpdateFlow, UpdateState};
pub use policy_doc::{parse_policy, public_key_for, sign_policy, verify_and_parse};
pub use ports::{UpdateSource, Updater, execute};
```

`crates/aragorn-app/src/lib.rs`:
```rust
//! 유스케이스 계층: 세션, 설정, 업데이트 정책. 외부 세계와는 포트 trait으로만 대화한다.

pub mod config;
pub mod update;
```

- [ ] **Step 2: 포트 실패 테스트 작성**

`crates/aragorn-app/src/update/ports.rs`:
```rust
use super::{UpdateCommand, UpdateError, UpdateEvent, UpdatePolicy};
use semver::Version;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct FakeSource(Result<UpdatePolicy, UpdateError>);

    impl UpdateSource for FakeSource {
        fn fetch_policy(&self) -> Result<UpdatePolicy, UpdateError> {
            self.0.clone()
        }
    }

    struct FakeUpdater {
        result: Result<(), UpdateError>,
        calls: Mutex<Vec<String>>,
    }

    impl FakeUpdater {
        fn new(result: Result<(), UpdateError>) -> Self {
            Self { result, calls: Mutex::new(vec![]) }
        }
        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl Updater for FakeUpdater {
        fn download(&self, version: &Version) -> Result<(), UpdateError> {
            self.calls.lock().unwrap().push(format!("download {version}"));
            self.result.clone()
        }
        fn apply_and_restart(&self) -> Result<(), UpdateError> {
            self.calls.lock().unwrap().push("apply_and_restart".into());
            self.result.clone()
        }
        fn apply_on_exit(&self) -> Result<(), UpdateError> {
            self.calls.lock().unwrap().push("apply_on_exit".into());
            self.result.clone()
        }
    }

    fn policy() -> UpdatePolicy {
        UpdatePolicy {
            latest: Version::new(1, 1, 0),
            minimum_supported: Version::new(1, 0, 0),
            message: String::new(),
        }
    }

    #[test]
    fn fetch_policy_returns_policy_fetched() {
        let source = FakeSource(Ok(policy()));
        let updater = FakeUpdater::new(Ok(()));
        let ev = execute(&UpdateCommand::FetchPolicy, &source, &updater);
        assert_eq!(ev, Some(UpdateEvent::PolicyFetched(Ok(policy()))));
    }

    #[test]
    fn download_reports_result() {
        let source = FakeSource(Ok(policy()));
        let updater = FakeUpdater::new(Err(UpdateError::Download("x".into())));
        let ev = execute(&UpdateCommand::Download(Version::new(1, 1, 0)), &source, &updater);
        assert_eq!(ev, Some(UpdateEvent::DownloadFinished(Err(UpdateError::Download("x".into())))));
        assert_eq!(updater.calls(), vec!["download 1.1.0"]);
    }

    #[test]
    fn successful_apply_produces_no_event() {
        let source = FakeSource(Ok(policy()));
        let updater = FakeUpdater::new(Ok(()));
        assert_eq!(execute(&UpdateCommand::ApplyOnExit, &source, &updater), None);
        assert_eq!(updater.calls(), vec!["apply_on_exit"]);
    }

    #[test]
    fn failed_apply_produces_apply_failed() {
        let source = FakeSource(Ok(policy()));
        let updater = FakeUpdater::new(Err(UpdateError::Apply("locked".into())));
        let ev = execute(&UpdateCommand::ApplyAndRestart, &source, &updater);
        assert_eq!(ev, Some(UpdateEvent::ApplyFailed(UpdateError::Apply("locked".into()))));
    }

    #[test]
    fn save_prefs_is_left_to_caller() {
        let source = FakeSource(Ok(policy()));
        let updater = FakeUpdater::new(Ok(()));
        let cmd = UpdateCommand::SavePrefs(Default::default());
        assert_eq!(execute(&cmd, &source, &updater), None);
        assert!(updater.calls().is_empty());
    }
}
```

- [ ] **Step 3: 설정 실패 테스트 작성**

`crates/aragorn-app/src/config.rs`:
```rust
use crate::update::UpdatePrefs;
use semver::Version;
use serde::{Deserialize, Serialize};

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
        let prefs = UpdatePrefs { auto_download: false, skipped_version: Some(Version::new(1, 2, 3)) };
        let cfg = UpdateConfig::from_prefs(&prefs);
        assert_eq!(cfg.skipped_version.as_deref(), Some("1.2.3"));
        assert_eq!(cfg.to_prefs(), prefs);
    }
}
```

- [ ] **Step 4: 테스트가 실패하는지 확인**

Run: `cargo test -p aragorn-app`
Expected: 컴파일 실패 — `cannot find function execute`, `cannot find type Config`

- [ ] **Step 5: 포트 구현**

`ports.rs`의 `use` 아래, `#[cfg(test)]` 위에 추가:
```rust
/// 서명 검증까지 끝낸 업데이트 정책을 가져온다.
pub trait UpdateSource: Send + Sync {
    fn fetch_policy(&self) -> Result<UpdatePolicy, UpdateError>;
}

/// 업데이트 패키지를 내려받고 설치한다.
pub trait Updater: Send + Sync {
    fn download(&self, version: &Version) -> Result<(), UpdateError>;
    /// 성공하면 프로세스가 재시작되므로 반환하지 않을 수 있다.
    fn apply_and_restart(&self) -> Result<(), UpdateError>;
    /// 앱이 종료된 뒤 업데이트가 적용되도록 예약한다.
    fn apply_on_exit(&self) -> Result<(), UpdateError>;
}

/// 입출력 명령을 실행하고, 상태 머신에 돌려줄 이벤트를 만든다.
/// `SavePrefs`는 설정 저장소를 가진 호출자가 처리한다.
pub fn execute(
    command: &UpdateCommand,
    source: &dyn UpdateSource,
    updater: &dyn Updater,
) -> Option<UpdateEvent> {
    match command {
        UpdateCommand::FetchPolicy => Some(UpdateEvent::PolicyFetched(source.fetch_policy())),
        UpdateCommand::Download(version) => {
            Some(UpdateEvent::DownloadFinished(updater.download(version)))
        }
        UpdateCommand::ApplyAndRestart => updater.apply_and_restart().err().map(UpdateEvent::ApplyFailed),
        UpdateCommand::ApplyOnExit => updater.apply_on_exit().err().map(UpdateEvent::ApplyFailed),
        UpdateCommand::SavePrefs(_) => None,
    }
}
```

- [ ] **Step 6: 설정 구현**

`config.rs`의 `use` 아래, `#[cfg(test)]` 위에 추가:
```rust
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
        Self { auto_download: true, skipped_version: None }
    }
}

impl UpdateConfig {
    /// 해석할 수 없는 버전 문자열은 무시한다.
    pub fn to_prefs(&self) -> UpdatePrefs {
        UpdatePrefs {
            auto_download: self.auto_download,
            skipped_version: self.skipped_version.as_deref().and_then(|s| Version::parse(s).ok()),
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
```

- [ ] **Step 7: 테스트 통과 확인**

Run: `cargo test -p aragorn-app`
Expected: 45 passed (기존 36 + ports 5 + config 4)

- [ ] **Step 8: 포맷, 린트, 커밋**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add crates/aragorn-app
git commit -m "feat(app): add update ports, command executor and config model

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: 릴리스 도구 `xtask` (키 생성, 정책 생성/서명, 태그 검증)

**Files:**
- Create: `xtask/Cargo.toml`, `xtask/src/main.rs`, `xtask/src/policy.rs`
- Create: `.cargo/config.toml`
- Create: `release/policy.toml`, `release/policy.pub` (생성 명령으로 만듦)
- Modify: `Cargo.toml` (members에 `"xtask"` 추가)

**Interfaces:**
- Consumes: `aragorn_app::update::{parse_policy, sign_policy, verify_and_parse, public_key_for}` (Task 2)
- Produces:
  - `cargo xtask gen-key <secret-out>`: 비밀키(base64)를 `<secret-out>`(권한 600)에, 공개키 32바이트를 `release/policy.pub`에 쓴다. 이미 있으면 덮어쓰지 않는다.
  - `cargo xtask make-policy <out-dir>`: `release/policy.toml`과 워크스페이스 버전으로 `<out-dir>/update-policy.json`과 `.sig`를 만든다. 환경 변수 `ARAGORN_POLICY_SIGNING_KEY`(base64)가 필요하다.
  - `cargo xtask check-tag <tag>`: 태그가 `v{workspace version}`이 아니면 종료 코드 1
  - `release/policy.pub`: Task 6의 `include_bytes!` 대상

- [ ] **Step 1: 크레이트 뼈대 작성**

`Cargo.toml`의 members 수정:
```toml
members = ["crates/gb-core", "crates/aragorn-app", "xtask"]
```

`xtask/Cargo.toml`:
```toml
[package]
name = "xtask"
version.workspace = true
edition.workspace = true
license.workspace = true
publish = false

[dependencies]
aragorn-app = { path = "../crates/aragorn-app" }
base64 = "0.22"
getrandom = "0.3"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
```

`.cargo/config.toml`:
```toml
[alias]
xtask = "run --quiet --package xtask --"
```

`release/policy.toml`:
```toml
# 강제 업데이트 기준 버전. 세이브 손상 같은 치명적 결함이 있을 때만 올린다.
minimum_supported = "0.1.0"
# 업데이트 알림에 함께 표시할 문구 (비워 두어도 된다)
message = ""
```

- [ ] **Step 2: 실패하는 테스트 작성**

`xtask/src/policy.rs`:
```rust
use aragorn_app::update::parse_policy;
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use serde::Deserialize;

#[cfg(test)]
mod tests {
    use super::*;
    use aragorn_app::update::{public_key_for, sign_policy, verify_and_parse};

    fn toml_policy(min: &str) -> PolicyToml {
        PolicyToml { minimum_supported: min.into(), message: "hi".into() }
    }

    #[test]
    fn builds_policy_with_workspace_version_as_latest() {
        let json = build_policy_json("0.2.0", &toml_policy("0.1.0")).unwrap();
        let p = parse_policy(&json).unwrap();
        assert_eq!(p.latest.to_string(), "0.2.0");
        assert_eq!(p.minimum_supported.to_string(), "0.1.0");
        assert_eq!(p.message, "hi");
    }

    #[test]
    fn rejects_minimum_above_version() {
        assert!(build_policy_json("0.1.0", &toml_policy("0.2.0")).is_err());
    }

    #[test]
    fn built_policy_verifies_after_signing() {
        let secret = [3u8; 32];
        let json = build_policy_json("0.1.0", &toml_policy("0.1.0")).unwrap();
        let sig = sign_policy(&json, &secret);
        assert!(verify_and_parse(&json, &sig, &public_key_for(&secret)).is_ok());
    }

    #[test]
    fn check_tag_accepts_matching_tag() {
        assert!(check_tag("v0.1.0", "0.1.0").is_ok());
    }

    #[test]
    fn check_tag_rejects_mismatch() {
        assert!(check_tag("v0.1.1", "0.1.0").is_err());
        assert!(check_tag("0.1.0", "0.1.0").is_err());
    }

    #[test]
    fn decode_secret_round_trip() {
        let secret = [5u8; 32];
        assert_eq!(decode_secret(&format!("{}\n", B64.encode(secret))).unwrap(), secret);
    }

    #[test]
    fn decode_secret_rejects_wrong_length() {
        assert!(decode_secret(&B64.encode([1u8; 16])).is_err());
        assert!(decode_secret("not base64!").is_err());
    }
}
```

`xtask/src/main.rs` (테스트 컴파일용 최소 형태):
```rust
mod policy;

fn main() {}
```

- [ ] **Step 3: 테스트가 실패하는지 확인**

Run: `cargo test -p xtask`
Expected: 컴파일 실패 — `cannot find type PolicyToml`, `cannot find function build_policy_json`

- [ ] **Step 4: `policy.rs` 구현**

`policy.rs`의 `use` 아래, `#[cfg(test)]` 위에 추가:
```rust
#[derive(Debug, Deserialize)]
pub struct PolicyToml {
    pub minimum_supported: String,
    #[serde(default)]
    pub message: String,
}

/// 배포용 정책 JSON을 만들고, 앱이 읽을 수 있는 형식인지 미리 검증한다.
pub fn build_policy_json(version: &str, policy: &PolicyToml) -> Result<Vec<u8>, String> {
    let doc = serde_json::json!({
        "latest": version,
        "minimum_supported": policy.minimum_supported,
        "message": policy.message,
    });
    let json = serde_json::to_vec_pretty(&doc).map_err(|e| e.to_string())?;
    parse_policy(&json).map_err(|e| e.to_string())?;
    Ok(json)
}

pub fn check_tag(tag: &str, version: &str) -> Result<(), String> {
    let expected = format!("v{version}");
    if tag == expected {
        Ok(())
    } else {
        Err(format!("태그 {tag}가 워크스페이스 버전 {expected}와 다릅니다"))
    }
}

pub fn decode_secret(b64: &str) -> Result<[u8; 32], String> {
    let bytes = B64.decode(b64.trim()).map_err(|e| format!("비밀키 base64 해석 실패: {e}"))?;
    bytes.try_into().map_err(|_| "비밀키는 32바이트여야 합니다".to_string())
}

pub fn encode_secret(secret: &[u8; 32]) -> String {
    B64.encode(secret)
}
```

- [ ] **Step 5: 테스트 통과 확인**

Run: `cargo test -p xtask`
Expected: 7 passed

- [ ] **Step 6: `main.rs` 명령 구현**

`xtask/src/main.rs` 전체:
```rust
mod policy;

use aragorn_app::update::{public_key_for, sign_policy, verify_and_parse};
use policy::{PolicyToml, build_policy_json, check_tag, decode_secret, encode_secret};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const USAGE: &str = "사용법: cargo xtask <gen-key <secret-out> | make-policy <out-dir> | check-tag <tag>>";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let arg = args.get(1).map(String::as_str);
    let result = match args.first().map(String::as_str) {
        Some("gen-key") => gen_key(arg),
        Some("make-policy") => make_policy(arg),
        Some("check-tag") => arg.ok_or_else(|| USAGE.to_string()).and_then(|tag| check_tag(tag, VERSION)),
        _ => Err(USAGE.to_string()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("오류: {e}");
            ExitCode::FAILURE
        }
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("xtask는 워크스페이스 안에 있다").to_path_buf()
}

fn public_key_path() -> PathBuf {
    root().join("release/policy.pub")
}

fn gen_key(secret_out: Option<&str>) -> Result<(), String> {
    let out = PathBuf::from(secret_out.ok_or_else(|| USAGE.to_string())?);
    if out.exists() {
        return Err(format!("{}가 이미 있습니다. 덮어쓰지 않습니다", out.display()));
    }
    let mut secret = [0u8; 32];
    getrandom::fill(&mut secret).map_err(|e| e.to_string())?;
    if let Some(dir) = out.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::write(&out, encode_secret(&secret)).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&out, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
    }
    fs::write(public_key_path(), public_key_for(&secret)).map_err(|e| e.to_string())?;
    println!("비밀키: {} (저장소에 커밋하지 말 것)", out.display());
    println!("공개키: {}", public_key_path().display());
    Ok(())
}

fn make_policy(out_dir: Option<&str>) -> Result<(), String> {
    let out = PathBuf::from(out_dir.ok_or_else(|| USAGE.to_string())?);
    let text = fs::read_to_string(root().join("release/policy.toml")).map_err(|e| e.to_string())?;
    let policy: PolicyToml = toml::from_str(&text).map_err(|e| e.to_string())?;
    let json = build_policy_json(VERSION, &policy)?;

    let secret_b64 = env::var("ARAGORN_POLICY_SIGNING_KEY")
        .map_err(|_| "ARAGORN_POLICY_SIGNING_KEY 환경 변수가 없습니다".to_string())?;
    let secret = decode_secret(&secret_b64)?;
    let signature = sign_policy(&json, &secret);

    let public: [u8; 32] = fs::read(public_key_path())
        .map_err(|e| e.to_string())?
        .try_into()
        .map_err(|_| "release/policy.pub는 32바이트여야 합니다".to_string())?;
    verify_and_parse(&json, &signature, &public)
        .map_err(|e| format!("서명 키와 release/policy.pub가 맞지 않습니다: {e}"))?;

    fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    fs::write(out.join("update-policy.json"), &json).map_err(|e| e.to_string())?;
    fs::write(out.join("update-policy.json.sig"), signature).map_err(|e| e.to_string())?;
    println!("{} 에 정책 파일을 만들었습니다 (latest = {VERSION})", out.display());
    Ok(())
}
```

- [ ] **Step 7: 키 생성 후 명령 동작 확인**

```bash
cargo xtask gen-key ~/.config/aragorn-release/policy-signing-key.b64
ls -l release/policy.pub        # Expected: 32 bytes
cargo xtask check-tag v0.1.0    # Expected: 종료 코드 0, 출력 없음
cargo xtask check-tag v9.9.9; echo $?   # Expected: "오류: 태그 v9.9.9가 ..." 와 1
ARAGORN_POLICY_SIGNING_KEY="$(cat ~/.config/aragorn-release/policy-signing-key.b64)" cargo xtask make-policy target/policy-check
ls -l target/policy-check       # Expected: update-policy.json, update-policy.json.sig(64 bytes)
```

- [ ] **Step 8: 포맷, 린트, 커밋** (비밀키는 저장소 밖에 있으므로 커밋되지 않는다)

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add Cargo.toml Cargo.lock .cargo/config.toml xtask release/policy.toml release/policy.pub
git commit -m "feat(xtask): add policy key generation, signing and tag check

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---

### Task 6: 데스크톱 어댑터 (정책 소스, 설정 저장소, 업데이터)

**Files:**
- Create: `crates/aragorn-desktop/Cargo.toml`, `crates/aragorn-desktop/src/lib.rs`
- Create: `crates/aragorn-desktop/src/build_info.rs`
- Create: `crates/aragorn-desktop/src/adapters/{mod.rs, github_policy.rs, toml_config.rs, velopack_updater.rs}`
- Modify: `Cargo.toml` (members에 `"crates/aragorn-desktop"` 추가)

**Interfaces:**
- Consumes: `UpdateSource`, `Updater`, `UpdateError`, `UpdatePolicy`, `verify_and_parse`, `sign_policy`, `public_key_for` (Task 2, 4), `Config`, `ConfigStore` (Task 4), `release/policy.pub` (Task 5)
- Produces:
  - `aragorn_desktop::build_info::{POLICY_PUBLIC_KEY: &[u8; 32], GITHUB_REPO: Option<&str>, current_version() -> Version, policy_base_url(repo: &str) -> String, repo_url(repo: &str) -> String, releases_page_url(repo: &str) -> String}`
  - `aragorn_desktop::adapters::GithubPolicySource::new(base_url: String, public_key: [u8; 32], timeout: Duration) -> Self` (impl `UpdateSource`)
  - `aragorn_desktop::adapters::TomlConfigStore::{new(path: PathBuf) -> Self, default_path() -> Option<PathBuf>}` (impl `ConfigStore`)
  - `aragorn_desktop::adapters::VelopackUpdater::new(repo_url: &str) -> Option<Self>` (impl `Updater`)
  - `aragorn_desktop::adapters::UnavailableUpdater` (impl `Updater`, 모든 메서드가 `Err(NotInstalled)`)

- [ ] **Step 1: 크레이트 뼈대 작성**

`Cargo.toml` members:
```toml
members = ["crates/gb-core", "crates/aragorn-app", "crates/aragorn-desktop", "xtask"]
```

`crates/aragorn-desktop/Cargo.toml`:
```toml
[package]
name = "aragorn-desktop"
version.workspace = true
edition.workspace = true
license.workspace = true

[lib]
name = "aragorn_desktop"
path = "src/lib.rs"

[dependencies]
aragorn-app = { path = "../aragorn-app" }
directories = "6"
log = "0.4"
semver = "1"
toml = "0.8"
ureq = "2"

[dev-dependencies]
tempfile = "3"
```

그다음 Velopack을 최신 버전으로 추가:
```bash
cargo add -p aragorn-desktop velopack
```

`crates/aragorn-desktop/src/lib.rs`:
```rust
//! 어댑터 계층: OS, 네트워크, UI 구현과 의존성 조립.

pub mod adapters;
pub mod build_info;
```

`crates/aragorn-desktop/src/adapters/mod.rs`:
```rust
mod github_policy;
mod toml_config;
mod velopack_updater;

pub use github_policy::GithubPolicySource;
pub use toml_config::TomlConfigStore;
pub use velopack_updater::{UnavailableUpdater, VelopackUpdater};
```

- [ ] **Step 2: `build_info` 테스트와 구현**

`crates/aragorn-desktop/src/build_info.rs`:
```rust
use semver::Version;

/// 업데이트 정책 서명 검증용 공개키 (`cargo xtask gen-key`로 생성).
pub const POLICY_PUBLIC_KEY: &[u8; 32] = include_bytes!("../../../release/policy.pub");

/// 릴리스 CI가 `ARAGORN_REPO=owner/repo`로 주입한다. 없으면 업데이트를 확인하지 않는다.
pub const GITHUB_REPO: Option<&str> = option_env!("ARAGORN_REPO");

pub fn current_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).expect("CARGO_PKG_VERSION은 semver다")
}

pub fn repo_url(repo: &str) -> String {
    format!("https://github.com/{repo}")
}

pub fn policy_base_url(repo: &str) -> String {
    format!("https://github.com/{repo}/releases/latest/download")
}

pub fn releases_page_url(repo: &str) -> String {
    format!("https://github.com/{repo}/releases/latest")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_follow_github_layout() {
        assert_eq!(repo_url("me/aragorn"), "https://github.com/me/aragorn");
        assert_eq!(
            policy_base_url("me/aragorn"),
            "https://github.com/me/aragorn/releases/latest/download"
        );
        assert_eq!(releases_page_url("me/aragorn"), "https://github.com/me/aragorn/releases/latest");
    }

    #[test]
    fn current_version_matches_workspace() {
        assert_eq!(current_version().to_string(), env!("CARGO_PKG_VERSION"));
    }
}
```

- [ ] **Step 3: `GithubPolicySource` 실패 테스트 작성**

`crates/aragorn-desktop/src/adapters/github_policy.rs`:
```rust
use aragorn_app::update::{UpdateError, UpdatePolicy, UpdateSource, verify_and_parse};
use std::{io::Read, time::Duration};

#[cfg(test)]
mod tests {
    use super::*;
    use aragorn_app::update::{public_key_for, sign_policy};
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpListener,
        thread,
        time::Instant,
    };

    const SECRET: [u8; 32] = [7; 32];
    const POLICY: &[u8] = br#"{"latest":"1.1.0","minimum_supported":"1.0.0","message":""}"#;

    /// 경로별 고정 응답을 주는 테스트용 HTTP 서버. 없는 경로는 404.
    fn serve(routes: Vec<(&'static str, Vec<u8>)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request_line = String::new();
                reader.read_line(&mut request_line).unwrap();
                loop {
                    let mut header = String::new();
                    reader.read_line(&mut header).unwrap();
                    if header == "\r\n" || header.is_empty() {
                        break;
                    }
                }
                let path = request_line.split_whitespace().nth(1).unwrap_or("").to_string();
                let (status, body): (&str, &[u8]) = match routes.iter().find(|(p, _)| *p == path) {
                    Some((_, body)) => ("200 OK", body),
                    None => ("404 Not Found", b"<html>Not Found</html>"),
                };
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                stream.write_all(body).unwrap();
            }
        });
        format!("http://{addr}")
    }

    fn source(base_url: String, timeout: Duration) -> GithubPolicySource {
        GithubPolicySource::new(base_url, public_key_for(&SECRET), timeout)
    }

    #[test]
    fn fetches_and_verifies_policy() {
        let sig = sign_policy(POLICY, &SECRET).to_vec();
        let base = serve(vec![
            ("/update-policy.json", POLICY.to_vec()),
            ("/update-policy.json.sig", sig),
        ]);
        let policy = source(base, Duration::from_secs(5)).fetch_policy().unwrap();
        assert_eq!(policy.latest.to_string(), "1.1.0");
    }

    #[test]
    fn tampered_policy_is_rejected() {
        let sig = sign_policy(POLICY, &SECRET).to_vec();
        let tampered = br#"{"latest":"9.0.0","minimum_supported":"9.0.0","message":""}"#.to_vec();
        let base = serve(vec![
            ("/update-policy.json", tampered),
            ("/update-policy.json.sig", sig),
        ]);
        let result = source(base, Duration::from_secs(5)).fetch_policy();
        assert_eq!(result, Err(UpdateError::BadSignature));
    }

    #[test]
    fn missing_policy_is_network_error() {
        let base = serve(vec![]);
        let result = source(base, Duration::from_secs(5)).fetch_policy();
        assert!(matches!(result, Err(UpdateError::Network(_))), "{result:?}");
    }

    #[test]
    fn hanging_server_times_out() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        thread::spawn(move || {
            let mut held = Vec::new();
            for stream in listener.incoming() {
                held.push(stream);
            }
        });
        let started = Instant::now();
        let result = source(base, Duration::from_millis(300)).fetch_policy();
        assert!(matches!(result, Err(UpdateError::Network(_))), "{result:?}");
        assert!(started.elapsed() < Duration::from_secs(3));
    }
}
```

- [ ] **Step 4: 테스트가 실패하는지 확인**

Run: `cargo test -p aragorn-desktop github_policy`
Expected: 컴파일 실패 — `cannot find type GithubPolicySource` (adapters/mod.rs의 다른 모듈 파일이 없으면 먼저 Step 6, 8의 파일을 빈 파일로 만들어 둔다: `touch crates/aragorn-desktop/src/adapters/{toml_config.rs,velopack_updater.rs}` 후 mod.rs의 해당 `pub use` 두 줄을 이 단계에서만 주석 처리)

- [ ] **Step 5: `GithubPolicySource` 구현**

`github_policy.rs`의 `use` 아래, `#[cfg(test)]` 위에 추가:
```rust
const POLICY_FILE: &str = "update-policy.json";
const SIGNATURE_FILE: &str = "update-policy.json.sig";
const MAX_BYTES: u64 = 64 * 1024;

/// GitHub Releases의 latest 릴리스에서 서명된 정책을 받아온다.
pub struct GithubPolicySource {
    agent: ureq::Agent,
    base_url: String,
    public_key: [u8; 32],
}

impl GithubPolicySource {
    pub fn new(base_url: String, public_key: [u8; 32], timeout: Duration) -> Self {
        let agent = ureq::AgentBuilder::new().timeout(timeout).build();
        Self { agent, base_url, public_key }
    }

    fn get(&self, file: &str) -> Result<Vec<u8>, UpdateError> {
        let url = format!("{}/{file}", self.base_url);
        let response = self
            .agent
            .get(&url)
            .call()
            .map_err(|e| UpdateError::Network(e.to_string()))?;
        let mut body = Vec::new();
        response
            .into_reader()
            .take(MAX_BYTES)
            .read_to_end(&mut body)
            .map_err(|e| UpdateError::Network(e.to_string()))?;
        Ok(body)
    }
}

impl UpdateSource for GithubPolicySource {
    fn fetch_policy(&self) -> Result<UpdatePolicy, UpdateError> {
        let json = self.get(POLICY_FILE)?;
        let signature = self.get(SIGNATURE_FILE)?;
        verify_and_parse(&json, &signature, &self.public_key)
    }
}
```

- [ ] **Step 6: `TomlConfigStore` 실패 테스트 작성**

`crates/aragorn-desktop/src/adapters/toml_config.rs`:
```rust
use aragorn_app::config::{Config, ConfigStore};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[cfg(test)]
mod tests {
    use super::*;

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
        fs::write(dir.path().join("nested/config.toml"), "this is = = not toml").unwrap();
        assert_eq!(store.load(), Config::default());
    }

    #[test]
    fn partial_file_keeps_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(dir.path());
        fs::create_dir_all(dir.path().join("nested")).unwrap();
        fs::write(dir.path().join("nested/config.toml"), "[update]\nskipped_version = \"1.0.0\"\n").unwrap();
        let config = store.load();
        assert!(config.update.auto_download);
        assert_eq!(config.update.skipped_version.as_deref(), Some("1.0.0"));
    }
}
```

- [ ] **Step 7: `TomlConfigStore` 구현**

`toml_config.rs`의 `use` 아래, `#[cfg(test)]` 위에 추가:
```rust
pub struct TomlConfigStore {
    path: PathBuf,
}

impl TomlConfigStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// OS 설정 디렉터리의 `Aragorn/config.toml`
    pub fn default_path() -> Option<PathBuf> {
        directories::ProjectDirs::from("", "", "Aragorn").map(|d| d.config_dir().join("config.toml"))
    }
}

impl ConfigStore for TomlConfigStore {
    fn load(&self) -> Config {
        match fs::read_to_string(&self.path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                log::warn!("설정 파일을 읽을 수 없어 기본값을 씁니다 ({}): {e}", self.path.display());
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
```
(`Path`는 테스트에서만 쓰이면 clippy가 unused import를 경고하므로, 경고가 나면 상단 `use`에서 `Path`를 빼고 테스트 모듈에 `use std::path::Path;`를 둔다.)

- [ ] **Step 8: 업데이터 테스트와 구현**

`crates/aragorn-desktop/src/adapters/velopack_updater.rs`:
```rust
use aragorn_app::update::{UpdateError, Updater};
use semver::Version;
use std::sync::Mutex;
use velopack::{UpdateCheck, UpdateInfo, UpdateManager, sources::GithubSource};

/// Velopack으로 설치된 앱에서만 동작하는 업데이터.
pub struct VelopackUpdater {
    manager: UpdateManager,
    pending: Mutex<Option<UpdateInfo>>,
}

impl VelopackUpdater {
    /// Velopack 설치판이 아니면(`cargo run`, 압축 해제 실행 등) `None`.
    pub fn new(repo_url: &str) -> Option<Self> {
        let source = GithubSource::new(repo_url, None, false);
        match UpdateManager::new(source, None, None) {
            Ok(manager) => Some(Self { manager, pending: Mutex::new(None) }),
            Err(e) => {
                log::info!("자동 업데이트 비활성화 (설치판 아님): {e}");
                None
            }
        }
    }

    fn with_pending<T>(&self, f: impl FnOnce(&UpdateInfo) -> Result<T, UpdateError>) -> Result<T, UpdateError> {
        let pending = self.pending.lock().expect("pending lock");
        let info = pending
            .as_ref()
            .ok_or_else(|| UpdateError::Apply("다운로드된 업데이트가 없습니다".into()))?;
        f(info)
    }
}

impl Updater for VelopackUpdater {
    fn download(&self, version: &Version) -> Result<(), UpdateError> {
        let check = self.manager.check_for_updates().map_err(|e| UpdateError::Download(e.to_string()))?;
        let UpdateCheck::UpdateAvailable(info) = check else {
            return Err(UpdateError::Download(format!("{version} 패키지를 찾을 수 없습니다")));
        };
        self.manager
            .download_updates(&info, None)
            .map_err(|e| UpdateError::Download(e.to_string()))?;
        *self.pending.lock().expect("pending lock") = Some(info);
        Ok(())
    }

    fn apply_and_restart(&self) -> Result<(), UpdateError> {
        self.with_pending(|info| {
            self.manager
                .apply_updates_and_restart(info)
                .map_err(|e| UpdateError::Apply(e.to_string()))
        })
    }

    fn apply_on_exit(&self) -> Result<(), UpdateError> {
        self.with_pending(|info| {
            self.manager
                .wait_exit_then_apply_updates(info, true, false, Vec::<String>::new())
                .map_err(|e| UpdateError::Apply(e.to_string()))
        })
    }
}

/// 설치판이 아닐 때 쓰는 업데이터. 상태 머신은 `updater_available = false`로 이 경로를 막지만,
/// 실수로 호출되더라도 안전하게 실패한다.
pub struct UnavailableUpdater;

impl Updater for UnavailableUpdater {
    fn download(&self, _version: &Version) -> Result<(), UpdateError> {
        Err(UpdateError::NotInstalled)
    }
    fn apply_and_restart(&self) -> Result<(), UpdateError> {
        Err(UpdateError::NotInstalled)
    }
    fn apply_on_exit(&self) -> Result<(), UpdateError> {
        Err(UpdateError::NotInstalled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_updater_reports_not_installed() {
        let u = UnavailableUpdater;
        assert_eq!(u.download(&Version::new(1, 0, 0)), Err(UpdateError::NotInstalled));
        assert_eq!(u.apply_and_restart(), Err(UpdateError::NotInstalled));
        assert_eq!(u.apply_on_exit(), Err(UpdateError::NotInstalled));
    }

    #[test]
    fn velopack_is_unavailable_outside_installed_app() {
        assert!(VelopackUpdater::new("https://github.com/example/aragorn").is_none());
    }
}
```
Velopack API 이름이 설치된 버전과 다르면 Global Constraints에 따라 이 파일 안에서만 맞춘다. (`GithubSource::new`, `UpdateManager::new`, `check_for_updates`, `download_updates`, `apply_updates_and_restart`, `wait_exit_then_apply_updates`를 docs.rs/velopack에서 확인)

Step 4에서 주석 처리한 `pub use` 줄을 복구한다.

- [ ] **Step 9: 테스트 통과 확인**

Run: `cargo test -p aragorn-desktop`
Expected: 12 passed (build_info 2 + github_policy 4 + toml_config 4 + velopack_updater 2)

- [ ] **Step 10: 포맷, 린트, 커밋**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add Cargo.toml Cargo.lock crates/aragorn-desktop
git commit -m "feat(desktop): add policy source, config store and updater adapters

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---

### Task 7: 데스크톱 앱 (업데이트 워커, UI, 조립)

**Files:**
- Create: `crates/aragorn-desktop/src/update_worker.rs`
- Create: `crates/aragorn-desktop/src/ui/{mod.rs, fonts.rs, update_view.rs}`
- Create: `crates/aragorn-desktop/src/app.rs`, `crates/aragorn-desktop/src/main.rs`
- Create: `crates/aragorn-desktop/assets/fonts/NanumGothic-Regular.ttf`, `crates/aragorn-desktop/assets/fonts/OFL.txt`
- Modify: `crates/aragorn-desktop/Cargo.toml`, `crates/aragorn-desktop/src/lib.rs`

**Interfaces:**
- Consumes: Task 3, 4, 6의 모든 공개 항목
- Produces:
  - `aragorn_desktop::update_worker::UpdateWorker::{spawn(source: Arc<dyn UpdateSource>, updater: Arc<dyn Updater>, notify: Box<dyn Fn() + Send>) -> Self, send(&self, UpdateCommand), try_recv(&self) -> Option<UpdateEvent>, recv_timeout(&self, Duration) -> Option<UpdateEvent>}`
  - `aragorn_desktop::ui::update_view::{UpdateUiAction { Event(UpdateEvent), Quit }, show(ctx, flow, release_page) -> Vec<UpdateUiAction>, status_text(state, updates_enabled) -> String}`
  - `aragorn_desktop::app::{AppDeps, AragornApp}` (`AragornApp::new(ctx: &egui::Context, deps: AppDeps) -> Self`)
  - 실행 파일 이름 `aragorn` (Windows `aragorn.exe`)

- [ ] **Step 1: 의존성, 바이너리, 폰트 추가**

```bash
cargo add -p aragorn-desktop eframe
cargo add -p aragorn-desktop env_logger@0.11
mkdir -p crates/aragorn-desktop/assets/fonts
curl -fL -o crates/aragorn-desktop/assets/fonts/NanumGothic-Regular.ttf \
  https://github.com/google/fonts/raw/main/ofl/nanumgothic/NanumGothic-Regular.ttf
curl -fL -o crates/aragorn-desktop/assets/fonts/OFL.txt \
  https://github.com/google/fonts/raw/main/ofl/nanumgothic/OFL.txt
file crates/aragorn-desktop/assets/fonts/NanumGothic-Regular.ttf   # Expected: TrueType Font data
```

`crates/aragorn-desktop/Cargo.toml`의 `[lib]` 아래에 추가:
```toml
[[bin]]
name = "aragorn"
path = "src/main.rs"
```

`crates/aragorn-desktop/src/lib.rs`:
```rust
//! 어댑터 계층: OS, 네트워크, UI 구현과 의존성 조립.

pub mod adapters;
pub mod app;
pub mod build_info;
pub mod ui;
pub mod update_worker;
```

- [ ] **Step 2: 워커와 상태 문구 실패 테스트 작성**

`crates/aragorn-desktop/src/update_worker.rs`:
```rust
use aragorn_app::update::{UpdateCommand, UpdateEvent, UpdateSource, Updater, execute};
use std::{
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::Duration,
};

#[cfg(test)]
mod tests {
    use super::*;
    use aragorn_app::update::{UpdateError, UpdatePolicy};
    use semver::Version;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct SlowSource;

    impl UpdateSource for SlowSource {
        fn fetch_policy(&self) -> Result<UpdatePolicy, UpdateError> {
            thread::sleep(Duration::from_millis(100));
            Err(UpdateError::Network("offline".into()))
        }
    }

    struct NoUpdater;

    impl Updater for NoUpdater {
        fn download(&self, _: &Version) -> Result<(), UpdateError> {
            Err(UpdateError::NotInstalled)
        }
        fn apply_and_restart(&self) -> Result<(), UpdateError> {
            Err(UpdateError::NotInstalled)
        }
        fn apply_on_exit(&self) -> Result<(), UpdateError> {
            Err(UpdateError::NotInstalled)
        }
    }

    #[test]
    fn worker_returns_events_asynchronously() {
        let notified = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&notified);
        let worker = UpdateWorker::spawn(
            Arc::new(SlowSource),
            Arc::new(NoUpdater),
            Box::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            }),
        );

        worker.send(UpdateCommand::FetchPolicy);
        assert_eq!(worker.try_recv(), None, "send는 결과를 기다리지 않아야 한다");

        let event = worker.recv_timeout(Duration::from_secs(2));
        assert_eq!(
            event,
            Some(UpdateEvent::PolicyFetched(Err(UpdateError::Network("offline".into()))))
        );
        assert_eq!(notified.load(Ordering::SeqCst), 1);
    }
}
```

`crates/aragorn-desktop/src/ui/mod.rs`:
```rust
pub mod fonts;
pub mod update_view;
```

`crates/aragorn-desktop/src/ui/update_view.rs`:
```rust
use aragorn_app::update::{UpdateEvent, UpdateFlow, UpdateState};
use eframe::egui;

#[cfg(test)]
mod tests {
    use super::*;
    use aragorn_app::update::UpdateError;
    use semver::Version;

    #[test]
    fn status_text_for_disabled_updates() {
        assert_eq!(status_text(&UpdateState::Idle, false), "개발 빌드: 업데이트 확인 안 함");
    }

    #[test]
    fn status_text_for_common_states() {
        assert_eq!(status_text(&UpdateState::Checking, true), "업데이트 확인 중…");
        assert_eq!(status_text(&UpdateState::UpToDate, true), "최신 버전입니다");
        assert_eq!(
            status_text(&UpdateState::CheckFailed { error: UpdateError::Network("x".into()) }, true),
            "업데이트 확인 실패: 네트워크 오류: x"
        );
        assert_eq!(
            status_text(&UpdateState::Downloading { version: Version::new(1, 1, 0), forced: false }, true),
            "다운로드 중: 1.1.0"
        );
    }
}
```

`crates/aragorn-desktop/src/ui/fonts.rs`:
```rust
use eframe::egui;

const NANUM_GOTHIC: &[u8] = include_bytes!("../../assets/fonts/NanumGothic-Regular.ttf");

/// egui 기본 폰트에는 한글이 없으므로 나눔고딕을 대체 폰트로 등록한다.
pub fn install(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert("nanum-gothic".into(), egui::FontData::from_static(NANUM_GOTHIC).into());
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts.families.entry(family).or_default().push("nanum-gothic".into());
    }
    ctx.set_fonts(fonts);
}
```

`app.rs`와 `main.rs`는 Step 6에서 작성하므로, 이 단계에서는 `lib.rs`의 `pub mod app;`을 주석 처리하고 `main.rs`는 `fn main() {}`로 둔다.

- [ ] **Step 3: 테스트가 실패하는지 확인**

Run: `cargo test -p aragorn-desktop`
Expected: 컴파일 실패 — `cannot find type UpdateWorker`, `cannot find function status_text`

- [ ] **Step 4: 워커 구현**

`update_worker.rs`의 `use` 아래, `#[cfg(test)]` 위에 추가:
```rust
/// 업데이트 입출력 명령을 백그라운드 스레드에서 실행해 UI를 막지 않는다.
pub struct UpdateWorker {
    commands: Sender<UpdateCommand>,
    events: Receiver<UpdateEvent>,
}

impl UpdateWorker {
    pub fn spawn(
        source: Arc<dyn UpdateSource>,
        updater: Arc<dyn Updater>,
        notify: Box<dyn Fn() + Send>,
    ) -> Self {
        let (command_tx, command_rx) = mpsc::channel::<UpdateCommand>();
        let (event_tx, event_rx) = mpsc::channel();
        thread::Builder::new()
            .name("update-worker".into())
            .spawn(move || {
                for command in command_rx {
                    if let Some(event) = execute(&command, source.as_ref(), updater.as_ref()) {
                        if event_tx.send(event).is_err() {
                            break;
                        }
                        notify();
                    }
                }
            })
            .expect("업데이트 워커 스레드 생성");
        Self { commands: command_tx, events: event_rx }
    }

    pub fn send(&self, command: UpdateCommand) {
        let _ = self.commands.send(command);
    }

    pub fn try_recv(&self) -> Option<UpdateEvent> {
        self.events.try_recv().ok()
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Option<UpdateEvent> {
        self.events.recv_timeout(timeout).ok()
    }
}
```

- [ ] **Step 5: 업데이트 UI 구현**

`update_view.rs`의 `use` 아래, `#[cfg(test)]` 위에 추가:
```rust
pub enum UpdateUiAction {
    Event(UpdateEvent),
    Quit,
}

pub fn status_text(state: &UpdateState, updates_enabled: bool) -> String {
    if !updates_enabled {
        return "개발 빌드: 업데이트 확인 안 함".into();
    }
    match state {
        UpdateState::Idle => String::new(),
        UpdateState::Checking => "업데이트 확인 중…".into(),
        UpdateState::UpToDate => "최신 버전입니다".into(),
        UpdateState::CheckFailed { error } => format!("업데이트 확인 실패: {error}"),
        UpdateState::SoftAvailable { version, .. } => format!("새 버전 {version} 사용 가능"),
        UpdateState::Forced { version, .. } => format!("필수 업데이트: {version}"),
        UpdateState::Downloading { version, .. } => format!("다운로드 중: {version}"),
        UpdateState::ReadyToRestart { version, .. } => format!("업데이트 준비 완료: {version}"),
        UpdateState::Failed { error, .. } => format!("업데이트 실패: {error}"),
    }
}

/// 강제 업데이트는 모달, 소프트 업데이트는 상단 배너로 보여준다.
pub fn show(ctx: &egui::Context, flow: &UpdateFlow, release_page: Option<&str>) -> Vec<UpdateUiAction> {
    let mut actions = Vec::new();
    match flow.state() {
        UpdateState::Forced { version, message } => forced_modal(ctx, |ui| {
            ui.heading("필수 업데이트");
            ui.label(format!(
                "현재 버전 {}은(는) 더 이상 지원되지 않습니다. {version}(으)로 업데이트해야 계속할 수 있습니다.",
                flow.current()
            ));
            if !message.is_empty() {
                ui.label(message);
            }
            ui.add_space(8.0);
            if flow.updater_available() {
                if ui.button("업데이트").clicked() {
                    actions.push(UpdateUiAction::Event(UpdateEvent::UpdateNow));
                }
            } else {
                ui.label("설치판이 아니어서 자동으로 업데이트할 수 없습니다. 새 버전을 직접 내려받아 주세요.");
                if let Some(url) = release_page {
                    ui.hyperlink_to("다운로드 페이지 열기", url);
                }
            }
            if ui.button("종료").clicked() {
                actions.push(UpdateUiAction::Quit);
            }
        }),
        UpdateState::Downloading { version, forced: true } => forced_modal(ctx, |ui| {
            ui.heading("필수 업데이트");
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(format!("{version} 다운로드 중…"));
            });
        }),
        UpdateState::ReadyToRestart { forced: true, .. } => forced_modal(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("업데이트를 적용하고 다시 시작합니다…");
            });
        }),
        UpdateState::Failed { error, forced: true, .. } => forced_modal(ctx, |ui| {
            ui.heading("업데이트 실패");
            ui.label(error.to_string());
            ui.horizontal(|ui| {
                if ui.button("다시 시도").clicked() {
                    actions.push(UpdateUiAction::Event(UpdateEvent::UpdateNow));
                }
                if ui.button("종료").clicked() {
                    actions.push(UpdateUiAction::Quit);
                }
            });
        }),
        UpdateState::SoftAvailable { version, message } => banner(ctx, |ui| {
            ui.label(format!("새 버전 {version}이(가) 있습니다."));
            if !message.is_empty() {
                ui.label(message);
            }
            if flow.updater_available() {
                if ui.button("지금 업데이트").clicked() {
                    actions.push(UpdateUiAction::Event(UpdateEvent::UpdateNow));
                }
            } else if let Some(url) = release_page {
                ui.hyperlink_to("다운로드 페이지", url);
            }
            if ui.button("나중에").clicked() {
                actions.push(UpdateUiAction::Event(UpdateEvent::Later));
            }
            if ui.button("이 버전 건너뛰기").clicked() {
                actions.push(UpdateUiAction::Event(UpdateEvent::SkipVersion));
            }
        }),
        UpdateState::Downloading { version, forced: false } => banner(ctx, |ui| {
            ui.spinner();
            ui.label(format!("업데이트 {version} 다운로드 중…"));
        }),
        UpdateState::ReadyToRestart { version, forced: false } => banner(ctx, |ui| {
            ui.label(format!("업데이트 {version} 준비 완료. 앱을 종료하면 적용됩니다."));
            if ui.button("재시작하여 업데이트").clicked() {
                actions.push(UpdateUiAction::Event(UpdateEvent::RestartNow));
            }
        }),
        UpdateState::Failed { error, forced: false, .. } => banner(ctx, |ui| {
            ui.label(format!("업데이트 실패: {error}"));
            if ui.button("다시 시도").clicked() {
                actions.push(UpdateUiAction::Event(UpdateEvent::UpdateNow));
            }
            if ui.button("나중에").clicked() {
                actions.push(UpdateUiAction::Event(UpdateEvent::Later));
            }
        }),
        UpdateState::Idle | UpdateState::Checking | UpdateState::UpToDate | UpdateState::CheckFailed { .. } => {}
    }
    actions
}

fn forced_modal(ctx: &egui::Context, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Modal::new(egui::Id::new("forced-update")).show(ctx, |ui| {
        ui.set_max_width(360.0);
        add_contents(ui);
    });
}

fn banner(ctx: &egui::Context, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::TopBottomPanel::top("update-banner").show(ctx, |ui| {
        ui.horizontal_wrapped(add_contents);
    });
}
```

- [ ] **Step 6: 앱과 진입점 구현**

`crates/aragorn-desktop/src/app.rs`:
```rust
use crate::{
    adapters::UnavailableUpdater,
    build_info,
    ui::update_view::{self, UpdateUiAction},
    update_worker::UpdateWorker,
};
use aragorn_app::{
    config::{Config, ConfigStore, UpdateConfig},
    update::{UpdateCommand, UpdateEvent, UpdateFlow, UpdateSource, Updater},
};
use eframe::egui;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

const RECHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

pub struct AppDeps {
    pub config_store: Box<dyn ConfigStore>,
    /// `None`이면 업데이트 확인을 하지 않는다 (개발 빌드)
    pub source: Option<Arc<dyn UpdateSource>>,
    /// `None`이면 설치판이 아니다
    pub updater: Option<Arc<dyn Updater>>,
    pub release_page: Option<String>,
}

pub struct AragornApp {
    flow: UpdateFlow,
    worker: Option<UpdateWorker>,
    updater: Arc<dyn Updater>,
    config_store: Box<dyn ConfigStore>,
    config: Config,
    release_page: Option<String>,
    last_check: Instant,
    exit_handled: bool,
}

impl AragornApp {
    pub fn new(ctx: &egui::Context, deps: AppDeps) -> Self {
        let config = deps.config_store.load();
        let flow = UpdateFlow::new(
            build_info::current_version(),
            config.update.to_prefs(),
            deps.updater.is_some(),
        );
        let updater: Arc<dyn Updater> = deps.updater.unwrap_or_else(|| Arc::new(UnavailableUpdater));
        let worker = deps.source.map(|source| {
            let ctx = ctx.clone();
            UpdateWorker::spawn(source, Arc::clone(&updater), Box::new(move || ctx.request_repaint()))
        });
        let mut app = Self {
            flow,
            worker,
            updater,
            config_store: deps.config_store,
            config,
            release_page: deps.release_page,
            last_check: Instant::now(),
            exit_handled: false,
        };
        if app.worker.is_some() {
            app.dispatch(UpdateEvent::CheckRequested);
        }
        app
    }

    fn dispatch(&mut self, event: UpdateEvent) {
        for command in self.flow.handle(event) {
            match command {
                UpdateCommand::SavePrefs(prefs) => {
                    self.config.update = UpdateConfig::from_prefs(&prefs);
                    self.save_config();
                }
                UpdateCommand::ApplyOnExit => {
                    self.flush_persistent_state();
                    if let Err(e) = self.updater.apply_on_exit() {
                        log::error!("종료 후 업데이트 예약 실패: {e}");
                    }
                }
                UpdateCommand::ApplyAndRestart => {
                    self.flush_persistent_state();
                    self.send(command);
                }
                UpdateCommand::FetchPolicy | UpdateCommand::Download(_) => self.send(command),
            }
        }
    }

    fn send(&self, command: UpdateCommand) {
        if let Some(worker) = &self.worker {
            worker.send(command);
        }
    }

    fn save_config(&self) {
        if let Err(e) = self.config_store.save(&self.config) {
            log::error!("설정 저장 실패: {e}");
        }
    }

    /// 업데이트를 적용하기 전에 디스크에 남겨야 하는 상태를 저장한다.
    /// 배터리 세이브는 마일스톤 4에서 여기에 추가한다.
    fn flush_persistent_state(&self) {
        self.save_config();
    }
}

impl eframe::App for AragornApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut events = Vec::new();
        if let Some(worker) = &self.worker {
            while let Some(event) = worker.try_recv() {
                events.push(event);
            }
            if self.last_check.elapsed() >= RECHECK_INTERVAL {
                self.last_check = Instant::now();
                events.actions.push(UpdateUiAction::Event(UpdateEvent::CheckRequested));
            }
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.exit_handled {
            self.exit_handled = true;
            events.actions.push(UpdateUiAction::Event(UpdateEvent::AppExiting));
        }

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let mut auto_download = self.flow.prefs().auto_download;
                if ui.checkbox(&mut auto_download, "자동으로 업데이트 다운로드").changed() {
                    events.push(UpdateEvent::SetAutoDownload(auto_download));
                }
                if self.worker.is_some() && ui.button("업데이트 확인").clicked() {
                    events.actions.push(UpdateUiAction::Event(UpdateEvent::CheckRequested));
                }
            });
        });

        for action in update_view::show(ctx, &self.flow, self.release_page.as_deref()) {
            match action {
                UpdateUiAction::Event(event) => events.push(event),
                UpdateUiAction::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            }
        }

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.label(update_view::status_text(self.flow.state(), self.worker.is_some()));
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(120.0);
                ui.heading(format!("Aragorn v{}", self.flow.current()));
                ui.label("ROM 실행은 다음 마일스톤에서 지원됩니다.");
            });
        });

        for event in events {
            self.dispatch(event);
        }
        // 6시간 재확인 타이머가 유휴 상태에서도 돌도록 주기적으로 깨운다.
        ctx.request_repaint_after(Duration::from_secs(60));
    }
}
```

`crates/aragorn-desktop/src/main.rs`:
```rust
use aragorn_app::update::{UpdateSource, Updater};
use aragorn_desktop::{
    adapters::{GithubPolicySource, TomlConfigStore, VelopackUpdater},
    app::{AppDeps, AragornApp},
    build_info, ui,
};
use eframe::egui;
use std::{path::PathBuf, sync::Arc, time::Duration};

const POLICY_TIMEOUT: Duration = Duration::from_secs(5);

fn main() -> eframe::Result {
    // Velopack 설치/제거 훅 처리. 반드시 가장 먼저 호출한다.
    velopack::VelopackApp::build().run();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let config_path = TomlConfigStore::default_path().unwrap_or_else(|| PathBuf::from("config.toml"));
    let (source, updater, release_page) = match build_info::GITHUB_REPO {
        Some(repo) => {
            let source: Arc<dyn UpdateSource> = Arc::new(GithubPolicySource::new(
                build_info::policy_base_url(repo),
                *build_info::POLICY_PUBLIC_KEY,
                POLICY_TIMEOUT,
            ));
            let updater = VelopackUpdater::new(&build_info::repo_url(repo))
                .map(|u| Arc::new(u) as Arc<dyn Updater>);
            (Some(source), updater, Some(build_info::releases_page_url(repo)))
        }
        None => (None, None, None),
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(format!("Aragorn v{}", build_info::current_version()))
            .with_inner_size([640.0, 576.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Aragorn",
        options,
        Box::new(move |cc| {
            ui::fonts::install(&cc.egui_ctx);
            let deps = AppDeps {
                config_store: Box::new(TomlConfigStore::new(config_path)),
                source,
                updater,
                release_page,
            };
            Ok(Box::new(AragornApp::new(&cc.egui_ctx, deps)))
        }),
    )
}
```
`lib.rs`에서 주석 처리한 `pub mod app;`을 복구한다.

- [ ] **Step 7: 테스트 통과 확인**

Run: `cargo test --workspace`
Expected: 모두 통과 (aragorn-app 45, xtask 7, aragorn-desktop 15)

- [ ] **Step 8: 수동 실행 확인 (WSLg)**

```bash
cargo run -p aragorn-desktop
```
Expected:
- "Aragorn v0.1.0" 창이 뜨고, 한글이 네모가 아닌 글자로 보인다.
- 하단 상태 표시줄에 "개발 빌드: 업데이트 확인 안 함"이 보인다.
- "자동으로 업데이트 다운로드" 체크를 끄고 창을 닫았다가 다시 실행하면 꺼진 상태가 유지된다 (`~/.config/aragorn/config.toml` 확인).

```bash
ARAGORN_REPO=nonexistent-owner/nonexistent-repo cargo run -p aragorn-desktop
```
Expected: 창이 즉시 뜨고, 5초 안에 상태 표시줄이 "업데이트 확인 실패: 네트워크 오류: …"로 바뀐다. 그동안 창은 계속 반응한다.

- [ ] **Step 9: 포맷, 린트, 커밋**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add crates/aragorn-desktop Cargo.lock
git commit -m "feat(desktop): add eframe shell with update banner, forced modal and worker

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---

### Task 8: CI와 릴리스 워크플로

**Files:**
- Create: `.github/workflows/ci.yml`
- Create: `.github/workflows/release.yml`

**Interfaces:**
- Consumes: `cargo xtask check-tag`, `cargo xtask make-policy` (Task 5), 실행 파일 `aragorn`/`aragorn.exe` (Task 7), 빌드 환경 변수 `ARAGORN_REPO` (Task 6)
- Produces: `vX.Y.Z` 태그 푸시 → 3개 OS의 Velopack 패키지와 `update-policy.json`(+`.sig`)이 담긴, 공개된 GitHub Release. 필요한 저장소 시크릿: `ARAGORN_POLICY_SIGNING_KEY`

- [ ] **Step 1: CI 워크플로 작성**

`.github/workflows/ci.yml`:
```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:

jobs:
  test:
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-22.04, windows-latest, macos-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt
      - uses: Swatinem/rust-cache@v2
      - name: Linux 빌드 의존성
        if: runner.os == 'Linux'
        run: sudo apt-get update && sudo apt-get install -y libxkbcommon-dev libwayland-dev libgl1-mesa-dev
      - run: cargo fmt --all --check
      - run: cargo clippy --workspace --all-targets -- -D warnings
      - run: cargo test --workspace
```

- [ ] **Step 2: 릴리스 워크플로 작성**

`.github/workflows/release.yml`:
```yaml
name: Release

on:
  push:
    tags: ["v*.*.*"]

permissions:
  contents: write

jobs:
  prepare:
    runs-on: ubuntu-22.04
    outputs:
      version: ${{ steps.version.outputs.version }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: 태그와 워크스페이스 버전 일치 확인
        run: cargo xtask check-tag "${GITHUB_REF_NAME}"
      - id: version
        run: echo "version=${GITHUB_REF_NAME#v}" >> "$GITHUB_OUTPUT"
      - name: 초안 릴리스 생성
        run: gh release create "${GITHUB_REF_NAME}" --draft --title "Aragorn ${{ steps.version.outputs.version }}" --generate-notes
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}

  package:
    needs: prepare
    strategy:
      fail-fast: true
      matrix:
        include:
          - os: windows-latest
            exe: aragorn.exe
          - os: macos-latest
            exe: aragorn
          - os: ubuntu-22.04
            exe: aragorn
    runs-on: ${{ matrix.os }}
    env:
      VERSION: ${{ needs.prepare.outputs.version }}
      ARAGORN_REPO: ${{ github.repository }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Linux 빌드 의존성
        if: runner.os == 'Linux'
        run: sudo apt-get update && sudo apt-get install -y libxkbcommon-dev libwayland-dev libgl1-mesa-dev
      - run: cargo test --workspace
      - run: cargo build --release -p aragorn-desktop
      - uses: actions/setup-dotnet@v4
        with:
          dotnet-version: "9.0.x"
      - name: vpk 설치 (velopack 크레이트와 같은 버전)
        shell: bash
        run: |
          VPK_VERSION="$(cargo pkgid velopack | sed 's/.*[@#]//')"
          dotnet tool install -g vpk --version "$VPK_VERSION"
      - name: 패키징과 업로드
        shell: bash
        run: |
          mkdir publish
          cp "target/release/${{ matrix.exe }}" publish/
          vpk pack --packId Aragorn --packVersion "$VERSION" --packDir publish --mainExe "${{ matrix.exe }}" --packTitle Aragorn
          vpk upload github --repoUrl "https://github.com/${{ github.repository }}" \
            --token "${{ secrets.GITHUB_TOKEN }}" --tag "v$VERSION" --releaseName "Aragorn $VERSION" --merge

  publish:
    needs: [prepare, package]
    runs-on: ubuntu-22.04
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: 업데이트 정책 생성과 서명
        run: cargo xtask make-policy dist
        env:
          ARAGORN_POLICY_SIGNING_KEY: ${{ secrets.ARAGORN_POLICY_SIGNING_KEY }}
      - name: 정책 업로드 후 릴리스 공개
        run: |
          gh release upload "${GITHUB_REF_NAME}" dist/update-policy.json dist/update-policy.json.sig
          gh release edit "${GITHUB_REF_NAME}" --draft=false --latest
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```
정책 파일과 패키지가 모두 올라간 뒤에만 릴리스가 공개되므로, 앱이 패키지 없는 정책을 받는 일은 없다.

- [ ] **Step 3: 로컬에서 확인할 수 있는 부분 검증**

```bash
python3 -c "import yaml,sys; [yaml.safe_load(open(f)) for f in sys.argv[1:]]; print('ok')" .github/workflows/ci.yml .github/workflows/release.yml
cargo xtask check-tag v0.1.0 && echo tag-ok
cargo pkgid velopack | sed 's/.*[@#]//'     # Expected: 버전 문자열 하나 (예: 0.0.1298)
```
Expected: `ok`, `tag-ok`, 버전 문자열. (`python3`에 yaml 모듈이 없으면 `pip install pyyaml` 대신 이 확인은 건너뛰고, Task 9의 첫 실행에서 검증한다고 보고한다.)

- [ ] **Step 4: 커밋**

```bash
git add .github
git commit -m "ci: add test workflow and tag-triggered release pipeline

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---

### Task 9: 첫 릴리스와 업데이트 종단 간 검증 (사용자 참여 필요)

이 작업은 공개 GitHub 저장소 생성, 시크릿 등록, 릴리스 공개처럼 **외부에 영향을 주는 동작**이다. 각 단계 전에 사용자에게 확인을 받는다.

**Files:**
- Modify: `Cargo.toml` (`workspace.package.version`), `release/policy.toml`

**Interfaces:**
- Consumes: Task 1–8 전체
- Produces: GitHub Releases의 v0.1.0, v0.1.1, v0.1.2와 검증 결과 보고

- [ ] **Step 1: 저장소 생성과 푸시 (사용자 확인 후)**

```bash
gh repo create aragorn --public --source . --push
git push -u origin feat/m0-walking-skeleton
```
그다음 PR을 만들어 `main`에 병합한다. CI가 3개 OS에서 통과해야 한다.

- [ ] **Step 2: 서명 키 시크릿 등록 (사용자 확인 후)**

```bash
gh secret set ARAGORN_POLICY_SIGNING_KEY < ~/.config/aragorn-release/policy-signing-key.b64
```
비밀키 파일은 안전한 곳(비밀번호 관리자 등)에 백업하라고 사용자에게 안내한다. 이 키를 잃어버리면 이미 배포된 앱에 정책을 보낼 수 없다.

- [ ] **Step 3: v0.1.0 릴리스**

```bash
git checkout main && git pull
git tag v0.1.0 && git push origin v0.1.0
gh run watch
```
Expected: Release 워크플로 성공, 릴리스에 Windows Setup, macOS 패키지, Linux AppImage, `update-policy.json`, `update-policy.json.sig`가 있다.

- [ ] **Step 4: 설치 후 "최신 버전" 확인 (사용자 수행)**

Windows에서 `Aragorn-win-Setup.exe`로 설치하고 실행한다.
Expected: 상태 표시줄 "최신 버전입니다".

- [ ] **Step 5: 소프트 업데이트 확인**

`Cargo.toml`의 버전을 `0.1.1`로 올리고 커밋, 병합한 뒤 `v0.1.1` 태그를 푸시한다.
v0.1.0 앱의 "자동으로 업데이트 다운로드"를 끄고 재실행 → 배너 "새 버전 0.1.1이(가) 있습니다" 확인 → "이 버전 건너뛰기" → 재실행 시 배너가 없어야 한다.
config.toml에서 `skipped_version`을 지우고 자동 다운로드를 켠 뒤 재실행 → "업데이트 0.1.1 준비 완료" 배너 → 앱 종료 → 다시 실행하면 창 제목이 v0.1.1이어야 한다.

- [ ] **Step 6: 강제 업데이트 확인**

`release/policy.toml`의 `minimum_supported`와 `Cargo.toml` 버전을 모두 `0.1.2`로 올려 `v0.1.2`를 릴리스한다.
v0.1.1 앱 실행 → "필수 업데이트" 모달이 뜨고 닫을 수 없어야 한다 → "업데이트" → 자동 재시작 후 v0.1.2.

- [ ] **Step 7: 오프라인 확인**

네트워크를 끊고 앱을 실행한다.
Expected: 창이 바로 뜨고, 상태 표시줄에 "업데이트 확인 실패"가 보이며, 앱은 정상적으로 사용할 수 있다.

- [ ] **Step 8: 결과 보고**

각 단계의 결과(성공/실패, 실패 시 로그와 화면)를 사용자에게 보고한다. 실패한 단계가 있으면 systematic-debugging 스킬로 원인을 찾은 뒤 패치 릴리스로 수정한다.
