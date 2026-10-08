//! M-사이클마다 나온 스테레오 값을 출력 샘플레이트로 평균 다운샘플링하고 DC를 제거한다.

/// 1초에 진행하는 M-사이클 수.
const T_CYCLES_PER_SEC: f64 = 4_194_304.0;
pub const DEFAULT_SAMPLE_RATE: f64 = 48_000.0;
/// 내보내지 않은 샘플이 이만큼(48 kHz에서 1초) 쌓이면 모두 버린다. 프론트엔드가 소리를 가져가지 않아도
/// (헤드리스 테스트) 메모리가 늘지 않게 한다.
const MAX_PENDING: usize = 48_000 * 2;

#[derive(Debug, Clone)]
pub struct Mixer {
    sample_rate: f64,
    /// 다음 출력 샘플까지 쌓인 위상(× M-사이클 1초).
    phase: f64,
    sum: (f32, f32),
    count: u32,
    /// 하이패스 필터 축전기 전압 (gbdev wiki "Game Boy Sound Hardware" Obscure Behavior).
    capacitor: (f32, f32),
    charge: f32,
    out: Vec<f32>,
}

impl Default for Mixer {
    fn default() -> Self {
        let mut mixer = Self {
            sample_rate: DEFAULT_SAMPLE_RATE,
            phase: 0.0,
            sum: (0.0, 0.0),
            count: 0,
            capacitor: (0.0, 0.0),
            charge: 0.0,
            out: Vec::new(),
        };
        mixer.set_sample_rate(DEFAULT_SAMPLE_RATE);
        mixer
    }
}

impl Mixer {
    /// 출력 샘플레이트(Hz). 프론트엔드가 오디오 버퍼 상태에 따라 조금씩 바꾼다.
    pub fn set_sample_rate(&mut self, rate: f64) {
        let rate = rate.clamp(8_000.0, 192_000.0);
        self.sample_rate = rate;
        // 실제 기기의 축전기는 T-사이클마다 0.999958배로 방전된다.
        self.charge = 0.999958f64.powf(T_CYCLES_PER_SEC / rate) as f32;
    }

    /// `t_cycles` T-사이클 동안 유지된 왼쪽·오른쪽 값(-1.0–1.0). 보통 4이고 CGB 2배속에서는 2다.
    pub fn push(&mut self, left: f32, right: f32, t_cycles: u32) {
        let weight = t_cycles as f32;
        self.sum.0 += left * weight;
        self.sum.1 += right * weight;
        self.count += t_cycles;
        self.phase += self.sample_rate * f64::from(t_cycles);
        if self.phase < T_CYCLES_PER_SEC {
            return;
        }
        self.phase -= T_CYCLES_PER_SEC;
        let n = self.count as f32;
        let left = self.high_pass(self.sum.0 / n, 0);
        let right = self.high_pass(self.sum.1 / n, 1);
        self.sum = (0.0, 0.0);
        self.count = 0;
        if self.out.len() >= MAX_PENDING {
            self.out.clear();
        }
        self.out.push(left);
        self.out.push(right);
    }

    fn high_pass(&mut self, input: f32, side: usize) -> f32 {
        let capacitor = if side == 0 {
            &mut self.capacitor.0
        } else {
            &mut self.capacitor.1
        };
        let output = input - *capacitor;
        *capacitor = input - output * self.charge;
        output
    }

    /// 쌓인 인터리브 스테레오 샘플을 `out` 뒤에 붙인다.
    pub fn drain(&mut self, out: &mut Vec<f32>) {
        out.append(&mut self.out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const M_CYCLES_PER_SEC: f64 = T_CYCLES_PER_SEC / 4.0;

    #[test]
    fn one_second_produces_sample_rate_frames() {
        let mut mixer = Mixer::default();
        mixer.set_sample_rate(44_100.0);
        for _ in 0..M_CYCLES_PER_SEC as u32 {
            mixer.push(0.0, 0.0, 4);
        }
        let mut out = Vec::new();
        mixer.drain(&mut out);
        assert_eq!(out.len(), 44_100 * 2);
        mixer.drain(&mut out);
        assert_eq!(out.len(), 44_100 * 2, "꺼낸 샘플은 다시 나오지 않는다");
    }

    #[test]
    fn constant_input_decays_to_zero() {
        let mut mixer = Mixer::default();
        for _ in 0..M_CYCLES_PER_SEC as u32 {
            mixer.push(0.5, -0.5, 4);
        }
        let mut out = Vec::new();
        mixer.drain(&mut out);
        assert!(out[0] > 0.4 && out[1] < -0.4, "처음에는 그대로 나온다");
        let (l, r) = (out[out.len() - 2], out[out.len() - 1]);
        assert!(l.abs() < 0.01 && r.abs() < 0.01, "DC는 사라진다: {l}, {r}");
    }

    #[test]
    fn undrained_samples_are_capped() {
        let mut mixer = Mixer::default();
        for _ in 0..3 * M_CYCLES_PER_SEC as u32 {
            mixer.push(0.0, 0.0, 4);
        }
        let mut out = Vec::new();
        mixer.drain(&mut out);
        assert!(!out.is_empty() && out.len() <= MAX_PENDING);
    }

    #[test]
    fn half_length_pushes_produce_the_same_rate() {
        let mut mixer = Mixer::default();
        mixer.set_sample_rate(44_100.0);
        for _ in 0..2 * M_CYCLES_PER_SEC as u32 {
            mixer.push(0.0, 0.0, 2);
        }
        let mut out = Vec::new();
        mixer.drain(&mut out);
        assert_eq!(out.len(), 44_100 * 2);
    }
}
