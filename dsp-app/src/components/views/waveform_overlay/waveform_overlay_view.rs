use egui::Ui;
use egui_plot::{Plot, Line, PlotPoints, Polygon};
use crate::core::session::WorkspaceState;
use crate::core::bridge::IoBridge;
use egui_tiles::TileId;
use super::waveform_overlay_state::{WaveformOverlayState, WaveformOverlayStatus, WaveformOverlayMode};
use super::waveform_overlay_view_model::WaveformOverlayViewModel;

pub struct WaveformOverlayView;

impl WaveformOverlayView {
    pub fn new() -> Self {
        Self
    }

    pub fn show(ui: &mut Ui, _pane_id: TileId, workspace: &mut WorkspaceState, _bridge: &IoBridge, mode: &mut WaveformOverlayMode) {
        let state = WaveformOverlayViewModel::prepare_state(workspace, *mode);
        Self::ui(ui, state, mode);
    }

    pub fn ui(ui: &mut Ui, state: WaveformOverlayState, mode_out: &mut WaveformOverlayMode) {
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt(ui.next_auto_id())
                .selected_text(format!("{:?}", state.mode))
                .show_ui(ui, |ui| {
                    ui.selectable_value(mode_out, WaveformOverlayMode::Stacked, "Stacked");
                    ui.selectable_value(mode_out, WaveformOverlayMode::MeanSpread, "Mean & Spread");
                });
        });

        match state.status {
            WaveformOverlayStatus::NoActiveRecording => {
                ui.label("No active recording.");
            }
            WaveformOverlayStatus::RecordingNotFound => {
                ui.label("Recording not found.");
            }
            WaveformOverlayStatus::Empty | WaveformOverlayStatus::Ready => {
                Plot::new(ui.next_auto_id())
                    .show(ui, |plot_ui| {
                        if state.status == WaveformOverlayStatus::Empty {
                            plot_ui.text(egui_plot::Text::new(
                                "Empty", 
                                [0.0, 0.0].into(), 
                                "No data loaded or clusters selected."
                            ));
                            return;
                        }

                        for cluster in state.clusters {
                            let label_str = format!("Label {}", cluster.label_id);
                            let n_samples = cluster.mean.len();
                            
                            match state.mode {
                                WaveformOverlayMode::Stacked => {
                                    let points: PlotPoints = cluster.mean.iter().enumerate()
                                        .map(|(i, &v)| [i as f64, v as f64])
                                        .collect();
                                    plot_ui.line(Line::new(label_str, points).color(cluster.color));
                                }
                                WaveformOverlayMode::MeanSpread => {
                                    // Render spread as polygon
                                    let mut points = Vec::new();
                                    for i in 0..n_samples {
                                        points.push([i as f64, (cluster.mean[i] + cluster.std[i]) as f64]);
                                    }
                                    for i in (0..n_samples).rev() {
                                        points.push([i as f64, (cluster.mean[i] - cluster.std[i]) as f64]);
                                    }
                                    plot_ui.polygon(
                                        Polygon::new(format!("{} spread", label_str), PlotPoints::from(points))
                                            .fill_color(cluster.color.linear_multiply(0.2))
                                            .stroke(egui::Stroke::NONE)
                                    );
                                    
                                    // Render mean as line
                                    let mean_points: PlotPoints = cluster.mean.iter().enumerate()
                                        .map(|(i, &v)| [i as f64, v as f64])
                                        .collect();
                                    plot_ui.line(Line::new(label_str, mean_points).color(cluster.color).width(2.0));
                                }
                            }
                        }
                    });
            }
        }
    }
}
