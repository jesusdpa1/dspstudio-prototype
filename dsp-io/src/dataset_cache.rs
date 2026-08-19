//! Process-global cache of per-recording read handles.
//!
//! Every viewport/event fetch previously rebuilt a [`StorageManager`] +
//! `FilesystemStore`, re-parsed the [`RecordingMeta`] JSON sidecar, and re-opened
//! the zarr arrays. During a pan/zoom that work repeated on every frame. This
//! cache keeps the immutable, read-only pieces alive and shareable across
//! requests, keyed by the recording's `.zarr` path.
//!
//! Only immutable state lives here. Virtual-channel reads still go through a
//! per-request [`VirtualChannelStore`](crate::virtual_channel::VirtualChannelStore)
//! because they are mutable and not shareable. Any path that mutates a dataset
//! (metadata save, processing graph run, peak build, re-encode) MUST call
//! [`invalidate`] so the next read re-opens fresh handles.

use crate::config::StorageConfig;
use crate::metadata::DatasetMetadata;
use crate::recording_meta::RecordingMeta;
use crate::zarr::StorageManager;
use anyhow::Result;
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Cached read handles for one recording.
pub struct CachedDataset {
    pub manager: StorageManager,
    pub meta: RecordingMeta,
    pub dataset: DatasetMetadata,
}

static CACHE: Lazy<RwLock<HashMap<PathBuf, Arc<CachedDataset>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// Returns cached handles for `zarr_path`, opening (and caching) them on a miss.
///
/// The handles are built outside the write lock so concurrent opens of *other*
/// datasets never block on disk I/O. A benign race may open the same dataset
/// twice on a cold miss; the last writer wins and both callers get valid state.
pub fn get_or_open(zarr_path: &Path) -> Result<Arc<CachedDataset>> {
    if let Some(hit) = CACHE.read().get(zarr_path) {
        return Ok(hit.clone());
    }

    let config = StorageConfig {
        raw_archive_path: zarr_path.to_path_buf(),
        ..StorageConfig::default()
    };
    let manager = StorageManager::new(config)?;
    let meta = RecordingMeta::load(zarr_path)?;
    let dataset = DatasetMetadata::new_power_of_two(meta.total_samples);
    let entry = Arc::new(CachedDataset { manager, meta, dataset });

    CACHE.write().insert(zarr_path.to_path_buf(), entry.clone());
    Ok(entry)
}

/// Drops any cached handles for `zarr_path`. Call after writing to the dataset.
pub fn invalidate(zarr_path: &Path) {
    CACHE.write().remove(zarr_path);
}
