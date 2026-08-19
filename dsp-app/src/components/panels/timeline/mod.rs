//! Rerun-inspired timeline panel.

pub mod timeline_state;
pub mod timeline_view_model;
pub mod timeline_view;

pub use timeline_state::{TimelineState, TimelineRowState, RowKind};
pub use timeline_view_model::TimelineViewModel;
pub use timeline_view::TimelineView;

use crate::core::session::WorkspaceState;
use crate::core::bridge::IoBridge;

pub struct TimelinePanel;

impl TimelinePanel {
    pub fn show(
        ui: &mut egui::Ui,
        workspace: &mut WorkspaceState,
        sample_rate: f32,
        bridge: &IoBridge,
    ) {
        // 1. Prepare State
        let state = TimelineViewModel::prepare_state(ui.ctx(), workspace, sample_rate);

        // 2. Advance Playback (ViewModel Logic)
        if state.playing {
            let dt = ui.input(|i| i.stable_dt).min(0.1_f32);
            TimelineViewModel::advance_playback(workspace, dt, sample_rate, state.total_samples);
        }

        // 3. Render View
        TimelineView::show(ui, &state, workspace, bridge);
    }

    pub fn orchestrate_fetch(workspace: &mut WorkspaceState, bridge: &IoBridge, width_px: u32) {
        TimelineViewModel::orchestrate_fetch(workspace, bridge, width_px);
    }
}
