//! Persistent virtual channel storage.
//!
//! Each virtual channel (the output of a processed node graph) is stored as a
//! separate memory-mapped file inside a `tmp/` directory that lives next to the
//! recording's `.zarr` directory:
//!
//! ```text
//! ~/datasets/recording_2025/
//!   neural_recording.zarr/    ← raw archive
//!   neural_recording.json     ← sidecar metadata
//!   tmp/
//!     ch0_drv.mmap            ← virtual channel derived from CH0
//!     ch1_drv.mmap
//! ```
//!
//! [`VirtualChannelStore`] owns a `HashMap<String, MmapEngine>` so callers
//! are completely oblivious of file paths or slot indices — channels are
//! addressed by name only.

use crate::mmap::MmapEngine;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ── VirtualChannelStore ───────────────────────────────────────────────────────

/// Manages a set of named virtual channels backed by memory-mapped files.
///
/// All files live in `{zarr_parent}/tmp/`. The store creates the directory on
/// first use and lazily opens/creates individual channel files.
pub struct VirtualChannelStore {
    /// `{zarr_parent}/tmp/`
    root: PathBuf,
    /// Open channel handles keyed by channel name.
    channels: HashMap<String, MmapEngine>,
}

impl VirtualChannelStore {
    /// Creates (or reopens) the store rooted at `{zarr_path.parent()}/tmp/`.
    ///
    /// The `tmp/` directory is created if it does not exist.
    pub fn new(zarr_path: &Path) -> Result<Self> {
        let parent = zarr_path
            .parent()
            .with_context(|| format!("zarr path has no parent: {}", zarr_path.display()))?;
        let root = parent.join("tmp");
        std::fs::create_dir_all(&root)
            .with_context(|| format!("creating virtual channel dir {}", root.display()))?;
        Ok(Self { root, channels: HashMap::new() })
    }

    /// Returns the file path for a named channel.
    fn channel_path(&self, name: &str) -> PathBuf {
        self.root.join(format!("{}.mmap", name))
    }

    /// Opens or creates the mmap file for `name`, sized for `total_samples` f32 values.
    ///
    /// If the file already exists with the correct size, it is opened in-place
    /// (preserving previous results). If the size differs it is recreated.
    pub fn open_or_create(&mut self, name: &str, total_samples: u64) -> Result<&mut MmapEngine> {
        if !self.channels.contains_key(name) {
            let path = self.channel_path(name);
            let engine = MmapEngine::new(&path, 1, total_samples)
                .with_context(|| format!("opening virtual channel '{}'", name))?;
            self.channels.insert(name.to_string(), engine);
        }
        Ok(self.channels.get_mut(name).unwrap())
    }

    /// Writes `samples` into the channel starting at `start_sample`.
    ///
    /// Creates the channel file if it does not exist. `total_samples` is the
    /// full recording length — needed only on first creation.
    pub fn write_window(
        &mut self,
        name: &str,
        start_sample: u64,
        total_samples: u64,
        samples: &[f32],
    ) -> Result<()> {
        let engine = self.open_or_create(name, total_samples)?;
        let slice = engine.get_channel_slice_mut(0, total_samples)?;
        let end = (start_sample as usize + samples.len()).min(slice.len());
        let write_len = end.saturating_sub(start_sample as usize);
        slice[start_sample as usize..end].copy_from_slice(&samples[..write_len]);
        Ok(())
    }

    /// Reads `count` samples from `start_sample` for the named channel.
    ///
    /// Returns zeros if the channel does not exist yet.
    pub fn read_window(
        &mut self,
        name: &str,
        start_sample: u64,
        count: u64,
        total_samples: u64,
    ) -> Result<Vec<f32>> {
        if !self.channels.contains_key(name) {
            let path = self.channel_path(name);
            if path.exists() {
                let engine = MmapEngine::new(&path, 1, total_samples)
                    .with_context(|| format!("opening virtual channel '{}'", name))?;
                self.channels.insert(name.to_string(), engine);
            } else {
                return Ok(vec![0.0f32; count as usize]);
            }
        }
        let engine = self.channels.get(name).unwrap();
        engine.read_window_masked(start_sample, count, &[0])
    }

