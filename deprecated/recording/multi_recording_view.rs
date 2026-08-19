//! Stateless stacked multi-channel waveform renderer.

use crate::core::session::{WorkspaceState, XAxisMode, RecordingSession};
use dsp_io::transmission::ui::ViewResponse;
use dsp_io::processing_graph::ChannelId;
use egui_plot::{Line, Plot, PlotBounds, PlotPoints};
use egui_tiles::TileId;

pub struct MultiRecordingView {
    pub show_integrated_sidebar: bool,
}

impl MultiRecordingView {
    pub fn new() -> Self { Self { show_integrated_sidebar: true } }

    pub fn show(
        &self,
        ui: &mut egui::Ui,
        pane_id: TileId,
        workspace: &mut WorkspaceState,
        dataset_id: &mut Option<String>,
        channel_spacing: f32,
    ) {
        ui.horizontal(|ui| {
            ui.label("Dataset:");
            let selected_name = dataset_id.as_deref()
                .and_then(|id| workspace.recordings.get(id))
                .map(|s| s.meta.recording_name.as_str())
                .unwrap_or("None")
                .to_string();
            egui::ComboBox::from_id_salt(("multi_dataset_select", pane_id))
                .selected_text(&selected_name)
                .show_ui(ui, |ui| {
                    for (id, session) in &workspace.recordings {
                        ui.selectable_value(dataset_id, Some(id.clone()), &session.meta.recording_name);
                    }
                });
        });

        let id = match dataset_id {
            Some(id) => id,
            None => {
                ui.centered_and_justified(|ui| { ui.label("Select a dataset to begin."); });
                return;
            }
        };

        let selection = workspace.selection;
        let x_axis_mode = workspace.x_axis_mode;
        let view_start = workspace.view_start;
        let view_width = workspace.view_width;

        let session = match workspace.get_recording_mut(id) {
            Some(s) => s,
            None => {
                ui.centered_and_justified(|ui| { ui.label(format!("Dataset {} not found.", id)); });
                return;
            }
        };

        // Integrated Sidebar (Collapsible)
        if self.show_integrated_sidebar {
            egui::Panel::right(ui.make_persistent_id("multi_sidebar"))
                .resizable(true)
                .default_size(200.0)
                .show_animated_inside(ui, true, |ui| {
                    crate::components::panels::recording::sidebar::show(ui, session);
                });
        }

        let visible_ids = session.visible_channel_ids();

        if visible_ids.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label("No channels visible.");
            });
            return;
        }

        let (x_min, x_max) = crate::core::session::current_x_range(view_start, view_width, x_axis_mode, session.meta.sample_rate);

        let mut viewport_update = None;

        ui.push_id(pane_id, |ui| {
            let plot = Plot::new(ui.make_persistent_id("multi_recording_plot"))
                .height(ui.available_height())
                .allow_zoom(true)
                .allow_drag(true)
                .allow_scroll(true)
                .allow_boxed_zoom(true)
                .show_axes([true, true])
                .x_axis_label(x_axis_mode.label())
                .y_axis_formatter(move |mark, _range| {
                    if channel_spacing > 0.0 {
                        format!("{:.0}", (mark.value / channel_spacing as f64).abs())
                    } else {
                        format!("{:.0}", mark.value.abs())
                    }
                });

            let resp = plot.show(ui, |plot_ui| {
                // Sync X bounds with session, but allow free Y bounds
                let curr_bounds = plot_ui.plot_bounds();
                plot_ui.set_plot_bounds(PlotBounds::from_min_max(
                    [x_min, curr_bounds.min()[1]],
                    [x_max, curr_bounds.max()[1]],
                ));

                crate::core::session::draw_selection(selection, x_axis_mode, plot_ui, session.meta.sample_rate);

                if let Some(r) = &session.cache {
                    for (i, cid) in visible_ids.iter().enumerate() {
                        let color = channel_color(cid, session);
                        // Descending Y: Channel 0 is at 0, Channel 1 is at -spacing, etc.
                        let offset = -(i as f64 * channel_spacing as f64);
                        draw_channel_offset(plot_ui, r, cid, color, x_axis_mode, session.meta.sample_rate, offset);
                    }
                }
            });

            // Update global viewport if interacted
            let bounds = resp.transform.bounds();
            let new_x_min = bounds.min()[0];
            let new_x_max = bounds.max()[0];
            let hz = session.meta.sample_rate as f64;
            
            if (new_x_min - x_min).abs() > 1e-9 || (new_x_max - x_max).abs() > 1e-9 {
                let (s, w) = match x_axis_mode {
                    XAxisMode::Samples => (new_x_min.max(0.0) as u64, (new_x_max - new_x_min).max(1.0) as u64),
                    XAxisMode::Seconds => ((new_x_min * hz).max(0.0) as u64, ((new_x_max - new_x_min) * hz).max(1.0) as u64),
                };
                viewport_update = Some((s, w));
            }
        });

        if let Some((s, w)) = viewport_update {
            workspace.view_start = s;
            workspace.view_width = w;
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn channel_color(id: &ChannelId, session: &RecordingSession) -> egui::Color32 {
    match id {
        ChannelId::Physical(idx) => session.display.get(*idx as usize).map(|d| d.egui_color()).unwrap_or(egui::Color32::GRAY),
        ChannelId::Virtual(name) => {
            if let Some(pos) = session.meta.virtual_channels.iter().position(|vc| vc.name == *name) {
                session.virtual_display.get(pos).map(|d| d.egui_color()).unwrap_or(egui::Color32::GRAY)
            } else {
                egui::Color32::GRAY
            }
        }
    }
}

fn draw_channel_offset(
    plot_ui: &mut egui_plot::PlotUi,
    resp: &ViewResponse,
    id: &ChannelId,
    color: egui::Color32,
    x_axis_mode: XAxisMode,
    sample_rate: f32,
    y_offset: f64,
) {
    let pos = match resp.channels_returned.iter().position(|c| c == id) {
        Some(p) => p,
        None => return,
    };

    let ppc = resp.points_per_channel;
    if ppc == 0 { return; }

    let stride = if resp.lod_level == 0 { 1 } else { 2 };
    let offset = pos * ppc * stride;
    let ratio = resp.decimation_ratio;

    let sr = sample_rate as f64;
    let to_x = |i: usize| -> f64 {
        let sample = resp.actual_start + i as u64 * ratio;
        match x_axis_mode {
            XAxisMode::Samples => sample as f64,
            XAxisMode::Seconds => sample as f64 / sr,
        }
    };

    if resp.lod_level == 0 {
        let pts: PlotPoints = (0..ppc)
            .map(|i| [to_x(i), (resp.data[offset + i] as f64) + y_offset])
            .collect();
        plot_ui.line(Line::new("", pts).color(color));
    } else {
        let pts: PlotPoints = (0..ppc)
            .flat_map(|i| {
                let x = to_x(i);
                let min = resp.data[offset + i * 2] as f64;
                let max = resp.data[offset + i * 2 + 1] as f64;
                [[x, min + y_offset], [x, max + y_offset], [f64::NAN, f64::NAN]]
            })
            .collect();
        plot_ui.line(Line::new("", pts).color(color));
    }
}
