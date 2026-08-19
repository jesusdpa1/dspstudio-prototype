//! High-fidelity signal reconstruction and sub-sample alignment.

/// Cubic Hermite Spline interpolation (Catmull-Rom variant).
/// 
/// Returns the interpolated value at fractional index `t` (where `0.0 <= t <= 1.0`)
/// between `y1` and `y2`. `y0` and `y3` are the surrounding samples.
pub fn cubic_interp(y0: f32, y1: f32, y2: f32, y3: f32, t: f32) -> f32 {
    let t2 = t * t;
    let t3 = t2 * t;
    
    0.5 * (
        (2.0 * y1) +
        (-y0 + y2) * t +
        (2.0 * y0 - 5.0 * y1 + 4.0 * y2 - y3) * t2 +
        (-y0 + 3.0 * y1 - 3.0 * y2 + y3) * t3
    )
}

/// Shifts a waveform snippet by a fractional sample offset using cubic interpolation.
/// 
/// # Arguments
/// * `input`: Original waveform snippet.
/// * `offset`: Fractional shift in samples (-0.5 to 0.5).
/// * `output`: Target buffer (must be same size as `input`).
pub fn shift_waveform_cubic(input: &[f32], offset: f32, output: &mut [f32]) {
    if input.len() < 4 || output.len() < input.len() {
        return;
    }

    let n = input.len();
    for i in 0..n {
        // To shift the waveform by 'offset' samples, we sample the reconstructed 
        // continuous signal at points i - offset.
        let virtual_idx = i as f32 - offset;
        let idx1 = virtual_idx.floor() as i32;
        let t = virtual_idx - idx1 as f32;
        
        // Samples at idx1-1, idx1, idx1+1, idx1+2
        let get_sample = |idx: i32| -> f32 {
            if idx < 0 {
                input[0]
            } else if idx >= n as i32 {
                input[n - 1]
            } else {
                input[idx as usize]
            }
        };

        output[i] = cubic_interp(
            get_sample(idx1 - 1),
            get_sample(idx1),
            get_sample(idx1 + 1),
            get_sample(idx1 + 2),
            t
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_cubic_interp_identity() {
        // At t=0, it should return y1
        assert_relative_eq!(cubic_interp(0.0, 10.0, 20.0, 30.0, 0.0), 10.0);
        // At t=1, it should return y2
        assert_relative_eq!(cubic_interp(0.0, 10.0, 20.0, 30.0, 1.0), 20.0);
    }

    #[test]
    fn test_shift_waveform() {
        let input = vec![0.0, 0.0, 10.0, 0.0, 0.0];
        let mut output = vec![0.0; 5];
        // Shift right by 0.5 samples
        shift_waveform_cubic(&input, 0.5, &mut output);
        // The peak at index 2 should be distributed between 2 and 3
        assert!(output[2] < 10.0);
        assert!(output[3] > 0.0);
    }
}
