use anyhow::Result;
use dsp_io::config::StorageConfig;
use dsp_io::metadata::DatasetMetadata;
use dsp_io::recording_meta::RecordingMeta;
use dsp_io::zarr::StorageManager;
use dsp_core::mock::signal::{SignalGenerator, SineWave, WhiteNoise};
use dsp_core::mock::neural::{SpikeTemplate, NeuralUnit};
use crate::cli::RecordingType;
use std::path::PathBuf;

pub fn run(
    sampling_rate: f32,
    recording_type: RecordingType,
    duration: f64,
    channels: u16,
    output: PathBuf,
    with_epochs: bool,
) -> Result<()> {
    println!("Creating recording:");
    println!("  Output: {:?}", output);
    println!("  Sampling Rate: {} Hz", sampling_rate);
    println!("  Type: {}", recording_type.to_string());
    println!("  Duration: {} s", duration);
    println!("  Channels: {}", channels);
    if with_epochs {
        println!("  Mock Epochs: Enabled");
    }

    let total_samples = (duration * sampling_rate as f64) as u64;
    let chunk_size = 32768usize; // 2^15

    let config = StorageConfig {
        sample_rate: sampling_rate as u32,
        channels,
        chunk_size,
        raw_archive_path: output.clone(),
        processed_archive_path: output.with_extension("processed.zarr"),
        shadow_path: output.with_extension("mmap"),
        default_surplus: 64,
        compression_level: 3,
    };

    let metadata = DatasetMetadata::new_power_of_two(total_samples);
    let manager = StorageManager::new(config)?;

    if output.exists() {
        println!("Warning: Output path already exists. Overwriting...");
        std::fs::remove_dir_all(&output)?;
    }

    manager.init_hierarchy(&metadata)?;

    let mut rec_meta = RecordingMeta::default_for(channels, total_samples, sampling_rate);
    rec_meta.recording_name = output.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("New Recording")
        .to_string();
    rec_meta.recording_type = recording_type.to_string();

    let total_chunks = (total_samples + chunk_size as u64 - 1) / chunk_size as u64;

    println!("Generating signal data...");

    match recording_type {
        RecordingType::Ephys => {
            // Three Poisson-firing units per channel injected onto a noise floor.
            // Units differ in firing rate and amplitude to simulate multi-unit activity.
            let mut channel_units: Vec<Vec<NeuralUnit>> = (0..channels as usize)
                .map(|ch| {
                    let seed_base = (ch as u32).wrapping_mul(10).wrapping_add(1);
                    vec![
                        NeuralUnit::new(SpikeTemplate::new_biphasic(sampling_rate), 10.0, 0.8, seed_base,     sampling_rate),
                        NeuralUnit::new(SpikeTemplate::new_biphasic(sampling_rate),  5.0, 1.2, seed_base + 1, sampling_rate),
                        NeuralUnit::new(SpikeTemplate::new_biphasic(sampling_rate),  2.0, 0.5, seed_base + 2, sampling_rate),
                    ]
                })
                .collect();

            let mut noise_gens: Vec<WhiteNoise> = (0..channels as usize)
                .map(|ch| WhiteNoise::new(ch as u32 + 1000, 0.1))
                .collect();

            for i in 0..total_chunks {
                let current_chunk_size =
                    ((chunk_size as u64).min(total_samples - i * chunk_size as u64)) as usize;
                let mut buf = vec![0.0_f32; channels as usize * current_chunk_size];

                for ch in 0..channels as usize {
                    let slice = &mut buf[ch * current_chunk_size..(ch + 1) * current_chunk_size];
                    noise_gens[ch].fill_buffer(slice, 1);
                    for unit in &mut channel_units[ch] {
                        unit.fill_buffer(slice, sampling_rate);
                    }
                }

                manager.write_raw_chunk(i, &buf)?;

                if i % 100 == 0 || i == total_chunks - 1 {
                    print!("\r  Signal Progress: {:.1}%", (i + 1) as f32 / total_chunks as f32 * 100.0);
                    use std::io::Write;
                    let _ = std::io::stdout().flush();
                }
            }
        }

        RecordingType::Emg | RecordingType::Ecog => {
            // Sine wave (10 Hz) + low-level white noise, identical across channels.
            let mut sine_gen = SineWave::new(10.0, sampling_rate, 1.0);
            let mut noise = WhiteNoise::new(42, 0.1);

            for i in 0..total_chunks {
                let current_chunk_size =
                    ((chunk_size as u64).min(total_samples - i * chunk_size as u64)) as usize;
                let mut buf = vec![0.0_f32; channels as usize * current_chunk_size];

                sine_gen.fill_buffer(&mut buf, channels as usize);
                let mut noise_buf = vec![0.0_f32; buf.len()];
                noise.fill_buffer(&mut noise_buf, channels as usize);
                for (s, n) in buf.iter_mut().zip(noise_buf.iter()) {
                    *s += n;
                }

                manager.write_raw_chunk(i, &buf)?;

                if i % 100 == 0 || i == total_chunks - 1 {
                    print!("\r  Signal Progress: {:.1}%", (i + 1) as f32 / total_chunks as f32 * 100.0);
                    use std::io::Write;
                    let _ = std::io::stdout().flush();
                }
            }
        }
    }
    println!();

    // ── Write mock epochs if requested ───────────────────────────────────────
    if with_epochs {
        println!("Generating mock epochs...");
        let track_name = "mock_events".to_string();
        let mut events_per_channel = Vec::with_capacity(channels as usize);
        let mut channel_indices = Vec::with_capacity(channels as usize);

        for i in 0..channels {
            let events = dsp_core::mock::epoch::generate_random_events(
                total_samples,
                0.0001,
                i as u32 % 5,
                42 + i as u32,
            );
            events_per_channel.push(events);
            channel_indices.push(i);
        }
        manager.write_events_track(&track_name, &events_per_channel)?;

        rec_meta.tracks.push(dsp_io::recording_meta::TrackMeta::events(
            track_name,
            channel_indices,
            dsp_core::signal::LabelVocabulary::new(vec![
                "Type A".into(), "Type B".into(), "Type C".into(),
                "Type D".into(), "Type E".into(),
            ])
        ));
    }

    println!("Initializing peak pyramid...");
    manager.build_peak_pyramid(&metadata, |p| {
        if (p * 100.0) as u32 % 25 == 0 {
            print!("\r  Progress: {:.0}%", p * 100.0);
            use std::io::Write;
            let _ = std::io::stdout().flush();
        }
    })?;
    println!("\r  Progress: 100%");

    rec_meta.lod_levels_available = metadata.lod_chain.iter().map(|l| l.level).collect();
    rec_meta.save(&output)?;

    println!("Successfully created recording at {:?}", output);
    Ok(())
}
