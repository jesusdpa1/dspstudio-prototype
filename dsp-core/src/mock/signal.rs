//! Deterministic signal generators for ground-truth testing and simulation.
//!
//! All generators implement [`SignalGenerator`] and write into a flat
//! channel-major buffer with layout `[Channels × Samples]`.

/// Fills a channel-major buffer with signal data.
///
/// The buffer layout is `[ch0_s0, ch0_s1, …, ch1_s0, ch1_s1, …]`.
/// Each call to `fill_buffer` advances the generator's internal phase so
/// successive calls produce a continuous stream.
pub trait SignalGenerator {
    fn fill_buffer(&mut self, buffer: &mut [f32], channels: usize);
}

/// A phase-continuous sine wave generator.
///
/// Phase accumulates across successive `fill_buffer` calls, making it
/// suitable for streaming use without audible discontinuities.
pub struct SineWave {
    pub frequency: f32,
    pub sample_rate: f32,
    pub amplitude: f32,
    phase: f64,
}

impl SineWave {
    /// Creates a new sine wave generator.
    ///
    /// # Arguments
    /// * `frequency` — frequency in Hz.
    /// * `sample_rate` — sample rate in Hz.
    /// * `amplitude` — peak amplitude.
    pub fn new(frequency: f32, sample_rate: f32, amplitude: f32) -> Self {
        Self { frequency, sample_rate, amplitude, phase: 0.0 }
    }
}

impl SignalGenerator for SineWave {
    fn fill_buffer(&mut self, buffer: &mut [f32], channels: usize) {
        let samples = buffer.len() / channels;
        let phase_inc = 2.0 * std::f64::consts::PI * self.frequency as f64 / self.sample_rate as f64;
        for s in 0..samples {
            let val = (self.phase.sin() * self.amplitude as f64) as f32;
            for c in 0..channels {
                buffer[c * samples + s] = val;
            }
            self.phase = (self.phase + phase_inc) % (2.0 * std::f64::consts::PI);
        }
    }
}

/// Deterministic white noise via a 32-bit Xorshift PRNG.
///
/// Using Xorshift instead of `rand` keeps output identical across platforms
/// and compiler versions, which is essential for reproducible test vectors.
pub struct WhiteNoise {
    state: u32,
    pub amplitude: f32,
}

impl WhiteNoise {
    /// Creates a new white noise generator with a fixed seed.
    ///
    /// Two generators created with the same `seed` will produce identical output.
    pub fn new(seed: u32, amplitude: f32) -> Self {
        Self { state: seed, amplitude }
    }

    fn next_f32(&mut self) -> f32 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 17;
        self.state ^= self.state << 5;
        (self.state as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

impl SignalGenerator for WhiteNoise {
    fn fill_buffer(&mut self, buffer: &mut [f32], _channels: usize) {
        for val in buffer.iter_mut() {
            *val = self.next_f32() * self.amplitude;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sine_wave_is_deterministic() {
        let mut a = SineWave::new(440.0, 44100.0, 1.0);
        let mut b = SineWave::new(440.0, 44100.0, 1.0);
        let mut buf_a = vec![0.0f32; 64];
        let mut buf_b = vec![0.0f32; 64];
        a.fill_buffer(&mut buf_a, 1);
        b.fill_buffer(&mut buf_b, 1);
        assert_eq!(buf_a, buf_b);
    }

    #[test]
    fn sine_wave_amplitude_bounded() {
        let mut g = SineWave::new(1000.0, 40000.0, 1.5);
        let mut buf = vec![0.0f32; 400];
        g.fill_buffer(&mut buf, 1);
        for &v in &buf {
            assert!(v <= 1.5 + 1e-5 && v >= -1.5 - 1e-5, "sample {} out of range", v);
        }
    }

    #[test]
    fn sine_wave_multichannel_identical_rows() {
        // All channels should carry the same signal.
        let mut g = SineWave::new(440.0, 44100.0, 1.0);
        let channels = 4;
        let samples = 32;
        let mut buf = vec![0.0f32; channels * samples];
        g.fill_buffer(&mut buf, channels);
        for s in 0..samples {
            let ref_val = buf[s];
            for c in 1..channels {
                assert_eq!(buf[c * samples + s], ref_val);
            }
        }
    }

    #[test]
    fn sine_phase_continuous_across_calls() {
        // Two consecutive fill_buffer calls should produce the same signal
        // as a single call of double length.
        let mut gen_split = SineWave::new(100.0, 1000.0, 1.0);
        let mut gen_full  = SineWave::new(100.0, 1000.0, 1.0);

        let mut half1 = vec![0.0f32; 32];
        let mut half2 = vec![0.0f32; 32];
        let mut full  = vec![0.0f32; 64];

        gen_split.fill_buffer(&mut half1, 1);
        gen_split.fill_buffer(&mut half2, 1);
        gen_full.fill_buffer(&mut full, 1);

        let combined: Vec<f32> = half1.iter().chain(half2.iter()).copied().collect();
        for (a, b) in combined.iter().zip(full.iter()) {
            assert!((a - b).abs() < 1e-6, "phase discontinuity: {} vs {}", a, b);
        }
    }

    #[test]
    fn white_noise_same_seed_reproducible() {
        let mut n1 = WhiteNoise::new(42, 1.0);
        let mut n2 = WhiteNoise::new(42, 1.0);
        let mut b1 = vec![0.0f32; 256];
        let mut b2 = vec![0.0f32; 256];
        n1.fill_buffer(&mut b1, 1);
        n2.fill_buffer(&mut b2, 1);
        assert_eq!(b1, b2);
    }

    #[test]
    fn white_noise_different_seeds_differ() {
        let mut n1 = WhiteNoise::new(1, 1.0);
        let mut n2 = WhiteNoise::new(2, 1.0);
        let mut b1 = vec![0.0f32; 64];
        let mut b2 = vec![0.0f32; 64];
        n1.fill_buffer(&mut b1, 1);
        n2.fill_buffer(&mut b2, 1);
        assert_ne!(b1, b2);
    }

    #[test]
    fn white_noise_amplitude_bounded() {
        let mut g = WhiteNoise::new(999, 0.5);
        let mut buf = vec![0.0f32; 1024];
        g.fill_buffer(&mut buf, 1);
        for &v in &buf {
            assert!(v.abs() <= 0.5 + 1e-5, "sample {} exceeds amplitude", v);
        }
    }
}
