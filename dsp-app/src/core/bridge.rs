use dsp_io::config::StorageConfig;
use dsp_io::metadata::DatasetMetadata;
use dsp_io::processing_graph::{GraphProcessor, ProcessingGraphSpec, ChannelId};
use dsp_io::recording_meta::{RecordingMeta, VirtualChannelMeta};
use dsp_io::transmission::ui::{ViewResponse, UiService, ClusterData};
use dsp_io::virtual_channel::VirtualChannelStore;
use dsp_io::zarr::StorageManager;
use dsp_io::proto::transmission::transmission_service_client::TransmissionServiceClient;
use dsp_io::proto::transmission::{
    OpenFileRequest, ViewRequest, RunGraphRequest, FetchEventsRequest,
    FetchClusterDataRequest, processing_response
};
use dsp_io::transmission::grpc_server::{from_proto_meta, to_proto_channel_id, from_proto_channel_id};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::UnboundedSender;
use tokio_stream::StreamExt;
use tonic::codec::CompressionEncoding;

/// Matches the server's per-message ceiling in `dsp-io` (64 MiB).
const MAX_MESSAGE_BYTES: usize = 64 * 1024 * 1024;

// ── Request / Response types ─────────────────────────────────────────────────

pub enum IoRequest {
    OpenFile(PathBuf),
    BuildPeakPyramid(PathBuf),
    FetchView {
        dataset_id: String,
        zarr_path: PathBuf,
        start_sample: u64,
        count: u64,
        width_px: u32,
        channels: Vec<ChannelId>,
        total_samples: u64,
    },
    /// Coarsest whole-recording view, fetched once and kept resident as a
    /// background fallback so scroll/zoom never blanks.
    FetchOverview {
        dataset_id: String,
        zarr_path: PathBuf,
        total_samples: u64,
        width_px: u32,
        channels: Vec<ChannelId>,
    },
    SaveRecordingMeta {
        zarr_path: PathBuf,
        meta: RecordingMeta,
    },
    RunProcessingGraph {
        dataset_id: String,
        zarr_path: PathBuf,
        graph_spec: ProcessingGraphSpec,
        total_samples: u64,
        start_sample: u64,
        count: u64,
        batch_size: u64,
        surplus: u64,
    },
    FetchEvents {
        dataset_id: String,
        zarr_path: PathBuf,
        track_name: String,
        channel_idx: u32,
        start_sample: u64,
        end_sample: u64,
    },
    FetchClusterData {
        dataset_id: String,
        zarr_path: PathBuf,
        track_name: String,
        label_id: u32,
        max_waveforms: u32,
        snippet_before: u32,
        snippet_after: u32,
    },
}

pub enum IoResponse {
    FileOpened {
        zarr_path: PathBuf,
        meta: RecordingMeta,
        _dataset: DatasetMetadata,
    },
    PeakPyramidMissing { zarr_path: PathBuf },
    PeakBuildProgress(f32),
    PeakBuildComplete { zarr_path: PathBuf },
    /// `start_sample` echoes the request so the UI can discard stale responses.
    ViewReady {
        dataset_id: String,
        start_sample: u64,
        response: ViewResponse,
    },
    /// Resident coarse overview is ready for this recording.
    OverviewReady {
        dataset_id: String,
        response: ViewResponse,
    },
    MetaSaved,
    EventsReady {
        dataset_id: String,
        track_name: String,
        channel_idx: u32,
        events: Vec<dsp_core::signal::Event>,
    },
    ClusterDataReady {
        dataset_id: String,
        track_name: String,
        data: ClusterData,
    },
    ProcessingProgress(f32),
    ProcessingComplete {
        dataset_id: String,
        virtual_channels: Vec<VirtualChannelMeta>
    },
    Error(String),
}

// ── Bridge ───────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub enum BackendConfig {
    Local,
    Remote(String),
}

pub struct IoBridge {
    /// Unbounded so the UI thread never blocks on send.
    tx: UnboundedSender<IoRequest>,
    pub rx: Receiver<IoResponse>,
    _rt: Runtime,
}

