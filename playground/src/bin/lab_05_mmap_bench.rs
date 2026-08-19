//! Lab 05 — VirtualChannelStore read/write benchmark.
//!
//! Benchmarks the mmap-backed virtual channel layer: write throughput, read
//! latency from UiService, and read latency from ProcessingService.
//!
//! Prerequisite: run `lab_neural_generation` first.

use dsp_io::config::StorageConfig;
use dsp_io::zarr::StorageManager;
use dsp_io::metadata::DatasetMetadata;
use dsp_io::virtual_channel::VirtualChannelStore;
use dsp_io::transmission::ui::UiService;
use dsp_io::transmission::processing::ProcessingService;
use dsp_io::processing_graph::ChannelId;
use dsp_core::math::arithmetic::mul_scalar;
use anyhow::Result;
use std::time::Instant;
use console::style;

fn main() -> Result<()> {
    println!("LAB_05  {}", style("Virtual Channel (Mmap) Benchmark").bold().cyan());

    let zarr_path = std::path::PathBuf::from("data/lab_neural/neural_recording.zarr");
    let mut config = StorageConfig::default();
    config.raw_archive_path = zarr_path.clone();

    let manager = StorageManager::new(config)?;
    let total_samples = 40000 * 60u64; // 1 minute
    let metadata = DatasetMetadata::new_power_of_two(total_samples);

    // 1. Create virtual channels and write synthetic data.
    let mut store = VirtualChannelStore::new(&zarr_path)?;
    let channel_name = "lab_05_gain";

    println!("Writing 2x-gain virtual channel ({} samples)...", total_samples);
    let start_write = Instant::now();

    // Generate a simple ramp and apply 2x gain.
    let mut data: Vec<f32> = (0..total_samples as usize).map(|i| i as f32 / total_samples as f32).collect();
    mul_scalar(&mut data, 2.0, &[0u16], 1);
    store.write_window(channel_name, 0, total_samples, &data)?;
    store.flush_all()?;

    println!("  Write: {}ms", style(start_write.elapsed().as_millis()).yellow());

    // 2. Bench UiService reads (with virtual channel).
    let virtual_ch = ChannelId::Virtual(channel_name.to_string());
    let phys_ch = ChannelId::Physical(0);
    let channels = vec![phys_ch, virtual_ch];

    let mut ui = UiService::new(&manager, Some(&mut store));
    let iterations = 1000u32;

    let start_ui = Instant::now();
    for _ in 0..iterations {
        ui.fetch_view(&metadata, 40000, 40000, 1920, &channels)?;
    }
    let ui_lat = start_ui.elapsed().as_micros() / iterations as u128;
    println!("UiService view (physical + virtual): {}µs avg", style(ui_lat).magenta());

    // 3. Bench ProcessingService reads.
    let mut store2 = VirtualChannelStore::new(&zarr_path)?;
    let proc_channels = vec![ChannelId::Virtual(channel_name.to_string())];
    let mut proc = ProcessingService::new(&manager, Some(&mut store2));

    let start_proc = Instant::now();
    for _ in 0..iterations {
        proc.fetch_package_with_surplus(40000, 40000, 1024, total_samples, &proc_channels)?;
    }
    let proc_lat = start_proc.elapsed().as_micros() / iterations as u128;
    println!("ProcessingService batch (virtual):   {}µs avg", style(proc_lat).magenta());

    println!("\n{} benchmark complete.", style("Virtual Channel Plane").green());
    Ok(())
}
