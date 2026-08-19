use crate::core::session::RecordingSession;

#[derive(Debug, Clone)]
pub struct RecordingInfoState {
    pub recording_name: String,
    pub recording_type: String,
    pub sample_rate: f32,
    pub n_channels: u16,
    pub total_samples: u64,
    pub created_at: String,
    pub lod_levels: Vec<u32>,
    pub description: String,
    pub channel_names: Vec<String>,
    pub is_dirty: bool,
    pub has_active_recording: bool,
}

impl Default for RecordingInfoState {
    fn default() -> Self {
        Self {
            recording_name: String::new(),
            recording_type: String::new(),
            sample_rate: 0.0,
            n_channels: 0,
            total_samples: 0,
            created_at: String::new(),
            lod_levels: Vec::new(),
            description: String::new(),
            channel_names: Vec::new(),
            is_dirty: false,
            has_active_recording: false,
        }
    }
}

impl From<&RecordingSession> for RecordingInfoState {
    fn from(session: &RecordingSession) -> Self {
        Self {
            recording_name: session.meta.recording_name.clone(),
            recording_type: session.meta.recording_type.clone(),
            sample_rate: session.meta.sample_rate,
            n_channels: session.meta.n_channels,
            total_samples: session.meta.total_samples,
            created_at: session.meta.created_at.clone(),
            lod_levels: session.meta.lod_levels_available.iter().map(|&l| l as u32).collect(),
            description: session.meta.description.clone(),
            channel_names: session.meta.channel_names.clone(),
            is_dirty: false,
            has_active_recording: true,
        }
    }
}
