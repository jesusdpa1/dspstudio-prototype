use crate::blueprint::ids::{
    Contents, pane_id_to_tile_id, container_id_to_tile_id,
};
use crate::blueprint::pane::{PaneBlueprint, PaneKind};
use crate::blueprint::container::ContainerBlueprint;
use crate::blueprint::viewport_blueprint::ViewportBlueprint;
use dsp_io::recording_meta::RecordingMeta;

pub fn auto_generate(meta: &RecordingMeta) -> ViewportBlueprint {
    let session_id = meta.session_id.clone();
    let mut root_children = Vec::new();
    let mut panes = std::collections::BTreeMap::new();

    let mut trace_pane = PaneBlueprint::new(PaneKind::TraceView);
    trace_pane.dataset_id = Some(session_id.clone());
    trace_pane.display_name = Some(format!("Trace — {}", meta.recording_name));
    let trace_id = trace_pane.id;
    panes.insert(trace_id, trace_pane);
    root_children.push(Contents::Pane(trace_id));

    if meta.event_tracks().count() > 0 {
        let mut raster_pane = PaneBlueprint::new(PaneKind::RasterPlot);
        raster_pane.dataset_id = Some(session_id);
        raster_pane.display_name = Some(format!("Raster — {}", meta.recording_name));
        let raster_id = raster_pane.id;
        panes.insert(raster_id, raster_pane);
        root_children.push(Contents::Pane(raster_id));
    }

    let mut graph_pane = PaneBlueprint::new(PaneKind::NodeGraph);
    graph_pane.display_name = Some("Node Graph".to_string());
    let graph_id = graph_pane.id;
    panes.insert(graph_id, graph_pane);
    root_children.push(Contents::Pane(graph_id));

    let root_container = if root_children.len() > 1 {
        ContainerBlueprint::new(egui_tiles::ContainerKind::Tabs, root_children)
    } else {
        ContainerBlueprint::new(egui_tiles::ContainerKind::Vertical, root_children)
    };

    let mut viewport = ViewportBlueprint::new(root_container);
    viewport.panes = panes;
    viewport.tree = build_tree(&viewport);
    viewport
}

pub fn default_blueprint() -> ViewportBlueprint {
    let mut panes = std::collections::BTreeMap::new();

    let mut trace_pane = PaneBlueprint::new(PaneKind::TraceView);
    trace_pane.display_name = Some("Trace View".to_string());
    let trace_id = trace_pane.id;
    panes.insert(trace_id, trace_pane);

    let mut raster_pane = PaneBlueprint::new(PaneKind::RasterPlot);
    raster_pane.display_name = Some("Events View".to_string());
    let raster_id = raster_pane.id;
    panes.insert(raster_id, raster_pane);

    let horiz_container = ContainerBlueprint::new(
        egui_tiles::ContainerKind::Horizontal,
        vec![Contents::Pane(trace_id), Contents::Pane(raster_id)],
    );
    let horiz_id = horiz_container.id;

    let mut graph_pane = PaneBlueprint::new(PaneKind::NodeGraph);
    graph_pane.display_name = Some("Node Graph".to_string());
    let graph_id = graph_pane.id;
    panes.insert(graph_id, graph_pane);

    let root_container = ContainerBlueprint::new(
        egui_tiles::ContainerKind::Vertical,
        vec![Contents::Container(horiz_id), Contents::Pane(graph_id)],
    );

    let mut viewport = ViewportBlueprint::new(root_container);
    viewport.panes = panes;
    viewport.containers.insert(horiz_id, horiz_container);
    viewport.tree = build_tree(&viewport);
    viewport
}

/// Builds an egui_tiles tree from scratch that matches the blueprint's container hierarchy.
pub fn build_tree(
    viewport: &ViewportBlueprint,
) -> egui_tiles::Tree<crate::blueprint::ids::PaneId> {
    let mut tiles = egui_tiles::Tiles::default();
    let root_tile =
        build_tile_recursive(&mut tiles, viewport, Contents::Container(viewport.root_container));
    egui_tiles::Tree::new("viewport_tree", root_tile, tiles)
}

fn build_tile_recursive(
    tiles: &mut egui_tiles::Tiles<crate::blueprint::ids::PaneId>,
    viewport: &ViewportBlueprint,
    contents: Contents,
) -> egui_tiles::TileId {
    match contents {
        Contents::Pane(id) => {
            let tile_id = pane_id_to_tile_id(id);
            tiles.insert(tile_id, egui_tiles::Tile::Pane(id));
            tile_id
        }
        Contents::Container(id) => {
            let Some(container_bp) = viewport.containers.get(&id) else {
                // This container is missing from the blueprint — skip it.
                // Returning the root tile id is invalid but this branch should
                // only be hit if the blueprint is internally inconsistent.
                return egui_tiles::TileId::from_u64(0);
            };

            let children: Vec<egui_tiles::TileId> = container_bp
                .contents
                .iter()
                .map(|&child| build_tile_recursive(tiles, viewport, child))
                .collect();

            let mut container = match container_bp.container_kind {
                egui_tiles::ContainerKind::Horizontal => {
                    egui_tiles::Container::new_linear(egui_tiles::LinearDir::Horizontal, children.clone())
                }
                egui_tiles::ContainerKind::Vertical => {
                    egui_tiles::Container::new_linear(egui_tiles::LinearDir::Vertical, children.clone())
                }
                egui_tiles::ContainerKind::Tabs => egui_tiles::Container::new_tabs(children),
                egui_tiles::ContainerKind::Grid => egui_tiles::Container::new_grid(children),
            };

            // Restore saved column/row shares.
            if let egui_tiles::Container::Linear(ref mut linear) = container {
                let shares = match container_bp.container_kind {
                    egui_tiles::ContainerKind::Horizontal => &container_bp.col_shares,
                    _ => &container_bp.row_shares,
                };
                for (i, &share) in shares.iter().enumerate() {
                    if i < linear.children.len() {
                        linear.shares[linear.children[i]] = share;
                    }
                }
            }

            let tile_id = container_id_to_tile_id(id);
            tiles.insert(tile_id, egui_tiles::Tile::Container(container));
            tile_id
        }
    }
}
