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
        assert_eq!(
            releases_page_url("me/aragorn"),
            "https://github.com/me/aragorn/releases/latest"
        );
    }

    #[test]
    fn current_version_matches_workspace() {
        assert_eq!(current_version().to_string(), env!("CARGO_PKG_VERSION"));
    }
}
