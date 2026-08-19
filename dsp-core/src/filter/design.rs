//! Filter design: analog prototypes → frequency transforms → bilinear → SOS.
//!
//! Nyquist validation and the `generate_sinc_coeffs` helper are also here for
//! backward compatibility with existing call sites.
//!
//! # Pipeline (matches scipy.signal internals)
//! 1. `butter_analog` / `cheby1_analog` / … → [`Zpk`] prototype at 1 rad/s
//! 2. `lp_to_{lp,hp,bp,bs}`                → scaled analog [`Zpk`]
//! 3. `bilinear_zpk`                        → digital [`Zpk`]
//! 4. `zpk_to_sos`                          → `Vec<`[`SosSection`]`>`

use num_complex::Complex64;
use std::f64::consts::PI;
use super::window::{WindowType, make_window};

// ── Public types ──────────────────────────────────────────────────────────────

/// Error returned by filter design and validation functions.
#[derive(Debug, Clone)]
pub enum FilterError {
    NyquistViolation { cutoff_hz: f32, nyquist_hz: f32 },
    InvalidParameters(String),
}

impl std::fmt::Display for FilterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FilterError::NyquistViolation { cutoff_hz, nyquist_hz } => write!(
                f, "cutoff {:.1} Hz exceeds Nyquist limit of {:.1} Hz", cutoff_hz, nyquist_hz,
            ),
            FilterError::InvalidParameters(msg) => write!(f, "invalid filter parameters: {}", msg),
        }
    }
}
impl std::error::Error for FilterError {}

/// Second-order section in normalized form (a0 = 1 always, coefficients in f64).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SosSection {
    /// Numerator: b0, b1, b2.
    pub b: [f64; 3],
    /// Denominator: a1, a2 (a0 = 1).
    pub a: [f64; 2],
}

impl SosSection {
    pub fn first_order(b0: f64, b1: f64, a1: f64) -> Self {
        Self { b: [b0, b1, 0.0], a: [a1, 0.0] }
    }

    pub fn second_order(b0: f64, b1: f64, b2: f64, a1: f64, a2: f64) -> Self {
        Self { b: [b0, b1, b2], a: [a1, a2] }
    }

    /// Construct from a SciPy-style `[b0, b1, b2, a0, a1, a2]` row (f32 → f64).
    pub fn from_row(row: [f32; 6]) -> Self {
        let a0 = row[3] as f64;
        assert!(a0.abs() > 1e-30, "SOS a0 must not be zero");
        Self::second_order(
            row[0] as f64 / a0, row[1] as f64 / a0, row[2] as f64 / a0,
            row[4] as f64 / a0, row[5] as f64 / a0,
        )
    }

}

/// Filter response type — carries the cutoff/band frequencies in Hz.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum FilterResponse {
    LowPass  { cutoff: f64 },
    HighPass { cutoff: f64 },
    BandPass { low: f64, high: f64 },
    BandStop { low: f64, high: f64 },
}

// ── Nyquist validation ────────────────────────────────────────────────────────

/// Returns `Err` when `cutoff_hz` is outside `(0, sample_rate / 2)`.
///
/// ```
/// use dsp_core::filter::validate_nyquist;
/// assert!(validate_nyquist(1_000.0, 40_000.0).is_ok());
/// assert!(validate_nyquist(20_000.0, 40_000.0).is_err());
/// ```
pub fn validate_nyquist(cutoff_hz: f32, sample_rate: f32) -> Result<(), FilterError> {
    let nyquist = sample_rate / 2.0;
    if cutoff_hz <= 0.0 || cutoff_hz >= nyquist {
        return Err(FilterError::NyquistViolation { cutoff_hz, nyquist_hz: nyquist });
    }
    Ok(())
}

