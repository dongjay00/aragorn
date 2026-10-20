//! 링크 케이블을 TCP로 잇는다. 한쪽이 기다리고(`host`) 다른 쪽이 그 주소로 접속한다(`connect`).
//!
//! 접속하면 서로 `ARGNLNK1`을 보내 확인한 뒤, 메시지마다 2바이트(종류, 값)를 주고받는다.
//! 읽기는 별도 스레드가 하고 게임 루프는 채널에서 꺼내기만 한다.

use aragorn_app::link::{LinkMessage, LinkStatus, LinkTransport};
use std::{
    io::{self, Read, Write},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket},
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, Sender, channel},
    },
    thread,
    time::Duration,
};

/// 기본 포트.
pub const DEFAULT_PORT: u16 = 5151;
const HELLO: &[u8; 8] = b"ARGNLNK1";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

struct Shared {
    status: Mutex<LinkStatus>,
    writer: Mutex<Option<TcpStream>>,
    /// 링크를 버렸다. 기다리던 스레드가 그만둔다.
    closed: AtomicBool,
}

impl Shared {
    fn set_status(&self, status: LinkStatus) {
        *self.status.lock().unwrap_or_else(PoisonError::into_inner) = status;
    }

    fn close(&self, reason: Option<String>) {
        if let Some(stream) = self
            .writer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            let _ = stream.shutdown(Shutdown::Both);
        }
        let mut status = self.status.lock().unwrap_or_else(PoisonError::into_inner);
        if !matches!(*status, LinkStatus::Closed { .. }) {
            *status = LinkStatus::Closed { reason };
        }
    }
}

pub struct TcpLink {
    shared: Arc<Shared>,
    received: Receiver<LinkMessage>,
}

impl TcpLink {
    /// `port`에서 상대를 기다린다. 상대가 입력할 주소(이 기기의 LAN 주소)를 상태로 보인다.
    pub fn host(port: u16) -> io::Result<TcpLink> {
        let listener = TcpListener::bind(("0.0.0.0", port))?;
        listener.set_nonblocking(true)?;
        let address = format!("{}:{port}", local_ip().unwrap_or_else(|| "?".into()));
        let (link, tx) = Self::new(LinkStatus::Waiting { address });
        let shared = Arc::clone(&link.shared);
        thread::Builder::new()
            .name("aragorn-link-host".into())
            .spawn(move || {
                // 링크를 버리면 그만둘 수 있게 짧게 자며 기다린다.
                while !shared.closed.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((stream, peer)) => {
                            let _ = stream.set_nonblocking(false);
                            run(stream, peer, &shared, tx);
                            return;
                        }
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(50));
                        }
                        Err(e) => {
                            shared.close(Some(e.to_string()));
                            return;
                        }
                    }
                }
            })?;
        Ok(link)
    }

    /// `address`(`192.168.0.5:5151`, 포트를 빼면 기본 포트)에 접속한다.
    pub fn connect(address: &str) -> TcpLink {
        let address = address.trim().to_string();
        let target = if address.contains(':') {
            address.clone()
        } else {
            format!("{address}:{DEFAULT_PORT}")
        };
        let (link, tx) = Self::new(LinkStatus::Connecting {
            address: target.clone(),
        });
        let shared = Arc::clone(&link.shared);
        let spawned = thread::Builder::new()
            .name("aragorn-link-connect".into())
            .spawn(move || {
                let connected = target
                    .to_socket_addrs()
                    .and_then(|mut addrs| {
                        addrs.next().ok_or_else(|| {
                            io::Error::new(io::ErrorKind::NotFound, "주소를 찾을 수 없습니다")
                        })
                    })
                    .and_then(|addr| {
                        TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).map(|s| (s, addr))
                    });
                match connected {
                    Ok((stream, peer)) => run(stream, peer, &shared, tx),
                    Err(e) => shared.close(Some(e.to_string())),
                }
            });
        if let Err(e) = spawned {
            link.shared.close(Some(e.to_string()));
        }
        link
    }

    fn new(status: LinkStatus) -> (TcpLink, Sender<LinkMessage>) {
        let (tx, rx) = channel();
        let shared = Arc::new(Shared {
            status: Mutex::new(status),
            writer: Mutex::new(None),
            closed: AtomicBool::new(false),
        });
        (
            TcpLink {
                shared,
                received: rx,
            },
            tx,
        )
    }
}

