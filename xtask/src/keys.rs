use crate::policy::encode_secret;
use aragorn_app::update::public_key_for;
use std::{fs, io::Write, path::Path};

/// 정책 서명 키 쌍을 만든다. 이미 배포된 앱에는 공개키가 내장되어 있으므로,
/// 공개키 교체는 `rotate`를 명시했을 때만 허용한다. 비밀키 파일은 절대 덮어쓰지 않는다.
pub fn generate_key_pair(secret_out: &Path, public_key: &Path, rotate: bool) -> Result<(), String> {
    if secret_out.exists() {
        return Err(format!(
            "{}가 이미 있습니다. 덮어쓰지 않습니다",
            secret_out.display()
        ));
    }
    if public_key.exists() && !rotate {
        return Err(format!(
            "{}가 이미 있습니다. 교체하면 이미 배포된 앱은 이후 모든 정책을 거부합니다. \
             정말 교체하려면 --rotate를 붙이세요",
            public_key.display()
        ));
    }
    let mut secret = [0u8; 32];
    getrandom::fill(&mut secret).map_err(|e| e.to_string())?;
    if let Some(dir) = secret_out.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(secret_out).map_err(|e| e.to_string())?;
    file.write_all(encode_secret(&secret).as_bytes())
        .map_err(|e| e.to_string())?;
    fs::write(public_key, public_key_for(&secret)).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn creates_secret_and_public_key() {
        let dir = tempfile::tempdir().unwrap();
        let (secret, public) = (
            dir.path().join("s/secret.b64"),
            dir.path().join("policy.pub"),
        );
        generate_key_pair(&secret, &public, false).unwrap();
        assert!(secret.exists());
        assert_eq!(fs::read(&public).unwrap().len(), 32);
    }

    #[test]
    fn refuses_to_replace_existing_public_key() {
        let dir = tempfile::tempdir().unwrap();
        let (secret, public) = (dir.path().join("secret.b64"), dir.path().join("policy.pub"));
        fs::write(&public, [1u8; 32]).unwrap();
        assert!(generate_key_pair(&secret, &public, false).is_err());
        assert_eq!(
            fs::read(&public).unwrap(),
            [1u8; 32],
            "기존 공개키가 바뀌면 안 된다"
        );
        assert!(!secret.exists());
    }

    #[test]
    fn rotate_replaces_existing_public_key() {
        let dir = tempfile::tempdir().unwrap();
        let (secret, public) = (dir.path().join("secret.b64"), dir.path().join("policy.pub"));
        fs::write(&public, [1u8; 32]).unwrap();
        generate_key_pair(&secret, &public, true).unwrap();
        assert_ne!(fs::read(&public).unwrap(), [1u8; 32]);
    }

    #[test]
    fn refuses_to_replace_existing_secret() {
        let dir = tempfile::tempdir().unwrap();
        let (secret, public) = (dir.path().join("secret.b64"), dir.path().join("policy.pub"));
        fs::write(&secret, "old").unwrap();
        assert!(generate_key_pair(&secret, &public, true).is_err());
        assert_eq!(fs::read_to_string(&secret).unwrap(), "old");
    }
}
