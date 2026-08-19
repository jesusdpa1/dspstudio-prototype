//! IIR filter design API and backward-compatible `SosFilter` wrapper.
//!
//! Use [`FilterDesign`] for new code — it holds SOS coefficients in f64 and
//! delegates to the DF2T biquad engine in [`super::biquad`].
//!
//! The [`SosFilter`] type is retained for compatibility with existing call
//! sites that pass pre-computed SciPy SOS rows.
//!
//! ## Design pipeline
//!   analog prototype (ZPK) → frequency transform → bilinear → SOS
//!   (all implemented in [`super::design`])

use rayon::prelude::*;

use super::biquad::{sosfilt, sosfilt_inplace, sosfiltfilt, sosfilt_from_f32, sosfiltfilt_from_f32, settling_npad};
use super::design::{
    FilterResponse, bessel_analog, butter_analog, cheby1_analog, cheby2_analog, design_sos,
};

// Re-export so `dsp_core::filter::iir::SosSection` keeps working.
pub use super::design::SosSection;

// ── FilterDesign ──────────────────────────────────────────────────────────────

/// A fully-designed digital IIR filter in SOS form (f64 coefficients).
///
/// Cheap to clone. Reuse across channels to avoid redundant coefficient computation.
#[derive(Debug, Clone)]
pub struct FilterDesign {
    pub sos: Vec<SosSection>,
    pub order: usize,
    pub label: String,
}

impl FilterDesign {
    fn new(sos: Vec<SosSection>, order: usize, label: impl Into<String>) -> Self {
        Self { sos, order, label: label.into() }
    }

    /// Construct a `FilterDesign` from pre-computed SOS sections (e.g. from
    /// scipy).  `order` is set to the number of sections; `label` is empty.
    pub fn from_sections(sections: Vec<SosSection>) -> Self {
        let order = sections.len();
        Self::new(sections, order, String::new())
    }

    /// Causal (one-pass) filtering. Output is phase-shifted relative to input.
    pub fn apply(&self, data: &[f64]) -> Vec<f64> {
        sosfilt(data, &self.sos)
    }

    /// Zero-phase (forward-backward) filtering. No phase distortion; effective
    /// order is double.
    pub fn apply_filtfilt(&self, data: &[f64]) -> Vec<f64> {
        sosfiltfilt(data, &self.sos)
    }

    /// In-place causal filtering (avoids output allocation).
    pub fn apply_inplace(&self, data: &mut [f64]) {
        sosfilt_inplace(data, &self.sos);
    }

    /// Causal f32 filter. Two allocations: one f64 working buffer, one f32 output.
    pub fn apply_f32(&self, data: &[f32]) -> Vec<f32> {
        sosfilt_from_f32(data, &self.sos)
    }

    /// Zero-phase f32 filter. Two allocations: one f64 padded buffer, one f32 output.
    pub fn apply_filtfilt_f32(&self, data: &[f32]) -> Vec<f32> {
        sosfiltfilt_from_f32(data, &self.sos)
    }

    /// Apply to all channels in a channel-major flat f32 buffer (Rayon-parallel).
    pub fn filter_channels_flat(
        &self,
        data: &[f32],
        n_channels: usize,
        filtfilt: bool,
    ) -> Vec<f32> {
        let n_samples = data.len() / n_channels;
        let mut out = vec![0.0_f32; data.len()];
        out.par_chunks_mut(n_samples).enumerate().for_each(|(c, dst)| {
            let src = &data[c * n_samples..(c + 1) * n_samples];
            let filtered = if filtfilt {
                sosfiltfilt_from_f32(src, &self.sos)
            } else {
                sosfilt_from_f32(src, &self.sos)
            };
            dst.copy_from_slice(&filtered);
        });
        out
    }

    pub fn recommended_surplus(&self) -> usize {
        settling_npad(&self.sos)
    }
}

// ── Design functions ──────────────────────────────────────────────────────────

/// Design a Butterworth IIR filter.
///
/// Maximally flat passband, monotonically decreasing stopband.
pub fn butterworth(order: usize, response: FilterResponse, fs: f64) -> FilterDesign {
    let proto = butter_analog(order);
    let sos = design_sos(proto, response, fs);
    FilterDesign::new(sos, order, format!("Butterworth order={order} {response:?}"))
}

/// Design a Chebyshev Type I IIR filter.
///
/// Equal passband ripple `ripple_db` (dB peak-to-peak); steeper roll-off
/// than Butterworth for the same order.
pub fn chebyshev1(order: usize, ripple_db: f64, response: FilterResponse, fs: f64) -> FilterDesign {
    let proto = cheby1_analog(order, ripple_db);
    let sos = design_sos(proto, response, fs);
    FilterDesign::new(sos, order, format!("Chebyshev-I order={order} ripple={ripple_db}dB {response:?}"))
}

/// Design a Chebyshev Type II IIR filter.
///
/// Monotone passband; equal stopband ripple `atten_db` (minimum stopband attenuation).
pub fn chebyshev2(order: usize, atten_db: f64, response: FilterResponse, fs: f64) -> FilterDesign {
    let proto = cheby2_analog(order, atten_db);
    let sos = design_sos(proto, response, fs);
    FilterDesign::new(sos, order, format!("Chebyshev-II order={order} atten={atten_db}dB {response:?}"))
}

/// Design a Bessel IIR filter (orders 1–8).
///
/// Maximally flat group delay (linear phase in the passband) — best for
/// preserving transient waveshape.
///
/// # Panics
/// Panics if `order` is not in 1..=8.
pub fn bessel(order: usize, response: FilterResponse, fs: f64) -> FilterDesign {
    let proto = bessel_analog(order);
    let sos = design_sos(proto, response, fs);
    FilterDesign::new(sos, order, format!("Bessel order={order} {response:?}"))
}

