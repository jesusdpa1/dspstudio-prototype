use super::timeseries_multichannel_state::{TimeseriesMultichannelState, TimeseriesStatus};

pub struct TimeseriesMultichannelPreview;

impl TimeseriesMultichannelPreview {
    pub fn mock_state() -> TimeseriesMultichannelState {
        TimeseriesMultichannelState::default()
    }
}
