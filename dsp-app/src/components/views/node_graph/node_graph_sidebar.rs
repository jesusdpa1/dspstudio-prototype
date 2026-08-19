use egui::{Ui, Pos2};
use egui_snarl::Snarl;
use dsp_io::processing_graph::{ArithOpSpec, ChannelId};
use dsp_core::filter::WindowType;
use dsp_core::detection::CrossingDirection;
use dsp_core::detection::double::DoubleThresholdMode;
use crate::core::session::{SessionState, WorkspaceState};
use super::nodes::DspNode;

pub struct NodeGraphSidebar;

impl NodeGraphSidebar {
    pub fn show(ui: &mut Ui, workspace: &mut WorkspaceState, snarl: &mut Snarl<DspNode>, add_pos: &mut Pos2) {
        ui.vertical_centered(|ui| {
            ui.heading("Node Inventory");
        });
        ui.separator();

        let session = match workspace.active_recording_id.as_ref().and_then(|id| workspace.recordings.get(id)) {
            Some(s) => s,
            None => {
                ui.label("No active recording.");
                return;
            }
        };

        let mut to_insert = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                Self::show_panel_content(ui, &mut to_insert, session);
            });

        if let Some(node) = to_insert {
            snarl.insert_node(*add_pos, node);
            *add_pos += egui::vec2(30.0, 30.0);
        }
    }

    fn panel_group_header(ui: &mut Ui, label: &str) {
        ui.add_space(6.0);
        ui.strong(label);
        ui.separator();
    }

    fn panel_btn(ui: &mut Ui, label: &str, to_insert: &mut Option<DspNode>, node: DspNode) {
        if ui
            .add(egui::Button::new(label).min_size(egui::vec2(ui.available_width(), 22.0)))
            .clicked()
        {
            *to_insert = Some(node);
        }
    }

    fn show_panel_content(ui: &mut Ui, to_insert: &mut Option<DspNode>, session: &SessionState) {
        Self::panel_group_header(ui, "Input");
        Self::panel_btn(ui, "📡  Channel", to_insert, DspNode::Channel {
            id: ChannelId::Physical(0),
            label: "Channel".into(),
            range: Default::default(),
        });

        let default_ids = if session.meta.n_channels >= 2 {
            vec![ChannelId::Physical(0), ChannelId::Physical(1)]
        } else {
            vec![ChannelId::Physical(0)]
        };
        let default_input = if session.meta.n_channels >= 2 { "0..2".into() } else { "[0]".into() };
        Self::panel_btn(ui, "📡  Multi Channel", to_insert, DspNode::MultiChannel {
            ids: default_ids,
            label: "MultiChannel".into(),
            input: default_input,
            range: Default::default(),
        });

        Self::panel_group_header(ui, "Math");
        Self::panel_btn(ui, "±   Arithmetic", to_insert, DspNode::Arithmetic { op: ArithOpSpec::Add });
        Self::panel_btn(ui, "1   Float", to_insert, DspNode::Float { value: 1.0, label: "Float".into() });
        Self::panel_btn(ui, "✓   Bool", to_insert, DspNode::Bool { value: false, label: "Bool".into() });

        Self::panel_group_header(ui, "Filters");
        Self::panel_btn(ui, "∿   IIR Filter", to_insert, DspNode::SosFilter {
            sos_text: String::new(),
            sos_rows: vec![],
            filtfilt: true,
            parse_error: None,
        });
        Self::panel_btn(ui, "∿   Sinc LP", to_insert, DspNode::SincLowpass {
            cutoff_hz: 300.0,
            n_taps: 101,
            window: WindowType::Hann,
            center: true,
        });
        Self::panel_btn(ui, "⌇   Moving Avg", to_insert, DspNode::MovingAverage {
            window: 40,
            center: true,
        });
        Self::panel_btn(ui, "⌇   Moving RMS", to_insert, DspNode::MovingRms {
            window: 40,
            center: true,
        });
        Self::panel_btn(ui, "⌇   EMA", to_insert, DspNode::ExponentialMovingAverage { alpha: 0.1 });
        Self::panel_btn(ui, "⌇   Median", to_insert, DspNode::MedianFilter {
            window: 5,
            center: true,
        });

        Self::panel_group_header(ui, "Epochs");
        Self::panel_btn(ui, "⚡  Threshold", to_insert, DspNode::SingleThresholdCrossing {
            threshold: 0.5,
            direction: CrossingDirection::Positive,
            refractory_samples: 40,
            label_pos: 1,
            label_neg: 2,
            range: Default::default(),
        });
        Self::panel_btn(ui, "⚡  Double Thr", to_insert, DspNode::DoubleThresholdCrossing {
            low: -0.2,
            high: 0.8,
            mode: DoubleThresholdMode::Hysteresis,
            refractory_samples: 20,
            label_high_enter: 10,
            label_low_exit: 20,
            range: Default::default(),
        });

        Self::panel_group_header(ui, "Output");
        Self::panel_btn(ui, "📤  Output", to_insert, DspNode::Output {
            label: "Output".into(),
            result: None,
        });
        Self::panel_btn(ui, "📤  Multi Output", to_insert, DspNode::MultiChannelOutput {
            n_channels: 2,
            label: "MultiOutput".into(),
            results: vec![None, None],
        });
        Self::panel_btn(ui, "📦  Events Output", to_insert, DspNode::EventsOutput {
            track_name: "spikes".into(),
            channel_idx: 0,
        });
    }

    pub fn show_graph_menu_content(ui: &mut Ui, snarl: &mut Snarl<DspNode>, pos: Pos2) {
        ui.strong("Input");
        ui.separator();
        if ui.button("Channel").clicked() {
            snarl.insert_node(pos, DspNode::Channel {
                id: ChannelId::Physical(0), label: "Channel".into(), range: Default::default(),
            });
            ui.close();
        }
        
        ui.strong("Math");
        ui.separator();
        if ui.button("Arithmetic").clicked() {
            snarl.insert_node(pos, DspNode::Arithmetic { op: ArithOpSpec::Add });
            ui.close();
        }

        ui.strong("Output");
        ui.separator();
        if ui.button("Output").clicked() {
            snarl.insert_node(pos, DspNode::Output { label: "Output".into(), result: None });
            ui.close();
        }
    }
}
