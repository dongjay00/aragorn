//! 시리얼 포트 (Pan Docs "Serial Data Transfer")와 링크 케이블.
//!
//! 링크가 없으면 내부 클럭 전송은 즉시 끝난 것으로 처리하고(받은 값 0xFF), 보낸 바이트를 `output`에
//! 모은다 (테스트 ROM 결과 출력용).
//!
//! 링크가 있으면 상대 기기와 [`LinkMessage`]를 주고받는다. 두 기기의 실행 시점이 맞지 않으므로 클럭을
//! 맞추는 대신 이렇게 한다.
//! - 외부 클럭으로 받을 준비를 하면(`SC`=0x80) 지금 `SB`를 `Ready`로 미리 알린다.
//! - 내부 클럭으로 보내면(`SC`=0x81) 상대가 알려 둔 바이트를 받고 내 바이트를 `Data`로 보낸다. 상대가
//!   아직 준비되지 않았으면 준비될 때까지 전송을 끝내지 않는다 (게임은 인터럽트를 기다린다).
//! - `Data`를 받으면 그 바이트로 전송을 끝낸다.

pub const SB: u16 = 0xFF01;
pub const SC: u16 = 0xFF02;

/// 보관하는 시리얼 출력의 최대 바이트 수. 넘치면 오래된 쪽부터 버린다.
pub const OUTPUT_LIMIT: usize = 64 * 1024;

/// 내부 클럭(8192Hz)으로 한 바이트를 보내는 시간 (M-사이클).
const BYTE_CYCLES: u16 = 1024;

/// 링크 케이블로 상대 기기와 주고받는 메시지.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkMessage {
    /// 외부 클럭으로 받을 준비가 됐다. 상대가 보내면 이 바이트를 받는다.
    Ready(u8),
    /// 더는 받을 준비가 아니다.
    Unready,
    /// 내부 클럭으로 보낸 바이트.
    Data(u8),
}

