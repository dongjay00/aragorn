//! `cargo xtask ios`: iOS 앱이 쓰는 Rust 정적 라이브러리와 Swift 바인딩을 만든다.
//!
//! Xcode 빌드(`mobile/ios`)의 첫 단계(`build-rust.sh`)가 이 명령을 부른다. 처음 한 번은 Xcode를 열기 전에
//! 직접 실행해야 한다. Xcode는 빌드를 시작할 때 폴더 안 파일 목록을 정하기 때문이다.
//! - `mobile/ios/build/<플랫폼>/libaragorn_mobile.a` (플랫폼: iphoneos, iphonesimulator)
//! - `mobile/ios/build/headers/`: C 헤더와 모듈맵 (Swift의 `import aragorn_mobileFFI`)
//! - `mobile/ios/build/Version.xcconfig`: 앱 버전 (워크스페이스 버전)
//! - `mobile/ios/Aragorn/Generated/`: Swift 바인딩과 라이브러리 해시

use crate::android::{cargo, run};
use sha2::{Digest, Sha256};
use std::{env, fs, path::Path};

/// (Xcode `PLATFORM_NAME`, Rust 타깃). 시뮬레이터는 Apple Silicon만 지원한다.
const PLATFORMS: [(&str, &str); 2] = [
    ("iphoneos", "aarch64-apple-ios"),
    ("iphonesimulator", "aarch64-apple-ios-sim"),
];
const LIBRARY: &str = "libaragorn_mobile.a";
const FFI_MODULE: &str = "aragorn_mobileFFI";
/// 앱의 최소 iOS 버전 (Xcode 프로젝트의 IPHONEOS_DEPLOYMENT_TARGET과 같다).
const DEPLOYMENT_TARGET: &str = "17.0";