/// 인사를 주고받고, 연결이 끊길 때까지 받은 메시지를 `tx`로 넘긴다.
fn run(mut stream: TcpStream, peer: SocketAddr, shared: &Shared, tx: Sender<LinkMessage>) {
    let _ = stream.set_nodelay(true);
    let hello = (|| {
        stream.write_all(HELLO)?;
        stream.set_read_timeout(Some(CONNECT_TIMEOUT))?;
        let mut answer = [0u8; 8];
        stream.read_exact(&mut answer)?;
        stream.set_read_timeout(None)?;
        if &answer != HELLO {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Aragorn 링크가 아닙니다 (버전이 다를 수 있습니다)",
            ));
        }
        stream.try_clone()
    })();
    let writer = match hello {
        Ok(writer) => writer,
        Err(e) => {
            shared.close(Some(e.to_string()));
            return;
        }
    };
    if shared.closed.load(Ordering::Acquire) {
        return;
    }
    *shared.writer.lock().unwrap_or_else(PoisonError::into_inner) = Some(writer);
    shared.set_status(LinkStatus::Connected {
        peer: peer.ip().to_string(),
    });
    let mut buf = [0u8; 2];
    loop {
        if let Err(e) = stream.read_exact(&mut buf) {
            let reason = (e.kind() != io::ErrorKind::UnexpectedEof).then(|| e.to_string());
            shared.close(reason.or_else(|| Some("상대가 연결을 끊었습니다".into())));
            return;
        }
        let message = match buf[0] {
            1 => LinkMessage::Ready(buf[1]),
            2 => LinkMessage::Unready,
            3 => LinkMessage::Data(buf[1]),
            _ => {
                shared.close(Some("알 수 없는 메시지를 받았습니다".into()));
                return;
            }
        };
        if tx.send(message).is_err() {
            return;
        }
    }
}

impl LinkTransport for TcpLink {
    fn send(&mut self, message: LinkMessage) {
        let bytes = match message {
            LinkMessage::Ready(b) => [1, b],
            LinkMessage::Unready => [2, 0],
            LinkMessage::Data(b) => [3, b],
        };
        let mut writer = self
            .shared
            .writer
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(stream) = writer.as_mut()
            && let Err(e) = stream.write_all(&bytes)
        {
            drop(writer);
            self.shared.close(Some(e.to_string()));
        }
    }

    fn try_recv(&mut self) -> Option<LinkMessage> {
        self.received.try_recv().ok()
    }

    fn status(&self) -> LinkStatus {
        self.shared
            .status
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Drop for TcpLink {
    fn drop(&mut self) {
        self.shared.closed.store(true, Ordering::Release);
        self.shared.close(None);
    }
}

/// 이 기기의 LAN 주소. 외부 주소로 UDP 소켓을 "연결"해 OS가 고른 출발 주소를 읽는다 (패킷은 보내지 않는다).
pub fn local_ip() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn wait_for(link: &TcpLink, connected: bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let status = link.status();
            let done = matches!(status, LinkStatus::Connected { .. }) == connected
                && !matches!(
                    status,
                    LinkStatus::Waiting { .. } | LinkStatus::Connecting { .. }
                );
            if done {
                return;
            }
            assert!(Instant::now() < deadline, "{status:?}");
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn recv(link: &mut TcpLink) -> LinkMessage {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(message) = link.try_recv() {
                return message;
            }
            assert!(Instant::now() < deadline, "메시지가 오지 않습니다");
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn host_and_client_exchange_messages() {
        let port = 35151;
        let mut host = TcpLink::host(port).unwrap();
        assert!(matches!(host.status(), LinkStatus::Waiting { .. }));
        let mut client = TcpLink::connect(&format!("127.0.0.1:{port}"));
        wait_for(&host, true);
        wait_for(&client, true);
        client.send(LinkMessage::Ready(0x42));
        assert_eq!(recv(&mut host), LinkMessage::Ready(0x42));
        host.send(LinkMessage::Data(0x07));
        host.send(LinkMessage::Unready);
        assert_eq!(recv(&mut client), LinkMessage::Data(0x07));
        assert_eq!(recv(&mut client), LinkMessage::Unready);
        drop(host);
        wait_for(&client, false);
        assert!(matches!(client.status(), LinkStatus::Closed { .. }));
    }

    #[test]
    fn connecting_to_nobody_reports_why() {
        let client = TcpLink::connect("127.0.0.1:1");
        wait_for(&client, false);
        assert!(matches!(
            client.status(),
            LinkStatus::Closed { reason: Some(_) }
        ));
    }
}
