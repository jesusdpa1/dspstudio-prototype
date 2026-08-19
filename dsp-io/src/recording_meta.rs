//! Sidecar metadata for a recording session.
//!
//! Each Zarr recording is accompanied by a JSON file stored next to the `.zarr`
//! directory. For example, `session.zarr` has a sidecar at `session.json`.
//! The sidecar stores human-readable information (channel names, sample rate,
//! recording type) that is not part of the Zarr schema itself.
//!
//! # Invariants
//! * `channel_names.len() == n_channels as usize`
//! * `lod_levels_available` lists only levels that have been fully written to
//!   the Zarr store (`/peaks/lod_N` arrays exist).

use anyhow::{Context, Result};
use dsp_core::signal::{LabelVocabulary, SignalFamily};
use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

// ── TrackMeta ─────────────────────────────────────────────────────────────────

/// Metadata for one named track within a recording session.
///
/// A track groups one or more physical channels that share the same
/// [`SignalFamily`].  Waveform tracks map directly to physical channel indices
/// in the Zarr `/raw` array.  Events tracks have their own storage under
/// `/events/{name}/`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrackMeta {
    /// Human-readable track name shown in the UI (e.g. `"LFP"`, `"Spikes"`,
    /// `"Stim Triggers"`).  Also used as the key in zarr event storage.
    pub name: String,
    /// Indices into `RecordingMeta::channel_names` that belong to this track.
    pub channel_indices: Vec<u16>,
    /// Data family: what kind of data this track holds and how to render it.
    pub family: SignalFamily,
    /// Label vocabulary for [`SignalFamily::Epochs`] tracks.
    /// Always empty for [`SignalFamily::Stream`] tracks.
    #[serde(default)]
    pub label_vocabulary: LabelVocabulary,
}

impl TrackMeta {
    /// Create a waveform track covering the given channel indices.
    pub fn waveform(name: impl Into<String>, channel_indices: Vec<u16>) -> Self {
        Self {
            name: name.into(),
            channel_indices,
            family: SignalFamily::waveform(),
            label_vocabulary: LabelVocabulary::default(),
        }
    }

    /// Create an events track covering the given channel indices.
    pub fn events(
        name: impl Into<String>,
        channel_indices: Vec<u16>,
        label_vocabulary: LabelVocabulary,
    ) -> Self {
        Self {
            name: name.into(),
            channel_indices,
            family: SignalFamily::events(),
            label_vocabulary,
        }
    }
}

// ── VirtualChannelMeta ────────────────────────────────────────────────────────

/// Describes one virtual (processed) channel stored in `tmp/`.
///
/// Virtual channels are derived from physical source channels by running a
/// node graph. The file lives at `{zarr_parent}/tmp/{name}.mmap`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VirtualChannelMeta {
    /// Unique name of the virtual channel, e.g. `"ch0_drv"`.
    /// Determines the mmap file name.
    pub name: String,
    /// Index of the physical source channel this was derived from.
    pub source_channel_idx: u16,
    /// Unix timestamp (seconds) when this virtual channel was first created.
    pub created_at: String,
}

impl VirtualChannelMeta {
    pub fn new(name: impl Into<String>, source_channel_idx: u16) -> Self {
        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs().to_string())
            .unwrap_or_default();
        Self { name: name.into(), source_channel_idx, created_at }
    }
}

// ── Annotation ────────────────────────────────────────────────────────────────

/// A named time-region annotation within a recording session.
///
/// Annotations are stored in the recording's sidecar JSON and are used to
/// mark regions of interest (e.g. "Baseline", "Treatment", "Artifact").
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Annotation {
    /// Stable identifier for this annotation.
    pub id: u64,
    /// User-defined label (e.g. `"Baseline"`, `"Stimulation"`).
    pub label: String,
    /// Start sample index (inclusive).
    pub start: u64,
    /// End sample index (exclusive).
    pub end: u64,
    /// RGB color for display in the timeline.
    pub color: [u8; 3],
    /// Display-only visibility toggle. Hidden annotations are still usable in processing.
    pub visible: bool,
    /// If true, the annotation cannot be dragged or resized in the timeline.
    pub locked: bool,
    /// Display order in the timeline (DAW-style track order).
    pub row_index: usize,
}