/// Windowed-sinc FIR lowpass coefficients normalized to unit DC gain.
///
/// This is the backward-compatible wrapper. For full FIR design (HP/BP/BS,
/// Kaiser auto-sizing, Savitzky-Golay) use `dsp_core::filter::fir`.
pub fn generate_sinc_coeffs(
    cutoff_hz: f32,
    sample_rate: f32,
    n_taps: usize,
    window: WindowType,
) -> Result<Vec<f32>, FilterError> {
    validate_nyquist(cutoff_hz, sample_rate)?;
    if n_taps < 3 {
        return Err(FilterError::InvalidParameters("n_taps must be >= 3".into()));
    }
    let fc = cutoff_hz as f64 / sample_rate as f64;
    let half = (n_taps as f64 - 1.0) / 2.0;
    let win = make_window(window, n_taps);
    let mut coeffs: Vec<f64> = (0..n_taps)
        .map(|i| {
            let t = i as f64 - half;
            let sinc = if t.abs() < 1e-12 {
                2.0 * fc
            } else {
                (2.0 * PI * fc * t).sin() / (PI * t)
            };
            sinc * win[i]
        })
        .collect();
    let sum: f64 = coeffs.iter().sum();
    if sum.abs() > 1e-12 {
        for c in &mut coeffs { *c /= sum; }
    }
    Ok(coeffs.iter().map(|&c| c as f32).collect())
}

// ── Internal ZPK representation ───────────────────────────────────────────────

#[derive(Debug, Clone)]
#[doc(hidden)]
pub struct Zpk {
    pub zeros: Vec<Complex64>,
    pub poles: Vec<Complex64>,
    pub gain: f64,
}


impl Zpk {
    fn new(zeros: Vec<Complex64>, poles: Vec<Complex64>, gain: f64) -> Self {
        Self { zeros, poles, gain }
    }
}

// ── Pre-warp ──────────────────────────────────────────────────────────────────

pub(crate) fn prewarp(freq_hz: f64, fs: f64) -> f64 {
    2.0 * fs * (PI * freq_hz / fs).tan()
}

// ── Analog prototypes ─────────────────────────────────────────────────────────

pub(crate) fn butter_analog(n: usize) -> Zpk {
    let poles: Vec<Complex64> = (0..n)
        .map(|k| {
            let theta = PI * (2 * k + n + 1) as f64 / (2 * n) as f64;
            Complex64::new(theta.cos(), theta.sin())
        })
        .collect();
    Zpk::new(vec![], poles, 1.0)
}

pub(crate) fn cheby1_analog(n: usize, ripple_db: f64) -> Zpk {
    let epsilon = (10.0_f64.powf(ripple_db / 10.0) - 1.0).sqrt();
    let mu = (1.0 / epsilon + (1.0 / (epsilon * epsilon) + 1.0).sqrt()).ln() / n as f64;
    let (sinh_mu, cosh_mu) = (mu.sinh(), mu.cosh());
    let poles: Vec<Complex64> = (0..n)
        .map(|k| {
            let theta = PI * (2 * k + 1) as f64 / (2 * n) as f64;
            Complex64::new(-sinh_mu * theta.sin(), cosh_mu * theta.cos())
        })
        .collect();
    let gain = if n % 2 == 0 { 10.0_f64.powf(-ripple_db / 20.0) } else { 1.0 };
    Zpk::new(vec![], poles, gain)
}

pub(crate) fn cheby2_analog(n: usize, atten_db: f64) -> Zpk {
    let epsilon = 1.0 / (10.0_f64.powf(atten_db / 10.0) - 1.0).sqrt();
    let mu = (1.0 / epsilon + (1.0 / (epsilon * epsilon) + 1.0).sqrt()).ln() / n as f64;
    let (sinh_mu, cosh_mu) = (mu.sinh(), mu.cosh());
    let cheb1_poles: Vec<Complex64> = (0..n)
        .map(|k| {
            let theta = PI * (2 * k + 1) as f64 / (2 * n) as f64;
            Complex64::new(-sinh_mu * theta.sin(), cosh_mu * theta.cos())
        })
        .collect();
    let poles: Vec<Complex64> = cheb1_poles.iter().map(|p| Complex64::new(1.0, 0.0) / p).collect();
    let zeros: Vec<Complex64> = (0..n)
        .filter_map(|k| {
            let c = (PI * (2 * k + 1) as f64 / (2 * n) as f64).cos();
            if c.abs() < 1e-12 { None } else { Some(Complex64::new(0.0, 1.0 / c)) }
        })
        .collect();
    Zpk::new(zeros, poles, 1.0)
}

