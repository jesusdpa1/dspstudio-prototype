use serde::{Deserialize, Serialize};
use dsp_core::filter::FilterResponse;
use dsp_core::detection::CrossingDirection;
use dsp_core::detection::double::DoubleThresholdMode;
use super::{ChannelId};

// ── Arithmetic ────────────────────────────────────────────────────────────────

/// The four basic arithmetic operations supported by the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArithOpSpec {
    Add,
    Subtract,
    Multiply,
    Divide,
}

impl ArithOpSpec {
    pub fn apply(&self, a: &[f32], b: &[f32]) -> Vec<f32> {
        let len = a.len().min(b.len());
        (0..len)
            .map(|i| match self {
                ArithOpSpec::Add => a[i] + b[i],
                ArithOpSpec::Subtract => a[i] - b[i],
                ArithOpSpec::Multiply => a[i] * b[i],
                ArithOpSpec::Divide => {
                    if b[i].abs() < f32::EPSILON {
                        0.0
                    } else {
                        a[i] / b[i]
                    }
                }
            })
            .collect()
    }
}

// ── SpecNode ──────────────────────────────────────────────────────────────────

/// One node in the serializable graph spec.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SpecNode {
    /// Emits one channel's samples (output pin 0).
    Channel { id: ChannelId },

    /// Emits N channels; output pin K → channels\[K\].
    MultiChannel { ids: Vec<ChannelId> },

    /// Constant scalar repeated for every sample (output pin 0).
    Float { value: f32 },

    /// Constant 0.0 (false) or 1.0 (true) repeated for every sample (output pin 0).
    Bool { value: bool },

    /// Element-wise arithmetic on two inputs; output pin 0 = result.
    Arithmetic { op: ArithOpSpec },

    // ── Filters (1 input → 1 output) ────────────────────────────────────────
    /// IIR filter via a Second-Order Sections (SOS) cascade.
    SosFilter {
        sos_rows: Vec<[f32; 6]>,
        filtfilt: bool,
    },

    /// Windowed-sinc FIR lowpass filter designed from `cutoff_hz`.
    SincLowpass {
        cutoff_hz: f32,
        n_taps: usize,
        window: dsp_core::filter::WindowType,
        center: bool,
    },

    /// Uniform boxcar moving average.
    MovingAverage { window: usize, center: bool },

    /// Moving root-mean-square envelope (non-linear).
    MovingRms { window: usize, center: bool },

    /// Causal exponential moving average: `y[t] = alpha*x[t] + (1-alpha)*y[t-1]`.
    ExponentialMovingAverage { alpha: f32 },

    /// Non-linear median filter for impulse noise rejection.
    MedianFilter { window: usize, center: bool },

    // ── Designed IIR filters ─────────────────────────────────────────────────
    /// Butterworth IIR filter — maximally flat passband.
    Butterworth {
        order: usize,
        response: FilterResponse,
        filtfilt: bool,
    },

    /// Chebyshev Type I IIR — equal passband ripple, steeper roll-off.
    ChebyshevI {
        order: usize,
        ripple_db: f64,
        response: FilterResponse,
        filtfilt: bool,
    },

    /// Chebyshev Type II IIR — monotone passband, equal stopband ripple.
    ChebyshevII {
        order: usize,
        atten_db: f64,
        response: FilterResponse,
        filtfilt: bool,
    },

    /// Bessel IIR — maximally flat group delay (linear phase in passband).
    Bessel {
        order: usize,
        response: FilterResponse,
        filtfilt: bool,
    },

    /// Narrow notch (zero-phase or causal).
    Notch {
        freq_hz: f64,
        q: f64,
        filtfilt: bool,
    },

    /// Parametric EQ peak/cut.
    PeakEq { freq_hz: f64, q: f64, gain_db: f64 },

    // ── Epochs (Waveform → Events) ───────────────────────────────────────────
    /// Triggers when a signal crosses a single fixed threshold.
    SingleThresholdCrossing {
        threshold: f32,
        direction: CrossingDirection,
        refractory_samples: usize,
        label_pos: u32,
        label_neg: u32,
    },

    /// Triggers using two thresholds (Hysteresis or Window mode).
    DoubleThresholdCrossing {
        low: f32,
        high: f32,
        mode: DoubleThresholdMode,
        refractory_samples: usize,
        label_high_enter: u32,
        label_low_exit: u32,
    },

    // ── Sinks ────────────────────────────────────────────────────────────────
    /// Sink node: writes input pin 0 to the canonical derived slot for `source_id`.
    Output { source_id: ChannelId },

    /// Sink node: writes input pin 0 to an **explicitly named** new channel.
    Fork { source_id: ChannelId, name: String },

    /// Multi-channel sink: N input pins → N named virtual channels.
    MultiChannelOutput {
        names: Vec<String>,
        source_ids: Vec<ChannelId>,
    },

    /// Events sink: accumulates events across all batches and writes them to
    /// the Zarr archive under `/events/{track_name}/` at the end of the run.
    EventsOutput {
        track_name: String,
        channel_idx: u16,
        source_id: ChannelId,
    },
}