// ── RecordingMeta ─────────────────────────────────────────────────────────────

/// Metadata for a DSP Studio recording session.
///
/// Serialized as a pretty-printed JSON file placed next to the `.zarr` directory.
///
/// # Multi-recording support
/// `session_id` is a stable UUID generated on first save. When multiple
/// recordings are loaded simultaneously, their session IDs distinguish them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingMeta {
    /// Stable identifier for this recording session (UUID v4, hex string).
    ///
    /// Generated once on first [`RecordingMeta::save`] and never changed.
    /// Defaults to an empty string for sidecars that pre-date this field.
    #[serde(default)]
    pub session_id: String,
    /// Human-readable name shown in the UI title bar and info panel.
    pub recording_name: String,
    /// Signal modality: `"EMG"`, `"EEG"`, `"ECG"`, `"Generic"`, etc.
    pub recording_type: String,
    /// Free-text description of the recording.
    pub description: String,
    /// Acquisition sample rate in Hz (e.g. `40000.0`).
    pub sample_rate: f32,
    /// Number of physical recording channels.
    pub n_channels: u16,
    /// Total number of samples per channel in the raw archive.
    pub total_samples: u64,
    /// Display name for each channel. Length must equal `n_channels`.
    pub channel_names: Vec<String>,
    /// LOD levels that have been built and exist in the Zarr store.
    /// An empty list means only raw (level 0) data is available.
    pub lod_levels_available: Vec<u8>,
    /// Unix timestamp (seconds since epoch) when the sidecar was first created.
    pub created_at: String,
    /// Virtual channels produced by running the node graph on this recording.
    ///
    /// Defaults to empty for sidecars that pre-date this field.
    #[serde(default)]
    pub virtual_channels: Vec<VirtualChannelMeta>,

    /// Named tracks grouping channels by signal family.
    ///
    /// Defaults to empty for sidecars that pre-date this field.
    /// Call [`RecordingMeta::ensure_tracks`] to auto-generate waveform tracks
    /// from `channel_names` when loading legacy sidecars.
    #[serde(default)]
    pub tracks: Vec<TrackMeta>,

    /// Optional preferred UI layout (Blueprint) stored as JSON.
    ///
    /// Defaults to None. If present, the UI will attempt to apply this layout
    /// when the recording is opened.
    #[serde(default)]
    pub preferred_blueprint: Option<String>,

    /// Named time-region annotations for this recording.
    ///
    /// Defaults to empty for sidecars that pre-date this field.
    #[serde(default)]
    pub annotations: Vec<Annotation>,
}

impl RecordingMeta {
    /// Returns the path of the sidecar JSON for a given Zarr directory.
    ///
    /// # Example
    /// ```
    /// use std::path::Path;
    /// use dsp_io::recording_meta::RecordingMeta;
    /// let p = RecordingMeta::sidecar_path(Path::new("/data/session.zarr"));
    /// assert_eq!(p, std::path::PathBuf::from("/data/session.json"));
    /// ```
    pub fn sidecar_path(zarr_path: &Path) -> std::path::PathBuf {
        zarr_path.with_extension("json")
    }

    /// Returns `true` if the sidecar file exists next to `zarr_path`.
    pub fn exists(zarr_path: &Path) -> bool {
        Self::sidecar_path(zarr_path).exists()
    }

    /// Loads the sidecar JSON from next to `zarr_path`.
    ///
    /// Fails if the file does not exist or cannot be parsed.
    pub fn load(zarr_path: &Path) -> Result<Self> {
        let path = Self::sidecar_path(zarr_path);
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading recording metadata from {}", path.display()))?;
        serde_json::from_str(&text)
            .with_context(|| format!("parsing recording metadata from {}", path.display()))
    }

