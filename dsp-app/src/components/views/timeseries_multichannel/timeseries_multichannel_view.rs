use egui::{Ui, Color32, Rect, Pos2, Vec2, Stroke};
use crate::core::session::WorkspaceState;
use crate::core::bridge::IoBridge;
use crate::blueprint::pane::YAxisRange;
use dsp_io::processing_graph::ChannelId;
use dsp_io::transmission::ui::ViewResponse;
use super::timeseries_multichannel_state::{TimeseriesMultichannelState, TimeseriesStatus, TimeseriesFocusMode};
use super::timeseries_multichannel_view_model::TimeseriesMultichannelViewModel;

pub struct TimeseriesMultichannelView;

const LEFT_COL_WIDTH: f32 = 180.0;
const SEL_FILL: Color32 = Color32::from_rgba_premultiplied(180, 120, 20, 50);
const PLAYHEAD_COLOR: Color32 = Color32::from_rgb(220, 220, 255);

impl TimeseriesMultichannelView {
    pub fn new() -> Self {
        Self
    }

    pub fn show(
        ui: &mut Ui,
        workspace: &mut WorkspaceState,
        _bridge: &IoBridge,
        row_height: f32,
        y_range: YAxisRange,
        focus_mode: &mut TimeseriesFocusMode,
    ) {
        let state = TimeseriesMultichannelViewModel::prepare_state(workspace, y_range, focus_mode.clone());
        Self::ui(ui, workspace, state, row_height, focus_mode);
    }

    pub fn ui(
        ui: &mut Ui, 
        workspace: &mut WorkspaceState, 
        state: TimeseriesMultichannelState, 
        row_height: f32,
        focus_mode_out: &mut TimeseriesFocusMode,
    ) {
        match state.status {
            TimeseriesStatus::NoRecordings => {
                ui.centered_and_justified(|ui| { ui.weak("No recordings loaded."); });
            }
            TimeseriesStatus::Empty => {
                ui.centered_and_justified(|ui| { ui.weak("No data to display."); });
            }
            TimeseriesStatus::Ready => {
                if let TimeseriesFocusMode::Channel { rec_id, channel_id } = &state.focus_mode {
                    Self::show_focused(ui, workspace, &state, rec_id, channel_id, focus_mode_out);
                } else {
                    Self::show_stacked(ui, workspace, &state, row_height, focus_mode_out);
                }
            }
        }
    }

