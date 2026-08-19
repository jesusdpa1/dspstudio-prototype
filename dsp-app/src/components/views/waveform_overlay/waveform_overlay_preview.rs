use super::waveform_overlay_state::{WaveformOverlayState, WaveformOverlayStatus};

pub struct WaveformOverlayPreview;

impl WaveformOverlayPreview {
    pub fn mock_state() -> WaveformOverlayState {
        WaveformOverlayState::default()
    }
}
