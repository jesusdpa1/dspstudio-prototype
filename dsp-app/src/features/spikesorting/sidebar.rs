use egui::{Ui, RichText};
use crate::core::session::RecordingSession;
use crate::core::bridge::{IoBridge, IoRequest};
use super::get_cluster_color;

pub fn show(ui: &mut Ui, session: &mut RecordingSession, bridge: &IoBridge) {
    let state = &mut session.sorting_state;
    let dataset_id = session.meta.session_id.clone();
    let zarr_path = session.zarr_path.clone();

    ui.vertical(|ui| {
        ui.heading("Spike Sorting Controls");
        ui.add_space(8.0);

        ui.label("Track Name:");
        ui.text_edit_singleline(&mut state.track_name);

        ui.add_space(8.0);
        ui.label("Fetch Settings:");
        ui.horizontal(|ui| {
            ui.label("Limit:");
            ui.add(egui::DragValue::new(&mut state.max_waveforms).range(10..=5000));
        });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Fetch Selected").clicked() {
                for &label in &state.selected_labels {
                    bridge.send(IoRequest::FetchClusterData {
                        dataset_id: dataset_id.clone(),
                        zarr_path: zarr_path.clone(),
                        track_name: state.track_name.clone(),
                        label_id: label,
                        max_waveforms: state.max_waveforms,
                        snippet_before: state.snippet_before,
                        snippet_after: state.snippet_after,
                    });
                }
            }

            if ui.button("Clear Cache").clicked() {
                session.cluster_cache.clear();
            }
        });

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(4.0);
        ui.label("Clusters (Labels):");

        egui::ScrollArea::vertical()
            .id_salt("sorting_labels_scroll")
            .show(ui, |ui| {
                // Assuming labels 1..=16 for now, or based on meta if available
                let n_labels = 16; 
                for l in 1..=n_labels {
                    let is_selected = state.selected_labels.contains(&l);
                    let color = get_cluster_color(l as usize);
                    
                    ui.horizontal(|ui| {
                        let mut check = is_selected;
                        if ui.checkbox(&mut check, "").changed() {
                            if check {
                                state.selected_labels.insert(l);
                            } else {
                                state.selected_labels.remove(&l);
                            }
                        }

                        let label_text = format!("Label {} (CH{})", l, l-1);
                        if ui.selectable_label(is_selected, RichText::new(label_text).color(color)).clicked() {
                            if is_selected {
                                state.selected_labels.remove(&l);
                            } else {
                                state.selected_labels.insert(l);
                            }
                        }
                    });
                }
            });
    });
}
