use egui::Color32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash, serde::Serialize, serde::Deserialize)]
pub enum WaveformOverlayMode {
    #[default]
    Stacked,
    MeanSpread,
}

#[derive(Debug, Clone)]
pub struct WaveformClusterData {
    pub label_id: u32,
    pub mean: Vec<f32>,
    pub std: Vec<f32>,
    pub color: Color32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WaveformOverlayStatus {
    Ready,
    NoActiveRecording,
    RecordingNotFound,
    Empty,
}

#[derive(Debug, Clone)]
pub struct WaveformOverlayState {
    pub clusters: Vec<WaveformClusterData>,
    pub status: WaveformOverlayStatus,
    pub mode: WaveformOverlayMode,
}

impl Default for WaveformOverlayState {
    fn default() -> Self {
        Self {
            clusters: Vec::new(),
            status: WaveformOverlayStatus::Empty,
            mode: WaveformOverlayMode::Stacked,
        }
    }
}
