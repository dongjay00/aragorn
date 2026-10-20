//! 표준 라이브러리와 cpal로 만든 어댑터: 세이브 파일, 시계, 오디오 출력, 링크 케이블(TCP).
//! 데스크톱과 모바일(Android/iOS)이 함께 쓴다.

mod cpal_audio;
mod fs_save_store;
mod sample_queue;
mod system_clock;
mod tcp_link;

pub use cpal_audio::CpalAudio;
pub use fs_save_store::FsSaveStore;
pub use system_clock::SystemClock;
pub use tcp_link::{DEFAULT_PORT, TcpLink, local_ip};