pub fn build(root: &Path, args: &[String], version: &str) -> Result<(), String> {
    let release = args.iter().any(|a| a == "--release");
    let profile = if release { "release" } else { "debug" };
    let platforms: Vec<(&str, &str)> = match args.iter().find_map(|a| a.strip_prefix("--platform="))
    {
        Some(name) => vec![
            *PLATFORMS
                .iter()
                .find(|(platform, _)| *platform == name)
                .ok_or_else(|| {
                    format!("알 수 없는 플랫폼입니다: {name} (iphoneos, iphonesimulator)")
                })?,
        ],
        None => PLATFORMS.to_vec(),
    };
    let ios = root.join("mobile/ios");
    let build_dir = ios.join("build");
    let generated = ios.join("Aragorn/Generated");

    let mut libraries = Vec::new();
    for (platform, target) in &platforms {
        let mut cargo_build = cargo(root);
        cargo_build.args(["build", "-p", "aragorn-mobile", "--lib", "--target", target]);
        if release {
            cargo_build.arg("--release");
        }
        if env::var_os("IPHONEOS_DEPLOYMENT_TARGET").is_none() {
            cargo_build.env("IPHONEOS_DEPLOYMENT_TARGET", DEPLOYMENT_TARGET);
        }
        run(
            cargo_build,
            &format!("rustup target add {target}으로 타깃을 설치했는지 확인하세요"),
        )?;
        let built = root.join("target").join(target).join(profile).join(LIBRARY);
        let dest = build_dir.join(platform).join(LIBRARY);
        fs::create_dir_all(build_dir.join(platform)).map_err(|e| e.to_string())?;
        copy(&built, &dest)?;
        libraries.push(dest);
    }

    // 바인딩은 어느 타깃의 라이브러리로 만들어도 같다.
    let bindings = build_dir.join("bindings");
    if bindings.exists() {
        fs::remove_dir_all(&bindings).map_err(|e| e.to_string())?;
    }
    let mut bindgen = cargo(root);
    bindgen
        .args([
            "run",
            "--quiet",
            "-p",
            "uniffi-bindgen",
            "--",
            "generate",
            "--library",
        ])
        .arg(&libraries[0])
        .args(["--language", "swift", "--out-dir"])
        .arg(&bindings);
    run(bindgen, "Swift 바인딩을 만들지 못했습니다")?;

    let headers = build_dir.join("headers");
    fs::create_dir_all(&headers).map_err(|e| e.to_string())?;
    copy(
        &bindings.join(format!("{FFI_MODULE}.h")),
        &headers.join(format!("{FFI_MODULE}.h")),
    )?;
    // SWIFT_INCLUDE_PATHS에 둔 폴더의 `module.modulemap`을 Swift가 찾는다.
    copy(
        &bindings.join(format!("{FFI_MODULE}.modulemap")),
        &headers.join("module.modulemap"),
    )?;
    fs::create_dir_all(&generated).map_err(|e| e.to_string())?;
    copy(
        &bindings.join("aragorn_mobile.swift"),
        &generated.join("aragorn_mobile.swift"),
    )?;

    // Xcode는 링커 플래그로 넘긴 정적 라이브러리가 바뀐 것을 모른다. 라이브러리 해시를 Swift 파일로 남겨,
    // Rust가 바뀌면 이 파일이 다시 컴파일되고 앱이 다시 링크되게 한다.
    let mut hasher = Sha256::new();
    for library in &libraries {
        hasher.update(fs::read(library).map_err(|e| format!("{}: {e}", library.display()))?);
    }
    let hash: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    write_if_changed(
        &generated.join("RustLibrary.swift"),
        &format!(
            "// cargo xtask ios가 만든다. 직접 고치지 않는다.\n\
             // Rust 라이브러리가 바뀌면 이 값이 바뀌어 Xcode가 앱을 다시 링크한다.\n\
             let rustLibraryHash = \"{hash}\"\n"
        ),
    )?;
    write_if_changed(
        &build_dir.join("Version.xcconfig"),
        &format!(
            "// cargo xtask ios가 워크스페이스 버전으로 만든다.\n\
             MARKETING_VERSION = {version}\n\
             CURRENT_PROJECT_VERSION = {}\n",
            build_number(version)?
        ),
    )?;

    for library in &libraries {
        println!("Rust 라이브러리: {}", library.display());
    }
    println!("Swift 바인딩: {}", generated.display());
    Ok(())
}

/// 1.2.3 → 10203 (Android versionCode와 같은 규칙).
fn build_number(version: &str) -> Result<u32, String> {
    let core = version.split('-').next().unwrap_or(version);
    let parts: Vec<u32> = core
        .split('.')
        .map(|p| {
            p.parse()
                .map_err(|_| format!("버전을 읽을 수 없습니다: {version}"))
        })
        .collect::<Result<_, _>>()?;
    match parts[..] {
        [major, minor, patch] => Ok(major * 10000 + minor * 100 + patch),
        _ => Err(format!("버전을 읽을 수 없습니다: {version}")),
    }
}

/// 내용이 같으면 쓰지 않는다. Xcode가 바뀌지 않은 파일을 다시 컴파일하지 않게 한다.
fn copy(from: &Path, to: &Path) -> Result<(), String> {
    let data = fs::read(from).map_err(|e| format!("{}: {e}", from.display()))?;
    write_bytes_if_changed(to, &data)
}

fn write_if_changed(path: &Path, text: &str) -> Result<(), String> {
    write_bytes_if_changed(path, text.as_bytes())
}

fn write_bytes_if_changed(path: &Path, data: &[u8]) -> Result<(), String> {
    if fs::read(path).ok().as_deref() == Some(data) {
        return Ok(());
    }
    fs::write(path, data).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_number_matches_android_version_code() {
        assert_eq!(build_number("0.12.0"), Ok(1200));
        assert_eq!(build_number("1.2.3"), Ok(10203));
        assert_eq!(build_number("1.2.3-beta.1"), Ok(10203));
        assert!(build_number("1.2").is_err());
    }
}
