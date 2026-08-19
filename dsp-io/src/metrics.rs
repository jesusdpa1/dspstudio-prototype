use once_cell::sync::Lazy;
use parking_lot::RwLock;
use std::collections::VecDeque;
use std::time::Instant;

pub struct GlobalMetrics {
    pub transmission_count: u64,
    pub transmission_bytes: u64,
    pub transmission_duration_ms: u64,
    
    pub processing_count: u64,
    pub processing_duration_ms: u64,
    pub processing_samples: u64,

    /// Recent durations for rolling average (max 100)
    pub recent_latencies_ms: VecDeque<u64>,
}

impl GlobalMetrics {
    fn new() -> Self {
        Self {
            transmission_count: 0,
            transmission_bytes: 0,
            transmission_duration_ms: 0,
            processing_count: 0,
            processing_duration_ms: 0,
            processing_samples: 0,
            recent_latencies_ms: VecDeque::with_capacity(100),
        }
    }
}

pub static METRICS: Lazy<RwLock<GlobalMetrics>> = Lazy::new(|| RwLock::new(GlobalMetrics::new()));

pub fn record_transmission(bytes: u64, duration_ms: u64) {
    let mut m = METRICS.write();
    m.transmission_count += 1;
    m.transmission_bytes += bytes;
    m.transmission_duration_ms += duration_ms;
    
    if m.recent_latencies_ms.len() >= 100 {
        m.recent_latencies_ms.pop_front();
    }
    m.recent_latencies_ms.push_back(duration_ms);
}

pub fn record_processing(samples: u64, duration_ms: u64) {
    let mut m = METRICS.write();
    m.processing_count += 1;
    m.processing_samples += samples;
    m.processing_duration_ms += duration_ms;
}

pub struct Timer {
    start: Instant,
}

impl Timer {
    pub fn new() -> Self {
        Self { start: Instant::now() }
    }
    
    pub fn elapsed_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }
}
