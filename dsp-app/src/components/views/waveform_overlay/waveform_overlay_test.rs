#[cfg(test)]
mod tests {
    use super::super::waveform_overlay_state::WaveformOverlayStatus;
    use super::super::waveform_overlay_view_model::WaveformOverlayViewModel;
    use crate::core::session::WorkspaceState;

    #[test]
    fn test_waveform_overlay_no_active_recording() {
        let workspace = WorkspaceState::new();
        let state = WaveformOverlayViewModel::prepare_state(&workspace, Default::default());
        assert_eq!(state.status, WaveformOverlayStatus::NoActiveRecording);
    }
}