impl IoBridge {
    pub fn new(config: BackendConfig) -> Self {
        let (req_tx, mut req_rx) = tokio::sync::mpsc::unbounded_channel::<IoRequest>();
        let (resp_tx, resp_rx) = mpsc::channel::<IoResponse>();
        let rt = Runtime::new().expect("failed to create tokio runtime");

        rt.spawn(async move {
            match config {
                BackendConfig::Local => {
                    // `recv().await` yields instead of blocking a Tokio worker thread.
                    while let Some(req) = req_rx.recv().await {
                        let resp_tx = resp_tx.clone();
                        tokio::task::spawn_blocking(move || {
                            handle_local_request(req, resp_tx);
                        });
                    }
                }
                BackendConfig::Remote(url) => {
                    let client = match TransmissionServiceClient::connect(url.clone()).await {
                        Ok(c) => c
                            .send_compressed(CompressionEncoding::Zstd)
                            .accept_compressed(CompressionEncoding::Zstd)
                            .max_decoding_message_size(MAX_MESSAGE_BYTES)
                            .max_encoding_message_size(MAX_MESSAGE_BYTES),
                        Err(e) => {
                            let _ = resp_tx.send(IoResponse::Error(
                                format!("gRPC connection failed: {}", e)
                            ));
                            return;
                        }
                    };
                    while let Some(req) = req_rx.recv().await {
                        let resp_tx = resp_tx.clone();
                        let mut client = client.clone();
                        tokio::spawn(async move {
                            handle_remote_request(req, &mut client, resp_tx).await;
                        });
                    }
                }
            }
        });
        Self { tx: req_tx, rx: resp_rx, _rt: rt }
    }

    pub fn send(&self, req: IoRequest) {
        let _ = self.tx.send(req);
    }
}

// ── Local Handler ────────────────────────────────────────────────────────────

fn handle_local_request(req: IoRequest, tx: Sender<IoResponse>) {
    match req {
        IoRequest::OpenFile(zarr_path) => { open_local_file(zarr_path, tx); }
        IoRequest::BuildPeakPyramid(zarr_path) => { build_local_peaks(zarr_path, tx); }
        IoRequest::FetchView { dataset_id, zarr_path, start_sample, count, width_px, channels, total_samples } => {
            fetch_local_view(dataset_id, zarr_path, start_sample, count, width_px, channels, total_samples, tx);
        }
        IoRequest::FetchOverview { dataset_id, zarr_path, total_samples, width_px, channels } => {
            fetch_local_overview(dataset_id, zarr_path, total_samples, width_px, channels, tx);
        }
        IoRequest::SaveRecordingMeta { zarr_path, meta } => {
            match meta.save(&zarr_path) {
                Ok(_) => {
                    dsp_io::dataset_cache::invalidate(&zarr_path);
                    let _ = tx.send(IoResponse::MetaSaved);
                }
                Err(e) => { let _ = tx.send(IoResponse::Error(e.to_string())); }
            }
        }
        IoRequest::RunProcessingGraph { dataset_id, zarr_path, graph_spec, total_samples, start_sample, count, batch_size, surplus } => {
            run_local_processing_graph(dataset_id, zarr_path, graph_spec, total_samples, start_sample, count, batch_size, surplus, tx);
        }
        IoRequest::FetchEvents { dataset_id, zarr_path, track_name, channel_idx, start_sample, end_sample } => {
            fetch_local_events(dataset_id, zarr_path, track_name, channel_idx, start_sample, end_sample, tx);
        }
        IoRequest::FetchClusterData { dataset_id, zarr_path, track_name, label_id, max_waveforms, snippet_before, snippet_after } => {
            fetch_local_cluster_data(dataset_id, zarr_path, track_name, label_id, max_waveforms, snippet_before, snippet_after, tx);
        }
    }
}

// ── Remote Handler (gRPC) ─────────────────────────────────────────────────────

