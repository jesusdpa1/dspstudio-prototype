# Processing Graph

The processing graph engine lets you express DSP operations as a serializable directed acyclic graph (DAG), execute it over a full recording in one call, and persist the results as named virtual channels (waveforms) or event tracks (sparse spikes).

---

## Key types

| Type | Role |
|------|------|
| `ProcessingGraphSpec` | Serializable spec — nodes + wires + sample rate |
| `SpecNode` | One node in the DAG |
| `SpecWire` | Directed edge from one node's output pin to another's input pin |
| `GraphProcessor` | Executes a spec over a full recording |
| `ChannelId` | Addresses physical or virtual channels uniformly |
| `SignalValue` | Typed wire payload (Waveform or Events) |

---

## Node types

### Source nodes (no inputs)

| Variant | Output pins | Description |
|---------|-------------|-------------|
| `SpecNode::Channel { id }` | 1 | Emits one channel's samples |
| `SpecNode::MultiChannel { ids }` | N | Emits N channels; output pin K → `ids[K]` |
| `SpecNode::Float { value }` | 1 | Constant scalar repeated for every sample |
| `SpecNode::Bool { value }` | 1 | Constant `0.0` (false) or `1.0` (true) |

### Transform nodes (Waveform → Waveform)

| Variant | Description |
|---------|-------------|
| `SpecNode::Arithmetic { op }` | Element-wise `Add, Subtract, Multiply, Divide` |
| `SpecNode::SosFilter { .. }` | IIR filter via Second-Order Sections cascade |
| `SpecNode::Butterworth { .. }` | Designed Butterworth filter (LP, HP, BP, BS) |
| `SpecNode::MovingAverage { .. }` | Uniform boxcar smoothing |
| `SpecNode::MedianFilter { .. }` | Non-linear spike/noise rejection |

### Detection nodes (Waveform → Events)

| Variant | Description |
|---------|-------------|
| `SpecNode::SpikeDetector { .. }` | Threshold-crossing event generator |

### Sink nodes (write outputs)

| Variant | Input pins | Description |
|---------|------------|-------------|
| `SpecNode::Output { source_id }` | 1 | Writes to `ch{N:02}_drv` (destructive) |
| `SpecNode::Fork { source_id, name }` | 1 | Writes to an explicitly named channel |
| `SpecNode::EventsOutput { track_name, .. }` | 1 | Accumulates events and writes to Zarr archive |

---

## Executing a graph

`GraphProcessor::run_full_recording` drives the sliding-window loop. It implements **Surplus Windowing** to prevent digital filtering artifacts (edge effects) at batch boundaries.

```rust
let processor = GraphProcessor::new(spec);
let virtual_channels = processor.run_full_recording(
    &manager,
    total_samples,
    start_sample,
    count,
    /*batch_size=*/ 40_000,
    /*surplus=*/    1_024,   // Margin for filter transients
    &mut store,
    Some(zarr_path),         // Appends to processing.json history
    |progress| println!("{:.0}%", progress * 100.0),
)?;
```

### Surplus Model

For each batch, the processor fetches `batch_size + 2 * surplus` samples. Filters are evaluated over this full window, but only the central `batch_size` region is written to the output file. This ensures that every sample in the output was computed with sufficient context (look-ahead and look-behind).

---

## JSON representation

Example JSON for a low-pass filtered output:

```json
{
  "nodes": [
    { "Channel": { "id": { "Physical": 0 } } },
    { "Butterworth": { "order": 4, "response": { "LowPass": { "cutoff": 1000.0 } }, "filtfilt": true } },
    { "Output": { "source_id": { "Physical": 0 } } }
  ],
  "wires": [
    { "from_node": 0, "from_output": 0, "to_node": 1, "to_input": 0 },
    { "from_node": 1, "from_output": 0, "to_node": 2, "to_input": 0 }
  ],
  "sample_rate": 40000.0
}
```
