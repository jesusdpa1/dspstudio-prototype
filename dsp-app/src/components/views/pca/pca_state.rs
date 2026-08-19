use egui::Color32;

#[derive(Debug, Clone)]
pub struct PcaClusterData {
    pub label_id: u32,
    pub points: Vec<[f64; 2]>,
    pub color: Color32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PcaStatus {
    Ready,
    NoActiveRecording,
    RecordingNotFound,
    Empty,
}

#[derive(Debug, Clone)]
pub struct PcaState {
    pub clusters: Vec<PcaClusterData>,
    pub status: PcaStatus,
}

impl Default for PcaState {
    fn default() -> Self {
        Self {
            clusters: Vec::new(),
            status: PcaStatus::Empty,
        }
    }
}
