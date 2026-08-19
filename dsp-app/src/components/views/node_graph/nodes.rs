//! Node types for the DSP node graph.

use serde::{Deserialize, Serialize};
use dsp_io::processing_graph::{ArithOpSpec, SpecNode, ChannelId};
use dsp_core::filter::{WindowType, FilterResponse};
use dsp_core::detection::{CrossingDirection};
use dsp_core::detection::double::DoubleThresholdMode;

// ── Types ─────────────────────────────────────────────────────────────────────

/// Signal buffer: one contiguous slice of `f32` samples.
pub type Signal = Vec<f32>;

/// Range of samples to process.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProcessingRange {
    WholeRecording,
    CurrentSelection,
    Annotation(u64),              // Annotation.id
    Custom { start: u64, end: u64 },
}

impl Default for ProcessingRange {
    fn default() -> Self { Self::WholeRecording }
}

// ── DspNode ───────────────────────────────────────────────────────────────────

/// One node in the DSP graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DspNode {
    // ── Sources ──────────────────────────────────────────────────────────────

    /// Single-channel source.
    Channel {
        id: ChannelId,
        label: String,
        #[serde(default)]
        range: ProcessingRange,
    },

    /// Multi-channel source.
    MultiChannel {
        ids: Vec<ChannelId>,
        label: String,
        /// Raw channel expression: `[0,3,5]`, `0..5`, `0..=5`, or `0-5`.
        input: String,
        #[serde(default)]
        range: ProcessingRange,
    },

    /// Constant scalar repeated for every sample.
    Float {
        value: f32,
        label: String,
    },

    /// Constant boolean: 1.0 (true) or 0.0 (false) for every sample.
    Bool {
        value: bool,
        label: String,
    },

    // ── Arithmetic ────────────────────────────────────────────────────────────

    /// Element-wise arithmetic on two single-channel inputs.
    Arithmetic {
        op: ArithOpSpec,
    },

    // ── Filters (1 input → 1 output) ─────────────────────────────────────────

    /// IIR filter via a Second-Order Sections (SOS) cascade.
    ///
    /// `sos_text` holds the user-editable JSON string (e.g. `"[[b0,b1,b2,1,a1,a2]]"`).
    /// `sos_rows` is kept in sync with parsed content.
    SosFilter {
        sos_text: String,
        sos_rows: Vec<[f32; 6]>,
        filtfilt: bool,
        parse_error: Option<String>,
    },

    /// Windowed-sinc FIR lowpass filter designed from `cutoff_hz`.
    SincLowpass {
        cutoff_hz: f32,
        n_taps: usize,
        window: WindowType,
        center: bool,
    },

    /// Boxcar (uniform) moving average.
    MovingAverage {
        window: usize,
        center: bool,
    },

    /// Moving root-mean-square envelope.
    MovingRms {
        window: usize,
        center: bool,
    },

    /// Causal exponential moving average: `y[t] = alpha*x[t] + (1-alpha)*y[t-1]`.
    ExponentialMovingAverage {
        alpha: f32,
    },

    /// Non-linear median filter for impulse noise rejection.
    MedianFilter {
        window: usize,
        center: bool,
    },

    // ── Designed IIR filters ──────────────────────────────────────────────────

    /// Butterworth IIR — maximally flat passband.
    Butterworth {
        order: usize,
        response: FilterResponse,
        filtfilt: bool,
    },

    /// Chebyshev Type I IIR — equal passband ripple.
    ChebyshevI {
        order: usize,
        ripple_db: f64,
        response: FilterResponse,
        filtfilt: bool,
    },

    /// Chebyshev Type II IIR — equal stopband ripple.
    ChebyshevII {
        order: usize,
        atten_db: f64,
        response: FilterResponse,
        filtfilt: bool,
    },

    /// Bessel IIR — maximally flat group delay.
    Bessel {
        order: usize,
        response: FilterResponse,
        filtfilt: bool,
    },

    /// Narrow notch filter.
    Notch {
        freq_hz: f64,
        q: f64,
        filtfilt: bool,
    },

    /// Parametric EQ peak/cut.
    PeakEq {
        freq_hz: f64,
        q: f64,
        gain_db: f64,
    },

    // ── Epochs (Waveform → Events) ───────────────────────────────────────────

    /// Single threshold crossing detector.
    SingleThresholdCrossing {
        threshold: f32,
        direction: CrossingDirection,
        refractory_samples: usize,
        label_pos: u32,
        label_neg: u32,
        #[serde(default)]
        range: ProcessingRange,
    },

    /// Double threshold detector (Hysteresis or Window).
    DoubleThresholdCrossing {
        low: f32,
        high: f32,
        mode: DoubleThresholdMode,
        refractory_samples: usize,
        label_high_enter: u32,
        label_low_exit: u32,
        #[serde(default)]
        range: ProcessingRange,
    },

    // ── Sinks ─────────────────────────────────────────────────────────────────

    /// Single-channel output with a mini result plot.
    Output {
        label: String,
        /// Filled after the graph is evaluated.
        #[serde(skip)]
        result: Option<Signal>,
    },

    /// Multi-channel output: one input pin per channel.
    MultiChannelOutput {
        n_channels: usize,
        label: String,
        #[serde(skip)]
        results: Vec<Option<Signal>>,
    },

    /// Events sink: writes to Zarr archive.
    EventsOutput {
        track_name: String,
        channel_idx: u16,
    },
}

