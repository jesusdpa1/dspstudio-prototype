use std::collections::{HashMap, HashSet};
use egui::{Color32, Pos2, Ui};
use egui_snarl::{InPin, NodeId, OutPin, Snarl, ui::{SnarlViewer, PinInfo}};
use dsp_io::processing_graph::{ProcessingGraphSpec, SpecNode, SpecWire, ChannelId};
use crate::core::bridge::{IoBridge, IoRequest};
use crate::core::session::{SessionState, XAxisMode};
use super::nodes::DspNode;
use super::node_impls::{sources, arithmetic, filters, detectors, sinks};

pub struct NodeGraphViewModel;

impl NodeGraphViewModel {
    pub fn dispatch_process(
        snarl: &Snarl<DspNode>,
        start_sample: u64,
        count: u64,
        session: &SessionState,
        bridge: &IoBridge
    ) {
        let spec = Self::extract_graph_spec(snarl, session.meta.sample_rate);
        if spec.output_node_indices().is_empty() { return; }
        let surplus = spec.required_surplus();

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

    pub fn extract_graph_spec(snarl: &Snarl<DspNode>, sample_rate: f32) -> ProcessingGraphSpec {
        let node_list: Vec<(NodeId, &DspNode)> = snarl.node_ids().collect();
        let id_to_idx: HashMap<NodeId, usize> = node_list
            .iter()
            .enumerate()
            .map(|(i, (id, _))| (*id, i))
            .collect();
        let wire_map: HashMap<(NodeId, usize), (NodeId, usize)> = snarl
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
                            node_id, 0, &wire_map, snarl, &mut HashSet::new(),
                        ) {
                            *source_id = src;
                        }
                    }
                    SpecNode::MultiChannelOutput { names, source_ids } => {
                        for pin in 0..names.len() {
                            if let Some(src) = Self::trace_source_channel(
                                node_id, pin, &wire_map, snarl, &mut HashSet::new(),
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

        let wires: Vec<SpecWire> = snarl
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

    pub fn resolve_active_range(snarl: &Snarl<DspNode>, session: &SessionState, selection: Option<[u64; 2]>) -> (u64, u64) {
        for node in snarl.nodes() {
            let range = match node {
                DspNode::Channel { range, .. } => Some(range),
                DspNode::MultiChannel { range, .. } => Some(range),
                DspNode::SingleThresholdCrossing { range, .. } => Some(range),
                DspNode::DoubleThresholdCrossing { range, .. } => Some(range),
                _ => None,
            };
            
            if let Some(r) = range {
                if *r != super::nodes::ProcessingRange::WholeRecording &&
                   *r != super::nodes::ProcessingRange::CurrentSelection {
                    return super::node_impls::utils::resolve_range(r, session, selection);
                }
            }
        }
        
        super::node_impls::utils::resolve_range(
            &super::nodes::ProcessingRange::CurrentSelection,
            session,
            selection
        )
    }
}

pub struct DspSnarlViewer<'a> {
    pub session: &'a SessionState,
    pub selection: Option<[u64; 2]>,
    pub x_axis_mode: XAxisMode,
    pub add_pos: &'a mut Pos2,
}

impl<'a> SnarlViewer<DspNode> for DspSnarlViewer<'a> {
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
        super::node_graph_sidebar::NodeGraphSidebar::show_graph_menu_content(ui, snarl, pos);
    }
}
