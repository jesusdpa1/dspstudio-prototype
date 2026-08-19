# Data Access Guide

This page is for developers writing new modules that need to read or write recording data. It covers the three main access patterns with working code examples.

---

## Pattern 1 — UI viewport fetch

Use `UiService` when you need data for rendering. It automatically selects the right LOD level based on the viewport width, so the renderer never loads more samples than the screen can display.

```rust
use dsp_io::config::StorageConfig;
use dsp_io::zarr::StorageManager;
use dsp_io::metadata::DatasetMetadata;
use dsp_io::recording_meta::RecordingMeta;
use dsp_io::virtual_channel::VirtualChannelStore;
use dsp_io::transmission::ui::UiService;
use dsp_io::processing_graph::ChannelId;
use std::path::PathBuf;

let zarr_path = PathBuf::from("recording.zarr");

// Open storage.
let mut config = StorageConfig::default();
config.raw_archive_path = zarr_path.clone();
let manager = StorageManager::new(config)?;

// Load metadata.
let meta = RecordingMeta::load(&zarr_path)?;
let dataset = DatasetMetadata::new_power_of_two(meta.total_samples);

// Optional: also serve virtual channels.
let mut store = VirtualChannelStore::new(&zarr_path)?;
let mut ui = UiService::new(&manager, Some(&mut store));

// Fetch a 1-second window at 1920px width for channels 0, 1, and a virtual channel.
let channels = vec![
    ChannelId::Physical(0),
    ChannelId::Physical(1),
    ChannelId::Virtual("ch0_gain".into()),
];
let view = ui.fetch_view(&dataset, /*start=*/ 0, /*count=*/ 40_000, /*width_px=*/ 1920, &channels)?;

// Interpret the result.
match view.lod_level {
    0 => {
        // Raw samples: view.data has view.points_per_channel floats per channel.
        let ch0 = &view.data[..view.points_per_channel];
    }
    _ => {
        // Peak pairs: view.data has view.points_per_channel * 2 floats per channel.
        // Each pair is (min, max) for a decimated window.
        let ch0_peaks = &view.data[..view.points_per_channel * 2];
        for pair in ch0_peaks.chunks(2) {
            let (lo, hi) = (pair[0], pair[1]);
        }
    }
}
# Ok::<(), anyhow::Error>(())
```

### `ViewResponse` fields

| Field | Type | Meaning |
|-------|------|---------|
| `data` | `Vec<f32>` | Channel-major buffer (see layout below) |
| `lod_level` | `u8` | 0 = raw, N = 16^N× decimation |
| `decimation_ratio` | `u64` | How many raw samples each point represents |
| `points_per_channel` | `usize` | Number of data points (not floats) per channel |
| `channels_returned` | `Vec<ChannelId>` | Order of channels in `data` |

**Data layout:**

```text
lod_level == 0:
  data = [ch0_s0, ch0_s1, …, ch0_sN,  ch1_s0, …,  chK_s0, …]
          ─── points_per_channel floats per channel ───

lod_level > 0:
  data = [ch0_min0, ch0_max0, ch0_min1, ch0_max1, …,  ch1_min0, ch1_max0, …]
          ─── points_per_channel * 2 floats per channel ───
```

---

## Pattern 2 — DSP kernel batch fetch

Use `ProcessingService` inside a DSP kernel or custom processor. It adds a **surplus** (overlap) on each side of the requested window to prevent filter edge effects.

```rust
use dsp_io::config::StorageConfig;
use dsp_io::zarr::StorageManager;
use dsp_io::virtual_channel::VirtualChannelStore;
use dsp_io::transmission::processing::ProcessingService;
use dsp_io::processing_graph::ChannelId;
use std::path::PathBuf;

let zarr_path = PathBuf::from("recording.zarr");
let mut config = StorageConfig::default();
config.raw_archive_path = zarr_path.clone();
let manager = StorageManager::new(config)?;

// Optionally open virtual channels for input too.
let mut store = VirtualChannelStore::new(&zarr_path)?;
let mut service = ProcessingService::new(&manager, Some(&mut store));

let batch_size  = 40_000u64; // 1 s at 40 kHz
let surplus     = 1_024u64;  // filter transient margin
let total       = 144_000_000u64;

// Fetch returns:  channels * (batch_size + 2 * surplus) floats  (channel-major)
let data = service.fetch_package_with_surplus(
    /*start=*/  0i64,
    /*count=*/  batch_size,
    /*surplus=*/ surplus,
    /*total_samples=*/ total,
    &[ChannelId::Physical(0), ChannelId::Virtual("ch0_gain".into())],
)?;

let window_len = (batch_size + 2 * surplus) as usize;

// Slice out the core region, stripping surplus.
let ch0_core = &data[surplus as usize..surplus as usize + batch_size as usize];
// ch1 starts at window_len.
let ch1_core = &data[window_len + surplus as usize..window_len + surplus as usize + batch_size as usize];
# Ok::<(), anyhow::Error>(())
```

