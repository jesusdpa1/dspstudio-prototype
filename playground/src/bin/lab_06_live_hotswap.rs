//! Lab 06 — Live routing: composite view mixing physical and virtual channels.
//!
//! Demonstrates UiService fetching physical Zarr channels alongside processed
//! virtual channels from a VirtualChannelStore, simulating the "filter on/off"
//! toggle that the UI exposes.
//!
//! Prerequisite: run `lab_neural_generation` first.

use dsp_io::config::StorageConfig;
use dsp_io::zarr::StorageManager;
use dsp_io::metadata::DatasetMetadata;
use dsp_io::virtual_channel::VirtualChannelStore;
use dsp_io::transmission::ui::UiService;
use dsp_io::processing_graph::{
    ChannelId, GraphProcessor, ProcessingGraphSpec, SpecNode, SpecWire, ArithOpSpec,
};
use anyhow::Result;
use std::time::Instant;
use console::style;

fn main() -> Result<()> {
    println!("LAB_06  {}", style("Live Routing & Hot-Swap").bold().cyan());

    let zarr_path = std::path::PathBuf::from("data/lab_neural/neural_recording.zarr");
    let mut config = StorageConfig::default();
    config.raw_archive_path = zarr_path.clone();

    let manager = StorageManager::new(config.clone())?;
    let total_samples = 40000 * 60u64; // 1 minute
    let metadata = DatasetMetadata::new_power_of_two(total_samples);

    // State 1: All channels are physical (raw archive).
    println!("\nState 1: all channels from {}", style("Archive (physical)").green());
    {
        let mut ui = UiService::new(&manager, None);
        let channels = vec![
            ChannelId::Physical(0),
            ChannelId::Physical(1),
            ChannelId::Physical(2),
        ];
        let t = Instant::now();
        let view = ui.fetch_view(&metadata, 0, 40000, 1920, &channels)?;
        println!("  Fetch latency: {}µs  points/ch: {}", t.elapsed().as_micros(), view.points_per_channel);
    }

    // State 2: Run a processing graph to produce virtual channel "ch0_gain5x".
    println!("\nState 2: running graph to create {} ...", style("ch0_gain5x").magenta());
    {
        // Graph: CH0 * 5.0 → ch0_gain5x
        let spec = ProcessingGraphSpec {
            nodes: vec![
                SpecNode::Channel { id: ChannelId::Physical(0) },
                SpecNode::Float { value: 5.0 },
                SpecNode::Arithmetic { op: ArithOpSpec::Multiply },
                SpecNode::Fork {
                    source_id: ChannelId::Physical(0),
                    name: "ch0_gain5x".into(),
                },
            ],
            wires: vec![
                SpecWire { from_node: 0, from_output: 0, to_node: 2, to_input: 0 },
                SpecWire { from_node: 1, from_output: 0, to_node: 2, to_input: 1 },
                SpecWire { from_node: 2, from_output: 0, to_node: 3, to_input: 0 },
            ],
            sample_rate: 40_000.0,
        };

        let mut store = VirtualChannelStore::new(&zarr_path)?;
        let processor = GraphProcessor::new(spec);
        let t = Instant::now();
        let vcs = processor.run_full_recording(
            &manager,
            total_samples,
            0,
            total_samples,
            40_000,
            1024,
            &mut store,
            Some(&zarr_path),
            |p| { if (p * 10.0) as u32 % 2 == 0 { eprint!("."); } },
        )?;
        eprintln!();
        println!("  Graph execution: {}ms  outputs: {:?}", t.elapsed().as_millis(), vcs.iter().map(|v| &v.name).collect::<Vec<_>>());
    }

    // State 3: Composite fetch — CH1 from archive, ch0_gain5x from virtual store.
    println!("\nState 3: composite view ({} + {})", style("Physical CH1").green(), style("Virtual ch0_gain5x").magenta());
    {
        let mut store = VirtualChannelStore::new(&zarr_path)?;
        let mut ui = UiService::new(&manager, Some(&mut store));
        let channels = vec![
            ChannelId::Physical(1),
            ChannelId::Virtual("ch0_gain5x".into()),
        ];
        let t = Instant::now();
        let view = ui.fetch_view(&metadata, 0, 40000, 1920, &channels)?;
        println!("  Fetch latency: {}µs  points/ch: {}  lod: {}", t.elapsed().as_micros(), view.points_per_channel, view.lod_level);
    }

    println!("\n{} routing demonstration complete.", style("Live Hotswap").green());
    Ok(())
}
