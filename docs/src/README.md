# DSP Studio

A desktop application for visualizing and processing neural and biomedical recordings — EMG, EEG, ECG, and similar multi-channel time-series signals.

Built with Rust, [egui](https://github.com/emilk/egui), and [Zarr v3](https://zarr.dev/) storage.

---

## What it does

- **Opens** large multi-channel recordings stored as Zarr v3 archives (hours of 40 kHz data, 16+ channels).
- **Visualizes** recordings at any zoom level using a pre-built LOD pyramid — rendering 1 hour of data is as fast as rendering 1 second.
- **Processes** signals through a serializable node graph (arithmetic, filtering, custom kernels) and stores results as virtual channels alongside the original data.
- **Serves** all of the above over gRPC so a headless server (`dsp-cli`) can drive a remote UI or a Python analysis script.

---

## Quick start

```bash
# Generate a 1-hour synthetic neural recording for testing
cargo run -p lab --bin lab_neural_generation

# Run the GUI (opens the recording above)
cargo run -p dsp-app --release

# Start a headless gRPC server
cargo run -p dsp-cli -- --serve [::1]:50051

# Run the end-to-end gRPC correctness test
cargo run -p lab --bin lab_10_grpc_e2e
```

---

## Where to go next

| Goal | Chapter |
|------|---------|
| Understand the workspace crates and their roles | [Workspace Overview](./overview.md) |
| Learn how data is stored (Zarr, LOD pyramids, virtual channels) | [Storage Foundation](./io_foundation.md) |
| Write code that reads or writes recording data | [Data Access Guide](./data_access.md) |
| Build or run a DSP processing graph | [Processing Graph](./processing_graph.md) |
| Use the gRPC API from Rust or another language | [gRPC API](./grpc_api.md) |
| Understand the pure DSP primitives | [dsp-core API](./dsp_core_api.md) |
| Understand how the egui app is structured | [UI Architecture](./ui_architecture.md) |
