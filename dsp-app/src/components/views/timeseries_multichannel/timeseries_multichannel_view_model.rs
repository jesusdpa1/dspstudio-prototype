use crate::core::session::WorkspaceState;
use crate::blueprint::pane::YAxisRange;
use dsp_io::processing_graph::ChannelId;
use super::timeseries_multichannel_state::{TimeseriesMultichannelState, TimeseriesStatus, StreamRowData, TimeseriesFocusMode};

pub struct TimeseriesMultichannelViewModel;

impl TimeseriesMultichannelViewModel {
    pub fn prepare_state(
        workspace: &WorkspaceState, 
        y_range_config: YAxisRange, 
        focus_mode: TimeseriesFocusMode
    ) -> TimeseriesMultichannelState {
        let total_samples = workspace.recordings.values()
            .map(|s| s.meta.total_samples)
            .max()
            .unwrap_or(0);

        if total_samples == 0 {
            return TimeseriesMultichannelState { status: TimeseriesStatus::NoRecordings, ..Default::default() };
        }

        let mut rows = Vec::new();
        for (rec_id, session) in &workspace.recordings {
            for (ch_idx, d) in session.display.iter().enumerate() {
                let label = session.meta.channel_names
                    .get(ch_idx)
                    .cloned()
                    .unwrap_or_else(|| format!("CH{}", ch_idx));
                rows.push(StreamRowData {
                    rec_id: rec_id.clone(),
                    channel_id: ChannelId::Physical(ch_idx as u16),
                    label,
                    color: d.egui_color(),
                    visible: d.visible,
                });
            }
            for (vc_idx, vc) in session.meta.virtual_channels.iter().enumerate() {
                let d = &session.virtual_display[vc_idx];
                rows.push(StreamRowData {
                    rec_id: rec_id.clone(),
                    channel_id: ChannelId::Virtual(vc.name.clone()),
                    label: format!("~ {}", vc.name),
                    color: d.egui_color(),
                    visible: d.visible,
                });
            }
        }

        let (y_min, y_max) = match y_range_config {
            YAxisRange::Auto => Self::compute_auto_y_range(workspace, &rows),
            YAxisRange::Manual { min, max } => {
                let lo = min.min(max);
                let hi = min.max(max);
                if (hi - lo).abs() < 1e-9 { (lo - 1.0, hi + 1.0) } else { (lo, hi) }
            }
        };

        TimeseriesMultichannelState {
            rows,
            status: TimeseriesStatus::Ready,
            y_min,
            y_max,
            view_start: workspace.view_start,
            view_width: workspace.view_width.max(1),
            playhead: if workspace.playhead >= workspace.view_start && workspace.playhead <= workspace.view_start + workspace.view_width {
                Some(workspace.playhead)
            } else {
                None
            },
            selection: match &workspace.selection {
                crate::core::session::GlobalSelection::TimeRange(r) => Some(*r),
                _ => None,
            },
            focus_mode,
        }
    }

    fn compute_auto_y_range(workspace: &WorkspaceState, rows: &[StreamRowData]) -> (f32, f32) {
        let mut global_max: f32 = 0.0;

        for row in rows {
            let Some(session) = workspace.recordings.get(&row.rec_id) else { continue; };
            let Some(cache) = &session.cache else { continue; };
            let Some(ch_pos) = cache.channels_returned.iter().position(|id| id == &row.channel_id) else { continue; };

            let pts = cache.points_per_channel;
            let max_abs = if cache.lod_level > 0 {
                let start = ch_pos * pts * 2;
                let end = (start + pts * 2).min(cache.data.len());
                if start < cache.data.len() {
                    cache.data[start..end].iter().map(|v| v.abs()).fold(0.0f32, f32::max)
                } else { 0.0 }
            } else {
                let start = ch_pos * pts;
                let end = (start + pts).min(cache.data.len());
                if start < cache.data.len() {
                    cache.data[start..end].iter().map(|v| v.abs()).fold(0.0f32, f32::max)
                } else { 0.0 }
            };
            global_max = global_max.max(max_abs);
        }

        if global_max == 0.0 { global_max = 1.0; }
        let margin = global_max * 0.05;
        (-(global_max + margin), global_max + margin)
    }

    pub fn toggle_visibility(workspace: &mut WorkspaceState, rec_id: &str, channel_id: &ChannelId) {
        let Some(session) = workspace.recordings.get_mut(rec_id) else { return; };
        match channel_id {
            ChannelId::Physical(idx) => {
                if let Some(d) = session.display.get_mut(*idx as usize) {
                    d.visible = !d.visible;
                }
            }
            ChannelId::Virtual(name) => {
                if let Some(i) = session.meta.virtual_channels.iter().position(|vc| &vc.name == name) {
                    if let Some(d) = session.virtual_display.get_mut(i) {
                        d.visible = !d.visible;
                    }
                }
            }
        }
    }
}
