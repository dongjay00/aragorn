use aragorn_app::update::{UpdateError, UpdatePolicy, UpdateSource, verify_and_parse};
use std::{
    io::Read,
    time::{Duration, Instant},
};

const POLICY_FILE: &str = "update-policy.json";
const SIGNATURE_FILE: &str = "update-policy.json.sig";
const MAX_BYTES: u64 = 64 * 1024;

/// GitHub Releases의 latest 릴리스에서 서명된 정책을 받아온다.
pub struct GithubPolicySource {
    agent: ureq::Agent,
    base_url: String,
    public_key: [u8; 32],
    /// 정책과 서명 두 요청을 합친 전체 제한 시간
    timeout: Duration,
}

impl GithubPolicySource {
    pub fn new(base_url: String, public_key: [u8; 32], timeout: Duration) -> Self {
        let agent = ureq::AgentBuilder::new().timeout(timeout).build();
        Self {
            agent,
            base_url,
            public_key,
            timeout,
        }
    }

    fn get(&self, file: &str, deadline: Instant) -> Result<Vec<u8>, UpdateError> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(UpdateError::Network("업데이트 확인 시간 초과".into()));
        }
        let url = format!("{}/{file}", self.base_url);
        let response = self
            .agent
            .get(&url)
            .timeout(remaining)
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
        let deadline = Instant::now() + self.timeout;
        let json = self.get(POLICY_FILE, deadline)?;
        let signature = self.get(SIGNATURE_FILE, deadline)?;
        verify_and_parse(&json, &signature, &self.public_key)
    }
}

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
        serve_with_delay(routes, Duration::ZERO)
    }

    /// 매 요청마다 `delay`만큼 기다린 뒤 응답한다.
    fn serve_with_delay(routes: Vec<(&'static str, Vec<u8>)>, delay: Duration) -> String {
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
                let path = request_line
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("")
                    .to_string();
                thread::sleep(delay);
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
    fn total_time_is_bounded_across_both_requests() {
        let sig = sign_policy(POLICY, &SECRET).to_vec();
        let base = serve_with_delay(
            vec![
                ("/update-policy.json", POLICY.to_vec()),
                ("/update-policy.json.sig", sig),
            ],
            Duration::from_millis(400),
        );
        let started = Instant::now();
        let result = source(base, Duration::from_millis(600)).fetch_policy();
        assert!(matches!(result, Err(UpdateError::Network(_))), "{result:?}");
        assert!(
            started.elapsed() < Duration::from_millis(900),
            "{:?}",
            started.elapsed()
        );
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
