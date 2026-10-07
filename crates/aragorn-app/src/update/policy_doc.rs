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
    key.verify(json, &signature)
        .map_err(|_| UpdateError::BadSignature)?;
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
    Ok(UpdatePolicy {
        latest,
        minimum_supported,
        message: doc.message,
    })
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
        assert_eq!(
            verify_and_parse(tampered, &sig, &pk()),
            Err(UpdateError::BadSignature)
        );
    }

    #[test]
    fn rejects_signature_from_other_key() {
        let sig = sign_policy(VALID, &OTHER_SECRET);
        assert_eq!(
            verify_and_parse(VALID, &sig, &pk()),
            Err(UpdateError::BadSignature)
        );
    }

    #[test]
    fn rejects_signature_of_wrong_length() {
        let html = b"<html>404 Not Found</html>";
        assert_eq!(
            verify_and_parse(VALID, html, &pk()),
            Err(UpdateError::BadSignature)
        );
    }

    #[test]
    fn rejects_html_body_even_if_signed() {
        let body = b"<!DOCTYPE html><html>Not Found</html>";
        let sig = sign_policy(body, &SECRET);
        assert!(matches!(
            verify_and_parse(body, &sig, &pk()),
            Err(UpdateError::Malformed(_))
        ));
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
