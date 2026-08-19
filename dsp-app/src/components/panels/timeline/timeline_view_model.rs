use crate::core::session::{WorkspaceState, XAxisMode};
use crate::core::bridge::{IoBridge, IoRequest};
use super::timeline_state::{TimelineState, TimelineRowState, RowKind};
use egui::Id;

pub struct TimelineViewModel;

impl TimelineViewModel {
    pub fn prepare_state(ctx: &egui::Context, workspace: &WorkspaceState, sample_rate: f32) -> TimelineState {
        let total_samples = workspace
            .recordings
            .values()
            .map(|s| s.meta.total_samples)
            .max()
            .unwrap_or(0);

        let mut rows = Vec::new();
        for (rec_id, session) in &workspace.recordings {
            rows.push(TimelineRowState {
                kind: RowKind::RecordingHeader { rec_id: rec_id.clone() },
                label: session.meta.recording_name.clone(),
                depth: 0,
                color: egui::Color32::from_gray(180),
                is_selected: workspace.timeline.selected_track.as_deref() == Some(&format!("rec::{rec_id}")),
                is_visible: true,
            });

            let rec_open_id = Id::new(("tl_rec_open", rec_id));
            let is_open: bool = ctx.memory(|m| m.data.get_temp(rec_open_id).unwrap_or(true));
            if !is_open {
                continue;
            }

            // Streams
            let has_streams = !session.meta.channel_names.is_empty() || !session.meta.virtual_channels.is_empty();
            if has_streams {
                let color = session.display.first().map(|d| d.egui_color()).unwrap_or(egui::Color32::from_gray(140));
                let kind = RowKind::StreamsGroup { rec_id: rec_id.clone() };
                rows.push(TimelineRowState {
                    kind: kind.clone(),
                    label: "Streams".to_string(),
                    depth: 1,
                    color,
                    is_selected: workspace.timeline.selected_track.as_deref() == Some(&kind.track_key()),
                    is_visible: session.display.iter().any(|d| d.visible) || session.virtual_display.iter().any(|d| d.visible),
                });
            }

            // Epochs
            for track in session.meta.event_tracks() {
                let kind = RowKind::EpochGroup { rec_id: rec_id.clone(), track_name: track.name.clone() };
                rows.push(TimelineRowState {
                    kind: kind.clone(),
                    label: track.name.clone(),
                    depth: 1,
                    color: egui::Color32::from_rgb(255, 200, 100),
                    is_selected: workspace.timeline.selected_track.as_deref() == Some(&kind.track_key()),
                    is_visible: session.event_track_visible.get(&track.name).copied().unwrap_or(true),
                });
            }

            // Annotations
            if !session.meta.annotations.is_empty() {
                let anno_hdr_id = Id::new(("tl_anno_open", rec_id.as_str()));
                let anno_open: bool = ctx.memory(|m| m.data.get_temp(anno_hdr_id).unwrap_or(true));
                rows.push(TimelineRowState {
                    kind: RowKind::AnnotationsHeader { rec_id: rec_id.clone() },
                    label: "Annotations".to_string(),
                    depth: 1,
                    color: egui::Color32::from_gray(160),
                    is_selected: workspace.timeline.selected_track.as_deref() == Some(&format!("annohdr::{rec_id}")),
                    is_visible: true,
                });

                if anno_open {
                    let mut sorted_annos: Vec<_> = session.meta.annotations.iter().collect();
                    sorted_annos.sort_by_key(|a| a.row_index);
                    for anno in sorted_annos {
                        let kind = RowKind::Annotation { rec_id: rec_id.clone(), anno_id: anno.id };
                        rows.push(TimelineRowState {
                            kind: kind.clone(),
                            label: anno.label.clone(),
                            depth: 2,
                            color: egui::Color32::from_rgb(anno.color[0], anno.color[1], anno.color[2]),
                            is_selected: workspace.timeline.selected_track.as_deref() == Some(&kind.track_key()),
                            is_visible: anno.visible,
                        });
                    }
                }
            }
        }

        TimelineState {
            view_start: workspace.view_start,
            view_width: workspace.view_width,
            total_samples,
            playhead: workspace.playhead,
            rows,
            selection: workspace.time_selection(),
            playing: workspace.playback.playing,
            play_speed: workspace.playback.play_speed,
            x_axis_mode: workspace.x_axis_mode,
            sample_rate,
        }
    }

