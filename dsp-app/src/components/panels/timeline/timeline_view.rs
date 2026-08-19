use egui::{Color32, Pos2, PointerButton, Rect, Stroke, Vec2, RichText, Id, Ui, ScrollArea, Painter};
use crate::core::session::{WorkspaceState, XAxisMode};
use crate::core::bridge::{IoBridge, IoRequest};
use super::timeline_state::{TimelineState, TimelineRowState, RowKind};
use super::timeline_view_model::TimelineViewModel;
use dsp_io::recording_meta::Annotation;
use dsp_io::transmission::ui::ViewResponse;
use dsp_io::processing_graph::ChannelId;

pub struct TimelineView;

// ── Layout constants ──────────────────────────────────────────────────────────
const ROW_HEIGHT: f32 = 18.0;
const LEFT_COL_WIDTH: f32 = 180.0;
const RULER_HEIGHT: f32 = 24.0;
const NAV_HEIGHT: f32 = 28.0;
const STATUS_HEIGHT: f32 = 24.0;

// ── Color palette ─────────────────────────────────────────────────────────────
const SEL_FILL: Color32 = Color32::from_rgba_premultiplied(180, 120, 20, 50);
const SEL_BORDER: Color32 = Color32::from_rgb(255, 180, 50);
const VP_FILL: Color32 = Color32::from_rgba_premultiplied(30, 100, 180, 40);
const VP_BORDER: Color32 = Color32::from_rgb(100, 200, 255);
const PLAYHEAD_COLOR: Color32 = Color32::from_rgb(220, 220, 255);

const ANNOTATION_PALETTE: [[u8; 3]; 8] = [
    [230, 159, 0], [86, 180, 233], [0, 158, 115], [240, 228, 66],
    [0, 114, 178], [213, 94, 0], [204, 121, 167], [100, 100, 100],
];

impl TimelineView {
    pub fn show(
        ui: &mut Ui,
        state: &TimelineState,
        workspace: &mut WorkspaceState,
        bridge: &IoBridge,
    ) {
        if state.total_samples == 0 {
            return;
        }

        let view_start = state.view_start;
        let view_width = state.view_width.max(1);
        let x_axis_mode = state.x_axis_mode;
        let sample_rate = state.sample_rate;
        let sr = sample_rate as f64;

        let avail_rect = ui.available_rect_before_wrap();
        let bar_left = avail_rect.left() + LEFT_COL_WIDTH;
        let bar_width = (avail_rect.width() - LEFT_COL_WIDTH).max(1.0);

        let drag_start_id = Id::new("tl_ruler_drag_start");

        ui.vertical(|ui| {
            // ── 1. Controls bar ───────────────────────────────────────────
            Self::show_controls_bar(ui, workspace, state);

            ui.separator();

            // ── 2. Ruler ──────────────────────────────────────────────────
            Self::show_ruler(ui, workspace, state, bar_left, bar_width, drag_start_id);

            // ── 3. Scrollable track rows ──────────────────────────────────
            Self::show_tracks(ui, workspace, state, bridge, bar_left, bar_width);

            // ── 4. Nav zone ───────────────────────────────────────────────
            Self::show_nav_zone(ui, workspace, state, bar_left, bar_width);

            // ── 5. Status / window bar ────────────────────────────────────
            Self::show_status_bar(ui, workspace, state);
        });
    }

    fn show_controls_bar(ui: &mut Ui, workspace: &mut WorkspaceState, state: &TimelineState) {
        ui.horizontal(|ui| {
            ui.set_height(STATUS_HEIGHT);
            let playing = workspace.playback.playing;
            let play_label = if playing { "⏸" } else { "▶" };
            if ui.button("⏮").on_hover_text("Jump to start").clicked() {
                workspace.playhead = 0;
                workspace.playback.playing = false;
            }
            if ui.button(play_label).clicked() {
                workspace.playback.playing = !playing;
                if workspace.playback.step_samples == 1 {
                    workspace.playback.step_samples = state.sample_rate as u64 / 60;
                }
            }
            if ui.button("⏭").on_hover_text("Jump to end").clicked() {
                workspace.playhead = state.total_samples.saturating_sub(1);
                workspace.playback.playing = false;
            }
            let loop_label = if workspace.playback.loop_enabled { "↩ On" } else { "↩ Off" };
            if ui.small_button(loop_label).clicked() {
                workspace.playback.loop_enabled = !workspace.playback.loop_enabled;
            }
            ui.separator();
            egui::ComboBox::from_id_salt("tl_speed")
                .selected_text(format!("{:.2}×", workspace.playback.play_speed))
                .width(60.0)
                .show_ui(ui, |ui| {
                    for &s in &[0.1_f32, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0] {
                        ui.selectable_value(&mut workspace.playback.play_speed, s, format!("{:.2}×", s));
                    }
                });
            ui.separator();
            ui.selectable_value(&mut workspace.x_axis_mode, XAxisMode::Seconds, "Sec");
            ui.selectable_value(&mut workspace.x_axis_mode, XAxisMode::Samples, "Smpl");
            ui.separator();
            
            let ts = match workspace.x_axis_mode {
                XAxisMode::Seconds => {
                    let s = workspace.playhead as f64 / state.sample_rate as f64;
                    let m = (s / 60.0) as u64;
                    let rem = s % 60.0;
                    format!("{m:02}:{rem:06.3}")
                }
                XAxisMode::Samples => format!("{}", workspace.playhead),
            };
            ui.label(RichText::new(ts).monospace().small());

            if let Some([s, e]) = state.selection {
                ui.separator();
                ui.label(RichText::new(format_selection(s, e, state.x_axis_mode, state.sample_rate)).small().color(SEL_BORDER));
                if ui.small_button("✕").clicked() {
                    workspace.set_time_selection(None);
                }
            }
        });
    }

