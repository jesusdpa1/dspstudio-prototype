//! Spatial referencing (CAR, CMR) for noise rejection across channels.

use crate::math::statistics::{compute_means, compute_medians};

/// Applies Common Average Reference (CAR) in-place.
/// 
/// `data` is a flat channel-major buffer. 
/// `channel_mask` defines which channels contribute to and receive the reference.
pub fn apply_car(data: &mut [f32], channel_mask: &[u16], total_channels: usize) {
    if data.is_empty() || total_channels == 0 || channel_mask.is_empty() {
        return;
    }
    
    let samples_per_channel = data.len() / total_channels;
    
    // For each sample index, compute the average across selected channels
    for s in 0..samples_per_channel {
        let mut sum = 0.0;
        for &ch in channel_mask {
            sum += data[ch as usize * samples_per_channel + s];
        }
        let avg = sum / channel_mask.len() as f32;
        
        // Subtract average from selected channels
        for &ch in channel_mask {
            data[ch as usize * samples_per_channel + s] -= avg;
        }
    }
}

/// Applies Common Median Reference (CMR) in-place.
/// 
/// More robust than CAR as it ignores outlier channels with massive spikes.
pub fn apply_cmr(data: &mut [f32], channel_mask: &[u16], total_channels: usize) {
    if data.is_empty() || total_channels == 0 || channel_mask.is_empty() {
        return;
    }
    
    let samples_per_channel = data.len() / total_channels;
    
    // CMR is expensive: requires a median calculation per sample point across channels
    for s in 0..samples_per_channel {
        let mut sample_values: Vec<f32> = channel_mask
            .iter()
            .map(|&ch| data[ch as usize * samples_per_channel + s])
            .collect();
        
        let mid = sample_values.len() / 2;
        let (_, &mut median, _) = sample_values.select_nth_unstable_by(mid, |a, b| a.total_cmp(b));
        
        for &ch in channel_mask {
            data[ch as usize * samples_per_channel + s] -= median;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_car_removes_shared_offset() {
        // 2 channels, 2 samples
        // Ch0: [10.0, 11.0]
        // Ch1: [10.0, 11.0]
        // Shared offset: 10 and 11. Result should be 0.
        let mut data = vec![10.0, 11.0, 10.0, 11.0];
        apply_car(&mut data, &[0, 1], 2);
        assert_eq!(data, vec![0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn test_cmr_ignores_outlier() {
        // 3 channels, 1 sample
        // Ch0: 10.0
        // Ch1: 10.0
        // Ch2: 1000.0 (outlier)
        // Median is 10.0. Result: [0, 0, 990]
        let mut data = vec![10.0, 10.0, 1000.0];
        apply_cmr(&mut data, &[0, 1, 2], 3);
        assert_eq!(data, vec![0.0, 0.0, 990.0]);
    }
}
