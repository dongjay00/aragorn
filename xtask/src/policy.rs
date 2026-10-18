use aragorn_app::update::{PackageHashes, parse_policy};
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

#[derive(Debug, Deserialize)]
pub struct PolicyToml {
    pub minimum_supported: String,
    #[serde(default)]
    pub message: String,
}

/// 배포용 정책 JSON을 만들고, 앱이 읽을 수 있는 형식인지 미리 검증한다.
pub fn build_policy_json(
    version: &str,
    policy: &PolicyToml,
    packages: &PackageHashes,
) -> Result<Vec<u8>, String> {
    if packages.is_empty() {
        return Err("패키지 해시가 하나도 없습니다. vpk 피드를 확인하세요".into());
    }
    let doc = serde_json::json!({
        "latest": version,
        "minimum_supported": policy.minimum_supported,
        "message": policy.message,
        "packages": packages,
    });
    let json = serde_json::to_vec_pretty(&doc).map_err(|e| e.to_string())?;
    parse_policy(&json).map_err(|e| e.to_string())?;
    Ok(json)
}

#[derive(Deserialize)]
struct Feed {
    #[serde(rename = "Assets")]
    assets: Vec<FeedAsset>,
}

#[derive(Deserialize)]
struct FeedAsset {
    #[serde(rename = "Version")]
    version: String,
    #[serde(rename = "Type")]
    kind: String,
    #[serde(rename = "SHA256")]
    sha256: String,
}

/// `vpk pack`이 만든 `releases.{channel}.json`에서 해당 버전의 전체 패키지 SHA256을 모은다.
pub fn collect_package_hashes(feeds_dir: &Path, version: &str) -> Result<PackageHashes, String> {
    let mut hashes = PackageHashes::new();
    let entries = fs::read_dir(feeds_dir).map_err(|e| format!("{}: {e}", feeds_dir.display()))?;
    for entry in entries {
        let path = entry.map_err(|e| e.to_string())?.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        let Some(channel) = name
            .strip_prefix("releases.")
            .and_then(|n| n.strip_suffix(".json"))
        else {
            continue;
        };
        let text = fs::read_to_string(&path).map_err(|e| format!("{name}: {e}"))?;
        let feed: Feed = serde_json::from_str(&text).map_err(|e| format!("{name}: {e}"))?;
        let asset = feed
            .assets
            .iter()
            .find(|a| a.version == version && a.kind.eq_ignore_ascii_case("full"))
            .ok_or_else(|| format!("{name}에 {version} 전체 패키지가 없습니다"))?;
        hashes.insert(channel.to_string(), asset.sha256.to_ascii_lowercase());
    }
    Ok(hashes)
}

