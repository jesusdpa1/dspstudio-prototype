//! Stateless individual-channel trace renderer.

use crate::core::session::{RecordingSession, WorkspaceState, XAxisMode};
use dsp_io::transmission::ui::ViewResponse;
use dsp_io::processing_graph::ChannelId;
use egui_plot::{Line, Plot, PlotBounds, PlotPoints};
use egui_tiles::TileId;

/// Which channel row has keyboard/mouse focus for interactive panning/zooming.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Deserialize, serde::Serialize)]
pub enum FocusMode {
    #[default]
    None,
    Physical(usize),
    Virtual(String),
}

/// Height of each channel row in the stacked (non-focus) view.
const ROW_HEIGHT: f32 = 80.0;

pub struct RecordingView {
    pub show_integrated_sidebar: bool,
}

impl RecordingView {
    pub fn new() -> Self { Self { show_integrated_sidebar: true } }

    pub fn show(
        &self,
        ui: &mut egui::Ui,
        pane_id: TileId,
        workspace: &mut WorkspaceState,
        dataset_id: &mut Option<String>,
        focus_mode: &mut FocusMode,
    ) {
        ui.horizontal(|ui| {
            ui.label("Dataset:");
            let selected_name = dataset_id.as_deref()
                .and_then(|id| workspace.recordings.get(id))
                .map(|s| s.meta.recording_name.as_str())
                .unwrap_or("None")
                .to_string();
            egui::ComboBox::from_id_salt(("dataset_select", pane_id))
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
            egui::Panel::right(ui.make_persistent_id("view_sidebar"))
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

        ui.push_id(pane_id, |ui| {
            match focus_mode {
                FocusMode::None => self.show_stacked(ui, x_axis_mode, selection, x_min, x_max, session, visible_ids, focus_mode),
                _ => self.show_focused(ui, x_axis_mode, selection, x_min, x_max, session, visible_ids, focus_mode),
            }
        });
    }

    fn show_stacked(
        &self,
        ui: &mut egui::Ui,
        x_axis_mode: XAxisMode,
        selection: Option<[u64; 2]>,
        x_min: f64,
        x_max: f64,
        session: &RecordingSession,
        visible_ids: Vec<ChannelId>,
        focus_mode: &mut FocusMode,
    ) {
        let total_rows = visible_ids.len();

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (stack_idx, id) in visible_ids.iter().enumerate() {
                    let is_last = stack_idx == total_rows - 1;
                    let ch_name = channel_name(id, session);
                    let color = channel_color(id, session);

                    let resp = Plot::new(ui.make_persistent_id(format!("trace_{:?}", id)))
                        .height(ROW_HEIGHT)
                        .allow_zoom(false)
                        .allow_drag(false)
                        .allow_scroll(false)
                        .allow_boxed_zoom(false)
                        .allow_double_click_reset(false)
                        .show_axes([is_last, true])
                        .x_axis_label(if is_last { x_axis_mode.label() } else { "" })
                        .y_axis_formatter(|mark, _range| format!("{:+.2e}", mark.value))
                        .show(ui, |plot_ui| {
                            let (y_min, y_max) = y_bounds(session.cache.as_ref(), id).unwrap_or((-1.0, 1.0));
                            plot_ui.set_plot_bounds(PlotBounds::from_min_max(
                                [x_min, y_min],
                                [x_max, y_max],
                            ));
                            crate::core::session::draw_selection(selection, x_axis_mode, plot_ui, session.meta.sample_rate);
                            if let Some(r) = &session.cache {
                                draw_channel(plot_ui, r, id, color, &ch_name, x_axis_mode, session.meta.sample_rate);
                            }
                            if let ChannelId::Physical(ch_idx) = id {
                                draw_event_ticks(plot_ui, session, *ch_idx, y_min, y_max, x_axis_mode);
                            }
                        });

                    if resp.response.double_clicked() || resp.response.secondary_clicked() {
                        *focus_mode = match id {
                            ChannelId::Physical(idx) => FocusMode::Physical(*idx as usize),
                            ChannelId::Virtual(name) => FocusMode::Virtual(name.clone()),
                        };
                    }
                }
            });
    }

