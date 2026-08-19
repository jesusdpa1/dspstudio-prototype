# Session Orchestration: Virtual Channels

A "session" in DSP Studio is the combination of a physical recording (Zarr archive) and any processed derivatives (virtual channels in the `tmp/` mmap store). The UI can display both sources simultaneously in a single composite view.

---

## Channel types

| Type | `ChannelId` variant | Storage | Mutable |
|------|-------------------|---------|---------|
| Physical | `ChannelId::Physical(u16)` | Zarr `/raw` array | No — immutable ground truth |
| Virtual | `ChannelId::Virtual(String)` | `tmp/<name>.mmap` | Yes — written by graph execution |

---

## Data flow

```text
Zarr archive (physical)
      │
      ▼
StorageManager::read_raw_window_masked()
      │                                         ProcessingGraphSpec
      │                                               │
      ▼                                               ▼
ProcessingService::fetch_package_with_surplus()   GraphProcessor::run_full_recording()
      │                                               │
      │  (surplus-windowed batch)                     │  (batch loop writes output)
      │                                               ▼
      └──────────────────────────────────► VirtualChannelStore (tmp/*.mmap)
                                                      │
                                                      ▼
                                            UiService::fetch_view()
                                                      │
                                           (mixed physical + virtual)
                                                      │
                                                      ▼
                                                ViewResponse → renderer
```

---

## Example: composite viewport

```rust
use dsp_io::config::StorageConfig;
use dsp_io::zarr::StorageManager;
use dsp_io::metadata::DatasetMetadata;
use dsp_io::virtual_channel::VirtualChannelStore;
use dsp_io::transmission::ui::UiService;
use dsp_io::processing_graph::ChannelId;
use std::path::PathBuf;

let zarr_path = PathBuf::from("recording.zarr");
let mut config = StorageConfig::default();
config.raw_archive_path = zarr_path.clone();
let manager = StorageManager::new(config)?;
let meta = dsp_io::recording_meta::RecordingMeta::load(&zarr_path)?;
let dataset = DatasetMetadata::new_power_of_two(meta.total_samples);

// Open the virtual channel store (creates tmp/ if absent).
let mut store = VirtualChannelStore::new(&zarr_path)?;

let mut ui = UiService::new(&manager, Some(&mut store));

// Request 3 channels: CH0 and CH1 from the Zarr archive, plus a processed
// virtual channel "ch0_gain" from the mmap store.
let view = ui.fetch_view(
    &dataset,
    0,
    40_000,
    1920,
    &[
        ChannelId::Physical(0),
        ChannelId::Physical(1),
        ChannelId::Virtual("ch0_gain".into()),
    ],
)?;
# Ok::<(), anyhow::Error>(())
```

`UiService` selects the same LOD level for all channels and decimates virtual channels on-the-fly with `dsp_core::util::resampling::generate_peaks_parallel`, so the envelope rendering is consistent across source types.

---

## Lifecycle of a virtual channel

```
1. Graph spec designed in node editor (dsp-app)
2. Spec serialized → sent to GraphProcessor (local) or gRPC RunProcessingGraph (remote)
3. GraphProcessor writes .mmap files in tmp/
4. VirtualChannelMeta returned → merged into RecordingMeta → saved to .json sidecar
5. UI reloads virtual_channels from meta → displays new channels in sidebar
6. User opens recording next session → VirtualChannelStore reopens .mmap files
```

---

## Persisting virtual channels after processing

After `run_full_recording` returns, merge the new metadata into the sidecar so it survives restarts:

```rust
use dsp_io::recording_meta::RecordingMeta;
use std::path::PathBuf;

let zarr_path = PathBuf::from("recording.zarr");
let mut meta = RecordingMeta::load(&zarr_path)?;

// virtual_channels comes from GraphProcessor::run_full_recording
for vc in virtual_channels {
    if !meta.virtual_channels.iter().any(|e| e.name == vc.name) {
        meta.virtual_channels.push(vc);
    }
}

meta.save(&zarr_path)?;
# Ok::<(), anyhow::Error>(())
```

---

## Data persistence layout

```text
dsp-studio/
├── dsp-core/          # Logic (zero I/O)
├── dsp-io/            # Storage & transmission
└── data/              # Runtime artifacts
    └── lab/
        ├── neural_recording.zarr/   # Immutable raw archive
        ├── neural_recording.json    # RecordingMeta sidecar
        └── tmp/
            ├── ch0_gain.mmap        # Virtual channel output
            └── ch1_drv.mmap
```

The `data/` directory is `.gitignore`d. Only source code is version controlled.
