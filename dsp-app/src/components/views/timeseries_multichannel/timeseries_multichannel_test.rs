#[cfg(test)]
mod tests {
    use super::super::timeseries_multichannel_view_model::TimeseriesMultichannelViewModel;
    use super::super::timeseries_multichannel_state::TimeseriesStatus;
    use crate::core::session::WorkspaceState;
    use crate::blueprint::pane::YAxisRange;

    #[test]
    fn test_timeseries_no_recordings() {
        let workspace = WorkspaceState::new();
        let state = TimeseriesMultichannelViewModel::prepare_state(&workspace, YAxisRange::Auto);
        assert_eq!(state.status, TimeseriesStatus::NoRecordings);
    }
}
