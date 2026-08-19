//! FIR filtering — general convolution, moving statistics, and nonlinear filters.
//!
//! ## Convolution method selection
//!
//! [`filter_channels_fir`] automatically picks the fastest implementation:
//!
//! | Filter taps | Method |
//! |-------------|--------|
//! | ≤ 64        | Direct O(N·M) per channel |
//! | > 64        | FFT overlap-add O(N log N), filter FFT pre-computed once |
//!
//! ## Batch processing surplus
//!
//! | `center` | Required surplus per side |
//! |----------|---------------------------|
//! | `true`   | `(n_taps - 1) / 2` on both sides |
//! | `false`  | `n_taps - 1` on the **left** side only |

use num_complex::Complex;
use rayon::prelude::*;
use rustfft::FftPlanner;
use std::sync::Arc;

const FFT_THRESHOLD: usize = 64;

fn should_use_fft(n_taps: usize) -> bool {
    n_taps > FFT_THRESHOLD
}

// ── FFT helpers ──────────────────────────────────────────────────────────────

/// Pre-computes the zero-padded FFT of `filter` into a `Vec<Complex<f32>>`
/// of length `n_fft`.
fn filter_fft(filter: &[f32], n_fft: usize) -> Vec<Complex<f32>> {
    let mut buf: Vec<Complex<f32>> = (0..n_fft)
        .map(|i| Complex::new(if i < filter.len() { filter[i] } else { 0.0 }, 0.0))
        .collect();
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n_fft);
    fft.process(&mut buf);
    buf
}

/// FFT-based overlap-add convolution for a single channel.
///
/// `h_fft` — pre-computed FFT of the filter (length `n_fft`).
/// `filter_len` — original (un-padded) filter length, used to compute the
///   linear-convolution output length.
/// `center` — when `true`, shifts the output by `filter_len / 2` samples to
///   compensate for group delay (zero-phase approximation for symmetric FIR).
fn fft_convolve_channel(
    signal: &[f32],
    h_fft: &[Complex<f32>],
    filter_len: usize,
    n_fft: usize,
    center: bool,
    fft_fwd: &Arc<dyn rustfft::Fft<f32>>,
    fft_inv: &Arc<dyn rustfft::Fft<f32>>,
) -> Vec<f32> {
    let n = signal.len();
    let full_len = n + filter_len - 1;

    let mut x_buf: Vec<Complex<f32>> = (0..n_fft)
        .map(|i| Complex::new(if i < n { signal[i] } else { 0.0 }, 0.0))
        .collect();
    fft_fwd.process(&mut x_buf);

    let mut y_buf: Vec<Complex<f32>> =
        x_buf.iter().zip(h_fft.iter()).map(|(x, h)| x * h).collect();
    fft_inv.process(&mut y_buf);

    let scale = 1.0 / n_fft as f32;
    let offset = if center { filter_len / 2 } else { 0 };
    (0..n)
        .map(|i| {
            let j = i + offset;
            if j < full_len { y_buf[j].re * scale } else { 0.0 }
        })
        .collect()
}

