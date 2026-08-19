// Biquad (second-order section) filtering in Direct Form II Transposed (DF2T).
//
// Denormal flushing: state variables are clamped to zero when they fall below
// `DENORMAL_THRESHOLD` to prevent the CPU from switching into slow "subnormal"
// mode on long silent passages.

use super::design::SosSection;

const DENORMAL_THRESHOLD: f64 = 1e-30;

// ── Single-section state ───────────────────────────────────────────────────────

/// Runtime state for one SOS section (two delay elements).
#[derive(Debug, Clone, Copy, Default)]
pub struct BiquadState {
    pub w1: f64,
    pub w2: f64,
}

impl BiquadState {
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn flush_denormals(&mut self) {
        if self.w1.abs() < DENORMAL_THRESHOLD { self.w1 = 0.0; }
        if self.w2.abs() < DENORMAL_THRESHOLD { self.w2 = 0.0; }
    }
}

// ── Direct Form II Transposed (DF2T) ──────────────────────────────────────────
//
//   w[n] = x[n] - a1*w1 - a2*w2
//   y[n] = b0*w[n] + b1*w1 + b2*w2
//   shift:  w2 ← w1,  w1 ← w[n]

/// Process a single sample through one SOS section (DF2T), without denormal
/// flushing.  Call `state.flush_denormals()` periodically from the caller.
#[inline]
pub fn process_sample_df2t_no_flush(sample: f64, sec: &SosSection, state: &mut BiquadState) -> f64 {
    let w = sample - sec.a[0] * state.w1 - sec.a[1] * state.w2;
    let y = sec.b[0] * w + sec.b[1] * state.w1 + sec.b[2] * state.w2;
    state.w2 = state.w1;
    state.w1 = w;
    y
}

/// Process a single sample through one SOS section (DF2T) with immediate
/// denormal flushing.  Use only when calling one sample at a time (e.g. in
/// streaming / real-time contexts); prefer `sosfilt_inplace` for bulk data.
#[inline]
pub fn process_sample_df2t(sample: f64, sec: &SosSection, state: &mut BiquadState) -> f64 {
    let y = process_sample_df2t_no_flush(sample, sec, state);
    state.flush_denormals();
    y
}

// ── Direct Form I (DF1) ────────────────────────────────────────────────────────

/// Runtime state for DF1 (four delay elements per section).
#[derive(Debug, Clone, Copy, Default)]
pub struct BiquadStateDF1 {
    pub x1: f64,
    pub x2: f64,
    pub y1: f64,
    pub y2: f64,
}

impl BiquadStateDF1 {
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    fn flush_denormals(&mut self) {
        if self.x1.abs() < DENORMAL_THRESHOLD { self.x1 = 0.0; }
        if self.x2.abs() < DENORMAL_THRESHOLD { self.x2 = 0.0; }
        if self.y1.abs() < DENORMAL_THRESHOLD { self.y1 = 0.0; }
        if self.y2.abs() < DENORMAL_THRESHOLD { self.y2 = 0.0; }
    }
}

/// Process a single sample through one SOS section (DF1).
#[inline]
pub fn process_sample_df1(sample: f64, sec: &SosSection, state: &mut BiquadStateDF1) -> f64 {
    let y = sec.b[0] * sample + sec.b[1] * state.x1 + sec.b[2] * state.x2
        - sec.a[0] * state.y1
        - sec.a[1] * state.y2;
    state.x2 = state.x1;
    state.x1 = sample;
    state.y2 = state.y1;
    state.y1 = y;
    state.flush_denormals();
    y
}

// ── Multi-section cascade (SOS) ───────────────────────────────────────────────

/// Filter a slice through a cascade of SOS sections (DF2T, causal).
pub fn sosfilt(data: &[f64], sos: &[SosSection]) -> Vec<f64> {
    let mut out = data.to_vec();
    sosfilt_inplace(&mut out, sos);
    out
}

/// In-place version of `sosfilt` (avoids output allocation).
///
/// States are stack-allocated for filters up to 16 sections (≤ order-32
/// bandpass); falls back to a heap Vec for unusual high-order designs.
/// Denormal flushing is performed every 8 samples instead of every sample
/// to reduce branch overhead on hot paths.
pub fn sosfilt_inplace(data: &mut [f64], sos: &[SosSection]) {
    let n_sec = sos.len();
    macro_rules! run_filter {
        ($states:expr) => {
            for (i, x) in data.iter_mut().enumerate() {
                let mut s = *x;
                for (sec, st) in sos.iter().zip($states.iter_mut()) {
                    s = process_sample_df2t_no_flush(s, sec, st);
                }
                *x = s;
                if i & 7 == 7 {
                    for st in $states.iter_mut() { st.flush_denormals(); }
                }
            }
        };
    }
    if n_sec <= 16 {
        let mut states = [BiquadState::new(); 16];
        let states = &mut states[..n_sec];
        run_filter!(states);
    } else {
        let mut states = vec![BiquadState::new(); n_sec];
        run_filter!(states);
    }
}