    fn show_focused(
        &self,
        ui: &mut egui::Ui,
        x_axis_mode: XAxisMode,
        selection: Option<[u64; 2]>,
        x_min: f64,
        x_max: f64,
        session: &RecordingSession,
        visible_ids: Vec<ChannelId>,
        focus_mode: &mut FocusMode,
    ) {
        let id = match visible_ids.iter().find(|id| match (&*focus_mode, *id) {
            (FocusMode::Physical(f), ChannelId::Physical(c)) => *f == *c as usize,
            (FocusMode::Virtual(f), ChannelId::Virtual(c)) => f == c,
            _ => false,
        }) {
            Some(id) => id,
            None => { *focus_mode = FocusMode::None; return; }
        };

        let ch_name = channel_name(id, session);
        let color = channel_color(id, session);

        let resp = Plot::new(ui.make_persistent_id(format!("trace_focus_{:?}", id)))
            .height(ui.available_height())
            .allow_zoom(true)
            .allow_drag(true)
            .allow_scroll(true)
            .allow_boxed_zoom(true)
            .show_axes([true, true])
            .x_axis_label(x_axis_mode.label())
            .y_axis_label(&ch_name)
            .show(ui, |plot_ui| {
                let (y_min, y_max) = y_bounds(session.cache.as_ref(), id).unwrap_or((-1.0, 1.0));
                plot_ui.set_plot_bounds(PlotBounds::from_min_max(
                    [x_min, y_min],
                    [x_max, y_max],
                ));
                crate::core::session::draw_selection(selection, x_axis_mode, plot_ui, session.meta.sample_rate);
                if let Some(r) = &session.cache {
                    draw_channel(plot_ui, r, id, color, &ch_name, x_axis_mode, session.meta.sample_rate);
                }
            });

        if resp.response.double_clicked() || resp.response.secondary_clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            *focus_mode = FocusMode::None;
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn channel_name(id: &ChannelId, session: &RecordingSession) -> String {
    match id {
        ChannelId::Physical(idx) => session
            .meta
            .channel_names
            .get(*idx as usize)
            .cloned()
            .unwrap_or_else(|| format!("CH{}", idx)),
        ChannelId::Virtual(name) => name.clone(),
    }
}

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

fn y_bounds(cache: Option<&ViewResponse>, id: &ChannelId) -> Option<(f64, f64)> {
    let r = cache?;
    let pos = r.channels_returned.iter().position(|cid| cid == id)?;
    let p_per_ch = r.points_per_channel;

    let chunk = if r.lod_level == 0 {
        &r.data[pos * p_per_ch..(pos + 1) * p_per_ch]
    } else {
        &r.data[pos * p_per_ch * 2..(pos + 1) * p_per_ch * 2]
    };

    if chunk.is_empty() { return None; }

    let mut min = f32::MAX;
    let mut max = f32::MIN;
    for &v in chunk {
        if v < min { min = v; }
        if v > max { max = v; }
    }

    if min == max {
        Some((min as f64 - 1.0, max as f64 + 1.0))
    } else {
        let padding = (max - min) * 0.1;
        Some((min as f64 - padding as f64, max as f64 + padding as f64))
    }
}

const EVENT_PALETTE: &[egui::Color32] = &[
    egui::Color32::from_rgb(255, 100, 100),
    egui::Color32::from_rgb(80, 200, 120),
    egui::Color32::from_rgb(100, 160, 255),
    egui::Color32::from_rgb(255, 200, 50),
    egui::Color32::from_rgb(220, 100, 220),
];

fn draw_event_ticks(
    plot_ui: &mut egui_plot::PlotUi,
    session: &RecordingSession,
    ch_idx: u16,
    y_min: f64,
    y_max: f64,
    x_axis_mode: XAxisMode,
) {
    let tick_h = (y_max - y_min) * 0.18;
    let hz = session.meta.sample_rate as f64;
    let bounds = plot_ui.plot_bounds();
    let x_lo = bounds.min()[0];
    let x_hi = bounds.max()[0];

    for track in session.meta.event_tracks() {
        if !session.event_track_visible.get(&track.name).copied().unwrap_or(true) {
            continue;
        }
        let events = match session.event_cache.get(&(track.name.clone(), ch_idx)) {
            Some(e) => e,
            None => continue,
        };

        let n_labels = track.label_vocabulary.labels.len().max(1);
        let mut by_label: Vec<Vec<f64>> = vec![Vec::new(); n_labels];
        for event in events {
            let lid = event.label_id as usize;
            if lid < n_labels {
                by_label[lid].push(event.sample_offset as f64);
            }
        }

        for (label_id, xs) in by_label.iter().enumerate() {
            if xs.is_empty() { continue; }
            let color = EVENT_PALETTE[label_id % EVENT_PALETTE.len()];
            let label_name = track.label_vocabulary.labels.get(label_id)
                .map(|s| s.as_str())
                .unwrap_or("event");

            let mut pts = Vec::with_capacity(xs.len() * 3);
            for &sample in xs {
                let x = match x_axis_mode {
                    XAxisMode::Samples => sample,
                    XAxisMode::Seconds => sample / hz,
                };
                if x < x_lo || x > x_hi { continue; }
                pts.push([x, y_min]);
                pts.push([x, y_min + tick_h]);
                pts.push([f64::NAN, f64::NAN]);
            }
            plot_ui.line(Line::new(label_name, PlotPoints::from_iter(pts)).color(color).width(1.5));
        }
    }
}

fn draw_channel(
    plot_ui: &mut egui_plot::PlotUi,
    r: &ViewResponse,
    id: &ChannelId,
    color: egui::Color32,
    name: &str,
    x_axis_mode: XAxisMode,
    sample_rate: f32,
) {
    let pos = match r.channels_returned.iter().position(|cid| cid == id) {
        Some(p) => p,
        None => return,
    };
    let p_per_ch = r.points_per_channel;
    let hz = sample_rate as f64;

    let to_x = |sample_idx: u64| match x_axis_mode {
        XAxisMode::Samples => sample_idx as f64,
        XAxisMode::Seconds => sample_idx as f64 / hz,
    };

    if r.lod_level == 0 {
        let chunk = &r.data[pos * p_per_ch..(pos + 1) * p_per_ch];
        let points: PlotPoints = chunk.iter().enumerate().map(|(i, &y)| {
            [to_x(r.actual_start + i as u64), y as f64]
        }).collect();
        plot_ui.line(Line::new(name, points).color(color));
    } else {
        // LOD peaks: isolated vertical bars (NaN separator prevents sawtooth).
        let chunk = &r.data[pos * p_per_ch * 2..(pos + 1) * p_per_ch * 2];
        let ratio = r.decimation_ratio;
        let mut points = Vec::with_capacity(p_per_ch * 3);
        for i in 0..p_per_ch {
            let min = chunk[i * 2];
            let max = chunk[i * 2 + 1];
            let x = to_x(r.actual_start + i as u64 * ratio);
            points.push([x, min as f64]);
            points.push([x, max as f64]);
            points.push([f64::NAN, f64::NAN]);
        }
        plot_ui.line(Line::new(name, PlotPoints::from_iter(points)).color(color));
    }
}
