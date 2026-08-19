//! Lab 04 — Sliding-window processing throughput benchmark.
//!
//! Prerequisite: run `lab_neural_generation` first.

use dsp_io::config::StorageConfig;
use dsp_io::zarr::StorageManager;
use dsp_io::metadata::DatasetMetadata;
use dsp_io::transmission::processing::ProcessingService;
use dsp_io::processing_graph::ChannelId;
use dsp_core::math::arithmetic::add_scalar;
use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Instant;
use console::style;

fn main() -> Result<()> {
    println!("LAB_04  {}", style("Sliding-Window Processing Benchmark").bold().cyan());

    let mut config = StorageConfig::default();
    config.raw_archive_path = "data/lab_neural/neural_recording.zarr".into();

    let manager = StorageManager::new(config.clone())?;
    let total_samples = 40000 * 3600u64;
    let _metadata = DatasetMetadata::new_power_of_two(total_samples);

    let mut service = ProcessingService::new(&manager, None);
    let batch_size = 40000u64;
    let surplus = 1024u64;
    let total_batches = total_samples / batch_size;

    let pb = ProgressBar::new(total_batches.min(500));
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{prefix:.bold.cyan} [{elapsed_precise}] {bar:40.magenta/blue} {pos}/{len} {msg}")
            .unwrap(),
    );
    pb.set_prefix("Processing 1-Hour Stream");

    let channels = vec![ChannelId::Physical(0)];
    let start_bench = Instant::now();
    let mut total_processed = 0u64;

    for i in 0..total_batches.min(500) {
        let start_sample = i * batch_size;

        let mut data = service.fetch_package_with_surplus(
            start_sample as i64,
            batch_size,
            surplus,
            total_samples,
            &channels,
        )?;

        // In-place offset applied to channel 0.
        add_scalar(&mut data, 0.5, &[0u16], 1);

        total_processed += batch_size;
        pb.inc(1);
    }

    let elapsed = start_bench.elapsed();
    let throughput = total_processed as f64 / elapsed.as_secs_f64();
    pb.finish_with_message(format!(
        "Throughput: {} samples/sec",
        style(format!("{:.2e}", throughput)).yellow().bold()
    ));

    println!("\n{} benchmark complete.", style("Processing Plane").green());
    Ok(())
}
