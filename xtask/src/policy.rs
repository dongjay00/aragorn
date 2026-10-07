use aragorn_app::update::parse_policy;
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use serde::Deserialize;

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
    use aragorn_app::update::{public_key_for, sign_policy, verify_and_parse};

    fn toml_policy(min: &str) -> PolicyToml {
        PolicyToml {
            minimum_supported: min.into(),
            message: "hi".into(),
        }
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
