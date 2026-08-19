use egui::Color32;

#[derive(Debug, Clone)]
pub struct RasterTrackData {
    pub name: String,
    pub rows: Vec<RasterRowData>,
    pub y_min: f64,
    pub y_max: f64,
}

#[derive(Debug, Clone)]
pub struct RasterRowData {
    pub channel_name: String,
    pub labels: Vec<RasterLabelData>,
}

#[derive(Debug, Clone)]
pub struct RasterLabelData {
    pub name: String,
    pub x_values: Vec<f64>,
    pub color: Color32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RasterStatus {
    Ready,
    NoDatasetSelected,
    DatasetNotFound,
    NoTracks,
}

#[derive(Debug, Clone)]
pub struct RasterState {
    pub tracks: Vec<RasterTrackData>,
    pub status: RasterStatus,
    pub x_min: f64,
    pub x_max: f64,
    pub x_label: String,
}

impl Default for RasterState {
    fn default() -> Self {
        Self {
            tracks: Vec::new(),
            status: RasterStatus::NoDatasetSelected,
            x_min: 0.0,
            x_max: 1.0,
            x_label: "Time".to_string(),
        }
    }
}