pub(crate) fn bessel_analog(n: usize) -> Zpk {
    let poles: Vec<Complex64> = match n {
        1 => vec![c(-1.0, 0.0)],
        2 => vec![c(-1.5, 0.8660254038), c(-1.5, -0.8660254038)],
        3 => vec![
            c(-2.3221853547, 0.0),
            c(-1.8389073229, 1.7543809986),
            c(-1.8389073229, -1.7543809986),
        ],
        4 => vec![
            c(-2.8962404580, 0.8672341289), c(-2.8962404580, -0.8672341289),
            c(-2.1042071088, 2.6574180419), c(-2.1042071088, -2.6574180419),
        ],
        5 => vec![
            c(-3.6467385953, 0.0),
            c(-3.3519563992, 1.7426614162), c(-3.3519563992, -1.7426614162),
            c(-2.3246743032, 3.5710507407), c(-2.3246743032, -3.5710507407),
        ],
        6 => vec![
            c(-4.2483749263, 0.8675341289), c(-4.2483749263, -0.8675341289),
            c(-3.7357014097, 2.6262723114), c(-3.7357014097, -2.6262723114),
            c(-2.5159438962, 4.4926690704), c(-2.5159438962, -4.4926690704),
        ],
        7 => vec![
            c(-4.9717868585, 0.0),
            c(-4.7583673960, 1.7392213694), c(-4.7583673960, -1.7392213694),
            c(-4.0701329668, 3.5171025834), c(-4.0701329668, -3.5171025834),
            c(-2.6856768621, 5.4232380396), c(-2.6856768621, -5.4232380396),
        ],
        8 => vec![
            c(-5.5878426693, 0.8675341289), c(-5.5878426693, -0.8675341289),
            c(-5.2048375955, 2.6148910625), c(-5.2048375955, -2.6148910625),
            c(-4.3683630085, 4.4140527998), c(-4.3683630085, -4.4140527998),
            c(-2.8394322488, 6.3615480498), c(-2.8394322488, -6.3615480498),
        ],
        _ => panic!("Bessel filter: order must be 1–8, got {n}"),
    };
    let norm = [0.0, 1.0, 1.3617, 1.7557, 2.1139, 2.4274, 2.7034, 2.9517, 3.1796][n];
    let poles_norm: Vec<Complex64> = poles.iter().map(|p| p / norm).collect();
    Zpk::new(vec![], poles_norm, 1.0)
}

#[inline]
fn c(re: f64, im: f64) -> Complex64 { Complex64::new(re, im) }

// ── Analog frequency transforms ───────────────────────────────────────────────

pub(crate) fn lp_to_lp(mut zpk: Zpk, wc: f64) -> Zpk {
    let degree = zpk.poles.len() - zpk.zeros.len();
    zpk.poles.iter_mut().for_each(|p| *p *= wc);
    zpk.zeros.iter_mut().for_each(|z| *z *= wc);
    zpk.gain *= wc.powi(degree as i32);
    zpk
}

pub(crate) fn lp_to_hp(zpk: Zpk, wc: f64) -> Zpk {
    let degree = zpk.poles.len() - zpk.zeros.len();
    let new_poles: Vec<Complex64> = zpk.poles.iter().map(|p| wc / p).collect();
    let mut new_zeros: Vec<Complex64> = zpk.zeros.iter().map(|z| wc / z).collect();
    new_zeros.extend(std::iter::repeat(Complex64::new(0.0, 0.0)).take(degree));
    let gain_num: Complex64 = zpk.poles.iter().map(|p| -p).product();
    let gain_den: Complex64 = new_poles.iter().map(|p| -p).product();
    let new_gain = zpk.gain * gain_num.re / gain_den.re;
    Zpk::new(new_zeros, new_poles, new_gain)
}

