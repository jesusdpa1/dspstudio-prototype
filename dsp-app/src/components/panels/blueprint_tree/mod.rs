pub mod data;
pub mod tree_ui;
pub mod drag_drop;

use egui::{Ui, Widget};
use crate::blueprint::ViewportBlueprint;
use crate::blueprint::viewport_command::BlueprintCommandQueue;
use crate::blueprint::ids::Contents;
use crate::core::session::WorkspaceState;
use std::collections::HashSet;
pub use data::ItemId;

pub struct BlueprintTree {
    pub expanded: HashSet<ItemId>,
    pub selected: Option<ItemId>,
    pub filter: String,
}

impl Default for BlueprintTree {
    fn default() -> Self {
        Self {
            expanded: HashSet::new(),
            selected: None,
            filter: String::new(),
        }
    }
}

impl BlueprintTree {
    pub fn show(
        &mut self,
        ui: &mut Ui,
        workspace: &mut WorkspaceState,
        viewport: &ViewportBlueprint,
        command_queue: &BlueprintCommandQueue,
        add_view_modal: &mut crate::components::add_view_modal::AddViewModal,
    ) {
        ui.vertical(|ui| {
            // Search Bar
            ui.horizontal(|ui| {
                crate::icons::SEARCH.as_image().ui(ui);
                ui.text_edit_singleline(&mut self.filter);
                
                if ui.button("+").on_hover_text("Add View").clicked() {
                    let target = match &self.selected {
                        Some(ItemId::Container(id)) => Some(*id),
                        Some(ItemId::View(pane_id)) => {
                            viewport.containers.iter()
                                .find(|(_, c)| c.contents.contains(&Contents::Pane(*pane_id)))
                                .map(|(id, _)| *id)
                        }
                        Some(ItemId::Header(h)) if h == "blueprint" => Some(viewport.root_container),
                        _ => None,
                    };
                    add_view_modal.target_container = target;
                    add_view_modal.is_open = true;
                }

                if !self.filter.is_empty() {
                    if ui.button("×").clicked() {
                        self.filter.clear();
                    }
                }
            });
            ui.separator();

            // Tree
            tree_ui::TreeUi::show(
                ui,
                workspace,
                viewport,
                command_queue,
                &mut self.expanded,
                &mut self.selected,
                &self.filter,
                add_view_modal,
            );
        });
    }
}
