//! 시리얼 포트 (Pan Docs "Serial Data Transfer").
//!
//! 링크 케이블은 지원하지 않는다. 내부 클럭 전송은 즉시 끝난 것으로 처리하고,
//! 보낸 바이트를 `output`에 모은다 (테스트 ROM 결과 출력용).

pub const SB: u16 = 0xFF01;
pub const SC: u16 = 0xFF02;

#[derive(Debug, Clone, Default)]
pub struct Serial {
    sb: u8,
    sc: u8,
    output: Vec<u8>,
}

impl Serial {
    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            SB => self.sb,
            _ => self.sc | 0x7E,
        }
    }

    /// 내부 클럭(SC=0x81) 전송이 시작되면 즉시 끝내고 `true` (시리얼 인터럽트 요청).
    pub fn write(&mut self, addr: u16, value: u8) -> bool {
        if addr == SB {
            self.sb = value;
            return false;
        }
        self.sc = value;
        if value & 0x81 != 0x81 {
            return false;
        }
        self.output.push(self.sb);
        self.sb = 0xFF;
        self.sc &= 0x7F;
        true
    }

    pub fn output(&self) -> &[u8] {
        &self.output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_clock_transfer_captures_byte() {
        let mut s = Serial::default();
        s.write(SB, b'A');
        assert!(s.write(SC, 0x81));
        assert_eq!(s.output(), b"A");
        assert_eq!(s.read(SC) & 0x80, 0);
        assert_eq!(s.read(SB), 0xFF);
    }

    #[test]
    fn external_clock_does_not_transfer() {
        let mut s = Serial::default();
        s.write(SB, b'A');
        assert!(!s.write(SC, 0x80));
        assert!(s.output().is_empty());
        assert_eq!(s.read(SC), 0xFE);
    }
}
