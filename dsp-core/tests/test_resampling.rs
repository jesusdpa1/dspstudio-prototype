use dsp_core::util::resampling::generate_peaks_parallel;

#[test]
fn test_min_max_basic() {
    let data = vec![1.0, -2.0, 5.0, 0.0, 10.0, -10.0, 3.0, 2.0];
    let channels = 1;
    let ratio = 4;
    
    let peaks = generate_peaks_parallel(&data, channels, ratio);
    
    assert_eq!(peaks.len(), 2);
    // First window: [1.0, -2.0, 5.0, 0.0] -> min -2, max 5
    assert_eq!(peaks[0].min, -2.0);
    assert_eq!(peaks[0].max, 5.0);
    // Second window: [10.0, -10.0, 3.0, 2.0] -> min -10, max 10
    assert_eq!(peaks[1].min, -10.0);
    assert_eq!(peaks[1].max, 10.0);
}

#[test]
fn test_parallel_multichannel() {
    // 2 channels, 8 samples each
    let mut data = vec![0.0; 16];
    // Channel 0: all 1.0
    for i in 0..8 { data[i] = 1.0; }
    // Channel 1: all -1.0
    for i in 8..16 { data[i] = -1.0; }
    
    let peaks = generate_peaks_parallel(&data, 2, 4);
    
    assert_eq!(peaks.len(), 4); // 2 channels * (8/4)
    assert_eq!(peaks[0].min, 1.0);
    assert_eq!(peaks[2].max, -1.0);
}
