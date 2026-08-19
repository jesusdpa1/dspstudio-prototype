use super::{DetectedEvent, DetectionDetector};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

/// Per-channel state for streaming double-threshold detection.
///
/// Carries Schmitt/window level and refractory bookkeeping across batch
/// boundaries. Initialize with `DoubleThresholdState::new(n_channels, initial_high)`.
pub struct DoubleThresholdState {
    /// Whether each channel is currently in the "high" (Hysteresis) or
    /// "in-window" (Window) state.
    pub is_high_or_in_window: Vec<bool>,
    /// Absolute sample index of the last event on each channel.
    pub last_event_sample: Vec<Option<u64>>,
}

impl DoubleThresholdState {
    pub fn new(n_channels: usize, initial_high_or_in_window: bool) -> Self {
        Self {
            is_high_or_in_window: vec![initial_high_or_in_window; n_channels],
            last_event_sample: vec![None; n_channels],
        }
    }
}

/// Mode for double-threshold detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DoubleThresholdMode {
    /// Schmitt trigger: triggers when crossing `high` (rising) and resets when crossing `low` (falling).
    Hysteresis,
    /// Triggers when entering and exiting the `(low, high)` range.
    Window,
}

/// Detects events using two thresholds.
pub struct DoubleThresholdDetector {
    pub low: f32,
    pub high: f32,
    pub mode: DoubleThresholdMode,
    pub refractory_samples: usize,
    /// Label for High trigger (Hysteresis) or Enter event (Window).
    pub label_high_enter: u32,
    /// Label for Low trigger (Hysteresis) or Exit event (Window).
    pub label_low_exit: u32,
}

impl DoubleThresholdDetector {
    pub fn new(
        low: f32,
        high: f32,
        mode: DoubleThresholdMode,
        refractory_samples: usize,
        label_high_enter: u32,
        label_low_exit: u32,
    ) -> Self {
        assert!(high > low, "High threshold must be greater than low threshold");
        Self {
            low,
            high,
            mode,
            refractory_samples,
            label_high_enter,
            label_low_exit,
        }
    }
}

impl DoubleThresholdDetector {
    /// Stateful streaming variant: Schmitt/window level and refractory period
    /// persist across batch boundaries. Sequential to allow mutable per-channel
    /// state without locks.
    pub fn detect_stateful(
        &self,
        data: &[f32],
        n_channels: usize,
        start_sample: u64,
        state: &mut DoubleThresholdState,
    ) -> Vec<DetectedEvent> {
        let samples_per_channel = data.len() / n_channels;
        let mut events = Vec::new();

        for c in 0..n_channels {
            let channel_data = &data[c * samples_per_channel..(c + 1) * samples_per_channel];

            match self.mode {
                DoubleThresholdMode::Hysteresis => {
                    for i in 1..samples_per_channel {
                        let abs_sample = start_sample + i as u64;
                        let curr = channel_data[i];
                        let is_refractory = state.last_event_sample[c]
                            .map(|last| abs_sample.saturating_sub(last) < self.refractory_samples as u64)
                            .unwrap_or(false);

                        if state.is_high_or_in_window[c] {
                            if curr < self.low && !is_refractory {
                                events.push(DetectedEvent::new(abs_sample, c as u16, self.label_low_exit));
                                state.is_high_or_in_window[c] = false;
                                state.last_event_sample[c] = Some(abs_sample);
                            }
                        } else if curr > self.high && !is_refractory {
                            events.push(DetectedEvent::new(abs_sample, c as u16, self.label_high_enter));
                            state.is_high_or_in_window[c] = true;
                            state.last_event_sample[c] = Some(abs_sample);
                        }
                    }
                }
                DoubleThresholdMode::Window => {
                    for i in 1..samples_per_channel {
                        let abs_sample = start_sample + i as u64;
                        let curr = channel_data[i];
                        let now_in_window = curr > self.low && curr < self.high;
                        let is_refractory = state.last_event_sample[c]
                            .map(|last| abs_sample.saturating_sub(last) < self.refractory_samples as u64)
                            .unwrap_or(false);

                        if !state.is_high_or_in_window[c] && now_in_window && !is_refractory {
                            events.push(DetectedEvent::new(abs_sample, c as u16, self.label_high_enter));
                            state.is_high_or_in_window[c] = true;
                            state.last_event_sample[c] = Some(abs_sample);
                        } else if state.is_high_or_in_window[c] && !now_in_window && !is_refractory {
                            events.push(DetectedEvent::new(abs_sample, c as u16, self.label_low_exit));
                            state.is_high_or_in_window[c] = false;
                            state.last_event_sample[c] = Some(abs_sample);
                        }
                    }
                }
            }
        }
        events
    }
}

