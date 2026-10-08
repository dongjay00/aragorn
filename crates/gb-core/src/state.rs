//! 세이브 스테이트 파일 형식 (스펙 §4.8).
//!
//! `"ARGN"` + 포맷 버전(u16 LE) + 저장 시각(u64 LE, 유닉스 초) + ROM 전역 체크섬(u16 LE)
//! + 제목 길이(u8) + 제목(UTF-8) + 본문(MessagePack, 필드 이름 포함).
//!
//! 본문은 필드 이름을 함께 저장하므로 다음 버전에서 필드가 늘어도 `#[serde(default)]`로 이전 스테이트를
//! 읽을 수 있다. ROM 데이터는 넣지 않는다.

// Task 2에서 GameBoy가 쓰기 전까지는 테스트에서만 쓴다.
#![allow(dead_code)]

use std::fmt;

pub const MAGIC: &[u8; 4] = b"ARGN";
/// 본문 구조가 호환되지 않게 바뀔 때만 올린다.
pub const FORMAT_VERSION: u16 = 1;

/// 스테이트를 불러올 수 없는 이유. 불러오기에 실패하면 지금 상태는 바뀌지 않는다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateError {
    /// 세이브 스테이트 파일이 아니다.
    NotAState,
    /// 이 버전이 읽지 못하는 포맷 버전이다.
    UnsupportedVersion(u16),
    /// 다른 게임의 스테이트다.
    WrongRom { title: String },
    /// 형식은 맞지만 내용이 깨졌다.
    Corrupt(String),
}

impl fmt::Display for StateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StateError::NotAState => write!(f, "세이브 스테이트 파일이 아닙니다"),
            StateError::UnsupportedVersion(v) => {
                write!(f, "지원하지 않는 스테이트 형식입니다 (버전 {v})")
            }
            StateError::WrongRom { title } => write!(f, "다른 게임({title})의 스테이트입니다"),
            StateError::Corrupt(why) => write!(f, "스테이트 파일이 손상되었습니다: {why}"),
        }
    }
}

impl std::error::Error for StateError {}

/// 스테이트가 어느 ROM의 것인지 가리는 값: 헤더의 전역 체크섬과 제목.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RomId {
    pub global_checksum: u16,
    pub title: String,
}

/// 헤더를 붙여 스테이트 파일 내용을 만든다.
pub(crate) fn encode<T: serde::Serialize>(rom: &RomId, saved_at: u64, body: &T) -> Vec<u8> {
    let title = rom.title.as_bytes();
    let title = &title[..title.len().min(255)];
    let mut out = Vec::with_capacity(64 * 1024);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&saved_at.to_le_bytes());
    out.extend_from_slice(&rom.global_checksum.to_le_bytes());
    out.push(title.len() as u8);
    out.extend_from_slice(title);
    rmp_serde::encode::write_named(&mut out, body).expect("메모리 버퍼 쓰기는 실패하지 않는다");
    out
}

/// 헤더를 확인하고 `(저장 시각, 본문 바이트)`를 돌려준다.
pub(crate) fn split<'a>(data: &'a [u8], rom: &RomId) -> Result<(u64, &'a [u8]), StateError> {
    let header = peek(data)?;
    if header.rom != *rom {
        return Err(StateError::WrongRom {
            title: header.rom.title,
        });
    }
    Ok((header.saved_at, &data[header.body_offset..]))
}

/// 본문을 읽는다.
pub(crate) fn decode<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T, StateError> {
    rmp_serde::from_slice(body).map_err(|e| StateError::Corrupt(e.to_string()))
}

/// 스테이트 파일 헤더.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateHeader {
    pub saved_at: u64,
    pub rom: RomId,
    body_offset: usize,
}