// ── Settling-time helpers ─────────────────────────────────────────────────────

/// Maximum pole radius of one SOS section.
/// For a conjugate pair, a2 = r², so r = sqrt(a2).
/// For a real pair, solve z² + a1·z + a2 = 0 and take the larger |root|.
fn pole_radius(sec: &SosSection) -> f64 {
    let a1 = sec.a[0];
    let a2 = sec.a[1];
    let disc = a1 * a1 - 4.0 * a2;
    if disc >= 0.0 {
        let sd = disc.sqrt();
        ((-a1 + sd) / 2.0).abs().max(((-a1 - sd) / 2.0).abs())
    } else {
        a2.abs().sqrt()
    }
}

/// Padding length required so filter transients settle to < −60 dB at each
/// edge of a `sosfiltfilt` call.
///
/// `T_settle ≈ ⌈−3 / ln(r_max)⌉` samples, floored at `6 * n_sections` so
/// non-resonant filters (where r_max ≈ 0) still get a reasonable minimum.
pub(crate) fn settling_npad(sos: &[SosSection]) -> usize {
    let r_max = sos.iter().map(pole_radius).fold(0.0_f64, f64::max);
    let min_pad = 6 * sos.len().max(1);
    if r_max < 1e-6 || r_max >= 1.0 {
        return min_pad;
    }
    let settling = (-3.0 / r_max.ln()).ceil() as usize;
    settling.max(min_pad)
}

// ── Zero-phase filtering (sosfiltfilt) ────────────────────────────────────────
//
// Algorithm:
//   1. Forward pass  → intermediate signal
//   2. Reverse in place
//   3. Backward pass → time-reversed output
//   4. Reverse in place → final result
//
// Padding strategy (Gustafsson 1994):
//   Reflect `npad` samples at each end so filter transients die away
//   before reaching the actual data region.

/// Zero-phase (forward-backward) SOS filter.
///
/// Output has the same length as input with zero phase distortion.
/// Effective order is double the causal filter order.
///
/// Uses a single heap allocation: the reflect-padded buffer is filtered
/// in-place (forward pass, reverse, backward pass, reverse), then trimmed
/// back to the original length via `copy_within` + `truncate`.
pub fn sosfiltfilt(data: &[f64], sos: &[SosSection]) -> Vec<f64> {
    let npad = settling_npad(sos);
    let n = data.len();

    // One allocation: padded buffer, operated on entirely in-place.
    let mut buf = pad_reflect(data, npad);

    sosfilt_inplace(&mut buf, sos);  // forward pass
    buf.reverse();
    sosfilt_inplace(&mut buf, sos);  // backward pass
    buf.reverse();

    // Trim the padding without a second allocation.
    buf.copy_within(npad..npad + n, 0);
    buf.truncate(n);
    buf
}

/// Edge-value reflection padding (mirrors samples around the boundary).
fn pad_reflect(data: &[f64], npad: usize) -> Vec<f64> {
    let n = data.len();
    let npad = npad.min(n.saturating_sub(1));
    let mut out = Vec::with_capacity(n + 2 * npad);
    for i in (1..=npad).rev() {
        out.push(data[i.min(n - 1)]);
    }
    out.extend_from_slice(data);
    for i in 1..=npad {
        out.push(data[n.saturating_sub(1 + i)]);
    }
    out
}

/// Like `pad_reflect` but converts f32 → f64 inline, avoiding a separate
/// conversion pass.  Saves one `Vec<f64>` allocation when the source is f32.
fn pad_reflect_f32(data: &[f32], npad: usize) -> Vec<f64> {
    let n = data.len();
    let npad = npad.min(n.saturating_sub(1));
    let mut out = Vec::with_capacity(n + 2 * npad);
    for i in (1..=npad).rev() {
        out.push(data[i.min(n - 1)] as f64);
    }
    for &x in data {
        out.push(x as f64);
    }
    for i in 1..=npad {
        out.push(data[n.saturating_sub(1 + i)] as f64);
    }
    out
}

/// Causal SOS filter on f32 input. Two allocations: one f64 working buffer,
/// one f32 output.
pub fn sosfilt_from_f32(data: &[f32], sos: &[SosSection]) -> Vec<f32> {
    let mut buf: Vec<f64> = data.iter().map(|&x| x as f64).collect();
    sosfilt_inplace(&mut buf, sos);
    buf.iter().map(|&x| x as f32).collect()
}

