//! Lab 07 — gRPC transmission layer performance benchmark.
//!
//! Prerequisite: run `lab_neural_generation` first.

use anyhow::Result;
use dsp_io::transmission::grpc_server::start_grpc_server;
use dsp_io::proto::transmission::transmission_service_client::TransmissionServiceClient;
use dsp_io::proto::transmission::{
    channel_id, ChannelId as ProtoChannelId,
    OpenFileRequest, ViewRequest, RunGraphRequest, processing_response,
};
use dsp_io::processing_graph::{ProcessingGraphSpec, SpecNode, SpecWire, ArithOpSpec, ChannelId};
use indicatif::{ProgressBar, ProgressStyle};
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tokio_stream::StreamExt;

fn physical(idx: u32) -> ProtoChannelId {
    ProtoChannelId { kind: Some(channel_id::Kind::Physical(idx)) }
}

#[tokio::main]
async fn main() -> Result<()> {
    let addr = "[::1]:50051".parse()?;
    let zarr_path = "data/lab_neural/neural_recording.zarr";

    println!("gRPC Performance Lab:");
    println!("  Target: {}", zarr_path);
    println!("  Server: {}", addr);

    tokio::spawn(async move {
        if let Err(e) = start_grpc_server(addr).await {
            eprintln!("gRPC server error: {}", e);
        }
    });

    sleep(Duration::from_millis(500)).await;

    let mut client = TransmissionServiceClient::connect(format!("http://{}", addr)).await?;
    println!("  Connected.\n");

    // OpenFile
    let t = Instant::now();
    let meta = client.open_file(OpenFileRequest { path: zarr_path.to_string() }).await?.into_inner();
    println!("[OK] OpenFile: {} channels, {} samples ({:?})", meta.n_channels, meta.total_samples, t.elapsed());

    // FetchView at several zoom levels
    println!("\nBenchmarking FetchView:");
    let test_cases = vec![
        (1_000u64,       1_000u32, "1K samples / 1000px  (raw)"),
        (100_000,        1_000,    "100K samples / 1000px (LOD)"),
        (10_000_000,     1_000,    "10M samples / 1000px  (deep LOD)"),
        (144_000_000,    1_000,    "Full hour / 1000px    (max LOD)"),
    ];

    let pb = ProgressBar::new(test_cases.len() as u64);
    pb.set_style(ProgressStyle::default_bar().template("{msg} {elapsed_precise} [{bar:40}]")?);

    for (count, width, label) in &test_cases {
        let req = ViewRequest {
            zarr_path: zarr_path.to_string(),
            start_sample: 0,
            count: *count,
            width_px: *width,
            channels: vec![physical(0), physical(1), physical(2), physical(3)],
        };
        let t = Instant::now();
        let resp = client.fetch_view(req).await?.into_inner();
        pb.set_message(label.to_string());
        println!("  {:<40} LOD={} pts={} ({:?})", label, resp.lod_level, resp.points_per_channel, t.elapsed());
        pb.inc(1);
    }
    pb.finish_and_clear();

    // RunProcessingGraph: (CH0 * 2.0) + CH1 → bench_out
    println!("\nBenchmarking RunProcessingGraph (streaming):");
    let spec = ProcessingGraphSpec {
        nodes: vec![
            SpecNode::Channel { id: ChannelId::Physical(0) },
            SpecNode::Float { value: 2.0 },
            SpecNode::Arithmetic { op: ArithOpSpec::Multiply },
            SpecNode::Channel { id: ChannelId::Physical(1) },
            SpecNode::Arithmetic { op: ArithOpSpec::Add },
            SpecNode::Fork {
                source_id: ChannelId::Physical(0),
                name: "bench_out".into(),
            },
        ],
        wires: vec![
            SpecWire { from_node: 0, from_output: 0, to_node: 2, to_input: 0 },
            SpecWire { from_node: 1, from_output: 0, to_node: 2, to_input: 1 },
            SpecWire { from_node: 2, from_output: 0, to_node: 4, to_input: 0 },
            SpecWire { from_node: 3, from_output: 0, to_node: 4, to_input: 1 },
            SpecWire { from_node: 4, from_output: 0, to_node: 5, to_input: 0 },
        ],
        sample_rate: meta.sample_rate,
    };

    let req = RunGraphRequest {
        zarr_path: zarr_path.to_string(),
        graph_spec_json: serde_json::to_string(&spec)?,
        total_samples: 40_000 * 60, // 1 minute
        batch_size: 40_000,
        surplus: 1024,
        start_sample: 0,
        count: 40_000 * 60,
    };

    let mut stream = client.run_processing_graph(req).await?.into_inner();
    let pb = ProgressBar::new(100);
    pb.set_style(ProgressStyle::default_bar().template("{msg} {elapsed_precise} [{bar:40}] {percent}%")?);
    pb.set_message("Graph Execution");

    let t = Instant::now();
    while let Some(resp) = stream.next().await {
        match resp?.event {
            Some(processing_response::Event::Progress(p)) => {
                pb.set_position((p.progress * 100.0) as u64);
            }
            Some(processing_response::Event::Complete(c)) => {
                pb.finish_with_message(format!(
                    "Done in {:?} — {} virtual channel(s)", t.elapsed(), c.virtual_channels.len()
                ));
            }
            None => {}
        }
    }

    println!("\ngRPC transmission layer benchmark complete.");
    Ok(())
}
