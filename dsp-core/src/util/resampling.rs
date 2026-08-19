//! Min-Max peak resampling for high-fidelity waveform visualization.
//!
//! When displaying a recording at a zoom level where many raw samples map to
//! a single screen pixel, rendering every sample is redundant and slow.
//! Instead, we compute a **Min-Max pair** per pixel window: the minimum and
//! maximum values seen in that window. Drawing both preserves every signal
//! spike visually, regardless of the decimation ratio.

use rayon::prelude::*;

/// A `(min, max)` envelope pair representing a decimated time window.
#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
pub struct Peak {
    pub min: f32,
    pub max: f32,
}

/// Computes min-max peaks from a channel-major buffer using parallel channel processing.
///
/// # Arguments
/// * `data` — flat `[Channels × Samples]` slice. Samples for channel `c` start at
///   `c * (data.len() / channels)`.
/// * `channels` — number of channels encoded in `data`.
/// * `decimation_ratio` — number of raw samples collapsed into each `Peak`.
///   `data.len() / channels` must be divisible by `decimation_ratio`.
///
/// # Returns
/// A `Vec<Peak>` of length `channels * (samples_per_channel / decimation_ratio)`.
/// Peaks are stored channel-major: all peaks for channel 0 first, then channel 1, etc.
///
/// # Panics
/// Does not panic, but will silently truncate if `samples_per_channel` is not divisible
/// by `decimation_ratio` (the last partial window is dropped).
pub fn generate_peaks_parallel(
    data: &[f32],
    channels: usize,
    decimation_ratio: usize,
) -> Vec<Peak> {
    assert!(decimation_ratio > 0, "decimation_ratio must be > 0");
    let samples_per_channel = data.len() / channels;
    let peaks_per_channel = samples_per_channel / decimation_ratio;
    if peaks_per_channel == 0 {
        return Vec::new();
    }
    let mut peaks = vec![Peak::default(); channels * peaks_per_channel];

    peaks
        .par_chunks_mut(peaks_per_channel)
        .enumerate()
        .for_each(|(c, channel_peaks)| {
            let channel_offset = c * samples_per_channel;
            let channel_data = &data[channel_offset..channel_offset + samples_per_channel];
            for p in 0..peaks_per_channel {
                let start = p * decimation_ratio;
                let end = start + decimation_ratio;
                let window = &channel_data[start..end];
                let mut min = f32::MAX;
                let mut max = f32::MIN;
                for &val in window {
                    if val < min { min = val; }
                    if val > max { max = val; }
                }
                channel_peaks[p] = Peak { min, max };
            }
        });

    peaks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_channel_basic_minmax() {
        // [1, -2, 5, 0,  10, -10, 3, 2]  ratio=4
        // Window 0: min=-2, max=5
        // Window 1: min=-10, max=10
        let data = vec![1.0f32, -2.0, 5.0, 0.0, 10.0, -10.0, 3.0, 2.0];
        let peaks = generate_peaks_parallel(&data, 1, 4);
        assert_eq!(peaks.len(), 2);
        assert_eq!(peaks[0].min, -2.0);
        assert_eq!(peaks[0].max, 5.0);
        assert_eq!(peaks[1].min, -10.0);
        assert_eq!(peaks[1].max, 10.0);
    }

    #[test]
    fn multichannel_independent_peaks() {
        // 2 channels x 8 samples, ratio=4 -> 2 peaks per channel
        let mut data = vec![0.0f32; 16];
        for i in 0..8  { data[i] = 1.0; }  // ch0: all 1.0
        for i in 8..16 { data[i] = -1.0; } // ch1: all -1.0
        let peaks = generate_peaks_parallel(&data, 2, 4);
        assert_eq!(peaks.len(), 4); // 2 channels * 2 peaks each
        // ch0 peaks
        assert_eq!(peaks[0].min, 1.0);
        assert_eq!(peaks[0].max, 1.0);
        assert_eq!(peaks[1].min, 1.0);
        assert_eq!(peaks[1].max, 1.0);
        // ch1 peaks
        assert_eq!(peaks[2].min, -1.0);
        assert_eq!(peaks[2].max, -1.0);
        assert_eq!(peaks[3].min, -1.0);
        assert_eq!(peaks[3].max, -1.0);
    }

    #[test]
    fn ratio_one_returns_sample_as_both_min_and_max() {
        let data = vec![3.0f32, 7.0, -1.0, 5.0];
        let peaks = generate_peaks_parallel(&data, 1, 1);
        assert_eq!(peaks.len(), 4);
        assert_eq!(peaks[0].min, 3.0);
        assert_eq!(peaks[0].max, 3.0);
        assert_eq!(peaks[2].min, -1.0);
        assert_eq!(peaks[2].max, -1.0);
    }

    #[test]
    fn uniform_signal_min_equals_max() {
        let data = vec![2.5f32; 16]; // 1 channel, 16 samples, all 2.5
        let peaks = generate_peaks_parallel(&data, 1, 4);
        assert_eq!(peaks.len(), 4);
        for p in &peaks {
            assert_eq!(p.min, 2.5);
            assert_eq!(p.max, 2.5);
        }
    }

    #[test]
    fn peak_preserves_extreme_spike() {
        // A single spike at position 3 in a 8-sample window, ratio=8
        let mut data = vec![0.0f32; 8];
        data[3] = 100.0;
        let peaks = generate_peaks_parallel(&data, 1, 8);
        assert_eq!(peaks.len(), 1);
        assert_eq!(peaks[0].max, 100.0);
        assert_eq!(peaks[0].min, 0.0);
    }

    #[test]
    fn output_length_matches_expected_formula() {
        let channels = 4;
        let samples_per_ch = 128;
        let ratio = 16;
        let data = vec![0.0f32; channels * samples_per_ch];
        let peaks = generate_peaks_parallel(&data, channels, ratio);
        assert_eq!(peaks.len(), channels * (samples_per_ch / ratio));
    }
}