impl DetectionDetector for DoubleThresholdDetector {
    fn detect(
        &self,
        data: &[f32],
        n_channels: usize,
        start_sample: u64,
    ) -> Vec<DetectedEvent> {
        if data.is_empty() || n_channels == 0 {
            return Vec::new();
        }
        let samples_per_channel = data.len() / n_channels;
        if samples_per_channel == 0 {
            return Vec::new();
        }

        (0..n_channels)
            .into_par_iter()
            .flat_map(|c| {
                let mut events = Vec::new();
                let channel_data = &data[c * samples_per_channel..(c + 1) * samples_per_channel];
                let mut last_event_idx: Option<usize> = None;

                match self.mode {
                    DoubleThresholdMode::Hysteresis => {
                        // Without persistent state, estimate the initial level from the first sample.
                        let mut is_high = channel_data[0] > self.high;

                        for i in 1..samples_per_channel {
                            let curr = channel_data[i];
                            let is_refractory = last_event_idx
                                .map(|last| i - last < self.refractory_samples)
                                .unwrap_or(false);

                            if is_high {
                                if curr < self.low && !is_refractory {
                                    events.push(DetectedEvent::new(start_sample + i as u64, c as u16, self.label_low_exit));
                                    is_high = false;
                                    last_event_idx = Some(i);
                                }
                            } else {
                                if curr > self.high && !is_refractory {
                                    events.push(DetectedEvent::new(start_sample + i as u64, c as u16, self.label_high_enter));
                                    is_high = true;
                                    last_event_idx = Some(i);
                                }
                            }
                        }
                    }
                    DoubleThresholdMode::Window => {
                        let first = channel_data[0];
                        let mut in_window = first > self.low && first < self.high;

                        for i in 1..samples_per_channel {
                            let curr = channel_data[i];
                            let now_in_window = curr > self.low && curr < self.high;

                            let is_refractory = last_event_idx
                                .map(|last| i - last < self.refractory_samples)
                                .unwrap_or(false);

                            if !in_window && now_in_window {
                                if !is_refractory {
                                    events.push(DetectedEvent::new(start_sample + i as u64, c as u16, self.label_high_enter));
                                    in_window = true;
                                    last_event_idx = Some(i);
                                }
                            } else if in_window && !now_in_window {
                                if !is_refractory {
                                    events.push(DetectedEvent::new(start_sample + i as u64, c as u16, self.label_low_exit));
                                    in_window = false;
                                    last_event_idx = Some(i);
                                }
                            }
                        }
                    }
                }
                events
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hysteresis() {
        let detector = DoubleThresholdDetector::new(0.2, 0.8, DoubleThresholdMode::Hysteresis, 0, 10, 20);
        let data = vec![0.0, 0.5, 0.9, 0.5, 0.1, 0.5];
        // Transitions: 
        // 0.0 (Low) 
        // 0.9 (High) -> Event 10 at index 2
        // 0.1 (Low)  -> Event 20 at index 4
        let events = detector.detect(&data, 1, 0);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].sample, 2);
        assert_eq!(events[0].label, 10);
        assert_eq!(events[1].sample, 4);
        assert_eq!(events[1].label, 20);
    }

    #[test]
    fn test_window() {
        let detector = DoubleThresholdDetector::new(0.3, 0.7, DoubleThresholdMode::Window, 0, 100, 200);
        let data = vec![0.0, 0.5, 1.0, 0.5, 0.0];
        // 0.0 -> 0.5 (Enter) -> Event 100 at index 1
        // 0.5 -> 1.0 (Exit)  -> Event 200 at index 2
        // 1.0 -> 0.5 (Enter) -> Event 100 at index 3
        // 0.5 -> 0.0 (Exit)  -> Event 200 at index 4
        let events = detector.detect(&data, 1, 0);
        assert_eq!(events.len(), 4);
        assert_eq!(events[0].label, 100);
        assert_eq!(events[1].label, 200);
        assert_eq!(events[2].label, 100);
        assert_eq!(events[3].label, 200);
    }
}
