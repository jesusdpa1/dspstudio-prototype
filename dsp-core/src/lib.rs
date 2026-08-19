//! # dsp-core — Pure DSP Primitives
//!
//! Zero-UI, zero-IO library crate. Every function in this crate operates on
//! plain `Vec<f32>` or `&[f32]` slices and has no dependency on file formats,
//! async runtimes, or graphics. This makes it easy to test headlessly and reuse
//! from any context (`dsp-io`, `dsp-app`, Python via PyO3, WASM, etc.).
//!
//! ---
//!
//! ## Module overview
//!
//! | Module | What's in it |
//! |--------|-------------|
//! | [`mock::signal`] | Deterministic signal generators ([`mock::signal::SineWave`], [`mock::signal::WhiteNoise`]) |
//! | [`math::arithmetic`] | In-place scalar math on channel-major buffers ([`math::arithmetic::add_scalar`], [`math::arithmetic::mul_scalar`]) |
//! | [`util::resampling`] | Min-max peak decimation for LOD visualization ([`util::resampling::generate_peaks_parallel`]) |
//! | [`detection`] | Sparse event detection (Threshold, Hysteresis, Window) |
//! | [`filter`] | Comprehensive bank of FIR and IIR filters |
//! | `spectral` | _(reserved — FFT/STFT, not yet implemented)_ |
//! | `math::statistics` | _(reserved — RMS, variance, etc., not yet implemented)_ |
//!
//! ---
//!
//! ## Data layout convention
//!
//! All multi-channel buffers use **channel-major** layout (identical to NumPy
//! C-order with shape `[channels, samples]`):
//!
//! ```text
//! buffer = [ch0_s0, ch0_s1, …, ch0_sN,  ch1_s0, ch1_s1, …, ch1_sN,  …]
//!           ─────── channel 0 ──────────  ──────── channel 1 ──────────
//! ```
//!
//! Functions that operate on a subset of channels accept a `channel_mask: &[u16]`
//! and a `total_channels: usize` parameter so they can compute per-channel
//! offsets without allocating.
//!
//! ---
//!
//! ## Quick start
//!
//! ### Generate a test signal
//! ```rust
//! use dsp_core::mock::signal::{SignalGenerator, SineWave};
//!
//! let mut wave = SineWave::new(440.0, 40_000.0, 1.0);
//! let mut buf = vec![0.0f32; 40_000];
//! wave.fill_buffer(&mut buf, 1);
//! assert!(buf.iter().all(|&v| v.abs() <= 1.0 + 1e-5));
//! ```
//!
//! ### Apply in-place gain to selected channels
//!
//! ```rust
//! use dsp_core::math::arithmetic::mul_scalar;
//!
//! // 2 channels × 4 samples (channel-major layout)
//! let mut data = vec![
//!     1.0f32, 1.0, 1.0, 1.0,  // channel 0
//!     2.0f32, 2.0, 2.0, 2.0,  // channel 1
//! ];
//! // Double channel 0 only.
//! mul_scalar(&mut data, 2.0, &[0u16], 2);
//! assert_eq!(&data[0..4], &[2.0, 2.0, 2.0, 2.0]);
//! assert_eq!(&data[4..8], &[2.0, 2.0, 2.0, 2.0]); // channel 1 unchanged
//! ```
//!
//! ### Compute min-max peaks for waveform visualization
//!
//! ```rust
//! use dsp_core::util::resampling::generate_peaks_parallel;
//!
//! // 1 channel × 400 samples → 10 peaks (decimation_ratio = 40)
//! let data: Vec<f32> = (0..400).map(|i| i as f32).collect();
//! let peaks = generate_peaks_parallel(&data, 1, 40);
//! assert_eq!(peaks.len(), 10);
//! assert_eq!(peaks[0].min, 0.0);
//! assert_eq!(peaks[0].max, 39.0);
//! ```

pub mod mock;
pub mod signal;
pub mod math;
pub mod spectral;
pub mod filter;
pub mod util;
pub mod detection;
pub mod spatial;
pub mod extraction;
