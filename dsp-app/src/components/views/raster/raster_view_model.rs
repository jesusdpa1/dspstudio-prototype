use egui::Color32;
use crate::core::session::{WorkspaceState, XAxisMode};
use super::raster_state::{RasterState, RasterStatus, RasterTrackData, RasterRowData, RasterLabelData};

pub struct RasterViewModel;

const RASTER_PALETTE: &[egui::Color32] = &[
    egui::Color32::from_rgb(255, 100, 100),
    egui::Color32::from_rgb(80,  200, 120),
    egui::Color32::from_rgb(100, 160, 255),
    egui::Color32::from_rgb(255, 200,  50),
    egui::Color32::from_rgb(220, 100, 220),
];

impl RasterViewModel {
    pub fn prepare_state(workspace: &WorkspaceState, dataset_id: &Option<String>) -> RasterState {
        let id = match dataset_id {
            Some(id) => id,
            None => return RasterState { status: RasterStatus::NoDatasetSelected, ..Default::default() },
        };

        let session = match workspace.recordings.get(id) {
            Some(s) => s,
            None => return RasterState { status: RasterStatus::DatasetNotFound, ..Default::default() },
        };

        let (x_min, x_max) = crate::core::session::current_x_range(
            workspace.view_start, workspace.view_width, workspace.x_axis_mode, session.meta.sample_rate,
        );
        let hz = session.meta.sample_rate as f64;

        let mut tracks_data = Vec::new();
        for track in session.meta.event_tracks() {
            if !session.event_track_visible.get(&track.name).copied().unwrap_or(true) {
                continue;
            }

            let n_channels = track.channel_indices.len();
            if n_channels == 0 {
                continue;
            }

            let mut rows = Vec::with_capacity(n_channels);
            let n_labels = track.label_vocabulary.labels.len().max(1);
            let label_names: Vec<String> = (0..n_labels)
                .map(|i| track.label_vocabulary.labels.get(i).cloned().unwrap_or_else(|| format!("event_{}", i)))
                .collect();

            for &ch_idx in &track.channel_indices {
                let channel_name = session.meta.channel_names.get(ch_idx as usize)
                    .cloned()
                    .unwrap_or_else(|| format!("CH{}", ch_idx));

                let mut labels_data = Vec::new();
                if let Some(events) = session.event_cache.get(&(track.name.clone(), ch_idx)) {
                    let mut label_xs: Vec<Vec<f64>> = vec![Vec::new(); n_labels];
                    for event in events {
                        let lid = event.label_id as usize;
                        if lid < n_labels {
                            let x = match workspace.x_axis_mode {
                                XAxisMode::Samples => event.sample_offset as f64,
                                XAxisMode::Seconds => event.sample_offset as f64 / hz,
                            };
                            label_xs[lid].push(x);
                        }
                    }

                    for (lid, xs) in label_xs.into_iter().enumerate() {
                        if !xs.is_empty() {
                            labels_data.push(RasterLabelData {
                                name: label_names[lid].clone(),
                                x_values: xs,
                                color: RASTER_PALETTE[lid % RASTER_PALETTE.len()],
                            });
                        }
                    }
                }

                rows.push(RasterRowData {
                    channel_name,
                    labels: labels_data,
                });
            }

            tracks_data.push(RasterTrackData {
                name: track.name.clone(),
                rows,
                y_min: -(n_channels as f64 - 0.5),
                y_max: 0.5,
            });
        }

        if tracks_data.is_empty() {
            return RasterState { status: RasterStatus::NoTracks, ..Default::default() };
        }

        RasterState {
            tracks: tracks_data,
            status: RasterStatus::Ready,
            x_min,
            x_max,
            x_label: workspace.x_axis_mode.label().to_string(),
        }
    }
}