pub(crate) fn lp_to_bp(zpk: Zpk, wl: f64, wh: f64) -> Zpk {
    let bw = wh - wl;
    let w0 = (wl * wh).sqrt();
    let w02 = w0 * w0;
    let degree = zpk.poles.len() - zpk.zeros.len();

    let mut new_poles = Vec::with_capacity(2 * zpk.poles.len());
    for &p in &zpk.poles {
        let half = Complex64::new(bw * 0.5, 0.0) * p;
        let disc = (half * half - w02).sqrt();
        new_poles.push(half + disc);
        new_poles.push(half - disc);
    }
    let mut new_zeros = Vec::with_capacity(2 * zpk.zeros.len() + degree);
    for &q in &zpk.zeros {
        let half = Complex64::new(bw * 0.5, 0.0) * q;
        let disc = (half * half - w02).sqrt();
        new_zeros.push(half + disc);
        new_zeros.push(half - disc);
    }
    for _ in 0..degree { new_zeros.push(Complex64::new(0.0, 0.0)); }
    Zpk::new(new_zeros, new_poles, zpk.gain * bw.powi(degree as i32))
}

pub(crate) fn lp_to_bs(zpk: Zpk, wl: f64, wh: f64) -> Zpk {
    let bw = wh - wl;
    let w0 = (wl * wh).sqrt();
    let w02 = w0 * w0;
    let degree = zpk.poles.len() - zpk.zeros.len();

    let mut new_poles = Vec::with_capacity(2 * zpk.poles.len());
    for &p in &zpk.poles {
        let half = Complex64::new(bw * 0.5, 0.0) / p;
        let disc = (half * half - w02).sqrt();
        new_poles.push(half + disc);
        new_poles.push(half - disc);
    }
    let mut new_zeros = Vec::with_capacity(2 * zpk.zeros.len() + 2 * degree);
    for &q in &zpk.zeros {
        let half = Complex64::new(bw * 0.5, 0.0) / q;
        let disc = (half * half - w02).sqrt();
        new_zeros.push(half + disc);
        new_zeros.push(half - disc);
    }
    for _ in 0..degree {
        new_zeros.push(Complex64::new(0.0, w0));
        new_zeros.push(Complex64::new(0.0, -w0));
    }
    let gain_lp: Complex64 = zpk.poles.iter().map(|p| -p).product();
    let gain_bs: Complex64 = new_poles.iter().map(|p| -p).product();
    Zpk::new(new_zeros, new_poles, zpk.gain * (gain_lp / gain_bs).re)
}

// ── Bilinear transform ────────────────────────────────────────────────────────

pub(crate) fn bilinear_zpk(zpk: Zpk, fs: f64) -> Zpk {
    let two_fs = 2.0 * fs;
    let degree = zpk.poles.len() as isize - zpk.zeros.len() as isize;
    let dz: Vec<Complex64> = zpk.zeros.iter().map(|z| (two_fs + z) / (two_fs - z)).collect();
    let dp: Vec<Complex64> = zpk.poles.iter().map(|p| (two_fs + p) / (two_fs - p)).collect();
    let mut new_zeros = dz;
    for _ in 0..degree { new_zeros.push(Complex64::new(-1.0, 0.0)); }
    let gain_num: Complex64 = zpk.zeros.iter().map(|z| two_fs - z).product();
    let gain_den: Complex64 = zpk.poles.iter().map(|p| two_fs - p).product();
    let new_gain = zpk.gain * (gain_num / gain_den).re;
    Zpk::new(new_zeros, dp, new_gain)
}

// ── ZPK → SOS ─────────────────────────────────────────────────────────────────

