use egui::Ui;
use egui_plot::{Plot, Points, PlotPoints};
use crate::core::session::WorkspaceState;
use crate::core::bridge::IoBridge;
use egui_tiles::TileId;
use super::pca_state::{PcaState, PcaStatus};
use super::pca_view_model::PcaViewModel;

pub struct PcaView;

impl PcaView {
    pub fn new() -> Self {
        Self
    }

    /// Stateful UI entry point
    pub fn show(ui: &mut Ui, _pane_id: TileId, workspace: &mut WorkspaceState, _bridge: &IoBridge, dataset_id: &mut Option<String>) {
        let state = PcaViewModel::prepare_state(workspace, dataset_id);
        Self::ui(ui, state);
    }

    /// Stateless UI component
    pub fn ui(ui: &mut Ui, state: PcaState) {
        match state.status {
            PcaStatus::NoActiveRecording => {
                ui.label("No active recording.");
            }
            PcaStatus::RecordingNotFound => {
                ui.label("Recording not found.");
            }
            PcaStatus::Empty | PcaStatus::Ready => {
                Plot::new(ui.next_auto_id())
                    .data_aspect(1.0)
                    .show(ui, |plot_ui| {
                        if state.status == PcaStatus::Empty {
                            plot_ui.text(egui_plot::Text::new(
                                "Empty", 
                                [0.0, 0.0].into(), 
                                "No data loaded or clusters selected."
                            ));
                            return;
                        }

                        for cluster in state.clusters {
                            let points: PlotPoints = cluster.points.into_iter().collect();
                            
                            plot_ui.points(
                                Points::new(format!("Label {}", cluster.label_id), points)
                                    .color(cluster.color)
                                    .radius(1.5)
                            );
                        }
                    });
            }
        }
    }
}
