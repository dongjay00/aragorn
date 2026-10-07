mod policy;

use aragorn_app::update::{public_key_for, sign_policy, verify_and_parse};
use policy::{PolicyToml, build_policy_json, check_tag, decode_secret, encode_secret};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const USAGE: &str =
    "사용법: cargo xtask <gen-key <secret-out> | make-policy <out-dir> | check-tag <tag>>";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let arg = args.get(1).map(String::as_str);
    let result = match args.first().map(String::as_str) {
        Some("gen-key") => gen_key(arg),
        Some("make-policy") => make_policy(arg),
        Some("check-tag") => arg
            .ok_or_else(|| USAGE.to_string())
            .and_then(|tag| check_tag(tag, VERSION)),
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
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask는 워크스페이스 안에 있다")
        .to_path_buf()
}

fn public_key_path() -> PathBuf {
    root().join("release/policy.pub")
}

fn gen_key(secret_out: Option<&str>) -> Result<(), String> {
    let out = PathBuf::from(secret_out.ok_or_else(|| USAGE.to_string())?);
    if out.exists() {
        return Err(format!(
            "{}가 이미 있습니다. 덮어쓰지 않습니다",
            out.display()
        ));
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
    println!(
        "{} 에 정책 파일을 만들었습니다 (latest = {VERSION})",
        out.display()
    );
    Ok(())
}