/// 본문을 풀지 않고 헤더만 읽는다 (슬롯 목록 표시용).
pub fn peek(data: &[u8]) -> Result<StateHeader, StateError> {
    if data.len() < 4 || &data[..4] != MAGIC {
        return Err(StateError::NotAState);
    }
    let fixed = 4 + 2 + 8 + 2 + 1;
    if data.len() < fixed {
        return Err(StateError::Corrupt("헤더가 잘렸습니다".into()));
    }
    let version = u16::from_le_bytes([data[4], data[5]]);
    if version != FORMAT_VERSION {
        return Err(StateError::UnsupportedVersion(version));
    }
    let saved_at = u64::from_le_bytes(data[6..14].try_into().unwrap());
    let global_checksum = u16::from_le_bytes([data[14], data[15]]);
    let title_len = usize::from(data[16]);
    let Some(title) = data.get(fixed..fixed + title_len) else {
        return Err(StateError::Corrupt("헤더가 잘렸습니다".into()));
    };
    Ok(StateHeader {
        saved_at,
        rom: RomId {
            global_checksum,
            title: String::from_utf8_lossy(title).into_owned(),
        },
        body_offset: fixed + title_len,
    })
}

/// 큰 바이트 배열(`[u8; N]`, `Box<[u8; N]>`)을 MessagePack bin으로 저장한다. serde는 33개 이상 원소의
/// 배열을 지원하지 않는다. 길이가 다르면 손상으로 본다.
pub(crate) mod bytes {
    use serde::{Deserializer, Serializer, de::Error};

    pub fn serialize<S: Serializer, const N: usize>(v: &[u8; N], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(v)
    }

    pub fn deserialize<'de, D: Deserializer<'de>, const N: usize>(
        d: D,
    ) -> Result<[u8; N], D::Error> {
        let v: serde_bytes_buf::Buf = serde::Deserialize::deserialize(d)?;
        v.0.try_into()
            .map_err(|v: Vec<u8>| D::Error::invalid_length(v.len(), &"정해진 길이의 바이트 배열"))
    }

    pub mod boxed {
        use serde::{Deserializer, Serializer};

        #[allow(clippy::borrowed_box)] // serde `with`는 필드 타입(`Box<[u8; N]>`)의 참조를 넘긴다
        pub fn serialize<S: Serializer, const N: usize>(
            v: &Box<[u8; N]>,
            s: S,
        ) -> Result<S::Ok, S::Error> {
            super::serialize(v, s)
        }

        pub fn deserialize<'de, D: Deserializer<'de>, const N: usize>(
            d: D,
        ) -> Result<Box<[u8; N]>, D::Error> {
            super::deserialize(d).map(Box::new)
        }
    }

    /// bin과 정수 배열 둘 다 받는 바이트 버퍼.
    pub mod serde_bytes_buf {
        use serde::de::{Deserialize, Deserializer, SeqAccess, Visitor};
        use std::fmt;

        pub struct Buf(pub Vec<u8>);

        impl<'de> Deserialize<'de> for Buf {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                struct V;
                impl<'de> Visitor<'de> for V {
                    type Value = Buf;
                    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                        f.write_str("바이트 배열")
                    }
                    fn visit_bytes<E>(self, v: &[u8]) -> Result<Buf, E> {
                        Ok(Buf(v.to_vec()))
                    }
                    fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Buf, E> {
                        Ok(Buf(v))
                    }
                    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Buf, A::Error> {
                        let mut out = Vec::new();
                        while let Some(b) = seq.next_element()? {
                            out.push(b);
                        }
                        Ok(Buf(out))
                    }
                }
                d.deserialize_bytes(V)
            }
        }
    }
}

/// 화면 버퍼(`Box<[u32; N]>`)를 리틀 엔디언 bin으로 저장한다.
pub(crate) mod words {
    use serde::{Deserializer, Serializer, de::Error};

    #[allow(clippy::borrowed_box)] // serde `with`는 필드 타입(`Box<[u32; N]>`)의 참조를 넘긴다
    pub fn serialize<S: Serializer, const N: usize>(
        v: &Box<[u32; N]>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        let bytes: Vec<u8> = v.iter().flat_map(|w| w.to_le_bytes()).collect();
        s.serialize_bytes(&bytes)
    }

