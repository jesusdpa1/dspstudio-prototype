//! Lab 12 — Detection Performance Benchmark.
//!
//! Measures the throughput of the parallel threshold detectors on synthetic data.

use dsp_core::detection::single::SingleThresholdDetector;
use dsp_core::detection::double::{DoubleThresholdDetector, DoubleThresholdMode};
use dsp_core::detection::{DetectionDetector, CrossingDirection};
use dsp_core::mock::signal::{SignalGenerator, SineWave, WhiteNoise};
use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Instant;
use console::style;

fn main() -> Result<()> {
    println!("LAB_12  {}", style("Sparse Event Detection Benchmark").bold().cyan());

    let n_channels = 128;
    let samples_per_channel = 1_000_000;
    let total_samples = (n_channels * samples_per_channel) as u64;

    println!("Configuration:");
    println!("  Channels: {}", style(n_channels).yellow());
    println!("  Samples/Ch: {}", style(samples_per_channel).yellow());
    println!("  Total points: {}\n", style(format!("{:.1}M", total_samples as f64 / 1_000_000.0)).yellow());

    // ── Data Generation ──────────────────────────────────────────────────────
    let mut data = vec![0.0f32; total_samples as usize];
    
    // Mix a sine wave with noise to ensure frequent crossings.
    let mut sine = SineWave::new(440.0, 40_000.0, 0.4);
    let mut noise = WhiteNoise::new(42, 0.2);
    
    sine.fill_buffer(&mut data, n_channels);
    let mut noise_buf = vec![0.0f32; data.len()];
    noise.fill_buffer(&mut noise_buf, n_channels);
    for i in 0..data.len() {
        data[i] += noise_buf[i];
    }

    // ── Single Threshold Benchmark ───────────────────────────────────────────
    let single_detector = SingleThresholdDetector::new(
        0.3, 
        CrossingDirection::Both, 
        10, // 10 samples refractory
        1,  // Label Pos
        2   // Label Neg
    );

    let pb = ProgressBar::new(10);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{prefix:.bold.cyan} [{elapsed_precise}] {bar:40.magenta/blue} {msg}")
            .unwrap(),
    );
    pb.set_prefix("Single Threshold");

    let start = Instant::now();
    let mut total_events = 0;
    for _ in 0..10 {
        let events = single_detector.detect(&data, n_channels, 0);
        total_events += events.len();
        pb.inc(1);
    }
    let elapsed = start.elapsed();
    let throughput = (total_samples * 10) as f64 / elapsed.as_secs_f64();
    
    pb.finish_with_message(format!(
        "{} events, {} samples/sec",
        style(total_events / 10).yellow(),
        style(format!("{:.2e}", throughput)).green().bold()
    ));

    // ── Double Threshold (Hysteresis) Benchmark ──────────────────────────────
    let double_detector = DoubleThresholdDetector::new(
        -0.2, 
        0.2, 
        DoubleThresholdMode::Hysteresis, 
        20, 
        10, 
        20
    );

    let pb = ProgressBar::new(10);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{prefix:.bold.cyan} [{elapsed_precise}] {bar:40.magenta/blue} {msg}")
            .unwrap(),
    );
    pb.set_prefix("Double Threshold");

    let start = Instant::now();
    let mut total_events = 0;
    for _ in 0..10 {
        let events = double_detector.detect(&data, n_channels, 0);
        total_events += events.len();
        pb.inc(1);
    }
    let elapsed = start.elapsed();
    let throughput = (total_samples * 10) as f64 / elapsed.as_secs_f64();

    pb.finish_with_message(format!(
        "{} events, {} samples/sec",
        style(total_events / 10).yellow(),
        style(format!("{:.2e}", throughput)).green().bold()
    ));

    println!("\n{} benchmark complete.", style("Detection Plane").green());
    Ok(())
}
