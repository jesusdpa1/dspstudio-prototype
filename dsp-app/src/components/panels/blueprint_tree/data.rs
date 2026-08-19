use crate::blueprint::ids::{PaneId, ContainerId, Contents, container_id_to_tile_id};
use crate::blueprint::ViewportBlueprint;
use crate::core::session::WorkspaceState;
use egui_tiles::{TileId, Tile};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ItemId {
    Header(String),
    Recording(String),
    Container(ContainerId),
    View(PaneId),
    Stream {
        recording_id: String,
        entity_path: String,
    },
}

#[derive(Debug, Clone)]
pub struct TreeItem {
    pub id: ItemId,
    pub depth: usize,
    pub is_expanded: bool,
    pub is_leaf: bool,
    pub label: String,
}

pub struct BlueprintTreeData {
    pub visible_items: Vec<TreeItem>,
}

impl BlueprintTreeData {
    pub fn from_blueprint(
        workspace: &WorkspaceState,
        viewport: &ViewportBlueprint,
        expanded: &HashSet<ItemId>,
        filter: &str,
    ) -> Self {
        let mut visible_items = Vec::new();
        let filter = filter.to_lowercase();

        // --- SECTION 1: DATA SOURCES ---
        let sources_id = ItemId::Header("sources".to_string());
        let sources_expanded = expanded.contains(&sources_id) || !filter.is_empty();
        visible_items.push(TreeItem {
            id: sources_id,
            depth: 0,
            is_expanded: sources_expanded,
            is_leaf: false,
            label: "Data Sources".to_string(),
        });

        if sources_expanded {
            for (rec_id, session) in &workspace.recordings {
                let rec_label = session.display_name();
                let rec_matches = filter.is_empty() || rec_label.to_lowercase().contains(&filter);
                
                let mut stream_children = Vec::new();
                // EEG channels (Streams)
                for (i, _d) in session.display.iter().enumerate() {
                    let label = format!("Channel {}", i);
                    if filter.is_empty() || label.to_lowercase().contains(&filter) {
                        stream_children.push(TreeItem {
                            id: ItemId::Stream {
                                recording_id: rec_id.clone(),
                                entity_path: format!("ch/phys/{}", i),
                            },
                            depth: 2,
                            is_expanded: false,
                            is_leaf: true,
                            label,
                        });
                    }
                }
                // Virtual channels
                for vc in &session.meta.virtual_channels {
                    if filter.is_empty() || vc.name.to_lowercase().contains(&filter) {
                        stream_children.push(TreeItem {
                            id: ItemId::Stream {
                                recording_id: rec_id.clone(),
                                entity_path: format!("ch/virt/{}", vc.name),
                            },
                            depth: 2,
                            is_expanded: false,
                            is_leaf: true,
                            label: vc.name.clone(),
                        });
                    }
                }

                if rec_matches || !stream_children.is_empty() {
                    let id = ItemId::Recording(rec_id.clone());
                    let is_expanded = expanded.contains(&id) || !filter.is_empty();
                    
                    visible_items.push(TreeItem {
                        id,
                        depth: 1,
                        is_expanded,
                        is_leaf: false,
                        label: rec_label.to_string(),
                    });

                    if is_expanded {
                        visible_items.extend(stream_children);
                    }
                }
            }
        }

        // --- SECTION 2: VIEWPORT LAYOUT ---
        let root_id = viewport.root_container;
        let root_tile_id = container_id_to_tile_id(root_id);
        
        // We represent the root container as the "Blueprint" header row itself.
        let id = ItemId::Container(root_id);
        let is_expanded = expanded.contains(&id) || !filter.is_empty();
        
        visible_items.push(TreeItem {
            id,
            depth: 0,
            is_expanded,
            is_leaf: false,
            label: "Blueprint".to_string(),
        });

        if is_expanded {
            if let Some(Tile::Container(container)) = viewport.tree.tiles.get(root_tile_id) {
                for &child_id in container.children() {
                    Self::traverse_tile(
                        child_id,
                        viewport,
                        1, // Start children at depth 1
                        expanded,
                        &filter,
                        &mut visible_items
                    );
                }
            } else {
                // Root is a Pane, show it as the only child
                Self::traverse_tile(
                    root_tile_id,
                    viewport,
                    1,
                    expanded,
                    &filter,
                    &mut visible_items
                );
            }
        }
        
        Self { visible_items }
    }

    fn traverse_tile(
        tile_id: TileId,
        viewport: &ViewportBlueprint,
        depth: usize,
        expanded: &HashSet<ItemId>,
        filter: &str,
        out: &mut Vec<TreeItem>,
    ) -> bool {
        let Some(tile) = viewport.tree.tiles.get(tile_id) else { return false };

        match tile {
            Tile::Pane(pane_id) => {
                let Some(pane) = viewport.panes.get(pane_id) else { return false };
                let label = pane.display_name.clone().unwrap_or_else(|| format!("{:?}", pane.kind));
                
                if filter.is_empty() || label.to_lowercase().contains(filter) {
                    out.push(TreeItem {
                        id: ItemId::View(*pane_id),
                        depth,
                        is_expanded: false,
                        is_leaf: true,
                        label,
                    });
                    true
                } else {
                    false
                }
            }
            Tile::Container(container) => {
                // Find our ContainerId by matching tile_id
                let container_id = viewport.containers.iter()
                    .find(|(id, _)| container_id_to_tile_id(**id) == tile_id)
                    .map(|(id, _)| *id);
                
                let Some(cid) = container_id else { return false };
                let container_bp = &viewport.containers[&cid];
                let label = container_bp.display_name.clone().unwrap_or_else(|| format!("{:?}", container.kind()));
                
                let id = ItemId::Container(cid);
                let is_expanded = expanded.contains(&id) || !filter.is_empty();
                
                let mut children_out = Vec::new();
                let mut any_child_visible = false;
                
                for &child_id in container.children() {
                    if Self::traverse_tile(child_id, viewport, depth + 1, expanded, filter, &mut children_out) {
                        any_child_visible = true;
                    }
                }

                if any_child_visible || filter.is_empty() || label.to_lowercase().contains(filter) {
                    out.push(TreeItem {
                        id,
                        depth,
                        is_expanded,
                        is_leaf: false,
                        label,
                    });
                    if is_expanded {
                        out.extend(children_out);
                    }
                    true
                } else {
                    false
                }
            }
        }
    }
}
