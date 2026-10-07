//! 타이머: DIV, TIMA, TMA, TAC (Pan Docs "Timer and Divider Registers").
//!
//! 내부 16비트 카운터가 T-사이클마다 1씩 증가하고, DIV는 그 상위 8비트다.
//! TAC가 고른 카운터 비트(AND 타이머 활성)가 1→0으로 떨어질 때 TIMA가 증가한다.
//! 그래서 DIV 쓰기나 TAC 변경으로도 TIMA가 증가할 수 있다.
//! 오버플로 후 1 M-사이클 지연은 M2에서 구현한다.

pub const DIV: u16 = 0xFF04;
pub const TIMA: u16 = 0xFF05;
pub const TMA: u16 = 0xFF06;
pub const TAC: u16 = 0xFF07;

#[derive(Debug, Clone)]
pub struct Timer {
    counter: u16,
    tima: u8,
    tma: u8,
    tac: u8,
}

impl Timer {
    pub fn new(counter: u16) -> Self {
        Self {
            counter,
            tima: 0,
            tma: 0,
            tac: 0,
        }
    }

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            DIV => (self.counter >> 8) as u8,
            TIMA => self.tima,
            TMA => self.tma,
            _ => self.tac | 0xF8,
        }
    }

    /// 쓰기로 TIMA가 넘치면 `true` (타이머 인터럽트 요청).
    pub fn write(&mut self, addr: u16, value: u8) -> bool {
        let before = self.signal();
        match addr {
            DIV => self.counter = 0,
            TIMA => self.tima = value,
            TMA => self.tma = value,
            _ => self.tac = value & 0x07,
        }
        self.falling_edge(before)
    }

    /// 1 M-사이클(4 T-사이클) 진행. TIMA가 넘치면 `true`.
    pub fn tick(&mut self) -> bool {
        let before = self.signal();
        self.counter = self.counter.wrapping_add(4);
        self.falling_edge(before)
    }

    /// TAC가 고른 카운터 비트 AND 타이머 활성.
    fn signal(&self) -> bool {
        const BITS: [u8; 4] = [9, 3, 5, 7];
        self.tac & 0x04 != 0 && self.counter & (1 << BITS[usize::from(self.tac & 0x03)]) != 0
    }

    fn falling_edge(&mut self, before: bool) -> bool {
        if !before || self.signal() {
            return false;
        }
        let (tima, overflow) = self.tima.overflowing_add(1);
        self.tima = if overflow { self.tma } else { tima };
        overflow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn div_increments_every_64_m_cycles() {
        let mut t = Timer::new(0);
        for _ in 0..63 {
            t.tick();
        }
        assert_eq!(t.read(DIV), 0);
        t.tick();
        assert_eq!(t.read(DIV), 1);
    }

    #[test]
    fn writing_div_resets_counter() {
        let mut t = Timer::new(0xABCC);
        t.write(DIV, 0x55);
        assert_eq!(t.read(DIV), 0);
    }

    #[test]
    fn tima_stays_when_disabled() {
        let mut t = Timer::new(0);
        t.write(TAC, 0x01);
        for _ in 0..1000 {
            t.tick();
        }
        assert_eq!(t.read(TIMA), 0);
    }

    #[test]
    fn tima_rates_follow_tac() {
        for (tac, m_cycles) in [(0x04, 256), (0x05, 4), (0x06, 16), (0x07, 64)] {
            let mut t = Timer::new(0);
            t.write(TAC, tac);
            for _ in 0..m_cycles - 1 {
                t.tick();
            }
            assert_eq!(t.read(TIMA), 0, "TAC {tac:#04X}");
            t.tick();
            assert_eq!(t.read(TIMA), 1, "TAC {tac:#04X}");
        }
    }

    #[test]
    fn overflow_reloads_tma_and_requests_interrupt() {
        let mut t = Timer::new(0);
        t.write(TAC, 0x05);
        t.write(TMA, 0x42);
        t.write(TIMA, 0xFF);
        let irqs: Vec<bool> = (0..4).map(|_| t.tick()).collect();
        assert_eq!(irqs, [false, false, false, true]);
        assert_eq!(t.read(TIMA), 0x42);
    }

    #[test]
    fn div_reset_can_increment_tima() {
        let mut t = Timer::new(0);
        t.write(TAC, 0x05);
        t.tick();
        t.tick(); // 카운터 8: 비트 3이 1
        t.write(DIV, 0);
        assert_eq!(t.read(TIMA), 1);
    }

    #[test]
    fn tac_upper_bits_read_as_one() {
        let mut t = Timer::new(0);
        t.write(TAC, 0x05);
        assert_eq!(t.read(TAC), 0xFD);
    }
}
