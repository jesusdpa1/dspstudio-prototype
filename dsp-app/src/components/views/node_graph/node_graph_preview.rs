use super::node_graph_state::{NodeGraphState, NodeGraphStatus};

pub struct NodeGraphPreview;

impl NodeGraphPreview {
    pub fn mock_state() -> NodeGraphState {
        NodeGraphState::default()
    }
}
