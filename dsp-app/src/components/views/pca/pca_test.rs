#[cfg(test)]
mod tests {
    use super::super::pca_view_model::PcaViewModel;
    use super::super::pca_state::PcaStatus;
    use crate::core::session::WorkspaceState;

    #[test]
    fn test_empty_workspace_returns_no_active_recording() {
        let workspace = WorkspaceState::new();
        let state = PcaViewModel::prepare_state(&workspace, &None);
        assert_eq!(state.status, PcaStatus::NoActiveRecording);
    }

    // Add more tests as needed for logic verification
}
