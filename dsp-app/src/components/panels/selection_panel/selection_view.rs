use egui::Ui;
use super::selection_state::SelectionState;

pub struct SelectionView;

impl SelectionView {
    pub fn show(ui: &mut Ui, state: &SelectionState) {
        ui.vertical_centered(|ui| {
            ui.heading("Selection");
        });
        ui.separator();

        match state {
            SelectionState::None => {
                ui.label("No selection.");
            }
            SelectionState::Recording { id, name, n_channels, sample_rate } => {
                ui.label(format!("Recording: {}", name));
                ui.label(format!("ID: {}", id));
                ui.label(format!("Channels: {}", n_channels));
                ui.label(format!("Sample Rate: {} Hz", sample_rate));
            }
            SelectionState::View { id: _, name, kind_label } => {
                ui.label(format!("View: {}", name));
                ui.label(format!("Type: {}", kind_label));
                // TODO: Add view-specific configuration controls
            }
            SelectionState::Container { id: _, name, kind_label } => {
                ui.label(format!("Container: {}", name));
                ui.label(format!("Type: {}", kind_label));
            }
            SelectionState::Stream { recording_id, entity_path, label } => {
                ui.label(format!("Stream: {}", label));
                ui.label(format!("Recording: {}", recording_id));
                ui.label(format!("Path: {}", entity_path));
            }
        }
    }
}
