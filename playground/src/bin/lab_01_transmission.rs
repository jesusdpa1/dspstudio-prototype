//! Lab 01 — Basic storage manager reads.
//!
//! Prerequisite: run `lab_neural_generation` first to create the data at
//! `data/lab_neural/neural_recording.zarr`.

use anyhow::Result;
use dsp_io::config::StorageConfig;
use dsp_io::zarr::StorageManager;
use std::path::PathBuf;

fn main() -> Result<()> {
    env_logger::init();
    println!("LAB_01  Basic Transmission Bridge");

    let zarr_path = PathBuf::from("data/lab_neural/neural_recording.zarr");

    let mut config = StorageConfig::default();
    config.raw_archive_path = zarr_path;

    let manager = StorageManager::new(config)?;

    println!("Requesting raw data: 100 samples from offset 0 (channels 0 and 1)...");
    match manager.read_raw_window_masked(0, 100, &[0u16, 1u16]) {
        Ok(data) => {
            println!("  Received {} data points.", data.len());
            println!("  First 5 samples (ch0): {:?}", &data[..5.min(data.len())]);
        }
        Err(e) => {
            println!("  Raw data not found — run lab_neural_generation first. Error: {}", e);
        }
    }

    println!("Requesting LOD level 1 peaks: 10 windows from offset 0...");
    match manager.read_peak_window_masked(1, 0, 10, &[0u16]) {
        Ok(peaks) => {
            println!("  Received {} peak values (min/max pairs).", peaks.len());
            println!("  First pair (min, max): ({:.4}, {:.4})", peaks[0], peaks[1]);
        }
        Err(e) => {
            println!("  Peaks not found — run lab_neural_generation first. Error: {}", e);
        }
    }

    Ok(())
}
