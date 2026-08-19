//! Digital filter bank — IIR (SOS), FIR, and filter design utilities.
//!
//! All functions operate on **channel-major** `[f32]` buffers identical to
//! the rest of `dsp-core`: channel `c` occupies indices
//! `c * n_samples .. (c+1) * n_samples`.
//!
//! Multi-channel entry points use **Rayon** to process channels in parallel.
//! Batch-level parallelism belongs in `dsp-io` via Tokio; this crate handles
//! only the per-batch math.
//!
//! ## IIR design API
//!
//! Use [`iir::butterworth`], [`iir::chebyshev1`], [`iir::chebyshev2`],
//! [`iir::bessel`], [`iir::notch`], and [`iir::peak_eq`] to design filters
//! from scratch (no pre-computed SciPy rows needed).
//!
//! ## Batch processing surplus
//!
//! | Filter type | Required surplus (each side) |
//! |-------------|------------------------------|
//! | IIR `sosfilt` | `6 * n_sections` |
//! | IIR `sosfiltfilt` (zero-phase) | `6 * n_sections` |
//! | FIR convolution, centered | `(n_taps - 1) / 2` |
//! | FIR convolution, causal | `n_taps - 1` (left side only) |
//!
//! ## Nyquist enforcement
//!
//! Every frequency-domain design function calls [`design::validate_nyquist`]
//! and returns a [`design::FilterError`] when the cutoff violates the
//! Nyquist criterion (`0 < cutoff_hz < sample_rate / 2`).

pub mod biquad;
pub mod design;
pub mod fir;
pub mod iir;
pub mod window;

pub use design::{FilterError, FilterResponse, generate_sinc_coeffs, validate_nyquist};
pub use window::WindowType;
#[allow(deprecated)]
pub use iir::{FilterDesign, SosFilter, SosSection, butterworth, chebyshev1, chebyshev2, bessel, notch, peak_eq};
pub use fir::{
    exponential_moving_average, filter_channels_fir, median_filter, moving_average, moving_rms,
};
