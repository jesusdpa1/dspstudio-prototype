//! Sparse event detection via threshold crossing.
//!
//! This module provides high-performance, parallelized detectors that scan
//! dense waveforms for specific triggers (crossings, windows, hysteresis).
//!
//! Results are returned as a sparse `Vec<DetectedEvent>`, optimized for
//! memory and subsequent storage.

use serde::{Deserialize, Serialize};

pub mod single;
pub mod double;
pub mod adaptive;

pub use single::{SingleThresholdDetector, SingleThresholdState};
pub use double::{DoubleThresholdDetector, DoubleThresholdState};

/// A sparse event detected during a processing run.
/// 
/// Aligned to the user's requested 3-column format [sample, channel, label].
/// Uses `u16` for channel to match the rest of the crate's conventions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DetectedEvent {
    /// Absolute sample index.
    pub sample: u64,
    /// Channel index.
    pub channel: u16,
    /// Label ID (e.g., from a LabelVocabulary).
    pub label: u32,
}

impl DetectedEvent {
    pub fn new(sample: u64, channel: u16, label: u32) -> Self {
        Self { sample, channel, label }
    }
}

/// Common trait for all threshold-based detectors.
pub trait DetectionDetector: Send + Sync {
    /// Scans a multi-channel buffer for events.
    /// 
    /// # Arguments
    /// * `data` - Channel-major flat buffer.
    /// * `n_channels` - Number of channels in the buffer.
    /// * `start_sample` - Absolute offset of the first sample in `data`.
    fn detect(
        &self,
        data: &[f32],
        n_channels: usize,
        start_sample: u64,
    ) -> Vec<DetectedEvent>;
}

/// Defines the direction of a signal crossing relative to a threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrossingDirection {
    /// Trigger when signal goes from below to above.
    Positive,
    /// Trigger when signal goes from above to below.
    Negative,
    /// Trigger on any crossing.
    Both,
}
