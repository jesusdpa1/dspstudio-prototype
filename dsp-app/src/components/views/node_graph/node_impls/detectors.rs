use egui::Ui;
use egui_snarl::ui::PinInfo;
use egui_snarl::NodeId;
use crate::components::views::node_graph::nodes::DspNode;
use crate::components::views::node_graph::layout::{PIN_FILTER, PIN_EVENTS};
use crate::core::session::{SessionState, XAxisMode};
use dsp_core::detection::CrossingDirection;
use dsp_core::detection::double::DoubleThresholdMode;
use super::utils::range_selector_ui;

pub fn show_detector_input(ui: &mut Ui) -> PinInfo {
    ui.label("Waveform");
    PinInfo::circle().with_fill(PIN_FILTER)
}

pub fn show_detector_output(ui: &mut Ui) -> PinInfo {
    ui.label("Events");
    PinInfo::circle().with_fill(PIN_EVENTS)
}

pub fn show_detector_body(
    node: &mut DspNode,
    ui: &mut Ui,
    node_id: NodeId,
    session: &SessionState,
    selection: Option<[u64; 2]>,
    x_axis_mode: XAxisMode,
) {
    match node {
        // ── Single Threshold ─────────────────────────────────────────────
        DspNode::SingleThresholdCrossing {
            threshold,
            direction,
            refractory_samples,
            label_pos,
            label_neg,
            range,
        } => {
            egui::Grid::new(("stc_grid", node_id))
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.label("Threshold");
                    ui.add(egui::DragValue::new(threshold).speed(0.1));
                    ui.end_row();

                    ui.label("Direction");
                    egui::ComboBox::from_id_salt(("dir", node_id))
                        .selected_text(format!("{:?}", direction))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(direction, CrossingDirection::Positive, "Positive");
                            ui.selectable_value(direction, CrossingDirection::Negative, "Negative");
                            ui.selectable_value(direction, CrossingDirection::Both, "Both");
                        });
                    ui.end_row();

                    ui.label("Refractory");
                    ui.add(egui::DragValue::new(refractory_samples).speed(1.0).suffix(" smp"));
                    ui.end_row();

                    ui.label("Label (+)");
                    ui.add(egui::DragValue::new(label_pos));
                    ui.end_row();

                    ui.label("Label (−)");
                    ui.add(egui::DragValue::new(label_neg));
                    ui.end_row();
                });

            ui.separator();
            ui.label("Range:");
            range_selector_ui(ui, range, session, selection, x_axis_mode, node_id);
        }

        // ── Double Threshold ─────────────────────────────────────────────
        DspNode::DoubleThresholdCrossing {
            low,
            high,
            mode,
            refractory_samples,
            label_high_enter,
            label_low_exit,
            range,
        } => {
            egui::Grid::new(("dtc_grid", node_id))
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.label("Low");
                    ui.add(egui::DragValue::new(low).speed(0.1));
                    ui.end_row();

                    ui.label("High");
                    ui.add(egui::DragValue::new(high).speed(0.1));
                    ui.end_row();

                    ui.label("Mode");
                    egui::ComboBox::from_id_salt(("mode", node_id))
                        .selected_text(format!("{:?}", mode))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(mode, DoubleThresholdMode::Hysteresis, "Hysteresis");
                            ui.selectable_value(mode, DoubleThresholdMode::Window, "Window");
                        });
                    ui.end_row();

                    ui.label("Refractory");
                    ui.add(egui::DragValue::new(refractory_samples).speed(1.0).suffix(" smp"));
                    ui.end_row();

                    let (l1, l2) = match mode {
                        DoubleThresholdMode::Hysteresis => ("Label High", "Label Low"),
                        DoubleThresholdMode::Window => ("Label Enter", "Label Exit"),
                    };
                    ui.label(l1);
                    ui.add(egui::DragValue::new(label_high_enter));
                    ui.end_row();

                    ui.label(l2);
                    ui.add(egui::DragValue::new(label_low_exit));
                    ui.end_row();
                });

            if *low >= *high {
                *high = *low + 0.1;
            }

            ui.separator();
            ui.label("Range:");
            range_selector_ui(ui, range, session, selection, x_axis_mode, node_id);
        }

        _ => {}
    }
}
