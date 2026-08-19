use egui::Color32;
use crate::core::session::WorkspaceState;
use super::waveform_overlay_state::{WaveformOverlayState, WaveformOverlayStatus, WaveformClusterData, WaveformOverlayMode};

pub struct WaveformOverlayViewModel;

impl WaveformOverlayViewModel {
    pub fn prepare_state(workspace: &WorkspaceState, mode: WaveformOverlayMode) -> WaveformOverlayState {
        let active_id = workspace.active_recording_id.as_ref();
        
        let id = match active_id {
            Some(id) => id,
            None => return WaveformOverlayState { status: WaveformOverlayStatus::NoActiveRecording, mode, ..Default::default() },
        };

        let session = match workspace.recordings.get(id) {
            Some(s) => s,
            None => return WaveformOverlayState { status: WaveformOverlayStatus::RecordingNotFound, mode, ..Default::default() },
        };

        let track_name = &session.sorting_state.track_name;
        let mut clusters: Vec<_> = session.cluster_cache.iter()
            .filter(|((t, l), _)| t == track_name && session.sorting_state.selected_labels.contains(l))
            .map(|((_, l), d)| {
                WaveformClusterData {
                    label_id: *l,
                    mean: d.mean_waveform.clone(),
                    std: d.std_waveform.clone(),
                    color: Self::get_label_color(*l as usize),
                }
            })
            .collect();

        if clusters.is_empty() {
            return WaveformOverlayState { clusters, status: WaveformOverlayStatus::Empty, mode };
        }

        clusters.sort_by_key(|c| c.label_id);

        WaveformOverlayState {
            clusters,
            status: WaveformOverlayStatus::Ready,
            mode,
        }
    }

    fn get_label_color(i: usize) -> Color32 {
        let colors = [
            Color32::from_rgb(100, 200, 255),
            Color32::from_rgb(255, 150, 50),
            Color32::from_rgb(100, 255, 150),
            Color32::from_rgb(255, 100, 150),
        ];
        if i == 0 { Color32::GRAY } else { colors[(i-1) % colors.len()] }
    }
}
