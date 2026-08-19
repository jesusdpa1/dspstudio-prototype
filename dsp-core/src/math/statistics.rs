//! Statistical analysis kernels (Mean, RMS, Variance, etc).
//!
//! All functions operate on a flat `[Channels × Samples]` layout where
//! channel `c` occupies indices `c * samples_per_channel ..(c+1) * samples_per_channel`.

use rayon::prelude::*;

/// Computes the arithmetic mean for the selected channels.
///
/// Returns a `Vec<f32>` where the index corresponds to the index in `channel_mask`.
pub fn compute_means(buffer: &[f32], channel_mask: &[u16], total_channels: usize) -> Vec<f32> {
    if buffer.is_empty() || total_channels == 0 {
        return vec![0.0; channel_mask.len()];
    }
    let samples_per_channel = buffer.len() / total_channels;

    channel_mask.par_iter().map(|&ch| {
        let start = ch as usize * samples_per_channel;
        let end = start + samples_per_channel;
        if end <= buffer.len() {
            let sum: f64 = buffer[start..end].iter().map(|&v| v as f64).sum();
            (sum / samples_per_channel as f64) as f32
        } else {
            0.0
        }
    }).collect()
}

/// Computes the variance and standard deviation for the selected channels.
///
/// `means` must match the order and length of `channel_mask`.
/// Returns `(variances, std_devs)`.
pub fn compute_variance_std(
    buffer: &[f32],
    means: &[f32],
    channel_mask: &[u16],
    total_channels: usize,
) -> (Vec<f32>, Vec<f32>) {
    assert!(
        means.len() == channel_mask.len(),
        "means.len() ({}) must equal channel_mask.len() ({})",
        means.len(),
        channel_mask.len()
    );
    if buffer.is_empty() || total_channels == 0 {
        return (vec![0.0; channel_mask.len()], vec![0.0; channel_mask.len()]);
    }
    let samples_per_channel = buffer.len() / total_channels;

    channel_mask.par_iter().enumerate().map(|(i, &ch)| {
        let start = ch as usize * samples_per_channel;
        let end = start + samples_per_channel;
        let mean = means[i] as f64;

        if end <= buffer.len() {
            let mut sum_sq_diff: f64 = 0.0;
            for &val in &buffer[start..end] {
                let diff = val as f64 - mean;
                sum_sq_diff += diff * diff;
            }
            let variance = (sum_sq_diff / samples_per_channel as f64) as f32;
            (variance, variance.sqrt())
        } else {
            (0.0, 0.0)
        }
    }).unzip()
}

/// Computes the minimum and maximum values for the selected channels.
///
/// Returns a `Vec<(min, max)>`.
pub fn compute_min_max(
    buffer: &[f32],
    channel_mask: &[u16],
    total_channels: usize,
) -> Vec<(f32, f32)> {
    let samples_per_channel = buffer.len() / total_channels;

    channel_mask.par_iter().map(|&ch| {
        let start = ch as usize * samples_per_channel;
        let end = start + samples_per_channel;
        if end <= buffer.len() && samples_per_channel > 0 {
            let mut min = f32::INFINITY;
            let mut max = f32::NEG_INFINITY;
            for &val in &buffer[start..end] {
                if val < min { min = val; }
                if val > max { max = val; }
            }
            (min, max)
        } else {
            (0.0, 0.0)
        }
    }).collect()
}

/// Computes the median for the selected channels.
/// 
/// Note: This is an expensive operation as it requires a partial sort of the data.
pub fn compute_medians(buffer: &[f32], channel_mask: &[u16], total_channels: usize) -> Vec<f32> {
    if buffer.is_empty() || total_channels == 0 {
        return vec![0.0; channel_mask.len()];
    }
    let samples_per_channel = buffer.len() / total_channels;

    channel_mask.par_iter().map(|&ch| {
        let start = ch as usize * samples_per_channel;
        let end = start + samples_per_channel;
        if end <= buffer.len() && samples_per_channel > 0 {
            let mut channel_data = buffer[start..end].to_vec();
            let mid = channel_data.len() / 2;
            let (_, &mut median, _) = channel_data.select_nth_unstable_by(mid, |a, b| a.total_cmp(b));
            median
        } else {
            0.0
        }
    }).collect()
}

/// Computes the Median Absolute Deviation (MAD) for the selected channels.
/// 
/// `MAD = median(abs(x - median(x)))`
/// 
/// This is the robust SpikeInterface standard for noise estimation.
/// Typically, the standard deviation is estimated as `1.4826 * MAD`.
pub fn compute_mads(
    buffer: &[f32],
    medians: &[f32],
    channel_mask: &[u16],
    total_channels: usize,
) -> Vec<f32> {
    assert!(
        medians.len() == channel_mask.len(),
        "medians.len() ({}) must equal channel_mask.len() ({})",
        medians.len(),
        channel_mask.len()
    );
    if buffer.is_empty() || total_channels == 0 {
        return vec![0.0; channel_mask.len()];
    }
    let samples_per_channel = buffer.len() / total_channels;

    channel_mask.par_iter().enumerate().map(|(i, &ch)| {
        let start = ch as usize * samples_per_channel;
        let end = start + samples_per_channel;
        let median = medians[i];

        if end <= buffer.len() && samples_per_channel > 0 {
            let mut abs_diffs: Vec<f32> = buffer[start..end]
                .iter()
                .map(|&v| (v - median).abs())
                .collect();
            let mid = abs_diffs.len() / 2;
            let (_, &mut mad, _) = abs_diffs.select_nth_unstable_by(mid, |a, b| a.total_cmp(b));
            mad
        } else {
            0.0
        }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_compute_medians() {
        let data = vec![1.0, 5.0, 2.0, 4.0, 3.0]; // Sorted: 1, 2, 3, 4, 5 -> Median 3
        let medians = compute_medians(&data, &[0], 1);
        assert_eq!(medians[0], 3.0);
    }

    #[test]
    fn test_compute_mads() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0]; // Median = 3
        // abs_diffs = [2, 1, 0, 1, 2] -> Sorted: 0, 1, 1, 2, 2 -> Median = 1
        let mads = compute_mads(&data, &[3.0], &[0], 1);
        assert_eq!(mads[0], 1.0);
    }

    #[test]
    fn test_compute_means() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 10.0, 20.0, 30.0, 40.0];
        let means = crate::math::statistics::compute_means(&data, &[0, 1], 2);
        assert_eq!(means.len(), 2);
        assert_relative_eq!(means[0], 2.5);
        assert_relative_eq!(means[1], 25.0);
    }

    #[test]
    fn test_compute_variance_std() {
        let data = vec![1.0, 2.0, 3.0, 4.0]; // Mean = 2.5
        let (vars, stds) = crate::math::statistics::compute_variance_std(&data, &[2.5], &[0], 1);
        assert_relative_eq!(vars[0], 1.25);
        assert_relative_eq!(stds[0], 1.25f32.sqrt());
    }
}
