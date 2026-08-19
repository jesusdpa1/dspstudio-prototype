use crate::zarr::StorageManager;
use crate::virtual_channel::VirtualChannelStore;
use crate::metadata::DatasetMetadata;
use crate::transmission::ui::{UiService, ViewResponse};
use crate::processing_graph::ChannelId;
use anyhow::Result;

/// Where a specific channel's data is currently being pulled from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChannelSource {
    /// Pull from the immutable Zarr archive (default).
    Archive,
    /// Pull from the active Mmap shadow (processed/active).
    Shadow,
}

/// Orchestrates the relationship between Zarr (Archive) and Mmap (Shadow).
///
/// Provides a unified "Virtual Data Plane" where each channel can be
/// independently routed to a different storage backend.
pub struct SessionManager {
    pub storage: StorageManager,
    pub virtual_store: VirtualChannelStore,
    pub routing: Vec<ChannelSource>,
}

impl SessionManager {
    pub fn new(storage: StorageManager, virtual_store: VirtualChannelStore, channel_count: u16) -> Self {
        Self {
            storage,
            virtual_store,
            routing: vec![ChannelSource::Archive; channel_count as usize],
        }
    }

    pub fn route_to_shadow(&mut self, channel_index: u16) {
        if let Some(source) = self.routing.get_mut(channel_index as usize) {
            *source = ChannelSource::Shadow;
        }
    }

    pub fn route_to_archive(&mut self, channel_index: u16) {
        if let Some(source) = self.routing.get_mut(channel_index as usize) {
            *source = ChannelSource::Archive;
        }
    }

    // ── Events passthrough ────────────────────────────────────────────────────

    /// Read all events for a channel within `[start_sample, end_sample)` from
    /// the Zarr archive.  Returns an empty `Vec` if no events track exists.
    pub fn read_events_window(
        &self,
        track_name: &str,
        channel_idx: u16,
        start_sample: u64,
        end_sample: u64,
    ) -> Result<Vec<dsp_core::signal::Event>> {
        self.storage
            .read_events_window(track_name, channel_idx, start_sample, end_sample)
    }

    /// Returns `true` if an events track with the given name exists in the
    /// archive (i.e. has been written by a previous processing run).
    pub fn events_track_exists(&self, track_name: &str) -> bool {
        self.storage.events_track_exists(track_name)
    }

    /// Fetches a unified view across both the archive and shadow (virtual)
    /// backends. Returns a merged `ViewResponse` with all requested channels.
    pub fn fetch_composite_view(
        &mut self,
        metadata: &DatasetMetadata,
        start_sample: u64,
        count: u64,
        width_px: u32,
        channels: &[u16],
    ) -> Result<ViewResponse> {
        let mut agnostic_channels = Vec::new();

        for &ch in channels {
            let source = self.routing.get(ch as usize).unwrap_or(&ChannelSource::Archive);
            match source {
                ChannelSource::Archive => agnostic_channels.push(ChannelId::Physical(ch)),
                ChannelSource::Shadow => agnostic_channels.push(ChannelId::Virtual(format!("ch{:02}_drv", ch))),
            }
        }

        let mut ui_service = UiService::new(&self.storage, Some(&mut self.virtual_store));
        let resp = ui_service.fetch_view(metadata, start_sample, count, width_px, &agnostic_channels)?;
        Ok(resp)
    }

}
