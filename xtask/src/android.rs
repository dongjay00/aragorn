//! `cargo xtask android`: Android 앱이 쓰는 Rust 라이브러리(.so)와 Kotlin 바인딩을 만든다.
//!
//! Gradle 빌드(`mobile/android`)가 이 명령을 먼저 부른다. 결과는 앱 모듈의 `build/` 아래에 둔다.

use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

/// 기본으로 빌드하는 ABI: 실기기(arm64)와 x86_64 에뮬레이터.
const DEFAULT_ABIS: [&str; 2] = ["arm64-v8a", "x86_64"];
/// 앱의 minSdk. cpal의 AAudio 출력이 API 26부터 있다.
const MIN_SDK: &str = "26";

pub fn build(root: &Path, args: &[String]) -> Result<(), String> {
    let release = args.iter().any(|a| a == "--release");
    let abis: Vec<&str> = args
        .iter()
        .filter_map(|a| a.strip_prefix("--abi="))
        .collect();
    let abis = if abis.is_empty() {
        DEFAULT_ABIS.to_vec()
    } else {
        abis
    };
    let app = root.join("mobile/android/app/build");
    let jni_libs = app.join("rustJniLibs");
    let kotlin = app.join("generated/uniffi/kotlin");

    let mut ndk = cargo(root);
    ndk.args(["ndk", "--platform", MIN_SDK, "-o"])
        .arg(&jni_libs);
    for abi in &abis {
        ndk.args(["-t", abi]);
    }
    ndk.args(["build", "-p", "aragorn-mobile", "--lib"]);
    if release {
        ndk.arg("--release");
    }
    run(
        ndk,
        "cargo ndk가 없으면 `cargo install cargo-ndk`로 설치하세요",
    )?;

    // 바인딩은 어느 ABI의 .so로 만들어도 같다.
    let library = jni_libs.join(abis[0]).join("libaragorn_mobile.so");
    let mut bindgen = cargo(root);
    bindgen
        .args([
            "run",
            "--quiet",
            "-p",
            "aragorn-mobile",
            "--features",
            "bindgen",
            "--bin",
            "uniffi-bindgen",
            "--",
            "generate",
            "--library",
        ])
        .arg(&library)
        .args(["--language", "kotlin", "--no-format", "--out-dir"])
        .arg(&kotlin);
    run(bindgen, "Kotlin 바인딩을 만들지 못했습니다")?;

    println!("Rust 라이브러리: {}", jni_libs.display());
    println!("Kotlin 바인딩: {}", kotlin.display());
    Ok(())
}

fn cargo(root: &Path) -> Command {
    let cargo = env::var_os("CARGO").map_or_else(|| PathBuf::from("cargo"), PathBuf::from);
    let mut command = Command::new(cargo);
    command.current_dir(root);
    command
}

fn run(mut command: Command, hint: &str) -> Result<(), String> {
    let status = command
        .status()
        .map_err(|e| format!("{command:?}을(를) 실행할 수 없습니다: {e} ({hint})"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{command:?} 실패: {status} ({hint})"))
    }
}
