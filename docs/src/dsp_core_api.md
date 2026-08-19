# dsp-core API

`dsp-core` is the pure DSP primitives crate. It has no dependencies on file I/O, async runtimes, or GUI frameworks. Every function operates on plain `&[f32]` or `Vec<f32]` slices in **channel-major** layout.

---

## Data layout convention

All multi-channel buffers use **channel-major** layout:

```text
buffer = [ch0_s0, ch0_s1, …, ch0_sN,  ch1_s0, ch1_s1, …, ch1_sN,  …]
          ─────── channel 0 ──────────  ──────── channel 1 ──────────
```

This matches the Zarr `/raw` array's in-memory representation and NumPy's C-order with shape `[channels, samples]`.

Functions that operate on a subset of channels accept:
- `channel_mask: &[u16]` — which channel indices to modify
- `total_channels: usize` — total number of channels encoded in the buffer

---

## `mock::signal` — Deterministic signal generators

All generators implement the `SignalGenerator` trait:

```rust
pub trait SignalGenerator {
    fn fill_buffer(&mut self, buffer: &mut [f32], channels: usize);
}
```

Each call advances the generator's internal phase, producing a continuous stream across successive calls.

### `SineWave`

```rust
use dsp_core::mock::signal::{SignalGenerator, SineWave};

let mut gen = SineWave::new(
    /*frequency=*/   440.0,     // Hz
    /*sample_rate=*/ 40_000.0,  // Hz
    /*amplitude=*/   1.0,
);

let mut buf = vec![0.0f32; 40_000]; // 1 channel × 1 second
gen.fill_buffer(&mut buf, 1);
```

### `WhiteNoise`

Deterministic Xorshift PRNG — identical output for the same seed across platforms and compiler versions.

```rust
use dsp_core::mock::signal::{SignalGenerator, WhiteNoise};

let mut noise = WhiteNoise::new(/*seed=*/ 42, /*amplitude=*/ 0.1);
let mut buf   = vec![0.0f32; 40_000];
noise.fill_buffer(&mut buf, 1);
```

---

## `math::arithmetic` — In-place channel-masked operations

All functions mutate the buffer in place and skip channels not in the mask.

### `add_scalar` / `mul_scalar`

```rust
use dsp_core::math::arithmetic::{add_scalar, mul_scalar};

// 2 channels × 4 samples
let mut data = vec![1.0f32; 8];
// Add 10.0 only to channel 1.
add_scalar(&mut data, 10.0, &[1u16], /*total_channels=*/ 2);
```

---

## `filter` — Digital Filters

Comprehensive bank of FIR and IIR filters.

### IIR Filters (SOS)

Designed via Butterworth, Chebyshev, or Bessel prototypes. Supports causal (`apply`) and zero-phase (`apply_filtfilt`) filtering.

```rust
use dsp_core::filter::{butterworth, FilterResponse};

let filter = butterworth(
    /*order=*/ 4, 
    FilterResponse::LowPass { cutoff: 1000.0 }, 
    /*fs=*/ 40000.0
);

let filtered = filter.apply_f32(&raw_data);
```

### FIR Filters

Automatic selection between direct convolution (for short kernels) and FFT-based overlap-add (for long kernels).

```rust
use dsp_core::filter::filter_channels_fir;

let coeffs = [0.1, 0.2, 0.4, 0.2, 0.1];
let filtered = filter_channels_fir(&data, n_channels, &coeffs, /*center=*/ true);
```

---

## `detection` — Sparse Event Detection

High-performance, parallelized detectors for finding triggers in dense waveforms.

### `SingleThresholdDetector`

Triggers when a signal crosses a fixed value. Supports positive, negative, or both directions.

```rust
use dsp_core::detection::single::SingleThresholdDetector;
use dsp_core::detection::{CrossingDirection, DetectionDetector};

let detector = SingleThresholdDetector::new(
    /*threshold=*/ 0.5,
    CrossingDirection::Positive,
    /*refractory=*/ 40, // samples
    /*label_pos=*/ 1,
    /*label_neg=*/ 2,
);

let events = detector.detect(&data, n_channels, /*start_sample=*/ 0);
// Returns Vec<DetectedEvent> where each event is [sample, channel, label]
```

### `DoubleThresholdDetector`

Supports **Hysteresis** (Schmitt trigger) and **Window** (enter/exit) modes.

```rust
use dsp_core::detection::double::{DoubleThresholdDetector, DoubleThresholdMode};

let detector = DoubleThresholdDetector::new(
    /*low=*/  -0.2,
    /*high=*/  0.8,
    DoubleThresholdMode::Hysteresis,
    /*refractory=*/ 20,
    /*label_high=*/ 10,
    /*label_low=*/  20,
);
```

---

## `util::resampling` — Min-max peak decimation

Used to produce LOD data for waveform rendering. Drawing min-max pairs preserves every signal spike visually regardless of zoom level.

```rust
use dsp_core::util::resampling::generate_peaks_parallel;

let peaks = generate_peaks_parallel(&data, n_channels, /*ratio=*/ 40);
```

---

## Writing a new `dsp-core` module

1. Add a file in `dsp-core/src/<module>.rs`.
2. Declare it in `dsp-core/src/lib.rs` with `pub mod <module>;`.
3. Use **Rayon** for multi-channel parallelism (`into_par_iter`).
4. Keep functions pure (no I/O) and operate on f32/f64 slices.