    /// Writes the sidecar JSON next to `zarr_path`.
    ///
    /// Auto-populates `session_id` with a new UUID v4 if the field is empty
    /// (e.g. sidecars created before this field existed).  The updated id is
    /// written to disk but **not** mutated on `self` — callers that need the
    /// assigned id should re-load after saving.
    pub fn save(&self, zarr_path: &Path) -> Result<()> {
        let path = Self::sidecar_path(zarr_path);
        let mut owned;
        let to_write: &RecordingMeta = if self.session_id.is_empty() {
            owned = self.clone();
            owned.session_id = Uuid::new_v4().to_string();
            &owned
        } else {
            self
        };
        let text = serde_json::to_string_pretty(to_write)
            .context("serializing recording metadata")?;
        std::fs::write(&path, text)
            .with_context(|| format!("writing recording metadata to {}", path.display()))
    }

    /// Ensure `tracks` is non-empty, generating one waveform track per
    /// physical channel from `channel_names` if tracks is empty.
    ///
    /// Call this after loading a legacy sidecar that pre-dates the tracks field.
    /// Idempotent — does nothing if tracks is already populated.
    pub fn ensure_tracks(&mut self) {
        if !self.tracks.is_empty() {
            return;
        }
        self.tracks = self
            .channel_names
            .iter()
            .enumerate()
            .map(|(i, name)| TrackMeta::waveform(name.clone(), vec![i as u16]))
            .collect();
    }

    /// Return all tracks whose family is a waveform stream.
    pub fn waveform_tracks(&self) -> impl Iterator<Item = &TrackMeta> {
        self.tracks.iter().filter(|t| t.family.supports_waveform_ops())
    }

    /// Return all tracks whose family is Events.
    pub fn event_tracks(&self) -> impl Iterator<Item = &TrackMeta> {
        self.tracks.iter().filter(|t| t.family.is_events())
    }

