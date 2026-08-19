use egui::Ui;
use egui_snarl::ui::PinInfo;
use crate::components::views::node_graph::nodes::DspNode;
use crate::components::views::node_graph::layout::PIN_OUTPUT;
use super::utils::show_mini_plot;

pub fn show_sink_input(node: &DspNode, pin: &egui_snarl::InPin, ui: &mut Ui) -> PinInfo {
    match node {
        DspNode::Output { .. } => {
            ui.label("In");
        }
        DspNode::MultiChannelOutput { .. } => {
            ui.label(format!("In {}", pin.id.input));
        }
        _ => {
            ui.label("Waveform");
        }
    }
    PinInfo::circle().with_fill(PIN_OUTPUT)
}

pub fn show_sink_output(_ui: &mut Ui) -> PinInfo {
    PinInfo::circle().with_fill(egui::Color32::GRAY)
}

pub fn show_sink_body(node: &mut DspNode, ui: &mut Ui, node_id: egui_snarl::NodeId) {
    match node {
        // ── Events Output ────────────────────────────────────────────────
        DspNode::EventsOutput { track_name, channel_idx } => {
            egui::Grid::new(("evout_grid", node_id))
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.label("Track");
                    ui.add(egui::TextEdit::singleline(track_name).desired_width(100.0));
                    ui.end_row();

                    ui.label("Channel");
                    ui.add(egui::DragValue::new(channel_idx));
                    ui.end_row();
                });
            ui.label(egui::RichText::new("Writes sparse events to Zarr").small().weak());
        }

        // ── Output ────────────────────────────────────────────────────────
        DspNode::Output { result, label } => {
            egui::Grid::new(("out_grid", node_id))
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.label("Name");
                    ui.add(egui::TextEdit::singleline(label).desired_width(120.0));
                    ui.end_row();
                });
            if let Some(sig) = result {
                show_mini_plot(ui, sig, PIN_OUTPUT, node_id, 0);
            } else {
                ui.label(egui::RichText::new("No data yet").small().weak());
            }
        }

        _ => {}
    }
}
