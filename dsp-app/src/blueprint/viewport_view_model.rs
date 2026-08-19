use crate::blueprint::ids::{
    PaneId, ContainerId, Contents,
    pane_id_to_tile_id, container_id_to_tile_id, contents_to_tile_id,
};
use crate::blueprint::container::ContainerBlueprint;
use crate::blueprint::viewport_command::BlueprintCommand;
use crate::blueprint::ViewportBlueprint;
use egui_tiles::{TileId, Tile};

pub struct ViewportViewModel;

impl ViewportViewModel {
    pub fn apply_commands(viewport: &mut ViewportBlueprint, commands: Vec<BlueprintCommand>) {
        for cmd in commands {
            Self::apply_command(viewport, cmd);
        }
    }

    fn apply_command(viewport: &mut ViewportBlueprint, cmd: BlueprintCommand) {
        match cmd {
            BlueprintCommand::SetTree(tree) => {
                viewport.tree = tree;
            }

            BlueprintCommand::AddPane { pane, parent_container, position_in_parent } => {
                let id = pane.id;
                viewport.panes.insert(id, pane);

                let tile_id = pane_id_to_tile_id(id);
                viewport.tree.tiles.insert(tile_id, Tile::Pane(id));

                let parent_id = parent_container.unwrap_or(viewport.root_container);
                let parent_tile_id = container_id_to_tile_id(parent_id);

                if let Some(Tile::Container(container)) = viewport.tree.tiles.get_mut(parent_tile_id) {
                    container.add_child(tile_id);
                    if let Some(pos) = position_in_parent {
                        viewport.tree.move_tile_to_container(tile_id, parent_tile_id, pos, true);
                    }
                } else {
                    // If parent is missing (root was deleted), wrap this pane in a new root tabs container.
                    let new_root_bp = ContainerBlueprint::new(egui_tiles::ContainerKind::Tabs, vec![Contents::Pane(id)]);
                    let new_root_id = new_root_bp.id;
                    viewport.containers.insert(new_root_id, new_root_bp);

                    let new_root_tile_id = container_id_to_tile_id(new_root_id);
                    let mut container = egui_tiles::Container::new_tabs(vec![tile_id]);
                    viewport.tree.tiles.insert(new_root_tile_id, Tile::Container(container));

                    viewport.tree.root = Some(new_root_tile_id);
                    viewport.root_container = new_root_id;
                }

                if let Some(parent) = viewport.containers.get_mut(&parent_id) {
                    parent.contents.push(Contents::Pane(id));
                }
            }

            BlueprintCommand::AddContainer { container_kind, parent_container } => {
                let container_bp = ContainerBlueprint::new(container_kind, Vec::new());
                let id = container_bp.id;
                viewport.containers.insert(id, container_bp);

                let tile_id = container_id_to_tile_id(id);
                let container = match container_kind {
                    egui_tiles::ContainerKind::Tabs => egui_tiles::Container::new_tabs(vec![]),
                    egui_tiles::ContainerKind::Horizontal => {
                        egui_tiles::Container::new_linear(egui_tiles::LinearDir::Horizontal, vec![])
                    }
                    egui_tiles::ContainerKind::Vertical => {
                        egui_tiles::Container::new_linear(egui_tiles::LinearDir::Vertical, vec![])
                    }
                    egui_tiles::ContainerKind::Grid => egui_tiles::Container::new_grid(vec![]),
                };
                viewport.tree.tiles.insert(tile_id, Tile::Container(container));

                let parent_id = parent_container.unwrap_or(viewport.root_container);
                let parent_tile_id = container_id_to_tile_id(parent_id);

                if let Some(Tile::Container(parent_container)) =
                    viewport.tree.tiles.get_mut(parent_tile_id)
                {
                    parent_container.add_child(tile_id);
                } else {
                    // If parent is missing (root was deleted), this container becomes the new root.
                    viewport.tree.root = Some(tile_id);
                    viewport.root_container = id;
                }

                if let Some(parent) = viewport.containers.get_mut(&parent_id) {
                    parent.contents.push(Contents::Container(id));
                }
            }

            BlueprintCommand::SetContainerKind(id, kind) => {
                if let Some(container) = viewport.containers.get_mut(&id) {
                    container.container_kind = kind;
                }
                let tile_id = container_id_to_tile_id(id);
                if let Some(Tile::Container(container)) = viewport.tree.tiles.get_mut(tile_id) {
                    container.set_kind(kind);
                }
            }

            BlueprintCommand::FocusTab(pane_id) => {
                viewport.tree.make_active(|_, tile| match tile {
                    Tile::Pane(id) => *id == pane_id,
                    Tile::Container(_) => false,
                });
            }

            BlueprintCommand::RemoveContents(contents) => {
                let tile_id = contents_to_tile_id(contents);

                for tile in viewport.tree.remove_recursively(tile_id) {
                    if let Tile::Pane(id) = tile {
                        viewport.panes.remove(&id);
                    }
                    // Orphaned containers are collected below via tree scan.
                }

                // Remove any container whose tile is no longer present in the tree.
                let live_tiles: std::collections::HashSet<TileId> =
                    viewport.tree.tiles.iter().map(|(tid, _)| *tid).collect();
                viewport.containers
                    .retain(|id, _| live_tiles.contains(&container_id_to_tile_id(*id)));

                // Prune stale references in parent content lists.
                let live_panes: std::collections::BTreeSet<PaneId> =
                    viewport.panes.keys().cloned().collect();
                let live_containers: std::collections::BTreeSet<ContainerId> =
                    viewport.containers.keys().cloned().collect();
                for container in viewport.containers.values_mut() {
                    container.contents.retain(|c| match c {
                        Contents::Pane(id)      => live_panes.contains(id),
                        Contents::Container(id) => live_containers.contains(id),
                    });
                }
            }

            BlueprintCommand::RenamePane(id, name) => {
                if let Some(pane) = viewport.panes.get_mut(&id) {
                    pane.display_name = if name.is_empty() { None } else { Some(name) };
                }
            }

            BlueprintCommand::RenameContainer(id, name) => {
                if let Some(container) = viewport.containers.get_mut(&id) {
                    container.display_name = if name.is_empty() { None } else { Some(name) };
                }
            }

            BlueprintCommand::SetPaneVisible(id, visible) => {
                if let Some(pane) = viewport.panes.get_mut(&id) {
                    pane.visible = visible;
                }
                viewport.tree.set_visible(pane_id_to_tile_id(id), visible);
            }

            BlueprintCommand::SimplifyContainer(id, options) => {
                let tile_id = container_id_to_tile_id(id);
                viewport.tree.simplify_children_of_tile(tile_id, &options);
            }

            BlueprintCommand::MakeAllChildrenSameSize(id) => {
                let tile_id = container_id_to_tile_id(id);
                if let Some(Tile::Container(container)) = viewport.tree.tiles.get_mut(tile_id) {
                    if let egui_tiles::Container::Linear(linear) = container {
                        for &child_id in &linear.children {
                            linear.shares[child_id] = 1.0;
                        }
                    }
                }
            }

            BlueprintCommand::MoveContents {
                contents_to_move,
                target_container,
                target_position_in_container,
            } => {
                let target_tile_id = container_id_to_tile_id(target_container);
                for contents in contents_to_move.iter().rev() {
                    let tile_id = contents_to_tile_id(*contents);
                    viewport.tree.move_tile_to_container(
                        tile_id,
                        target_tile_id,
                        target_position_in_container,
                        true,
                    );
                }
            }

            BlueprintCommand::MoveContentsToNewContainer {
                contents_to_move,
                new_container_kind,
                target_container,
                target_position_in_container,
            } => {
                let new_container_bp = ContainerBlueprint::new(new_container_kind, Vec::new());
                let new_id = new_container_bp.id;
                viewport.containers.insert(new_id, new_container_bp);

                let new_tile_id = container_id_to_tile_id(new_id);
                let container = match new_container_kind {
                    egui_tiles::ContainerKind::Tabs => egui_tiles::Container::new_tabs(vec![]),
                    egui_tiles::ContainerKind::Horizontal => {
                        egui_tiles::Container::new_linear(egui_tiles::LinearDir::Horizontal, vec![])
                    }
                    egui_tiles::ContainerKind::Vertical => {
                        egui_tiles::Container::new_linear(egui_tiles::LinearDir::Vertical, vec![])
                    }
                    egui_tiles::ContainerKind::Grid => egui_tiles::Container::new_grid(vec![]),
                };
                viewport.tree.tiles.insert(new_tile_id, Tile::Container(container));

                let target_tile_id = container_id_to_tile_id(target_container);
                viewport.tree.move_tile_to_container(
                    new_tile_id,
                    target_tile_id,
                    target_position_in_container,
                    true,
                );

                for (pos, contents) in contents_to_move.into_iter().enumerate() {
                    viewport.tree
                        .move_tile_to_container(contents_to_tile_id(contents), new_tile_id, pos, true);
                }
            }

            BlueprintCommand::SetMaximized(id) => {
                viewport.maximized = id;
            }

            BlueprintCommand::SetPaneConfig(id, config) => {
                if let Some(pane) = viewport.panes.get_mut(&id) {
                    pane.config = config;
                }
            }

            BlueprintCommand::SetContainerVisible(id, visible) => {
                if let Some(container) = viewport.containers.get_mut(&id) {
                    container.visible = visible;
                }
                viewport.tree.set_visible(container_id_to_tile_id(id), visible);
            }
        }
    }
}
