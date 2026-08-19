//! Lab 03 — Rich recording with streams + two epoch tracks.
//!
//! Generates data/lab03/recording.zarr with:
//!   - 8 channels at 40 kHz, 60 seconds
//!   - Each channel has a distinct waveform (varied frequency + noise floor)
//!   - Two event tracks: "motor_events" (4 labels) and "sensory_events" (3 labels)
//!   - No saved preferred_blueprint — tests auto_generate path in the app

use anyhow::Result;
use dsp_core::mock::epoch::generate_random_events;
use dsp_core::mock::signal::{SignalGenerator, SineWave, WhiteNoise};
use dsp_core::signal::LabelVocabulary;
use dsp_io::config::StorageConfig;
use dsp_io::metadata::DatasetMetadata;
use dsp_io::recording_meta::{RecordingMeta, TrackMeta};
use dsp_io::zarr::StorageManager;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    let sample_rate = 40_000.0_f32;
    let n_channels: u16 = 8;
    let duration_secs = 60.0_f64;
    let total_samples = (sample_rate as f64 * duration_secs) as u64;
    let chunk_size: usize = 32768;

    let zarr_path = PathBuf::from("data/lab03/recording.zarr");
    if let Some(parent) = zarr_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if zarr_path.exists() {
        println!("Removing existing recording...");
        std::fs::remove_dir_all(&zarr_path)?;
    }

    println!("Lab 03 — Rich Recording Generator");
    println!("  Path:        {:?}", zarr_path);
    println!("  Channels:    {}", n_channels);
    println!("  Sample Rate: {} Hz", sample_rate);
    println!("  Duration:    {} s", duration_secs);
    println!("  Samples:     {}", total_samples);

    let config = StorageConfig {
        sample_rate: sample_rate as u32,
        channels: n_channels,
        chunk_size,
        raw_archive_path: zarr_path.clone(),
        processed_archive_path: zarr_path.with_extension("processed.zarr"),
        shadow_path: zarr_path.with_extension("mmap"),
        default_surplus: 64,
        compression_level: 3,
    };

    let metadata = DatasetMetadata::new_power_of_two(total_samples);
    let manager = StorageManager::new(config)?;
    manager.init_hierarchy(&metadata)?;

    // ── Signal generation ────────────────────────────────────────────────────
    let freqs = [5.0_f32, 10.0, 20.0, 40.0, 80.0, 120.0, 160.0, 200.0];
    let amps  = [1.0_f32, 0.9,  0.8,  0.7,  0.6,  0.5,   0.4,   0.3];
    let mut sines: Vec<SineWave> = freqs.iter().zip(amps.iter())
        .map(|(&f, &a)| SineWave::new(f, sample_rate, a))
        .collect();
    let mut noises: Vec<WhiteNoise> = (0..n_channels as u32)
        .map(|seed| WhiteNoise::new(seed, 0.05))
        .collect();

    let total_chunks = (total_samples + chunk_size as u64 - 1) / chunk_size as u64;
    println!("Writing {} chunks of signal data...", total_chunks);

    for chunk_idx in 0..total_chunks {
        let cs = (chunk_size as u64).min(total_samples - chunk_idx * chunk_size as u64) as usize;
        let mut buf = vec![0.0_f32; n_channels as usize * cs];

        for ch in 0..n_channels as usize {
            let offset = ch * cs;
            let slice = &mut buf[offset..offset + cs];
            sines[ch].fill_buffer(slice, 1);
            let mut nbuf = vec![0.0_f32; cs];
            noises[ch].fill_buffer(&mut nbuf, 1);
            for (s, n) in slice.iter_mut().zip(nbuf.iter()) {
                *s += n;
            }
        }

        manager.write_raw_chunk(chunk_idx, &buf)?;
    }

    // ── Epoch track 1: motor_events ──────────────────────────────────────────
    println!("Generating motor_events track...");
    let motor_events_per_ch: Vec<_> = (0..n_channels)
        .map(|i| generate_random_events(total_samples, 0.0002, i as u32 % 4, 100 + i as u32))
        .collect();
    manager.write_events_track("motor_events", &motor_events_per_ch)?;

    // ── Epoch track 2: sensory_events ────────────────────────────────────────
    println!("Generating sensory_events track...");
    let sensory_events_per_ch: Vec<_> = (0..n_channels)
        .map(|i| generate_random_events(total_samples, 0.0001, i as u32 % 3, 200 + i as u32))
        .collect();
    manager.write_events_track("sensory_events", &sensory_events_per_ch)?;

    // ── Peak pyramid ─────────────────────────────────────────────────────────
    println!("Building peak pyramid...");
    manager.build_peak_pyramid(&metadata, |p| {
        if (p * 100.0) as u32 % 25 == 0 {
            print!("\r  {:.0}%", p * 100.0);
            use std::io::Write;
            let _ = std::io::stdout().flush();
        }
    })?;
    println!("\r  100%");

    // ── Metadata ─────────────────────────────────────────────────────────────
    let channel_indices: Vec<u16> = (0..n_channels).collect();

    let mut rec_meta = RecordingMeta::default_for(n_channels, total_samples, sample_rate);
    rec_meta.recording_name = "lab03_rich_recording".to_string();
    rec_meta.recording_type = "ECoG".to_string();
    rec_meta.description = "8-channel recording with two epoch tracks, no saved blueprint.".to_string();
    rec_meta.lod_levels_available = metadata.lod_chain.iter().map(|l| l.level).collect();

    rec_meta.tracks.push(TrackMeta::events(
        "motor_events".to_string(),
        channel_indices.clone(),
        LabelVocabulary::new(vec!["Flex".into(), "Extend".into(), "Grip".into(), "Release".into()]),
    ));
    rec_meta.tracks.push(TrackMeta::events(
        "sensory_events".to_string(),
        channel_indices,
        LabelVocabulary::new(vec!["Touch".into(), "Pressure".into(), "Pain".into()]),
    ));

    // Intentionally leave preferred_blueprint as None to exercise auto_generate.
    rec_meta.save(&zarr_path)?;

    println!("Done! Recording saved to {:?}", zarr_path);
    Ok(())
}
