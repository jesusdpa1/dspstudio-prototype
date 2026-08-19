use crate::components::views::pca::{PcaView, PcaViewModel};
use crate::core::bridge::IoBridge;
use crate::core::session::WorkspaceState;
use egui::Ui;
use egui_tiles::TileId;

use egui::Color32;

#[derive(Debug, Clone, PartialEq, Default)]
pub enum SpikeViewMode {
    #[default]
    Stacked,
    MeanSpread,
}

pub mod sidebar;

pub fn get_cluster_color(index: usize) -> Color32 {
    const PALETTE: &[Color32] = &[
        Color32::from_rgb(100, 200, 255), // Sky Blue
        Color32::from_rgb(255, 150, 50),  // Orange
        Color32::from_rgb(100, 255, 150), // Green
        Color32::from_rgb(255, 100, 150), // Pink
        Color32::from_rgb(200, 100, 255), // Purple
        Color32::from_rgb(255, 255, 100), // Yellow
        Color32::from_rgb(100, 255, 255), // Cyan
        Color32::from_rgb(255, 100, 100), // Red
    ];
    if index == 0 {
        return Color32::GRAY;
    }
    PALETTE[(index - 1) % PALETTE.len()]
}

pub struct SpikeSortingView {
    pub dataset_id: Option<String>,
    pub spike_mode: SpikeViewMode,
    pub show_pca: bool,
    pub show_waveforms: bool,
    pub show_sidebar: bool,
}

impl SpikeSortingView {
    pub fn new() -> Self {
        Self {
            dataset_id: None,
            spike_mode: SpikeViewMode::default(),
            show_pca: true,
            show_waveforms: true,
            show_sidebar: true,
        }
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        _pane_id: TileId,
        workspace: &mut WorkspaceState,
        bridge: &IoBridge,
    ) {
        let active_id = self.dataset_id.as_ref().or(workspace.active_recording_id.as_ref());
        let id = match active_id {
            Some(id) => id.clone(),
            None => {
                ui.label("No active recording.");
                return;
            }
        };

        let session = match workspace.recordings.get_mut(&id) {
            Some(s) => s,
            None => {
                ui.label("Recording not found.");
                return;
            }
        };

        // ── Top Bar Controls (Unified) ──
        ui.horizontal(|ui| {
            ui.style_mut().spacing.item_spacing.x = 12.0;

            ui.horizontal(|ui| {
                ui.label("Layout:");
                ui.toggle_value(&mut self.show_sidebar, "📁 Sidebar");
                ui.toggle_value(&mut self.show_pca, "⬢ PCA");
                ui.toggle_value(&mut self.show_waveforms, "🗠 Waveforms");
            });

            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Waveform Mode:");
                ui.selectable_value(&mut self.spike_mode, SpikeViewMode::Stacked, "Stacked");
                ui.selectable_value(
                    &mut self.spike_mode,
                    SpikeViewMode::MeanSpread,
                    "Mean/Spread",
                );
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("⟲ Reset").on_hover_text("Reset Layout").clicked() {
                    self.show_pca = true;
                    self.show_waveforms = true;
                    self.show_sidebar = true;
                }
            });
        });

        ui.add_space(4.0);
        ui.separator();
        ui.add_space(4.0);

        // ── Main Content Area with Sidebar ──
        if self.show_sidebar {
            egui::Panel::right(ui.make_persistent_id("sorting_sidebar"))
                .resizable(true)
                .default_size(200.0)
                .show_animated_inside(ui, true, |ui| {
                    self::sidebar::show(ui, session, bridge);
                });
        }

        // The remaining area is split or single view
        let ds_id = Some(id);
        if self.show_pca && self.show_waveforms {
            ui.columns(2, |columns| {
                let pca_state = PcaViewModel::prepare_state(workspace, &ds_id);
                PcaView::ui(&mut columns[0], pca_state);
                columns[1].centered_and_justified(|ui| { ui.label("Waveform view not available"); });
            });
        } else if self.show_pca {
            let pca_state = PcaViewModel::prepare_state(workspace, &ds_id);
            PcaView::ui(ui, pca_state);
        } else if self.show_waveforms {
            ui.centered_and_justified(|ui| { ui.label("Waveform view not available"); });
        } else {
            ui.centered_and_justified(|ui| {
                ui.label("Select a view to display (PCA or Waveforms)");
            });
        }
    }
}
