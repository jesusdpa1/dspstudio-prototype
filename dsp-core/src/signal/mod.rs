//! Signal family type hierarchy.
//!
//! Two top-level families cover all data DSP Studio can hold:
//!
//! * [`SignalFamily::Stream`] — dense, uniformly sampled data (waveform,
//!   image sequences).  Shares a continuous time axis implicitly defined by
//!   `sample_index / fs`.  PKF decimation, filtering, and spectral analysis
//!   are all valid on Stream data.
//!
//! * [`SignalFamily::Epochs`] — sparse, event-indexed data (spike timestamps,
//!   stimulus triggers, behaviour markers).  Events reference the same `fs`
//!   clock as the session's streams, so `sample_offset / fs` gives the
//!   event's absolute time.  PKF and convolution are *not* valid on Epochs
//!   data; raster / rug plots are.
//!
//! Both families are first-class: either can originate from hardware, a file,
//! or the output of a DSP processing node.

use serde::{Deserialize, Serialize};

pub mod dataset;

// ── Stream ─────────────────────────────────────────────────────────────────────

/// Concrete subtypes of [`SignalFamily::Stream`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StreamKind {
    /// Scalar value per channel per sample — electrophysiology, EEG, EMG, audio.
    Waveform,
    /// 2D spatial frame per sample — calcium imaging, behavioural video.
    /// `width` × `height` pixels, stored as `f32` per pixel.
    ImageSequence { width: u32, height: u32 },
}

// ── Epochs ─────────────────────────────────────────────────────────────────────

/// Concrete subtypes of [`SignalFamily::Epochs`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EpochsKind {
    /// Discrete timestamped events with integer labels resolved via a
    /// [`LabelVocabulary`].  Per-channel sorted by `sample_offset`.
    Events,
}

// ── SignalFamily ───────────────────────────────────────────────────────────────

/// Top-level discriminant for a track's data family.
///
/// Determines valid storage layout, processing operations, and plot renderer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SignalFamily {
    Stream(StreamKind),
    Epochs(EpochsKind),
}

impl SignalFamily {
    /// Convenience constructor for the most common stream type.
    pub fn waveform() -> Self {
        Self::Stream(StreamKind::Waveform)
    }

    /// Convenience constructor for discrete event tracks.
    pub fn events() -> Self {
        Self::Epochs(EpochsKind::Events)
    }

    /// Returns `true` if this family supports PKF (peak-keep-floor) decimation
    /// and waveform-based operations (filtering, spectral analysis).
    pub fn supports_waveform_ops(&self) -> bool {
        matches!(self, Self::Stream(StreamKind::Waveform))
    }

    /// Returns `true` if this is a sparse event track.
    pub fn is_events(&self) -> bool {
        matches!(self, Self::Epochs(EpochsKind::Events))
    }

    /// Returns `true` if this is any continuously-sampled stream.
    pub fn is_stream(&self) -> bool {
        matches!(self, Self::Stream(_))
    }

    /// Human-readable name used in the UI.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Stream(StreamKind::Waveform) => "Waveform",
            Self::Stream(StreamKind::ImageSequence { .. }) => "Image Sequence",
            Self::Epochs(EpochsKind::Events) => "Events",
        }
    }
}

impl Default for SignalFamily {
    fn default() -> Self {
        Self::waveform()
    }
}

// ── Event ──────────────────────────────────────────────────────────────────────

/// A single sparse event: the sample at which it occurred and its label.
///
/// `sample_offset / session_fs` gives the wall-clock time.
/// `label_id` indexes into the track's [`LabelVocabulary`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Event {
    /// Sample index relative to the start of the session.
    pub sample_offset: u64,
    /// Index into the track's [`LabelVocabulary`].
    pub label_id: u32,
}

impl Event {
    pub fn new(sample_offset: u64, label_id: u32) -> Self {
        Self { sample_offset, label_id }
    }

    /// Wall-clock time in seconds.
    pub fn time_secs(&self, fs: f64) -> f64 {
        self.sample_offset as f64 / fs
    }
}

