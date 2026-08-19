//! Node graph view: canvas + toggleable node panel.

use std::collections::{HashMap, HashSet};
use egui::{Color32, Id, Pos2, Ui, vec2};
use egui_snarl::{
    InPin, NodeId, OutPin, Snarl,
    ui::{PinInfo, SnarlStyle, SnarlViewer, SnarlWidget},
};
use dsp_io::processing_graph::{ArithOpSpec, ProcessingGraphSpec, SpecNode, SpecWire, ChannelId};
use dsp_io::recording_meta::VirtualChannelMeta;
use dsp_core::filter::{WindowType};
use dsp_core::detection::{CrossingDirection};
use dsp_core::detection::double::DoubleThresholdMode;
use crate::components::views::node_graph::nodes::{DspNode};
use crate::core::bridge::{IoBridge, IoRequest};
use crate::core::session::{SessionState, XAxisMode};

use super::node_impls::{
    sources, arithmetic, filters, detectors, sinks,
};

// ── Pin colors ────────────────────────────────────────────────────────────────

pub const PIN_CHANNEL: Color32 = Color32::from_rgb(255, 180, 100);  // orange
pub const PIN_ARITH:   Color32 = Color32::from_rgb(100, 200, 255);  // blue
pub const PIN_FILTER:  Color32 = Color32::from_rgb(180, 120, 255);  // purple
pub const PIN_OUTPUT:  Color32 = Color32::from_rgb(100, 255, 150);  // green
pub const PIN_FLOAT:   Color32 = Color32::from_rgb(200, 200, 100);  // yellow
pub const PIN_EVENTS:  Color32 = Color32::from_rgb(255, 100, 150);  // pink/red

// ── NodeGraphLayout ───────────────────────────────────────────────────────────

pub struct NodeGraphLayout {
    pub snarl: Snarl<DspNode>,
    processing: bool,
    processing_progress: f32,
    pub completed_virtual_channels: Vec<VirtualChannelMeta>,
    add_pos: Pos2,
    pub show_node_panel: bool,
    pub node_panel_on_right: bool,
}

impl NodeGraphLayout {
    pub fn new() -> Self {
        Self {
            snarl: Snarl::new(),
            processing: false,
            processing_progress: 0.0,
            completed_virtual_channels: Vec::new(),
            add_pos: Pos2::ZERO,
            show_node_panel: true,
            node_panel_on_right: false,
        }
    }

    pub fn on_processing_progress(&mut self, p: f32) {
        self.processing_progress = p;
    }