/// Single-frequency notch filter (2nd-order IIR).
///
/// Places zeros on the unit circle at `freq_hz`; `q` controls bandwidth
/// (`bw = freq_hz / q`). Higher Q → narrower notch.
pub fn notch(freq_hz: f64, q: f64, fs: f64) -> FilterDesign {
    use std::f64::consts::PI;
    let w0 = 2.0 * PI * freq_hz / fs;
    let bw = w0 / q;
    let a0_raw = 1.0 + bw / 2.0;
    let b_norm = 1.0 / a0_raw;
    let sos = vec![SosSection::second_order(
        b_norm,
        -2.0 * w0.cos() * b_norm,
        b_norm,
        -2.0 * w0.cos() / a0_raw,
        (1.0 - bw / 2.0) / a0_raw,
    )];
    FilterDesign::new(sos, 2, format!("Notch f={freq_hz}Hz Q={q}"))
}

/// Single-frequency peak (parametric EQ) filter.
///
/// Boosts or cuts by `gain_db` around `freq_hz` with bandwidth `freq_hz / q`.
pub fn peak_eq(freq_hz: f64, q: f64, gain_db: f64, fs: f64) -> FilterDesign {
    use std::f64::consts::PI;
    let w0 = 2.0 * PI * freq_hz / fs;
    let a_gain = 10.0_f64.powf(gain_db / 40.0);
    let bw = w0 / (2.0 * q);
    let a0r = 1.0 + bw / a_gain;
    let bsc = 1.0 / a0r;
    let sos = vec![SosSection::second_order(
        (1.0 + a_gain * bw) * bsc,
        -2.0 * w0.cos() * bsc,
        (1.0 - a_gain * bw) * bsc,
        -2.0 * w0.cos() / a0r,
        (1.0 - bw / a_gain) / a0r,
    )];
    FilterDesign::new(sos, 2, format!("PeakEQ f={freq_hz}Hz Q={q} gain={gain_db}dB"))
}

// ── Backward-compat SosFilter ─────────────────────────────────────────────────

/// Backward-compatible cascade of SOS biquads operating on f32 data.
///
/// Existing code that passes pre-computed scipy SOS rows can keep using this.
/// Internally converts to f64 via the DF2T biquad engine.
#[deprecated(since = "0.2.0", note = "Use `FilterDesign` instead.")]
#[derive(Debug, Clone)]
pub struct SosFilter {
    inner: FilterDesign,
}

#[allow(deprecated)]
impl SosFilter {
    /// Construct from a `Vec<SosSection>` (already normalized f64 sections).
    pub fn new(sections: Vec<SosSection>) -> Self {
        let order = sections.len();
        let inner = FilterDesign::new(sections, order, String::new());
        Self { inner }
    }

    /// Construct from a slice of `[b0, b1, b2, a0, a1, a2]` rows.
    pub fn from_rows(rows: &[[f32; 6]]) -> Self {
        let sections = rows.iter().map(|&r| SosSection::from_row(r)).collect();
        Self::new(sections)
    }

    pub fn recommended_surplus(&self) -> usize {
        self.inner.recommended_surplus()
    }

    /// Single-pass causal IIR filter (f32 I/O).
    pub fn sosfilt(&self, input: &[f32]) -> Vec<f32> {
        self.inner.apply_f32(input)
    }

    /// Zero-phase forward-backward filter (f32 I/O).
    pub fn sosfiltfilt(&self, input: &[f32]) -> Vec<f32> {
        self.inner.apply_filtfilt_f32(input)
    }

    /// Apply to all channels in a channel-major flat f32 buffer (Rayon-parallel).
    pub fn filter_channels(&self, data: &[f32], n_channels: usize, filtfilt: bool) -> Vec<f32> {
        self.inner.filter_channels_flat(data, n_channels, filtfilt)
    }
}

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::*;

    fn identity() -> SosFilter {
        SosFilter::from_rows(&[[1.0, 0.0, 0.0, 1.0, 0.0, 0.0]])
    }

    #[test]
    fn identity_passthrough() {
        let f = identity();
        let input: Vec<f32> = (0..8).map(|i| i as f32).collect();
        assert_eq!(f.sosfilt(&input), input);
    }

    #[test]
    fn filtfilt_identity_passthrough() {
        let f = identity();
        let input: Vec<f32> = (0..8).map(|i| i as f32).collect();
        let out = f.sosfiltfilt(&input);
        for (a, b) in input.iter().zip(out.iter()) {
            assert!((a - b).abs() < 1e-5, "{a} vs {b}");
        }
    }

    #[test]
    fn lowpass_attenuates_high_frequency() {
        let sos = SosFilter::from_rows(&[[
            0.06745527, 0.13491055, 0.06745527,
            1.0, -1.14298051, 0.41280160,
        ]]);
        let dc = vec![1.0_f32; 200];
        let out = sos.sosfilt(&dc);
        let mean: f32 = out[100..].iter().sum::<f32>() / 100.0;
        assert!((mean - 1.0).abs() < 0.02, "DC gain = {mean}");
    }

    #[test]
    fn butterworth_lp_dc_gain() {
        let f = butterworth(2, FilterResponse::LowPass { cutoff: 100.0 }, 1000.0);
        let dc: Vec<f32> = vec![1.0_f32; 300];
        let out = f.apply_f32(&dc);
        let mean: f32 = out[200..].iter().sum::<f32>() / 100.0;
        assert!((mean - 1.0).abs() < 0.02, "Butterworth DC gain = {mean}");
    }

    #[test]
    fn recommended_surplus_scales_with_sections() {
        let f = SosFilter::from_rows(&[
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        ]);
        assert_eq!(f.recommended_surplus(), 18);
    }
}
