//! Deterministic epoch/event generators for ground-truth testing.

use crate::signal::Event;

/// Generates a set of random events based on a probability of occurrence.
///
/// # Arguments
/// * `total_samples` — The duration to cover.
/// * `probability` — Chance of an event occurring at each sample (0.0 to 1.0).
/// * `label_id` — The label to assign to all generated events.
/// * `seed` — PRNG seed for reproducibility.
pub fn generate_random_events(
    total_samples: u64,
    probability: f64,
    label_id: u32,
    seed: u32,
) -> Vec<Event> {
    let mut events = Vec::new();
    let mut state = seed;
    
    let threshold = (probability * u32::MAX as f64) as u32;

    for i in 0..total_samples {
        // Simple Xorshift
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        
        if state < threshold {
            events.push(Event::new(i, label_id));
        }
    }
    events
}

/// Generates a set of periodic events.
///
/// # Arguments
/// * `total_samples` — The duration to cover.
/// * `period_samples` — Number of samples between events.
/// * `label_id` — The label to assign to all generated events.
pub fn generate_periodic_events(
    total_samples: u64,
    period_samples: u64,
    label_id: u32,
) -> Vec<Event> {
    let mut events = Vec::new();
    if period_samples == 0 { return events; }
    
    let mut current = 0;
    while current < total_samples {
        events.push(Event::new(current, label_id));
        current += period_samples;
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn periodic_events_spacing() {
        let evs = generate_periodic_events(100, 10, 1);
        assert_eq!(evs.len(), 10);
        assert_eq!(evs[0].sample_offset, 0);
        assert_eq!(evs[1].sample_offset, 10);
    }

    #[test]
    fn random_events_reproducible() {
        let evs1 = generate_random_events(1000, 0.05, 1, 42);
        let evs2 = generate_random_events(1000, 0.05, 1, 42);
        assert_eq!(evs1, evs2);
    }

    #[test]
    fn random_events_different_seeds() {
        let evs1 = generate_random_events(1000, 0.05, 1, 1);
        let evs2 = generate_random_events(1000, 0.05, 1, 2);
        assert_ne!(evs1, evs2);
    }
}
