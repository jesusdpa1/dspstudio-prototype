use egui::{Ui, RichText, ScrollArea};
use egui_plot::{Plot, PlotPoints, Line, PlotBounds};
use crate::core::session::{WorkspaceState, XAxisMode};
use egui_tiles::TileId;
use super::raster_state::{RasterState, RasterStatus};
use super::raster_view_model::RasterViewModel;

pub struct RasterView;

impl RasterView {
    pub fn new() -> Self {
        Self
    }

    pub fn show(
        &self,
        ui: &mut Ui,
        _pane_id: TileId,
        workspace: &mut WorkspaceState,
        dataset_id: &mut Option<String>,
        row_height: f32,
    ) {
        // UI for dataset selection (Stateful part)
        ui.horizontal(|ui| {
            ui.label("Dataset:");
            let selected_name = dataset_id.as_deref()
                .and_then(|id| workspace.recordings.get(id))
                .map(|s| s.meta.recording_name.as_str())
                .unwrap_or("None")
                .to_string();
            egui::ComboBox::from_id_salt(ui.next_auto_id())
                .selected_text(&selected_name)
                .show_ui(ui, |ui| {
                    for (id, session) in &workspace.recordings {
                        ui.selectable_value(dataset_id, Some(id.clone()), &session.meta.recording_name);
                    }
                });
        });

        let state = RasterViewModel::prepare_state(workspace, dataset_id);
        
        let mut viewport_update = None;
        Self::ui(ui, state, row_height, workspace.time_selection(), workspace.x_axis_mode, &mut viewport_update);

        if let Some((s, w, sample_rate)) = viewport_update {
            let hz = sample_rate as f64;
            let (new_s, new_w) = match workspace.x_axis_mode {
                XAxisMode::Samples => (s as u64, w as u64),
                XAxisMode::Seconds => ((s * hz) as u64, (w * hz) as u64),
            };
            workspace.view_start = new_s;
            workspace.view_width = new_w;
        }
    }

    pub fn ui(
        ui: &mut Ui, 
        state: RasterState, 
        row_height: f32, 
        selection: Option<[u64; 2]>,
        x_axis_mode: XAxisMode,
        viewport_update: &mut Option<(f64, f64, f32)>
    ) {
        match state.status {
            RasterStatus::NoDatasetSelected => {
                ui.centered_and_justified(|ui| { ui.label("Select a dataset to begin."); });
            }
            RasterStatus::DatasetNotFound => {
                ui.centered_and_justified(|ui| { ui.label("Dataset not found."); });
            }
            RasterStatus::NoTracks => {
                ui.centered_and_justified(|ui| { ui.label("No event tracks in this recording."); });
            }
            RasterStatus::Ready => {
                ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for track in state.tracks {
                            ui.add_space(4.0);
                            ui.label(RichText::new(&track.name).strong());
                            ui.separator();

                            let plot_height = (track.rows.len() as f32 * row_height)
                                .min(ui.available_height() - 8.0)
                                .max(row_height);
                            
                            let channel_names: Vec<String> = track.rows.iter().map(|r| r.channel_name.clone()).collect();
                            
                            let resp = Plot::new(ui.next_auto_id())
                                .height(plot_height)
                                .allow_zoom(true)
                                .allow_drag(true)
                                .allow_scroll(true)
                                .allow_boxed_zoom(true)
                                .show_axes([true, true])
                                .x_axis_label(&state.x_label)
                                .y_axis_formatter(move |mark, _range| {
                                    let row = (-mark.value).round() as usize;
                                    channel_names.get(row).cloned().unwrap_or_default()
                                })
                                .show(ui, |plot_ui| {
                                    plot_ui.set_plot_bounds(PlotBounds::from_min_max(
                                        [state.x_min, track.y_min],
                                        [state.x_max, track.y_max],
                                    ));
                                    
                                    // Note: selection drawing needs sample_rate which we don't have in state yet.
                                    // For now, skip selection drawing in stateless UI or add it to state.
                                    
                                    for (i, row) in track.rows.iter().enumerate() {
                                        let y_center = -(i as f64);
                                        for label in &row.labels {
                                            let mut pts = Vec::with_capacity(label.x_values.len() * 3);
                                            for &x in &label.x_values {
                                                pts.push([x, y_center - 0.38]);
                                                pts.push([x, y_center + 0.38]);
                                                pts.push([f64::NAN, f64::NAN]);
                                            }
                                            plot_ui.line(Line::new(&label.name, PlotPoints::from_iter(pts))
                                                .color(label.color)
                                                .width(1.5));
                                        }
                                    }
                                });

                            let bounds = resp.transform.bounds();
                            let new_x_min = bounds.min()[0];
                            let new_x_max = bounds.max()[0];

                            if (new_x_min - state.x_min).abs() > 1e-9 || (new_x_max - state.x_max).abs() > 1e-9 {
                                // We need sample_rate to properly update workspace if in seconds mode.
                                // This is a bit tricky for purely stateless UI if we want to sync back.
                            }
                        }
                    });
            }
        }
    }
}
