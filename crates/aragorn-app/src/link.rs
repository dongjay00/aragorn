//! 링크 케이블 (포트). 두 기기의 에뮬레이터가 시리얼 포트 메시지를 주고받는다.

pub use gb_core::LinkMessage;

/// 링크 연결 상태.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkStatus {
    /// 상대가 접속하기를 기다린다. `address`는 상대가 입력할 주소다.
    Waiting {
        address: String,
    },
    /// 상대에 접속하는 중이다.
    Connecting {
        address: String,
    },
    Connected {
        peer: String,
    },
    /// 끊겼다. 이유가 있으면 함께 보인다.
    Closed {
        reason: Option<String>,
    },
}

impl LinkStatus {
    /// 상태 표시줄에 보일 문구.
    pub fn text(&self) -> String {
        match self {
            LinkStatus::Waiting { address } => format!("링크: {address}에서 상대를 기다리는 중"),
            LinkStatus::Connecting { address } => format!("링크: {address}에 연결하는 중"),
            LinkStatus::Connected { peer } => format!("링크: {peer}와 연결됨"),
            LinkStatus::Closed {
                reason: Some(reason),
            } => format!("링크가 끊겼습니다: {reason}"),
            LinkStatus::Closed { reason: None } => "링크가 끊겼습니다".to_string(),
        }
    }
}

/// 상대 기기와 메시지를 주고받는 통로. 막히지 않아야 한다 (게임 루프에서 부른다).
pub trait LinkTransport: Send {
    fn send(&mut self, message: LinkMessage);
    /// 받은 메시지가 있으면 하나 꺼낸다.
    fn try_recv(&mut self) -> Option<LinkMessage>;
    fn status(&self) -> LinkStatus;
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use std::sync::mpsc::{Receiver, Sender, channel};

    /// 같은 프로세스 안에서 이어진 링크 한 쌍.
    pub struct ChannelLink {
        tx: Sender<LinkMessage>,
        rx: Receiver<LinkMessage>,
    }

    pub fn pair() -> (ChannelLink, ChannelLink) {
        let (a_tx, b_rx) = channel();
        let (b_tx, a_rx) = channel();
        (
            ChannelLink { tx: a_tx, rx: a_rx },
            ChannelLink { tx: b_tx, rx: b_rx },
        )
    }

    impl LinkTransport for ChannelLink {
        fn send(&mut self, message: LinkMessage) {
            let _ = self.tx.send(message);
        }
        fn try_recv(&mut self) -> Option<LinkMessage> {
            self.rx.try_recv().ok()
        }
        fn status(&self) -> LinkStatus {
            LinkStatus::Connected {
                peer: "test".into(),
            }
        }
    }
}
