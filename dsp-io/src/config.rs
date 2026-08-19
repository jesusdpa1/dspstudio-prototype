use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Global configuration for the DSP-Studio I/O layer.
/// 
/// This struct defines the paths and hardware-specific parameters
/// for data ingestion and processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Samples per second (e.g., 40000)
    pub sample_rate: u32,
    /// Number of physical channels (e.g., 16)
    pub channels: u16,
    /// Number of samples per chunk per channel (power of 2 recommended, e.g., 32768)
    pub chunk_size: usize,
    
    /// Root directory for the immutable Zarr v3 raw data
    pub raw_archive_path: PathBuf,
    /// Root directory for the processed Zarr v3 data
    pub processed_archive_path: PathBuf,
    /// Path to the mmap-sync shadow file (hot buffer)
    pub shadow_path: PathBuf,
    /// Default surplus (overlap) for processing windows (default: 1024)
    pub default_surplus: u64,
    /// Zstd compression level applied to bulk Zarr arrays (raw, peaks, spike
    /// artifacts). Higher = smaller/slower. `3` is a good size/speed balance.
    pub compression_level: i32,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            sample_rate: 40_000,
            channels: 16,
            chunk_size: 1 << 15, // 32,768 samples (~0.8s)
            raw_archive_path: PathBuf::from("data/archive/raw.zarr"),
            processed_archive_path: PathBuf::from("data/archive/processed.zarr"),
            shadow_path: PathBuf::from("data/tmp/session_shadow.mmap"),
            default_surplus: 1024,
            compression_level: 3,
        }
    }
}
