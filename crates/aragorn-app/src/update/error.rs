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
