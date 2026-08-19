# Workspace Overview

DSP Studio is a Cargo workspace of five crates. Each has a single, clearly bounded responsibility.

```
dsp-studio/
├── dsp-core/     Pure DSP — no IO, no UI
├── dsp-io/       Storage + transmission layer
├── dsp-app/      egui/eframe desktop application
├── dsp-cli/      Headless gRPC server binary
└── playground/   Lab binaries — benchmarks, integration tests, code examples
```

---

## Dependency graph

```
dsp-app ──────────────┐
                       ▼
dsp-cli ──────────► dsp-io ──────────► dsp-core
                       ▲
playground ────────────┘
```

`dsp-core` has no dependencies inside the workspace — it is always safe to compile and test without a display server or storage backend.

---

## Crate roles

### `dsp-core`

Pure math and signal-generation primitives. Every function takes `&[f32]` or `Vec<f32>` and returns `Vec<f32>`. No file I/O, no async, no egui.

Key modules:

| Module | Contents |
|--------|----------|
| `signal_gen` | `SineWave`, `WhiteNoise` — deterministic generators for testing |
| `math::arithmetic` | `add_scalar`, `mul_scalar` — in-place channel-masked operations |
| `util::resampling` | `generate_peaks_parallel` — min-max LOD decimation via Rayon |
| `filter` | _(reserved)_ |
| `spectral` | _(reserved)_ |

---

### `dsp-io`

The storage and transmission layer. Everything between bytes on disk and values in a renderer or DSP kernel lives here.

Three planes:

| Plane | Key types |
|-------|-----------|
| **Archive** | `zarr::StorageManager` — Zarr v3 reads/writes, LOD pyramid builder |
| **Virtual** | `virtual_channel::VirtualChannelStore` — mmap-backed named processed channels |
| **Transmission** | `transmission::ui::UiService`, `transmission::processing::ProcessingService`, `transmission::grpc_server` |

Supporting types:

| Type | Purpose |
|------|---------|
| `recording_meta::RecordingMeta` | JSON sidecar with session metadata |
| `processing_graph::ProcessingGraphSpec` | Serializable DAG of DSP nodes |
| `processing_graph::GraphProcessor` | Executes a spec over a full recording |
| `config::StorageConfig` | Path + hardware parameters |
| `metadata::DatasetMetadata` | LOD chain description |

---

### `dsp-app`

egui/eframe desktop GUI. Depends on `dsp-io` via an `IoBridge` that wraps all blocking I/O in tokio `spawn_blocking`. The render thread never blocks.

State machine: `Idle → CheckingFile → Active(SessionState)`.

---

### `dsp-cli`

Thin binary that starts a `MyTransmissionService` gRPC server. Accepts `--serve <addr>` to listen on a socket.

```bash
cargo run -p dsp-cli -- --serve [::1]:50051
```

---

### `playground`

Lab binaries (`cargo run -p lab --bin lab_NN_*`) covering:

| Lab | Topic |
|-----|-------|
| `lab_neural_generation` | Generate a 1-hour 16ch synthetic recording |
| `lab_01_transmission` | Basic `StorageManager` reads |
| `lab_02_signal` | Signal generator inspection |
| `lab_03_ui_latency` | `UiService` viewport latency at multiple zoom levels |
| `lab_04_processing_throughput` | Sliding-window batch throughput |
| `lab_05_mmap_bench` | `VirtualChannelStore` read/write benchmarks |
| `lab_06_live_hotswap` | Composite fetch mixing physical + virtual channels |
| `lab_07_grpc_bench` | gRPC endpoint latency benchmark |
| `lab_08_arith_test` | `ArithOpSpec` serialization round-trip |
| `lab_09_arith_full_test` | `GraphProcessor::evaluate_graph` correctness |
| `lab_10_grpc_e2e` | **End-to-end gRPC correctness test** (no prerequisite) |

---

## Key workspace dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `zarrs` | 0.23 | Zarr v3 storage engine |
| `tonic` | 0.14 | gRPC server + client |
| `tonic-prost` | 0.14 | Prost codec for tonic |
| `prost` | 0.14 | Protocol Buffers serialization |
| `memmap2` | 0.9 | Memory-mapped file I/O |
| `rayon` | 1.10 | Data-parallel resampling |
| `serde` / `serde_json` | 1 | JSON metadata serialization |
| `tokio` | 1 | Async runtime (gRPC + UI bridge) |
| `egui` / `eframe` | 0.34 | Immediate-mode UI |
