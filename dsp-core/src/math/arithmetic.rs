//! In-place arithmetic operations on channel-major `[f32]` buffers.
//!
//! All functions operate on a flat `[Channels × Samples]` layout where
//! channel `c` occupies indices `c * samples_per_channel ..(c+1) * samples_per_channel`.

use rayon::prelude::*;

/// Adds `value` to every sample in the selected channels.
///
/// # Arguments
/// * `buffer` — flat `[Channels × Samples]` slice (channel-major).
/// * `value` — scalar to add.
/// * `channel_mask` — indices of channels to modify; out-of-range indices are silently skipped.
/// * `total_channels` — total number of channels encoded in `buffer`.
///
/// # Example
/// ```
/// use dsp_core::math::arithmetic::add_scalar;
/// let mut data = vec![0.0f32; 4]; // 1 channel × 4 samples
/// add_scalar(&mut data, 1.0, &[0], 1);
/// assert_eq!(data, vec![1.0, 1.0, 1.0, 1.0]);
/// ```
pub fn add_scalar(buffer: &mut [f32], value: f32, channel_mask: &[u16], total_channels: usize) {
    if buffer.is_empty() || total_channels == 0 { return; }
    let samples_per_channel = buffer.len() / total_channels;
    
    buffer
        .par_chunks_mut(samples_per_channel)
        .enumerate()
        .for_each(|(ch_idx, chunk)| {
            if channel_mask.contains(&(ch_idx as u16)) {
                chunk.iter_mut().for_each(|v| *v += value);
            }
        });
}

/// Multiplies every sample in the selected channels by `value`.
///
/// # Arguments
/// * `buffer` — flat `[Channels × Samples]` slice (channel-major).
/// * `value` — scalar gain factor.
/// * `channel_mask` — indices of channels to modify; out-of-range indices are silently skipped.
/// * `total_channels` — total number of channels encoded in `buffer`.
///
/// # Example
/// ```
/// use dsp_core::math::arithmetic::mul_scalar;
/// let mut data = vec![1.0f32; 4]; // 1 channel × 4 samples
/// mul_scalar(&mut data, 2.0, &[0], 1);
/// assert_eq!(data, vec![2.0, 2.0, 2.0, 2.0]);
/// ```
pub fn mul_scalar(buffer: &mut [f32], value: f32, channel_mask: &[u16], total_channels: usize) {
    if buffer.is_empty() || total_channels == 0 { return; }
    let samples_per_channel = buffer.len() / total_channels;
    
    buffer
        .par_chunks_mut(samples_per_channel)
        .enumerate()
        .for_each(|(ch_idx, chunk)| {
            if channel_mask.contains(&(ch_idx as u16)) {
                chunk.iter_mut().for_each(|v| *v *= value);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_scalar_single_channel() {
        let mut buf = vec![1.0f32, 2.0, 3.0, 4.0];
        add_scalar(&mut buf, 10.0, &[0], 1);
        assert_eq!(buf, vec![11.0, 12.0, 13.0, 14.0]);
    }

    #[test]
    fn add_scalar_selects_only_masked_channel() {
        // 2 channels x 4 samples each
        let mut buf = vec![1.0f32; 8];
        add_scalar(&mut buf, 5.0, &[1], 2);
        assert_eq!(&buf[0..4], &[1.0, 1.0, 1.0, 1.0]); // ch0 untouched
        assert_eq!(&buf[4..8], &[6.0, 6.0, 6.0, 6.0]); // ch1 modified
    }

    #[test]
    fn add_scalar_negative_value() {
        let mut buf = vec![5.0f32; 4];
        add_scalar(&mut buf, -5.0, &[0], 1);
        assert!(buf.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn mul_scalar_single_channel() {
        let mut buf = vec![1.0f32, 2.0, 3.0, 4.0];
        mul_scalar(&mut buf, 3.0, &[0], 1);
        assert_eq!(buf, vec![3.0, 6.0, 9.0, 12.0]);
    }

    #[test]
    fn mul_scalar_selects_only_masked_channel() {
        // 2 channels x 4 samples each
        let mut buf = vec![2.0f32; 8];
        mul_scalar(&mut buf, 4.0, &[0], 2);
        assert_eq!(&buf[0..4], &[8.0, 8.0, 8.0, 8.0]); // ch0 scaled
        assert_eq!(&buf[4..8], &[2.0, 2.0, 2.0, 2.0]); // ch1 untouched
    }

    #[test]
    fn mul_scalar_by_zero_clears_channel() {
        let mut buf = vec![42.0f32; 4];
        mul_scalar(&mut buf, 0.0, &[0], 1);
        assert!(buf.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn out_of_range_channel_is_silently_ignored() {
        let mut buf = vec![1.0f32; 4];
        add_scalar(&mut buf, 99.0, &[5], 1);
        assert_eq!(buf, vec![1.0, 1.0, 1.0, 1.0]);
    }
}