impl DspNode {
    pub fn n_inputs(&self) -> usize {
        match self {
            DspNode::Channel { .. }
            | DspNode::MultiChannel { .. }
            | DspNode::Float { .. }
            | DspNode::Bool { .. } => 0,
            DspNode::Arithmetic { .. } => 2,
            // All filter nodes have exactly 1 input
            DspNode::SosFilter { .. }
            | DspNode::SincLowpass { .. }
            | DspNode::MovingAverage { .. }
            | DspNode::MovingRms { .. }
            | DspNode::ExponentialMovingAverage { .. }
            | DspNode::MedianFilter { .. }
            | DspNode::Butterworth { .. }
            | DspNode::ChebyshevI { .. }
            | DspNode::ChebyshevII { .. }
            | DspNode::Bessel { .. }
            | DspNode::Notch { .. }
            | DspNode::PeakEq { .. }
            | DspNode::SingleThresholdCrossing { .. }
            | DspNode::DoubleThresholdCrossing { .. }
            | DspNode::EventsOutput { .. } => 1,
            DspNode::Output { .. } => 1,
            DspNode::MultiChannelOutput { n_channels, .. } => *n_channels,
        }
    }

    pub fn n_outputs(&self) -> usize {
        match self {
            DspNode::Channel { .. } | DspNode::Float { .. } | DspNode::Bool { .. } => 1,
            DspNode::MultiChannel { ids, .. } => ids.len().max(1),
            DspNode::Arithmetic { .. } => 1,
            // All filter and detection nodes have exactly 1 output
            DspNode::SosFilter { .. }
            | DspNode::SincLowpass { .. }
            | DspNode::MovingAverage { .. }
            | DspNode::MovingRms { .. }
            | DspNode::ExponentialMovingAverage { .. }
            | DspNode::MedianFilter { .. }
            | DspNode::Butterworth { .. }
            | DspNode::ChebyshevI { .. }
            | DspNode::ChebyshevII { .. }
            | DspNode::Bessel { .. }
            | DspNode::Notch { .. }
            | DspNode::PeakEq { .. }
            | DspNode::SingleThresholdCrossing { .. }
            | DspNode::DoubleThresholdCrossing { .. } => 1,
            DspNode::Output { .. }
            | DspNode::MultiChannelOutput { .. }
            | DspNode::EventsOutput { .. } => 0,
        }
    }

