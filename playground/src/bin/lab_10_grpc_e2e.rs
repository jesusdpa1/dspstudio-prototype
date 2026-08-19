//! Lab 10 — End-to-end gRPC correctness test.
//!
//! Creates a small, deterministic recording, spins up a gRPC server, and
//! exercises every RPC endpoint, asserting that outputs are numerically correct.
//!
//! No external files required — the recording is generated inline.

use anyhow::{ensure, Result};
use dsp_core::mock::signal::{SignalGenerator, SineWave};
use dsp_io::config::StorageConfig;
use dsp_io::metadata::DatasetMetadata;
use dsp_io::recording_meta::RecordingMeta;
use dsp_io::transmission::grpc_server::start_grpc_server;
use dsp_io::zarr::StorageManager;
use dsp_io::proto::transmission::{
    channel_id, ChannelId as ProtoChannelId,
    OpenFileRequest, SaveMetadataRequest,
    ViewRequest, RunGraphRequest, processing_response,
    RecordingMeta as ProtoRecordingMeta,
};
use dsp_io::proto::transmission::transmission_service_client::TransmissionServiceClient;
use tonic::transport::Channel;
use dsp_io::processing_graph::{ChannelId, ProcessingGraphSpec, SpecNode, SpecWire, ArithOpSpec};
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::sleep;
use tokio_stream::StreamExt;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn physical(idx: u32) -> ProtoChannelId {
    ProtoChannelId { kind: Some(channel_id::Kind::Physical(idx)) }
}

fn pass(label: &str) {
    println!("  [PASS] {}", label);
}

// ── Recording Setup ───────────────────────────────────────────────────────────

const SAMPLE_RATE: f32 = 1_000.0; // 1 kHz — small enough to be fast
const N_CHANNELS: u16 = 4;
const DURATION_S: u64 = 20;
const TOTAL_SAMPLES: u64 = SAMPLE_RATE as u64 * DURATION_S;
const CHUNK_SIZE: usize = 512;

/// Writes a deterministic small recording and returns the path.
fn create_test_recording() -> Result<PathBuf> {
    let zarr_path = PathBuf::from("data/lab10/recording.zarr");
    if zarr_path.exists() {
        std::fs::remove_dir_all(&zarr_path)?;
    }
    std::fs::create_dir_all(zarr_path.parent().unwrap())?;

    let config = StorageConfig {
        sample_rate: SAMPLE_RATE as u32,
        channels: N_CHANNELS,
        chunk_size: CHUNK_SIZE,
        raw_archive_path: zarr_path.clone(),
        processed_archive_path: PathBuf::from("data/lab10/processed.zarr"),
        shadow_path: PathBuf::from("data/lab10/shadow.mmap"),
        default_surplus: 64,
        compression_level: 3,
    };

    let metadata = DatasetMetadata::new_power_of_two(TOTAL_SAMPLES);
    let manager = StorageManager::new(config)?;
    manager.init_hierarchy(&metadata)?;

    // CH0: 10 Hz sine, CH1: constant 1.0, CH2: constant 2.0, CH3: zeros
    let mut generators: Vec<Box<dyn SignalGenerator>> = vec![
        Box::new(SineWave::new(10.0, SAMPLE_RATE, 1.0)), // CH0
        Box::new(SineWave::new(0.0, SAMPLE_RATE, 0.0)),  // CH1 — zero sine = 0
        Box::new(SineWave::new(0.0, SAMPLE_RATE, 0.0)),  // CH2 — will be overwritten below
        Box::new(SineWave::new(0.0, SAMPLE_RATE, 0.0)),  // CH3 — zeros
    ];

    let total_chunks = TOTAL_SAMPLES.div_ceil(CHUNK_SIZE as u64);
    let mut chunk_buf = vec![0.0f32; N_CHANNELS as usize * CHUNK_SIZE];

    for chunk_idx in 0..total_chunks {
        chunk_buf.fill(0.0);
        let chunk_samples = (CHUNK_SIZE as u64).min(TOTAL_SAMPLES - chunk_idx * CHUNK_SIZE as u64) as usize;

        for (ch, generator) in generators.iter_mut().enumerate() {
            let mut tmp = vec![0.0f32; chunk_samples];
            generator.fill_buffer(&mut tmp, 1);
            chunk_buf[ch * CHUNK_SIZE..ch * CHUNK_SIZE + chunk_samples].copy_from_slice(&tmp);
        }

        // CH1 = constant 1.0, CH2 = constant 2.0 (override the zeros from SineWave)
        for s in 0..chunk_samples {
            chunk_buf[1 * CHUNK_SIZE + s] = 1.0;
            chunk_buf[2 * CHUNK_SIZE + s] = 2.0;
        }

        manager.write_raw_chunk(chunk_idx, &chunk_buf)?;
    }

    manager.build_peak_pyramid(&metadata, |_| {})?;

    let mut rec_meta = RecordingMeta::default_for(N_CHANNELS, TOTAL_SAMPLES, SAMPLE_RATE);
    rec_meta.recording_name = "E2E Test Recording".to_string();
    rec_meta.lod_levels_available = metadata.lod_chain.iter().map(|l| l.level).collect();
    rec_meta.save(&zarr_path)?;

    Ok(zarr_path)
}

