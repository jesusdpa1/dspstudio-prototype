use egui::{Id, Pos2, Ui};
use egui_snarl::{Snarl, ui::{SnarlStyle, SnarlWidget}};
use crate::core::bridge::IoBridge;
use crate::core::session::{WorkspaceState, XAxisMode};
use super::nodes::DspNode;
use super::node_graph_state::{NodeGraphState, NodeGraphStatus};
use super::node_graph_view_model::{NodeGraphViewModel, DspSnarlViewer};

pub struct NodeGraphView;

impl NodeGraphView {
    pub fn new() -> Self {
        Self
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        workspace: &mut WorkspaceState,
        bridge: &IoBridge,
        state: &mut NodeGraphState,
    ) {
        if state.add_pos == Pos2::ZERO {
            state.add_pos = ui.clip_rect().center();
        }

        // Extract values that don't require borrowing workspace across closures
        let time_sel = workspace.time_selection();
        let x_axis_mode = workspace.x_axis_mode;

        if workspace.active_recording_id.as_ref().and_then(|id| workspace.recordings.get(id)).is_none() {
            ui.centered_and_justified(|ui| { ui.label("No active recording."); });
            return;
        }

        // ── Top bar ───────────────────────────────────────────────────────
        egui::Panel::top(ui.next_auto_id())
            .resizable(false)
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    let session = match workspace.active_recording_id.as_ref().and_then(|id| workspace.recordings.get(id)) {
                        Some(s) => s,
                        None => return,
                    };
                    let icon = if state.show_node_panel { "✕" } else { "☰" };
                    if ui.button(format!("{icon} Nodes")).clicked() {
                        state.show_node_panel = !state.show_node_panel;
                    }

                    ui.separator();

                    let sr = session.meta.sample_rate as f64;
                    let (start_sample, end_sample) = NodeGraphViewModel::resolve_active_range(
                        &state.snarl, session, time_sel
                    );
                    let count = end_sample.saturating_sub(start_sample);

                    let btn_label = if count == session.meta.total_samples {
                        "▶ Process All".to_string()
                    } else {
                        match x_axis_mode {
                            XAxisMode::Seconds => format!("▶ Process  {:.2}s – {:.2}s", start_sample as f64 / sr, end_sample as f64 / sr),
                            XAxisMode::Samples => format!("▶ Process  {} – {}", start_sample, end_sample),
                        }
                    };

                    let processing = matches!(state.status, NodeGraphStatus::Processing(_));
                    if ui
                        .add_enabled(!processing, egui::Button::new(btn_label))
                        .clicked()
                    {
                        NodeGraphViewModel::dispatch_process(&state.snarl, start_sample, count, session, bridge);
                        state.status = NodeGraphStatus::Processing(0.0);
                    }

                    if let NodeGraphStatus::Processing(progress) = state.status {
                        ui.spinner();
                        ui.label(format!("Processing {:.0}%…", progress * 100.0));
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("📥 Load Graph").clicked() {}
                        if ui.button("💾 Save Graph").clicked() {}
                    });
                });
            });

        // ── Node panel (Sidebar) ──────────────────────────────────────────
        // workspace is not borrowed here, so we can pass it mutably
        if state.show_node_panel {
            let panel = if state.node_panel_on_right {
                egui::Panel::right(ui.next_auto_id())
            } else {
                egui::Panel::left(ui.next_auto_id())
            };

            panel
                .resizable(false)
                .exact_size(200.0)
                .show_inside(ui, |ui| {
                    super::node_graph_sidebar::NodeGraphSidebar::show(ui, workspace, &mut state.snarl, &mut state.add_pos);
                });
        }

        // ── Canvas ────────────────────────────────────────────────────────
        egui::CentralPanel::default().show_inside(ui, |ui| {
            let session = match workspace.active_recording_id.as_ref().and_then(|id| workspace.recordings.get(id)) {
                Some(s) => s,
                None => return,
            };
            let mut viewer = DspSnarlViewer {
                session,
                selection: time_sel,
                x_axis_mode,
                add_pos: &mut state.add_pos,
            };
            SnarlWidget::new()
                .id(Id::new("dsp_node_graph"))
                .style(SnarlStyle::default())
                .show(&mut state.snarl, &mut viewer, ui);
        });
    }
}
