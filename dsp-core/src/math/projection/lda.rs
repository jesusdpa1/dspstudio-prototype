//! Linear Discriminant Analysis (LDA) projection.
//!
//! Projects pre-extracted waveform snippets using a pre-fitted LDA transformation
//! matrix. The fitting step (computing S_W^{-1} S_B eigenvectors from labelled
//! training data) is out-of-scope for this module; `components` must be supplied
//! by the caller.
//!
//! The projection kernel is a dense matrix multiply identical in form to PCA
//! (both reduce to `Features = (Waveform - Mean) * W^T`), but the columns of `W`
//! are Fisher discriminant vectors rather than principal components.

use rayon::prelude::*;

/// Projects a batch of waveform snippets into LDA space.
///
/// # Arguments
/// * `snippets` — flat `[n_spikes × snippet_len]` slice.
/// * `snippet_len` — length of each waveform snippet.
/// * `components` — flat `[n_components × snippet_len]` LDA transformation matrix
///   (Fisher discriminant vectors as rows).
/// * `mean` — optional mean vector `[snippet_len]` to subtract before projection.
/// * `output` — flat `[n_spikes × n_components]` output buffer (caller-allocated).
pub fn project_lda(
    snippets: &[f32],
    snippet_len: usize,
    components: &[f32],
    mean: Option<&[f32]>,
    output: &mut [f32],
) {
    if snippets.is_empty() || components.is_empty() || snippet_len == 0 {
        return;
    }

    let n_components = components.len() / snippet_len;

    output
        .par_chunks_mut(n_components)
        .enumerate()
        .for_each(|(s, spike_features)| {
            let snippet = &snippets[s * snippet_len..(s + 1) * snippet_len];
            for f in 0..n_components {
                let comp_row = &components[f * snippet_len..(f + 1) * snippet_len];
                let mut dot = 0.0f32;
                for i in 0..snippet_len {
                    let val = match mean {
                        Some(m) => snippet[i] - m[i],
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
    fn test_lda_identity_components() {
        let snippets = vec![1.0, 2.0, 3.0, 4.0];
        let components = vec![1.0, 0.0, 0.0, 1.0]; // identity 2×2
        let mut output = vec![0.0; 4];
        project_lda(&snippets, 2, &components, None, &mut output);
        assert_relative_eq!(output[0], 1.0);
        assert_relative_eq!(output[1], 2.0);
        assert_relative_eq!(output[2], 3.0);
        assert_relative_eq!(output[3], 4.0);
    }

    #[test]
    fn test_lda_with_mean_subtraction() {
        let snippets = vec![5.0, 7.0];
        let mean = vec![2.0, 3.0];
        // After centering: [3, 4]; project onto [1, 0] → 3.0
        let components = vec![1.0, 0.0];
        let mut output = vec![0.0; 1];
        project_lda(&snippets, 2, &components, Some(&mean), &mut output);
        assert_relative_eq!(output[0], 3.0);
    }

    #[test]
    fn test_lda_multi_spike_parallel() {
        // 3 spikes, 2 samples each, 1 discriminant component [1, 1] / sqrt(2)
        let snippets = vec![1.0, 1.0, 2.0, 2.0, 3.0, 3.0];
        let scale = 1.0_f32 / 2.0_f32.sqrt();
        let components = vec![scale, scale];
        let mut output = vec![0.0; 3];
        project_lda(&snippets, 2, &components, None, &mut output);
        assert_relative_eq!(output[0], 2.0_f32.sqrt(), epsilon = 1e-5);
        assert_relative_eq!(output[1], 2.0 * 2.0_f32.sqrt(), epsilon = 1e-5);
        assert_relative_eq!(output[2], 3.0 * 2.0_f32.sqrt(), epsilon = 1e-5);
    }
}
