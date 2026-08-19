//! Re-encode an existing recording with zstd-compressed Zarr arrays.
//!
//! Recordings created before compression was enabled store `/raw` and the peak
//! pyramid uncompressed. This command copies the raw data into a fresh,
//! compressed store, rebuilds the peak pyramid, copies any event tracks, and
//! copies the metadata sidecar — reclaiming disk on existing datasets.
//!
//! Derived spike artifacts (waveforms / PCA / flat label arrays) are NOT
//! migrated; re-run the processing graph to regenerate them if needed.

use anyhow::{Context, Result};
use dsp_io::config::StorageConfig;
use dsp_io::metadata::DatasetMetadata;
use dsp_io::recording_meta::RecordingMeta;
use dsp_io::zarr::StorageManager;
use std::io::Write;
use std::path::{Path, PathBuf};

pub fn run(input: PathBuf, output: PathBuf, level: i32) -> Result<()> {
    if !RecordingMeta::exists(&input) {
        anyhow::bail!("No recording metadata sidecar found next to {:?}", input);
    }
    if output.exists() {
        anyhow::bail!("Output path {:?} already exists; refusing to overwrite", output);
    }

    let meta = RecordingMeta::load(&input)?;
    let total_samples = meta.total_samples;
    let channels = meta.n_channels;
    let chunk_size = 32768usize; // 2^15, matches create-recording
    let dataset = DatasetMetadata::new_power_of_two(total_samples);

    println!("Re-encoding {:?} -> {:?}", input, output);
    println!("  Channels: {}  Samples: {}  zstd level: {}", channels, total_samples, level);

    // Source store (reads ignore compression_level).
    let mut src_cfg = StorageConfig::default();
    src_cfg.raw_archive_path = input.clone();
    src_cfg.channels = channels;
    src_cfg.chunk_size = chunk_size;
    let src = StorageManager::new(src_cfg)?;

    // Destination store with zstd-compressed arrays.
    let mut dst_cfg = StorageConfig::default();
    dst_cfg.raw_archive_path = output.clone();
    dst_cfg.channels = channels;
    dst_cfg.chunk_size = chunk_size;
    dst_cfg.compression_level = level;
    let dst = StorageManager::new(dst_cfg)?;
    dst.init_hierarchy(&dataset)?;

    // ── Copy raw data chunk-by-chunk ─────────────────────────────────────────
    let all_channels: Vec<u16> = (0..channels).collect();
    let total_chunks = (total_samples + chunk_size as u64 - 1) / chunk_size as u64;
    for chunk_idx in 0..total_chunks {
        let start = chunk_idx * chunk_size as u64;
        let count = (chunk_size as u64).min(total_samples - start);
        let data = src
            .read_raw_window_masked(start, count, &all_channels)
            .with_context(|| format!("reading raw chunk {}", chunk_idx))?;
        dst.write_raw_chunk(chunk_idx, &data)?;
        if chunk_idx % 50 == 0 || chunk_idx + 1 == total_chunks {
            print!("\r  Raw: {:.0}%", (chunk_idx + 1) as f32 / total_chunks as f32 * 100.0);
            let _ = std::io::stdout().flush();
        }
    }
    println!();

    // ── Rebuild the peak pyramid in the compressed store ─────────────────────
    println!("  Rebuilding peak pyramid...");
    dst.build_peak_pyramid(&dataset, |_| {})?;

    // ── Copy event tracks (sample_offsets / label_ids) ───────────────────────
    let mut copied = 0usize;
    for track in &meta.tracks {
        if !track.family.is_events() || !src.events_track_exists(&track.name) {
            continue;
        }
        let mut per_channel = Vec::with_capacity(channels as usize);
        for ch in 0..channels {
            per_channel.push(src.read_events_channel(&track.name, ch).unwrap_or_default());
        }
        dst.write_events_track(&track.name, &per_channel)?;
        copied += 1;
    }
    if copied > 0 {
        println!("  Copied {} event track(s).", copied);
    }
    for track in &meta.tracks {
        if src.has_flat_artifacts(&track.name) {
            println!(
                "  NOTE: spike artifacts for '{}' were not migrated; re-run processing to regenerate.",
                track.name
            );
        }
    }

    // ── Copy the metadata sidecar ────────────────────────────────────────────
    meta.save(&output)?;

    let src_size = dir_size(&input).unwrap_or(0);
    let dst_size = dir_size(&output).unwrap_or(0);
    let ratio = if dst_size > 0 { src_size as f64 / dst_size as f64 } else { 0.0 };
    println!(
        "Done. Size {} -> {} ({:.2}x smaller)",
        human(src_size),
        human(dst_size),
        ratio
    );
    Ok(())
}

/// Total size in bytes of all files under `path` (recursively).
fn dir_size(path: &Path) -> Result<u64> {
    let mut total = 0;
    if path.is_file() {
        return Ok(std::fs::metadata(path)?.len());
    }
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        if ft.is_dir() {
            total += dir_size(&entry.path()).unwrap_or(0);
        } else if ft.is_file() {
            total += entry.metadata().map(|m| m.len()).unwrap_or(0);
        }
    }
    Ok(total)
}

fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    format!("{:.2} {}", size, UNITS[unit])
}
