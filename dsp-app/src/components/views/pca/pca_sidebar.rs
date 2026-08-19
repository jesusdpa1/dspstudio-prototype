use egui::Ui;
use crate::core::session::WorkspaceState;
use super::pca_view_model::PcaViewModel;

pub struct PcaSidebar;

impl PcaSidebar {
    pub fn show(ui: &mut Ui, workspace: &mut WorkspaceState, dataset_id: &mut Option<String>) {
        ui.vertical_centered(|ui| {
            ui.heading("PCA Settings");
        });
        ui.separator();

        let active_id = dataset_id.as_ref().or(workspace.active_recording_id.as_ref()).cloned();
        
        if let Some(id) = active_id {
            if let Some(session) = workspace.recordings.get_mut(&id) {
                ui.label(format!("Track: {}", session.sorting_state.track_name));
                
                ui.add_space(8.0);
                ui.label("Selected Labels:");
                
                // This is a simplified version of what might be in a real sidebar
                let mut labels: Vec<_> = session.sorting_state.selected_labels.iter().cloned().collect();
                labels.sort();
                
                for label in labels {
                    let mut is_selected = true;
                    if ui.checkbox(&mut is_selected, format!("Label {}", label)).changed() {
                        if !is_selected {
                            session.sorting_state.selected_labels.remove(&label);
                        }
                    }
                }
            }
        } else {
            ui.label("No active recording.");
        }
    }
}
