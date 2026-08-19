use dsp_io::config::StorageConfig;
use dsp_io::metadata::DatasetMetadata;
use dsp_io::zarr::StorageManager;
use tempfile::tempdir;

#[test]
fn test_zarr_integration_lifecycle() {
    let dir = tempdir().unwrap();
    let mut config = StorageConfig::default();
    config.raw_archive_path = dir.path().join("archive.zarr");

    let samples = config.chunk_size as u64 * 2;
    let metadata = DatasetMetadata::new_power_of_two(samples);

    let manager = StorageManager::new(config.clone()).unwrap();
    manager
        .init_hierarchy(&metadata)
        .expect("Failed to init hierarchy");

    // Write 2 chunks of data
    let mut chunk_data = vec![1.0f32; config.channels as usize * config.chunk_size];
    chunk_data[0] = 99.0; // Marker

    manager.write_raw_chunk(0, &chunk_data).unwrap();
    manager.write_raw_chunk(1, &chunk_data).unwrap();

    // Read window across chunks
    // Request 100 samples from the end of chunk 0 and start of chunk 1
    let window_start = config.chunk_size as u64 - 50;
    let window_count = 100;

    let read_data = manager.read_raw_window_masked(window_start, window_count, &[0, 1]).unwrap();
    
    // read_data is [Channels, window_count]
    // Total elements = 2 channels * 100
    assert_eq!(read_data.len(), (2 * window_count) as usize);
}
