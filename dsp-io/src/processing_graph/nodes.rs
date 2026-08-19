use anyhow::Result;
use dsp_core::detection::double::DoubleThresholdDetector;
use dsp_core::detection::single::SingleThresholdDetector;
use dsp_core::detection::DetectionDetector;
use dsp_core::signal::Event;
use super::{ProcessingGraphSpec, SpecNode, ChannelId, SignalValue};

pub fn evaluate_node(
    spec: &ProcessingGraphSpec,
    node_idx: usize,
    signals: &mut std::collections::HashMap<(usize, usize), SignalValue>,
    raw: &std::collections::HashMap<ChannelId, Vec<f32>>,
    window_len: usize,
    compiled_filters: &std::collections::HashMap<usize, dsp_core::filter::FilterDesign>,
    wire_map: &std::collections::HashMap<(usize, usize), (usize, usize)>,
) -> Result<()> {
    let node = &spec.nodes[node_idx];
    match node {
        SpecNode::Channel { id } => {
            let data = raw
                .get(id)
                .cloned()
                .unwrap_or_else(|| vec![0.0; window_len]);
            signals.insert((node_idx, 0), SignalValue::Waveform(data));
        }
        SpecNode::MultiChannel { ids } => {
            for (pin, id) in ids.iter().enumerate() {
                let data = raw
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| vec![0.0; window_len]);
                signals.insert((node_idx, pin), SignalValue::Waveform(data));
            }
        }
        SpecNode::Float { value } => {
            signals.insert(
                (node_idx, 0),
                SignalValue::Waveform(vec![*value; window_len]),
            );
        }
        SpecNode::Bool { value } => {
            let v = if *value { 1.0f32 } else { 0.0f32 };
            signals.insert((node_idx, 0), SignalValue::Waveform(vec![v; window_len]));
        }
        SpecNode::Arithmetic { op } => {
            let a = resolve_waveform_input(signals, node_idx, 0, window_len, wire_map);
            let b = resolve_waveform_input(signals, node_idx, 1, window_len, wire_map);
            signals.insert((node_idx, 0), SignalValue::Waveform(op.apply(&a, &b)));
        }
        SpecNode::SosFilter { sos_rows, filtfilt } => {
            let src = resolve_waveform_input(signals, node_idx, 0, window_len, wire_map);
            let sections = sos_rows
                .iter()
                .map(|&r| dsp_core::filter::iir::SosSection::from_row(r))
                .collect();
            let f = dsp_core::filter::FilterDesign::from_sections(sections);
            signals.insert(
                (node_idx, 0),
                SignalValue::Waveform(f.filter_channels_flat(&src, 1, *filtfilt)),
            );
        }
        SpecNode::SincLowpass {
            cutoff_hz,
            n_taps,
            window,
            center,
        } => {
            let src = resolve_waveform_input(signals, node_idx, 0, window_len, wire_map);
            let out = dsp_core::filter::generate_sinc_coeffs(
                *cutoff_hz,
                spec.sample_rate,
                *n_taps,
                *window,
            )
            .map(|coeffs| {
                dsp_core::filter::filter_channels_fir(&src, 1, &coeffs, *center)
            })?;
            signals.insert((node_idx, 0), SignalValue::Waveform(out));
        }
        SpecNode::MovingAverage { window, center } => {
            let src = resolve_waveform_input(signals, node_idx, 0, window_len, wire_map);
            signals.insert(
                (node_idx, 0),
                SignalValue::Waveform(dsp_core::filter::moving_average(
                    &src, 1, *window, *center,
                )),
            );
        }
        SpecNode::MovingRms { window, center } => {
            let src = resolve_waveform_input(signals, node_idx, 0, window_len, wire_map);
            signals.insert(
                (node_idx, 0),
                SignalValue::Waveform(dsp_core::filter::moving_rms(
                    &src, 1, *window, *center,
                )),
            );
        }
        SpecNode::ExponentialMovingAverage { alpha } => {
            let src = resolve_waveform_input(signals, node_idx, 0, window_len, wire_map);
            signals.insert(
                (node_idx, 0),
                SignalValue::Waveform(dsp_core::filter::exponential_moving_average(
                    &src, 1, *alpha,
                )),
            );
        }
        SpecNode::MedianFilter { window, center } => {
            let src = resolve_waveform_input(signals, node_idx, 0, window_len, wire_map);
            signals.insert(
                (node_idx, 0),
                SignalValue::Waveform(dsp_core::filter::median_filter(
                    &src, 1, *window, *center,
                )),
            );
        }
        SpecNode::Butterworth { filtfilt, .. }
        | SpecNode::ChebyshevI { filtfilt, .. }
        | SpecNode::ChebyshevII { filtfilt, .. }
        | SpecNode::Bessel { filtfilt, .. }
        | SpecNode::Notch { filtfilt, .. } => {
            let src = resolve_waveform_input(signals, node_idx, 0, window_len, wire_map);
            let out = if let Some(f) = compiled_filters.get(&node_idx) {
                if *filtfilt {
                    f.apply_filtfilt_f32(&src)
                } else {
                    f.apply_f32(&src)
                }
            } else {
                src
            };
            signals.insert((node_idx, 0), SignalValue::Waveform(out));
        }
        SpecNode::PeakEq { .. } => {
            let src = resolve_waveform_input(signals, node_idx, 0, window_len, wire_map);
            let out = if let Some(f) = compiled_filters.get(&node_idx) {
                f.apply_f32(&src)
            } else {
                src
            };
            signals.insert((node_idx, 0), SignalValue::Waveform(out));
        }
        SpecNode::SingleThresholdCrossing {
            threshold,
            direction,
            refractory_samples,
            label_pos,
            label_neg,
        } => {
            let src = resolve_waveform_input(signals, node_idx, 0, window_len, wire_map);
            let detector = SingleThresholdDetector::new(
                *threshold,
                *direction,
                *refractory_samples,
                *label_pos,
                *label_neg,
            );
            let evs = detector.detect(&src, 1, 0);
            let mapped_evs = evs
                .into_iter()
                .map(|e| Event::new(e.sample, e.label))
                .collect();
            signals.insert((node_idx, 0), SignalValue::Events(mapped_evs));
        }
        SpecNode::DoubleThresholdCrossing {
            low,
            high,
            mode,
            refractory_samples,
            label_high_enter,
            label_low_exit,
        } => {
            let src = resolve_waveform_input(signals, node_idx, 0, window_len, wire_map);
            let detector = DoubleThresholdDetector::new(
                *low,
                *high,
                *mode,
                *refractory_samples,
                *label_high_enter,
                *label_low_exit,
            );
            let evs = detector.detect(&src, 1, 0);
            let mapped_evs = evs
                .into_iter()
                .map(|e| Event::new(e.sample, e.label))
                .collect();
            signals.insert((node_idx, 0), SignalValue::Events(mapped_evs));
        }
        SpecNode::Output { .. } | SpecNode::Fork { .. } | SpecNode::EventsOutput { .. } => {
            let src = resolve_input_raw(signals, node_idx, 0, window_len, wire_map);
            signals.insert((node_idx, 0), src);
        }
        SpecNode::MultiChannelOutput { names, .. } => {
            for (pin, _) in names.iter().enumerate() {
                let src = resolve_waveform_input(signals, node_idx, pin, window_len, wire_map);
                signals.insert((node_idx, pin), SignalValue::Waveform(src));
            }
        }
    }
    Ok(())
}

fn resolve_waveform_input(
    signals: &std::collections::HashMap<(usize, usize), SignalValue>,
    node_idx: usize,
    pin: usize,
    window_len: usize,
    wire_map: &std::collections::HashMap<(usize, usize), (usize, usize)>,
) -> Vec<f32> {
    wire_map
        .get(&(node_idx, pin))
        .and_then(|&(from_node, from_output)| signals.get(&(from_node, from_output)))
        .and_then(|sv| sv.as_waveform())
        .map(|s| s.to_vec())
        .unwrap_or_else(|| vec![0.0; window_len])
}

fn resolve_input_raw(
    signals: &std::collections::HashMap<(usize, usize), SignalValue>,
    node_idx: usize,
    pin: usize,
    window_len: usize,
    wire_map: &std::collections::HashMap<(usize, usize), (usize, usize)>,
) -> SignalValue {
    wire_map
        .get(&(node_idx, pin))
        .and_then(|&(from_node, from_output)| signals.get(&(from_node, from_output)))
        .cloned()
        .unwrap_or_else(|| SignalValue::Waveform(vec![0.0; window_len]))
}