/// Zero-phase SOS filter on f32 input. Two allocations: one f64 padded buffer,
/// one f32 output.  Avoids the intermediate f64 copy that `apply_filtfilt_f32`
/// previously allocated before calling `sosfiltfilt`.
pub fn sosfiltfilt_from_f32(data: &[f32], sos: &[SosSection]) -> Vec<f32> {
    let npad = settling_npad(sos);
    let n = data.len();

    let mut buf = pad_reflect_f32(data, npad);

    sosfilt_inplace(&mut buf, sos);
    buf.reverse();
    sosfilt_inplace(&mut buf, sos);
    buf.reverse();

    buf.copy_within(npad..npad + n, 0);
    buf.truncate(n);
    buf.iter().map(|&x| x as f32).collect()
}

// ── Initial conditions (zi) ────────────────────────────────────────────────────

/// Compute steady-state initial conditions for a DC input of value `x0`.
///
/// Returns one `BiquadState` per section. Apply these to avoid a click/step
/// transient at the start of a signal (matches scipy's `sosfilt_zi` behaviour).
///
/// In DF2T steady state both delay elements hold the same value
/// `w_ss = x_in / (1 + a1 + a2)`, and the section output that feeds the next
/// section is `y_ss = (b0 + b1 + b2) · w_ss`.
pub fn sosfilt_zi(sos: &[SosSection], x0: f64) -> Vec<BiquadState> {
    let mut states = Vec::with_capacity(sos.len());
    let mut signal = x0;
    for sec in sos {
        let denom = 1.0 + sec.a[0] + sec.a[1];
        let w_ss = if denom.abs() > 1e-12 { signal / denom } else { 0.0 };
        states.push(BiquadState { w1: w_ss, w2: w_ss });
        // Propagate DC gain to feed the next section's input.
        signal = (sec.b[0] + sec.b[1] + sec.b[2]) * w_ss;
    }
    states
}

/// Apply `sosfilt` with pre-computed initial states.
pub fn sosfilt_with_zi(
    data: &[f64],
    sos: &[SosSection],
    states: &mut Vec<BiquadState>,
) -> Vec<f64> {
    data.iter()
        .map(|&x| {
            let mut s = x;
            for (sec, st) in sos.iter().zip(states.iter_mut()) {
                s = process_sample_df2t(s, sec, st);
            }
            s
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::design::SosSection;

    fn identity_sos() -> SosSection {
        SosSection::second_order(1.0, 0.0, 0.0, 0.0, 0.0)
    }

    #[test]
    fn identity_filter_passthrough() {
        let sos = vec![identity_sos()];
        let input: Vec<f64> = (0..64).map(|i| i as f64).collect();
        let output = sosfilt(&input, &sos);
        for (a, b) in input.iter().zip(output.iter()) {
            assert!((a - b).abs() < 1e-12, "identity failed: {a} vs {b}");
        }
    }

    #[test]
    fn filtfilt_dc_passthrough() {
        let sos = vec![identity_sos()];
        let input = vec![1.0_f64; 256];
        let output = sosfiltfilt(&input, &sos);
        for &y in &output {
            assert!((y - 1.0).abs() < 1e-10, "DC drift: {y}");
        }
    }

    #[test]
    fn denormal_flush_on_silence() {
        let sos = vec![SosSection::second_order(0.5, 0.0, 0.0, 0.0, 0.0)];
        let mut data: Vec<f64> = std::iter::once(1.0)
            .chain(std::iter::repeat(0.0).take(200))
            .collect();
        sosfilt_inplace(&mut data, &sos);
        assert_eq!(data[200], 0.0);
    }

    #[test]
    fn sosfilt_zi_no_transient_on_dc_step() {
        // A lowpass SOS fed with a DC=1 step should produce exactly 1.0 at every
        // sample when the initial states are set to the DC steady-state values.
        let sos = vec![SosSection::second_order(
            0.06745527, 0.13491055, 0.06745527,
            -1.14298051, 0.41280160,
        )];
        let zi = sosfilt_zi(&sos, 1.0);
        let input = vec![1.0_f64; 64];
        let output = sosfilt_with_zi(&input, &sos, &mut zi.clone());
        // With correct initial conditions there should be no step transient.
        for (i, &y) in output.iter().enumerate() {
            assert!((y - 1.0).abs() < 1e-6, "sample {} = {}", i, y);
        }
    }

    #[test]
    fn settling_npad_resonant_filter_exceeds_section_count_floor() {
        // A near-unit-circle pole (r ≈ 0.98) has a long settling time.
        // settling_npad must exceed the 6*n_sections minimum.
        let sos = vec![SosSection::second_order(
            0.001, 0.002, 0.001,
            -1.9607, 0.9608, // poles at r ≈ 0.98
        )];
        let npad = settling_npad(&sos);
        let min_pad = 6 * sos.len();
        assert!(npad > min_pad, "expected npad={} > min_pad={}", npad, min_pad);
    }
}
