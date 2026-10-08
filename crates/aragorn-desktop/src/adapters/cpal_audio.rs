//! cpal 오디오 출력 (`AudioSink` 어댑터). 장치가 없으면 소리 없이 실행하고(스펙 §6.1),
//! 쓰던 장치가 끊기면(헤드폰 분리, 기본 장치 변경) 잠시 뒤 기본 장치를 다시 연다.

use super::sample_queue::SampleQueue;
use aragorn_app::audio::{AudioSink, target_frames};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{ErrorKind, FromSample, SampleFormat, SizedSample};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

/// 끊긴 장치를 다시 열기까지 기다리는 시간.
const REOPEN_DELAY: Duration = Duration::from_secs(3);
/// 콜백 한 번에 요청할 길이(초). 지정하지 않으면 PulseAudio는 2초 단위로 가져가 큐를 한 번에 비운다.
const CALLBACK_SECS: u32 = 50; // 1/50초 = 20ms

/// 열려 있는 출력 스트림 하나.
struct Stream {
    _stream: cpal::Stream,
    queue: Arc<Mutex<SampleQueue>>,
    broken: Arc<AtomicBool>,
    sample_rate: u32,
    /// 장치 버퍼 목표 분량(스테레오 프레임).
    target: usize,
}

impl Stream {
    /// 리눅스는 PulseAudio(PipeWire 포함)를 먼저 쓰고, 안 되면 기본 호스트(ALSA)를 쓴다.
    fn open_default() -> Result<Stream, String> {
        let mut last_error = String::from("오디오 장치가 없습니다");
        for host in hosts() {
            match Stream::open(&host) {
                Ok(stream) => {
                    log::info!("오디오 출력: {:?}, {}Hz", host.id(), stream.sample_rate);
                    return Ok(stream);
                }
                Err(e) => {
                    log::warn!("오디오 출력을 열 수 없습니다 ({:?}): {e}", host.id());
                    last_error = e;
                }
            }
        }
        Err(last_error)
    }

    fn open(host: &cpal::Host) -> Result<Stream, String> {
        let device = host.default_output_device().ok_or("출력 장치가 없습니다")?;
        let supported = device.default_output_config().map_err(|e| e.to_string())?;
        let format = supported.sample_format();
        let sample_rate = supported.sample_rate();
        let fixed = match *supported.buffer_size() {
            cpal::SupportedBufferSize::Range { min, max } => {
                Some((sample_rate / CALLBACK_SECS).clamp(min, max))
            }
            cpal::SupportedBufferSize::Unknown => None,
        };
        let target = target_frames(sample_rate);
        let queue = Arc::new(Mutex::new(SampleQueue::new(target, target * 4)));
        let broken = Arc::new(AtomicBool::new(false));
        let mut config: cpal::StreamConfig = supported.into();
        if let Some(frames) = fixed {
            config.buffer_size = cpal::BufferSize::Fixed(frames);
        }
        let stream = build_for_format(format, &device, &config, &queue, &broken).or_else(|e| {
            // 고정 버퍼 크기를 받지 않는 장치가 있다.
            log::warn!("고정 버퍼로 열 수 없어 기본 버퍼를 씁니다: {e}");
            config.buffer_size = cpal::BufferSize::Default;
            build_for_format(format, &device, &config, &queue, &broken)
        })?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Stream {
            _stream: stream,
            queue,
            broken,
            sample_rate,
            target,
        })
    }

    fn queue(&self) -> std::sync::MutexGuard<'_, SampleQueue> {
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn hosts() -> Vec<cpal::Host> {
    let mut hosts = Vec::new();
    #[cfg(target_os = "linux")]
    match cpal::host_from_id(cpal::HostId::PulseAudio) {
        Ok(host) => hosts.push(host),
        Err(e) => log::info!("PulseAudio를 쓸 수 없습니다: {e}"),
    }
    hosts.push(cpal::default_host());
    hosts
}

fn build_for_format(
    format: SampleFormat,
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    queue: &Arc<Mutex<SampleQueue>>,
    broken: &Arc<AtomicBool>,
) -> Result<cpal::Stream, String> {
    match format {
        SampleFormat::F32 => build::<f32>(device, config, queue, broken),
        SampleFormat::I16 => build::<i16>(device, config, queue, broken),
        SampleFormat::U16 => build::<u16>(device, config, queue, broken),
        SampleFormat::I32 => build::<i32>(device, config, queue, broken),
        other => Err(format!("지원하지 않는 샘플 형식입니다: {other}")),
    }
}

