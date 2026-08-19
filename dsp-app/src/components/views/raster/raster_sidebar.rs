use egui::Ui;
use crate::core::session::WorkspaceState;

pub struct RasterSidebar;

impl RasterSidebar {
    pub fn show(ui: &mut Ui, workspace: &mut WorkspaceState, dataset_id: &mut Option<String>) {
        ui.vertical_centered(|ui| {
            ui.heading("Raster Settings");
        });
        ui.separator();

        if let Some(id) = dataset_id {
            if let Some(session) = workspace.recordings.get_mut(id) {
                ui.label("Track Visibility");
                ui.separator();
                for (track_name, visible) in &mut session.event_track_visible {
                    ui.checkbox(visible, track_name.as_str());
                }
            }
        } else {
            ui.label("No dataset selected.");
        }
    }
}
