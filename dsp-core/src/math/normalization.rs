//! In-place normalization kernels (Z-Score, Min-Max).
//!
//! All functions operate on a flat `[Channels × Samples]` layout where
//! channel `c` occupies indices `c * samples_per_channel ..(c+1) * samples_per_channel`.

/// Standardizes selected channels to zero mean and unit variance (Z-Score).
///
/// `(x - mean) / std_dev`
///
/// `means` and `std_devs` must match the order and length of `channel_mask`.
/// If `std_dev` is near zero, the channel is zeroed.
pub fn normalize_zscore(
    buffer: &mut [f32],
    means: &[f32],
    std_devs: &[f32],
    channel_mask: &[u16],
    total_channels: usize,
) {
    let samples_per_channel = buffer.len() / total_channels;
    for (i, &ch) in channel_mask.iter().enumerate() {
        let start = ch as usize * samples_per_channel;
        let end = start + samples_per_channel;
        let mean = means[i];
        let std = std_devs[i];

        if end <= buffer.len() {
            if std.abs() < f32::EPSILON {
                for val in &mut buffer[start..end] {
                    *val = 0.0;
                }
            } else {
                let inv_std = 1.0 / std;
                for val in &mut buffer[start..end] {
                    *val = (*val - mean) * inv_std;
                }
            }
        }
    }
}

/// Scales selected channels to a target range (Min-Max Scaling).
///
/// `(x - min) / (max - min) * (target_max - target_min) + target_min`
///
/// `min_max` must match the order and length of `channel_mask`.
/// If `max - min` is near zero, the channel is set to `target_min`.
pub fn normalize_min_max(
    buffer: &mut [f32],
    min_max: &[(f32, f32)],
    target_min: f32,
    target_max: f32,
    channel_mask: &[u16],
    total_channels: usize,
) {
    let samples_per_channel = buffer.len() / total_channels;
    let target_range = target_max - target_min;

    for (i, &ch) in channel_mask.iter().enumerate() {
        let start = ch as usize * samples_per_channel;
        let end = start + samples_per_channel;
        let (min, max) = min_max[i];
        let range = max - min;

        if end <= buffer.len() {
            if range.abs() < f32::EPSILON {
                for val in &mut buffer[start..end] {
                    *val = target_min;
                }
            } else {
                let inv_range = 1.0 / range;
                for val in &mut buffer[start..end] {
                    *val = ((*val - min) * inv_range) * target_range + target_min;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_normalize_zscore() {
        let mut data = vec![1.0, 2.0, 3.0, 4.0];
        // Mean = 2.5, StdDev = 1.118034
        normalize_zscore(&mut data, &[2.5], &[1.118034], &[0], 1);
        
        // (1-2.5)/1.118034 = -1.34164
        // (4-2.5)/1.118034 = 1.34164
        assert_relative_eq!(data[0], -1.3416407);
        assert_relative_eq!(data[3], 1.3416407);
    }

    #[test]
    fn test_normalize_min_max() {
        let mut data = vec![0.0, 5.0, 10.0];
        normalize_min_max(&mut data, &[(0.0, 10.0)], -1.0, 1.0, &[0], 1);
        assert_eq!(data, vec![-1.0, 0.0, 1.0]);
    }
}
