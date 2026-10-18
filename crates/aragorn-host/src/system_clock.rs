//! 운영체제 시계 (`Clock` 어댑터).

use aragorn_app::session::Clock;
use std::time::SystemTime;

pub struct SystemClock;

impl Clock for SystemClock {
    /// 시계가 1970년보다 앞서 있으면 0이다.
    fn now_unix(&self) -> u64 {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_a_time_after_2020() {
        assert!(SystemClock.now_unix() > 1_577_836_800);
    }
}
