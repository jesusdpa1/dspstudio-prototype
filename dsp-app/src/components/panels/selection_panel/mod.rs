pub mod selection_state;
pub mod selection_view_model;
pub mod selection_view;

pub use selection_state::SelectionState;
pub use selection_view_model::SelectionViewModel;
pub use selection_view::SelectionView;

use egui::Ui;
use crate::core::session::WorkspaceState;
use crate::blueprint::ViewportBlueprint;
use crate::components::panels::blueprint_tree::ItemId;

pub struct SelectionPanel;

impl SelectionPanel {
    pub fn show(
        ui: &mut Ui,
        workspace: &WorkspaceState,
        viewport: &ViewportBlueprint,
        selected: &Option<ItemId>,
    ) {
        let state = SelectionViewModel::prepare_state(workspace, viewport, selected);
        SelectionView::show(ui, &state);
    }
}
