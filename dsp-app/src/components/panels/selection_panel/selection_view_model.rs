use crate::core::session::WorkspaceState;
use crate::blueprint::ViewportBlueprint;
use crate::components::panels::blueprint_tree::ItemId;
use super::selection_state::SelectionState;

pub struct SelectionViewModel;

impl SelectionViewModel {
    pub fn prepare_state(
        workspace: &WorkspaceState,
        viewport: &ViewportBlueprint,
        selected: &Option<ItemId>,
    ) -> SelectionState {
        let Some(item_id) = selected else {
            return SelectionState::None;
        };

        match item_id {
            ItemId::Recording(id) => {
                if let Some(session) = workspace.recordings.get(id) {
                    SelectionState::Recording {
                        id: id.clone(),
                        name: session.meta.recording_name.clone(),
                        n_channels: session.meta.n_channels,
                        sample_rate: session.meta.sample_rate,
                    }
                } else {
                    SelectionState::None
                }
            }
            ItemId::View(pane_id) => {
                if let Some(pane) = viewport.panes.get(pane_id) {
                    SelectionState::View {
                        id: *pane_id,
                        name: pane.display_name.clone().unwrap_or_else(|| format!("{:?}", pane.kind)),
                        kind_label: format!("{:?}", pane.kind),
                    }
                } else {
                    SelectionState::None
                }
            }
            ItemId::Container(container_id) => {
                if let Some(container) = viewport.containers.get(container_id) {
                    SelectionState::Container {
                        id: *container_id,
                        name: container.display_name.clone().unwrap_or_else(|| "Container".to_string()),
                        kind_label: "Container".to_string(), // Could be more specific based on egui_tiles::Container
                    }
                } else {
                    SelectionState::None
                }
            }
            ItemId::Stream { recording_id, entity_path } => {
                SelectionState::Stream {
                    recording_id: recording_id.clone(),
                    entity_path: entity_path.clone(),
                    label: entity_path.clone(), // Could be prettified
                }
            }
            ItemId::Header(_) => SelectionState::None,
        }
    }
}