### Choosing the surplus

```rust
// Safe Nyquist-derived surplus for a given cutoff frequency.
let surplus = service.calculate_nyquist_surplus(/*sample_rate=*/ 40_000, /*cutoff_hz=*/ 100.0);
```

The formula is `ceil((sample_rate / cutoff_hz) * 2.0)`. For a 100 Hz low-pass on a 40 kHz stream this gives 800 samples. Using `1 024` is a conservative default.

---

## Pattern 3 — Reading virtual channels directly

When you don't need surplus windowing, read from `VirtualChannelStore` directly:

```rust
use dsp_io::virtual_channel::VirtualChannelStore;
use std::path::PathBuf;

let zarr_path = PathBuf::from("recording.zarr");
let mut store = VirtualChannelStore::new(&zarr_path)?;

// Returns a Vec<f32> of length `count`, zero-padded for unwritten regions.
let samples = store.read_window(
    /*name=*/          "ch0_gain",
    /*start_sample=*/  80_000u64,
    /*count=*/         40_000u64,
    /*total_samples=*/ 144_000_000u64,
)?;
# Ok::<(), anyhow::Error>(())
```

---

## Pattern 4 — Writing a new virtual channel

```rust
use dsp_io::virtual_channel::VirtualChannelStore;
use std::path::PathBuf;

let zarr_path = PathBuf::from("recording.zarr");
let mut store = VirtualChannelStore::new(&zarr_path)?;
let total_samples = 144_000_000u64;

// Must be called before the first write.
store.open_or_create("my_output", total_samples)?;

// Write in whatever batch size you like.
let batch = vec![0.5f32; 40_000];
store.write_window("my_output", /*start=*/ 0, total_samples, &batch)?;

// Sync to disk when done.
store.flush_all()?;
# Ok::<(), anyhow::Error>(())
```

---

## Pattern 5 — Raw Zarr reads (no overhead)

For maximum control, bypass the service layer and call `StorageManager` directly:

```rust
use dsp_io::config::StorageConfig;
use dsp_io::zarr::StorageManager;
use std::path::PathBuf;

let mut config = StorageConfig::default();
config.raw_archive_path = PathBuf::from("recording.zarr");
let manager = StorageManager::new(config)?;

// Read raw samples for channels 0 and 3.
// Returns channel-major Vec<f32>.
let raw = manager.read_raw_window_masked(
    /*start_sample=*/ 40_000u64,
    /*count=*/        1_000u64,
    /*channels=*/     &[0u16, 3u16],
)?;

// Read LOD 1 peaks for channel 0.
// Returns interleaved (min, max) pairs.
let peaks = manager.read_peak_window_masked(
    /*lod_level=*/ 1u8,
    /*lod_start=*/ 100u64,   // index in LOD space, not sample space
    /*lod_count=*/ 50u64,
    /*channels=*/  &[0u16],
)?;
# Ok::<(), anyhow::Error>(())
```

> **Note on LOD indexing:** `lod_start` and `lod_count` are in the decimated
> coordinate space. Multiply by `16^lod_level` to convert to sample indices.

---

## `ChannelId` — addressing channels

All service calls accept `&[ChannelId]` rather than raw integers. This allows mixing physical and virtual channels in a single request.

```rust
use dsp_io::processing_graph::ChannelId;

let ch0_physical = ChannelId::Physical(0);
let ch_processed = ChannelId::Virtual("ch0_gain".into());
```

Over gRPC, `ChannelId` maps to the proto `oneof`:

```protobuf
message ChannelId {
    oneof kind {
        uint32 physical = 1;
        string virtual  = 2;
    }
}
```

```rust
use dsp_io::proto::transmission::{channel_id, ChannelId as ProtoChannelId};

let proto_physical = ProtoChannelId { kind: Some(channel_id::Kind::Physical(0)) };
let proto_virtual  = ProtoChannelId { kind: Some(channel_id::Kind::Virtual("ch0_gain".into())) };
```

---

## Thread safety

| Type | Thread safety |
|------|--------------|
| `StorageManager` | `Send + Sync` — share via `Arc` |
| `VirtualChannelStore` | `Send` — one writer at a time; clone the store handle for concurrent readers |
| `UiService` | `Send` — `&mut self` on `fetch_view`; one active fetch per service |
| `ProcessingService` | `Send` — `&mut self`; one active fetch per service |
| `GraphProcessor` | `Send + Sync` — stateless spec; call `run_full_recording` on any thread |
