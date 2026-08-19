//! Recording metadata viewer / editor.
//!
//! All fields in [`dsp_io::recording_meta::RecordingMeta`] are editable inline.
//! Changes are held in [`SessionState::meta`] (in memory only) until the user
//! clicks **Save**, which sends [`IoRequest::SaveRecordingMeta`] to persist the
//! sidecar JSON to disk.
//!
//! A brief "Saved ✓" confirmation is shown after a successful save using
//! egui's per-widget temporary state storage.

use crate::core::bridge::{IoBridge, IoRequest};
use crate::core::session::SessionState;

const RECORDING_TYPES: &[&str] = &["EMG", "EEG", "ECG", "LFP", "Generic", "Other"];

/// Render the recording info panel.
///
/// Displays and allows editing of all [`dsp_io::recording_meta::RecordingMeta`]
/// fields. A **Save** button at the bottom persists changes to the sidecar JSON.
pub fn show(ui: &mut egui::Ui, session: &mut SessionState, bridge: &IoBridge) {
    // Check for a pending save confirmation message.
    let saved_id = ui.id().with("rec_info_saved");
    let show_saved: bool = ui.data(|d| d.get_temp(saved_id).unwrap_or(false));

    // Poll bridge for MetaSaved so we can clear the confirmation.
    if show_saved {
        // The confirmation is cleared after one "show" cycle by the main app's
        // logic() handler. We just display it here.
    }

    ui.heading("Recording Info");
    ui.separator();

    egui::ScrollArea::vertical().show(ui, |ui| {
        // ── Core fields ──────────────────────────────────────────────────────
        egui::Grid::new("rec_info_grid")
            .num_columns(2)
            .min_col_width(100.0)
            .spacing([8.0, 6.0])
            .show(ui, |ui| {
                ui.label("Name");
                ui.add(
                    egui::TextEdit::singleline(&mut session.meta.recording_name)
                        .desired_width(f32::INFINITY),
                );
                ui.end_row();

                ui.label("Type");
                egui::ComboBox::from_id_salt("rec_type")
                    .selected_text(&session.meta.recording_type)
                    .show_ui(ui, |ui| {
                        for &t in RECORDING_TYPES {
                            ui.selectable_value(
                                &mut session.meta.recording_type,
                                t.to_string(),
                                t,
                            );
                        }
                    });
                ui.end_row();

                ui.label("Sample rate (Hz)");
                ui.add(
                    egui::DragValue::new(&mut session.meta.sample_rate)
                        .range(1.0..=2_000_000.0)
                        .speed(10.0),
                );
                ui.end_row();

                ui.label("Channels");
                ui.label(session.meta.n_channels.to_string());
                ui.end_row();

                ui.label("Total samples");
                ui.label(format!("{}", session.meta.total_samples));
                ui.end_row();

                ui.label("Created");
                let ts = session.meta.created_at.parse::<u64>().ok();
                let created = ts
                    .map(|s| format!("Unix {}", s))
                    .unwrap_or_else(|| session.meta.created_at.clone());
                ui.label(created);
                ui.end_row();

                ui.label("LOD levels");
                let lods = if session.meta.lod_levels_available.is_empty() {
                    "none (raw only)".to_string()
                } else {
                    session
                        .meta
                        .lod_levels_available
                        .iter()
                        .map(|l| l.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                ui.label(lods);
                ui.end_row();
            });

        // ── Description ──────────────────────────────────────────────────────
        ui.separator();
        ui.label("Description");
        ui.add(
            egui::TextEdit::multiline(&mut session.meta.description)
                .desired_rows(3)
                .desired_width(f32::INFINITY),
        );

        // ── Channel names ────────────────────────────────────────────────────
        ui.separator();
        ui.label("Channel names");
        egui::ScrollArea::vertical()
            .id_salt("ch_names_scroll")
            .max_height(180.0)
            .show(ui, |ui| {
                egui::Grid::new("ch_names_grid")
                    .num_columns(2)
                    .min_col_width(40.0)
                    .spacing([4.0, 2.0])
                    .show(ui, |ui| {
                        for (i, name) in session.meta.channel_names.iter_mut().enumerate() {
                            ui.label(format!("CH{}", i));
                            ui.add(
                                egui::TextEdit::singleline(name).desired_width(f32::INFINITY),
                            );
                            ui.end_row();
                        }
                    });
            });

        // ── Save ─────────────────────────────────────────────────────────────
        ui.separator();
        ui.horizontal(|ui| {
            let save_btn = ui.add(
                egui::Button::new("💾 Save")
                    .min_size(egui::vec2(80.0, 0.0)),
            );
            if save_btn.clicked() {
                bridge.send(IoRequest::SaveRecordingMeta {
                    zarr_path: session.zarr_path.clone(),
                    meta: session.meta.clone(),
                });
            }

            if show_saved {
                ui.colored_label(egui::Color32::from_rgb(0, 200, 100), "Saved ✓");
            }
        });
    });
}

/// Called by `App::logic()` when `IoResponse::MetaSaved` arrives.
///
/// Sets a temporary flag in egui storage so the panel shows "Saved ✓" for
/// one frame.
pub fn on_meta_saved(ctx: &egui::Context) {
    // We store the flag keyed on a stable ID; the panel reads it next frame.
    let id = egui::Id::new("rec_info").with("rec_info_saved");
    ctx.data_mut(|d| d.insert_temp(id, true));
}
