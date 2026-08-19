use egui::Ui;
use crate::core::session::WorkspaceState;
use crate::blueprint::pane::YAxisRange;

pub struct TimeseriesMultichannelSidebar;

impl TimeseriesMultichannelSidebar {
    pub fn show(ui: &mut Ui, workspace: &mut WorkspaceState, y_range: &mut YAxisRange) {
        ui.vertical_centered(|ui| {
            ui.heading("Timeseries Settings");
        });
        ui.separator();

        ui.label("Y-Axis Scaling:");
        ui.horizontal(|ui| {
            let mut is_auto = matches!(y_range, YAxisRange::Auto);
            if ui.selectable_label(is_auto, "Auto").clicked() {
                *y_range = YAxisRange::Auto;
            }
            if ui.selectable_label(!is_auto, "Manual").clicked() {
                if is_auto {
                    *y_range = YAxisRange::Manual { min: -1.0, max: 1.0 };
                }
            }
        });

        if let YAxisRange::Manual { min, max } = y_range {
            ui.add(egui::DragValue::new(min).speed(0.1).prefix("Min: "));
            ui.add(egui::DragValue::new(max).speed(0.1).prefix("Max: "));
        }

        ui.separator();
        ui.label("Channel Visibility:");
        // In a real implementation, we could list all channels here for bulk toggling
        if ui.button("Show All").clicked() {
            for session in workspace.recordings.values_mut() {
                for d in &mut session.display { d.visible = true; }
                for d in &mut session.virtual_display { d.visible = true; }
            }
        }
        if ui.button("Hide All").clicked() {
            for session in workspace.recordings.values_mut() {
                for d in &mut session.display { d.visible = false; }
                for d in &mut session.virtual_display { d.visible = false; }
            }
        }
    }
}
