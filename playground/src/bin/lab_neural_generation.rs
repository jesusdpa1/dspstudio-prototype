use anyhow::Result;
use dsp_core::mock::signal::{SignalGenerator, SineWave, WhiteNoise};
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
    let duration_secs = 3600; // 1 hour
    let total_samples = (sample_rate as u64) * duration_secs;
    let chunk_size = 65536;

    println!("Generating neural recording lab:");
    println!("  Channels:    {}", n_channels);
    println!("  Sample Rate: {} Hz", sample_rate);
    println!("  Duration:    {} seconds (1 hour)", duration_secs);
    println!("  Total:       {} samples/channel", total_samples);

    let zarr_path = PathBuf::from("data/lab_neural/neural_recording.zarr");
    if let Some(parent) = zarr_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let config = StorageConfig {
        sample_rate: sample_rate as u32,
        channels: n_channels as u16,
        chunk_size,
        raw_archive_path: zarr_path.clone(),
        processed_archive_path: PathBuf::from("data/lab_neural/processed.zarr"),
        shadow_path: PathBuf::from("data/lab_neural/shadow.mmap"),
        default_surplus: 1024,
        compression_level: 3,
    };

    let metadata = DatasetMetadata::new_power_of_two(total_samples);
    let manager = StorageManager::new(config.clone())?;

    println!("Initializing Zarr hierarchy at {:?}...", zarr_path);
    manager.init_hierarchy(&metadata)?;

    // Generators (one per channel, kept alive across all chunks so state advances correctly):
    //   CH0: 10Hz sine (Delta-like)  + noise
    //   CH1: 20Hz sine (Beta-like)   + noise
    //   CH2: 40Hz sine (Gamma-like)  + noise
    //   CH3-15: White noise floor only
    let mut sines: Vec<SineWave> = vec![
        SineWave::new(10.0, sample_rate, 0.5),
        SineWave::new(20.0, sample_rate, 0.4),
        SineWave::new(40.0, sample_rate, 0.3),
    ];
    // Persistent noise generators — one per channel, NOT recreated each chunk.
    let mut noises: Vec<WhiteNoise> = (0..n_channels)
        .map(|ch| {
            // Sine channels get a low-amplitude noise floor (amplitude 0.05).
            // Pure-noise channels get a higher amplitude (0.1).
            let amplitude = if ch < 3 { 0.05 } else { 0.1 };
            WhiteNoise::new(ch as u32 + 100, amplitude)
        })
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

        for ch in 0..n_channels as usize {
            let start = ch * chunk_size;
            let end = start + current_chunk_samples;
            let slice = &mut chunk_buf[start..end];

            // Fill sine for the first three channels; noise for the rest.
            if ch < 3 {
                sines[ch].fill_buffer(slice, 1);
            }
            // Add noise (additive for sine channels, primary signal for ch3-15).
            let mut nbuf = vec![0.0_f32; current_chunk_samples];
            noises[ch].fill_buffer(&mut nbuf, 1);
            for (s, n) in slice.iter_mut().zip(nbuf.iter()) {
                *s += n;
            }
        }

        manager.write_raw_chunk(chunk_idx, &chunk_buf)?;
        pb.inc(1);
    }

    pb.finish_with_message("Generation complete");

    println!("Building peak pyramid (LODs)...");
    manager.build_peak_pyramid(&metadata, |_| {})?;

    println!("Saving recording metadata...");
    let mut rec_meta = RecordingMeta::default_for(n_channels as u16, total_samples, sample_rate);
    rec_meta.recording_name = "Lab Neural Recording".to_string();
    rec_meta.recording_type = "Neural".to_string();
    rec_meta.description = "16ch neural recording (simulated) for 1 hour @ 40kHz.".to_string();
    rec_meta.lod_levels_available = metadata.lod_chain.iter().map(|l| l.level).collect();
    rec_meta.save(&zarr_path)?;

    println!("Success! Recording saved to {:?}", zarr_path);
    println!("Metadata saved to {:?}", RecordingMeta::sidecar_path(&zarr_path));

    Ok(())
}
