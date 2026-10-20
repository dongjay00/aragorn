//! 최근에 연 게임 목록. 설정 파일에 함께 저장한다.

use serde::{Deserialize, Serialize};

/// 목록에 남기는 최대 개수.
pub const MAX_RECENT: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentGame {
    /// ROM 파일 경로.
    pub path: String,
    /// 게임 제목. 헤더에 제목이 없으면 파일 이름이다.
    pub title: String,
    /// 마지막으로 연 시각(유닉스 초).
    pub opened_at: u64,
}

/// 맨 앞에 넣는다. 같은 경로는 한 번만 남기고, `MAX_RECENT`개를 넘으면 오래된 것을 버린다.
pub fn add(list: &mut Vec<RecentGame>, game: RecentGame) {
    list.retain(|g| g.path != game.path);
    list.insert(0, game);
    list.truncate(MAX_RECENT);
}

/// 경로가 `path`인 항목을 뺀다 (파일이 사라졌을 때).
pub fn remove(list: &mut Vec<RecentGame>, path: &str) {
    list.retain(|g| g.path != path);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(path: &str, at: u64) -> RecentGame {
        RecentGame {
            path: path.into(),
            title: path.to_uppercase(),
            opened_at: at,
        }
    }

    #[test]
    fn newest_first_without_duplicates() {
        let mut list = Vec::new();
        add(&mut list, game("a", 1));
        add(&mut list, game("b", 2));
        add(&mut list, game("a", 3));
        assert_eq!(
            list.iter()
                .map(|g| (&*g.path, g.opened_at))
                .collect::<Vec<_>>(),
            [("a", 3), ("b", 2)]
        );
    }

    #[test]
    fn keeps_at_most_max() {
        let mut list = Vec::new();
        for i in 0..MAX_RECENT + 3 {
            add(&mut list, game(&i.to_string(), i as u64));
        }
        assert_eq!(list.len(), MAX_RECENT);
        assert_eq!(list[0].path, (MAX_RECENT + 2).to_string());
    }

    #[test]
    fn remove_drops_the_path() {
        let mut list = vec![game("a", 1), game("b", 2)];
        remove(&mut list, "a");
        assert_eq!(list, [game("b", 2)]);
    }
}
