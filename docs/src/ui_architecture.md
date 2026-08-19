# UI Architecture

`dsp-app` is an eframe/egui desktop application. This page describes how the render loop, IO bridge, state machine, and view components are organized.

---

## Design principles

1. **No blocking IO on the render thread.** All Zarr reads and processing graph execution run on a tokio blocking thread pool. The render thread only calls `try_recv()`.
2. **Fetch only on viewport change.** A new `FetchView` request is dispatched only when the navigator sliders actually move — no continuous IO loop.
3. **Stale-while-revalidate.** The last cached `ViewResponse` is always rendered at its original position while a new fetch is in flight — no blank flash between updates.
4. **Separation of concerns.** `WaveformView` is a stateless renderer. `TimeseriesLayout` owns the cache and fetch policy. `App::logic()` owns the bridge and state machine.

---

## eframe split: `logic()` and `ui()`

| Method | Called when | Rules |
|--------|-------------|-------|
| `logic()` | every frame, before painting | may not call UI widgets |
| `ui()` | every frame, during painting | may not block |

`logic()` polls the IO bridge and calls `ctx.request_repaint()` whenever new data arrives. This wakes eframe even if it is throttling the frame rate.

---

## AppState machine

```text
Idle
 │  File > Open…
 ▼
CheckingFile          (spinner shown)
 │  peak pyramid present?
 ├─[no]──► PeakBuildDialog ──[Cancel]──► Idle
 │              │ [Generate]
 │              ▼
 │          BuildingPeaks   (progress bar)
 │              │ done
 │              └────────────────────────────┐
 │  peak pyramid OK?                         │
 └────────────────────────────────────────► Active(SessionState)
                                              │ File > Close
                                              ▼
                                             Idle
```

### `SessionState`

```rust
pub struct SessionState {
    pub zarr_path: PathBuf,
    pub meta: RecordingMeta,
    pub dataset: DatasetMetadata,
    pub display: Vec<ChannelDisplay>,         // physical channels
    pub virtual_display: Vec<ChannelDisplay>, // virtual channels
    pub view_start: u64,
    pub view_width: u64,
}
```

`virtual_display` is kept in sync with `meta.virtual_channels` via `merge_virtual_channels()`, which is called after graph execution completes.

---

## IoBridge

`IoBridge` owns a `tokio::runtime::Runtime` and two `std::sync::mpsc` channels.

```
Render thread                          Tokio blocking pool
──────────────                         ─────────────────────
bridge.send(IoRequest)  ─── mpsc ───►  handle_request()
                                          │  StorageManager::…
bridge.rx.try_recv()    ◄── mpsc ───   tx.send(IoResponse)
```

`StorageManager` is constructed fresh for each request — it only wraps an `Arc<FilesystemStore>`, so construction is cheap.

### Backend modes

`IoBridge` supports two backends selected at startup:

| Mode | Description |
|------|-------------|
| `Local` | Direct library calls to `dsp-io` — no network overhead |
| `Remote(url)` | tonic gRPC client — connects to a running `dsp-cli` instance |

### Request / Response summary

| Request | Response(s) |
|---------|-------------|
| `OpenFile(path)` | `FileOpened(meta, dataset)` or `PeakPyramidMissing` or `Error` |
| `BuildPeakPyramid(path)` | `PeakBuildProgress(f32)` × N, then `PeakBuildComplete` or `Error` |
| `FetchView { … }` | `ViewReady(ViewResponse)` or `Error` |
| `SaveRecordingMeta { … }` | `MetaSaved` or `Error` |
| `RunProcessingGraph { … }` | `GraphProgress(f32)` × N, then `GraphComplete(Vec<VirtualChannelMeta>)` or `Error` |

---

## Timeseries view components

```
TimeseriesLayout  (owns ViewResponse cache + fetch state)
│
├── WaveformView  (stateless renderer — takes Option<&ViewResponse>)
│     └── one egui Plot per visible channel (stacked vertically)
│
├── Recording Info panel  (channel name editor, metadata display)
│
└── Navigation sliders (Window size, Position)
```

### WaveformView rendering

`WaveformView` renders from `Option<&ViewResponse>` passed by `TimeseriesLayout`. If `None`, plots are drawn empty. Stale data (fetch in flight) renders at its original position — no layout jitter.

- `allow_zoom(false)` and `allow_drag(false)` — sliders are the sole navigation authority.
- LOD 0 → single `Line`. LOD > 0 → min/max `fill()` envelope.
- Y bounds are computed per-channel from the data with ±10% padding.

### TimeseriesLayout fetch policy

```rust
if view_changed || (cache.is_none() && !fetch_pending) {
    try_fetch(session, bridge);
}

fn try_fetch() {
    if fetch_pending { return; }
    if view_start == last_fetched_start && view_width == last_fetched_width { return; }
    bridge.send(FetchView { … });
    fetch_pending = true;
}
```

At most one request is in flight at any time. `fetch_pending` is cleared only by `on_view_ready()` (success) or `on_fetch_error()` (failure).

---

## Node graph editor

`NodeGraphLayout` wraps an [egui-snarl](https://github.com/zakarumych/egui-snarl) canvas. Users wire nodes together visually; the layout serializes the result to a `ProcessingGraphSpec` and dispatches `IoRequest::RunProcessingGraph`.

The graph editor is egui-only — it has no direct dependency on `dsp-io` types. The serializable `ProcessingGraphSpec` is the boundary between the two.