    pub fn on_processing_complete(&mut self, virtual_channels: Vec<VirtualChannelMeta>) {
        self.processing = false;
        self.processing_progress = 1.0;
        self.completed_virtual_channels = virtual_channels;
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        selection: Option<[u64; 2]>,
        x_axis_mode: XAxisMode,
        session: &SessionState,
        bridge: &IoBridge,
    ) {
        if self.add_pos == Pos2::ZERO {
            self.add_pos = ui.clip_rect().center();
        }

        // ── Top bar ───────────────────────────────────────────────────────
        egui::Panel::top("ng_toolbar")
            .resizable(false)
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    let icon = if self.show_node_panel { "✕" } else { "☰" };
                    if ui.button(format!("{icon} Nodes")).clicked() {
                        self.show_node_panel = !self.show_node_panel;
                    }

                    if self.show_node_panel {
                        let side_icon = if self.node_panel_on_right { "◀" } else { "▶" };
                        if ui.button(side_icon)
                            .on_hover_text("Move panel to other side")
                            .clicked()
                        {
                            self.node_panel_on_right = !self.node_panel_on_right;
                        }
                    }

                    ui.separator();

                    let sr = session.meta.sample_rate as f64;
                    
                    // C.4 resolve range before dispatch
                    let (start_sample, end_sample) = self.resolve_active_range(session, selection);
                    let count = end_sample.saturating_sub(start_sample);
                    
                    let btn_label = if count == session.meta.total_samples {
                        "▶ Process All".to_string()
                    } else {
                        match x_axis_mode {
                            XAxisMode::Seconds => format!("▶ Process  {:.2}s – {:.2}s", start_sample as f64 / sr, end_sample as f64 / sr),
                            XAxisMode::Samples => format!("▶ Process  {} – {}", start_sample, end_sample),
                        }
                    };

                    if ui
                        .add_enabled(
                            !self.processing,
                            egui::Button::new(btn_label),
                        )
                        .clicked()
                    {
                        self.dispatch_process(start_sample, count, session, bridge);
                    }
                    if self.processing {
                        ui.spinner();
                        ui.label(format!(
                            "Processing {:.0}%…",
                            self.processing_progress * 100.0
                        ));
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("📥 Load Graph").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("JSON", &["json"])
                                .set_title("Load Processing Graph")
                                .pick_file()
                            {
                                match std::fs::read_to_string(path) {
                                    Ok(json) => {
                                        match serde_json::from_str::<egui_snarl::Snarl<DspNode>>(&json) {
                                            Ok(new_snarl) => {
                                                self.snarl = new_snarl;
                                            }
                                            Err(e) => {
                                                log::error!("Failed to parse graph JSON: {}", e);
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        log::error!("Failed to read graph file: {}", e);
                                    }
                                }
                            }
                        }

                        if ui.button("💾 Save Graph").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("JSON", &["json"])
                                .set_title("Save Processing Graph")
                                .save_file()
                            {
                                match serde_json::to_string_pretty(&self.snarl) {
                                    Ok(json) => {
                                        if let Err(e) = std::fs::write(path, json) {
                                            log::error!("Failed to write graph file: {}", e);
                                        }
                                    }
                                    Err(e) => {
                                        log::error!("Failed to serialize graph: {}", e);
                                    }
                                }
                            }
                        }
                    });
                });
            });

        // ── Node panel ────────────────────────────────────────────────────
        let mut to_insert: Option<DspNode> = None;

        if self.show_node_panel {
            let panel = if self.node_panel_on_right {
                egui::Panel::right("ng_node_panel")
            } else {
                egui::Panel::left("ng_node_panel")
            };

            panel
                .resizable(false)
                .exact_size(160.0)
                .show_inside(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            show_panel_content(ui, &mut to_insert, session);
                        });
                });
        }

        if let Some(node) = to_insert {
            self.snarl.insert_node(self.add_pos, node);
            self.add_pos += vec2(30.0, 30.0);
        }

        // ── Canvas ────────────────────────────────────────────────────────
        egui::CentralPanel::default().show_inside(ui, |ui| {
            let mut viewer = DspViewer {
                session,
                selection,
                x_axis_mode,
                add_pos: &mut self.add_pos,
            };
            SnarlWidget::new()
                .id(Id::new("dsp_node_graph"))
                .style(SnarlStyle::default())
                .show(&mut self.snarl, &mut viewer, ui);
        });
    }

    // ── Dispatch helpers ──────────────────────────────────────────────────

    fn resolve_active_range(&self, session: &SessionState, selection: Option<[u64; 2]>) -> (u64, u64) {
        // Use the range from the first node that has a non-default range, 
        // or fall back to the global selection.
        for node in self.snarl.nodes() {
            let range = match node {
                DspNode::Channel { range, .. } => Some(range),
                DspNode::MultiChannel { range, .. } => Some(range),
                DspNode::SingleThresholdCrossing { range, .. } => Some(range),
                DspNode::DoubleThresholdCrossing { range, .. } => Some(range),
                _ => None,
            };
            
            if let Some(r) = range {
                if *r != crate::components::views::node_graph::nodes::ProcessingRange::WholeRecording &&
                   *r != crate::components::views::node_graph::nodes::ProcessingRange::CurrentSelection {
                    return super::node_impls::utils::resolve_range(r, session, selection);
                }
            }
        }
        
        // Default to global selection
        super::node_impls::utils::resolve_range(
            &crate::components::views::node_graph::nodes::ProcessingRange::CurrentSelection,
            session,
            selection
        )
    }

    fn dispatch_process(&mut self, start_sample: u64, count: u64, session: &SessionState, bridge: &IoBridge) {
        let spec = self.extract_graph_spec(session.meta.sample_rate);
        if spec.output_node_indices().is_empty() { return; }
        let surplus = spec.required_surplus();

        self.processing = true;
        self.processing_progress = 0.0;
        bridge.send(IoRequest::RunProcessingGraph {
            dataset_id: session.meta.session_id.clone(),
            zarr_path: session.zarr_path.clone(),
            graph_spec: spec,
            total_samples: session.meta.total_samples,
            start_sample,
            count,
            batch_size: 40_000,
            surplus,
        });
    }

    pub fn extract_graph_spec(&self, sample_rate: f32) -> ProcessingGraphSpec {
        let node_list: Vec<(NodeId, &DspNode)> = self.snarl.node_ids().collect();
        let id_to_idx: HashMap<NodeId, usize> = node_list
            .iter()
            .enumerate()
            .map(|(i, (id, _))| (*id, i))
            .collect();
        let wire_map: HashMap<(NodeId, usize), (NodeId, usize)> = self
            .snarl
            .wires()
            .map(|(out, inp)| ((inp.node, inp.input), (out.node, out.output)))
            .collect();

        let nodes: Vec<SpecNode> = node_list
            .iter()
            .map(|&(node_id, node)| {
                let mut spec = node.to_spec_node();
                match &mut spec {
                    SpecNode::Output { source_id } | SpecNode::Fork { source_id, .. } => {
                        if let Some(src) = Self::trace_source_channel(
                            node_id, 0, &wire_map, &self.snarl, &mut HashSet::new(),
                        ) {
                            *source_id = src;
                        }
                    }
                    SpecNode::MultiChannelOutput { names, source_ids } => {
                        for pin in 0..names.len() {
                            if let Some(src) = Self::trace_source_channel(
                                node_id, pin, &wire_map, &self.snarl, &mut HashSet::new(),
                            ) {
                                names[pin] = src.drv_name();
                                source_ids[pin] = src;
                            }
                        }
                    }
                    _ => {}
                }
                spec
            })
            .collect();

        let wires: Vec<SpecWire> = self
            .snarl
            .wires()
            .filter_map(|(out, inp)| {
                let from = id_to_idx.get(&out.node)?;
                let to = id_to_idx.get(&inp.node)?;
                Some(SpecWire {
                    from_node: *from,
                    from_output: out.output,
                    to_node: *to,
                    to_input: inp.input,
                })
            })
            .collect();

        ProcessingGraphSpec { nodes, wires, sample_rate }
    }

    fn trace_source_channel(
        node_id: NodeId,
        input_pin: usize,
        wire_map: &HashMap<(NodeId, usize), (NodeId, usize)>,
        snarl: &Snarl<DspNode>,
        visited: &mut HashSet<NodeId>,
    ) -> Option<ChannelId> {
        if !visited.insert(node_id) { return None; }
        let &(src_node, src_pin) = wire_map.get(&(node_id, input_pin))?;
        match snarl.get_node(src_node)? {
            DspNode::Channel { id, .. } => Some(id.clone()),
            DspNode::MultiChannel { ids, .. } => ids.get(src_pin).cloned(),
            _ => Self::trace_source_channel(src_node, 0, wire_map, snarl, visited),
        }
    }
}

