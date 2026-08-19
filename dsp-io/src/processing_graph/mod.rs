use crate::processing_history::ProcessingHistory;
use crate::recording_meta::VirtualChannelMeta;
use crate::transmission::processing::ProcessingService;
use crate::virtual_channel::VirtualChannelStore;
use crate::zarr::StorageManager;
use anyhow::{Result, bail};
use dsp_core::signal::Event;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

pub mod spec;
pub mod processor;
pub mod nodes;

pub use spec::*;
pub use processor::*;
pub use nodes::*;

// ── SignalValue ───────────────────────────────────────────────────────────────

/// Typed wire payload.  A wire carries either a dense waveform or a sparse
/// list of events, depending on the upstream node's output type.
#[derive(Debug, Clone)]
pub enum SignalValue {
    /// Dense, uniformly-sampled waveform: one f32 per sample in the window.
    Waveform(Vec<f32>),
    /// Sparse events: (relative_sample_offset, label_id) within the window.
    Events(Vec<Event>),
}

impl SignalValue {
    /// Borrow the waveform slice, or `None` if this is an events value.
    pub fn as_waveform(&self) -> Option<&[f32]> {
        if let SignalValue::Waveform(v) = self {
            Some(v)
        } else {
            None
        }
    }

    /// Consume and return the waveform, or `None` if this is an events value.
    pub fn into_waveform(self) -> Option<Vec<f32>> {
        if let SignalValue::Waveform(v) = self {
            Some(v)
        } else {
            None
        }
    }

    /// Borrow the event list, or `None` if this is a waveform value.
    pub fn as_events(&self) -> Option<&[Event]> {
        if let SignalValue::Events(v) = self {
            Some(v)
        } else {
            None
        }
    }
}

// ── Channel Addressing ────────────────────────────────────────────────────────

/// Unified identifier for a channel, regardless of its storage backend.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ChannelId {
    /// A raw physical channel from the Zarr archive.
    Physical(u16),
    /// A processed virtual channel from an Mmap file.
    Virtual(String),
}

impl ChannelId {
    /// Returns the physical index if this is a physical channel.
    pub fn as_physical(&self) -> Option<u16> {
        match self {
            ChannelId::Physical(idx) => Some(*idx),
            _ => None,
        }
    }

    /// Returns the canonical derived-slot name for this channel.
    ///
    /// - `Physical(N)` → `"ch{N:02}_drv"` (e.g. `"ch00_drv"`, `"ch01_drv"`)
    /// - `Virtual(name)` → `name` (same slot, in-place overwrite)
    pub fn drv_name(&self) -> String {
        match self {
            ChannelId::Physical(idx) => format!("ch{:02}_drv", idx),
            ChannelId::Virtual(name) => name.clone(),
        }
    }
}

// Union-find helpers for independent_groups().

pub(crate) fn uf_find(parent: &mut Vec<usize>, x: usize) -> usize {
    let mut root = x;
    while parent[root] != root {
        root = parent[root];
    }
    // Path compression.
    let mut curr = x;
    while parent[curr] != root {
        let next = parent[curr];
        parent[curr] = root;
        curr = next;
    }
    root
}

pub(crate) fn uf_union(parent: &mut Vec<usize>, x: usize, y: usize) {
    let rx = uf_find(parent, x);
    let ry = uf_find(parent, y);
    if rx != ry {
        parent[ry] = rx;
    }
}
mod tests;
