use egui::{Ui, Id};
use egui_snarl::ui::PinInfo;
use egui_snarl::NodeId;
use crate::components::views::node_graph::nodes::{DspNode, parse_channel_input};
use crate::components::views::node_graph::layout::{PIN_CHANNEL, PIN_FLOAT};
use crate::core::session::{SessionState, XAxisMode};
use dsp_io::processing_graph::ChannelId;
use super::utils::{show_channel_selector, range_selector_ui};

pub fn show_sources_input(_node: &DspNode, _pin: &egui_snarl::InPin, _ui: &mut Ui) -> PinInfo {
    PinInfo::circle().with_fill(egui::Color32::GRAY)
}

pub fn show_sources_output(
    node: &mut DspNode,
    pin: &egui_snarl::OutPin,
    ui: &mut Ui,
    session: &SessionState,
) -> PinInfo {
    match node {
        DspNode::Channel { id, .. } => {
            show_channel_selector(ui, id, session, pin.id.node, 0);
            PinInfo::circle().with_fill(PIN_CHANNEL)
        }
        DspNode::MultiChannel { ids, .. } => {
            let name = ids
                .get(pin.id.output)
                .map(|id| match id {
                    ChannelId::Physical(idx) => session
                        .meta
                        .channel_names
                        .get(*idx as usize)
                        .cloned()
                        .unwrap_or_else(|| format!("CH{}", idx)),
                    ChannelId::Virtual(n) => n.clone(),
                })
                .unwrap_or_else(|| format!("pin {}", pin.id.output));
            ui.label(name);
            PinInfo::circle().with_fill(PIN_CHANNEL)
        }
        DspNode::Float { value, .. } => {
            ui.label(format!("{:.3}", value));
            PinInfo::circle().with_fill(PIN_FLOAT)
        }
        _ => PinInfo::circle().with_fill(egui::Color32::GRAY),
    }
}

pub fn show_sources_body(
    node: &mut DspNode,
    ui: &mut Ui,
    session: &SessionState,
    selection: Option<[u64; 2]>,
    x_axis_mode: XAxisMode,
    node_id: NodeId,
) {
    match node {
        DspNode::Float { value, label } => {
            egui::Grid::new(("float_grid", node_id))
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.label("Label");
                    ui.add(egui::TextEdit::singleline(label).desired_width(80.0));
                    ui.end_row();

                    ui.label("Value");
                    ui.add(egui::DragValue::new(value).speed(0.01));
                    ui.end_row();
                });
        }
        DspNode::MultiChannel { ids, input, range, .. } => {
            egui::Grid::new(("mch_grid", node_id))
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.label("Channels");
                    let resp = ui.add(
                        egui::TextEdit::singleline(input)
                            .hint_text("[0,3,5] or 0..5")
                            .desired_width(120.0),
                    );
                    ui.end_row();

                    if resp.changed() {
                        let parsed = parse_channel_input(input);
                        if !parsed.is_empty() {
                            *ids = parsed;
                        }
                    }
                });

            let preview: Vec<String> = ids
                .iter()
                .map(|id| match id {
                    ChannelId::Physical(idx) => session
                        .meta
                        .channel_names
                        .get(*idx as usize)
                        .cloned()
                        .unwrap_or_else(|| format!("CH{}", idx)),
                    ChannelId::Virtual(n) => n.clone(),
                })
                .collect();
            ui.label(egui::RichText::new(preview.join(", ")).small().weak());

            ui.separator();
            ui.label("Range:");
            range_selector_ui(ui, range, session, selection, x_axis_mode, node_id);
        }
        DspNode::Channel { range, .. } => {
            ui.label("Range:");
            range_selector_ui(ui, range, session, selection, x_axis_mode, node_id);
        }
        _ => {}
    }
}
