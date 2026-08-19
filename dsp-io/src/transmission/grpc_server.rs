//! tonic gRPC server exposing the transmission service.
//!
//! # Service endpoints
//!
//! | RPC | Request | Response | Notes |
//! |-----|---------|----------|-------|
//! | `OpenFile` | `path: String` | `RecordingMeta` | Loads the JSON sidecar next to the `.zarr` |
//! | `SaveMetadata` | `zarr_path`, `RecordingMeta` | `Empty` | Persists the sidecar |
//! | `FetchView` | [`ViewRequest`] | [`ViewResponse`] | LOD-aware viewport; see data layout below |
//! | `FetchRaw` | [`ViewRequest`] | [`ViewResponse`] | Alias for `FetchView` (same LOD logic) |
//! | `RunProcessingGraph` | [`RunGraphRequest`] | `Stream<ProcessingResponse>` | Streams `Progress` events then `Complete` |
//!
//! # ViewResponse data layout
//!
//! ```text
//! lod_level == 0 (raw):
//!   data = [ch0_s0, ch0_s1, …, ch1_s0, …]      // points_per_channel floats per channel
//!
//! lod_level > 0 (peaks):
//!   data = [ch0_min0, ch0_max0, ch0_min1, …]    // points_per_channel * 2 floats per channel
//! ```
//!
//! # Starting the server
//!
//! ```rust,no_run
//! use dsp_io::transmission::grpc_server::start_grpc_server;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let addr = "[::1]:50051".parse()?;
//!     start_grpc_server(addr).await?;
//!     Ok(())
//! }
//! ```
//!
//! # Connecting a client (same process)
//!
//! ```rust,no_run
//! use dsp_io::proto::transmission::transmission_service_client::TransmissionServiceClient;
//! use dsp_io::proto::transmission::{channel_id, ChannelId, OpenFileRequest, ViewRequest};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let mut client = TransmissionServiceClient::connect("http://[::1]:50051").await?;
//!
//!     let meta = client.open_file(OpenFileRequest {
//!         path: "path/to/recording.zarr".into(),
//!     }).await?.into_inner();
//!
//!     let view = client.fetch_view(ViewRequest {
//!         zarr_path: "path/to/recording.zarr".into(),
//!         start_sample: 0,
//!         count: 40_000,
//!         width_px: 1920,
//!         channels: vec![ChannelId { kind: Some(channel_id::Kind::Physical(0)) }],
//!     }).await?.into_inner();
//!
//!     println!("LOD {}, {} points/ch", view.lod_level, view.points_per_channel);
//!     Ok(())
//! }
//! ```

use std::path::PathBuf;
use tonic::{Request, Response, Status};
use tonic::codec::CompressionEncoding;

/// Upper bound for a single gRPC message. Neural viewports / spike artifacts can
/// far exceed tonic's 4 MiB default, so raise it on both encode and decode.
const MAX_MESSAGE_BYTES: usize = 64 * 1024 * 1024;
use crate::config::StorageConfig;
use crate::recording_meta::RecordingMeta;
use crate::zarr::StorageManager;
use crate::transmission::ui::UiService;
use crate::processing_graph::{GraphProcessor, ProcessingGraphSpec, ChannelId};
use crate::virtual_channel::VirtualChannelStore;
use crate::proto::transmission::transmission_service_server::TransmissionService;
use crate::proto::transmission::{
    OpenFileRequest, SaveMetadataRequest, ViewRequest, ViewResponse,
    FetchEventsRequest, FetchEventsResponse, FetchClusterDataRequest, FetchClusterDataResponse,
    Event as ProtoEvent,
    RunGraphRequest, ProcessingResponse, ProcessingProgress, ProcessingComplete,
    Empty, VirtualChannelMeta as ProtoVirtualChannelMeta,
    RecordingMeta as ProtoRecordingMeta,
    ChannelId as ProtoChannelId,
};
use crate::proto::transmission::channel_id::Kind as ProtoChannelKind;

pub struct MyTransmissionService {}

impl MyTransmissionService {
    pub fn new() -> Self {
        Self {}
    }
}

