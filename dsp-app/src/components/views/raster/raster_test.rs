#[cfg(test)]
mod tests {
    use super::super::raster_view_model::RasterViewModel;
    use super::super::raster_state::RasterStatus;
    use crate::core::session::WorkspaceState;

    #[test]
    fn test_raster_no_dataset() {
        let workspace = WorkspaceState::new();
        let state = RasterViewModel::prepare_state(&workspace, &None);
        assert_eq!(state.status, RasterStatus::NoDatasetSelected);
    }
}
