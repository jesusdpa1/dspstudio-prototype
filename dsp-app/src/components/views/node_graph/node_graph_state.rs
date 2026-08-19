use egui::Pos2;
use egui_snarl::Snarl;
use dsp_io::recording_meta::VirtualChannelMeta;
use super::nodes::DspNode;

#[derive(Debug, Clone, PartialEq)]
pub enum NodeGraphStatus {
    Idle,
    Processing(f32),
    Complete,
    Error(String),
}

pub struct NodeGraphState {
    pub snarl: Snarl<DspNode>,
    pub status: NodeGraphStatus,
    pub completed_virtual_channels: Vec<VirtualChannelMeta>,
    pub add_pos: Pos2,
    pub show_node_panel: bool,
    pub node_panel_on_right: bool,
}

impl NodeGraphState {
    pub fn on_processing_progress(&mut self, p: f32) {
        self.status = NodeGraphStatus::Processing(p);
    }

    pub fn on_processing_complete(&mut self, virtual_channels: Vec<VirtualChannelMeta>) {
        self.status = NodeGraphStatus::Complete;
        self.completed_virtual_channels = virtual_channels;
    }
}

impl Default for NodeGraphState {
    fn default() -> Self {
        Self {
            snarl: Snarl::new(),
            status: NodeGraphStatus::Idle,
            completed_virtual_channels: Vec::new(),
            add_pos: Pos2::ZERO,
            show_node_panel: true,
            node_panel_on_right: false,
        }
    }
}
