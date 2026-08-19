//! Deterministic neural spike emulator for ground-truth testing.
//!
//! `SpikeTemplate` models an extracellular action potential waveform.
//! `NeuralUnit` fires at a Poisson-distributed rate and injects template
//! copies (additive superposition) into a pre-existing noise buffer.
//!
//! Both types use seeded Xorshift32 PRNGs so output is fully reproducible.
//! The sample counter advances by exactly `buffer.len()` per `fill_buffer`
//! call — no drift across chunk boundaries.

/// A synthetic extracellular action potential waveform.
///
/// The waveform is normalized so its negative peak equals exactly -1.0.
/// Multiply by the unit's amplitude at injection time.
pub struct SpikeTemplate {
    pub waveform: Vec<f32>,
}

impl SpikeTemplate {
    /// Builds a physiologically-plausible biphasic extracellular AP waveform.
    ///
    /// Duration ≈ 2 ms.  The negative phase (depolarization) peaks near 0.6 ms
    /// and is followed by a positive overshoot (repolarization) near 1.3 ms with
    /// ~35% relative amplitude, matching typical tetrode/Neuropixels recordings.
    pub fn new_biphasic(sample_rate: f32) -> Self {
        let n_samples = ((sample_rate * 0.002) as usize).max(4);
        let mut waveform = Vec::with_capacity(n_samples);

        for i in 0..n_samples {
            let t = i as f32 / n_samples as f32;
            // Negative phase: Gaussian centred at t = 0.30, σ = 0.12
            let neg = -((-0.5 * ((t - 0.30) / 0.12).powi(2)).exp());
            // Positive overshoot: Gaussian centred at t = 0.65, σ = 0.18, 35% amplitude
            let pos = 0.35 * (-0.5 * ((t - 0.65) / 0.18).powi(2)).exp();
            waveform.push(neg + pos);
        }

        // Normalise so negative peak = -1.0
        let peak_neg = waveform.iter().cloned().fold(f32::INFINITY, f32::min);
        let scale = 1.0 / peak_neg.abs().max(1e-9);
        for v in waveform.iter_mut() {
            *v *= scale;
        }

        Self { waveform }
    }
}

/// Generates spikes from a single neuron at a Poisson-distributed firing rate.
///
/// Call `fill_buffer` repeatedly with successive chunk buffers.  The unit
/// accumulates an exact sample count so inter-spike intervals remain correct
/// across arbitrary buffer boundaries.
pub struct NeuralUnit {
    template: SpikeTemplate,
    firing_rate_hz: f32,
    pub amplitude: f32,
    next_spike_sample: u64,
    /// Absolute sample index of the start of the *next* `fill_buffer` call.
    pub current_sample: u64,
    prng_state: u32,
}

impl NeuralUnit {
    /// Creates a new unit.
    ///
    /// * `sample_rate` — used both to convert firing rate to samples and to
    ///   schedule the first spike at construction time.
    pub fn new(
        template: SpikeTemplate,
        firing_rate_hz: f32,
        amplitude: f32,
        seed: u32,
        sample_rate: f32,
    ) -> Self {
        let mut unit = Self {
            template,
            firing_rate_hz,
            amplitude,
            next_spike_sample: 0,
            current_sample: 0,
            prng_state: seed.max(1), // Xorshift must not start at 0
        };
        unit.schedule_next_spike(sample_rate);
        unit
    }

    fn xorshift(&mut self) -> u32 {
        self.prng_state ^= self.prng_state << 13;
        self.prng_state ^= self.prng_state >> 17;
        self.prng_state ^= self.prng_state << 5;
        self.prng_state
    }

    fn next_f32(&mut self) -> f32 {
        self.xorshift() as f32 / u32::MAX as f32
    }

    fn schedule_next_spike(&mut self, sample_rate: f32) {
        // Exponential inter-spike interval for a Poisson process.
        let u = self.next_f32().max(1e-7);
        let interval_samples = (-u.ln() / self.firing_rate_hz * sample_rate) as u64;
        self.next_spike_sample = self.current_sample + interval_samples.max(1);
    }

    /// Additively injects spike waveforms into `buffer` (channel-major, 1 channel).
    ///
    /// The buffer is expected to already contain the noise floor; spikes are
    /// superimposed on top.  `self.current_sample` advances by exactly
    /// `buffer.len()` — no drift regardless of how many spikes fire per call.
    pub fn fill_buffer(&mut self, buffer: &mut [f32], sample_rate: f32) {
        let n_samples = buffer.len();
        // Save the start offset once so mid-loop state mutation cannot corrupt it.
        let start_sample = self.current_sample;

        let mut i = 0;
        while i < n_samples {
            let abs_sample = start_sample + i as u64;

            if abs_sample >= self.next_spike_sample {
                let spike_len = self.template.waveform.len();
                for j in 0..spike_len {
                    if i + j < n_samples {
                        buffer[i + j] += self.template.waveform[j] * self.amplitude;
                    }
                }
                // Schedule the *next* spike from this absolute position so
                // the Poisson interval is measured from the current firing
                // time, not the start of the buffer.
                self.current_sample = abs_sample;
                self.schedule_next_spike(sample_rate);
                // Restore current_sample to the buffer start so the outer
                // advance at the end is correct.
                self.current_sample = start_sample;
            }
            i += 1;
        }

        // Advance by exactly the number of samples processed.
        self.current_sample = start_sample + n_samples as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detection::{CrossingDirection, DetectionDetector};
    use crate::detection::single::SingleThresholdDetector;

    #[test]
    fn biphasic_template_shape() {
        let t = SpikeTemplate::new_biphasic(40_000.0);

        let min = t.waveform.iter().cloned().fold(f32::INFINITY, f32::min);
        let max = t.waveform.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

        // Negative peak must be normalised to exactly -1.0.
        assert!((min + 1.0).abs() < 1e-5, "negative peak = {}", min);
        // Positive overshoot ≥ 25% of negative peak magnitude.
        assert!(max >= 0.25, "positive overshoot too small: {}", max);
        // Positive overshoot must not exceed the negative peak.
        assert!(max < 1.0, "positive overshoot exceeds negative peak");
    }

    #[test]
    fn sample_counter_no_drift() {
        let fs = 40_000.0_f32;
        let template = SpikeTemplate::new_biphasic(fs);
        let mut unit = NeuralUnit::new(template, 100.0, 1.0, 1, fs);

        let chunk = 1024_usize;
        let mut buf = vec![0.0_f32; chunk];

        unit.fill_buffer(&mut buf, fs);
        assert_eq!(unit.current_sample, chunk as u64, "after first call");

        unit.fill_buffer(&mut buf, fs);
        assert_eq!(unit.current_sample, 2 * chunk as u64, "after second call");
    }

    #[test]
    fn inject_detect_roundtrip() {
        // 100 Hz neuron into a 2-second, single-channel buffer.
        // Expect ~200 spikes detected; allow generous ±60% for Poisson variance.
        let fs = 40_000.0_f32;
        let n_samples = (fs as usize) * 2;
        let template = SpikeTemplate::new_biphasic(fs);
        let mut unit = NeuralUnit::new(template, 100.0, 1.0, 42, fs);

        let mut buf = vec![0.0_f32; n_samples];
        unit.fill_buffer(&mut buf, fs);

        let detector = SingleThresholdDetector::new(-0.5, CrossingDirection::Negative, 40, 0, 0);
        let events = detector.detect(&buf, 1, 0);

        assert!(events.len() >= 80,  "too few spikes: {}", events.len());
        assert!(events.len() <= 440, "too many spikes: {}", events.len());
    }
}