    pub fn title(&self) -> &str {
        match self {
            DspNode::Channel { label, .. } => label,
            DspNode::MultiChannel { label, .. } => label,
            DspNode::Float { label, .. } => label,
            DspNode::Bool { label, .. } => label,
            DspNode::Arithmetic { op } => match op {
                ArithOpSpec::Add => "Add (+)",
                ArithOpSpec::Subtract => "Subtract (−)",
                ArithOpSpec::Multiply => "Multiply (×)",
                ArithOpSpec::Divide => "Divide (÷)",
            },
            DspNode::SosFilter { .. } => "IIR Filter",
            DspNode::SincLowpass { .. } => "Sinc LP",
            DspNode::MovingAverage { .. } => "Moving Avg",
            DspNode::MovingRms { .. } => "Moving RMS",
            DspNode::ExponentialMovingAverage { .. } => "EMA",
            DspNode::MedianFilter { .. } => "Median",
            DspNode::Butterworth { .. } => "Butterworth",
            DspNode::ChebyshevI { .. } => "Chebyshev I",
            DspNode::ChebyshevII { .. } => "Chebyshev II",
            DspNode::Bessel { .. } => "Bessel",
            DspNode::Notch { .. } => "Notch",
            DspNode::PeakEq { .. } => "Peak EQ",
            DspNode::SingleThresholdCrossing { .. } => "Threshold Crossing",
            DspNode::DoubleThresholdCrossing { .. } => "Double Threshold",
            DspNode::Output { label, .. } => label,
            DspNode::MultiChannelOutput { label, .. } => label,
            DspNode::EventsOutput { track_name, .. } => track_name,
        }
    }

