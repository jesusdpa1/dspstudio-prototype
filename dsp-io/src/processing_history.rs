//! Per-channel processing event log, persisted as `processing.json`.
//!
//! Every time a [`GraphProcessor`] runs over a channel and writes a result to
//! a [`VirtualChannelStore`] slot, one [`ProcessingEvent`] is appended to that
//! channel's history.  The history survives across sessions and can be used to
//! reconstruct or visualize the full transformation chain applied to each channel.
//!
//! ## File location
//!
//! ```text
//! ~/datasets/recording_2025/
//!   neural_recording.zarr/
//!   neural_recording.json      ← RecordingMeta sidecar
//!   processing.json            ← ProcessingHistory  ← this file
//!   tmp/
//!     ch00_drv.mmap
//! ```
//!
//! [`GraphProcessor`]: crate::processing_graph::GraphProcessor
//! [`VirtualChannelStore`]: crate::virtual_channel::VirtualChannelStore

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

// ── ProcessingEvent ────────────────────────────────────────────────────────────

/// One processing step applied to a single virtual channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingEvent {
    /// Unix timestamp (seconds) when this event was recorded.
    pub timestamp: String,
    /// Short human-readable description of the operations, e.g. `"multiply, add"`.
    pub label: String,
    /// Full [`ProcessingGraphSpec`] JSON that produced this result.
    /// Can be deserialized and re-run to reproduce the output.
    ///
    /// [`ProcessingGraphSpec`]: crate::processing_graph::ProcessingGraphSpec
    pub graph_spec_json: String,
}

// ── ProcessingHistory ─────────────────────────────────────────────────────────

/// Ordered per-channel log of every processing operation applied to a recording.
///
/// Stored as pretty-printed JSON next to the Zarr archive.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProcessingHistory {
    /// Key = virtual channel name (e.g. `"ch00_drv"`).
    /// Value = ordered list of events, oldest first.
    pub channels: HashMap<String, Vec<ProcessingEvent>>,
}

impl ProcessingHistory {
    /// Returns the path of the `processing.json` file for a given Zarr directory.
    pub fn history_path(zarr_path: &Path) -> std::path::PathBuf {
        zarr_path
            .parent()
            .unwrap_or(Path::new("."))
            .join("processing.json")
    }

    /// Loads the history from disk, or returns an empty history if the file
    /// does not exist yet.
    pub fn load(zarr_path: &Path) -> Result<Self> {
        let path = Self::history_path(zarr_path);
        if !path.exists() {
            return Ok(Self::default());
        }
        let json = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        serde_json::from_str(&json)
            .with_context(|| format!("parsing {}", path.display()))
    }

    /// Persists the history to `processing.json`.
    pub fn save(&self, zarr_path: &Path) -> Result<()> {
        let path = Self::history_path(zarr_path);
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, json)
            .with_context(|| format!("writing {}", path.display()))
    }

    /// Appends one processing event to `channel`'s log.
    pub fn append(&mut self, channel: &str, label: impl Into<String>, graph_spec_json: impl Into<String>) {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs().to_string())
            .unwrap_or_default();

        self.channels
            .entry(channel.to_string())
            .or_default()
            .push(ProcessingEvent {
                timestamp,
                label: label.into(),
                graph_spec_json: graph_spec_json.into(),
            });
    }

    /// Returns all events for `channel` in order, or an empty slice.
    pub fn events_for(&self, channel: &str) -> &[ProcessingEvent] {
        self.channels.get(channel).map(Vec::as_slice).unwrap_or(&[])
    }
}