pub(crate) fn zpk_to_sos(zpk: Zpk) -> Vec<SosSection> {
    let (mut real_poles, mut complex_poles) = split_real_complex(&zpk.poles);
    let (mut real_zeros, mut complex_zeros) = split_real_complex(&zpk.zeros);

    while real_zeros.len() + 2 * complex_zeros.len()
        < real_poles.len() + 2 * complex_poles.len()
    {
        real_zeros.push(0.0);
    }

    let mut sections: Vec<SosSection> = Vec::new();

    complex_poles.sort_by(|a, b| a.norm().partial_cmp(&b.norm()).unwrap());
    complex_zeros.sort_by(|a, b| a.norm().partial_cmp(&b.norm()).unwrap());

    while !complex_poles.is_empty() {
        let p = complex_poles.remove(0);
        if !complex_zeros.is_empty() {
            let idx = nearest_idx(&complex_zeros, p);
            let z = complex_zeros.remove(idx);
            sections.push(sos_from_complex_pair(p, z));
        } else if !real_zeros.is_empty() {
            let idx = nearest_real_idx(&real_zeros, p.re);
            let r = real_zeros.remove(idx);
            sections.push(sos_from_complex_pole_real_zero(p, r, &mut real_zeros));
        } else {
            sections.push(sos_from_complex_pole_only(p));
        }
    }

    real_poles.sort_by(|a, b| a.abs().partial_cmp(&b.abs()).unwrap());
    real_zeros.sort_by(|a, b| a.abs().partial_cmp(&b.abs()).unwrap());

    while !real_poles.is_empty() {
        let p1 = real_poles.remove(0);
        let z1 = if !real_zeros.is_empty() { real_zeros.remove(0) } else { 0.0 };
        if !real_poles.is_empty() {
            let p2 = real_poles.remove(0);
            let z2 = if !real_zeros.is_empty() { real_zeros.remove(0) } else { 0.0 };
            sections.push(sos_from_real_pairs(p1, p2, z1, z2));
        } else {
            sections.push(SosSection::first_order(1.0, -z1, -p1));
        }
    }

    if !sections.is_empty() {
        sections[0].b[0] *= zpk.gain;
        sections[0].b[1] *= zpk.gain;
        sections[0].b[2] *= zpk.gain;
    }
    sections
}

// ── High-level SOS design entry point ────────────────────────────────────────

pub(crate) fn design_sos(prototype: Zpk, response: FilterResponse, fs: f64) -> Vec<SosSection> {
    let zpk = match response {
        FilterResponse::LowPass { cutoff } => {
            bilinear_zpk(lp_to_lp(prototype, prewarp(cutoff, fs)), fs)
        }
        FilterResponse::HighPass { cutoff } => {
            bilinear_zpk(lp_to_hp(prototype, prewarp(cutoff, fs)), fs)
        }
        FilterResponse::BandPass { low, high } => {
            bilinear_zpk(lp_to_bp(prototype, prewarp(low, fs), prewarp(high, fs)), fs)
        }
        FilterResponse::BandStop { low, high } => {
            bilinear_zpk(lp_to_bs(prototype, prewarp(low, fs), prewarp(high, fs)), fs)
        }
    };
    let mut sos = zpk_to_sos(zpk);
    normalise_sos_gain(&mut sos, response, fs);
    sos
}

/// Normalise the overall passband gain to unity by evaluating H(z) at the
/// characteristic passband frequency and scaling the first section.
///
/// Evaluation points:
/// - LowPass / BandStop : z = 1        (DC, always in passband)
/// - HighPass            : z = −1       (Nyquist, always in passband)
/// - BandPass            : z = e^{jω₀} where ω₀ = 2π√(f_low·f_high)/fs
///   (geometric centre of the passband, correct after bilinear warp)
fn normalise_sos_gain(sections: &mut [SosSection], response: FilterResponse, fs: f64) {
    let z: Complex64 = match response {
        FilterResponse::LowPass { .. } | FilterResponse::BandStop { .. } => {
            Complex64::new(1.0, 0.0)
        }
        FilterResponse::HighPass { .. } => {
            Complex64::new(-1.0, 0.0)
        }
        FilterResponse::BandPass { low, high } => {
            // Use the bilinear-warped geometric centre so the evaluation point
            // lands at the true passband peak after the frequency mapping.
            let w_low = (PI * low / fs).tan();
            let w_high = (PI * high / fs).tan();
            let omega0 = 2.0 * (w_low * w_high).sqrt().atan();
            Complex64::new(omega0.cos(), omega0.sin())
        }
    };
    let h: Complex64 = sections.iter().map(|s| {
        let num = s.b[0] + s.b[1] / z + s.b[2] / (z * z);
        let den = 1.0 + s.a[0] / z + s.a[1] / (z * z);
        num / den
    }).product();
    let scale = 1.0 / h.norm();
    if scale.is_finite() && scale > 0.0 {
        sections[0].b[0] *= scale;
        sections[0].b[1] *= scale;
        sections[0].b[2] *= scale;
    }
}