    /// Converts this UI node into a pure-data `SpecNode` for the core processor.
    pub fn to_spec_node(&self) -> SpecNode {
        match self {
            DspNode::Channel { id, .. } => SpecNode::Channel { id: id.clone() },
            DspNode::MultiChannel { ids, .. } => SpecNode::MultiChannel { ids: ids.clone() },
            DspNode::Float { value, .. } => SpecNode::Float { value: *value },
            DspNode::Bool { value, .. } => SpecNode::Bool { value: *value },
            DspNode::Arithmetic { op } => SpecNode::Arithmetic { op: *op },
            DspNode::SosFilter { sos_rows, filtfilt, .. } => {
                SpecNode::SosFilter { sos_rows: sos_rows.clone(), filtfilt: *filtfilt }
            }
            DspNode::SincLowpass { cutoff_hz, n_taps, window, center } => {
                SpecNode::SincLowpass {
                    cutoff_hz: *cutoff_hz,
                    n_taps: *n_taps,
                    window: *window,
                    center: *center,
                }
            }
            DspNode::MovingAverage { window, center } => {
                SpecNode::MovingAverage { window: *window, center: *center }
            }
            DspNode::MovingRms { window, center } => {
                SpecNode::MovingRms { window: *window, center: *center }
            }
            DspNode::ExponentialMovingAverage { alpha } => {
                SpecNode::ExponentialMovingAverage { alpha: *alpha }
            }
            DspNode::MedianFilter { window, center } => {
                SpecNode::MedianFilter { window: *window, center: *center }
            }
            DspNode::Butterworth { order, response, filtfilt } => {
                SpecNode::Butterworth { order: *order, response: *response, filtfilt: *filtfilt }
            }
            DspNode::ChebyshevI { order, ripple_db, response, filtfilt } => {
                SpecNode::ChebyshevI { order: *order, ripple_db: *ripple_db, response: *response, filtfilt: *filtfilt }
            }
            DspNode::ChebyshevII { order, atten_db, response, filtfilt } => {
                SpecNode::ChebyshevII { order: *order, atten_db: *atten_db, response: *response, filtfilt: *filtfilt }
            }
            DspNode::Bessel { order, response, filtfilt } => {
                SpecNode::Bessel { order: *order, response: *response, filtfilt: *filtfilt }
            }
            DspNode::Notch { freq_hz, q, filtfilt } => {
                SpecNode::Notch { freq_hz: *freq_hz, q: *q, filtfilt: *filtfilt }
            }
            DspNode::PeakEq { freq_hz, q, gain_db } => {
                SpecNode::PeakEq { freq_hz: *freq_hz, q: *q, gain_db: *gain_db }
            }
            DspNode::SingleThresholdCrossing {
                threshold,
                direction,
                refractory_samples,
                label_pos,
                label_neg,
                ..
            } => SpecNode::SingleThresholdCrossing {
                threshold: *threshold,
                direction: *direction,
                refractory_samples: *refractory_samples,
                label_pos: *label_pos,
                label_neg: *label_neg,
            },
            DspNode::DoubleThresholdCrossing {
                low,
                high,
                mode,
                refractory_samples,
                label_high_enter,
                label_low_exit,
                ..
            } => SpecNode::DoubleThresholdCrossing {
                low: *low,
                high: *high,
                mode: *mode,
                refractory_samples: *refractory_samples,
                label_high_enter: *label_high_enter,
                label_low_exit: *label_low_exit,
            },
            DspNode::Output { label, .. } => {
                if label == "Output" || label.ends_with("_drv") {
                    SpecNode::Output { source_id: ChannelId::Physical(0) }
                } else {
                    SpecNode::Fork { source_id: ChannelId::Physical(0), name: label.clone() }
                }
            }
            DspNode::MultiChannelOutput { label, n_channels, .. } => {
                let names = (0..*n_channels).map(|i| format!("{}_{}", label, i)).collect();
                SpecNode::MultiChannelOutput {
                    names,
                    source_ids: vec![ChannelId::Physical(0); *n_channels],
                }
            }
            DspNode::EventsOutput { track_name, channel_idx } => SpecNode::EventsOutput {
                track_name: track_name.clone(),
                channel_idx: *channel_idx,
                source_id: ChannelId::Physical(0),
            },
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Parses a JSON SOS matrix string into a flat Vec of 6-element rows.
///
/// Accepts both `Vec<[f32; 6]>` and `Vec<Vec<f32>>` JSON representations.
pub fn parse_sos_text(text: &str) -> Result<Vec<[f32; 6]>, String> {
    // Direct fixed-array parse
    if let Ok(rows) = serde_json::from_str::<Vec<[f32; 6]>>(text) {
        return Ok(rows);
    }
    // Fallback: nested Vec<f32>
    let rows: Vec<Vec<f32>> =
        serde_json::from_str(text).map_err(|e| format!("JSON: {}", e))?;
    rows.iter()
        .enumerate()
        .map(|(i, row)| {
            if row.len() != 6 {
                Err(format!("row {}: need 6 values, got {}", i, row.len()))
            } else {
                Ok([row[0], row[1], row[2], row[3], row[4], row[5]])
            }
        })
        .collect()
}

/// Channel expression parser — supports `[0,3,5]`, `0..5`, `0..=5`, `0-5`, `3`.
pub fn parse_channel_input(s: &str) -> Vec<ChannelId> {
    let s = s.trim();

    if s.starts_with('[') && s.ends_with(']') {
        return s[1..s.len() - 1]
            .split(',')
            .filter_map(|p| p.trim().parse::<u16>().ok())
            .map(ChannelId::Physical)
            .collect();
    }

    if let Some(pos) = s.find("..=") {
        if let (Ok(a), Ok(b)) = (
            s[..pos].trim().parse::<u16>(),
            s[pos + 3..].trim().parse::<u16>(),
        ) {
            return (a..=b).map(ChannelId::Physical).collect();
        }
    }

    if let Some(pos) = s.find("..") {
        if let (Ok(a), Ok(b)) = (
            s[..pos].trim().parse::<u16>(),
            s[pos + 2..].trim().parse::<u16>(),
        ) {
            return (a..b).map(ChannelId::Physical).collect();
        }
    }

    if let Some(pos) = s.find('-').filter(|&p| p > 0) {
        if let (Ok(a), Ok(b)) = (
            s[..pos].trim().parse::<u16>(),
            s[pos + 1..].trim().parse::<u16>(),
        ) {
            return (a..=b).map(ChannelId::Physical).collect();
        }
    }

    if let Ok(n) = s.parse::<u16>() {
        return vec![ChannelId::Physical(n)];
    }

    vec![]
}