async fn handle_remote_request(
    req: IoRequest,
    client: &mut TransmissionServiceClient<tonic::transport::Channel>,
    tx: Sender<IoResponse>,
) {
    match req {
        IoRequest::OpenFile(zarr_path) => {
            let req = OpenFileRequest { path: zarr_path.to_string_lossy().to_string() };
            match client.open_file(req).await {
                Ok(resp) => {
                    let proto_meta = resp.into_inner();
                    let meta = from_proto_meta(proto_meta);
                    let dataset = DatasetMetadata::new_power_of_two(meta.total_samples);
                    let _ = tx.send(IoResponse::FileOpened { zarr_path, meta, _dataset: dataset });
                }
                Err(e) => { let _ = tx.send(IoResponse::Error(e.to_string())); }
            }
        }
        IoRequest::FetchView { dataset_id, zarr_path, start_sample, count, width_px, channels, .. } => {
            let req = ViewRequest {
                zarr_path: zarr_path.to_string_lossy().to_string(),
                start_sample,
                count,
                width_px,
                channels: channels.into_iter().map(to_proto_channel_id).collect(),
            };
            match client.fetch_view(req).await {
                Ok(resp) => {
                    let r = resp.into_inner();
                    let lod_level = r.lod_level as u8;
                    let response = ViewResponse {
                        data: r.data,
                        lod_level,
                        decimation_ratio: r.decimation_ratio,
                        points_per_channel: r.points_per_channel as usize,
                        channels_returned: r.channels_returned.into_iter().map(from_proto_channel_id).collect(),
                        actual_start: r.actual_start,
                    };
                    let _ = tx.send(IoResponse::ViewReady { dataset_id, start_sample, response });
                }
                Err(e) => { let _ = tx.send(IoResponse::Error(e.to_string())); }
            }
        }
        IoRequest::FetchOverview { dataset_id, zarr_path, total_samples, width_px, channels } => {
            let req = ViewRequest {
                zarr_path: zarr_path.to_string_lossy().to_string(),
                start_sample: 0,
                count: total_samples,
                width_px,
                channels: channels.into_iter().map(to_proto_channel_id).collect(),
            };
            match client.fetch_view(req).await {
                Ok(resp) => {
                    let r = resp.into_inner();
                    let response = ViewResponse {
                        data: r.data,
                        lod_level: r.lod_level as u8,
                        decimation_ratio: r.decimation_ratio,
                        points_per_channel: r.points_per_channel as usize,
                        channels_returned: r.channels_returned.into_iter().map(from_proto_channel_id).collect(),
                        actual_start: r.actual_start,
                    };
                    let _ = tx.send(IoResponse::OverviewReady { dataset_id, response });
                }
                Err(e) => { let _ = tx.send(IoResponse::Error(e.to_string())); }
            }
        }
        IoRequest::RunProcessingGraph { dataset_id, zarr_path, graph_spec, total_samples, start_sample, count, batch_size, surplus } => {
            let spec_json = serde_json::to_string(&graph_spec).unwrap_or_default();
            let req = RunGraphRequest {
                zarr_path: zarr_path.to_string_lossy().to_string(),
                graph_spec_json: spec_json,
                total_samples,
                batch_size,
                surplus,
                start_sample,
                count,
            };
            match client.run_processing_graph(req).await {
                Ok(resp) => {
                    let mut stream = resp.into_inner();
                    while let Some(item) = stream.next().await {
                        match item {
                            Ok(pr) => {
                                match pr.event {
                                    Some(processing_response::Event::Progress(p)) => {
                                        let _ = tx.send(IoResponse::ProcessingProgress(p.progress));
                                    }
                                    Some(processing_response::Event::Complete(c)) => {
                                        let virtual_channels = c.virtual_channels.into_iter().map(|vc| VirtualChannelMeta {
                                            name: vc.name,
                                            source_channel_idx: vc.source_channel_idx as u16,
                                            created_at: vc.created_at,
                                        }).collect();
                                        let _ = tx.send(IoResponse::ProcessingComplete { dataset_id: dataset_id.clone(), virtual_channels });
                                    }
                                    None => {}
                                }
                            }
                            Err(e) => { let _ = tx.send(IoResponse::Error(e.to_string())); break; }
                        }
                    }
                }
                Err(e) => { let _ = tx.send(IoResponse::Error(e.to_string())); }
            }
        }
        IoRequest::FetchEvents { dataset_id, zarr_path, track_name, channel_idx, start_sample, end_sample } => {
            let req = FetchEventsRequest {
                zarr_path: zarr_path.to_string_lossy().to_string(),
                track_name: track_name.clone(),
                channel_idx,
                start_sample,
                end_sample,
            };
            match client.fetch_events(req).await {
                Ok(resp) => {
                    let r = resp.into_inner();
                    let events = r.events.into_iter().map(|e| dsp_core::signal::Event::new(e.sample_offset, e.label_id)).collect();
                    let _ = tx.send(IoResponse::EventsReady { dataset_id, track_name, channel_idx, events });
                }
                Err(e) => { let _ = tx.send(IoResponse::Error(e.to_string())); }
            }
        }
        IoRequest::FetchClusterData { dataset_id, zarr_path, track_name, label_id, max_waveforms, snippet_before, snippet_after } => {
            let req = FetchClusterDataRequest {
                zarr_path: zarr_path.to_string_lossy().to_string(),
                track_name: track_name.clone(),
                label_id,
                max_waveforms,
                snippet_before,
                snippet_after,
            };
            match client.fetch_cluster_data(req).await {
                Ok(resp) => {
                    let r = resp.into_inner();
                    let data = ClusterData {
                        label_id: r.label_id,
                        pca_pc1: r.pca_pc1,
                        pca_pc2: r.pca_pc2,
                        waveforms: r.waveforms,
                        mean_waveform: r.mean_waveform,
                        std_waveform: r.std_waveform,
                        snippet_len: r.snippet_len as usize,
                        n_spikes: r.n_spikes as usize,
                    };
                    let _ = tx.send(IoResponse::ClusterDataReady { dataset_id, track_name, data });
                }
                Err(e) => { let _ = tx.send(IoResponse::Error(e.to_string())); }
            }
        }
        _ => {
            let _ = tx.send(IoResponse::Error("Remote backend does not support this request".to_string()));
        }
    }
}