// ── SOS coefficient helpers ───────────────────────────────────────────────────

fn sos_from_complex_pair(pole: Complex64, zero: Complex64) -> SosSection {
    SosSection::second_order(1.0, -2.0 * zero.re, zero.norm_sqr(), -2.0 * pole.re, pole.norm_sqr())
}

fn sos_from_complex_pole_real_zero(pole: Complex64, z1: f64, real_zeros: &mut Vec<f64>) -> SosSection {
    let z2 = if !real_zeros.is_empty() { real_zeros.remove(0) } else { 0.0 };
    SosSection::second_order(1.0, -(z1 + z2), z1 * z2, -2.0 * pole.re, pole.norm_sqr())
}

fn sos_from_complex_pole_only(pole: Complex64) -> SosSection {
    SosSection::second_order(1.0, 0.0, 0.0, -2.0 * pole.re, pole.norm_sqr())
}

fn sos_from_real_pairs(p1: f64, p2: f64, z1: f64, z2: f64) -> SosSection {
    SosSection::second_order(1.0, -(z1 + z2), z1 * z2, -(p1 + p2), p1 * p2)
}

fn split_real_complex(vals: &[Complex64]) -> (Vec<f64>, Vec<Complex64>) {
    const EPS: f64 = 1e-8;
    let mut real = Vec::new();
    let mut cplx = Vec::new();
    let mut used = vec![false; vals.len()];
    for i in 0..vals.len() {
        if used[i] { continue; }
        if vals[i].im.abs() < EPS {
            real.push(vals[i].re);
            used[i] = true;
        } else {
            for j in (i + 1)..vals.len() {
                if !used[j]
                    && (vals[j].re - vals[i].re).abs() < EPS
                    && (vals[j].im + vals[i].im).abs() < EPS
                {
                    cplx.push(vals[i]);
                    used[i] = true;
                    used[j] = true;
                    break;
                }
            }
            if !used[i] {
                real.push(vals[i].re);
                used[i] = true;
            }
        }
    }
    (real, cplx)
}

fn nearest_idx(vals: &[Complex64], target: Complex64) -> usize {
    vals.iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| (*a - target).norm().partial_cmp(&(*b - target).norm()).unwrap())
        .map(|(i, _)| i)
        .unwrap_or(0)
}

fn nearest_real_idx(vals: &[f64], target: f64) -> usize {
    vals.iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| (*a - target).abs().partial_cmp(&(*b - target).abs()).unwrap())
        .map(|(i, _)| i)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nyquist_rejects_zero() { assert!(validate_nyquist(0.0, 40_000.0).is_err()); }
    #[test]
    fn nyquist_rejects_at_nyquist() { assert!(validate_nyquist(20_000.0, 40_000.0).is_err()); }
    #[test]
    fn nyquist_accepts_valid() { assert!(validate_nyquist(1_000.0, 40_000.0).is_ok()); }

    #[test]
    fn butter2_poles_on_unit_circle() {
        let zpk = butter_analog(2);
        for p in &zpk.poles {
            assert!((p.norm() - 1.0).abs() < 1e-10);
        }
    }

    #[test]
    fn sinc_coeffs_dc_gain() {
        let c = generate_sinc_coeffs(1_000.0, 40_000.0, 51, WindowType::Hann).unwrap();
        let sum: f32 = c.iter().sum();
        assert!((sum - 1.0).abs() < 1e-5);
    }

    #[test]
    fn sinc_coeffs_symmetric() {
        let c = generate_sinc_coeffs(1_000.0, 40_000.0, 51, WindowType::Hamming).unwrap();
        for i in 0..25 {
            assert!((c[i] - c[50 - i]).abs() < 1e-6);
        }
    }
}
