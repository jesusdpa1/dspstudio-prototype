//! Lab 03 — Resolution-aware viewport latency stress test.
//!
//! Prerequisite: run `lab_neural_generation` first to create the data at
//! `data/lab_neural/neural_recording.zarr`.

use dsp_io::config::StorageConfig;
use dsp_io::zarr::StorageManager;
use dsp_io::metadata::DatasetMetadata;
use dsp_io::transmission::ui::UiService;
use dsp_io::processing_graph::ChannelId;
use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle, MultiProgress};
use std::time::Instant;
use console::style;

fn main() -> Result<()> {
    println!("LAB_03  {}", style("UI Latency Stress Test").bold().cyan());

    let mut config = StorageConfig::default();
    config.raw_archive_path = "data/lab_neural/neural_recording.zarr".into();

    let manager = StorageManager::new(config.clone())?;
    let total_samples = 40000 * 3600u64;
    let metadata = DatasetMetadata::new_power_of_two(total_samples);

    let mut ui = UiService::new(&manager, None);
    let screen_width = 1920u32;
    let m = MultiProgress::new();

    let channels = vec![ChannelId::Physical(0), ChannelId::Physical(1)];

    run_scenario(&m, &mut ui, &metadata, total_samples, screen_width, 1, "1s Zoom (Raw)", &channels);
    run_scenario(&m, &mut ui, &metadata, total_samples, screen_width, 10, "10s View (LOD)", &channels);
    run_scenario(&m, &mut ui, &metadata, total_samples, screen_width, 60, "1m Macro", &channels);
    run_scenario(&m, &mut ui, &metadata, total_samples, screen_width, 3600, "Full Hour (Deep LOD)", &channels);

    println!("\n{} stress test complete.", style("UI Transmission").green());
    Ok(())
}

fn run_scenario(
    m: &MultiProgress,
    ui: &mut UiService,
    metadata: &DatasetMetadata,
    total_samples: u64,
    width: u32,
    seconds_per_window: u64,
    label: &str,
    channels: &[ChannelId],
) {
    let samples_per_window = 40000 * seconds_per_window;
    let total_windows = total_samples / samples_per_window;

    let pb = m.add(ProgressBar::new(total_windows.min(100)));
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{prefix:.bold.cyan} [{elapsed_precise}] {bar:40.green/blue} {pos}/{len} ({eta}) {msg}")
            .unwrap(),
    );
    pb.set_prefix(label.to_string());

    let mut total_latency = 0u128;
    let mut successes = 0u64;

    for i in 0..total_windows.min(100) {
        let start = Instant::now();
        match ui.fetch_view(metadata, i * samples_per_window, samples_per_window, width, channels) {
            Ok(_) => {
                total_latency += start.elapsed().as_micros();
                successes += 1;
            }
            Err(_) => {}
        }
        pb.inc(1);
    }

    let avg_latency = if successes > 0 { total_latency / successes as u128 } else { 0 };
    pb.finish_with_message(format!("Avg Latency: {}µs", style(avg_latency).magenta()));
}
