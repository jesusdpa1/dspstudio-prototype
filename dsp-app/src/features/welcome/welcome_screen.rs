use crate::core::recent::RecentStore;
use egui::{Align, Align2, Color32, Layout, RichText, Ui, vec2};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WelcomeTab {
    Projects,
    Plugins,
    Learn,
}

pub struct WelcomeScreen {
    pub selected_tab: WelcomeTab,
    pub search_query: String,
    pub recent: RecentStore,
}

impl WelcomeScreen {
    pub fn new() -> Self {
        Self {
            selected_tab: WelcomeTab::Projects,
            search_query: String::new(),
            recent: RecentStore::load(),
        }
    }

    pub fn show(&mut self, ui: &mut Ui) -> Option<WelcomeAction> {
        let mut action = None;

        let full_height = ui.available_height();

        ui.horizontal(|ui| {
            ui.set_height(full_height); // 🔥 ensure full height row

            // ── Sidebar ─────────────────────────────────────────────
            ui.vertical(|ui| {
                ui.set_width(200.0);
                ui.set_height(full_height);

                ui.add_space(20.0);
                ui.horizontal(|ui| {
                    ui.add_space(20.0);
                    ui.heading(RichText::new("DSP Studio").strong());
                });

                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    ui.add_space(20.0);
                    ui.label(RichText::new("2026.1.1").small().weak());
                });

                ui.add_space(20.0);

                self.tab_button(ui, WelcomeTab::Projects, "Projects");
                self.tab_button(ui, WelcomeTab::Plugins, "Plugins");
                self.tab_button(ui, WelcomeTab::Learn, "Learn");

                // Push gear to bottom
                ui.add_space(ui.available_height());

                ui.horizontal(|ui| {
                    ui.add_space(20.0);
                    if ui.button("⚙").clicked() {
                        // Settings
                    }
                });

                ui.add_space(20.0);
            });

            // ── Full-height divider (manual) ────────────────────────
            let (rect, _) = ui.allocate_exact_size(vec2(1.0, full_height), egui::Sense::hover());
            ui.painter().rect_filled(
                rect,
                0.0,
                ui.visuals().widgets.noninteractive.bg_stroke.color,
            );

            // ── Main Content ────────────────────────────────────────
            ui.vertical(|ui| {
                ui.set_height(full_height);
                ui.set_width(ui.available_width());

                ui.add_space(20.0);

                match self.selected_tab {
                    WelcomeTab::Projects => {
                        if let Some(a) = self.show_projects_tab(ui) {
                            action = Some(a);
                        }
                    }
                    _ => {
                        ui.centered_and_justified(|ui| {
                            ui.label(format!("{:?} tab is under construction", self.selected_tab));
                        });
                    }
                }
            });
        });

        action
    }

    fn tab_button(&mut self, ui: &mut Ui, tab: WelcomeTab, label: &str) {
        let is_selected = self.selected_tab == tab;

        ui.horizontal(|ui| {
            ui.add_space(10.0);
            let btn_width = 180.0;

            let response = ui
                .allocate_ui_with_layout(
                    vec2(btn_width, 32.0),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        let (rect, response) =
                            ui.allocate_exact_size(vec2(btn_width, 32.0), egui::Sense::click());

                        if ui.is_rect_visible(rect) {
                            let bg_color = if is_selected {
                                ui.visuals().selection.bg_fill
                            } else if response.hovered() {
                                ui.visuals().widgets.hovered.bg_fill
                            } else {
                                Color32::TRANSPARENT
                            };

                            if bg_color != Color32::TRANSPARENT {
                                ui.painter().rect_filled(rect, 4.0, bg_color);
                            }

                            let text_color = if is_selected {
                                ui.visuals().selection.stroke.color
                            } else {
                                ui.visuals().widgets.inactive.text_color()
                            };

                            ui.painter().text(
                                rect.left_center() + vec2(15.0, 0.0),
                                Align2::LEFT_CENTER,
                                label,
                                egui::FontId::proportional(14.0),
                                text_color,
                            );
                        }

                        response
                    },
                )
                .inner;

            if response.clicked() {
                self.selected_tab = tab;
            }
        });
    }

    fn show_projects_tab(&mut self, ui: &mut Ui) -> Option<WelcomeAction> {
        let mut action = None;

        ui.horizontal(|ui| {
            ui.add_space(20.0);

            let search_resp = ui.add(
                egui::TextEdit::singleline(&mut self.search_query)
                    .hint_text("Search projects")
                    .desired_width(400.0),
            );

            if search_resp.changed() {}

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(20.0);

                if ui.button("Open").clicked() {
                    action = Some(WelcomeAction::OpenFolder);
                }

                if ui.button(RichText::new("New Project").strong()).clicked() {}
            });
        });

        ui.add_space(20.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(20.0);

                let filtered: Vec<_> = self
                    .recent
                    .recordings
                    .iter()
                    .filter(|r| {
                        self.search_query.is_empty()
                            || r.name
                                .to_lowercase()
                                .contains(&self.search_query.to_lowercase())
                            || r.path
                                .to_string_lossy()
                                .to_lowercase()
                                .contains(&self.search_query.to_lowercase())
                    })
                    .collect();

                if filtered.is_empty() {
                    ui.add_space(50.0);
                    ui.weak("No recent recordings found.");
                } else {
                    for rec in filtered {
                        if let Some(a) = self.show_recent_item(ui, rec) {
                            action = Some(a);
                        }
                        ui.add_space(5.0);
                    }
                }
            });

        action
    }

    fn show_recent_item(
        &self,
        ui: &mut Ui,
        rec: &crate::core::recent::RecentRecording,
    ) -> Option<WelcomeAction> {
        let mut action = None;

        let path_exists = rec.path.exists();
        let bg_fill = ui.visuals().widgets.inactive.bg_fill;

        egui::Frame::default()
            .fill(bg_fill)
            .corner_radius(4.0)
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());

                ui.horizontal(|ui| {
                    let (icon_rect, _) =
                        ui.allocate_exact_size(vec2(32.0, 32.0), egui::Sense::hover());
                    let icon_color = if path_exists {
                        Color32::from_rgb(60, 120, 200)
                    } else {
                        ui.visuals().widgets.noninteractive.bg_stroke.color
                    };
                    ui.painter().rect_filled(icon_rect, 4.0, icon_color);
                    ui.painter().text(
                        icon_rect.center(),
                        Align2::CENTER_CENTER,
                        "DS",
                        egui::FontId::proportional(14.0),
                        Color32::WHITE,
                    );

                    ui.add_space(8.0);

                    // Reserve a background shape slot BEFORE text is drawn so the
                    // hover highlight appears underneath the text, not on top of it.
                    let bg_slot = ui.painter().add(egui::Shape::Noop);

                    let text_resp = ui
                        .vertical(|ui| {
                            let name_text = if path_exists {
                                RichText::new(&rec.name).strong().size(14.0)
                            } else {
                                RichText::new(&rec.name).strong().size(14.0).weak()
                            };
                            ui.label(name_text);
                            let path_label = if path_exists {
                                RichText::new(rec.path.to_string_lossy()).small().weak()
                            } else {
                                RichText::new(format!("{} (not found)", rec.path.to_string_lossy()))
                                    .small()
                                    .color(ui.visuals().error_fg_color)
                            };
                            ui.label(path_label);
                        })
                        .response;

                    let mut text_rect = text_resp.rect;
                    text_rect.max.x = ui.available_rect_before_wrap().max.x - 40.0;

                    let click_resp =
                        ui.interact(text_rect, ui.id().with(&rec.path), egui::Sense::click());

                    if click_resp.hovered() {
                        // Fill the pre-reserved slot; this paints below the text.
                        ui.painter().set(
                            bg_slot,
                            egui::Shape::rect_filled(
                                text_rect,
                                4.0,
                                ui.visuals().widgets.hovered.bg_fill.linear_multiply(0.3),
                            ),
                        );
                    }

                    if click_resp.clicked() && path_exists {
                        action = Some(WelcomeAction::OpenFile(rec.path.clone()));
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("🗑").clicked() {
                            action = Some(WelcomeAction::RemoveFile(rec.path.clone()));
                        }
                    });
                });
            });

        action
    }
}

pub enum WelcomeAction {
    OpenFolder,
    OpenFile(PathBuf),
    RemoveFile(PathBuf),
}
