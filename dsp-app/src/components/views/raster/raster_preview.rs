use egui::Color32;
use super::raster_state::{RasterState, RasterStatus, RasterTrackData, RasterRowData, RasterLabelData};

pub struct RasterPreview;

impl RasterPreview {
    pub fn mock_state() -> RasterState {
        RasterState {
            tracks: vec![
                RasterTrackData {
                    name: "Mock Track".to_string(),
                    rows: vec![
                        RasterRowData {
                            channel_name: "CH0".to_string(),
                            labels: vec![
                                RasterLabelData {
                                    name: "spike".to_string(),
                                    x_values: vec![0.1, 0.5, 0.9],
                                    color: Color32::RED,
                                }
                            ]
                        }
                    ],
                    y_min: -0.5,
                    y_max: 0.5,
                }
            ],
            status: RasterStatus::Ready,
            x_min: 0.0,
            x_max: 1.0,
            x_label: "Time (s)".to_string(),
        }
    }
}
