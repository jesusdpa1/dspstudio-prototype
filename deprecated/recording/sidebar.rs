use crate::core::session::{RecordingSession};

/// Renders the channel list sidebar and x-axis mode toggle for a specific dataset.
pub fn show(ui: &mut egui::Ui, session: &mut RecordingSession) -> bool {
    let mut changed = false;

    ui.heading("Channels");
    ui.horizontal(|ui| {
        if ui.button("Select All").clicked() {
            for d in &mut session.display { d.visible = true; }
            for d in &mut session.virtual_display { d.visible = true; }
            changed = true;
        }
        if ui.button("Deselect All").clicked() {
            for d in &mut session.display { d.visible = false; }
            for d in &mut session.virtual_display { d.visible = false; }
            changed = true;
        }
    });
    ui.separator();

    egui::ScrollArea::vertical()
        .max_height(ui.available_height() - 20.0)
        .show(ui, |ui| {
            // ── Physical channels ─────────────────────────────────────────────
            for (i, display) in session.display.iter_mut().enumerate() {
                let name = session
                    .meta
                    .channel_names
                    .get(i)
                    .cloned()
                    .unwrap_or_else(|| format!("CH{}", i));

                ui.horizontal(|ui| {
                    if ui.checkbox(&mut display.visible, "").changed() {
                        changed = true;
                    }
                    
                    let mut color = display.egui_color();
                    if egui::color_picker::color_edit_button_srgba(
                        ui,
                        &mut color,
                        egui::color_picker::Alpha::Opaque,
                    ).changed() {
                        display.color = [color.r(), color.g(), color.b(), color.a()];
                        changed = true;
                    }
                    ui.label(&name);
                });
            }

            // ── Virtual channels ──────────────────────────────────────────────
            if !session.meta.virtual_channels.is_empty() {
                ui.separator();
                ui.label(egui::RichText::new("Processed").weak());
            }
            for (i, vc) in session.meta.virtual_channels.iter().enumerate() {
                let display = match session.virtual_display.get_mut(i) {
                    Some(d) => d,
                    None => continue,
                };
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut display.visible, "").changed() {
                        changed = true;
                    }
                    let mut color = display.egui_color();
                    if egui::color_picker::color_edit_button_srgba(
                        ui,
                        &mut color,
                        egui::color_picker::Alpha::Opaque,
                    ).changed() {
                        display.color = [color.r(), color.g(), color.b(), color.a()];
                        changed = true;
                    }
                    ui.label(&vc.name);
                });
            }

            // ── Event tracks ──────────────────────────────────────────────────
            let event_tracks: Vec<(String, usize)> = session.meta.event_tracks()
                .map(|t| (t.name.clone(), t.label_vocabulary.labels.len()))
                .collect();
            if !event_tracks.is_empty() {
                ui.separator();
                ui.label(egui::RichText::new("Events").weak());
                for (name, label_count) in &event_tracks {
                    let visible = session.event_track_visible.entry(name.clone()).or_insert(true);
                    ui.horizontal(|ui| {
                        if ui.checkbox(visible, "").changed() {
                            changed = true;
                        }
                        ui.label(name.as_str());
                        ui.weak(format!("({} labels)", label_count));
                    });
                }
            }
        });

    changed
}
