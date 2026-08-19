//! Principal Component Analysis (PCA) projection.

use rayon::prelude::*;

/// Projects a batch of waveform snippets into PCA space.
///
/// # Arguments
/// * `snippets`: Flat buffer of extracted waveforms [N_Spikes x N_Samples].
/// * `n_samples`: Number of samples per snippet.
/// * `components`: Flattened projection matrix [N_Features x N_Samples].
/// * `mean_waveform`: Optional average waveform to subtract [N_Samples].
/// * `output`: Target buffer for features [N_Spikes x N_Features].
///
/// # Mathematical Operation
/// `Features = (Waveform - Mean) * Components^T`
pub fn project_pca(
    snippets: &[f32],
    n_samples: usize,
    components: &[f32],
    mean_waveform: Option<&[f32]>,
    output: &mut [f32],
) {
    if snippets.is_empty() || components.is_empty() || n_samples == 0 {
        return;
    }

    let n_features = components.len() / n_samples;

    output
        .par_chunks_mut(n_features)
        .enumerate()
        .for_each(|(s, spike_features)| {
            let snippet = &snippets[s * n_samples..(s + 1) * n_samples];
            for f in 0..n_features {
                let comp_row = &components[f * n_samples..(f + 1) * n_samples];
                let mut dot = 0.0f32;
                for i in 0..n_samples {
                    let val = match mean_waveform {
                        Some(mean) => snippet[i] - mean[i],
                        None => snippet[i],
                    };
                    dot += val * comp_row[i];
                }
                spike_features[f] = dot;
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_pca_projection_identity() {
        let snippets = vec![1.0, 2.0];
        let components = vec![1.0, 0.0];
        let mut output = vec![0.0; 1];
        project_pca(&snippets, 2, &components, None, &mut output);
        assert_eq!(output[0], 1.0);
    }

    #[test]
    fn test_pca_with_mean_subtraction() {
        let snippets = vec![10.0, 20.0];
        let mean = vec![10.0, 10.0];
        let components = vec![0.0, 1.0];
        let mut output = vec![0.0; 1];
        project_pca(&snippets, 2, &components, Some(&mean), &mut output);
        assert_eq!(output[0], 10.0);
    }

    #[test]
    fn test_pca_multi_spike() {
        // 2 spikes, 2 samples each → 2 features
        let snippets = vec![1.0, 0.0, 0.0, 1.0];
        let components = vec![1.0, 0.0, 0.0, 1.0]; // identity
        let mut output = vec![0.0; 4];
        project_pca(&snippets, 2, &components, None, &mut output);
        assert_relative_eq!(output[0], 1.0);
        assert_relative_eq!(output[1], 0.0);
        assert_relative_eq!(output[2], 0.0);
        assert_relative_eq!(output[3], 1.0);
    }
}
