use egui::Color32;
use dsp_io::processing_graph::ChannelId;

#[derive(Debug, Clone)]
pub struct StreamRowData {
    pub rec_id: String,
    pub channel_id: ChannelId,
    pub label: String,
    pub color: Color32,
    pub visible: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TimeseriesStatus {
    Ready,
    NoRecordings,
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum TimeseriesFocusMode {
    #[default]
    None,
    Channel { rec_id: String, channel_id: ChannelId },
}

#[derive(Debug, Clone)]
pub struct TimeseriesMultichannelState {
    pub rows: Vec<StreamRowData>,
    pub status: TimeseriesStatus,
    pub y_min: f32,
    pub y_max: f32,
    pub view_start: u64,
    pub view_width: u64,
    pub playhead: Option<u64>,
    pub selection: Option<[u64; 2]>,
    pub focus_mode: TimeseriesFocusMode,
}

impl Default for TimeseriesMultichannelState {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            status: TimeseriesStatus::Empty,
            y_min: -1.0,
            y_max: 1.0,
            view_start: 0,
            view_width: 1000,
            playhead: None,
            selection: None,
            focus_mode: TimeseriesFocusMode::None,
        }
    }
}
