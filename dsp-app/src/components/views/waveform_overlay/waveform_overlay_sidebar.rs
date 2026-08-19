use egui::Ui;
use crate::core::session::WorkspaceState;

pub struct WaveformOverlaySidebar;

impl WaveformOverlaySidebar {
    pub fn show(ui: &mut Ui, workspace: &mut WorkspaceState) {
        ui.vertical_centered(|ui| {
            ui.heading("Waveform Overlay Settings");
        });
        ui.separator();

        let active_id = workspace.active_recording_id.as_ref().cloned();
        
        if let Some(id) = active_id {
            if let Some(session) = workspace.recordings.get_mut(&id) {
                ui.label(format!("Track: {}", session.sorting_state.track_name));
                
                ui.add_space(8.0);
                ui.label("Selected Labels:");
                
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