    /// Creates a default `RecordingMeta` from the minimal known facts.
    ///
    /// Channel names default to `"CH0"`, `"CH1"`, …
    /// `lod_levels_available` starts empty — call
    /// [`StorageManager::build_peak_pyramid`](crate::zarr::StorageManager::build_peak_pyramid)
    /// to populate it.
    pub fn default_for(n_channels: u16, total_samples: u64, sample_rate: f32) -> Self {
        let channel_names = (0..n_channels).map(|i| format!("CH{}", i)).collect();
        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs().to_string())
            .unwrap_or_default();
        let tracks = (0..n_channels)
            .map(|i| TrackMeta::waveform(format!("CH{}", i), vec![i]))
            .collect();
        Self {
            session_id: Uuid::new_v4().to_string(),
            recording_name: "Untitled Recording".to_string(),
            recording_type: "Generic".to_string(),
            description: String::new(),
            sample_rate,
            n_channels,
            total_samples,
            channel_names,
            lod_levels_available: Vec::new(),
            created_at,
            virtual_channels: Vec::new(),
            tracks,
            preferred_blueprint: None,
            annotations: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn sidecar_path_replaces_extension() {
        let p = RecordingMeta::sidecar_path(std::path::Path::new("/data/my_rec.zarr"));
        assert_eq!(p, std::path::PathBuf::from("/data/my_rec.json"));
    }

    #[test]
    fn sidecar_path_no_extension() {
        let p = RecordingMeta::sidecar_path(std::path::Path::new("/data/session"));
        assert_eq!(p, std::path::PathBuf::from("/data/session.json"));
    }

    #[test]
    fn default_for_generates_correct_channel_names() {
        let meta = RecordingMeta::default_for(4, 1000, 40000.0);
        assert_eq!(meta.channel_names, vec!["CH0", "CH1", "CH2", "CH3"]);
        assert_eq!(meta.n_channels, 4);
        assert_eq!(meta.total_samples, 1000);
        assert_eq!(meta.sample_rate, 40000.0);
        assert!(meta.lod_levels_available.is_empty());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempdir().unwrap();
        let zarr_path = dir.path().join("test.zarr");

        let mut original = RecordingMeta::default_for(2, 8000, 40000.0);
        original.recording_name = "Test Session".to_string();
        original.channel_names = vec!["EMG_L".to_string(), "EMG_R".to_string()];
        original.lod_levels_available = vec![1, 2, 3];

        original.save(&zarr_path).expect("save failed");
        assert!(RecordingMeta::exists(&zarr_path));

        let loaded = RecordingMeta::load(&zarr_path).expect("load failed");
        assert_eq!(loaded.recording_name, "Test Session");
        assert_eq!(loaded.channel_names, vec!["EMG_L", "EMG_R"]);
        assert_eq!(loaded.lod_levels_available, vec![1, 2, 3]);
        assert_eq!(loaded.sample_rate, 40000.0);
        assert_eq!(loaded.total_samples, 8000);
    }

    #[test]
    fn exists_returns_false_when_no_sidecar() {
        let dir = tempdir().unwrap();
        let zarr_path = dir.path().join("missing.zarr");
        assert!(!RecordingMeta::exists(&zarr_path));
    }

    #[test]
    fn load_fails_gracefully_on_missing_file() {
        let dir = tempdir().unwrap();
        let zarr_path = dir.path().join("ghost.zarr");
        assert!(RecordingMeta::load(&zarr_path).is_err());
    }

    #[test]
    fn default_for_has_session_id_and_empty_virtual_channels() {
        let meta = RecordingMeta::default_for(2, 1000, 40000.0);
        assert!(!meta.session_id.is_empty(), "session_id should be generated");
        assert!(meta.virtual_channels.is_empty());
    }

    #[test]
    fn save_generates_session_id_for_empty_sidecar() {
        let dir = tempdir().unwrap();
        let zarr_path = dir.path().join("rec.zarr");

        let meta = RecordingMeta::default_for(1, 100, 1000.0);
        let mut meta = RecordingMeta { session_id: String::new(), ..meta }; // simulate legacy sidecar
        meta.save(&zarr_path).unwrap();

        let loaded = RecordingMeta::load(&zarr_path).unwrap();
        assert!(!loaded.session_id.is_empty(), "save should assign a session_id");
    }

    #[test]
    fn save_preserves_existing_session_id() {
        let dir = tempdir().unwrap();
        let zarr_path = dir.path().join("rec.zarr");

        let mut meta = RecordingMeta::default_for(1, 100, 1000.0);
        let original_id = meta.session_id.clone();
        meta.save(&zarr_path).unwrap();

        let loaded = RecordingMeta::load(&zarr_path).unwrap();
        assert_eq!(loaded.session_id, original_id);
    }

    #[test]
    fn virtual_channel_meta_roundtrip() {
        let dir = tempdir().unwrap();
        let zarr_path = dir.path().join("rec.zarr");

        let mut meta = RecordingMeta::default_for(1, 100, 1000.0);
        meta.virtual_channels.push(VirtualChannelMeta::new("ch0_drv", 0));
        meta.save(&zarr_path).unwrap();

        let loaded = RecordingMeta::load(&zarr_path).unwrap();
        assert_eq!(loaded.virtual_channels.len(), 1);
        assert_eq!(loaded.virtual_channels[0].name, "ch0_drv");
        assert_eq!(loaded.virtual_channels[0].source_channel_idx, 0);
        assert!(!loaded.virtual_channels[0].created_at.is_empty());
    }

    #[test]
    fn save_overwrites_existing_sidecar() {
        let dir = tempdir().unwrap();
        let zarr_path = dir.path().join("rec.zarr");

        let mut meta = RecordingMeta::default_for(1, 100, 1000.0);
        meta.recording_name = "First".to_string();
        meta.save(&zarr_path).unwrap();

        meta.recording_name = "Second".to_string();
        meta.save(&zarr_path).unwrap();

        let loaded = RecordingMeta::load(&zarr_path).unwrap();
        assert_eq!(loaded.recording_name, "Second");
    }
}