#[tonic::async_trait]
impl TransmissionService for MyTransmissionService {
    async fn open_file(
        &self,
        request: Request<OpenFileRequest>,
    ) -> Result<Response<ProtoRecordingMeta>, Status> {
        let path = PathBuf::from(request.into_inner().path);
        if !RecordingMeta::exists(&path) {
            return Err(Status::not_found("Recording metadata not found"));
        }
        let mut meta = RecordingMeta::load(&path)
            .map_err(|e| Status::internal(format!("Failed to load metadata: {}", e)))?;
        
        meta.ensure_tracks();
        Ok(Response::new(to_proto_meta(meta)))
    }

    async fn save_metadata(
        &self,
        request: Request<SaveMetadataRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let path = PathBuf::from(req.zarr_path);
        let proto_meta = req.meta.ok_or_else(|| Status::invalid_argument("Missing metadata"))?;
        let meta = from_proto_meta(proto_meta);
        meta.save(&path)
            .map_err(|e| Status::internal(format!("Failed to save metadata: {}", e)))?;
        // Cached handles hold a stale RecordingMeta copy; drop them.
        crate::dataset_cache::invalidate(&path);
        Ok(Response::new(Empty {}))
    }

    async fn fetch_view(
        &self,
        request: Request<ViewRequest>,
    ) -> Result<Response<ViewResponse>, Status> {
        let timer = crate::metrics::Timer::new();
        let req = request.into_inner();
        let path = PathBuf::from(req.zarr_path);

        let cached = crate::dataset_cache::get_or_open(&path)
            .map_err(|e| Status::internal(format!("Failed to open storage: {}", e)))?;

        let mut store = VirtualChannelStore::new(&path)
            .map_err(|e| Status::internal(format!("Failed to open virtual store: {}", e)))?;

        let mut ui_service = UiService::new(&cached.manager, Some(&mut store));

        let mut channels: Vec<ChannelId> = Vec::with_capacity(req.channels.len());
        for c in req.channels {
            if c.kind.is_none() {
                return Err(Status::invalid_argument("ChannelId with empty kind"));
            }
            channels.push(from_proto_channel_id(c));
        }
        let view = ui_service.fetch_view(&cached.dataset, req.start_sample, req.count, req.width_px, &channels)
            .map_err(|e| Status::internal(format!("Failed to fetch view: {}", e)))?;

        let bytes = (view.data.len() * 4) as u64;
        crate::metrics::record_transmission(bytes, timer.elapsed_ms());

        Ok(Response::new(ViewResponse {
            data: view.data,
            lod_level: view.lod_level as u32,
            decimation_ratio: view.decimation_ratio,
            points_per_channel: view.points_per_channel as u64,
            channels_returned: view.channels_returned.into_iter().map(to_proto_channel_id).collect(),
            actual_start: view.actual_start,
        }))
    }

    async fn fetch_raw(
        &self,
        request: Request<ViewRequest>,
    ) -> Result<Response<ViewResponse>, Status> {
        // NOTE: `fetch_raw` is a thin alias for `fetch_view` and is therefore
        // **LOD-aware** — it returns decimated peaks when the request spans more
        // samples than `width_px`, not guaranteed raw samples. It only yields
        // true raw data when the LOD resolves to level 0 (high zoom). Callers
        // needing guaranteed full-resolution data must request a window narrow
        // enough to select LOD 0.
        self.fetch_view(request).await
    }

    async fn fetch_events(
        &self,
        request: Request<FetchEventsRequest>,
    ) -> Result<Response<FetchEventsResponse>, Status> {
        let timer = crate::metrics::Timer::new();
        let req = request.into_inner();
        let path = PathBuf::from(req.zarr_path);

        let cached = crate::dataset_cache::get_or_open(&path)
            .map_err(|e| Status::internal(format!("Failed to open storage: {}", e)))?;

        let end = if req.end_sample == 0 { u64::MAX } else { req.end_sample };

        let events = cached.manager.read_events_window(&req.track_name, req.channel_idx as u16, req.start_sample, end)
            .map_err(|e| Status::internal(format!("Failed to read events: {}", e)))?;

        let proto_events: Vec<ProtoEvent> = events.into_iter().map(|e| ProtoEvent {
            sample_offset: e.sample_offset,
            label_id: e.label_id,
        }).collect();

        // Approximate size: 12 bytes per event (8 + 4)
        let bytes = (proto_events.len() * 12) as u64;
        crate::metrics::record_transmission(bytes, timer.elapsed_ms());

        Ok(Response::new(FetchEventsResponse {
            events: proto_events,
        }))
    }