// ── SpecWire ──────────────────────────────────────────────────────────────────

/// A directed wire from one node's output pin to another node's input pin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecWire {
    pub from_node: usize,
    pub from_output: usize,
    pub to_node: usize,
    pub to_input: usize,
}

// ── ProcessingGraphSpec ───────────────────────────────────────────────────────

/// A complete, serializable description of a DSP processing graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingGraphSpec {
    pub nodes: Vec<SpecNode>,
    pub wires: Vec<SpecWire>,
    pub sample_rate: f32,
}

impl ProcessingGraphSpec {
    /// Returns the indices of all sink nodes (waveform or events).
    pub fn output_node_indices(&self) -> Vec<usize> {
        self.nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| match n {
                SpecNode::Output { .. }
                | SpecNode::Fork { .. }
                | SpecNode::MultiChannelOutput { .. }
                | SpecNode::EventsOutput { .. } => Some(i),
                _ => None,
            })
            .collect()
    }

    /// Returns all `(channel_name, source_id)` pairs declared by waveform sink
    /// nodes (Output / Fork / MultiChannelOutput).  EventsOutput is excluded.
    pub fn declared_outputs(&self) -> Vec<(String, ChannelId)> {
        let mut out = Vec::new();
        for node in &self.nodes {
            match node {
                SpecNode::Output { source_id } => {
                    out.push((source_id.drv_name(), source_id.clone()));
                }
                SpecNode::Fork { source_id, name } => {
                    out.push((name.clone(), source_id.clone()));
                }
                SpecNode::MultiChannelOutput { names, source_ids } => {
                    for (name, id) in names.iter().zip(source_ids.iter()) {
                        out.push((name.clone(), id.clone()));
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// Returns all `(track_name, channel_idx, source_id)` tuples declared by
    /// [`SpecNode::EventsOutput`] nodes.
    pub fn declared_events_outputs(&self) -> Vec<(String, u16, ChannelId)> {
        self.nodes
            .iter()
            .filter_map(|n| {
                if let SpecNode::EventsOutput {
                    track_name,
                    channel_idx,
                    source_id,
                } = n
                {
                    Some((track_name.clone(), *channel_idx, source_id.clone()))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Returns the minimum surplus (samples each side) required by any filter node
    /// in this graph.
    pub fn required_surplus(&self) -> u64 {
        let max_s: usize = self
            .nodes
            .iter()
            .map(|n| match n {
                SpecNode::SosFilter { sos_rows, .. } => {
                    let sections: Vec<_> = sos_rows
                        .iter()
                        .map(|&r| dsp_core::filter::iir::SosSection::from_row(r))
                        .collect();
                    dsp_core::filter::FilterDesign::from_sections(sections).recommended_surplus()
                }
                SpecNode::SincLowpass { n_taps, center, .. } => {
                    if *center {
                        n_taps.saturating_sub(1) / 2
                    } else {
                        n_taps.saturating_sub(1)
                    }
                }
                SpecNode::MovingAverage { window, center }
                | SpecNode::MovingRms { window, center } => {
                    if *center {
                        window.saturating_sub(1) / 2
                    } else {
                        window.saturating_sub(1)
                    }
                }
                SpecNode::MedianFilter { window, .. } => window / 2,
                SpecNode::ExponentialMovingAverage { .. } => 0,
                SpecNode::Butterworth {
                    order, response, ..
                }
                | SpecNode::ChebyshevI {
                    order, response, ..
                }
                | SpecNode::ChebyshevII {
                    order, response, ..
                }
                | SpecNode::Bessel {
                    order, response, ..
                } => {
                    let sections = match response {
                        FilterResponse::BandPass { .. } | FilterResponse::BandStop { .. } => {
                            order * 2
                        }
                        _ => order.saturating_add(1) / 2 + order % 2,
                    };
                    12 * sections
                }
                SpecNode::Notch { .. } | SpecNode::PeakEq { .. } => 64,
                SpecNode::SingleThresholdCrossing { .. }
                | SpecNode::DoubleThresholdCrossing { .. } => 0,
                _ => 0,
            })
            .max()
            .unwrap_or(0);
        (max_s.max(64)) as u64
    }

    /// Like `required_surplus` but uses pre-compiled filter designs.
    pub fn required_surplus_compiled(
        &self,
        compiled: &std::collections::HashMap<usize, dsp_core::filter::FilterDesign>,
    ) -> u64 {
        let max_s: usize = self
            .nodes
            .iter()
            .enumerate()
            .map(|(idx, n)| {
                if let Some(fd) = compiled.get(&idx) {
                    return fd.recommended_surplus();
                }
                match n {
                    SpecNode::SosFilter { sos_rows, .. } => {
                        let sections: Vec<_> = sos_rows
                            .iter()
                            .map(|&r| dsp_core::filter::iir::SosSection::from_row(r))
                            .collect();
                        dsp_core::filter::FilterDesign::from_sections(sections)
                            .recommended_surplus()
                    }
                    SpecNode::SincLowpass { n_taps, center, .. } => {
                        if *center {
                            n_taps.saturating_sub(1) / 2
                        } else {
                            n_taps.saturating_sub(1)
                        }
                    }
                    SpecNode::MovingAverage { window, center }
                    | SpecNode::MovingRms { window, center } => {
                        if *center {
                            window.saturating_sub(1) / 2
                        } else {
                            window.saturating_sub(1)
                        }
                    }
                    SpecNode::MedianFilter { window, .. } => window / 2,
                    _ => 0,
                }
            })
            .max()
            .unwrap_or(0);
        (max_s.max(64)) as u64
    }

    /// Generates a short human-readable label summarising the operations in the graph.
    pub fn auto_label(&self) -> String {
        let parts: Vec<String> = self
            .nodes
            .iter()
            .filter_map(|n| match n {
                SpecNode::Arithmetic { op } => Some(format!("{:?}", op).to_lowercase()),
                SpecNode::SosFilter { filtfilt, .. } => {
                    Some(if *filtfilt { "iir_ff" } else { "iir" }.into())
                }
                SpecNode::SincLowpass { cutoff_hz, .. } => Some(format!("lp{:.0}hz", cutoff_hz)),
                SpecNode::MovingAverage { window, .. } => Some(format!("ma{}", window)),
                SpecNode::MovingRms { window, .. } => Some(format!("rms{}", window)),
                SpecNode::ExponentialMovingAverage { alpha } => Some(format!("ema{:.2}", alpha)),
                SpecNode::MedianFilter { window, .. } => Some(format!("med{}", window)),
                SpecNode::Butterworth {
                    order, filtfilt, ..
                } => Some(format!(
                    "butter{}{}",
                    order,
                    if *filtfilt { "_ff" } else { "" }
                )),
                SpecNode::ChebyshevI {
                    order, filtfilt, ..
                } => Some(format!(
                    "cheby1_{}{}",
                    order,
                    if *filtfilt { "_ff" } else { "" }
                )),
                SpecNode::ChebyshevII {
                    order, filtfilt, ..
                } => Some(format!(
                    "cheby2_{}{}",
                    order,
                    if *filtfilt { "_ff" } else { "" }
                )),
                SpecNode::Bessel {
                    order, filtfilt, ..
                } => Some(format!(
                    "bessel{}{}",
                    order,
                    if *filtfilt { "_ff" } else { "" }
                )),
                SpecNode::Notch { freq_hz, .. } => Some(format!("notch{:.0}hz", freq_hz)),
                SpecNode::PeakEq {
                    freq_hz, gain_db, ..
                } => Some(format!("eq{:.0}hz_{:+.0}db", freq_hz, gain_db)),
                SpecNode::SingleThresholdCrossing { threshold, .. } => {
                    Some(format!("single_thr{:.3}", threshold))
                }
                SpecNode::DoubleThresholdCrossing { low, high, .. } => {
                    Some(format!("double_{:.3}_{:.3}", low, high))
                }
                _ => None,
            })
            .collect();
        if parts.is_empty() {
            "identity".to_string()
        } else {
            parts.join("_")
        }
    }
}
