use crate::blueprint::ids::{PaneId, ContainerId};
use crate::components::panels::blueprint_tree::ItemId;

#[derive(Debug, Clone)]
pub enum SelectionState {
    None,
    Recording {
        id: String,
        name: String,
        n_channels: u16,
        sample_rate: f32,
    },
    View {
        id: PaneId,
        name: String,
        kind_label: String,
    },
    Container {
        id: ContainerId,
        name: String,
        kind_label: String,
    },
    Stream {
        recording_id: String,
        entity_path: String,
        label: String,
    },
}

impl Default for SelectionState {
    fn default() -> Self {
        Self::None
    }
}
