//! 게임마다 저장하는 치트 목록. 세이브 옆 `<ROM 이름>.cht`(JSON)에 둔다.

use gb_core::Cheat;
use serde::{Deserialize, Serialize};

/// 목록의 치트 하나. 코드는 사용자가 입력한 그대로 보관한다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheatEntry {
    pub code: String,
    /// 사용자가 붙인 이름. 비어 있으면 코드를 보인다.
    #[serde(default)]
    pub name: String,
    #[serde(default = "enabled")]
    pub enabled: bool,
}

fn enabled() -> bool {
    true
}

impl CheatEntry {
    /// 코드를 확인하고 켠 상태로 만든다. 여러 코드를 `+`나 줄바꿈으로 이어 쓸 수 있다
    /// (게임 지니 코드 여러 개가 한 치트인 경우가 많다).
    pub fn new(code: &str, name: &str) -> Result<CheatEntry, String> {
        let entry = CheatEntry {
            code: normalize(code),
            name: name.trim().to_string(),
            enabled: true,
        };
        entry.cheats().map_err(|e| e.to_string())?;
        Ok(entry)
    }

    /// 코드를 해석한다. 하나라도 틀리면 오류다.
    pub fn cheats(&self) -> Result<Vec<Cheat>, gb_core::CheatError> {
        let parts: Vec<&str> = self
            .code
            .split('+')
            .filter(|p| !p.trim().is_empty())
            .collect();
        if parts.is_empty() {
            return Err(gb_core::CheatError::Format);
        }
        parts.into_iter().map(Cheat::parse).collect()
    }

    /// 목록에 보일 이름.
    pub fn label(&self) -> &str {
        if self.name.is_empty() {
            &self.code
        } else {
            &self.name
        }
    }
}

/// 대문자로 바꾸고 줄바꿈·쉼표를 `+`로 모은다.
fn normalize(code: &str) -> String {
    code.split(['\n', ',', '+', ';'])
        .map(|part| part.trim().to_uppercase())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("+")
}

/// 켠 치트를 코어에 넘길 모양으로 모은다. 해석할 수 없는 항목(손으로 고친 파일)은 건너뛴다.
pub fn active(entries: &[CheatEntry]) -> Vec<Cheat> {
    entries
        .iter()
        .filter(|e| e.enabled)
        .filter_map(|e| e.cheats().ok())
        .flatten()
        .collect()
}

pub fn encode(entries: &[CheatEntry]) -> Vec<u8> {
    serde_json::to_vec_pretty(entries).expect("문자열과 bool만 있다")
}

pub fn decode(data: &[u8]) -> Result<Vec<CheatEntry>, String> {
    serde_json::from_slice(data).map_err(|e| format!("치트 파일을 읽을 수 없습니다: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn several_codes_become_one_entry() {
        let entry = CheatEntry::new("00a-17b-c49\n 3EA-17B ", "무적").unwrap();
        assert_eq!(entry.code, "00A-17B-C49+3EA-17B");
        assert_eq!(entry.cheats().unwrap().len(), 2);
        assert_eq!(entry.label(), "무적");
    }

    #[test]
    fn invalid_code_is_reported() {
        let err = CheatEntry::new("hello", "").unwrap_err();
        assert!(err.contains("게임 지니"), "{err}");
        assert!(CheatEntry::new("  ", "").is_err());
    }

    #[test]
    fn only_enabled_entries_are_active() {
        let mut off = CheatEntry::new("010138CD", "").unwrap();
        off.enabled = false;
        let on = CheatEntry::new("016338CD", "돈").unwrap();
        assert_eq!(active(&[off, on]).len(), 1);
    }

    #[test]
    fn file_round_trips_and_defaults_missing_fields() {
        let entries = vec![CheatEntry::new("010138CD", "체력").unwrap()];
        assert_eq!(decode(&encode(&entries)).unwrap(), entries);
        let old = decode(br#"[{"code":"010138CD"}]"#).unwrap();
        assert!(old[0].enabled);
        assert_eq!(old[0].label(), "010138CD");
    }
}
