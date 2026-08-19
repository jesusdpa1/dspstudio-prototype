use egui::{Ui, ScrollArea, Widget, Response};
use crate::blueprint::ViewportBlueprint;
use crate::blueprint::viewport_command::{BlueprintCommand, BlueprintCommandQueue};
use crate::blueprint::ids::Contents;
use crate::core::session::WorkspaceState;
use super::data::{BlueprintTreeData, ItemId, TreeItem};
use std::collections::HashSet;

pub struct TreeUi {}

impl TreeUi {
    pub fn show(
        ui: &mut Ui,
        workspace: &mut WorkspaceState,
        viewport: &ViewportBlueprint,
        command_queue: &BlueprintCommandQueue,
        expanded: &mut HashSet<ItemId>,
        selected: &mut Option<ItemId>,
        filter: &str,
        add_view_modal: &mut crate::components::add_view_modal::AddViewModal,
    ) {
        let tree_data = BlueprintTreeData::from_blueprint(workspace, viewport, expanded, filter);

        let row_height = 24.0;
        let total_rows = tree_data.visible_items.len();

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, row_height, total_rows, |ui, row_range| {
                for i in row_range {
                    let item = &tree_data.visible_items[i];
                    Self::render_row(ui, item, viewport, command_queue, expanded, selected, add_view_modal);
                }
            });
    }

    fn render_row(
        ui: &mut Ui,
        item: &TreeItem,
        viewport: &ViewportBlueprint,
        command_queue: &BlueprintCommandQueue,
        expanded: &mut HashSet<ItemId>,
        selected: &mut Option<ItemId>,
        add_view_modal: &mut crate::components::add_view_modal::AddViewModal,
    ) {
        let (rect, response) = ui.allocate_at_least(
            egui::vec2(ui.available_width(), 24.0),
            egui::Sense::click_and_drag(),
        );

        let is_selected = selected.as_ref() == Some(&item.id);
        
        // Highlight background
        let bg_color = if is_selected {
            ui.visuals().selection.bg_fill
        } else if response.hovered() {
            ui.visuals().widgets.hovered.bg_fill.gamma_multiply(0.2)
        } else if matches!(item.id, ItemId::Header(_)) {
            ui.visuals().widgets.noninteractive.bg_fill.gamma_multiply(0.5)
        } else {
            egui::Color32::TRANSPARENT
        };
        ui.painter().rect_filled(rect, 2.0, bg_color);

        // --- Context Menu ---
        response.context_menu(|ui| {
            match &item.id {
                ItemId::Header(h) if h == "blueprint" => {
                    ui.label("Root Layout");
                    ui.separator();
                    ui.menu_button("Set Kind", |ui| {
                        for (_icon, label, kind) in [
                            (&crate::icons::CONTAINER_TABS, "Tabs", egui_tiles::ContainerKind::Tabs),
                            (&crate::icons::CONTAINER_HORIZONTAL, "Horizontal", egui_tiles::ContainerKind::Horizontal),
                            (&crate::icons::CONTAINER_VERTICAL, "Vertical", egui_tiles::ContainerKind::Vertical),
                            (&crate::icons::CONTAINER_GRID, "Grid", egui_tiles::ContainerKind::Grid),
                        ] {
                            if ui.button(label).clicked() {
                                command_queue.push(BlueprintCommand::SetContainerKind(viewport.root_container, kind));
                                ui.close();
                            }
                        }
                    });
                }
                ItemId::View(id) => {
                    ui.label(format!("View: {}", item.label));
                    ui.separator();
                    if ui.button("Remove").clicked() {
                        command_queue.push(BlueprintCommand::RemoveContents(Contents::Pane(*id)));
                        ui.close();
                    }
                }
                ItemId::Container(id) => {
                    ui.label(format!("Container: {}", item.label));
                    ui.separator();
                    
                    ui.menu_button("Set Layout", |ui| {
                        for (_icon, label, kind) in [
                            (&crate::icons::CONTAINER_TABS, "Tabs", egui_tiles::ContainerKind::Tabs),
                            (&crate::icons::CONTAINER_HORIZONTAL, "Horizontal", egui_tiles::ContainerKind::Horizontal),
                            (&crate::icons::CONTAINER_VERTICAL, "Vertical", egui_tiles::ContainerKind::Vertical),
                            (&crate::icons::CONTAINER_GRID, "Grid", egui_tiles::ContainerKind::Grid),
                        ] {
                            if ui.button(label).clicked() {
                                command_queue.push(BlueprintCommand::SetContainerKind(*id, kind));
                                ui.close();
                            }
                        }
                    });

                    if ui.button("Add View…").clicked() {
                        add_view_modal.target_container = Some(*id);
                        add_view_modal.is_open = true;
                        ui.close();
                    }

                    if *id != viewport.root_container {
                        if ui.button("Remove").clicked() {
                            command_queue.push(BlueprintCommand::RemoveContents(Contents::Container(*id)));
                            ui.close();
                        }
                    }
                }
                ItemId::Header(_) => {
                    ui.label(&item.label);
                }
                ItemId::Recording(_) => {
                    ui.label(format!("Recording: {}", item.label));
                }
                ItemId::Stream { .. } => {
                    ui.label(format!("Stream: {}", item.label));
                }
            }
        });

        // --- Drag and Drop ---
        let _dnd_id = ui.id().with("dnd").with(&item.id);
        let can_drag = match &item.id {
            ItemId::View(_) => true,
            ItemId::Container(id) => *id != viewport.root_container,
            _ => false,
        };

        if can_drag {
            if let Some(source) = egui::DragAndDrop::payload::<ItemId>(ui.ctx()) {
                // Drop target detection
                if response.hovered() {
                    let is_valid_drop = match (&*source, &item.id) {
                        (ItemId::View(_), ItemId::Container(_)) => true,
                        (ItemId::Container(src_c), ItemId::Container(dst_c)) => src_c != dst_c,
                        _ => false,
                    };

                    if is_valid_drop {
                        ui.painter().rect_stroke(rect, 2.0, ui.visuals().selection.stroke, egui::StrokeKind::Outside);
                        if ui.input(|i| i.pointer.any_released()) {
                            match (&*source, &item.id) {
                                (ItemId::View(src_p), ItemId::Container(dst_c)) => {
                                    command_queue.push(BlueprintCommand::MoveContents {
                                        contents_to_move: vec![Contents::Pane(*src_p)],
                                        target_container: *dst_c,
                                        target_position_in_container: 0,
                                    });
                                }
                                (ItemId::Container(src_c), ItemId::Container(dst_c)) => {
                                    command_queue.push(BlueprintCommand::MoveContents {
                                        contents_to_move: vec![Contents::Container(*src_c)],
                                        target_container: *dst_c,
                                        target_position_in_container: 0,
                                    });
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }

            // Start drag
            if response.drag_started() {
                egui::DragAndDrop::set_payload(ui.ctx(), item.id.clone());
            }

            if ui.ctx().is_being_dragged(response.id) {
                egui::Area::new(egui::Id::new("dnd_preview"))
                    .pivot(egui::Align2::LEFT_CENTER)
                    .order(egui::Order::Tooltip)
                    .interactable(false)
                    .show(ui.ctx(), |ui| {
                        ui.painter().rect_filled(ui.max_rect(), 2.0, ui.visuals().selection.bg_fill.gamma_multiply(0.5));
                        ui.label(egui::RichText::new(format!("Moving {}", item.label)).strong());
                    });
            }
        }

        let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        ui.horizontal(|ui| {
            ui.add_space(item.depth as f32 * 16.0);

            // Expand/Collapse arrow
            if !item.is_leaf {
                let arrow = if item.is_expanded { "⏷" } else { "⏵" };
                if ui.selectable_label(false, arrow).clicked() {
                    if item.is_expanded {
                        expanded.remove(&item.id);
                    } else {
                        expanded.insert(item.id.clone());
                    }
                }
            } else {
                ui.add_space(12.0);
            }

            // Icon
            let icon = match &item.id {
                ItemId::Header(_) => &crate::icons::DATA_SOURCE, 
                ItemId::Recording(_) => &crate::icons::RECORDING,
                ItemId::View(_) => &crate::icons::VIEW_GENERIC,
                ItemId::Stream { .. } => &crate::icons::ENTITY,
                ItemId::Container(id) => {
                    if let Some(c) = viewport.containers.get(id) {
                        match c.container_kind {
                            egui_tiles::ContainerKind::Tabs => &crate::icons::CONTAINER_TABS,
                            egui_tiles::ContainerKind::Horizontal => &crate::icons::CONTAINER_HORIZONTAL,
                            egui_tiles::ContainerKind::Vertical => &crate::icons::CONTAINER_VERTICAL,
                            egui_tiles::ContainerKind::Grid => &crate::icons::CONTAINER_GRID,
                        }
                    } else {
                        &crate::icons::CONTAINER_HORIZONTAL
                    }
                }
            };
            
            // Special styling and icons for top-level Blueprint/Sources
            let icon = if item.depth == 0 {
                if let ItemId::Container(_) = &item.id { &crate::icons::BLUEPRINT } else { &crate::icons::DATA_SOURCE }
            } else {
                icon
            };
            
            icon.as_image().ui(ui);

            // Label
            let mut label = egui::RichText::new(&item.label);
            if item.depth == 0 {
                label = label.strong().size(13.0);
            }
            ui.label(label);

            // --- Right-aligned buttons ---
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                match &item.id {
                    ItemId::View(id) => {
                        // Remove button
                        if ui.small_button("🗑").clicked() {
                            command_queue.push(BlueprintCommand::RemoveContents(Contents::Pane(*id)));
                        }
                        // Visibility toggle
                        let visible = viewport.panes.get(id).map(|p| p.visible).unwrap_or(true);
                        let icon = if visible { &crate::icons::VISIBLE } else { &crate::icons::INVISIBLE };
                        if icon.as_button().ui(ui).clicked() {
                            command_queue.push(BlueprintCommand::SetPaneVisible(*id, !visible));
                        }
                    }
                    ItemId::Container(id) => {
                        if *id != viewport.root_container {
                            if ui.small_button("🗑").clicked() {
                                command_queue.push(BlueprintCommand::RemoveContents(Contents::Container(*id)));
                            }
                        }
                        // Add button
                        if ui.small_button("+").clicked() {
                            add_view_modal.target_container = Some(*id);
                            add_view_modal.is_open = true;
                        }
                    }
                    ItemId::Recording(_id) => {
                        if ui.small_button("×").on_hover_text("Close Recording").clicked() {
                            // This would need a command to close a recording from the workspace.
                            // For now we just select it.
                        }
                    }
                    _ => {}
                }
            });
        });

        if response.clicked() {
            *selected = Some(item.id.clone());
        }
    }
}