    fn show_stacked(
        ui: &mut Ui,
        workspace: &mut WorkspaceState,
        state: &TimeseriesMultichannelState,
        row_height: f32,
        focus_mode_out: &mut TimeseriesFocusMode,
    ) {
        let avail_rect = ui.available_rect_before_wrap();
        let bar_left = avail_rect.left() + LEFT_COL_WIDTH;
        let bar_width = (avail_rect.width() - LEFT_COL_WIDTH).max(1.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for row in &state.rows {
                    ui.horizontal(|ui| {
                        // ── Left col: eye toggle + label ─────────────────
                        ui.allocate_ui_with_layout(
                            Vec2::new(LEFT_COL_WIDTH, row_height),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.set_min_height(row_height);
                                ui.add_space(4.0);
                                let eye = if row.visible { "👁" } else { "  " };
                                if ui.small_button(eye).clicked() {
                                    TimeseriesMultichannelViewModel::toggle_visibility(workspace, &row.rec_id, &row.channel_id);
                                }
                                ui.colored_label(row.color, &row.label);
                            },
                        );

                        // ── Right col: waveform ───────────────────────────
                        let (track_rect, _track_resp) = ui.allocate_exact_size(
                            Vec2::new(bar_width, row_height),
                            egui::Sense::click(),
                        );

                        let painter = ui.painter();
                        painter.rect_filled(track_rect, 0.0, Color32::from_gray(20));

                        // Zero line
                        let zero_y = Self::val_to_y(0.0, track_rect, state.y_min, state.y_max);
                        painter.hline(track_rect.x_range(), zero_y, Stroke::new(0.5, Color32::from_gray(45)));

                        // Waveform — resident coarse overview first (dimmed), then
                        // the detailed cache on top so it sharpens in place.
                        if let Some(session) = workspace.recordings.get(&row.rec_id) {
                            let detail_cover = session.cache.as_ref()
                                .map(|c| (c.actual_start, c.actual_start + c.points_per_channel as u64 * c.decimation_ratio));
                            if let Some(ov) = &session.overview {
                                Self::draw_channel_track(
                                    painter, track_rect, ov, &row.channel_id, row.color,
                                    state.view_start, state.view_width, bar_left, bar_width,
                                    state.y_min, state.y_max, true, detail_cover,
                                );
                            }
                            if let Some(cache) = &session.cache {
                                Self::draw_channel_track(
                                    painter, track_rect, cache, &row.channel_id, row.color,
                                    state.view_start, state.view_width, bar_left, bar_width,
                                    state.y_min, state.y_max, false, None,
                                );
                            }
                        }

                        // Playhead
                        if let Some(ph) = state.playhead {
                            let ph_x = Self::view_time_to_px(ph, state.view_start, state.view_width, bar_left, bar_width);
                            painter.vline(ph_x, track_rect.y_range(), Stroke::new(1.0, PLAYHEAD_COLOR));
                        }

                        // Selection overlay
                        if let Some([sel_s, sel_e]) = state.selection {
                            let lx = Self::view_time_to_px(sel_s, state.view_start, state.view_width, bar_left, bar_width);
                            let rx = Self::view_time_to_px(sel_e, state.view_start, state.view_width, bar_left, bar_width);
                            let r = Rect::from_min_max(
                                Pos2::new(lx.max(track_rect.left()), track_rect.top()),
                                Pos2::new(rx.min(track_rect.right()), track_rect.bottom()),
                            );
                            if r.is_positive() {
                                painter.rect_filled(r, 0.0, SEL_FILL);
                            }
                        }

                        if _track_resp.double_clicked() || _track_resp.secondary_clicked() {
                            *focus_mode_out = TimeseriesFocusMode::Channel { 
                                rec_id: row.rec_id.clone(), 
                                channel_id: row.channel_id.clone() 
                            };
                        }
                    });
                    ui.separator();
                }
            });
    }

    fn show_focused(
        ui: &mut Ui,
        workspace: &mut WorkspaceState,
        state: &TimeseriesMultichannelState,
        rec_id: &str,
        channel_id: &ChannelId,
        focus_mode_out: &mut TimeseriesFocusMode,
    ) {
        let row = match state.rows.iter().find(|r| &r.rec_id == rec_id && &r.channel_id == channel_id) {
            Some(r) => r,
            None => { *focus_mode_out = TimeseriesFocusMode::None; return; }
        };

        let avail_rect = ui.available_rect_before_wrap();
        let bar_left = avail_rect.left();
        let bar_width = avail_rect.width().max(1.0);
        let height = avail_rect.height();

        let (track_rect, _track_resp) = ui.allocate_exact_size(
            Vec2::new(bar_width, height),
            egui::Sense::click(),
        );

        let painter = ui.painter();
        painter.rect_filled(track_rect, 0.0, Color32::from_gray(20));

        // Zero line
        let zero_y = Self::val_to_y(0.0, track_rect, state.y_min, state.y_max);
        painter.hline(track_rect.x_range(), zero_y, Stroke::new(0.5, Color32::from_gray(45)));

        // Waveform — coarse overview first (dimmed), detailed cache on top.
        if let Some(session) = workspace.recordings.get(&row.rec_id) {
            let detail_cover = session.cache.as_ref()
                .map(|c| (c.actual_start, c.actual_start + c.points_per_channel as u64 * c.decimation_ratio));
            if let Some(ov) = &session.overview {
                Self::draw_channel_track(
                    painter, track_rect, ov, &row.channel_id, row.color,
                    state.view_start, state.view_width, bar_left, bar_width,
                    state.y_min, state.y_max, true, detail_cover,
                );
            }
            if let Some(cache) = &session.cache {
                Self::draw_channel_track(
                    painter, track_rect, cache, &row.channel_id, row.color,
                    state.view_start, state.view_width, bar_left, bar_width,
                    state.y_min, state.y_max, false, None,
                );
            }
        }

        // Interaction
        if _track_resp.double_clicked() || _track_resp.secondary_clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            *focus_mode_out = TimeseriesFocusMode::None;
        }

        // Label overlay
        painter.text(
            track_rect.left_top() + Vec2::new(10.0, 10.0),
            egui::Align2::LEFT_TOP,
            &row.label,
            egui::FontId::proportional(16.0),
            row.color
        );
    }

    fn val_to_y(v: f32, rect: Rect, y_min: f32, y_max: f32) -> f32 {
        let y_span = y_max - y_min;
        let t = if y_span.abs() > 1e-9 {
            ((v - y_min) / y_span).clamp(0.0, 1.0)
        } else {
            0.5
        };
        rect.top() + (1.0 - t) * rect.height()
    }

    fn view_time_to_px(sample: u64, view_start: u64, view_width: u64, bar_left: f32, bar_width: f32) -> f32 {
        if view_width == 0 { return bar_left; }
        bar_left + sample.saturating_sub(view_start) as f32 / view_width as f32 * bar_width
    }

    fn draw_channel_track(
        painter: &egui::Painter,
        rect: Rect,
        cache: &ViewResponse,
        channel_id: &ChannelId,
        color: Color32,
        view_start: u64,
        view_width: u64,
        bar_left: f32,
        bar_width: f32,
        y_min: f32,
        y_max: f32,
        dim: bool,
        skip: Option<(u64, u64)>,
    ) {
        // The resident overview is drawn dimmed underneath the detail layer so it
        // reads as a preview that sharpens when the detailed cache arrives.
        let color = if dim { color.gamma_multiply(0.5) } else { color };
        let Some(ch_pos) = cache.channels_returned.iter().position(|id| id == channel_id) else {
            return;
        };

        let pts = cache.points_per_channel;
        let ratio = cache.decimation_ratio;
        let px_per_sample = bar_width / view_width as f32;

        if cache.lod_level > 0 {
            let start = ch_pos * pts * 2;
            let end = (start + pts * 2).min(cache.data.len());
            if start >= cache.data.len() { return; }
            let data = &cache.data[start..end];
            let actual_pts = data.len() / 2;

            for i in 0..actual_pts {
                let sample = cache.actual_start + i as u64 * ratio;
                if sample + ratio < view_start || sample > view_start + view_width { continue; }
                // Skip points already covered by the detail layer (overview only fills gaps).
                if let Some((ss, se)) = skip { if sample + ratio > ss && sample < se { continue; } }
                let x = bar_left + sample.saturating_sub(view_start) as f32 * px_per_sample;
                let y_top = Self::val_to_y(data[i * 2 + 1], rect, y_min, y_max);
                let y_bot = Self::val_to_y(data[i * 2], rect, y_min, y_max);
                let w = (px_per_sample * ratio as f32).clamp(1.0, 3.0);
                let r = Rect::from_min_max(
                    Pos2::new(x.max(rect.left()), y_top.min(y_bot)),
                    Pos2::new(
                        (x + w).min(rect.right()),
                        y_top.max(y_bot).max(y_top.min(y_bot) + 1.0),
                    ),
                );
                painter.rect_filled(r, 0.0, color);
            }
        } else {
            let start = ch_pos * pts;
            let end = (start + pts).min(cache.data.len());
            if start >= cache.data.len() { return; }
            let data = &cache.data[start..end];

            let pts_vec: Vec<Pos2> = data.iter().enumerate().filter_map(|(i, &v)| {
                let sample = cache.actual_start + i as u64;
                if sample < view_start || sample > view_start + view_width { return None; }
                if let Some((ss, se)) = skip { if sample >= ss && sample < se { return None; } }
                let x = bar_left + (sample - view_start) as f32 * px_per_sample;
                Some(Pos2::new(x, Self::val_to_y(v, rect, y_min, y_max)))
            }).collect();

            if pts_vec.len() >= 2 {
                painter.add(egui::Shape::line(pts_vec, Stroke::new(1.0, color)));
            }
        }
    }
}
