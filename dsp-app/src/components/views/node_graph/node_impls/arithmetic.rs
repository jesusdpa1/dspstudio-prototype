use crate::components::views::node_graph::layout::PIN_ARITH;
use crate::components::views::node_graph::nodes::DspNode;
use dsp_io::processing_graph::ArithOpSpec;
use egui::Ui;
use egui_snarl::ui::PinInfo;

pub fn show_arith_input(pin: &egui_snarl::InPin, ui: &mut Ui) -> PinInfo {
    ui.label(if pin.id.input == 0 { "A" } else { "B" });
    PinInfo::circle().with_fill(PIN_ARITH)
}

pub fn show_arith_output(ui: &mut Ui) -> PinInfo {
    ui.label("Out");
    PinInfo::circle().with_fill(PIN_ARITH)
}

pub fn show_arith_body(node: &mut DspNode, ui: &mut Ui, node_id: egui_snarl::NodeId) {
    if let DspNode::Arithmetic { op } = node {
        egui::ComboBox::from_id_salt(("arith_op", node_id))
            .selected_text(format!("{:?}", op))
            .show_ui(ui, |ui| {
                for candidate in [
                    ArithOpSpec::Add,
                    ArithOpSpec::Subtract,
                    ArithOpSpec::Multiply,
                    ArithOpSpec::Divide,
                ] {
                    ui.selectable_value(op, candidate, format!("{:?}", candidate));
                }
            });
    }
}