// ── Test Cases ────────────────────────────────────────────────────────────────

async fn test_open_file(client: &mut TransmissionServiceClient<Channel>, zarr_path: &str) -> Result<ProtoRecordingMeta> {
    let meta = client
        .open_file(OpenFileRequest { path: zarr_path.to_string() })
        .await?
        .into_inner();

    ensure!(meta.n_channels == N_CHANNELS as u32, "wrong channel count");
    ensure!(meta.total_samples == TOTAL_SAMPLES, "wrong total_samples");
    ensure!(meta.sample_rate == SAMPLE_RATE, "wrong sample_rate");
    ensure!(!meta.session_id.is_empty(), "session_id missing");
    ensure!(!meta.lod_levels_available.is_empty(), "no LOD levels");
    pass("OpenFile: metadata fields correct");

    Ok(meta)
}

async fn test_save_metadata(client: &mut TransmissionServiceClient<Channel>, zarr_path: &str, mut meta: ProtoRecordingMeta) -> Result<()> {
    meta.description = "Updated by e2e test".to_string();
    client
        .save_metadata(SaveMetadataRequest {
            zarr_path: zarr_path.to_string(),
            meta: Some(meta.clone()),
        })
        .await?;

    // Reload to verify persistence.
    let reloaded = client
        .open_file(OpenFileRequest { path: zarr_path.to_string() })
        .await?
        .into_inner();
    ensure!(reloaded.description == "Updated by e2e test", "description not persisted");
    pass("SaveMetadata: description persisted across reload");

    Ok(())
}

async fn test_fetch_view_raw(client: &mut TransmissionServiceClient<Channel>, zarr_path: &str) -> Result<()> {
    // Request 100 samples at raw resolution (1px/sample → LOD 0).
    let resp = client
        .fetch_view(ViewRequest {
            zarr_path: zarr_path.to_string(),
            start_sample: 0,
            count: 100,
            width_px: 100, // 1px/sample → raw
            channels: vec![physical(1), physical(2)],
        })
        .await?
        .into_inner();

    ensure!(resp.lod_level == 0, "expected LOD 0 for raw fetch, got {}", resp.lod_level);
    ensure!(!resp.data.is_empty(), "no data returned");

    // CH1 is all 1.0, CH2 is all 2.0.
    let pts = resp.points_per_channel as usize;
    ensure!(pts >= 100, "points_per_channel too small");
    for i in 0..pts.min(100) {
        let ch1 = resp.data[i];
        let ch2 = resp.data[pts + i];
        ensure!((ch1 - 1.0).abs() < 1e-4, "CH1[{}] = {} (expected 1.0)", i, ch1);
        ensure!((ch2 - 2.0).abs() < 1e-4, "CH2[{}] = {} (expected 2.0)", i, ch2);
    }
    pass("FetchView (raw): CH1=1.0 and CH2=2.0 verified");

    Ok(())
}

async fn test_fetch_view_lod(client: &mut TransmissionServiceClient<Channel>, zarr_path: &str) -> Result<()> {
    // Request all 5 seconds across 100px → triggers LOD decimation.
    let resp = client
        .fetch_view(ViewRequest {
            zarr_path: zarr_path.to_string(),
            start_sample: 0,
            count: TOTAL_SAMPLES,
            width_px: 100,
            channels: vec![physical(1)],
        })
        .await?
        .into_inner();

    ensure!(resp.lod_level > 0, "expected LOD > 0 for compressed view, got 0");
    ensure!(!resp.data.is_empty(), "no data returned");

    // With CH1 = constant 1.0, every min/max peak pair should be (1.0, 1.0).
    for chunk in resp.data.chunks(2) {
        let (lo, hi) = (chunk[0], chunk[1]);
        ensure!((lo - 1.0).abs() < 1e-3, "peak min = {} (expected ~1.0)", lo);
        ensure!((hi - 1.0).abs() < 1e-3, "peak max = {} (expected ~1.0)", hi);
    }
    pass("FetchView (LOD): constant CH1 peaks are (1.0, 1.0)");

    Ok(())
}