/// 모바일 패키지: (정책 채널 이름, 릴리스 파일 이름). 채널 이름은 앱의 `PACKAGE_CHANNEL`과 같다.
/// 모바일 앱은 정책에 자기 채널이 있을 때만 새 버전을 알린다.
pub fn mobile_packages(version: &str) -> [(&'static str, String); 2] {
    [
        ("android", format!("Aragorn-{version}.apk")),
        ("ios", format!("Aragorn-{version}.ipa")),
    ]
}

/// `dir`의 모바일 패키지(APK, IPA) SHA256을 각 채널로 넣는다. 파일이 없으면 오류다.
pub fn add_mobile_hashes(
    hashes: &mut PackageHashes,
    dir: &Path,
    version: &str,
) -> Result<(), String> {
    for (channel, name) in mobile_packages(version) {
        let data =
            fs::read(dir.join(&name)).map_err(|e| format!("{name}을(를) 읽을 수 없습니다: {e}"))?;
        let hex: String = Sha256::digest(&data)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        hashes.insert(channel.to_string(), hex);
    }
    Ok(())
}

pub fn check_tag(tag: &str, version: &str) -> Result<(), String> {
    let expected = format!("v{version}");
    if tag == expected {
        Ok(())
    } else {
        Err(format!(
            "태그 {tag}가 워크스페이스 버전 {expected}와 다릅니다"
        ))
    }
}

pub fn decode_secret(b64: &str) -> Result<[u8; 32], String> {
    let bytes = B64
        .decode(b64.trim())
        .map_err(|e| format!("비밀키 base64 해석 실패: {e}"))?;
    bytes
        .try_into()
        .map_err(|_| "비밀키는 32바이트여야 합니다".to_string())
}

pub fn encode_secret(secret: &[u8; 32]) -> String {
    B64.encode(secret)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aragorn_app::update::{PackageHashes, public_key_for, sign_policy, verify_and_parse};
    use std::fs;

    const HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn feed(version: &str, kind: &str, sha256: &str) -> String {
        format!(
            r#"{{"Assets":[{{"PackageId":"Aragorn","Version":"{version}","Type":"{kind}","FileName":"Aragorn-{version}-full.nupkg","SHA1":"x","SHA256":"{sha256}","Size":1}}]}}"#
        )
    }

    fn hashes() -> PackageHashes {
        PackageHashes::from([("win".to_string(), HASH.to_string())])
    }

    fn toml_policy(min: &str) -> PolicyToml {
        PolicyToml {
            minimum_supported: min.into(),
            message: "hi".into(),
        }
    }

    #[test]
    fn builds_policy_with_workspace_version_as_latest() {
        let json = build_policy_json("0.2.0", &toml_policy("0.1.0"), &hashes()).unwrap();
        let p = parse_policy(&json).unwrap();
        assert_eq!(p.latest.to_string(), "0.2.0");
        assert_eq!(p.minimum_supported.to_string(), "0.1.0");
        assert_eq!(p.message, "hi");
        assert_eq!(p.packages, hashes());
    }

    #[test]
    fn rejects_policy_without_packages() {
        assert!(build_policy_json("0.1.0", &toml_policy("0.1.0"), &PackageHashes::new()).is_err());
    }

    #[test]
    fn collects_full_package_hash_per_channel() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("releases.win.json"),
            feed("0.2.0", "Full", HASH),
        )
        .unwrap();
        fs::write(
            dir.path().join("releases.linux.json"),
            feed("0.2.0", "Full", &HASH.to_uppercase()),
        )
        .unwrap();
        fs::write(dir.path().join("unrelated.json"), "{}").unwrap();
        let found = collect_package_hashes(dir.path(), "0.2.0").unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!(found["win"], HASH);
        assert_eq!(found["linux"], HASH);
    }

    #[test]
    fn feed_without_full_package_for_version_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("releases.osx.json"),
            feed("0.1.0", "Full", HASH),
        )
        .unwrap();
        assert!(collect_package_hashes(dir.path(), "0.2.0").is_err());
        fs::write(
            dir.path().join("releases.osx.json"),
            feed("0.2.0", "Delta", HASH),
        )
        .unwrap();
        assert!(collect_package_hashes(dir.path(), "0.2.0").is_err());
    }

    #[test]
    fn adds_mobile_package_hashes() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Aragorn-0.2.0.apk"), b"abc").unwrap();
        fs::write(dir.path().join("Aragorn-0.2.0.ipa"), b"").unwrap();
        let mut found = hashes();
        add_mobile_hashes(&mut found, dir.path(), "0.2.0").unwrap();
        // SHA256("abc"), SHA256("")
        assert_eq!(
            found["android"],
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            found["ios"],
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(found["win"], HASH);
    }

    #[test]
    fn missing_mobile_package_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Aragorn-0.2.0.apk"), b"apk").unwrap();
        assert!(add_mobile_hashes(&mut hashes(), dir.path(), "0.2.0").is_err());
    }

    #[test]
    fn rejects_minimum_above_version() {
        assert!(build_policy_json("0.1.0", &toml_policy("0.2.0"), &hashes()).is_err());
    }

    #[test]
    fn built_policy_verifies_after_signing() {
        let secret = [3u8; 32];
        let json = build_policy_json("0.1.0", &toml_policy("0.1.0"), &hashes()).unwrap();
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
        assert_eq!(
            decode_secret(&format!("{}\n", B64.encode(secret))).unwrap(),
            secret
        );
    }

    #[test]
    fn decode_secret_rejects_wrong_length() {
        assert!(decode_secret(&B64.encode([1u8; 16])).is_err());
        assert!(decode_secret("not base64!").is_err());
    }
}
