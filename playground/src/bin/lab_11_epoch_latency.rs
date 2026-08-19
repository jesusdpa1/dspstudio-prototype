//! Lab 11 — Epoch data transmission latency benchmark.
//!
//! Evaluates the latency of reading epoch data from local storage (Zarr)
//! and compares it with baseline gRPC request-response overhead.

use anyhow::Result;
use dsp_core::mock::epoch::generate_random_events;
use dsp_io::config::StorageConfig;
use dsp_io::zarr::StorageManager;
use dsp_io::metadata::DatasetMetadata;
use dsp_io::transmission::grpc_server::start_grpc_server;
use dsp_io::proto::transmission::transmission_service_client::TransmissionServiceClient;
use dsp_io::proto::transmission::FetchEventsRequest;
use std::time::{Duration, Instant};
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    println!("LAB_11 — Epoch Data Latency Benchmark");

    let temp_dir = tempfile::tempdir()?;
    let zarr_path = temp_dir.path().join("latency_test.zarr");
    let zarr_path_str = zarr_path.to_string_lossy().to_string();

    let sampling_rate = 40000.0;
    let duration_secs = 600.0; // 10 minutes
    let total_samples = (duration_secs * sampling_rate) as u64;
    let channels = 16u16;

    let config = StorageConfig {
        sample_rate: sampling_rate as u32,
        channels,
        chunk_size: 512,
        raw_archive_path: zarr_path.clone(),
        processed_archive_path: zarr_path.with_extension("processed.zarr"),
        shadow_path: zarr_path.with_extension("mmap"),
        default_surplus: 64,
        compression_level: 3,
    };

    let manager = StorageManager::new(config)?;
    let metadata = DatasetMetadata::new_power_of_two(total_samples);
    manager.init_hierarchy(&metadata)?;

    println!("Generating and writing mock epochs ({} samples, {} channels)...", total_samples, channels);
    let mut events_per_channel = Vec::with_capacity(channels as usize);
    for i in 0..channels {
        events_per_channel.push(generate_random_events(total_samples, 0.001, i as u32, 42 + i as u32));
    }
    
    let t_write = Instant::now();
    manager.write_events_track("latency_track", &events_per_channel)?;
    println!("  Write completed in {:?}", t_write.elapsed());

    println!("\nBenchmark: Local Read Latency (StorageManager)");
    let iterations = 100;
    let mut total_duration = Duration::ZERO;
    
    for _ in 0..iterations {
        let t = Instant::now();
        let _ = manager.read_events_channel("latency_track", 0)?;
        total_duration += t.elapsed();
    }
    
    let avg_latency = total_duration / iterations as u32;
    println!("  Avg Local Read Latency (1 channel, all events): {:?}", avg_latency);

    println!("\nBenchmark: Filtered Window Read Latency");
    total_duration = Duration::ZERO;
    for _ in 0..iterations {
        let t = Instant::now();
        let _ = manager.read_events_window("latency_track", 0, 0, 40000)?; // 1 second window
        total_duration += t.elapsed();
    }
    let avg_window_latency = total_duration / iterations as u32;
    println!("  Avg Local Window Read Latency (1s window): {:?}", avg_window_latency);

    // gRPC Benchmark
    println!("\nBenchmark: gRPC Transmission Latency");
    let addr = "[::1]:50052".parse()?;
    tokio::spawn(async move {
        if let Err(e) = start_grpc_server(addr).await {
            eprintln!("gRPC server error: {}", e);
        }
    });

    sleep(Duration::from_millis(500)).await;

    let mut client = TransmissionServiceClient::connect(format!("http://{}", addr)).await?;
    
    total_duration = Duration::ZERO;
    for _ in 0..iterations {
        let req = FetchEventsRequest {
            zarr_path: zarr_path_str.clone(),
            track_name: "latency_track".into(),
            channel_idx: 0,
            start_sample: 0,
            end_sample: 40000,
        };
        let t = Instant::now();
        let _ = client.fetch_events(req).await?.into_inner();
        total_duration += t.elapsed();
    }
    let avg_grpc_latency = total_duration / iterations as u32;
    println!("  Avg gRPC FetchEvents Latency (1s window): {:?}", avg_grpc_latency);

    println!("\nEpoch data transmission latency benchmark complete.");

    Ok(())
}
