use egui::{Ui, Grid, ScrollArea, TextEdit, DragValue, ComboBox, vec2};
use crate::core::bridge::IoBridge;
use crate::core::session::WorkspaceState;
use super::recording_info_view_model::RecordingInfoViewModel;

pub struct RecordingInfoView;

const RECORDING_TYPES: &[&str] = &["EMG", "EEG", "ECG", "LFP", "Generic", "Other"];

impl RecordingInfoView {
    pub fn new() -> Self {
        Self
    }

    pub fn on_meta_saved(ctx: &egui::Context) {
        let id = egui::Id::new("rec_info").with("rec_info_saved");
        ctx.data_mut(|d| d.insert_temp(id, std::time::Instant::now()));
    }

    pub fn show(ui: &mut Ui, workspace: &mut WorkspaceState, bridge: &IoBridge, dataset_id: &Option<String>) {
        let id = dataset_id.as_ref().or(workspace.active_recording_id.as_ref()).cloned();

        let Some(id) = id else {
            ui.centered_and_justified(|ui| { ui.label("No active recording."); });
            return;
        };

        if !workspace.recordings.contains_key(&id) {
            ui.centered_and_justified(|ui| { ui.label("No active recording."); });
            return;
        }

        ui.heading("Recording Info");
        ui.separator();

        let mut save_requested = false;

        ScrollArea::vertical().show(ui, |ui| {
            let session = workspace.recordings.get_mut(&id).unwrap();

            Grid::new("rec_info_grid")
                .num_columns(2)
                .min_col_width(100.0)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Name");
                    ui.add(TextEdit::singleline(&mut session.meta.recording_name).desired_width(f32::INFINITY));
                    ui.end_row();

                    ui.label("Type");
                    ComboBox::from_id_salt("rec_type")
                        .selected_text(&session.meta.recording_type)
                        .show_ui(ui, |ui| {
                            for &t in RECORDING_TYPES {
                                ui.selectable_value(&mut session.meta.recording_type, t.to_string(), t);
                            }
                        });
                    ui.end_row();

                    ui.label("Sample rate (Hz)");
                    ui.add(DragValue::new(&mut session.meta.sample_rate).range(1.0..=2_000_000.0).speed(10.0));
                    ui.end_row();

                    ui.label("Channels");
                    ui.label(session.meta.n_channels.to_string());
                    ui.end_row();

                    ui.label("Total samples");
                    ui.label(format!("{}", session.meta.total_samples));
                    ui.end_row();
                });

            ui.separator();
            ui.label("Description");
            ui.add(TextEdit::multiline(&mut session.meta.description).desired_rows(3).desired_width(f32::INFINITY));

            ui.separator();
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new("💾 Save").min_size(vec2(80.0, 0.0))).clicked() {
                    save_requested = true;
                }
            });
        });

        if save_requested {
            RecordingInfoViewModel::save_meta(workspace, dataset_id, bridge);
        }
    }
}
