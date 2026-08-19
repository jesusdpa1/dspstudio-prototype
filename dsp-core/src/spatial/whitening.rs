//! Spatial whitening (decorrelation) kernels.
//! 
//! Whitening transforms multi-channel data such that the noise covariance 
//! matrix becomes identity. This ensures that noise is independent and 
//! uniform across all directions, which is a prerequisite for effective 
//! PCA and clustering.

use rayon::prelude::*;

/// Applies a whitening matrix to a multi-channel buffer in-place.
/// 
/// `data`: Flat channel-major buffer [Channels x Samples].
/// `whitening_matrix`: Flattened [N x N] matrix where N is `channel_mask.len()`.
/// `channel_mask`: Which channels to apply the transformation to.
pub fn apply_whitening(
    data: &mut [f32],
    whitening_matrix: &[f32],
    channel_mask: &[u16],
    total_channels: usize,
) {
    if data.is_empty() || whitening_matrix.is_empty() || channel_mask.is_empty() {
        return;
    }

    let n_masked = channel_mask.len();
    let samples_per_channel = data.len() / total_channels;

    // Use a temporary buffer for the spatial vector at each sample to avoid
    // constant re-allocation if possible, though for a pure kernel we'll 
    // keep it simple first.
    
    for s in 0..samples_per_channel {
        let mut x = vec![0.0f32; n_masked];
        for (i, &ch) in channel_mask.iter().enumerate() {
            x[i] = data[ch as usize * samples_per_channel + s];
        }

        for i in 0..n_masked {
            let mut y_i = 0.0f32;
            let row_offset = i * n_masked;
            for j in 0..n_masked {
                y_i += whitening_matrix[row_offset + j] * x[j];
            }
            let ch = channel_mask[i];
            data[ch as usize * samples_per_channel + s] = y_i;
        }
    }
}

/// Estimates the covariance matrix from a multi-channel buffer.
/// 
/// Returns a flattened [N x N] covariance matrix.
pub fn estimate_covariance(
    data: &[f32],
    channel_mask: &[u16],
    total_channels: usize,
) -> Vec<f32> {
    let n = channel_mask.len();
    let samples = data.len() / total_channels;
    let mut cov = vec![0.0f32; n * n];
    
    if samples <= 1 { return cov; }

    // 1. Compute means for each masked channel
    let mut means = vec![0.0f32; n];
    for (i, &ch) in channel_mask.iter().enumerate() {
        let start = ch as usize * samples;
        let channel_data = &data[start..start + samples];
        let sum: f64 = channel_data.iter().map(|&v| v as f64).sum();
        means[i] = (sum / samples as f64) as f32;
    }

    // 2. Accumulate outer product (x - mu)(x - mu)^T
    for s in 0..samples {
        // Spatial vector for this sample across masked channels
        let mut x = vec![0.0f32; n];
        for (i, &ch) in channel_mask.iter().enumerate() {
            x[i] = data[ch as usize * samples + s] - means[i];
        }

        for i in 0..n {
            let row_offset = i * n;
            let xi = x[i];
            for j in 0..n {
                cov[row_offset + j] += xi * x[j];
            }
        }
    }

    // 3. Normalize (N-1 for unbiased estimator)
    let scale = 1.0 / (samples - 1) as f32;
    for val in cov.iter_mut() {
        *val *= scale;
    }

    cov
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_apply_identity_whitening() {
        let mut data = vec![1.0, 2.0, 3.0, 4.0]; // 2ch, 2 samples (Ch0:[1,2], Ch1:[3,4])
        let identity = vec![1.0, 0.0, 0.0, 1.0];
        apply_whitening(&mut data, &identity, &[0, 1], 2);
        assert_eq!(data, vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn test_covariance_diagonal() {
        // Ch0: [1, -1, 1, -1] -> Mean 0, Var = (1+1+1+1)/3 = 1.333
        // Ch1: [2, -2, 2, -2] -> Mean 0, Var = (4+4+4+4)/3 = 5.333
        // They are perfectly correlated here actually: Ch1 = 2 * Ch0
        // Cov = E[X*Y] = (2 + 2 + 2 + 2)/3 = 8/3 = 2.666
        let data = vec![
            1.0, -1.0, 1.0, -1.0, // Ch0
            2.0, -2.0, 2.0, -2.0  // Ch1
        ];
        let cov = estimate_covariance(&data, &[0, 1], 2);
        assert_relative_eq!(cov[0], 4.0/3.0);
        assert_relative_eq!(cov[3], 16.0/3.0);
        assert_relative_eq!(cov[1], 8.0/3.0); // Correctly correlated
    }
}