    async fn fetch_cluster_data(
        &self,
        request: Request<FetchClusterDataRequest>,
    ) -> Result<Response<FetchClusterDataResponse>, Status> {
        let timer = crate::metrics::Timer::new();
        let req = request.into_inner();
        let path = PathBuf::from(req.zarr_path);

        let cached = crate::dataset_cache::get_or_open(&path)
            .map_err(|e| Status::internal(format!("Failed to open storage: {}", e)))?;

        let ui_service = UiService::new(&cached.manager, None);
        let data = ui_service.fetch_cluster_data(
            &req.track_name, 
            req.label_id, 
            req.max_waveforms, 
            req.snippet_before, 
            req.snippet_after
        ).map_err(|e| Status::internal(format!("Failed to fetch cluster data: {}", e)))?;

        crate::metrics::record_transmission((data.waveforms.len() * 4) as u64, timer.elapsed_ms());

        Ok(Response::new(FetchClusterDataResponse {
            label_id: data.label_id,
            pca_pc1: data.pca_pc1,
            pca_pc2: data.pca_pc2,
            waveforms: data.waveforms,
            mean_waveform: data.mean_waveform,
            std_waveform: data.std_waveform,
            snippet_len: data.snippet_len as u32,
            n_spikes: data.n_spikes as u32,
        }))
    }

    type RunProcessingGraphStream = tokio_stream::wrappers::ReceiverStream<Result<ProcessingResponse, Status>>;

    async fn run_processing_graph(
        &self,
        request: Request<RunGraphRequest>,
    ) -> Result<Response<Self::RunProcessingGraphStream>, Status> {
        let req = request.into_inner();
        let path = PathBuf::from(req.zarr_path);
        let spec: ProcessingGraphSpec = serde_json::from_str(&req.graph_spec_json)
            .map_err(|e| Status::invalid_argument(format!("Invalid graph spec JSON: {}", e)))?;

        let (tx, rx) = tokio::sync::mpsc::channel(100);

        tokio::task::spawn_blocking(move || {
            let timer = crate::metrics::Timer::new();
            let total_samples = req.total_samples;
            let mut config = StorageConfig::default();
            config.raw_archive_path = path.clone();
            
            let manager = match StorageManager::new(config) {
                Ok(m) => m,
                Err(e) => {
                    let _ = tx.blocking_send(Err(Status::internal(format!("Failed to open storage: {}", e))));
                    return;
                }
            };

            let mut store = match VirtualChannelStore::new(&path) {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx.blocking_send(Err(Status::internal(format!("Failed to open virtual store: {}", e))));
                    return;
                }
            };

            let processor = GraphProcessor::new(spec);
            let tx_clone = tx.clone();
            let start_sample = req.start_sample;
            let count = if req.count == 0 { req.total_samples } else { req.count };
            
            let result = processor.run_full_recording(
                &manager,
                req.total_samples,
                start_sample,
                count,
                req.batch_size,
                req.surplus,
                &mut store,
                Some(&path),
                move |p| {
                    let _ = tx_clone.blocking_send(Ok(ProcessingResponse {
                        event: Some(crate::proto::transmission::processing_response::Event::Progress(ProcessingProgress {
                            progress: p,
                            message: format!("Processing batch... {:.1}%", p * 100.0),
                        })),
                    }));
                },
            );

            // Processing writes virtual channels and may add event/spike arrays;
            // drop cached handles so subsequent reads see the new state.
            crate::dataset_cache::invalidate(&path);

            match result {
                Ok(virtual_channels) => {
                    crate::metrics::record_processing(total_samples, timer.elapsed_ms());
                    let proto_vcs = virtual_channels.into_iter().map(|vc| ProtoVirtualChannelMeta {
                        name: vc.name,
                        source_channel_idx: vc.source_channel_idx as u32,
                        created_at: vc.created_at,
                    }).collect();

                    let _ = tx.blocking_send(Ok(ProcessingResponse {
                        event: Some(crate::proto::transmission::processing_response::Event::Complete(ProcessingComplete {
                            virtual_channels: proto_vcs,
                        })),
                    }));
                }
                Err(e) => {
                    let _ = tx.blocking_send(Err(Status::internal(format!("Processing failed: {}", e))));
                }
            }
        });

        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }
}

