use super::{DetectedEvent, DetectionDetector, CrossingDirection};
use rayon::prelude::*;

/// Per-channel state for streaming single-threshold detection.
///
/// Pass to `detect_stateful` to maintain refractory tracking across batch
/// boundaries. Initialize with `SingleThresholdState::new(n_channels)`.
pub struct SingleThresholdState {
    /// Absolute sample index of the last event on each channel.
    pub last_event_sample: Vec<Option<u64>>,
}

impl SingleThresholdState {
    pub fn new(n_channels: usize) -> Self {
        Self { last_event_sample: vec![None; n_channels] }
    }
}

/// Detects when a signal crosses a single fixed threshold.
pub struct SingleThresholdDetector {
    pub threshold: f32,
    pub direction: CrossingDirection,
    pub refractory_samples: usize,
    /// Label assigned to positive (rising) crossings.
    pub label_pos: u32,
    /// Label assigned to negative (falling) crossings.
    pub label_neg: u32,
}

impl SingleThresholdDetector {
    pub fn new(
        threshold: f32,
        direction: CrossingDirection,
        refractory_samples: usize,
        label_pos: u32,
        label_neg: u32,
    ) -> Self {
        Self {
            threshold,
            direction,
            refractory_samples,
            label_pos,
            label_neg,
        }
    }
}

impl SingleThresholdDetector {
    /// Stateful streaming variant: refractory period tracks absolute sample
    /// indices across batch boundaries. Sequential (no Rayon) to allow mutable
    /// state per channel without a lock.
    pub fn detect_stateful(
        &self,
        data: &[f32],
        n_channels: usize,
        start_sample: u64,
        state: &mut SingleThresholdState,
    ) -> Vec<DetectedEvent> {
        let samples_per_channel = data.len() / n_channels;
        let mut events = Vec::new();

        for c in 0..n_channels {
            let channel_data = &data[c * samples_per_channel..(c + 1) * samples_per_channel];
            for i in 1..samples_per_channel {
                let abs_sample = start_sample + i as u64;
                let prev = channel_data[i - 1];
                let curr = channel_data[i];

                let is_refractory = state.last_event_sample[c]
                    .map(|last| abs_sample.saturating_sub(last) < self.refractory_samples as u64)
                    .unwrap_or(false);

                if is_refractory {
                    continue;
                }

                if (self.direction == CrossingDirection::Positive || self.direction == CrossingDirection::Both)
                    && prev <= self.threshold && curr > self.threshold
                {
                    events.push(DetectedEvent::new(abs_sample, c as u16, self.label_pos));
                    state.last_event_sample[c] = Some(abs_sample);
                } else if (self.direction == CrossingDirection::Negative || self.direction == CrossingDirection::Both)
                    && prev >= self.threshold && curr < self.threshold
                {
                    events.push(DetectedEvent::new(abs_sample, c as u16, self.label_neg));
                    state.last_event_sample[c] = Some(abs_sample);
                }
            }
        }
        events
    }
}

impl DetectionDetector for SingleThresholdDetector {
    fn detect(
        &self,
        data: &[f32],
        n_channels: usize,
        start_sample: u64,
    ) -> Vec<DetectedEvent> {
        let samples_per_channel = data.len() / n_channels;

        (0..n_channels)
            .into_par_iter()
            .flat_map(|c| {
                let mut events = Vec::new();
                let start = c * samples_per_channel;
                let end = start + samples_per_channel;
                let channel_data = &data[start..end];
                let mut last_event_idx: Option<usize> = None;

                for i in 1..samples_per_channel {
                    let abs_sample = start_sample + i as u64;
                    let prev = channel_data[i - 1];
                    let curr = channel_data[i];

                    let is_refractory = last_event_idx
                        .map(|last| i - last < self.refractory_samples)
                        .unwrap_or(false);

                    if is_refractory {
                        continue;
                    }

                    // Positive crossing (Rising)
                    if (self.direction == CrossingDirection::Positive || self.direction == CrossingDirection::Both)
                        && prev <= self.threshold && curr > self.threshold
                    {
                        events.push(DetectedEvent::new(
                            abs_sample,
                            c as u16,
                            self.label_pos,
                        ));
                        last_event_idx = Some(i);
                    }
                    // Negative crossing (Falling)
                    else if (self.direction == CrossingDirection::Negative || self.direction == CrossingDirection::Both)
                        && prev >= self.threshold && curr < self.threshold
                    {
                        events.push(DetectedEvent::new(
                            abs_sample,
                            c as u16,
                            self.label_neg,
                        ));
                        last_event_idx = Some(i);
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
    fn test_single_crossing_positive() {
        let detector = SingleThresholdDetector::new(0.5, CrossingDirection::Positive, 0, 1, 2);
        let data = vec![0.0, 0.4, 0.6, 0.7, 0.4, 0.6];
        // Crossings at index 2 and index 5.
        let events = detector.detect(&data, 1, 100);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].sample, 102);
        assert_eq!(events[0].label, 1);
        assert_eq!(events[1].sample, 105);
    }

    #[test]
    fn test_multichannel_detection() {
        let detector = SingleThresholdDetector::new(0.5, CrossingDirection::Positive, 0, 1, 2);
        // 2 channels, 5 samples each
        // ch0: [0, 1, 0, 1, 0] -> positive crossings at index 1 and 3
        // ch1: [0, 0, 0, 1, 1] -> positive crossing at index 3
        let data = vec![
            0.0, 1.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0, 1.0,
        ];
        let events = detector.detect(&data, 2, 0);
        assert_eq!(events.len(), 3);
        
        let ch0_events: Vec<_> = events.iter().filter(|e| e.channel == 0).collect();
        let ch1_events: Vec<_> = events.iter().filter(|e| e.channel == 1).collect();
        
        assert_eq!(ch0_events.len(), 2);
        assert_eq!(ch1_events.len(), 1);
        assert_eq!(ch1_events[0].sample, 3);
    }
}