async fn test_run_processing_graph(client: &mut TransmissionServiceClient<Channel>, zarr_path: &str) -> Result<()> {
    // Graph: CH1 * CH2 → "ch1_times_ch2"   (1.0 * 2.0 = 2.0 everywhere)
    let spec = ProcessingGraphSpec {
        nodes: vec![
            SpecNode::Channel { id: ChannelId::Physical(1) },
            SpecNode::Channel { id: ChannelId::Physical(2) },
            SpecNode::Arithmetic { op: ArithOpSpec::Multiply },
            SpecNode::Fork {
                source_id: ChannelId::Physical(1),
                name: "ch1_times_ch2".into(),
            },
        ],
        wires: vec![
            SpecWire { from_node: 0, from_output: 0, to_node: 2, to_input: 0 },
            SpecWire { from_node: 1, from_output: 0, to_node: 2, to_input: 1 },
            SpecWire { from_node: 2, from_output: 0, to_node: 3, to_input: 0 },
        ],
        sample_rate: SAMPLE_RATE,
    };

    let req = RunGraphRequest {
        zarr_path: zarr_path.to_string(),
        graph_spec_json: serde_json::to_string(&spec)?,
        total_samples: TOTAL_SAMPLES,
        batch_size: 512,
        surplus: 64,
        start_sample: 0,
        count: TOTAL_SAMPLES,
    };

    let mut stream = client.run_processing_graph(req).await?.into_inner();
    let mut last_progress = 0.0f32;
    let mut completed = false;
    let mut virtual_channels = vec![];

    while let Some(resp) = stream.next().await {
        match resp?.event {
            Some(processing_response::Event::Progress(p)) => {
                ensure!(p.progress >= last_progress, "progress went backwards");
                last_progress = p.progress;
            }
            Some(processing_response::Event::Complete(c)) => {
                completed = true;
                virtual_channels = c.virtual_channels;
            }
            None => {}
        }
    }

    ensure!(completed, "stream ended without Complete event");
    ensure!(last_progress > 0.0, "no progress events received");
    ensure!(virtual_channels.len() == 1, "expected 1 virtual channel, got {}", virtual_channels.len());
    ensure!(virtual_channels[0].name == "ch1_times_ch2", "wrong virtual channel name");
    pass("RunProcessingGraph: stream completed, 1 virtual channel created");

    // Now fetch the virtual channel and verify values = 2.0.
    let resp = client
        .fetch_view(ViewRequest {
            zarr_path: zarr_path.to_string(),
            start_sample: 0,
            count: 100,
            width_px: 100,
            channels: vec![ProtoChannelId {
                kind: Some(channel_id::Kind::Virtual("ch1_times_ch2".into())),
            }],
        })
        .await?
        .into_inner();

    ensure!(resp.lod_level == 0, "expected raw LOD for virtual channel read");
    let pts = resp.points_per_channel as usize;
    for i in 0..pts.min(100) {
        let v = resp.data[i];
        ensure!((v - 2.0).abs() < 1e-3, "ch1_times_ch2[{}] = {} (expected 2.0)", i, v);
    }
    pass("Virtual channel read: ch1_times_ch2 = 2.0 throughout");

    Ok(())
}

// ── Entry Point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    println!("LAB_10  End-to-End gRPC Correctness Test");
    println!("  {}×{}s @ {}Hz", N_CHANNELS, DURATION_S, SAMPLE_RATE);

    println!("\nCreating test recording...");
    let zarr_path = create_test_recording()?;
    let zarr_str = zarr_path.to_str().unwrap();
    println!("  Written to {:?}", zarr_path);

    let addr = "[::1]:50052".parse()?; // distinct port from lab_07
    tokio::spawn(async move {
        if let Err(e) = start_grpc_server(addr).await {
            eprintln!("Server error: {}", e);
        }
    });
    sleep(Duration::from_millis(300)).await;

    let mut client = TransmissionServiceClient::connect(format!("http://{}", addr)).await?;
    println!("  Connected to gRPC server at {}\n", addr);

    let mut pass_count = 0u32;
    let mut fail_count = 0u32;

    macro_rules! run {
        ($test:expr) => {
            match $test {
                Ok(_) => { pass_count += 1; }
                Err(e) => { eprintln!("  {}", e); fail_count += 1; }
            }
        };
    }

    let meta = test_open_file(&mut client, zarr_str).await;
    match meta {
        Ok(m) => {
            pass_count += 1;
            run!(test_save_metadata(&mut client, zarr_str, m).await);
        }
        Err(e) => { eprintln!("  {}", e); fail_count += 1; }
    }

    run!(test_fetch_view_raw(&mut client, zarr_str).await);
    run!(test_fetch_view_lod(&mut client, zarr_str).await);
    run!(test_run_processing_graph(&mut client, zarr_str).await);

    println!("\n─────────────────────────────────────");
    println!("  PASSED: {}  FAILED: {}", pass_count, fail_count);

    if fail_count > 0 {
        std::process::exit(1);
    }
    Ok(())
}
