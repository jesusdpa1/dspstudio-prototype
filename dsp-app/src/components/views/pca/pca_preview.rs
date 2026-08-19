use egui::Color32;
use super::pca_state::{PcaState, PcaStatus, PcaClusterData};

pub struct PcaPreview;

impl PcaPreview {
    pub fn mock_state() -> PcaState {
        PcaState {
            clusters: vec![
                PcaClusterData {
                    label_id: 1,
                    points: vec![[0.0, 0.0], [1.0, 1.0], [2.0, 0.5]],
                    color: Color32::RED,
                },
                PcaClusterData {
                    label_id: 2,
                    points: vec![[-1.0, -1.0], [-2.0, -0.5], [-1.5, -2.0]],
                    color: Color32::BLUE,
                },
            ],
            status: PcaStatus::Ready,
        }
    }

    pub fn empty_state() -> PcaState {
        PcaState {
            clusters: vec![],
            status: PcaStatus::Empty,
        }
    }
}
