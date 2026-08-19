use crate::core::session::WorkspaceState;
use crate::core::bridge::{IoBridge, IoRequest};
use super::recording_info_state::RecordingInfoState;

pub struct RecordingInfoViewModel;

impl RecordingInfoViewModel {
    pub fn prepare_state(workspace: &WorkspaceState, dataset_id: &Option<String>) -> RecordingInfoState {
        let id = dataset_id.as_ref().or(workspace.active_recording_id.as_ref());
        
        if let Some(id) = id {
            if let Some(session) = workspace.recordings.get(id) {
                return RecordingInfoState::from(session);
            }
        }
        
        RecordingInfoState::default()
    }

    pub fn save_meta(workspace: &mut WorkspaceState, dataset_id: &Option<String>, bridge: &IoBridge) {
        let id = dataset_id.as_ref().or(workspace.active_recording_id.as_ref());
        
        if let Some(id) = id {
            if let Some(session) = workspace.recordings.get(id) {
                bridge.send(IoRequest::SaveRecordingMeta {
                    zarr_path: session.zarr_path.clone(),
                    meta: session.meta.clone(),
                });
            }
        }
    }
}
