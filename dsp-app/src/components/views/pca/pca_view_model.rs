use egui::Color32;
use crate::core::session::WorkspaceState;
use super::pca_state::{PcaState, PcaStatus, PcaClusterData};

pub struct PcaViewModel;

impl PcaViewModel {
    pub fn prepare_state(workspace: &WorkspaceState, dataset_id: &Option<String>) -> PcaState {
        let active_id = dataset_id.as_ref().or(workspace.active_recording_id.as_ref());
        
        let id = match active_id {
            Some(id) => id,
            None => return PcaState { clusters: vec![], status: PcaStatus::NoActiveRecording },
        };

        let session = match workspace.recordings.get(id) {
            Some(s) => s,
            None => return PcaState { clusters: vec![], status: PcaStatus::RecordingNotFound },
        };

        let track_name = &session.sorting_state.track_name;
        let mut clusters: Vec<_> = session.cluster_cache.iter()
            .filter(|((t, l), _)| t == track_name && session.sorting_state.selected_labels.contains(l))
            .map(|((_, l), d)| {
                let points = d.pca_pc1.iter().zip(d.pca_pc2.iter())
                    .map(|(&x, &y)| [x as f64, y as f64])
                    .collect();
                
                PcaClusterData {
                    label_id: *l,
                    points,
                    color: Self::get_label_color(*l as usize),
                }
            })
            .collect();

        if clusters.is_empty() {
            return PcaState { clusters, status: PcaStatus::Empty };
        }

        clusters.sort_by_key(|c| c.label_id);

        PcaState {
            clusters,
            status: PcaStatus::Ready,
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