// ── Spike ──────────────────────────────────────────────────────────────────────

/// A rich representation of a neural spike.
/// 
/// Unlike a simple [`Event`], a `Spike` carries its associated waveform
/// snippet and extracted PCA features, making it suitable for sorting and 
/// clustering workflows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Spike {
    /// Absolute sample index of the peak.
    pub sample: u64,
    /// Channel index where the spike was detected.
    pub channel: u16,
    /// Unit ID (label) assigned by a sorter. 0 usually means "unclassified".
    pub unit_id: u32,
    /// Waveform snippet around the detection peak.
    /// Layout: [Samples] for single-channel, or flattened [Samples * Channels] for masked.
    pub waveform: Option<Vec<f32>>,
    /// Extracted features (e.g., PCA scores).
    pub features: Option<Vec<f32>>,
}

impl Spike {
    pub fn new(sample: u64, channel: u16) -> Self {
        Self {
            sample,
            channel,
            unit_id: 0,
            waveform: None,
            features: None,
        }
    }

    pub fn with_waveform(mut self, waveform: Vec<f32>) -> Self {
        self.waveform = Some(waveform);
        self
    }

    pub fn with_features(mut self, features: Vec<f32>) -> Self {
        self.features = Some(features);
        self
    }

    pub fn with_unit(mut self, unit_id: u32) -> Self {
        self.unit_id = unit_id;
        self
    }
}

// ── LabelVocabulary ────────────────────────────────────────────────────────────

/// Dense mapping from integer `label_id` → human-readable label string.
///
/// IDs are assigned in insertion order starting from 0.  The vocabulary is
/// stored in the session sidecar JSON alongside the track that owns it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LabelVocabulary {
    /// `labels[label_id]` is the string name for that ID.
    pub labels: Vec<String>,
}

impl LabelVocabulary {
    pub fn new(labels: Vec<String>) -> Self {
        Self { labels }
    }

    /// Look up the string for a label ID.
    pub fn label(&self, id: u32) -> Option<&str> {
        self.labels.get(id as usize).map(String::as_str)
    }

    /// Find the ID for a given label string.
    pub fn id_for(&self, label: &str) -> Option<u32> {
        self.labels.iter().position(|l| l == label).map(|i| i as u32)
    }

    /// Return the existing ID for `label`, or insert it and return the new ID.
    pub fn get_or_insert(&mut self, label: &str) -> u32 {
        if let Some(id) = self.id_for(label) {
            return id;
        }
        let id = self.labels.len() as u32;
        self.labels.push(label.to_string());
        id
    }

    pub fn len(&self) -> usize {
        self.labels.len()
    }

    pub fn is_empty(&self) -> bool {
        self.labels.is_empty()
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_family_helpers() {
        let wf = SignalFamily::waveform();
        assert!(wf.supports_waveform_ops());
        assert!(wf.is_stream());
        assert!(!wf.is_events());

        let ev = SignalFamily::events();
        assert!(!ev.supports_waveform_ops());
        assert!(!ev.is_stream());
        assert!(ev.is_events());
    }

    #[test]
    fn label_vocabulary_get_or_insert() {
        let mut vocab = LabelVocabulary::default();
        assert_eq!(vocab.get_or_insert("spike"), 0);
        assert_eq!(vocab.get_or_insert("noise"), 1);
        assert_eq!(vocab.get_or_insert("spike"), 0); // idempotent
        assert_eq!(vocab.len(), 2);
    }

    #[test]
    fn event_time_secs() {
        let ev = Event::new(40_000, 0);
        assert!((ev.time_secs(40_000.0) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn signal_family_equality() {
        assert_eq!(SignalFamily::events(), SignalFamily::events());
        assert_ne!(SignalFamily::waveform(), SignalFamily::events());
        let img = SignalFamily::Stream(StreamKind::ImageSequence { width: 512, height: 512 });
        assert_ne!(img, SignalFamily::waveform());
    }
}
