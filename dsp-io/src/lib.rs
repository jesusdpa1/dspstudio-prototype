//! # dsp-io — Storage & Transmission Layer
//!
//! `dsp-io` owns everything between raw bytes on disk and the values a DSP
//! kernel or a UI renderer consumes.  It has three distinct planes:
//!
//! | Plane | What it does | Key type |
//! |-------|-------------|----------|
//! | **Archive** | Immutable Zarr v3 on-disk storage (raw + LOD pyramids) | [`zarr::StorageManager`] |
//! | **Virtual** | Mmap-backed processed-channel store | [`virtual_channel::VirtualChannelStore`] |
//! | **Transmission** | Resolution-aware viewport service, surplus-windowed batch service, gRPC server | `transmission::*` |
//!
//! ---
//!
//! ## Storage layout
//!
//! ```text
//! ~/datasets/neural_recording/
//!   neural_recording.zarr/        ← Zarr v3 archive
//!     /raw                        ← [n_ch × total_samples] f32  (full resolution)
//!     /peaks/
//!       lod_1                     ← [n_ch × (total_samples/16) × 2] f32  (min/max)
//!       lod_2                     ← [n_ch × (total_samples/256) × 2] f32
//!       …
//!   neural_recording.json         ← RecordingMeta sidecar
//!   tmp/
//!     ch0_drv.mmap                ← virtual channel (processing output)
//!     ch1_drv.mmap
//! ```
//!
//! ---
//!
//! ## Getting data — quick start
//!
//! ### 1. Read raw samples from a recording
//!
//! ```rust,no_run
//! use dsp_io::config::StorageConfig;
//! use dsp_io::zarr::StorageManager;
//! use std::path::PathBuf;
//!
//! let mut config = StorageConfig::default();
//! config.raw_archive_path = PathBuf::from("path/to/recording.zarr");
//!
//! let manager = StorageManager::new(config)?;
//!
//! // Read 1 000 raw samples from offset 0 for channels 0 and 1.
//! // Returns channel-major Vec<f32>: [ch0_s0..ch0_s999, ch1_s0..ch1_s999]
//! let raw = manager.read_raw_window_masked(0, 1_000, &[0u16, 1u16])?;
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! ### 2. Serve a resolution-aware viewport to the UI
//!
//! ```rust,no_run
//! use dsp_io::config::StorageConfig;
//! use dsp_io::zarr::StorageManager;
//! use dsp_io::metadata::DatasetMetadata;
//! use dsp_io::transmission::ui::UiService;
//! use dsp_io::processing_graph::ChannelId;
//! use std::path::PathBuf;
//!
//! let mut config = StorageConfig::default();
//! config.raw_archive_path = PathBuf::from("recording.zarr");
//! let manager = StorageManager::new(config)?;
//! let meta = dsp_io::recording_meta::RecordingMeta::load(&PathBuf::from("recording.zarr"))?;
//! let dataset = DatasetMetadata::new_power_of_two(meta.total_samples);
//!
//! let mut ui = UiService::new(&manager, None);
//!
//! // Fetch whatever LOD the screen width warrants — raw at high zoom, decimated at low.
//! let view = ui.fetch_view(
//!     &dataset,
//!     /*start_sample=*/ 0,
//!     /*count=*/        40_000,   // 1 second at 40 kHz
//!     /*width_px=*/     1920,
//!     &[ChannelId::Physical(0), ChannelId::Physical(1)],
//! )?;
//! // view.lod_level == 0 → data is raw samples (points_per_channel floats per channel)
//! // view.lod_level  > 0 → data is peak pairs  (points_per_channel * 2 floats per channel)
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! ### 3. Run a DSP graph and write virtual channels
//!
//! ```rust,no_run
//! use dsp_io::config::StorageConfig;
//! use dsp_io::zarr::StorageManager;
//! use dsp_io::virtual_channel::VirtualChannelStore;
//! use dsp_io::processing_graph::{
//!     ChannelId, GraphProcessor, ProcessingGraphSpec,
//!     SpecNode, SpecWire, ArithOpSpec,
//! };
//! use std::path::PathBuf;
//!
//! let zarr_path = PathBuf::from("recording.zarr");
//! let mut config = StorageConfig::default();
//! config.raw_archive_path = zarr_path.clone();
//! let manager = StorageManager::new(config)?;
//! let mut store = VirtualChannelStore::new(&zarr_path)?;
//!
//! // Graph: CH0 * 2.0 → ch00_drv  (auto-derived slot, destructive)
//! let spec = ProcessingGraphSpec {
//!     nodes: vec![
//!         SpecNode::Channel    { id: ChannelId::Physical(0) },
//!         SpecNode::Float      { value: 2.0 },
//!         SpecNode::Arithmetic { op: ArithOpSpec::Multiply },
//!         SpecNode::Output     { source_id: ChannelId::Physical(0) }, // → "ch00_drv"
//!     ],
//!     wires: vec![
//!         SpecWire { from_node: 0, from_output: 0, to_node: 2, to_input: 0 },
//!         SpecWire { from_node: 1, from_output: 0, to_node: 2, to_input: 1 },
//!         SpecWire { from_node: 2, from_output: 0, to_node: 3, to_input: 0 },
//!     ],
//!     sample_rate: 40_000.0,
//! };
//!
//! let processor = GraphProcessor::new(spec);
//! let virtual_channels = processor.run_full_recording(
//!     &manager,
//!     /*total_samples=*/ 144_000_000,
//!     /*start_sample=*/  0,
//!     /*count=*/         144_000_000,
//!     /*batch_size=*/    40_000,
//!     /*surplus=*/       1_024,
//!     &mut store,
//!     /*zarr_path=*/     Some(zarr_path.as_path()),
//!     |progress| eprintln!("Progress: {:.0}%", progress * 100.0),
//! )?;
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! ### 4. Fetch data for a DSP kernel (surplus-windowed)
//!
//! ```rust,no_run
//! use dsp_io::config::StorageConfig;
//! use dsp_io::zarr::StorageManager;
//! use dsp_io::transmission::processing::ProcessingService;
//! use dsp_io::processing_graph::ChannelId;
//! use std::path::PathBuf;
//!
//! let mut config = StorageConfig::default();
//! config.raw_archive_path = PathBuf::from("recording.zarr");
//! let manager = StorageManager::new(config)?;
//! let mut service = ProcessingService::new(&manager, None);
//!
//! // Fetch 40 000 samples + 1 024 surplus on each side for filter edge-effect prevention.
//! // Returns channel-major Vec<f32> of length channels * (batch + 2 * surplus).
//! let data = service.fetch_package_with_surplus(
//!     /*start=*/         0i64,
//!     /*count=*/         40_000u64,
//!     /*surplus=*/       1_024u64,
//!     /*total_samples=*/ 144_000_000u64,
//!     &[ChannelId::Physical(0)],
//! )?;
//! // Strip surplus before writing output: data[1024..1024 + 40_000]
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! ### 5. Read / write virtual channels directly
//!
//! ```rust,no_run
//! use dsp_io::virtual_channel::VirtualChannelStore;
//! use std::path::PathBuf;
//!
//! let zarr_path = PathBuf::from("recording.zarr");
//! let mut store = VirtualChannelStore::new(&zarr_path)?;
//! let total_samples = 144_000_000u64;
//!
//! // Write a block of processed samples.
//! let processed: Vec<f32> = vec![0.0; 40_000];
//! store.write_window("ch0_processed", /*start=*/ 0, total_samples, &processed)?;
//!
//! // Read a window back.
//! let readback = store.read_window("ch0_processed", 0, 40_000, total_samples)?;
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! ---
//!
//! ## Data layout convention
//!
//! All multi-channel buffers in this crate use **channel-major** (C-order) layout:
//!
//! ```text
//! [ch0_s0, ch0_s1, …, ch0_sN,  ch1_s0, ch1_s1, …, ch1_sN,  …]
//!  ──────── channel 0 ────────  ─────── channel 1 ───────────
//! ```
//!
//! Peak arrays double the sample count with interleaved `(min, max)` pairs:
//!
//! ```text
//! [ch0_min0, ch0_max0, ch0_min1, ch0_max1, …,  ch1_min0, ch1_max0, …]
//! ```

pub mod config;
pub mod metadata;
pub mod session;
pub mod recording_meta;
pub mod virtual_channel;
pub mod processing_graph;
pub mod processing_history;
pub mod mmap;
pub mod zarr;
pub mod dataset_cache;
pub mod transmission;
pub mod metrics;

pub mod proto {
    pub mod transmission {
        tonic::include_proto!("dsp_studio.transmission");
    }
}
