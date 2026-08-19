use anyhow::Result;
use dsp_core::mock::neural::{SpikeTemplate, NeuralUnit};
use dsp_core::mock::signal::{SignalGenerator, WhiteNoise};
use dsp_io::config::StorageConfig;
use dsp_io::metadata::DatasetMetadata;
use dsp_io::recording_meta::RecordingMeta;
use dsp_io::zarr::StorageManager;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    let sample_rate = 40_000.0_f32;
    let n_channels = 16;
    let duration_secs = 600; // 10 minutes
    let total_samples = (sample_rate as u64) * duration_secs;
    let chunk_size = 1048576; // 2^20, large enough for ratio 16^5

    println!("Generating High-Fidelity Neuropixel Emulator:");
    println!("  Channels:    {}", n_channels);
    println!("  Sample Rate: {} Hz", sample_rate);
    println!("  Duration:    {} seconds (10 mins)", duration_secs);

    let zarr_path = PathBuf::from("data/lab13/ephys_emulator.zarr");
    if let Some(parent) = zarr_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let config = StorageConfig {
        sample_rate: sample_rate as u32,
        channels: n_channels as u16,
        chunk_size,
        raw_archive_path: zarr_path.clone(),
        processed_archive_path: PathBuf::from("data/lab13/processed_ephys.zarr"),
        shadow_path: PathBuf::from("data/lab13/shadow_ephys.mmap"),
        default_surplus: 1024,
        compression_level: 3,
    };

    let mut metadata = DatasetMetadata::new_power_of_two(total_samples);
    // Force 5 peak LOD levels as requested (lod_1 to lod_5)
    metadata.lod_chain = (0..=5).map(|l| dsp_io::metadata::LodLevel {
        level: l as u8,
        ratio: 1 << (l * 4),
    }).collect();

    let manager = StorageManager::new(config.clone())?;
    manager.init_hierarchy(&metadata)?;

    // Three Poisson-firing units per channel with distinct rates and amplitudes.
    // Seeds are deterministic so the recording is reproducible.
    let mut channel_units: Vec<Vec<NeuralUnit>> = (0..n_channels)
        .map(|ch| {
            let seed_base = (ch as u32).wrapping_mul(10);
            vec![
                NeuralUnit::new(SpikeTemplate::new_biphasic(sample_rate), 10.0, 0.8, seed_base,     sample_rate),
                NeuralUnit::new(SpikeTemplate::new_biphasic(sample_rate),  5.0, 1.2, seed_base + 1, sample_rate),
                NeuralUnit::new(SpikeTemplate::new_biphasic(sample_rate),  2.0, 0.5, seed_base + 2, sample_rate),
            ]
        })
        .collect();

    // Per-channel noise floors, kept alive across chunks so noise state advances correctly.
    let mut noise_gens: Vec<WhiteNoise> = (0..n_channels)
        .map(|ch| WhiteNoise::new(ch as u32 + 1000, 0.1))
        .collect();

    let total_chunks = (total_samples + chunk_size as u64 - 1) / chunk_size as u64;
    let pb = ProgressBar::new(total_chunks);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")?
        .progress_chars("#>-"));

    let mut chunk_buf = vec![0.0_f32; n_channels as usize * chunk_size];

    for chunk_idx in 0..total_chunks {
        chunk_buf.fill(0.0);
        let current_chunk_samples = if chunk_idx == total_chunks - 1 {
            let rem = (total_samples % chunk_size as u64) as usize;
            if rem == 0 { chunk_size } else { rem }
        } else {
            chunk_size
        };

        for ch in 0..n_channels {
            let start = ch * chunk_size;
            let end = start + current_chunk_samples;
            let channel_slice = &mut chunk_buf[start..end];

            // 1. Noise floor
            noise_gens[ch].fill_buffer(channel_slice, 1);
            // 2. Spike injection (additive)
            for unit in &mut channel_units[ch] {
                unit.fill_buffer(channel_slice, sample_rate);
            }
        }

        manager.write_raw_chunk(chunk_idx, &chunk_buf)?;
        pb.inc(1);
    }

    pb.finish_with_message("Ephys data generation complete");

    println!("Building peak pyramid...");
    manager.build_peak_pyramid(&metadata, |_| {})?;

    let mut rec_meta = RecordingMeta::default_for(n_channels as u16, total_samples, sample_rate);
    rec_meta.recording_name = "Synthetic Neuropixel Data".to_string();
    rec_meta.recording_type = "Ephys".to_string();
    rec_meta.save(&zarr_path)?;

    println!("Success! Ephys emulator saved to {:?}", zarr_path);
    Ok(())
}
