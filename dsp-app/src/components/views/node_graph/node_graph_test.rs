#[cfg(test)]
mod tests {
    use super::super::node_graph_state::NodeGraphState;

    #[test]
    fn test_node_graph_default_state() {
        let state = NodeGraphState::default();
        assert!(state.snarl.nodes().next().is_none());
    }
}
