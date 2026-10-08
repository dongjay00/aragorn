//! 디버거 상태: PC 브레이크포인트, 정지 위치, 최근 실행한 PC (스펙 §4.3, §6.3).
//! 세이브 스테이트에는 넣지 않는다.

/// 최근 실행한 명령을 이만큼 기억한다.
pub const TRACE_LEN: usize = 16;

#[derive(Debug, Clone)]
pub struct Debugger {
    /// 주소마다 1비트. 명령마다 확인하므로 집합 대신 비트맵을 쓴다.
    breakpoints: Box<[u64; 1024]>,
    count: usize,
    /// 브레이크포인트에 걸려 멈춘 PC. 다음 실행은 이 PC의 명령을 한 번 통과시킨다.
    hit: Option<u16>,
    trace: [u16; TRACE_LEN],
    /// 다음에 쓸 자리와 채워진 개수
    trace_next: usize,
    trace_len: usize,
}

impl Default for Debugger {
    fn default() -> Self {
        Self {
            breakpoints: Box::new([0; 1024]),
            count: 0,
            hit: None,
            trace: [0; TRACE_LEN],
            trace_next: 0,
            trace_len: 0,
        }
    }
}

impl Debugger {
    pub fn set_breakpoints(&mut self, pcs: &[u16]) {
        self.breakpoints.fill(0);
        for &pc in pcs {
            self.breakpoints[usize::from(pc >> 6)] |= 1 << (pc & 63);
        }
        self.count = self
            .breakpoints
            .iter()
            .map(|w| w.count_ones() as usize)
            .sum();
    }

    pub fn is_breakpoint(&self, pc: u16) -> bool {
        self.count != 0 && self.breakpoints[usize::from(pc >> 6)] & (1 << (pc & 63)) != 0
    }

    pub fn hit(&self) -> Option<u16> {
        self.hit
    }

    pub fn set_hit(&mut self, pc: Option<u16>) {
        self.hit = pc;
    }

    pub fn record(&mut self, pc: u16) {
        self.trace[self.trace_next] = pc;
        self.trace_next = (self.trace_next + 1) % TRACE_LEN;
        self.trace_len = (self.trace_len + 1).min(TRACE_LEN);
    }

    /// 최근 실행한 PC, 오래된 것부터.
    pub fn trace(&self) -> Vec<u16> {
        (0..self.trace_len)
            .map(|i| self.trace[(self.trace_next + TRACE_LEN - self.trace_len + i) % TRACE_LEN])
            .collect()
    }

    /// 스테이트를 불러온 뒤: 브레이크포인트는 두고 정지 위치와 실행 흔적은 지운다.
    pub fn reset_run_state(&mut self) {
        self.hit = None;
        self.trace_next = 0;
        self.trace_len = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breakpoints_are_replaced_as_a_set() {
        let mut d = Debugger::default();
        assert!(!d.is_breakpoint(0x0150));
        d.set_breakpoints(&[0x0150, 0xFFFF, 0x0000, 0x0150]);
        assert!(d.is_breakpoint(0x0150));
        assert!(d.is_breakpoint(0xFFFF));
        assert!(d.is_breakpoint(0x0000));
        assert!(!d.is_breakpoint(0x0151));
        d.set_breakpoints(&[]);
        assert!(!d.is_breakpoint(0x0150));
    }

    #[test]
    fn trace_keeps_last_entries_oldest_first() {
        let mut d = Debugger::default();
        assert!(d.trace().is_empty());
        for pc in 0..3u16 {
            d.record(pc);
        }
        assert_eq!(d.trace(), [0, 1, 2]);
        for pc in 3..40u16 {
            d.record(pc);
        }
        let expected: Vec<u16> = (40 - TRACE_LEN as u16..40).collect();
        assert_eq!(d.trace(), expected);
        d.reset_run_state();
        assert!(d.trace().is_empty());
    }
}