    pub fn advance_playback(workspace: &mut WorkspaceState, dt: f32, sample_rate: f32, total_samples: u64) {
        if !workspace.playback.playing { return; }
        
        let step = workspace.playback.step_samples as f64;
        let speed = workspace.playback.play_speed as f64;
        let advance = (step * speed * dt as f64) as u64;
        
        workspace.playhead = workspace.playhead.saturating_add(advance);
        if workspace.playhead >= total_samples {
            if workspace.playback.loop_enabled {
                workspace.playhead = 0;
            } else {
                workspace.playhead = total_samples.saturating_sub(1);
                workspace.playback.playing = false;
            }
        }

        // Auto-scroll
        if workspace.playhead > workspace.view_start + workspace.view_width || workspace.playhead < workspace.view_start {
            workspace.view_start = workspace.playhead.saturating_sub(workspace.view_width / 4);
        }
    }

    pub fn orchestrate_fetch(workspace: &mut WorkspaceState, bridge: &IoBridge, width_px: u32) {
        /// Resolution of the resident coarse overview (points across the whole recording).
        const OVERVIEW_WIDTH_PX: u32 = 4096;

        for session in workspace.recordings.values_mut() {
            // Resident coarse overview — fetched once per recording (and again
            // after processing clears it). Drawn under the detail layer so
            // scroll/zoom never blanks. Independent of the detail fetch gate below.
            if session.overview.is_none() && !session.overview_pending {
                let channels = session.all_channel_ids();
                if !channels.is_empty() {
                    bridge.send(IoRequest::FetchOverview {
                        dataset_id: session.meta.session_id.clone(),
                        zarr_path: session.zarr_path.clone(),
                        total_samples: session.meta.total_samples,
                        width_px: OVERVIEW_WIDTH_PX,
                        channels,
                    });
                    session.overview_pending = true;
                }
            }

            if session.fetch_pending { continue; }

            let needs_wf_fetch = match &session.cache {
                None => true,
                Some(_) => workspace.view_start != session.cache_x_start,
            };

            if needs_wf_fetch {
                let channels = session.visible_channel_ids();
                if !channels.is_empty() {
                    bridge.send(IoRequest::FetchView {
                        dataset_id: session.meta.session_id.clone(),
                        zarr_path: session.zarr_path.clone(),
                        start_sample: workspace.view_start,
                        count: workspace.view_width,
                        width_px,
                        channels,
                        total_samples: session.meta.total_samples,
                    });
                    session.cache_x_start = workspace.view_start;
                    session.fetch_pending = true;
                }
            }

            // Event fetching
            let needs_events = match session.event_cache_range {
                None => true,
                Some([s, e]) => workspace.view_start < s || (workspace.view_start + workspace.view_width) > e,
            };

            if needs_events {
                let start = workspace.view_start;
                let end = workspace.view_start + workspace.view_width;
                for track in session.meta.event_tracks() {
                    for &ch_idx in &track.channel_indices {
                        let key = (track.name.clone(), ch_idx as u16);
                        if session.events_fetch_pending.contains(&key) { continue; }
                        bridge.send(IoRequest::FetchEvents {
                            dataset_id: session.meta.session_id.clone(),
                            zarr_path: session.zarr_path.clone(),
                            track_name: track.name.clone(),
                            channel_idx: ch_idx as u32,
                            start_sample: start,
                            end_sample: end,
                        });
                        session.events_fetch_pending.insert(key);
                    }
                }
                session.event_cache_range = Some([start, end]);
            }
        }
    }
}
