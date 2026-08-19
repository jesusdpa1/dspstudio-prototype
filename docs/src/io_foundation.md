# Storage Foundation

This page describes how DSP Studio stores, indexes, and retrieves recording data.

---

## Filesystem layout

```text
~/datasets/neural_recording/
  neural_recording.zarr/          ← Zarr v3 archive (directory store)
    /raw                          ← [n_channels × total_samples] f32
    /peaks/
      lod_1                       ← [n_channels × (total_samples/16) × 2] f32
      lod_2                       ← [n_channels × (total_samples/256) × 2] f32
      lod_3                       ← [n_channels × (total_samples/4096) × 2] f32
      …
  neural_recording.json           ← RecordingMeta sidecar
  processing.json                 ← ProcessingHistory log (optional)
  tmp/
    ch0_gain.mmap                 ← virtual channel (processing output)
    ch1_drv.mmap
```

The `.zarr` directory is the Zarr v3 store. The `.json` sidecar and `processing.json` live **next to** it, not inside it. The `tmp/` directory is created automatically by `VirtualChannelStore`.

---

## RecordingMeta sidecar

`recording_meta::RecordingMeta` is serialized to a pretty-printed JSON file whenever metadata changes. It stores high-level facts about the acquisition and the available virtual channels.

```json
{
  "session_id": "a1b2c3d4-...",
  "recording_name": "Pilot Session 01",
  "recording_type": "EMG",
  "description": "16-channel forearm EMG",
  "sample_rate": 40000.0,
  "n_channels": 16,
  "total_samples": 144000000,
  "channel_names": ["CH0", "CH1", "CH2", "..."],
  "lod_levels_available": [1, 2, 3],
  "created_at": "1713312000",
  "virtual_channels": [
    {
      "name": "ch0_gain",
      "source_channel_idx": 0,
      "created_at": "1713315000"
    }
  ]
}
```

Key invariants:
- `channel_names.len() == n_channels`
- `lod_levels_available` lists only LOD levels whose Zarr arrays exist **and are fully written**
- `session_id` is generated once on first save and never changed
- `virtual_channels` is updated by `run_full_recording` after a graph executes

---

## ProcessingHistory log

`processing_history::ProcessingHistory` is serialized to `processing.json`. It maintains a per-channel log of every transformation applied to the data.

```json
{
  "channels": {
    "ch0_gain": [
      {
        "timestamp": "1713315000",
        "label": "multiply",
        "graph_spec_json": "{\"nodes\":[...], \"wires\":[...] }"
      }
    ]
  }
}
```

The `graph_spec_json` field contains the full `ProcessingGraphSpec` used to generate that specific virtual channel state, allowing for perfect reproducibility.

### API

```rust
use dsp_io::recording_meta::RecordingMeta;
use std::path::PathBuf;

let zarr_path = PathBuf::from("recording.zarr");

// Load existing sidecar.
let meta = RecordingMeta::load(&zarr_path)?;

// Create a new sidecar for a fresh recording.
let mut meta = RecordingMeta::default_for(/*n_channels=*/ 16, /*total_samples=*/ 144_000_000, /*sample_rate=*/ 40_000.0);
meta.recording_name = "My Session".into();
meta.save(&zarr_path)?;

// Check if a sidecar exists.
if RecordingMeta::exists(&zarr_path) { /* … */ }

// Get the sidecar path for a given zarr path.
let json_path = RecordingMeta::sidecar_path(&zarr_path); // "recording.json"
```

---

## Zarr storage (`StorageManager`)

`zarr::StorageManager` wraps the Zarr v3 store. It is cheap to construct — it only holds an `Arc<FilesystemStore>`.

### Raw data array

Shape: `[n_channels, total_samples]`, dtype `f32`, chunked at `[n_channels, chunk_size]`.

Default chunk size: `32 768` samples (~0.8 s at 40 kHz). Each chunk is independent, enabling random seeks with no full-scan overhead.

### Peak pyramid arrays

Each LOD level is a separate Zarr array at `/peaks/lod_N`:

| Level | Shape | Decimation | Coverage at 40 kHz |
|-------|-------|------------|-------------------|
| 1 | `[n_ch, total/16, 2]` | 16× | ~0.4 ms/point |
| 2 | `[n_ch, total/256, 2]` | 256× | ~6.4 ms/point |
| 3 | `[n_ch, total/4096, 2]` | 4096× | ~102 ms/point |
| 4 | `[n_ch, total/65536, 2]` | 65536× | ~1.6 s/point |