// ── Node panel content ────────────────────────────────────────────────────────

fn panel_group_header(ui: &mut Ui, label: &str) {
    ui.add_space(6.0);
    ui.strong(label);
    ui.separator();
}

fn panel_btn(ui: &mut Ui, label: &str, to_insert: &mut Option<DspNode>, node: DspNode) {
    if ui
        .add(egui::Button::new(label).min_size(egui::vec2(148.0, 22.0)))
        .clicked()
    {
        *to_insert = Some(node);
    }
}

fn show_panel_content(ui: &mut Ui, to_insert: &mut Option<DspNode>, session: &SessionState) {
    panel_group_header(ui, "Input");
    panel_btn(ui, "📡  Channel", to_insert, DspNode::Channel {
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
    panel_btn(ui, "📡  Multi Channel", to_insert, DspNode::MultiChannel {
        ids: default_ids,
        label: "MultiChannel".into(),
        input: default_input,
        range: Default::default(),
    });

    panel_group_header(ui, "Math");
    panel_btn(ui, "±   Arithmetic", to_insert, DspNode::Arithmetic { op: ArithOpSpec::Add });
    panel_btn(ui, "1   Float", to_insert, DspNode::Float { value: 1.0, label: "Float".into() });
    panel_btn(ui, "✓   Bool", to_insert, DspNode::Bool { value: false, label: "Bool".into() });

    panel_group_header(ui, "Filters");
    panel_btn(ui, "∿   Butterworth", to_insert, DspNode::Butterworth {
        order: 4,
        response: dsp_core::filter::FilterResponse::LowPass { cutoff: 300.0 },
        filtfilt: true,
    });
    panel_btn(ui, "∿   Chebyshev I", to_insert, DspNode::ChebyshevI {
        order: 4,
        ripple_db: 1.0,
        response: dsp_core::filter::FilterResponse::LowPass { cutoff: 300.0 },
        filtfilt: true,
    });
    panel_btn(ui, "∿   Chebyshev II", to_insert, DspNode::ChebyshevII {
        order: 4,
        atten_db: 60.0,
        response: dsp_core::filter::FilterResponse::LowPass { cutoff: 300.0 },
        filtfilt: true,
    });
    panel_btn(ui, "∿   Bessel", to_insert, DspNode::Bessel {
        order: 4,
        response: dsp_core::filter::FilterResponse::LowPass { cutoff: 300.0 },
        filtfilt: true,
    });
    panel_btn(ui, "⊗   Notch", to_insert, DspNode::Notch {
        freq_hz: 50.0,
        q: 30.0,
        filtfilt: true,
    });
    panel_btn(ui, "≋   Peak EQ", to_insert, DspNode::PeakEq {
        freq_hz: 1000.0,
        q: 2.0,
        gain_db: 0.0,
    });
    panel_btn(ui, "∿   IIR Filter", to_insert, DspNode::SosFilter {
        sos_text: String::new(),
        sos_rows: vec![],
        filtfilt: true,
        parse_error: None,
    });
    panel_btn(ui, "∿   Sinc LP", to_insert, DspNode::SincLowpass {
        cutoff_hz: 300.0,
        n_taps: 101,
        window: WindowType::Hann,
        center: true,
    });
    panel_btn(ui, "⌇   Moving Avg", to_insert, DspNode::MovingAverage {
        window: 40,
        center: true,
    });
    panel_btn(ui, "⌇   Moving RMS", to_insert, DspNode::MovingRms {
        window: 40,
        center: true,
    });
    panel_btn(ui, "⌇   EMA", to_insert, DspNode::ExponentialMovingAverage { alpha: 0.1 });
    panel_btn(ui, "⌇   Median", to_insert, DspNode::MedianFilter {
        window: 5,
        center: true,
    });

    panel_group_header(ui, "Epochs");
    panel_btn(ui, "⚡  Threshold", to_insert, DspNode::SingleThresholdCrossing {
        threshold: 0.5,
        direction: CrossingDirection::Positive,
        refractory_samples: 40,
        label_pos: 1,
        label_neg: 2,
        range: Default::default(),
    });
    panel_btn(ui, "⚡  Double Thr", to_insert, DspNode::DoubleThresholdCrossing {
        low: -0.2,
        high: 0.8,
        mode: DoubleThresholdMode::Hysteresis,
        refractory_samples: 20,
        label_high_enter: 10,
        label_low_exit: 20,
        range: Default::default(),
    });

    panel_group_header(ui, "Output");
    panel_btn(ui, "📤  Output", to_insert, DspNode::Output {
        label: "Output".into(),
        result: None,
    });
    panel_btn(ui, "📤  Multi Output", to_insert, DspNode::MultiChannelOutput {
        n_channels: 2,
        label: "MultiOutput".into(),
        results: vec![None, None],
    });
    panel_btn(ui, "📦  Events Output", to_insert, DspNode::EventsOutput {
        track_name: "spikes".into(),
        channel_idx: 0,
    });
}

// ── DspViewer ─────────────────────────────────────────────────────────────────

struct DspViewer<'a> {
    session: &'a SessionState,
    selection: Option<[u64; 2]>,
    x_axis_mode: XAxisMode,
    add_pos: &'a mut Pos2,
}

impl<'a> SnarlViewer<DspNode> for DspViewer<'a> {
    fn title(&mut self, node: &DspNode) -> String { node.title().to_string() }
    fn inputs(&mut self, node: &DspNode) -> usize { node.n_inputs() }
    fn outputs(&mut self, node: &DspNode) -> usize { node.n_outputs() }

    fn show_input(
        &mut self,
        pin: &InPin,
        ui: &mut Ui,
        snarl: &mut Snarl<DspNode>,
    ) -> impl egui_snarl::ui::SnarlPin + 'static {
        let node = match snarl.get_node(pin.id.node) {
            Some(n) => n,
            None => return PinInfo::circle().with_fill(Color32::GRAY),
        };
        match node {
            DspNode::Arithmetic { .. } => arithmetic::show_arith_input(pin, ui),
            DspNode::Output { .. } | DspNode::MultiChannelOutput { .. } | DspNode::EventsOutput { .. } => {
                sinks::show_sink_input(node, pin, ui)
            }
            DspNode::SingleThresholdCrossing { .. } | DspNode::DoubleThresholdCrossing { .. } => {
                detectors::show_detector_input(ui)
            }
            n if n.n_inputs() > 0 => filters::show_filter_input(ui),
            _ => PinInfo::circle().with_fill(Color32::GRAY),
        }
    }

    fn show_output(
        &mut self,
        pin: &OutPin,
        ui: &mut Ui,
        snarl: &mut Snarl<DspNode>,
    ) -> impl egui_snarl::ui::SnarlPin + 'static {
        let node = match snarl.get_node_mut(pin.id.node) {
            Some(n) => n,
            None => return PinInfo::circle().with_fill(Color32::GRAY),
        };
        match node {
            DspNode::Channel { .. } | DspNode::MultiChannel { .. } | DspNode::Float { .. } | DspNode::Bool { .. } => {
                sources::show_sources_output(node, pin, ui, self.session)
            }
            DspNode::Arithmetic { .. } => arithmetic::show_arith_output(ui),
            DspNode::SingleThresholdCrossing { .. } | DspNode::DoubleThresholdCrossing { .. } => {
                detectors::show_detector_output(ui)
            }
            n if n.n_outputs() > 0 => filters::show_filter_output(ui),
            _ => PinInfo::circle().with_fill(Color32::GRAY),
        }
    }

    fn has_body(&mut self, _node: &DspNode) -> bool { true }

    fn show_body(
        &mut self,
        node_id: NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut Ui,
        snarl: &mut Snarl<DspNode>,
    ) {
        let node = match snarl.get_node_mut(node_id) {
            Some(n) => n,
            None => return,
        };

        match node {
            DspNode::Float { .. } | DspNode::MultiChannel { .. } | DspNode::Bool { .. } | DspNode::Channel { .. } => {
                sources::show_sources_body(node, ui, self.session, self.selection, self.x_axis_mode, node_id);
            }
            DspNode::Arithmetic { .. } => {
                arithmetic::show_arith_body(node, ui, node_id);
            }
            DspNode::Output { .. } | DspNode::EventsOutput { .. } | DspNode::MultiChannelOutput { .. } => {
                sinks::show_sink_body(node, ui, node_id);
            }
            DspNode::SingleThresholdCrossing { .. } | DspNode::DoubleThresholdCrossing { .. } => {
                detectors::show_detector_body(node, ui, node_id, self.session, self.selection, self.x_axis_mode);
            }
            _ => {
                filters::show_filter_body(node, ui, node_id, self.session);
            }
        }
    }

    fn has_node_menu(&mut self, _node: &DspNode) -> bool { true }

    fn show_node_menu(
        &mut self,
        node_id: NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut Ui,
        snarl: &mut Snarl<DspNode>,
    ) {
        if ui.button("🗑  Delete node").clicked() {
            snarl.remove_node(node_id);
            ui.close();
        }
    }

    fn has_graph_menu(&mut self, _pos: Pos2, _snarl: &mut Snarl<DspNode>) -> bool { true }

    fn show_graph_menu(&mut self, pos: Pos2, ui: &mut Ui, snarl: &mut Snarl<DspNode>) {
        *self.add_pos = pos;

        ui.strong("Input");
        ui.separator();
        if ui.button("Channel").clicked() {
            snarl.insert_node(pos, DspNode::Channel {
                id: ChannelId::Physical(0), label: "Channel".into(), range: Default::default(),
            });
            ui.close();
        }
        if ui.button("Multi Channel").clicked() {
            snarl.insert_node(pos, DspNode::MultiChannel {
                ids: vec![ChannelId::Physical(0)], label: "MultiChannel".into(),
                input: "[0]".into(), range: Default::default(),
            });
            ui.close();
        }

        ui.add_space(4.0);
        ui.strong("Math");
        ui.separator();
        if ui.button("Arithmetic").clicked() {
            snarl.insert_node(pos, DspNode::Arithmetic { op: ArithOpSpec::Add });
            ui.close();
        }
        if ui.button("Float").clicked() {
            snarl.insert_node(pos, DspNode::Float { value: 1.0, label: "Float".into() });
            ui.close();
        }
        if ui.button("Bool").clicked() {
            snarl.insert_node(pos, DspNode::Bool { value: false, label: "Bool".into() });
            ui.close();
        }

        ui.add_space(4.0);
        ui.strong("Filters");
        ui.separator();
        if ui.button("IIR Filter").clicked() {
            snarl.insert_node(pos, DspNode::SosFilter {
                sos_text: String::new(), sos_rows: vec![], filtfilt: true, parse_error: None,
            });
            ui.close();
        }
        if ui.button("Sinc LP").clicked() {
            snarl.insert_node(pos, DspNode::SincLowpass {
                cutoff_hz: 300.0, n_taps: 101, window: WindowType::Hann, center: true,
            });
            ui.close();
        }
        if ui.button("Moving Avg").clicked() {
            snarl.insert_node(pos, DspNode::MovingAverage { window: 40, center: true });
            ui.close();
        }
        if ui.button("Moving RMS").clicked() {
            snarl.insert_node(pos, DspNode::MovingRms { window: 40, center: true });
            ui.close();
        }
        if ui.button("EMA").clicked() {
            snarl.insert_node(pos, DspNode::ExponentialMovingAverage { alpha: 0.1 });
            ui.close();
        }
        if ui.button("Median").clicked() {
            snarl.insert_node(pos, DspNode::MedianFilter { window: 5, center: true });
            ui.close();
        }

        ui.add_space(4.0);
        ui.strong("Epochs");
        ui.separator();
        if ui.button("Threshold Crossing").clicked() {
            snarl.insert_node(pos, DspNode::SingleThresholdCrossing {
                threshold: 0.5,
                direction: CrossingDirection::Positive,
                refractory_samples: 40,
                label_pos: 1,
                label_neg: 2,
                range: Default::default(),
            });
            ui.close();
        }
        if ui.button("Double Threshold").clicked() {
            snarl.insert_node(pos, DspNode::DoubleThresholdCrossing {
                low: -0.2,
                high: 0.8,
                mode: DoubleThresholdMode::Hysteresis,
                refractory_samples: 20,
                label_high_enter: 10,
                label_low_exit: 20,
                range: Default::default(),
            });
            ui.close();
        }

        ui.add_space(4.0);
        ui.strong("Output");
        ui.separator();
        if ui.button("Output").clicked() {
            snarl.insert_node(pos, DspNode::Output { label: "Output".into(), result: None });
            ui.close();
        }
        if ui.button("Multi Output").clicked() {
            snarl.insert_node(pos, DspNode::MultiChannelOutput {
                n_channels: 2, label: "MultiOutput".into(), results: vec![None, None],
            });
            ui.close();
        }
        if ui.button("Events Output").clicked() {
            snarl.insert_node(pos, DspNode::EventsOutput {
                track_name: "spikes".into(), channel_idx: 0,
            });
            ui.close();
        }
    }
}
