# gRPC API

DSP Studio exposes a tonic gRPC service (`TransmissionService`) that provides remote access to every storage and processing operation. The server runs in `dsp-cli`; clients can be written in any language with gRPC support.

---

## Proto definition

```protobuf
syntax = "proto3";
package dsp_studio.transmission;

service TransmissionService {
    rpc OpenFile          (OpenFileRequest)   returns (RecordingMeta);
    rpc SaveMetadata      (SaveMetadataRequest) returns (Empty);
    rpc FetchView         (ViewRequest)       returns (ViewResponse);
    rpc FetchRaw          (ViewRequest)       returns (ViewResponse);
    rpc FetchEvents       (FetchEventsRequest) returns (FetchEventsResponse);
    rpc FetchClusterData  (FetchClusterDataRequest) returns (FetchClusterDataResponse);
    rpc RunProcessingGraph(RunGraphRequest)   returns (stream ProcessingResponse);
}
```

The `.proto` file lives at `dsp-io/proto/transmission.proto`. Generated Rust stubs are available at `dsp_io::proto::transmission`.

---

## Starting the server

```bash
# Headless (dsp-cli)
cargo run -p dsp-cli -- --serve [::1]:50051

# In-process (tests, labs)
use dsp_io::transmission::grpc_server::start_grpc_server;
let addr = "[::1]:50051".parse()?;
tokio::spawn(async move { start_grpc_server(addr).await.unwrap() });
```

---

## Connecting a Rust client

```rust
use dsp_io::proto::transmission::transmission_service_client::TransmissionServiceClient;

let mut client = TransmissionServiceClient::connect("http://[::1]:50051").await?;
```

---

## Endpoints

### `OpenFile`

Loads the `RecordingMeta` sidecar for a recording.

**Request**
```protobuf
message OpenFileRequest { string path = 1; }
```
`path` is the path to the `.zarr` directory.

**Response:** `RecordingMeta` — see [Storage Foundation](./io_foundation.md) for the JSON equivalent.

```rust
let meta = client.open_file(OpenFileRequest {
    path: "data/lab/neural_recording.zarr".into(),
}).await?.into_inner();

println!("{}ch @ {}Hz, {} samples", meta.n_channels, meta.sample_rate, meta.total_samples);
println!("LOD levels: {:?}", meta.lod_levels_available);
```

---

### `SaveMetadata`

Persists an updated `RecordingMeta` sidecar. Use this after renaming channels or adding virtual channel entries.

```rust
let mut meta = /* from OpenFile */;
meta.description = "Updated description".into();

client.save_metadata(SaveMetadataRequest {
    zarr_path: "data/lab/neural_recording.zarr".into(),
    meta: Some(meta),
}).await?;
```

---

### `FetchView`

Returns a viewport of data. Automatically selects the right LOD level.

**Request**
```protobuf
message ViewRequest {
    string zarr_path  = 1;
    uint64 start_sample = 2;
    uint64 count        = 3;
    uint32 width_px     = 4;
    repeated ChannelId channels = 5;
}
```

**Response**
```protobuf
message ViewResponse {
    repeated float data            = 1;  // see layout below
    uint32         lod_level       = 2;
    uint64         decimation_ratio = 3;
    uint64         points_per_channel = 4;
    repeated ChannelId channels_returned = 5;
}
```

**Data layout:**
```text
lod_level == 0 (raw):
  data[i * points_per_channel .. (i+1) * points_per_channel]  →  channel i raw samples

lod_level > 0 (peaks):
  data[i * points_per_channel*2 .. (i+1) * points_per_channel*2]  →  channel i min/max pairs
  Each pair: [min, max]
```

**Rust example:**
```rust
use dsp_io::proto::transmission::{channel_id, ChannelId, ViewRequest};

fn physical(idx: u32) -> ChannelId {
    ChannelId { kind: Some(channel_id::Kind::Physical(idx)) }
}
fn virtual_ch(name: &str) -> ChannelId {
    ChannelId { kind: Some(channel_id::Kind::Virtual(name.into())) }
}

let resp = client.fetch_view(ViewRequest {
    zarr_path: "recording.zarr".into(),
    start_sample: 0,
    count: 40_000,       // 1 second
    width_px: 1920,
    channels: vec![physical(0), physical(1), virtual_ch("ch0_gain")],
}).await?.into_inner();

let pts = resp.points_per_channel as usize;
if resp.lod_level == 0 {
    let ch0_samples = &resp.data[..pts];
} else {
    let ch0_peaks: Vec<(f32, f32)> = resp.data[..pts*2]
        .chunks(2).map(|p| (p[0], p[1])).collect();
}
```

---

### `FetchRaw`

Identical to `FetchView` — same request, same response, same LOD logic. Exists as a semantic alias for callers that only want non-decimated data (they should set `width_px` equal to `count` to force LOD 0).

---

### `FetchEvents`

Returns event markers (e.g., spike times) for a specific track and channel within a time window.

**Request**
```protobuf
message FetchEventsRequest {
    string zarr_path    = 1;
    string track_name   = 2;
    uint32 channel_idx  = 3;
    uint64 start_sample = 4;
    uint64 end_sample   = 5; // 0 = until end
}
```

**Response**
```protobuf
message FetchEventsResponse {
    repeated Event events = 1;
}
message Event {
    uint64 sample_offset = 1;
    uint32 label_id      = 2;
}
```

---

### `FetchClusterData`