    /// Returns the names of all channels currently open in this store.
    pub fn channel_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.channels.keys().cloned().collect();
        names.sort();
        names
    }

    /// Returns the names of all `.mmap` files that exist on disk under `tmp/`,
    /// regardless of whether they are currently open.
    pub fn persisted_channel_names(&self) -> Result<Vec<String>> {
        let mut names = Vec::new();
        for entry in std::fs::read_dir(&self.root)
            .with_context(|| format!("reading {}", self.root.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("mmap") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    names.push(stem.to_string());
                }
            }
        }
        names.sort();
        Ok(names)
    }

    /// Flushes all open channel files to disk.
    pub fn flush_all(&self) -> Result<()> {
        for (name, engine) in &self.channels {
            engine
                .flush()
                .with_context(|| format!("flushing virtual channel '{}'", name))?;
        }
        Ok(())
    }

    /// Returns the root `tmp/` directory path.
    pub fn root(&self) -> &Path {
        &self.root
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn make_store(_total_samples: u64) -> (VirtualChannelStore, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        // Simulate a zarr path: dir/recording.zarr
        let zarr_path = dir.path().join("recording.zarr");
        let store = VirtualChannelStore::new(&zarr_path).unwrap();
        assert!(store.root().exists());
        (store, dir)
    }

    #[test]
    fn write_and_read_roundtrip() {
        let total = 1000u64;
        let (mut store, _dir) = make_store(total);

        let data: Vec<f32> = (0..100).map(|i| i as f32 * 0.1).collect();
        store.write_window("ch0_drv", 0, total, &data).unwrap();

        let read = store.read_window("ch0_drv", 0, 100, total).unwrap();
        assert_eq!(read.len(), 100);
        for (i, (&a, &b)) in data.iter().zip(read.iter()).enumerate() {
            assert!((a - b).abs() < 1e-6, "mismatch at {}: {} != {}", i, a, b);
        }
    }

    #[test]
    fn write_mid_window() {
        let total = 1000u64;
        let (mut store, _dir) = make_store(total);

        let data = vec![7.0f32; 50];
        store.write_window("ch0_drv", 200, total, &data).unwrap();

        // Zeros before window
        let before = store.read_window("ch0_drv", 0, 100, total).unwrap();
        assert!(before.iter().all(|&v| v == 0.0));

        // Written region
        let written = store.read_window("ch0_drv", 200, 50, total).unwrap();
        assert!(written.iter().all(|&v| (v - 7.0).abs() < 1e-6));
    }

    #[test]
    fn two_channels_are_independent() {
        let total = 500u64;
        let (mut store, _dir) = make_store(total);

        store.write_window("ch0_drv", 0, total, &vec![1.0f32; 100]).unwrap();
        store.write_window("ch1_drv", 0, total, &vec![2.0f32; 100]).unwrap();

        let ch0 = store.read_window("ch0_drv", 0, 100, total).unwrap();
        let ch1 = store.read_window("ch1_drv", 0, 100, total).unwrap();
        assert!(ch0.iter().all(|&v| (v - 1.0).abs() < 1e-6));
        assert!(ch1.iter().all(|&v| (v - 2.0).abs() < 1e-6));
    }

    #[test]
    fn missing_channel_returns_zeros() {
        let total = 200u64;
        let (mut store, _dir) = make_store(total);
        let zeros = store.read_window("ghost", 0, 50, total).unwrap();
        assert!(zeros.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn persisted_names_survive_reopen() {
        let total = 300u64;
        let dir = tempdir().unwrap();
        let zarr_path = dir.path().join("rec.zarr");

        {
            let mut store = VirtualChannelStore::new(&zarr_path).unwrap();
            store.write_window("ch0_drv", 0, total, &vec![1.0f32; 10]).unwrap();
            store.write_window("ch2_drv", 0, total, &vec![2.0f32; 10]).unwrap();
            store.flush_all().unwrap();
        }

        let store2 = VirtualChannelStore::new(&zarr_path).unwrap();
        let names = store2.persisted_channel_names().unwrap();
        assert_eq!(names, vec!["ch0_drv", "ch2_drv"]);
    }

    #[test]
    fn channel_names_sorted() {
        let total = 100u64;
        let (mut store, _dir) = make_store(total);
        store.write_window("ch2_drv", 0, total, &[]).unwrap();
        store.write_window("ch0_drv", 0, total, &[]).unwrap();
        store.write_window("ch1_drv", 0, total, &[]).unwrap();
        let names = store.channel_names();
        assert_eq!(names, vec!["ch0_drv", "ch1_drv", "ch2_drv"]);
    }
}