    fn show_ruler(ui: &mut Ui, workspace: &mut WorkspaceState, state: &TimelineState, bar_left: f32, bar_width: f32, drag_start_id: Id) {
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(Vec2::new(LEFT_COL_WIDTH, RULER_HEIGHT), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.set_min_width(LEFT_COL_WIDTH);
                ui.add_space(6.0);
                ui.label(RichText::new("Streams").strong().small());
            });

            let (ruler_rect, resp_ruler) = ui.allocate_exact_size(Vec2::new(bar_width, RULER_HEIGHT), egui::Sense::click_and_drag());
            let painter = ui.painter();
            painter.rect_filled(ruler_rect, 0.0, Color32::from_gray(25));

            let tick_interval = nice_tick_interval(state.view_width, bar_width, state.sample_rate, state.x_axis_mode);
            if tick_interval > 0 {
                let first = state.view_start / tick_interval * tick_interval;
                let last = state.view_start + state.view_width + tick_interval;
                let mut t = first;
                while t <= last {
                    if t >= state.view_start {
                        let x = view_time_to_px(t, state.view_start, state.view_width, bar_left, bar_width);
                        if x >= ruler_rect.left() - 2.0 && x <= ruler_rect.right() + 2.0 {
                            let is_major = (t / tick_interval) % 2 == 0;
                            let tick_h = if is_major { RULER_HEIGHT * 0.45 } else { RULER_HEIGHT * 0.25 };
                            painter.line_segment([Pos2::new(x, ruler_rect.bottom()), Pos2::new(x, ruler_rect.bottom() - tick_h)], Stroke::new(1.0, Color32::from_gray(90)));
                            if is_major {
                                painter.text(Pos2::new(x + 3.0, ruler_rect.top() + 3.0), egui::Align2::LEFT_TOP, format_time(t, state.x_axis_mode, state.sample_rate), egui::FontId::proportional(9.0), Color32::from_gray(160));
                            }
                        }
                    }
                    t = t.saturating_add(tick_interval);
                }
            }

            if let Some([sel_s, sel_e]) = state.selection {
                let lx = view_time_to_px(sel_s, state.view_start, state.view_width, bar_left, bar_width);
                let rx = view_time_to_px(sel_e, state.view_start, state.view_width, bar_left, bar_width);
                let r = Rect::from_min_max(Pos2::new(lx.max(ruler_rect.left()), ruler_rect.top()), Pos2::new(rx.min(ruler_rect.right()), ruler_rect.bottom()));
                if r.is_positive() {
                    painter.rect_filled(r, 0.0, SEL_FILL);
                    painter.rect_stroke(r, 0.0, Stroke::new(1.5, SEL_BORDER), egui::StrokeKind::Outside);
                }
            }

            if workspace.playhead >= state.view_start && workspace.playhead <= state.view_start + state.view_width {
                let px = view_time_to_px(workspace.playhead, state.view_start, state.view_width, bar_left, bar_width);
                painter.vline(px, ruler_rect.y_range(), Stroke::new(1.5, PLAYHEAD_COLOR));
                let pts = vec![Pos2::new(px, ruler_rect.top()), Pos2::new(px + 5.0, ruler_rect.top() + 5.0), Pos2::new(px, ruler_rect.top() + 10.0), Pos2::new(px - 5.0, ruler_rect.top() + 5.0)];
                painter.add(egui::Shape::convex_polygon(pts, PLAYHEAD_COLOR, Stroke::NONE));
            }

            if resp_ruler.drag_started_by(PointerButton::Primary) {
                if let Some(pos) = resp_ruler.interact_pointer_pos() {
                    let t = px_to_view_time(pos.x, state.view_start, state.view_width, bar_left, bar_width);
                    ui.memory_mut(|m| m.data.insert_temp(drag_start_id, t));
                }
            }
            if resp_ruler.dragged_by(PointerButton::Primary) {
                if let Some(pos) = resp_ruler.interact_pointer_pos() {
                    let ds: u64 = ui.memory(|m| m.data.get_temp(drag_start_id).unwrap_or(state.view_start));
                    let cur = px_to_view_time(pos.x, state.view_start, state.view_width, bar_left, bar_width);
                    let (s, e) = if ds <= cur { (ds, cur) } else { (cur, ds) };
                    if e > s { workspace.set_time_selection(Some([s, e])); }
                }
            }
            if resp_ruler.clicked_by(PointerButton::Primary) { workspace.set_time_selection(None); }

            if resp_ruler.hovered() {
                if let Some(dy) = consume_ctrl_scroll(ui) {
                    let ptr_x = ui.input(|i| i.pointer.latest_pos()).map(|p| p.x).unwrap_or(ruler_rect.center().x);
                    apply_scroll_zoom_at(&mut workspace.view_start, &mut workspace.view_width, dy, ptr_x, bar_left, bar_width, state.total_samples);
                }
            }

