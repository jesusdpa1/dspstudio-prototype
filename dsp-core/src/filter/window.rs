use std::f64::consts::PI;

/// Window function used for FIR design and spectral analysis.
///
/// Extends the previous `Hann/Hamming/Blackman/Bartlett/Rectangular` set with
/// `BlackmanHarris`, `FlatTop`, and a `Kaiser { beta }` window that trades off
/// main-lobe width against sidelobe attenuation via a single parameter.
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub enum WindowType {
    #[default]
    Hann,
    Hamming,
    Blackman,
    Bartlett,
    Rectangular,
    BlackmanHarris,
    FlatTop,
    Kaiser { beta: f64 },
}

/// Build a symmetric window of length `n`.
pub fn make_window(kind: WindowType, n: usize) -> Vec<f64> {
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![1.0];
    }
    let m = (n - 1) as f64;
    match kind {
        WindowType::Rectangular => vec![1.0; n],

        WindowType::Hann => (0..n)
            .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f64 / m).cos()))
            .collect(),

        WindowType::Hamming => (0..n)
            .map(|i| 0.54 - 0.46 * (2.0 * PI * i as f64 / m).cos())
            .collect(),

        WindowType::Blackman => (0..n)
            .map(|i| {
                let t = 2.0 * PI * i as f64 / m;
                0.42 - 0.50 * t.cos() + 0.08 * (2.0 * t).cos()
            })
            .collect(),

        WindowType::BlackmanHarris => (0..n)
            .map(|i| {
                let t = 2.0 * PI * i as f64 / m;
                0.35875 - 0.48829 * t.cos() + 0.14128 * (2.0 * t).cos()
                    - 0.01168 * (3.0 * t).cos()
            })
            .collect(),

        WindowType::Bartlett => (0..n)
            .map(|i| {
                let half = m / 2.0;
                1.0 - (i as f64 - half).abs() / half
            })
            .collect(),

        WindowType::FlatTop => (0..n)
            .map(|i| {
                let t = 2.0 * PI * i as f64 / m;
                0.215_578_95
                    - 0.416_631_58 * t.cos()
                    + 0.277_263_16 * (2.0 * t).cos()
                    - 0.083_578_95 * (3.0 * t).cos()
                    + 0.006_610_53 * (4.0 * t).cos()
            })
            .collect(),

        WindowType::Kaiser { beta } => {
            let half = m / 2.0;
            let i0_b = bessel_i0(beta);
            (0..n)
                .map(|i| {
                    let ratio = (i as f64 - half) / half;
                    let arg = beta * (1.0 - ratio * ratio).max(0.0).sqrt();
                    bessel_i0(arg) / i0_b
                })
                .collect()
        }
    }
}

/// Estimate the Kaiser β from a desired minimum stopband attenuation (Harris 1978).
pub fn kaiser_beta_from_attenuation(atten_db: f64) -> f64 {
    if atten_db < 21.0 {
        0.0
    } else if atten_db <= 50.0 {
        let a = atten_db - 21.0;
        0.5842 * a.powf(0.4) + 0.07886 * a
    } else {
        0.1102 * (atten_db - 8.7)
    }
}

/// Minimum Kaiser window length for a normalised transition bandwidth `delta_f`.
pub fn kaiser_min_length(atten_db: f64, delta_f: f64) -> usize {
    let n = (atten_db - 8.0) / (2.285 * 2.0 * PI * delta_f);
    (n.ceil() as usize) | 1 // force odd
}

/// Modified Bessel function I₀(x) (accurate to ~1 ulp for |x| ≤ 700).
pub(crate) fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0_f64;
    let mut term = 1.0_f64;
    let x2 = (x * 0.5) * (x * 0.5);
    for k in 1_u32..=40 {
        term *= x2 / (k * k) as f64;
        sum += term;
        if term.abs() < 1e-20 * sum.abs() {
            break;
        }
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hann_sum() {
        let w = make_window(WindowType::Hann, 64);
        let s: f64 = w.iter().sum();
        assert!((s - 31.5).abs() < 0.01, "Hann sum = {s}");
    }

    #[test]
    fn kaiser_b0_rectangular() {
        let r = make_window(WindowType::Rectangular, 64);
        let k = make_window(WindowType::Kaiser { beta: 0.0 }, 64);
        for (a, b) in r.iter().zip(k.iter()) {
            assert!((a - b).abs() < 1e-10);
        }
    }

    #[test]
    fn kaiser_beta_empirical() {
        assert!((kaiser_beta_from_attenuation(60.0) - 5.653).abs() < 0.01);
    }
}