// ── Local Implementation Details ──────────────────────────────────────────────

fn open_local_file(zarr_path: PathBuf, tx: Sender<IoResponse>) {
    let mut config = StorageConfig::default();
    config.raw_archive_path = zarr_path.clone();
    let manager = match StorageManager::new(config.clone()) {
        Ok(m) => m,
        Err(e) => { let _ = tx.send(IoResponse::Error(e.to_string())); return; }
    };
    if !manager.peak_pyramid_exists() {
        let _ = tx.send(IoResponse::PeakPyramidMissing { zarr_path });
        return;
    }
    let meta = if RecordingMeta::exists(&zarr_path) {
        match RecordingMeta::load(&zarr_path) {
            Ok(m) => m,
            Err(e) => { let _ = tx.send(IoResponse::Error(e.to_string())); return; }
        }
    } else {
        let mut meta = RecordingMeta::default_for(config.channels, config.chunk_size as u64, config.sample_rate as f32);
        if let Some(stem) = zarr_path.file_stem().and_then(|s| s.to_str()) {
            meta.recording_name = stem.to_string();
        }
        let _ = meta.save(&zarr_path);
        meta
    };
    let dataset = DatasetMetadata::new_power_of_two(meta.total_samples);
    let _ = tx.send(IoResponse::FileOpened { zarr_path, meta, _dataset: dataset });
}

fn build_local_peaks(zarr_path: PathBuf, tx: Sender<IoResponse>) {
    let mut config = StorageConfig::default();
    config.raw_archive_path = zarr_path.clone();
    let manager = match StorageManager::new(config) {
        Ok(m) => m,
        Err(e) => { let _ = tx.send(IoResponse::Error(e.to_string())); return; }
    };
    let total_samples = if RecordingMeta::exists(&zarr_path) {
        RecordingMeta::load(&zarr_path).map(|m| m.total_samples).unwrap_or(32768)
    } else { 32768 };
    let dataset = DatasetMetadata::new_power_of_two(total_samples);
    let tx_progress = tx.clone();
    let _ = manager.build_peak_pyramid(&dataset, move |p| {
        let _ = tx_progress.send(IoResponse::PeakBuildProgress(p));
    });
    if let Ok(mut meta) = RecordingMeta::load(&zarr_path) {
        meta.lod_levels_available = dataset.lod_chain.iter().filter(|l| l.level > 0).map(|l| l.level).collect();
        let _ = meta.save(&zarr_path);
    }
    dsp_io::dataset_cache::invalidate(&zarr_path);
    let _ = tx.send(IoResponse::PeakBuildComplete { zarr_path });
}

fn fetch_local_view(
    dataset_id: String,
    zarr_path: PathBuf,
    start_sample: u64,
    count: u64,
    width_px: u32,
    channels: Vec<ChannelId>,
    _total_samples: u64,
    tx: Sender<IoResponse>,
) {
    let cached = match dsp_io::dataset_cache::get_or_open(&zarr_path) {
        Ok(c) => c,
        Err(e) => { let _ = tx.send(IoResponse::Error(e.to_string())); return; }
    };
    let mut store = match VirtualChannelStore::new(&zarr_path) {
        Ok(s) => s,
        Err(e) => { let _ = tx.send(IoResponse::Error(e.to_string())); return; }
    };
    let mut ui = UiService::new(&cached.manager, Some(&mut store));
    match ui.fetch_view(&cached.dataset, start_sample, count, width_px, &channels) {
        Ok(resp) => { let _ = tx.send(IoResponse::ViewReady { dataset_id, start_sample, response: resp }); }
        Err(e) => { let _ = tx.send(IoResponse::Error(e.to_string())); }
    }
}