            if resp_ruler.dragged_by(PointerButton::Middle) {
                let dx = ui.input(|i| i.pointer.delta().x);
                let delta_t = (dx as f64 / bar_width as f64 * state.view_width as f64) as i64;
                let max_start = state.total_samples.saturating_sub(state.view_width) as i64;
                workspace.view_start = ((workspace.view_start as i64 - delta_t).clamp(0, max_start)) as u64;
            }

            if resp_ruler.double_clicked() {
                workspace.view_start = 0;
                workspace.view_width = state.total_samples;
            }

            if let crate::core::session::GlobalSelection::TimeRange([s, e]) = workspace.selection {
                resp_ruler.context_menu(|ui| {
                    if ui.button("Add Annotation…").clicked() {
                        spawn_pending_annotation(workspace, s, e);
                        ui.close();
                    }
                });
            }
        });
    }

    fn show_tracks(ui: &mut Ui, workspace: &mut WorkspaceState, state: &TimelineState, bridge: &IoBridge, bar_left: f32, bar_width: f32) {
        let avail_h = ui.available_height() - NAV_HEIGHT - STATUS_HEIGHT - 12.0;
        let scroll_h = avail_h.max(40.0);

        let (pending_create, pending_cancel) = show_pending_annotation_modal(ui, workspace);

        // Consume Ctrl+scroll before ScrollArea can grab it, then apply zoom
        if ui.rect_contains_pointer(ui.available_rect_before_wrap()) {
            if let Some(dy) = consume_ctrl_scroll(ui) {
                let ptr_x = ui.input(|i| i.pointer.latest_pos()).map(|p| p.x).unwrap_or(bar_left + bar_width * 0.5);
                apply_scroll_zoom_at(&mut workspace.view_start, &mut workspace.view_width, dy, ptr_x, bar_left, bar_width, state.total_samples);
            }
        }

        ScrollArea::vertical().max_height(scroll_h).auto_shrink([false, false]).show(ui, |ui| {
            if state.rows.is_empty() {
                ui.centered_and_justified(|ui| { ui.weak("No recordings loaded"); });
                return;
            }

            for row in &state.rows {
                let track_key = row.kind.track_key();
                ui.horizontal(|ui| {
                    let bg_slot = ui.painter().add(egui::Shape::Noop);
                    ui.allocate_ui_with_layout(Vec2::new(LEFT_COL_WIDTH, ROW_HEIGHT), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.set_min_height(ROW_HEIGHT);
                        ui.add_space(row.depth as f32 * 12.0 + 4.0);

                        match &row.kind {
                            RowKind::RecordingHeader { rec_id } => {
                                let open_id = Id::new(("tl_rec_open", rec_id));
                                let is_open: bool = ui.memory(|m| m.data.get_temp(open_id).unwrap_or(true));
                                let tri = if is_open { "▼" } else { "▶" };
                                if ui.small_button(tri).clicked() { ui.memory_mut(|m| m.data.insert_temp(open_id, !is_open)); }
                                ui.colored_label(row.color, &row.label);
                            }
                            RowKind::AnnotationsHeader { rec_id } => {
                                let hdr_id = Id::new(("tl_anno_open", rec_id.as_str()));
                                let is_open: bool = ui.memory(|m| m.data.get_temp(hdr_id).unwrap_or(true));
                                let tri = if is_open { "▼" } else { "▶" };
                                if ui.small_button(tri).clicked() { ui.memory_mut(|m| m.data.insert_temp(hdr_id, !is_open)); }
                                ui.colored_label(row.color, &row.label);
                            }
                            RowKind::Annotation { rec_id, anno_id } => {
                                let eye = if row.is_visible { "👁" } else { "  " };
                                if ui.small_button(eye).clicked() { toggle_row_visible(workspace, &row.kind, bridge); }
                                let label_resp = ui.colored_label(row.color, &row.label);
                                if label_resp.clicked() { workspace.timeline.selected_track = Some(track_key.clone()); }
                                if ui.small_button("▲").clicked() { if let Some(session) = workspace.recordings.get(rec_id) { move_annotation(session, *anno_id, -1, bridge); } }
                                if ui.small_button("▼").clicked() { if let Some(session) = workspace.recordings.get(rec_id) { move_annotation(session, *anno_id, 1, bridge); } }
                            }
                            _ => {
                                let eye = if row.is_visible { "👁" } else { "  " };
                                if ui.small_button(eye).clicked() { toggle_row_visible(workspace, &row.kind, bridge); }
                                let label_resp = ui.colored_label(row.color, &row.label);
                                if label_resp.clicked() { workspace.timeline.selected_track = Some(track_key.clone()); }
                            }
                        }

                        let remaining = ui.available_width().max(0.0);
                        let (_, left_resp) = ui.allocate_exact_size(Vec2::new(remaining, ROW_HEIGHT), egui::Sense::click());
                        if left_resp.clicked() { workspace.timeline.selected_track = Some(track_key.clone()); }

                        let left_rect = ui.min_rect();
                        let bg = row_bg_color(ui, row.is_selected, left_resp.hovered());
                        if bg != Color32::TRANSPARENT { ui.painter().set(bg_slot, egui::Shape::rect_filled(left_rect, 0.0, bg)); }
                    });

                    let (track_rect, track_resp) = ui.allocate_exact_size(Vec2::new(bar_width, ROW_HEIGHT), egui::Sense::click_and_drag());
                    
                    let mut anno_draw_data: Option<(Annotation, bool)> = None;
                    if let RowKind::Annotation { rec_id, anno_id } = &row.kind {
                        if let Some(session) = workspace.recordings.get(rec_id) {
                            if let Some(a) = session.meta.annotations.iter().find(|a| a.id == *anno_id) {
                                anno_draw_data = Some((a.clone(), workspace.timeline.selected_annotation == Some(a.id)));
                            }
                        }
                    }

                    let playhead_x = if workspace.playhead >= state.view_start && workspace.playhead <= state.view_start + state.view_width {
                        Some(view_time_to_px(workspace.playhead, state.view_start, state.view_width, bar_left, bar_width))
                    } else { None };

                    {
                        let painter = ui.painter();
                        let bg = row_bg_color(ui, row.is_selected, track_resp.hovered());
                        if bg != Color32::TRANSPARENT { painter.rect_filled(track_rect, 0.0, bg); }

                        match &row.kind {
                            RowKind::RecordingHeader { .. } => {
                                painter.hline(track_rect.x_range(), track_rect.center().y, Stroke::new(1.0, Color32::from_gray(40)));
                            }
                            RowKind::AnnotationsHeader { rec_id } => {
                                let hdr_id = Id::new(("tl_anno_open", rec_id.as_str()));
                                let is_open: bool = ui.memory(|m| m.data.get_temp(hdr_id).unwrap_or(true));
                                if is_open {
                                    painter.hline(track_rect.x_range(), track_rect.center().y, Stroke::new(1.0, Color32::from_gray(40)));
                                } else {
                                    if let Some(session) = workspace.recordings.get(rec_id) {
                                        for anno in &session.meta.annotations {
                                            if !anno.visible { continue; }
                                            draw_annotation_span(&painter, track_rect, anno, false, state.view_start, state.view_width, bar_left, bar_width, true);
                                        }
                                    }
                                }
                            }
                            RowKind::StreamsGroup { rec_id } => {
                                if let Some(session) = workspace.recordings.get(rec_id) {
                                    let color = session.display.first().map(|d| d.egui_color()).unwrap_or(row.color);
                                    let lx = view_time_to_px(0, state.view_start, state.view_width, bar_left, bar_width).max(track_rect.left());
                                    let rx = view_time_to_px(session.meta.total_samples, state.view_start, state.view_width, bar_left, bar_width).min(track_rect.right());
                                    if rx > lx {
                                        let r = Rect::from_min_max(Pos2::new(lx, track_rect.top() + 3.0), Pos2::new(rx, track_rect.bottom() - 3.0));
                                        painter.rect_filled(r, 2.0, color.gamma_multiply(0.35));
                                        painter.rect_stroke(r, 2.0, Stroke::new(1.0, color.gamma_multiply(0.7)), egui::StrokeKind::Middle);
                                    }
                                }
                            }
                            RowKind::EpochGroup { rec_id, track_name } => {
                                if let Some(session) = workspace.recordings.get(rec_id) {
                                    if row.is_visible {
                                        if let Some(track) = session.meta.event_tracks().find(|t| &t.name == track_name) {
                                            for &ch_idx in &track.channel_indices {
                                                if let Some(events) = session.event_cache.get(&(track_name.clone(), ch_idx as u16)) {
                                                    draw_event_track_row(&painter, track_rect, events, row.color, state.view_start, state.view_width, bar_left, bar_width);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            RowKind::Annotation { .. } => {
                                if let Some((anno, selected)) = &anno_draw_data {
                                    if anno.visible {
                                        draw_annotation_span(&painter, track_rect, anno, *selected, state.view_start, state.view_width, bar_left, bar_width, false);
                                        if !anno.locked {
                                            let lx = view_time_to_px(anno.start, state.view_start, state.view_width, bar_left, bar_width);
                                            let rx = view_time_to_px(anno.end, state.view_start, state.view_width, bar_left, bar_width);
                                            let hw = 6.0;
                                            let hcol = Color32::from_rgb(anno.color[0], anno.color[1], anno.color[2]).gamma_multiply(1.4);
                                            let lh = Rect::from_min_max(Pos2::new(lx, track_rect.top()), Pos2::new(lx + hw, track_rect.bottom()));
                                            let rh = Rect::from_min_max(Pos2::new(rx - hw, track_rect.top()), Pos2::new(rx, track_rect.bottom()));
                                            if lh.intersects(track_rect) { painter.rect_filled(lh, 0.0, hcol); }
                                            if rh.intersects(track_rect) { painter.rect_filled(rh, 0.0, hcol); }
                                        }
                                    }
                                }
                            }
                        }

                        if let Some(px) = playhead_x {
                            painter.vline(px, track_rect.y_range(), Stroke::new(1.0, PLAYHEAD_COLOR.gamma_multiply(0.6)));
                        }
                    }

                    if let RowKind::Annotation { rec_id, .. } = &row.kind {
                        if let Some((anno, _)) = &anno_draw_data {
                            if !anno.locked {
                                draw_annotation_handles(ui, anno, track_rect, rec_id, state.total_samples, state.view_start, state.view_width, bar_left, bar_width, bridge, workspace);
                            }
                        }
                    }

                    if track_resp.clicked_by(PointerButton::Primary) {
                        if let Some(pos) = track_resp.interact_pointer_pos() {
                            workspace.playhead = px_to_view_time(pos.x, state.view_start, state.view_width, bar_left, bar_width);
                            workspace.timeline.selected_track = Some(track_key.clone());
                        }
                    }


                    if track_resp.dragged_by(PointerButton::Middle) {
                        let dx = ui.input(|i| i.pointer.delta().x);
                        let delta_t = (dx as f64 / bar_width as f64 * state.view_width as f64) as i64;
                        let max_start = state.total_samples.saturating_sub(state.view_width) as i64;
                        workspace.view_start = ((workspace.view_start as i64 - delta_t).clamp(0, max_start)) as u64;
                    }
                });
            }
        });

        if pending_create {
            let pending = workspace.timeline.pending_annotation.take().unwrap();
            if let Some(id) = workspace.active_recording_id.clone() {
                if let Some(session) = workspace.recordings.get(&id) {
                    let mut anno = pending;
                    anno.label = auto_suffix_label(&anno.label, &session.meta.annotations.iter().map(|a| a.label.clone()).collect::<Vec<_>>());
                    let mut meta = session.meta.clone();
                    meta.annotations.push(anno);
                    bridge.send(IoRequest::SaveRecordingMeta { zarr_path: session.zarr_path.clone(), meta });
                }
            }
        } else if pending_cancel {
            workspace.timeline.pending_annotation = None;
        }

        show_annotation_properties(ui, workspace, state.sample_rate, bridge);
    }

    fn show_nav_zone(ui: &mut Ui, workspace: &mut WorkspaceState, state: &TimelineState, bar_left: f32, bar_width: f32) {
        let (nav_rect, resp_nav) = ui.allocate_exact_size(Vec2::new(ui.available_width(), NAV_HEIGHT), egui::Sense::click_and_drag());
        let painter = ui.painter();
        painter.rect_filled(nav_rect, 0.0, Color32::from_gray(18));

        for session in workspace.recordings.values() {
            let lx = total_time_to_px(0, state.total_samples, bar_left, bar_width);
            let rx = total_time_to_px(session.meta.total_samples, state.total_samples, bar_left, bar_width);
            let bar_rect = Rect::from_min_max(Pos2::new(lx, nav_rect.center().y - 3.0), Pos2::new(rx, nav_rect.center().y + 3.0));
            let color = session.display.first().map(|d| d.egui_color()).unwrap_or(Color32::from_gray(80));
            painter.rect_filled(bar_rect, 1.0, color.gamma_multiply(0.3));
        }

        if let Some([sel_s, sel_e]) = state.selection {
            let lx = total_time_to_px(sel_s, state.total_samples, bar_left, bar_width);
            let rx = total_time_to_px(sel_e, state.total_samples, bar_left, bar_width);
            let r = Rect::from_min_max(Pos2::new(lx, nav_rect.top()), Pos2::new(rx, nav_rect.bottom()));
            painter.rect_filled(r, 0.0, SEL_FILL);
        }

        let vp_l = total_time_to_px(state.view_start, state.total_samples, bar_left, bar_width);
        let vp_r = total_time_to_px(state.view_start + state.view_width, state.total_samples, bar_left, bar_width);
        let vp_rect = Rect::from_min_max(Pos2::new(vp_l, nav_rect.top()), Pos2::new(vp_r, nav_rect.bottom()));
        painter.rect_filled(vp_rect, 0.0, VP_FILL);
        painter.rect_stroke(vp_rect, 0.0, Stroke::new(1.5, VP_BORDER), egui::StrokeKind::Middle);

        let ph_x = total_time_to_px(workspace.playhead, state.total_samples, bar_left, bar_width);
        painter.vline(ph_x, nav_rect.y_range(), Stroke::new(1.0, PLAYHEAD_COLOR));

        if resp_nav.dragged_by(PointerButton::Primary) {
            if let Some(pos) = resp_nav.interact_pointer_pos() {
                let target = px_to_total_time(pos.x, state.total_samples, bar_left, bar_width);
                let half_w = state.view_width / 2;
                workspace.view_start = target.saturating_sub(half_w).min(state.total_samples.saturating_sub(state.view_width));
            }
        }

        if resp_nav.hovered() {
            if let Some(dy) = consume_ctrl_scroll(ui) {
                let ptr_x = ui.input(|i| i.pointer.latest_pos()).map(|p| p.x).unwrap_or(nav_rect.center().x);
                apply_scroll_zoom_at(&mut workspace.view_start, &mut workspace.view_width, dy, ptr_x, bar_left, bar_width, state.total_samples);
            }
        }
    }

    fn show_status_bar(ui: &mut Ui, workspace: &mut WorkspaceState, state: &TimelineState) {
        ui.horizontal(|ui| {
            ui.set_height(STATUS_HEIGHT);
            ui.label(RichText::new("Window:").small());
            let mut w = workspace.view_width as f64;
            if ui.add(egui::DragValue::new(&mut w).speed(100.0).range(100.0..=(state.total_samples as f64))).changed() {
                workspace.view_width = w as u64;
            }
        });
        let rem = ui.available_size_before_wrap();
        if rem.y > 0.0 { ui.allocate_space(rem); }
    }
}

// ── Copy of private helpers from original mod.rs ──────────────────────────────
fn format_time(sample: u64, mode: XAxisMode, sample_rate: f32) -> String {
    match mode {
        XAxisMode::Samples => format!("{sample}"),
        XAxisMode::Seconds => {
            let s = sample as f64 / sample_rate as f64;
            if s < 60.0 { format!("{s:.2}s") }
            else { let m = (s / 60.0) as u64; let rem = s % 60.0; format!("{m}m{rem:.1}s") }
        }
    }
}

fn format_selection(s: u64, e: u64, mode: XAxisMode, sample_rate: f32) -> String {
    let sr = sample_rate as f64;
    let dur = e.saturating_sub(s);
    match mode {
        XAxisMode::Samples => format!("Sel: {} – {} ({} smpl)", s, e, dur),
        XAxisMode::Seconds => {
            let s_s = s as f64 / sr; let e_s = e as f64 / sr; let d_s = dur as f64 / sr;
            format!("Sel: {:.2}s – {:.2}s ({:.2}s)", s_s, e_s, d_s)
        }
    }
}

fn row_bg_color(ui: &egui::Ui, is_selected: bool, is_hovered: bool) -> Color32 {
    if is_selected { ui.visuals().selection.bg_fill.gamma_multiply(0.4) }
    else if is_hovered { ui.visuals().widgets.hovered.weak_bg_fill.gamma_multiply(0.3) }
    else { Color32::TRANSPARENT }
}

fn view_time_to_px(sample: u64, view_start: u64, view_width: u64, bar_left: f32, bar_width: f32) -> f32 {
    if view_width == 0 { return bar_left; }
    bar_left + sample.saturating_sub(view_start) as f32 / view_width as f32 * bar_width
}

fn px_to_view_time(px: f32, view_start: u64, view_width: u64, bar_left: f32, bar_width: f32) -> u64 {
    if bar_width <= 0.0 || view_width == 0 { return view_start; }
    let ratio = ((px - bar_left) / bar_width).clamp(0.0, 1.0) as f64;
    view_start + (ratio * view_width as f64) as u64
}

fn total_time_to_px(sample: u64, total_samples: u64, bar_left: f32, bar_width: f32) -> f32 {
    if total_samples == 0 { return bar_left; }
    bar_left + sample as f32 / total_samples as f32 * bar_width
}

fn px_to_total_time(px: f32, total_samples: u64, bar_left: f32, bar_width: f32) -> u64 {
    if bar_width <= 0.0 { return 0; }
    let ratio = ((px - bar_left) / bar_width).clamp(0.0, 1.0) as f64;
    (ratio * total_samples as f64) as u64
}

fn nice_tick_interval(view_width: u64, ruler_px: f32, sample_rate: f32, mode: XAxisMode) -> u64 {
    const MIN_TICK_PX: f32 = 60.0;
    if ruler_px <= 0.0 || view_width == 0 { return view_width.max(1) / 4; }
    let samples_per_px = view_width as f64 / ruler_px as f64;
    let min_samples = (MIN_TICK_PX as f64 * samples_per_px).max(1.0) as u64;
    const CANDIDATES: &[u64] = &[1, 2, 5, 10, 20, 50, 100, 200, 500, 1_000, 2_000, 5_000, 10_000, 20_000, 50_000, 100_000, 200_000, 500_000, 1_000_000, 2_000_000, 5_000_000, 10_000_000, 50_000_000, 100_000_000];
    for &c in CANDIDATES {
        let s = if mode == XAxisMode::Seconds { (c as f64 * sample_rate as f64) as u64 } else { c };
        if s >= min_samples { return s.max(1); }
    }
    view_width / 2
}

/// Drains Ctrl+scroll wheel events this frame and returns the normalised delta (None if no Ctrl or no scroll).
/// Removing the events prevents `ScrollArea` from also scrolling the track list.
fn consume_ctrl_scroll(ui: &egui::Ui) -> Option<f32> {
    ui.input_mut(|i| {
        if !(i.modifiers.ctrl || i.modifiers.command || i.modifiers.mac_cmd) {
            return None;
        }
        let mut dy = 0.0f32;
        i.events.retain(|e| {
            if let egui::Event::MouseWheel { unit, delta, .. } = e {
                let scale = match unit {
                    egui::MouseWheelUnit::Line => 50.0,
                    egui::MouseWheelUnit::Point => 1.0,
                    egui::MouseWheelUnit::Page => 300.0,
                };
                dy += delta.y * scale;
                false
            } else {
                true
            }
        });
        // Also zero the smooth delta so the ScrollArea doesn't scroll the tracks.
        i.smooth_scroll_delta.y = 0.0;
        if dy != 0.0 { Some(dy) } else { None }
    })
}

fn apply_scroll_zoom_at(view_start: &mut u64, view_width: &mut u64, scroll_dy: f32, pointer_x: f32, bar_left: f32, bar_width: f32, total_samples: u64) {
    let zoom = (scroll_dy * 0.002_f32).exp() as f64;
    let ratio = ((pointer_x - bar_left) / bar_width).clamp(0.0, 1.0) as f64;
    let pointer_sample = *view_start + (ratio * *view_width as f64) as u64;
    let new_width = ((*view_width as f64 / zoom) as u64).clamp(100, total_samples);
    let new_start = pointer_sample.saturating_sub((ratio * new_width as f64) as u64);
    *view_start = new_start.min(total_samples.saturating_sub(new_width));
    *view_width = new_width;
}

fn spawn_pending_annotation(workspace: &mut WorkspaceState, s: u64, e: u64) {
    let id = workspace.recordings.values().flat_map(|s| s.meta.annotations.iter().map(|a| a.id)).max().unwrap_or(0) + 1;
    let color_idx = workspace.recordings.values().flat_map(|s| s.meta.annotations.iter()).count() % ANNOTATION_PALETTE.len();
    let row_index = workspace.recordings.values().flat_map(|s| s.meta.annotations.iter().map(|a| a.row_index)).max().map(|m| m + 1).unwrap_or(0);
    workspace.timeline.pending_annotation = Some(Annotation { id, label: "annotation".to_string(), start: s, end: e, color: ANNOTATION_PALETTE[color_idx], visible: true, locked: false, row_index });
}

fn show_pending_annotation_modal(ui: &mut Ui, workspace: &mut WorkspaceState) -> (bool, bool) {
    let mut create = false;
    let mut cancel = false;
    if let Some(pending) = &mut workspace.timeline.pending_annotation {
        let mut label = pending.label.clone();
        egui::Window::new("New Annotation").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).show(ui.ctx(), |ui| {
            ui.label("Name:");
            let resp = ui.text_edit_singleline(&mut label);
            resp.request_focus();
            ui.horizontal(|ui| {
                let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.button("Create").clicked() || enter { create = true; }
                if ui.button("Cancel").clicked() { cancel = true; }
            });
        });
        pending.label = label;
    }
    (create, cancel)
}

fn show_annotation_properties(ui: &mut Ui, workspace: &mut WorkspaceState, sample_rate: f32, bridge: &IoBridge) {
    let sr = sample_rate as f64;
    let Some(selected_id) = workspace.timeline.selected_annotation else { return; };
    let mut found = None;
    for (_, session) in &workspace.recordings {
        if let Some(anno) = session.meta.annotations.iter().find(|a| a.id == selected_id) {
            found = Some((anno.clone(), session.zarr_path.clone(), session.meta.clone()));
            break;
        }
    }
    let Some((mut anno, zarr_path, meta)) = found else { return; };
    let mut open = true;
    let mut changed = false;
    egui::Window::new(format!("Annotation: {}", anno.label)).open(&mut open).collapsible(false).show(ui.ctx(), |ui| {
        egui::Grid::new("anno_props_grid").show(ui, |ui| {
            ui.label("Label:"); if ui.text_edit_singleline(&mut anno.label).changed() { changed = true; } ui.end_row();
            ui.label("Start:");
            if workspace.x_axis_mode == XAxisMode::Seconds {
                let mut v = anno.start as f64 / sr; if ui.add(egui::DragValue::new(&mut v).speed(0.1).suffix("s")).changed() { anno.start = (v * sr) as u64; changed = true; }
            } else if ui.add(egui::DragValue::new(&mut anno.start).speed(10.0)).changed() { changed = true; }
            ui.end_row();
            ui.label("End:");
            if workspace.x_axis_mode == XAxisMode::Seconds {
                let mut v = anno.end as f64 / sr; if ui.add(egui::DragValue::new(&mut v).speed(0.1).suffix("s")).changed() { anno.end = (v * sr) as u64; changed = true; }
            } else if ui.add(egui::DragValue::new(&mut anno.end).speed(10.0)).changed() { changed = true; }
            ui.end_row();
            ui.label("Color:"); let mut col = anno.color; if ui.color_edit_button_srgb(&mut col).changed() { anno.color = col; changed = true; } ui.end_row();
        });
    });
    if changed {
        let mut new_meta = meta;
        if let Some(a) = new_meta.annotations.iter_mut().find(|a| a.id == selected_id) { *a = anno; }
        bridge.send(IoRequest::SaveRecordingMeta { zarr_path, meta: new_meta });
    }
    if !open { workspace.timeline.selected_annotation = None; }
}

fn toggle_row_visible(workspace: &mut WorkspaceState, kind: &RowKind, bridge: &IoBridge) {
    match kind {
        RowKind::StreamsGroup { rec_id } => {
            if let Some(session) = workspace.recordings.get_mut(rec_id) {
                let any_visible = session.display.iter().any(|d| d.visible) || session.virtual_display.iter().any(|d| d.visible);
                let new_vis = !any_visible;
                for d in &mut session.display { d.visible = new_vis; }
                for d in &mut session.virtual_display { d.visible = new_vis; }
            }
        }
        RowKind::EpochGroup { rec_id, track_name } => {
            if let Some(session) = workspace.recordings.get_mut(rec_id) {
                let v = session.event_track_visible.entry(track_name.clone()).or_insert(true);
                *v = !*v;
            }
        }
        RowKind::Annotation { rec_id, anno_id } => {
            if let Some(session) = workspace.recordings.get(rec_id) {
                let mut meta = session.meta.clone();
                if let Some(a) = meta.annotations.iter_mut().find(|a| a.id == *anno_id) {
                    a.visible = !a.visible;
                    bridge.send(IoRequest::SaveRecordingMeta { zarr_path: session.zarr_path.clone(), meta });
                }
            }
        }
        _ => {}
    }
}

fn move_annotation(session: &crate::core::session::RecordingSession, id: u64, delta: i32, bridge: &IoBridge) {
    let mut meta = session.meta.clone();
    if let Some(i) = meta.annotations.iter().position(|a| a.id == id) {
        let current_row = meta.annotations[i].row_index;
        let target_row = (current_row as i32 + delta).max(0) as usize;
        if let Some(j) = meta.annotations.iter().position(|a| a.row_index == target_row && a.id != id) { meta.annotations[j].row_index = current_row; }
        meta.annotations[i].row_index = target_row;
        bridge.send(IoRequest::SaveRecordingMeta { zarr_path: session.zarr_path.clone(), meta });
    }
}

fn auto_suffix_label(label: &str, existing: &[String]) -> String {
    if !existing.contains(&label.to_string()) { return label.to_string(); }
    let mut candidate = format!("{}_1", label);
    let mut n = 1u32;
    while existing.contains(&candidate) { n += 1; candidate = format!("{}_{}", label, n); }
    candidate
}

fn draw_event_track_row(painter: &Painter, rect: Rect, events: &[dsp_core::signal::Event], color: Color32, view_start: u64, view_width: u64, bar_left: f32, bar_width: f32) {
    if view_width == 0 { return; }
    let px_per_sample = bar_width / view_width as f32;
    for event in events {
        if event.sample_offset < view_start || event.sample_offset > view_start + view_width { continue; }
        let x = bar_left + (event.sample_offset - view_start) as f32 * px_per_sample;
        painter.vline(x, rect.y_range(), Stroke::new(1.5, color));
    }
}

fn draw_annotation_span(painter: &Painter, rect: Rect, anno: &Annotation, is_selected: bool, view_start: u64, view_width: u64, bar_left: f32, bar_width: f32, transparent: bool) {
    let lx = view_time_to_px(anno.start, view_start, view_width, bar_left, bar_width);
    let rx = view_time_to_px(anno.end, view_start, view_width, bar_left, bar_width);
    if rx < rect.left() || lx > rect.right() { return; }
    let lx = lx.max(rect.left()); let rx = rx.min(rect.right());
    let color = Color32::from_rgb(anno.color[0], anno.color[1], anno.color[2]);
    let span = Rect::from_min_max(Pos2::new(lx, rect.top() + 2.0), Pos2::new(rx.max(lx + 2.0), rect.bottom() - 2.0));
    let alpha = if transparent { 0.3 } else { 0.6 };
    painter.rect_filled(span, 2.0, color.gamma_multiply(alpha));
    if is_selected { painter.rect_stroke(span, 2.0, Stroke::new(1.5, Color32::WHITE), egui::StrokeKind::Outside); }
}

fn draw_annotation_handles(ui: &mut Ui, anno: &Annotation, row_rect: Rect, rec_id: &str, total_samples: u64, view_start: u64, view_width: u64, bar_left: f32, bar_width: f32, bridge: &IoBridge, workspace: &mut WorkspaceState) {
    let lx = view_time_to_px(anno.start, view_start, view_width, bar_left, bar_width);
    let rx = view_time_to_px(anno.end, view_start, view_width, bar_left, bar_width);
    let handle_w = 6.0;
    let color = Color32::from_rgb(anno.color[0], anno.color[1], anno.color[2]);
    let left_h = Rect::from_min_max(Pos2::new(lx, row_rect.top()), Pos2::new(lx + handle_w, row_rect.bottom()));
    let right_h = Rect::from_min_max(Pos2::new(rx - handle_w, row_rect.top()), Pos2::new(rx, row_rect.bottom()));
    if left_h.intersects(row_rect) { ui.painter().rect_filled(left_h, 0.0, color.gamma_multiply(1.4)); }
    if right_h.intersects(row_rect) { ui.painter().rect_filled(right_h, 0.0, color.gamma_multiply(1.4)); }
    let left_id = Id::new(("anno_left", anno.id));
    let lr = ui.interact(left_h, left_id, egui::Sense::drag());
    if lr.dragged() {
        let dx = ui.input(|i| i.pointer.delta().x);
        let dt = (dx as f64 / bar_width as f64 * view_width as f64) as i64;
        if let Some(session) = workspace.recordings.get(rec_id) {
            let mut meta = session.meta.clone();
            if let Some(a) = meta.annotations.iter_mut().find(|a| a.id == anno.id) {
                a.start = (a.start as i64 + dt).clamp(0, a.end as i64 - 1) as u64;
                bridge.send(IoRequest::SaveRecordingMeta { zarr_path: session.zarr_path.clone(), meta });
            }
        }
    }
    let right_id = Id::new(("anno_right", anno.id));
    let rr = ui.interact(right_h, right_id, egui::Sense::drag());
    if rr.dragged() {
        let dx = ui.input(|i| i.pointer.delta().x);
        let dt = (dx as f64 / bar_width as f64 * view_width as f64) as i64;
        if let Some(session) = workspace.recordings.get(rec_id) {
            let mut meta = session.meta.clone();
            if let Some(a) = meta.annotations.iter_mut().find(|a| a.id == anno.id) {
                a.end = (a.end as i64 + dt).clamp(a.start as i64 + 1, total_samples as i64) as u64;
                bridge.send(IoRequest::SaveRecordingMeta { zarr_path: session.zarr_path.clone(), meta });
            }
        }
    }
}