/// 장치 채널 수에 맞춰 스테레오를 나눠 담는다. 모노는 두 채널 평균, 3채널 이상은 앞 두 채널만 쓴다.
fn build<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    queue: &Arc<Mutex<SampleQueue>>,
    broken: &Arc<AtomicBool>,
) -> Result<cpal::Stream, String> {
    let channels = usize::from(config.channels).max(1);
    let queue = Arc::clone(queue);
    let broken = Arc::clone(broken);
    let mut stereo = Vec::new();
    device
        .build_output_stream(
            *config,
            move |data: &mut [T], _| {
                stereo.resize(data.len() / channels * 2, 0.0);
                queue
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .pop_into(&mut stereo);
                for (frame, lr) in data.chunks_mut(channels).zip(stereo.chunks(2)) {
                    for (channel, out) in frame.iter_mut().enumerate() {
                        let value = match (channels, channel) {
                            (1, _) => (lr[0] + lr[1]) * 0.5,
                            (_, 0) => lr[0],
                            (_, 1) => lr[1],
                            _ => 0.0,
                        };
                        *out = T::from_sample(value);
                    }
                }
            },
            move |e| {
                if breaks_stream(e.kind()) {
                    log::warn!("오디오 스트림이 끊겼습니다: {e}");
                    broken.store(true, Ordering::Relaxed);
                } else {
                    log::debug!("오디오: {e}");
                }
            },
            None,
        )
        .map_err(|e| e.to_string())
}

/// 스트림 오류 뒤 장치를 다시 열어야 하는지. 언더런·실시간 우선순위 거절은 일시적이고,
/// `DeviceChanged`는 macOS가 스트림을 새 기본 출력으로 이미 옮긴 뒤 알리는 것이다.
/// Windows는 기본 장치가 바뀌면 `StreamInvalidated`를 보내므로 다시 연다.
fn breaks_stream(kind: ErrorKind) -> bool {
    !matches!(
        kind,
        ErrorKind::Xrun | ErrorKind::RealtimeDenied | ErrorKind::DeviceChanged
    )
}

/// 앱이 쓰는 오디오 출력. 처음에 장치가 없으면 계속 소리 없이 실행한다.
pub struct CpalAudio {
    stream: Option<Stream>,
    /// 끊긴 장치를 다시 열 시각. 처음부터 장치가 없었으면 `None`(다시 시도하지 않는다).
    reopen_at: Option<Instant>,
}

impl CpalAudio {
    pub fn open() -> Self {
        let stream = Stream::open_default()
            .map_err(|e| log::warn!("소리 없이 실행합니다: {e}"))
            .ok();
        Self {
            stream,
            reopen_at: None,
        }
    }

    /// 쓸 수 있는 출력. 끊긴 스트림은 버리고 `REOPEN_DELAY` 뒤에 다시 연다.
    pub fn sink(&mut self) -> Option<&mut dyn AudioSink> {
        if self
            .stream
            .as_ref()
            .is_some_and(|s| s.broken.load(Ordering::Relaxed))
        {
            self.stream = None;
            self.reopen_at = Some(Instant::now() + REOPEN_DELAY);
        }
        if self.stream.is_none() && self.reopen_at.is_some_and(|at| Instant::now() >= at) {
            match Stream::open_default() {
                Ok(stream) => {
                    self.stream = Some(stream);
                    self.reopen_at = None;
                }
                Err(_) => self.reopen_at = Some(Instant::now() + REOPEN_DELAY),
            }
        }
        self.stream.as_mut().map(|s| s as &mut dyn AudioSink)
    }
}

impl AudioSink for Stream {
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn fill_ratio(&self) -> f32 {
        self.queue().queued_frames() as f32 / self.target.max(1) as f32
    }

    fn push(&mut self, samples: &[f32]) {
        self.queue().push(samples);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rerouted_or_glitched_stream_is_kept() {
        // macOS CoreAudio는 기본 출력이 바뀌면 스트림을 이미 옮긴 뒤 DeviceChanged를 알린다.
        for kind in [
            ErrorKind::DeviceChanged,
            ErrorKind::Xrun,
            ErrorKind::RealtimeDenied,
        ] {
            assert!(!breaks_stream(kind), "{kind:?}");
        }
    }

    #[test]
    fn invalidated_stream_is_reopened() {
        for kind in [
            ErrorKind::StreamInvalidated,
            ErrorKind::DeviceNotAvailable,
            ErrorKind::BackendError,
        ] {
            assert!(breaks_stream(kind), "{kind:?}");
        }
    }
}