/// 연결된 링크의 상태. 스테이트에 넣지 않는다.
#[derive(Debug, Clone, Default)]
struct Link {
    /// 상대가 받을 준비를 하며 알려 준 바이트.
    peer_ready: Option<u8>,
    /// 내부 클럭으로 시작했지만 상대가 아직 준비되지 않았다.
    waiting: bool,
    /// 받은 바이트와 전송이 끝날 때까지 남은 M-사이클.
    finishing: Option<(u8, u16)>,
    /// 받을 준비가 됐다고 알렸다.
    announced: bool,
    /// 준비하기 전에 받은 바이트. 준비하면 바로 그 바이트로 끝낸다.
    early: Option<u8>,
    outbox: Vec<LinkMessage>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Serial {
    sb: u8,
    sc: u8,
    /// 테스트 ROM 결과 출력용이라 스테이트에 넣지 않는다.
    #[serde(skip)]
    output: Vec<u8>,
    #[serde(skip)]
    link: Option<Link>,
}

impl Serial {
    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            SB => self.sb,
            _ => self.sc | 0x7E,
        }
    }

    /// 전송이 끝나 시리얼 인터럽트를 요청해야 하면 `true`.
    pub fn write(&mut self, addr: u16, value: u8) -> bool {
        if addr == SB {
            self.sb = value;
            if let Some(link) = &mut self.link
                && link.announced
            {
                link.outbox.push(LinkMessage::Ready(value));
            }
            return false;
        }
        self.sc = value;
        match &mut self.link {
            None => self.write_sc_unlinked(value),
            Some(link) => {
                let start = value & 0x81;
                // 받을 준비를 그만두면(내부 클럭으로 바꾸거나 끄면) 알린다.
                if link.announced && start != 0x80 {
                    link.announced = false;
                    link.outbox.push(LinkMessage::Unready);
                }
                match start {
                    0x81 => {
                        match link.peer_ready.take() {
                            Some(byte) => {
                                link.outbox.push(LinkMessage::Data(self.sb));
                                link.finishing = Some((byte, BYTE_CYCLES));
                                link.waiting = false;
                            }
                            None => link.waiting = true,
                        }
                        false
                    }
                    0x80 => {
                        link.waiting = false;
                        if let Some(byte) = link.early.take() {
                            self.finish(byte);
                            return true;
                        }
                        if !link.announced {
                            link.announced = true;
                            link.outbox.push(LinkMessage::Ready(self.sb));
                        }
                        false
                    }
                    _ => {
                        link.waiting = false;
                        false
                    }
                }
            }
        }
    }

    /// 링크가 없을 때: 내부 클럭 전송은 즉시 끝나고 0xFF를 받는다.
    fn write_sc_unlinked(&mut self, value: u8) -> bool {
        if value & 0x81 != 0x81 {
            return false;
        }
        if self.output.len() >= OUTPUT_LIMIT {
            self.output.drain(..OUTPUT_LIMIT / 2);
        }
        self.output.push(self.sb);
        self.sb = 0xFF;
        self.sc &= 0x7F;
        true
    }

    fn finish(&mut self, received: u8) {
        self.sb = received;
        self.sc &= 0x7F;
    }

    /// M-사이클마다 부른다. 링크 전송이 끝나면 `true` (시리얼 인터럽트).
    pub fn tick(&mut self) -> bool {
        let Some(link) = &mut self.link else {
            return false;
        };
        let Some((byte, left)) = link.finishing else {
            return false;
        };
        if left > 1 {
            link.finishing = Some((byte, left - 1));
            return false;
        }
        link.finishing = None;
        self.finish(byte);
        true
    }

    /// 링크를 잇거나 끊는다. 끊으면 끝나지 않은 전송은 0xFF를 받고 끝난다(`true`면 인터럽트).
    pub fn set_link(&mut self, connected: bool) -> bool {
        if connected {
            if self.link.is_none() {
                let mut link = Link::default();
                // 이미 받을 준비를 하고 있었으면 바로 알린다.
                if self.sc & 0x81 == 0x80 {
                    link.announced = true;
                    link.outbox.push(LinkMessage::Ready(self.sb));
                }
                self.link = Some(link);
            }
            return false;
        }
        let Some(link) = self.link.take() else {
            return false;
        };
        if link.waiting || link.finishing.is_some() {
            self.finish(0xFF);
            return true;
        }
        false
    }

    pub fn is_linked(&self) -> bool {
        self.link.is_some()
    }

    /// 상대가 보낸 메시지를 받는다. 받은 바이트로 전송이 끝나면 `true` (시리얼 인터럽트).
    pub fn receive(&mut self, message: LinkMessage) -> bool {
        let Some(link) = &mut self.link else {
            return false;
        };
        match message {
            LinkMessage::Ready(byte) => {
                if link.waiting {
                    link.waiting = false;
                    link.outbox.push(LinkMessage::Data(self.sb));
                    link.finishing = Some((byte, BYTE_CYCLES));
                } else {
                    link.peer_ready = Some(byte);
                }
                false
            }
            LinkMessage::Unready => {
                link.peer_ready = None;
                false
            }
            LinkMessage::Data(byte) => {
                if link.announced && self.sc & 0x81 == 0x80 {
                    link.announced = false;
                    self.finish(byte);
                    true
                } else {
                    link.early = Some(byte);
                    false
                }
            }
        }
    }

    /// 상대에게 보낼 메시지를 꺼낸다.
    pub fn take_outbox(&mut self) -> Vec<LinkMessage> {
        self.link
            .as_mut()
            .map(|link| std::mem::take(&mut link.outbox))
            .unwrap_or_default()
    }

    /// 스테이트를 불러올 때: 링크 연결은 그대로 두고 진행 중이던 전송은 버린다.
    pub(crate) fn adopt_host(&mut self, host: &Serial) {
        self.link = None;
        if host.link.is_some() {
            self.set_link(true);
        }
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
    fn output_buffer_is_bounded_and_keeps_latest_bytes() {
        let mut s = Serial::default();
        for i in 0..(OUTPUT_LIMIT * 3) {
            s.write(SB, (i % 251) as u8);
            s.write(SC, 0x81);
        }
        assert!(s.output().len() <= OUTPUT_LIMIT);
        let last = ((OUTPUT_LIMIT * 3 - 1) % 251) as u8;
        assert_eq!(s.output().last(), Some(&last));
    }

    #[test]
    fn external_clock_does_not_transfer() {
        let mut s = Serial::default();
        s.write(SB, b'A');
        assert!(!s.write(SC, 0x80));
        assert!(s.output().is_empty());
        assert_eq!(s.read(SC), 0xFE);
    }

    /// 한쪽이 보낸 메시지를 다른 쪽에 전한다. 인터럽트가 생긴 쪽을 돌려준다.
    fn deliver(from: &mut Serial, to: &mut Serial) -> bool {
        let mut irq = false;
        for message in from.take_outbox() {
            irq |= to.receive(message);
        }
        irq
    }

    fn run(s: &mut Serial, cycles: u16) -> bool {
        (0..cycles).any(|_| s.tick())
    }

    fn linked() -> (Serial, Serial) {
        let (mut a, mut b) = (Serial::default(), Serial::default());
        a.set_link(true);
        b.set_link(true);
        (a, b)
    }

    #[test]
    fn ready_slave_lets_master_exchange_without_waiting() {
        let (mut master, mut slave) = linked();
        slave.write(SB, 0x22);
        slave.write(SC, 0x80);
        deliver(&mut slave, &mut master);
        master.write(SB, 0x11);
        assert!(!master.write(SC, 0x81), "한 바이트 시간 뒤에 끝난다");
        assert!(run(&mut master, BYTE_CYCLES));
        assert_eq!((master.read(SB), master.read(SC) & 0x80), (0x22, 0));
        assert!(deliver(&mut master, &mut slave), "받는 쪽도 인터럽트");
        assert_eq!((slave.read(SB), slave.read(SC) & 0x80), (0x11, 0));
    }

    #[test]
    fn master_waits_until_slave_is_ready() {
        let (mut master, mut slave) = linked();
        master.write(SB, 0x33);
        master.write(SC, 0x81);
        assert!(!run(&mut master, BYTE_CYCLES * 3), "상대를 기다린다");
        assert_eq!(master.read(SC) & 0x80, 0x80);
        slave.write(SB, 0x44);
        slave.write(SC, 0x80);
        deliver(&mut slave, &mut master);
        assert!(run(&mut master, BYTE_CYCLES));
        assert_eq!(master.read(SB), 0x44);
        assert!(deliver(&mut master, &mut slave));
        assert_eq!(slave.read(SB), 0x33);
    }

    #[test]
    fn changing_sb_while_ready_updates_the_peer() {
        let (mut master, mut slave) = linked();
        slave.write(SC, 0x80);
        slave.write(SB, 0x55);
        deliver(&mut slave, &mut master);
        master.write(SC, 0x81);
        run(&mut master, BYTE_CYCLES);
        assert_eq!(master.read(SB), 0x55, "나중 값");
    }

    #[test]
    fn slave_that_stopped_waiting_is_not_used() {
        let (mut master, mut slave) = linked();
        slave.write(SC, 0x80);
        slave.write(SC, 0x00);
        deliver(&mut slave, &mut master);
        master.write(SC, 0x81);
        assert!(!run(&mut master, BYTE_CYCLES * 2));
    }

    #[test]
    fn disconnecting_finishes_a_waiting_transfer_with_ff() {
        let (mut master, _) = linked();
        master.write(SB, 0x01);
        master.write(SC, 0x81);
        assert!(master.set_link(false));
        assert_eq!(master.read(SB), 0xFF);
        assert!(!master.is_linked());
    }
}