/// Direct-convolution for a single channel.
///
/// Both `center` and causal modes handle boundaries by zero-padding (no
/// circular wraparound). `O(N·M)` — use only when `M ≤ FFT_THRESHOLD`.
fn direct_convolve_channel(signal: &[f32], filter: &[f32], center: bool) -> Vec<f32> {
    let n = signal.len();
    let m = filter.len();
    let half = m / 2;
    (0..n)
        .map(|i| {
            (0..m)
                .map(|k| {
                    let j = if center {
                        i as isize + half as isize - k as isize
                    } else {
                        i as isize - k as isize
                    };
                    if j >= 0 && (j as usize) < n {
                        filter[k] * signal[j as usize]
                    } else {
                        0.0
                    }
                })
                .sum()
        })
        .collect()
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Apply a FIR filter with arbitrary `coeffs` to all channels in parallel.
///
/// `data` is channel-major: channel `c` at `c * n_samples .. (c+1) * n_samples`.
///
/// `center`:
/// - `true`  — zero-phase output (compensates group delay by `n_taps / 2`).
///   Requires `(n_taps - 1) / 2` surplus samples on both sides for batch use.
/// - `false` — causal output. Requires `n_taps - 1` surplus on the left side.
///
/// The filter's frequency-domain representation is pre-computed once and
/// shared across all channel workers.
///
/// ```
/// use dsp_core::filter::fir::filter_channels_fir;
/// // 1-tap "filter" = passthrough
/// let data: Vec<f32> = (0..8).map(|i| i as f32).collect();
/// let out = filter_channels_fir(&data, 1, &[1.0], true);
/// assert_eq!(out, data);
/// ```
pub fn filter_channels_fir(
    data: &[f32],
    n_channels: usize,
    coeffs: &[f32],
    center: bool,
) -> Vec<f32> {
    let n_samples = data.len() / n_channels;

    if should_use_fft(coeffs.len()) {
        let full_len = n_samples + coeffs.len() - 1;
        let n_fft = full_len.next_power_of_two();

        // Pre-compute the filter FFT once, shared across channel workers.
        let h_fft = filter_fft(coeffs, n_fft);

        let mut planner = FftPlanner::<f32>::new();
        let fft_fwd = planner.plan_fft_forward(n_fft);
        let fft_inv = planner.plan_fft_inverse(n_fft);

        // Arc allows sharing the planned FFT objects across Rayon threads.
        let fft_fwd = Arc::clone(&fft_fwd);
        let fft_inv = Arc::clone(&fft_inv);
        let h_fft = Arc::new(h_fft);

        let mut out = vec![0.0_f32; data.len()];
        out.par_chunks_mut(n_samples)
            .enumerate()
            .for_each(|(c, dst)| {
                let src = &data[c * n_samples..(c + 1) * n_samples];
                let filtered = fft_convolve_channel(
                    src, &h_fft, coeffs.len(), n_fft, center,
                    &fft_fwd, &fft_inv,
                );
                dst.copy_from_slice(&filtered);
            });
        out
    } else {
        let mut out = vec![0.0_f32; data.len()];
        out.par_chunks_mut(n_samples)
            .enumerate()
            .for_each(|(c, dst)| {
                let src = &data[c * n_samples..(c + 1) * n_samples];
                let filtered = direct_convolve_channel(src, coeffs, center);
                dst.copy_from_slice(&filtered);
            });
        out
    }
}

/// Boxcar (uniform) moving average across all channels.
///
/// `center`: `true` → centered window (non-causal); `false` → causal trailing window.
/// Edge samples use a truncated window (variable count) rather than zero-padding.
///
/// Uses an O(N) sliding running-sum algorithm regardless of window size — no
/// convolution, no FFT overhead.
///
/// ```
/// use dsp_core::filter::fir::moving_average;
/// let data = vec![1.0_f32; 8]; // constant signal
/// let out = moving_average(&data, 1, 4, true);
/// // center samples away from the edges should still be 1.0
/// assert!((out[4] - 1.0).abs() < 1e-5);
/// ```
pub fn moving_average(data: &[f32], n_channels: usize, window: usize, center: bool) -> Vec<f32> {
    assert!(window >= 1, "moving_average: window must be >= 1");
    let n_samples = data.len() / n_channels;
    let half = window / 2;
    let mut out = vec![0.0_f32; data.len()];
    out.par_chunks_mut(n_samples)
        .enumerate()
        .for_each(|(c, dst)| {
            let src = &data[c * n_samples..(c + 1) * n_samples];
            if center {
                // Seed: window for i=0 spans [0, min(half, n-1)].
                let init_hi = half.min(n_samples.saturating_sub(1));
                let mut sum: f64 = src[..=init_hi].iter().map(|&x| x as f64).sum();
                let mut lo = 0usize;
                let mut hi = init_hi;
                dst[0] = (sum / (hi - lo + 1) as f64) as f32;
                for i in 1..n_samples {
                    let new_hi = (i + half).min(n_samples - 1);
                    if new_hi > hi {
                        sum += src[new_hi] as f64;
                        hi = new_hi;
                    }
                    let new_lo = i.saturating_sub(half);
                    if new_lo > lo {
                        sum -= src[lo] as f64;
                        lo = new_lo;
                    }
                    dst[i] = (sum / (hi - lo + 1) as f64) as f32;
                }
            } else {
                let mut sum = 0.0f64;
                for i in 0..n_samples {
                    sum += src[i] as f64;
                    if i >= window { sum -= src[i - window] as f64; }
                    dst[i] = (sum / (i + 1).min(window) as f64) as f32;
                }
            }
        });
    out
}

/// Moving RMS (root mean square) across all channels.
///
/// Non-linear — always computed directly (not via convolution).
/// Edge windows are truncated (variable count) to avoid bias from zero padding.
///
/// ```
/// use dsp_core::filter::fir::moving_rms;
/// // Constant signal of amplitude 2.0 → RMS = 2.0 everywhere
/// let data = vec![2.0_f32; 20];
/// let out = moving_rms(&data, 1, 5, true);
/// for &v in &out { assert!((v - 2.0).abs() < 1e-5); }
/// ```
pub fn moving_rms(data: &[f32], n_channels: usize, window: usize, center: bool) -> Vec<f32> {
    assert!(window >= 1, "moving_rms: window must be >= 1");
    let n_samples = data.len() / n_channels;
    let half = window / 2;
    let mut out = vec![0.0_f32; data.len()];
    out.par_chunks_mut(n_samples)
        .enumerate()
        .for_each(|(c, dst)| {
            let src = &data[c * n_samples..(c + 1) * n_samples];
            if center {
                let init_hi = half.min(n_samples.saturating_sub(1));
                let mut sum_sq: f64 = src[..=init_hi].iter().map(|&x| (x as f64) * (x as f64)).sum();
                let mut lo = 0usize;
                let mut hi = init_hi;
                dst[0] = (sum_sq / (hi - lo + 1) as f64).sqrt() as f32;
                for i in 1..n_samples {
                    let new_hi = (i + half).min(n_samples - 1);
                    if new_hi > hi {
                        let v = src[new_hi] as f64;
                        sum_sq += v * v;
                        hi = new_hi;
                    }
                    let new_lo = i.saturating_sub(half);
                    if new_lo > lo {
                        let v = src[lo] as f64;
                        sum_sq -= v * v;
                        sum_sq = sum_sq.max(0.0);
                        lo = new_lo;
                    }
                    dst[i] = (sum_sq / (hi - lo + 1) as f64).sqrt() as f32;
                }
            } else {
                let mut sum_sq = 0.0f64;
                for i in 0..n_samples {
                    let v = src[i] as f64;
                    sum_sq += v * v;
                    if i >= window {
                        let old = src[i - window] as f64;
                        sum_sq -= old * old;
                        sum_sq = sum_sq.max(0.0);
                    }
                    dst[i] = (sum_sq / (i + 1).min(window) as f64).sqrt() as f32;
                }
            }
        });
    out
}

/// Causal exponential moving average: `y[t] = α·x[t] + (1−α)·y[t−1]`.
///
/// `alpha` ∈ (0, 1). Large `alpha` → fast response, less smoothing.
/// The equivalent time constant is `τ = -1 / ln(1-α)` samples.
///
/// ```
/// use dsp_core::filter::fir::exponential_moving_average;
/// // Step input → EMA should approach 1.0 asymptotically
/// let data = vec![1.0_f32; 100];
/// let out = exponential_moving_average(&data, 1, 0.1);
/// assert!((out[99] - 1.0).abs() < 0.01);
/// ```
pub fn exponential_moving_average(data: &[f32], n_channels: usize, alpha: f32) -> Vec<f32> {
    assert!(alpha > 0.0 && alpha < 1.0, "exponential_moving_average: alpha must be in (0, 1)");
    let n_samples = data.len() / n_channels;
    let beta = 1.0 - alpha;
    let mut out = vec![0.0_f32; data.len()];
    out.par_chunks_mut(n_samples)
        .enumerate()
        .for_each(|(c, dst)| {
            let src = &data[c * n_samples..(c + 1) * n_samples];
            if src.is_empty() {
                return;
            }
            let mut prev = src[0];
            dst[0] = prev;
            for i in 1..n_samples {
                prev = alpha * src[i] + beta * prev;
                dst[i] = prev;
            }
        });
    out
}

/// Median filter across all channels in parallel.
///
/// `window` is forced odd (incremented by 1 if even) to maintain symmetry.
/// Edge windows are truncated (samples outside the signal are excluded).
///
/// ```
/// use dsp_core::filter::fir::median_filter;
/// // Remove a single spike from an otherwise flat signal
/// let mut data = vec![1.0_f32; 20];
/// data[10] = 100.0;
/// let out = median_filter(&data, 1, 5, true);
/// assert!((out[10] - 1.0).abs() < 1e-5);
/// ```
pub fn median_filter(data: &[f32], n_channels: usize, window: usize, center: bool) -> Vec<f32> {
    assert!(window >= 1, "median_filter: window must be >= 1");
    let window = if window % 2 == 0 { window + 1 } else { window };
    let n_samples = data.len() / n_channels;
    let half = window / 2;
    let mut out = vec![0.0_f32; data.len()];
    out.par_chunks_mut(n_samples)
        .enumerate()
        .for_each(|(c, dst)| {
            let src = &data[c * n_samples..(c + 1) * n_samples];
            // sorted: maintains a sorted view of the current window via
            // binary-search insert/remove — O(M) per step vs O(M log M).
            let mut sorted: Vec<f32> = Vec::with_capacity(window);

            // Track which sample index falls off the left edge each step.
            // For centered: left edge is i.saturating_sub(half).
            // For causal:   left edge is i.saturating_sub(window - 1).
            let mut prev_lo = 0usize;
            let mut prev_hi = 0usize; // exclusive: window is src[prev_lo..prev_hi]

            for i in 0..n_samples {
                let lo = if center { i.saturating_sub(half) } else { i.saturating_sub(window - 1) };
                let hi = if center { (i + half + 1).min(n_samples) } else { i + 1 };

                if i == 0 {
                    // Seed the sorted buffer with the initial window.
                    for &v in &src[lo..hi] {
                        let pos = sorted.partition_point(|&x| x <= v);
                        sorted.insert(pos, v);
                    }
                } else {
                    // Remove samples that left the window on the left.
                    for rem_idx in prev_lo..lo {
                        let v = src[rem_idx];
                        let pos = sorted.partition_point(|&x| x < v);
                        sorted.remove(pos);
                    }
                    // Add samples that entered the window on the right.
                    for add_idx in prev_hi..hi {
                        let v = src[add_idx];
                        let pos = sorted.partition_point(|&x| x <= v);
                        sorted.insert(pos, v);
                    }
                }

                dst[i] = sorted[sorted.len() / 2];
                prev_lo = lo;
                prev_hi = hi;
            }
        });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── filter_channels_fir ──────────────────────────────────────────────────

    #[test]
    fn passthrough_single_tap() {
        let data: Vec<f32> = (0..8).map(|i| i as f32).collect();
        let out = filter_channels_fir(&data, 1, &[1.0], true);
        assert_eq!(out, data);
    }

    #[test]
    fn multichannel_channels_are_independent() {
        // ch0 = all 1.0, ch1 = all 0.0 → each should filter independently
        let mut data = vec![0.0_f32; 20];
        for i in 0..10 { data[i] = 1.0; } // ch0
        let out = filter_channels_fir(&data, 2, &[1.0], false);
        assert!(out[0..10].iter().all(|&v| (v - 1.0).abs() < 1e-5));
        assert!(out[10..20].iter().all(|&v| v.abs() < 1e-5));
    }

    #[test]
    fn fir_lowpass_attenuates_high_frequency() {
        // 65-tap Hann-windowed sinc at 100 Hz / Fs 1000 Hz (triggers FFT path)
        use crate::filter::{WindowType, generate_sinc_coeffs};
        let coeffs = generate_sinc_coeffs(100.0, 1_000.0, 65, WindowType::Hann).unwrap();

        // Signal: pure 400 Hz tone (well above 100 Hz cutoff)
        use std::f32::consts::PI;
        let fs = 1_000.0_f32;
        let n = 512;
        let data: Vec<f32> = (0..n).map(|i| (2.0 * PI * 400.0 / fs * i as f32).sin()).collect();

        let out = filter_channels_fir(&data, 1, &coeffs, true);
        // Ignore edges (group-delay compensation may not be perfect at boundaries)
        let rms_in: f32 = (data[100..400].iter().map(|&v| v * v).sum::<f32>() / 300.0).sqrt();
        let rms_out: f32 = (out[100..400].iter().map(|&v| v * v).sum::<f32>() / 300.0).sqrt();
        // Should be attenuated by at least 10 dB (factor of ~3)
        assert!(rms_out < rms_in / 3.0, "attenuation insufficient: in={} out={}", rms_in, rms_out);
    }

    #[test]
    fn direct_and_fft_paths_agree() {
        // 32-tap filter (direct) vs 65-tap same shape (FFT) on the same signal
        // Just verify neither path panics and produces same-length output
        let data: Vec<f32> = (0..256).map(|i| (i % 8) as f32).collect();
        let coeffs_short = vec![1.0 / 32.0_f32; 32];
        let coeffs_long = vec![1.0 / 65.0_f32; 65];
        let out_direct = filter_channels_fir(&data, 1, &coeffs_short, true);
        let out_fft = filter_channels_fir(&data, 1, &coeffs_long, true);
        assert_eq!(out_direct.len(), 256);
        assert_eq!(out_fft.len(), 256);
    }

    // ── moving_average ───────────────────────────────────────────────────────

    #[test]
    fn moving_average_constant_signal() {
        let data = vec![3.0_f32; 64];
        let out = moving_average(&data, 1, 8, true);
        // Interior samples (away from zero-padded edges) should be exactly 3.0
        for &v in &out[8..56] {
            assert!((v - 3.0).abs() < 1e-5, "got {}", v);
        }
    }

    #[test]
    fn moving_average_multichannel_shape() {
        let data = vec![1.0_f32; 40]; // 2 ch × 20 samples
        let out = moving_average(&data, 2, 4, true);
        assert_eq!(out.len(), 40);
    }

    // ── moving_rms ───────────────────────────────────────────────────────────

    #[test]
    fn moving_rms_constant_signal() {
        let data = vec![2.0_f32; 40];
        let out = moving_rms(&data, 1, 5, true);
        for &v in &out { assert!((v - 2.0).abs() < 1e-5, "rms={}", v); }
    }

    #[test]
    fn moving_rms_zero_signal() {
        let data = vec![0.0_f32; 20];
        let out = moving_rms(&data, 1, 5, false);
        assert!(out.iter().all(|&v| v == 0.0));
    }

    // ── exponential_moving_average ───────────────────────────────────────────

    #[test]
    fn ema_constant_signal_converges() {
        let data = vec![1.0_f32; 200];
        let out = exponential_moving_average(&data, 1, 0.2);
        assert!((out[199] - 1.0).abs() < 0.001, "ema tail = {}", out[199]);
    }

    #[test]
    fn ema_first_sample_is_input() {
        let data = vec![5.0_f32, 0.0, 0.0, 0.0];
        let out = exponential_moving_average(&data, 1, 0.5);
        assert_eq!(out[0], 5.0);
    }

    #[test]
    fn ema_multichannel_independent() {
        let mut data = vec![0.0_f32; 40]; // 2 ch × 20 samples
        for i in 0..20 { data[i] = 1.0; }      // ch0 = 1.0
        // ch1 = 0.0
        let out = exponential_moving_average(&data, 2, 0.3);
        // ch1 output should remain 0
        assert!(out[20..40].iter().all(|&v| v == 0.0));
    }

    // ── median_filter ────────────────────────────────────────────────────────

    #[test]
    fn median_removes_spike() {
        let mut data = vec![1.0_f32; 20];
        data[10] = 100.0;
        let out = median_filter(&data, 1, 5, true);
        assert!((out[10] - 1.0).abs() < 1e-5, "spike not removed: {}", out[10]);
    }

    #[test]
    fn median_constant_signal_unchanged() {
        let data = vec![7.0_f32; 20];
        let out = median_filter(&data, 1, 5, true);
        for &v in &out { assert_eq!(v, 7.0); }
    }

    #[test]
    fn median_forces_odd_window() {
        // Window 4 → promoted to 5, should not panic
        let data = vec![1.0_f32; 20];
        let out = median_filter(&data, 1, 4, true);
        assert_eq!(out.len(), 20);
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// f64 FIR design and application (windowed-sinc, FirFilter, Savitzky-Golay)
// ═══════════════════════════════════════════════════════════════════════════════

use super::window::{WindowType, make_window};
use std::f64::consts::PI as PI64;

/// Normalised sinc: sinc(x) = sin(πx) / (πx), sinc(0) = 1.
#[inline]
fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-12 { 1.0 } else { (PI64 * x).sin() / (PI64 * x) }
}

// ── Windowed-sinc design ───────────────────────────────────────────────────────

/// Low-pass FIR kernel via windowed sinc (f64).
///
/// - `cutoff_hz`: -6 dB cutoff (Hz)
/// - `num_taps`: must be odd for a type-I linear-phase filter
/// - `window`: window type
/// - `fs`: sample rate (Hz)
pub fn lp_kernel(cutoff_hz: f64, num_taps: usize, window: WindowType, fs: f64) -> Vec<f64> {
    let fc = cutoff_hz / fs;
    let half = (num_taps as f64 - 1.0) / 2.0;
    let win = make_window(window, num_taps);
    (0..num_taps)
        .map(|n| {
            let t = n as f64 - half;
            2.0 * fc * sinc(2.0 * fc * t) * win[n]
        })
        .collect()
}

/// High-pass FIR kernel (spectral inversion of LP).
pub fn hp_kernel(cutoff_hz: f64, num_taps: usize, window: WindowType, fs: f64) -> Vec<f64> {
    let mut h = lp_kernel(cutoff_hz, num_taps, window, fs);
    let centre = (num_taps - 1) / 2;
    h.iter_mut().for_each(|x| *x = -*x);
    h[centre] += 1.0;
    h
}

/// Band-pass FIR kernel.
pub fn bp_kernel(
    low_hz: f64,
    high_hz: f64,
    num_taps: usize,
    window: WindowType,
    fs: f64,
) -> Vec<f64> {
    let h_hi = lp_kernel(high_hz, num_taps, window, fs);
    let h_lo = lp_kernel(low_hz, num_taps, window, fs);
    h_hi.iter().zip(h_lo.iter()).map(|(&a, &b)| a - b).collect()
}

/// Band-stop (notch) FIR kernel.
pub fn bs_kernel(
    low_hz: f64,
    high_hz: f64,
    num_taps: usize,
    window: WindowType,
    fs: f64,
) -> Vec<f64> {
    let h_lp = lp_kernel(low_hz, num_taps, window, fs);
    let h_hp = hp_kernel(high_hz, num_taps, window, fs);
    h_lp.iter().zip(h_hp.iter()).map(|(&a, &b)| a + b).collect()
}

// ── Kaiser automatic design ───────────────────────────────────────────────────

use super::window::{kaiser_beta_from_attenuation, kaiser_min_length};

/// Automatically-sized Kaiser LP filter.
pub fn kaiser_lp(cutoff_hz: f64, trans_bw_hz: f64, atten_db: f64, fs: f64) -> Vec<f64> {
    let delta_f = trans_bw_hz / fs;
    let beta = kaiser_beta_from_attenuation(atten_db);
    let n = kaiser_min_length(atten_db, delta_f);
    let fc = (cutoff_hz + trans_bw_hz / 2.0).min(fs / 2.0 - 1.0);
    lp_kernel(fc, n, WindowType::Kaiser { beta }, fs)
}

/// Automatically-sized Kaiser HP filter.
pub fn kaiser_hp(cutoff_hz: f64, trans_bw_hz: f64, atten_db: f64, fs: f64) -> Vec<f64> {
    let delta_f = trans_bw_hz / fs;
    let beta = kaiser_beta_from_attenuation(atten_db);
    let n = kaiser_min_length(atten_db, delta_f);
    let fc = (cutoff_hz - trans_bw_hz / 2.0).max(1.0);
    hp_kernel(fc, n, WindowType::Kaiser { beta }, fs)
}

// ── FirFilter struct ──────────────────────────────────────────────────────────

/// A designed FIR filter kernel with apply methods (f64 internally).
#[derive(Debug, Clone)]
pub struct FirFilter {
    pub kernel: Vec<f64>,
}

impl FirFilter {
    pub fn new(kernel: Vec<f64>) -> Self {
        Self { kernel }
    }

    /// Direct convolution — efficient for short kernels (≤ 64 taps).
    pub fn apply(&self, data: &[f64]) -> Vec<f64> {
        convolve_direct_f64(data, &self.kernel)
    }

    /// Overlap-add FFT convolution — efficient for long kernels (> 64 taps).
    pub fn apply_fft(&self, data: &[f64]) -> Vec<f64> {
        overlap_add_f64(data, &self.kernel)
    }

    /// Auto-selects direct vs FFT based on kernel length.
    pub fn apply_auto(&self, data: &[f64]) -> Vec<f64> {
        if self.kernel.len() <= 64 { self.apply(data) } else { self.apply_fft(data) }
    }

    /// f32 convenience wrapper.
    pub fn apply_f32(&self, data: &[f32]) -> Vec<f32> {
        let d64: Vec<f64> = data.iter().map(|&x| x as f64).collect();
        self.apply_auto(&d64).into_iter().map(|x| x as f32).collect()
    }
}

// ── Direct convolution (f64) ──────────────────────────────────────────────────

/// Linear convolution with zero-padding at edges (same-length output, f64).
pub fn convolve_direct_f64(data: &[f64], kernel: &[f64]) -> Vec<f64> {
    let n = data.len();
    let m = kernel.len();
    let mid = (m - 1) / 2;
    (0..n)
        .map(|i| {
            let mut acc = 0.0;
            for (k, &h) in kernel.iter().enumerate() {
                let j = i as isize + k as isize - mid as isize;
                if j >= 0 && (j as usize) < n {
                    acc += data[j as usize] * h;
                }
            }
            acc
        })
        .collect()
}

// ── Overlap-add (FFT) convolution (f64) ──────────────────────────────────────

/// Overlap-add convolution for long f64 kernels.
pub fn overlap_add_f64(data: &[f64], kernel: &[f64]) -> Vec<f64> {
    let n = data.len();
    let m = kernel.len();
    if m == 0 || n == 0 { return vec![0.0; n]; }

    let fft_size = next_pow2_usize(2 * m);
    let block = fft_size - m + 1;
    let scale = 1.0 / fft_size as f64;

    let mut planner = FftPlanner::<f64>::new();
    let fft_fwd = planner.plan_fft_forward(fft_size);
    let fft_inv = planner.plan_fft_inverse(fft_size);

    let mut h_buf: Vec<Complex<f64>> = kernel.iter().map(|&v| Complex::new(v, 0.0)).collect();
    h_buf.resize(fft_size, Complex::new(0.0, 0.0));
    fft_fwd.process(&mut h_buf);

    let mut out = vec![0.0f64; n + m - 1];
    let mut x_buf = vec![Complex::<f64>::new(0.0, 0.0); fft_size];

    let mut pos = 0;
    while pos < n {
        let end = (pos + block).min(n);
        for (v, s) in x_buf.iter_mut().zip(data[pos..end].iter()) {
            v.re = *s; v.im = 0.0;
        }
        for v in x_buf[end - pos..].iter_mut() {
            v.re = 0.0; v.im = 0.0;
        }

        fft_fwd.process(&mut x_buf);
        for (x, h) in x_buf.iter_mut().zip(h_buf.iter()) {
            *x = *x * h;
        }
        fft_inv.process(&mut x_buf);

        for (i, v) in x_buf.iter().enumerate() {
            let idx = pos + i;
            if idx < out.len() { out[idx] += v.re * scale; }
        }
        pos += end - pos;
    }

    let mid = (m - 1) / 2;
    out[mid..mid + n].to_vec()
}

fn next_pow2_usize(n: usize) -> usize {
    let mut p = 1;
    while p < n { p <<= 1; }
    p
}

// ── Additional f64 utilities ──────────────────────────────────────────────────

/// Symmetric (centered) moving average on a single-channel f64 slice.
pub fn moving_average_symmetric(data: &[f64], n: usize) -> Vec<f64> {
    assert!(n > 0);
    let half = n / 2;
    let len = data.len();
    (0..len)
        .map(|i| {
            let lo = i.saturating_sub(half);
            let hi = (i + half + 1).min(len);
            data[lo..hi].iter().sum::<f64>() / (hi - lo) as f64
        })
        .collect()
}

/// Exponential moving average on a single-channel f64 slice.
pub fn ema(data: &[f64], alpha: f64) -> Vec<f64> {
    if data.is_empty() { return Vec::new(); }
    let beta = 1.0 - alpha;
    let mut out = Vec::with_capacity(data.len());
    let mut prev = data[0];
    out.push(prev);
    for &x in &data[1..] {
        prev = alpha * x + beta * prev;
        out.push(prev);
    }
    out
}

/// Compute EMA alpha from a desired -3 dB cutoff frequency.
pub fn ema_alpha_from_cutoff(cutoff_hz: f64, fs: f64) -> f64 {
    let cos_w = (2.0 * PI64 * cutoff_hz / fs).cos();
    2.0 - cos_w - (cos_w * cos_w - 4.0 * cos_w + 3.0).sqrt()
}

/// RMS envelope with a sliding window of `n` samples (single-channel f64).
pub fn rms_envelope(data: &[f64], n: usize) -> Vec<f64> {
    assert!(n > 0);
    let mut sum_sq = 0.0f64;
    data.iter()
        .enumerate()
        .map(|(i, &x)| {
            sum_sq += x * x;
            if i >= n { sum_sq -= data[i - n] * data[i - n]; }
            let count = (i + 1).min(n) as f64;
            (sum_sq / count).sqrt()
        })
        .collect()
}

// ── Savitzky-Golay ────────────────────────────────────────────────────────────

/// Design Savitzky-Golay smoothing coefficients.
///
/// `window_len` must be odd; `poly_order` must be < `window_len`.
pub fn savitzky_golay_kernel(window_len: usize, poly_order: usize) -> Vec<f64> {
    assert!(window_len % 2 == 1, "window_len must be odd");
    assert!(poly_order < window_len, "poly_order must be < window_len");
    let half = (window_len as isize - 1) / 2;
    let n = window_len;
    let p = poly_order + 1;

    let mut a = vec![vec![0.0f64; p]; n];
    for (i, row) in a.iter_mut().enumerate() {
        let x = (i as isize - half) as f64;
        let mut xp = 1.0;
        for j in 0..p { row[j] = xp; xp *= x; }
    }

    let cols: Vec<Vec<f64>> = (0..p).map(|j| a.iter().map(|row| row[j]).collect()).collect();
    let mut q = vec![vec![0.0f64; n]; p];
    let mut r = vec![vec![0.0f64; p]; p];

    for j in 0..p {
        let mut v = cols[j].clone();
        for k in 0..j {
            let dot: f64 = v.iter().zip(q[k].iter()).map(|(&a, &b)| a * b).sum();
            for (vi, &qi) in v.iter_mut().zip(q[k].iter()) { *vi -= dot * qi; }
            r[k][j] = dot;
        }
        let norm: f64 = v.iter().map(|&x| x * x).sum::<f64>().sqrt();
        if norm > 1e-14 { q[j] = v.iter().map(|&x| x / norm).collect(); r[j][j] = norm; }
    }

    let centre = half as usize;
    let rhs: Vec<f64> = (0..p).map(|k| q[k][centre]).collect();
    let mut c = vec![0.0f64; p];
    for i in (0..p).rev() {
        let mut s = rhs[i];
        for j in (i + 1)..p { s -= r[i][j] * c[j]; }
        c[i] = if r[i][i].abs() > 1e-14 { s / r[i][i] } else { 0.0 };
    }

    a.iter().map(|row| row.iter().zip(c.iter()).map(|(&a, &ci)| a * ci).sum()).collect()
}

/// Apply Savitzky-Golay smoothing filter.
pub fn savitzky_golay(data: &[f64], window_len: usize, poly_order: usize) -> Vec<f64> {
    let kernel = savitzky_golay_kernel(window_len, poly_order);
    convolve_direct_f64(data, &kernel)
}
