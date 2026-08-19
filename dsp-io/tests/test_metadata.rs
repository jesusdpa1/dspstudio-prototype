use dsp_io::metadata::DatasetMetadata;

#[test]
fn test_lod_chain_logic() {
    // 1 hour at 40kHz = 144,000,000 samples
    let total_samples = 40_000 * 3600;
    let meta = DatasetMetadata::new_power_of_two(total_samples);
    
    // Level 0: 1:1
    assert_eq!(meta.lod_chain[0].ratio, 1);
    // Level 1: 1:16
    assert_eq!(meta.lod_chain[1].ratio, 16);
    // Level 2: 1:256
    assert_eq!(meta.lod_chain[2].ratio, 256);
    
    // Ensure we don't decimate into oblivion (should stop around 1024 samples)
    let last_level = meta.lod_chain.last().unwrap();
    let decimated_count = total_samples / last_level.ratio as u64;
    assert!(decimated_count >= 1024 || meta.lod_chain.len() == 8);
}
