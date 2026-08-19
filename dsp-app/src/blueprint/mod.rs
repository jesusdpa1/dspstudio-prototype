pub mod ids;
pub mod pane;
pub mod container;
pub mod viewport_blueprint;
pub mod viewport_command;
pub mod viewport_view_model;
pub mod auto_layout;

pub use ids::{
    PaneId, ContainerId, Contents,
    pane_id_to_tile_id, container_id_to_tile_id, contents_to_tile_id,
};
pub use pane::{PaneBlueprint, PaneKind, PaneConfig};
pub use viewport_blueprint::{ViewportBlueprint, tree_simplification_options};
pub use viewport_command::{BlueprintCommand, BlueprintCommandQueue};
pub use viewport_view_model::ViewportViewModel;

pub fn viewport_from_json(s: &str) -> Option<ViewportBlueprint> {
    ViewportBlueprint::from_json(s)
}

pub fn viewport_to_json(bp: &ViewportBlueprint) -> String {
    bp.to_json()
}
