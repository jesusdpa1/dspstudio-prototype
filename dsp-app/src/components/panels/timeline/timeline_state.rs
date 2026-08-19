use egui::Color32;
use dsp_io::transmission::ui::ViewResponse;

#[derive(Debug, Clone, PartialEq)]
pub enum RowKind {
    RecordingHeader { rec_id: String },
    AnnotationsHeader { rec_id: String },
    StreamsGroup { rec_id: String },
    EpochGroup { rec_id: String, track_name: String },
    Annotation { rec_id: String, anno_id: u64 },
}

impl RowKind {
    pub fn track_key(&self) -> String {
        match self {
            Self::RecordingHeader { rec_id }        => format!("rec::{rec_id}"),
            Self::AnnotationsHeader { rec_id }      => format!("annohdr::{rec_id}"),
            Self::StreamsGroup { rec_id }           => format!("streams::{rec_id}"),
            Self::EpochGroup { rec_id, track_name } => format!("epoch::{rec_id}::{track_name}"),
            Self::Annotation { rec_id, anno_id }    => format!("anno::{rec_id}::{anno_id}"),
        }
    }
}

pub struct TimelineRowState {
    pub kind: RowKind,
    pub label: String,
    pub depth: usize,
    pub color: Color32,
    pub is_selected: bool,
    pub is_visible: bool,
}

pub struct TimelineState {
    pub view_start: u64,
    pub view_width: u64,
    pub total_samples: u64,
    pub playhead: u64,
    pub rows: Vec<TimelineRowState>,
    pub selection: Option<[u64; 2]>,
    pub playing: bool,
    pub play_speed: f32,
    pub x_axis_mode: crate::core::session::XAxisMode,
    pub sample_rate: f32,
}

impl Default for TimelineState {
    fn default() -> Self {
        Self {
            view_start: 0,
            view_width: 1000,
            total_samples: 0,
            playhead: 0,
            rows: Vec::new(),
            selection: None,
            playing: false,
            play_speed: 1.0,
            x_axis_mode: crate::core::session::XAxisMode::Seconds,
            sample_rate: 40000.0,
        }
    }
}