    pub fn deserialize<'de, D: Deserializer<'de>, const N: usize>(
        d: D,
    ) -> Result<Box<[u32; N]>, D::Error> {
        let buf: super::bytes::serde_bytes_buf::Buf = serde::Deserialize::deserialize(d)?;
        if buf.0.len() != N * 4 {
            return Err(D::Error::invalid_length(
                buf.0.len(),
                &"화면 크기 × 4바이트",
            ));
        }
        let mut out = Box::new([0u32; N]);
        let (chunks, _) = buf.0.as_chunks::<4>();
        for (w, chunk) in out.iter_mut().zip(chunks) {
            *w = u32::from_le_bytes(*chunk);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    fn rom() -> RomId {
        RomId {
            global_checksum: 0xBEEF,
            title: "POKEMON CRYSTAL".into(),
        }
    }

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct Big {
        #[serde(with = "bytes")]
        hram: [u8; 0x7F],
        #[serde(with = "bytes::boxed")]
        wram: Box<[u8; 0x8000]>,
        #[serde(with = "words")]
        screen: Box<[u32; 6]>,
    }

    fn big() -> Big {
        let mut wram = Box::new([0u8; 0x8000]);
        wram[0x7FFF] = 9;
        Big {
            hram: [7; 0x7F],
            wram,
            screen: Box::new([1, 2, 3, 0xFFFF_FFFF, 5, 6]),
        }
    }

    #[test]
    fn round_trips_header_and_body() {
        let data = encode(&rom(), 1_700_000_000, &big());
        assert_eq!(&data[..4], MAGIC);
        let header = peek(&data).unwrap();
        assert_eq!(header.saved_at, 1_700_000_000);
        assert_eq!(header.rom, rom());
        let (saved_at, body) = split(&data, &rom()).unwrap();
        assert_eq!(saved_at, 1_700_000_000);
        assert_eq!(decode::<Big>(body).unwrap(), big());
    }

    #[test]
    fn big_arrays_are_stored_as_bytes() {
        let data = encode(&rom(), 0, &big());
        assert!(data.len() < 0x8000 + 0x7F + 24 + 200, "{}", data.len());
    }

    #[test]
    fn rejects_other_files_versions_and_roms() {
        assert_eq!(peek(b"").unwrap_err(), StateError::NotAState);
        assert_eq!(peek(b"PNG\x00....").unwrap_err(), StateError::NotAState);
        let mut data = encode(&rom(), 0, &big());
        let other = RomId {
            global_checksum: 0x1234,
            title: "POKEMON GOLD".into(),
        };
        assert_eq!(
            split(&data, &other).unwrap_err(),
            StateError::WrongRom {
                title: "POKEMON CRYSTAL".into()
            }
        );
        data[4] = 2;
        assert_eq!(peek(&data).unwrap_err(), StateError::UnsupportedVersion(2));
    }

    #[test]
    fn truncated_or_garbled_data_is_corrupt_not_panic() {
        let data = encode(&rom(), 0, &big());
        for len in [5, 10, 16, 20, 40, data.len() - 1] {
            let cut = &data[..len];
            let result = split(cut, &rom()).and_then(|(_, body)| decode::<Big>(body));
            assert!(matches!(result, Err(StateError::Corrupt(_))), "{len}");
        }
    }

    #[test]
    fn wrong_array_length_is_corrupt() {
        #[derive(Serialize)]
        struct Short {
            #[serde(with = "serde_bytes_short")]
            hram: Vec<u8>,
            wram: (),
            screen: (),
        }
        mod serde_bytes_short {
            pub fn serialize<S: serde::Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
                s.serialize_bytes(v)
            }
        }
        let data = encode(
            &rom(),
            0,
            &Short {
                hram: vec![1; 3],
                wram: (),
                screen: (),
            },
        );
        let (_, body) = split(&data, &rom()).unwrap();
        assert!(matches!(decode::<Big>(body), Err(StateError::Corrupt(_))));
    }
}