The `2` in the shape is the `[min, max]` pair.

### Creating a new archive

```rust
use dsp_io::config::StorageConfig;
use dsp_io::metadata::DatasetMetadata;
use dsp_io::zarr::StorageManager;
use std::path::PathBuf;

let mut config = StorageConfig::default();
config.raw_archive_path = PathBuf::from("new_recording.zarr");
config.channels = 16;
config.sample_rate = 40_000;

let total_samples = 40_000u64 * 3600; // 1 hour
let metadata = DatasetMetadata::new_power_of_two(total_samples);
let manager = StorageManager::new(config)?;

// Initializes /raw and all /peaks/lod_N arrays (filled with zeros).
manager.init_hierarchy(&metadata)?;
```

### Writing raw chunks

Data must be provided in **channel-major** layout: all samples for channel 0 first, then channel 1, etc.

```rust
// chunk_buf layout: [ch0_s0..ch0_sN, ch1_s0..ch1_sN, …]
let mut chunk_buf = vec![0.0f32; n_channels * chunk_size];
// … fill chunk_buf from your signal source …
manager.write_raw_chunk(/*chunk_idx=*/ 0, &chunk_buf)?;
```

### Building the peak pyramid

Call once after all raw chunks have been written. Reads raw data in batches, computes min-max pairs via `dsp_core::util::resampling::generate_peaks_parallel`, and writes every LOD level.

```rust
manager.build_peak_pyramid(&metadata, |progress| {
    println!("LOD build: {:.0}%", progress * 100.0);
})?;
```

### Reading data

```rust
// Raw window — channel-major Vec<f32>
let raw = manager.read_raw_window_masked(
    /*start_sample=*/ 0,
    /*count=*/        40_000,
    /*channels=*/     &[0u16, 1u16],
)?;
// raw = [ch0_s0..ch0_s39999, ch1_s0..ch1_s39999]

// Peak window — channel-major, interleaved min/max pairs
let peaks = manager.read_peak_window_masked(
    /*lod_level=*/ 1,
    /*lod_start=*/ 0,
    /*lod_count=*/ 100,
    /*channels=*/  &[0u16],
)?;
// peaks = [ch0_min0, ch0_max0, ch0_min1, ch0_max1, …]
```

---

## Virtual channels (`VirtualChannelStore`)

Virtual channels are the outputs of processing graphs. Each is a flat `f32` array stored as a memory-mapped file.

```text
tmp/
  ch0_gain.mmap    ← total_samples × sizeof(f32) bytes
```

The store lazily creates files on first write and reopens them across sessions.

### API

```rust
use dsp_io::virtual_channel::VirtualChannelStore;
use std::path::PathBuf;

let zarr_path = PathBuf::from("recording.zarr");
let mut store = VirtualChannelStore::new(&zarr_path)?;

let total_samples = 144_000_000u64;

// Open or create a channel (must be called before writing).
store.open_or_create("ch0_gain", total_samples)?;

// Write a window of processed samples.
let processed = vec![1.0f32; 40_000];
store.write_window("ch0_gain", /*start_sample=*/ 0, total_samples, &processed)?;

// Read a window back (returns zeros for samples not yet written).
let readback = store.read_window("ch0_gain", 0, 40_000, total_samples)?;

// Flush all channels to disk.
store.flush_all()?;

// List channels that have persisted .mmap files on disk.
let names = store.persisted_channel_names()?;
```

---

## LOD selection heuristic

`UiService::get_optimal_zarr_lod` selects the coarsest LOD that still provides
at least 2 rendered points per screen pixel:

```
chosen_level = max L  where  (count / 16^L) ≥ (width_px / 2)
```

| count | width_px | selected LOD |
|-------|----------|-------------|
| 40 000 (1 s) | 1920 | 0 (raw) |
| 1 440 000 (36 s) | 1920 | 1 |
| 23 040 000 (576 s) | 1920 | 2 |
| 144 000 000 (1 h) | 1920 | 3 |

At LOD 0 the renderer draws one line per sample. At LOD > 0 it draws a filled
min/max envelope — the signal shape is preserved at any zoom level.
