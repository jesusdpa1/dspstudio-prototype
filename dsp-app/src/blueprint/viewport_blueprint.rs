use std::collections::BTreeMap;
use crate::blueprint::ids::{PaneId, ContainerId, container_id_to_tile_id};
use crate::blueprint::pane::PaneBlueprint;
use crate::blueprint::container::ContainerBlueprint;
use egui_tiles::Tile;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ViewportBlueprint {
    pub panes: BTreeMap<PaneId, PaneBlueprint>,
    pub containers: BTreeMap<ContainerId, ContainerBlueprint>,
    pub root_container: ContainerId,
    pub tree: egui_tiles::Tree<PaneId>,
    pub maximized: Option<PaneId>,
    pub auto_layout: bool,
}

impl ViewportBlueprint {
    pub fn new(root: ContainerBlueprint) -> Self {
        let root_id = root.id;
        let mut containers = BTreeMap::new();
        let kind = root.container_kind;
        containers.insert(root_id, root);

        let root_tile_id = container_id_to_tile_id(root_id);
        let mut tiles = egui_tiles::Tiles::default();
        let container = match kind {
            egui_tiles::ContainerKind::Tabs => egui_tiles::Container::new_tabs(vec![]),
            egui_tiles::ContainerKind::Horizontal => {
                egui_tiles::Container::new_linear(egui_tiles::LinearDir::Horizontal, vec![])
            }
            egui_tiles::ContainerKind::Vertical => {
                egui_tiles::Container::new_linear(egui_tiles::LinearDir::Vertical, vec![])
            }
            egui_tiles::ContainerKind::Grid => egui_tiles::Container::new_grid(vec![]),
        };
        tiles.insert(root_tile_id, Tile::Container(container));

        Self {
            panes: BTreeMap::new(),
            containers,
            root_container: root_id,
            tree: egui_tiles::Tree::new("viewport_tree", root_tile_id, tiles),
            maximized: None,
            auto_layout: true,
        }
    }

    pub fn from_json(s: &str) -> Option<Self> {
        serde_json::from_str(s).ok()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

pub fn tree_simplification_options() -> egui_tiles::SimplificationOptions {
    egui_tiles::SimplificationOptions {
        prune_empty_tabs: false,
        all_panes_must_have_tabs: true,
        prune_empty_containers: false,
        prune_single_child_tabs: false,
        prune_single_child_containers: false,
        join_nested_linear_containers: true,
    }
}
