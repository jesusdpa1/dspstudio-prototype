//! Transmission layer — serves data to consumers (UI, DSP kernels, remote clients).
//!
//! Three sub-modules handle distinct access patterns:
//!
//! | Sub-module | Consumer | Key type |
//! |------------|----------|----------|
//! | [`ui`] | Renderer / egui | [`ui::UiService`] — LOD-aware viewport fetch |
//! | [`processing`] | DSP kernels / graph executor | [`processing::ProcessingService`] — surplus-windowed batch fetch |
//! | [`grpc_server`] | Remote processes (dsp-cli, Python) | [`grpc_server::MyTransmissionService`] — tonic gRPC server |
//!
//! All three share the same underlying [`crate::zarr::StorageManager`] and
//! [`crate::virtual_channel::VirtualChannelStore`] primitives; they differ only
//! in how they slice and serve the data.

pub mod ui;
pub mod processing;
pub mod grpc_server;