pub async fn start_grpc_server(addr: std::net::SocketAddr) -> Result<(), tonic::transport::Error> {
    let service = MyTransmissionService::new();
    let server = crate::proto::transmission::transmission_service_server::TransmissionServiceServer::new(service)
        .send_compressed(CompressionEncoding::Zstd)
        .accept_compressed(CompressionEncoding::Zstd)
        .max_decoding_message_size(MAX_MESSAGE_BYTES)
        .max_encoding_message_size(MAX_MESSAGE_BYTES);
    tonic::transport::Server::builder()
        .add_service(server)
        .serve(addr)
        .await
}

// ── Conversion Helpers ─────────────────────────────────────────────────────────

pub fn to_proto_channel_id(id: ChannelId) -> ProtoChannelId {
    match id {
        ChannelId::Physical(idx) => ProtoChannelId {
            kind: Some(ProtoChannelKind::Physical(idx as u32)),
        },
        ChannelId::Virtual(name) => ProtoChannelId {
            kind: Some(ProtoChannelKind::Virtual(name)),
        },
    }
}

pub fn from_proto_channel_id(proto: ProtoChannelId) -> ChannelId {
    match proto.kind {
        Some(ProtoChannelKind::Physical(idx)) => ChannelId::Physical(idx as u16),
        Some(ProtoChannelKind::Virtual(name)) => ChannelId::Virtual(name),
        None => ChannelId::Physical(0),
    }
}

pub fn to_proto_meta(m: RecordingMeta) -> ProtoRecordingMeta {
    let tracks = m.tracks.iter().map(|t| crate::proto::transmission::TrackMeta {
        name: t.name.clone(),
        channel_indices: t.channel_indices.iter().map(|&i| i as u32).collect(),
        is_events: t.family.is_events(),
        labels: t.label_vocabulary.labels.clone(),
    }).collect();

    ProtoRecordingMeta {
        session_id: m.session_id,
        recording_name: m.recording_name,
        recording_type: m.recording_type,
        description: m.description,
        sample_rate: m.sample_rate,
        n_channels: m.n_channels as u32,
        total_samples: m.total_samples,
        channel_names: m.channel_names,
        lod_levels_available: m.lod_levels_available.iter().map(|&l| l as u32).collect(),
        created_at: m.created_at,
        virtual_channels: m.virtual_channels.into_iter().map(|vc| ProtoVirtualChannelMeta {
            name: vc.name,
            source_channel_idx: vc.source_channel_idx as u32,
            created_at: vc.created_at,
        }).collect(),
        tracks,
    }
}

pub fn from_proto_meta(m: ProtoRecordingMeta) -> RecordingMeta {
    let tracks = m.tracks.into_iter().map(|t| {
        let family = if t.is_events {
            dsp_core::signal::SignalFamily::events()
        } else {
            dsp_core::signal::SignalFamily::waveform()
        };
        crate::recording_meta::TrackMeta {
            name: t.name,
            channel_indices: t.channel_indices.iter().map(|&i| i as u16).collect(),
            family,
            label_vocabulary: dsp_core::signal::LabelVocabulary::new(t.labels),
        }
    }).collect();

    RecordingMeta {
        session_id: m.session_id,
        recording_name: m.recording_name,
        recording_type: m.recording_type,
        description: m.description,
        sample_rate: m.sample_rate,
        n_channels: m.n_channels as u16,
        total_samples: m.total_samples,
        channel_names: m.channel_names,
        lod_levels_available: m.lod_levels_available.iter().map(|&l| l as u8).collect(),
        created_at: m.created_at,
        virtual_channels: m.virtual_channels.into_iter().map(|vc| crate::recording_meta::VirtualChannelMeta {
            name: vc.name,
            source_channel_idx: vc.source_channel_idx as u16,
            created_at: vc.created_at,
        }).collect(),
        tracks,
        annotations: Vec::new(),
        preferred_blueprint: None,
    }
}