Returns subsampled waveforms and PCA features for a specific cluster (`label_id`) within a track. This is used by the PCA and Waveform views. It automatically chooses between reading pre-processed artifacts (if they exist) or performing on-the-fly extraction from raw data.

**Request**
```protobuf
message FetchClusterDataRequest {
    string zarr_path      = 1;
    string track_name     = 2;
    uint32 label_id       = 3;
    uint32 max_waveforms  = 4; // Subsampling limit
    uint32 snippet_before = 5; // Default: 20
    uint32 snippet_after  = 6; // Default: 28
}
```

**Response**
```protobuf
message FetchClusterDataResponse {
    uint32 label_id       = 1;
    repeated float pca_pc1 = 2;
    repeated float pca_pc2 = 3;
    repeated float waveforms = 4; // Flat: [n_spikes * snippet_len]
    repeated float mean_waveform = 5;
    repeated float std_waveform  = 6;
    uint32 snippet_len    = 7;
    uint32 n_spikes       = 8;
}
```

---

### `RunProcessingGraph`

Executes a `ProcessingGraphSpec` over a recording and streams progress.

**Request**
```protobuf
message RunGraphRequest {
    string zarr_path       = 1;
    string graph_spec_json = 2;  // JSON-serialized ProcessingGraphSpec
    uint64 total_samples   = 3;
    uint64 batch_size      = 4;
    uint64 surplus         = 5;
}
```

**Response stream:**
```protobuf
message ProcessingResponse {
    oneof event {
        ProcessingProgress progress = 1;
        ProcessingComplete complete = 2;
    }
}

message ProcessingProgress {
    float  progress = 1;  // 0.0 → 1.0
    string message  = 2;
}

message ProcessingComplete {
    repeated VirtualChannelMeta virtual_channels = 1;
}
```

The stream emits `Progress` events after each batch and a single `Complete` event when done. Clients should consume the stream until it closes.

**Rust example:**
```rust
use dsp_io::processing_graph::{ChannelId, ProcessingGraphSpec, SpecNode, SpecWire, ArithOpSpec};
use dsp_io::proto::transmission::{RunGraphRequest, processing_response};
use tokio_stream::StreamExt;

let spec = ProcessingGraphSpec {
    nodes: vec![
        SpecNode::Channel { id: ChannelId::Physical(0) },
        SpecNode::Float   { value: 2.0 },
        SpecNode::Arithmetic { op: ArithOpSpec::Multiply },
        SpecNode::Output  { name: "ch0_gain".into(), source_id: ChannelId::Physical(0) },
    ],
    wires: vec![
        SpecWire { from_node: 0, from_output: 0, to_node: 2, to_input: 0 },
        SpecWire { from_node: 1, from_output: 0, to_node: 2, to_input: 1 },
        SpecWire { from_node: 2, from_output: 0, to_node: 3, to_input: 0 },
    ],
    sample_rate: 40_000.0,
};

let mut stream = client.run_processing_graph(RunGraphRequest {
    zarr_path: "recording.zarr".into(),
    graph_spec_json: serde_json::to_string(&spec)?,
    total_samples: 144_000_000,
    batch_size: 40_000,
    surplus: 1_024,
}).await?.into_inner();

while let Some(event) = stream.next().await {
    match event?.event {
        Some(processing_response::Event::Progress(p)) => {
            println!("{:.1}% — {}", p.progress * 100.0, p.message);
        }
        Some(processing_response::Event::Complete(c)) => {
            println!("Done. Created {} virtual channel(s).", c.virtual_channels.len());
            for vc in &c.virtual_channels {
                println!("  {} (from ch{})", vc.name, vc.source_channel_idx);
            }
        }
        None => {}
    }
}
```

---

## Proto message reference

### `ChannelId`

```protobuf
message ChannelId {
    oneof kind {
        uint32 physical = 1;   // index into the Zarr /raw array rows
        string virtual  = 2;   // name of a .mmap file in tmp/
    }
}
```

### `RecordingMeta`

```protobuf
message RecordingMeta {
    string session_id     = 1;
    string recording_name = 2;
    string recording_type = 3;
    string description    = 4;
    float  sample_rate    = 5;
    uint32 n_channels     = 6;
    uint64 total_samples  = 7;
    repeated string  channel_names        = 8;
    repeated uint32  lod_levels_available = 9;
    string created_at = 10;
    repeated VirtualChannelMeta virtual_channels = 11;
}

message VirtualChannelMeta {
    string name                 = 1;
    uint32 source_channel_idx   = 2;
    string created_at           = 3;
}
```

---

## Using from Python (gRPC stub generation)

Generate Python stubs from the `.proto` file:

```bash
python -m grpc_tools.protoc \
  -I dsp-io/proto \
  --python_out=. \
  --grpc_python_out=. \
  dsp-io/proto/transmission.proto
```

Then connect:

```python
import grpc
import transmission_pb2 as pb
import transmission_pb2_grpc as pbg

channel = grpc.insecure_channel("[::1]:50051")
stub = pbg.TransmissionServiceStub(channel)

meta = stub.OpenFile(pb.OpenFileRequest(path="recording.zarr"))
print(f"{meta.n_channels}ch @ {meta.sample_rate} Hz")

view = stub.FetchView(pb.ViewRequest(
    zarr_path="recording.zarr",
    start_sample=0,
    count=40_000,
    width_px=1920,
    channels=[pb.ChannelId(physical=0)],
))
samples = list(view.data)
```
