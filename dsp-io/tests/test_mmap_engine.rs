use dsp_io::mmap::MmapEngine;
use tempfile::tempdir;

#[test]
fn test_mmap_lifecycle() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.mmap");
    let channels = 16;
    let samples = 1000;

    // 1. Create and Write
    {
        let mut engine = MmapEngine::new(&path, channels, samples).expect("Failed to create mmap");
        let slice = engine.get_channel_slice_mut(0, samples).unwrap();
        slice[0] = 42.0;
        slice[999] = -42.0;
        engine.flush().unwrap();
    }

    // 2. Re-open and Read
    {
        let engine = MmapEngine::new(&path, channels, samples).expect("Failed to open mmap");
        let slice = engine.get_channel_slice(0, samples).unwrap();
        assert_eq!(slice[0], 42.0);
        assert_eq!(slice[999], -42.0);
    }
}