fn fetch_local_overview(
    dataset_id: String,
    zarr_path: PathBuf,
    total_samples: u64,
    width_px: u32,
    channels: Vec<ChannelId>,
    tx: Sender<IoResponse>,
) {
    let cached = match dsp_io::dataset_cache::get_or_open(&zarr_path) {
        Ok(c) => c,
        Err(e) => { let _ = tx.send(IoResponse::Error(e.to_string())); return; }
    };
    let mut store = match VirtualChannelStore::new(&zarr_path) {
        Ok(s) => s,
        Err(e) => { let _ = tx.send(IoResponse::Error(e.to_string())); return; }
    };
    let mut ui = UiService::new(&cached.manager, Some(&mut store));
    // Whole recording (start 0, count = total) → coarsest LOD for ~width_px points.
    match ui.fetch_view(&cached.dataset, 0, total_samples, width_px, &channels) {
        Ok(resp) => { let _ = tx.send(IoResponse::OverviewReady { dataset_id, response: resp }); }
        Err(e) => { let _ = tx.send(IoResponse::Error(e.to_string())); }
    }
}

fn run_local_processing_graph(
    dataset_id: String,
    zarr_path: PathBuf,
    graph_spec: ProcessingGraphSpec,
    total_samples: u64,
    start_sample: u64,
    count: u64,
    batch_size: u64,
    surplus: u64,
    tx: Sender<IoResponse>,
) {
    let mut config = StorageConfig::default();
    config.raw_archive_path = zarr_path.clone();
    let manager = match StorageManager::new(config) {
        Ok(m) => m,
        Err(e) => { let _ = tx.send(IoResponse::Error(e.to_string())); return; }
    };
    let mut store = match VirtualChannelStore::new(&zarr_path) {
        Ok(s) => s,
        Err(e) => { let _ = tx.send(IoResponse::Error(e.to_string())); return; }
    };
    let processor = GraphProcessor::new(graph_spec);
    let tx_progress = tx.clone();
    let result = processor.run_full_recording(&manager, total_samples, start_sample, count, batch_size, surplus, &mut store, Some(&zarr_path), |p| {
        let _ = tx_progress.send(IoResponse::ProcessingProgress(p));
    });
    dsp_io::dataset_cache::invalidate(&zarr_path);
    match result {
        Ok(virtual_channels) => {
            if let Ok(mut meta) = RecordingMeta::load(&zarr_path) {
                for new_vc in &virtual_channels {
                    meta.virtual_channels.retain(|vc| vc.name != new_vc.name);
                    meta.virtual_channels.push(new_vc.clone());
                }
                let _ = meta.save(&zarr_path);
            }
            let _ = tx.send(IoResponse::ProcessingComplete { dataset_id, virtual_channels });
        }
        Err(e) => { let _ = tx.send(IoResponse::Error(e.to_string())); }
    }
}

fn fetch_local_events(
    dataset_id: String,
    zarr_path: PathBuf,
    track_name: String,
    channel_idx: u32,
    start_sample: u64,
    end_sample: u64,
    tx: Sender<IoResponse>,
) {
    let cached = match dsp_io::dataset_cache::get_or_open(&zarr_path) {
        Ok(c) => c,
        Err(e) => { let _ = tx.send(IoResponse::Error(e.to_string())); return; }
    };
    let end = if end_sample == 0 { u64::MAX } else { end_sample };
    match cached.manager.read_events_window(&track_name, channel_idx as u16, start_sample, end) {
        Ok(events) => { let _ = tx.send(IoResponse::EventsReady { dataset_id, track_name, channel_idx, events }); }
        Err(e) => { let _ = tx.send(IoResponse::Error(e.to_string())); }
    }
}

fn fetch_local_cluster_data(
    dataset_id: String,
    zarr_path: PathBuf,
    track_name: String,
    label_id: u32,
    max_waveforms: u32,
    snippet_before: u32,
    snippet_after: u32,
    tx: Sender<IoResponse>,
) {
    let cached = match dsp_io::dataset_cache::get_or_open(&zarr_path) {
        Ok(c) => c,
        Err(e) => { let _ = tx.send(IoResponse::Error(e.to_string())); return; }
    };

    let ui = UiService::new(&cached.manager, None);
    match ui.fetch_cluster_data(&track_name, label_id, max_waveforms, snippet_before, snippet_after) {
        Ok(data) => {
            let _ = tx.send(IoResponse::ClusterDataReady { dataset_id, track_name, data });
        }
        Err(e) => { let _ = tx.send(IoResponse::Error(e.to_string())); }
    }
}
