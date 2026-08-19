//! Top-level eframe application.

use crate::blueprint::{
    BlueprintCommand, BlueprintCommandQueue, PaneBlueprint, PaneConfig, PaneId, PaneKind,
    ViewportBlueprint, pane_id_to_tile_id, tree_simplification_options,
};
use crate::components::panels::blueprint_tree::BlueprintTree;
use crate::components::views::node_graph::{NodeGraphView, NodeGraphState};
use crate::components::views::pca::PcaView;
use crate::components::views::timeseries_multichannel::{TimeseriesMultichannelView, TimeseriesFocusMode};
use crate::components::views::raster::RasterView;
use crate::components::views::recording_info::RecordingInfoView;
use crate::components::views::waveform_overlay::{WaveformOverlayView, WaveformOverlayMode};
use crate::core::bridge::{BackendConfig, IoBridge, IoRequest, IoResponse};
use crate::core::session::{AppState, WorkspaceState};
use crate::features::spikesorting::SpikeSortingView;
use crate::features::welcome::{WelcomeAction, WelcomeScreen};
use egui::{Widget, Panel};
use egui_tiles::{Behavior, SimplificationOptions, TileId, UiResponse};
use std::path::PathBuf;

use std::collections::HashMap;

pub struct DspStudioApp {
    state: AppState,
    bridge: IoBridge,
    viewport: Option<ViewportBlueprint>,
    command_queue: BlueprintCommandQueue,
    blueprint_tree: BlueprintTree,

    // Per-instance view state for complex panes
    node_graphs: HashMap<PaneId, NodeGraphState>,
    spike_sortings: HashMap<PaneId, SpikeSortingView>,

    add_view_modal: crate::components::add_view_modal::AddViewModal,
    welcome_screen: WelcomeScreen,
    status_msg: Option<String>,
    status_clear_at: Option<std::time::Instant>,
    cli_path: Option<PathBuf>,
    cli_sent: bool,
    remote_url: String,
    show_remote_dialog: bool,
    show_about_dialog: bool,

    // ── UI Visibility ────────────────────────────────────────────────────────
    show_left_panel: bool,
    show_bottom_panel: bool,
    show_right_panel: bool,

    // ── Panel sizes (persisted so toggle off/on restores user's chosen height) ─
    timeline_height: f32,

    // ── Shared View States ───────────────────────────────────────────────────
    focus_mode: TimeseriesFocusMode,
}

impl DspStudioApp {
    pub fn new(
        _cc: &eframe::CreationContext,
        cli_path: Option<PathBuf>,
        backend: BackendConfig,
        _starts_visible: bool,
    ) -> Self {
        Self {
            state: AppState::Idle,
            bridge: IoBridge::new(backend),
            viewport: None,
            command_queue: BlueprintCommandQueue::default(),
            blueprint_tree: BlueprintTree::default(),
            node_graphs: HashMap::new(),
            spike_sortings: HashMap::new(),
            add_view_modal: crate::components::add_view_modal::AddViewModal::new(),
            welcome_screen: WelcomeScreen::new(),
            status_msg: None,
            status_clear_at: None,
            cli_path,
            cli_sent: false,
            remote_url: "http://[::1]:50051".to_string(),
            show_remote_dialog: false,
            show_about_dialog: false,
            show_left_panel: true,
            show_bottom_panel: true,
            show_right_panel: true,
            timeline_height: 200.0,
            focus_mode: TimeseriesFocusMode::None,
        }
    }

    fn set_status(&mut self, msg: impl Into<String>) {
        self.status_msg = Some(msg.into());
        self.status_clear_at = Some(std::time::Instant::now() + std::time::Duration::from_secs(5));
    }

    fn open_file(&mut self, path: PathBuf) {
        self.bridge.send(IoRequest::OpenFile(path));
        self.state = AppState::CheckingFile;
        self.set_status("Opening file…");
    }

    fn activate_session(
        &mut self,
        zarr_path: PathBuf,
        meta: dsp_io::recording_meta::RecordingMeta,
    ) {
        let preferred_blueprint = meta.preferred_blueprint.clone();
        let recording_name = meta.recording_name.clone();

        // Add to recent store
        self.welcome_screen
            .recent
            .add(recording_name.clone(), zarr_path.clone());

        if !matches!(self.state, AppState::Active(_)) {
            self.state = AppState::Active(WorkspaceState::new());
        }

        let id = match &mut self.state {
            AppState::Active(ws) => ws.add_recording(zarr_path, meta),
            _ => unreachable!(),
        };

        // Build viewport only on first recording
        if self.viewport.is_none() {
            let vp = preferred_blueprint
                .and_then(|json| crate::blueprint::viewport_from_json(&json))
                .unwrap_or_else(|| match &self.state {
                    AppState::Active(ws) => ws
                        .recordings
                        .get(&id)
                        .map(|s| crate::blueprint::auto_layout::auto_generate(&s.meta))
                        .unwrap_or_else(crate::blueprint::auto_layout::default_blueprint),
                    _ => crate::blueprint::auto_layout::default_blueprint(),
                });
            self.viewport = Some(vp);
        }

        self.set_status(format!("Recording '{}' loaded.", recording_name));
    }
}

impl eframe::App for DspStudioApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx();
        // ── Status auto-clear ─────────────────────────────────────────────────
        if let Some(clear_at) = self.status_clear_at {
            if std::time::Instant::now() >= clear_at {
                self.status_msg = None;
                self.status_clear_at = None;
            }
        }

        // ── CLI path ──────────────────────────────────────────────────────────
        if !self.cli_sent {
            self.cli_sent = true;
            if let Some(path) = self.cli_path.take() {
                self.open_file(path);
            }
        }

        // ── IO responses ──────────────────────────────────────────────────────
        while let Ok(resp) = self.bridge.rx.try_recv() {
            match resp {
                IoResponse::FileOpened {
                    zarr_path, meta, ..
                } => {
                    self.activate_session(zarr_path, meta);
                    ctx.request_repaint();
                }
                IoResponse::PeakPyramidMissing { zarr_path } => {
                    self.state = AppState::PeakBuildDialog { zarr_path };
                    ctx.request_repaint();
                }
                IoResponse::PeakBuildProgress(p) => {
                    if let AppState::BuildingPeaks { ref mut progress } = self.state {
                        *progress = p;
                    }
                    ctx.request_repaint();
                }
                IoResponse::PeakBuildComplete { zarr_path } => {
                    self.bridge.send(IoRequest::OpenFile(zarr_path));
                    self.state = AppState::CheckingFile;
                    ctx.request_repaint();
                }
                IoResponse::ViewReady {
                    dataset_id,
                    start_sample,
                    response,
                } => {
                    if let AppState::Active(ref mut ws) = self.state {
                        if let Some(session) = ws.recordings.get_mut(&dataset_id) {
                            if start_sample == session.cache_x_start {
                                session.cache = Some(response);
                            }
                            session.fetch_pending = false;
                        }
                    }
                    ctx.request_repaint();
                }
                IoResponse::OverviewReady { dataset_id, response } => {
                    if let AppState::Active(ref mut ws) = self.state {
                        if let Some(session) = ws.recordings.get_mut(&dataset_id) {
                            session.overview = Some(response);
                            session.overview_pending = false;
                        }
                    }
                    ctx.request_repaint();
                }
                IoResponse::ProcessingProgress(p) => {
                    for graph in self.node_graphs.values_mut() {
                        graph.on_processing_progress(p.clone());
                    }
                    ctx.request_repaint();
                }
                IoResponse::ProcessingComplete {
                    dataset_id,
                    virtual_channels,
                } => {
                    if let AppState::Active(ref mut ws) = self.state {
                        if let Some(session) = ws.recordings.get_mut(&dataset_id) {
                            session.merge_virtual_channels(virtual_channels.clone());
                            session.cache = None;
                            // Refetch the overview so the new virtual channels get a
                            // coarse fallback layer too.
                            session.overview = None;
                        }
                    }
                    for graph in self.node_graphs.values_mut() {
                        graph.on_processing_complete(virtual_channels.clone());
                    }
                    self.set_status("Processing complete.");
                    ctx.request_repaint();
                }
                IoResponse::EventsReady {
                    dataset_id,
                    track_name,
                    channel_idx,
                    events,
                } => {
                    if let AppState::Active(ref mut ws) = self.state {
                        if let Some(session) = ws.recordings.get_mut(&dataset_id) {
                            session
                                .event_cache
                                .insert((track_name.clone(), channel_idx as u16), events);
                            session
                                .events_fetch_pending
                                .remove(&(track_name, channel_idx as u16));
                        }
                    }
                    ctx.request_repaint();
                }
                IoResponse::ClusterDataReady {
                    dataset_id,
                    track_name,
                    data,
                } => {
                    if let AppState::Active(ref mut ws) = self.state {
                        if let Some(session) = ws.recordings.get_mut(&dataset_id) {
                            session
                                .cluster_cache
                                .insert((track_name, data.label_id), data);
                        }
                    }
                    ctx.request_repaint();
                }
                IoResponse::MetaSaved => {
                    self.set_status("Metadata saved.");
                    crate::components::views::recording_info::RecordingInfoView::on_meta_saved(ctx);
                    ctx.request_repaint();
                }
                IoResponse::Error(e) => {
                    if let AppState::Active(ref mut ws) = self.state {
                        // Reset all in-flight markers so a failed fetch doesn't
                        // permanently wedge a view or an event track.
                        for s in ws.recordings.values_mut() {
                            s.fetch_pending = false;
                            s.events_fetch_pending.clear();
                            s.overview_pending = false;
                        }
                    }
                    self.set_status(format!("Error: {}", e));
                    if matches!(
                        self.state,
                        AppState::CheckingFile | AppState::BuildingPeaks { .. }
                    ) {
                        self.state = AppState::Idle;
                    }
                    ctx.request_repaint();
                }
            }
        }

        self.add_view_modal.show(ctx, &self.command_queue);

        // --- UI PANELS ---
        if !matches!(self.state, AppState::Idle) {
            egui::Panel::top("top_panel").show(ctx, |ui| {
                egui::MenuBar::new().ui(ui, |ui| {
                    ui.menu_button("File", |ui| {
                        if ui.button("📂 Open Folder…").clicked() {
                            if let Some(path) = pick_zarr_file() {
                                self.open_file(path);
                            }
                            ui.close();
                        }
                        if ui.button("🌐 Connect to Remote…").clicked() {
                            self.show_remote_dialog = true;
                            ui.close();
                        }
                        ui.separator();
                        if let AppState::Active(workspace) = &mut self.state {
                            if ui.button("💾 Save Blueprint").clicked() {
                                if let Some(viewport) = &self.viewport {
                                    let json = crate::blueprint::viewport_to_json(viewport);
                                    if let Some(id) = &workspace.active_recording_id {
                                        if let Some(session) = workspace.recordings.get_mut(id) {
                                            session.meta.preferred_blueprint = Some(json);
                                            self.bridge.send(IoRequest::SaveRecordingMeta {
                                                zarr_path: session.zarr_path.clone(),
                                                meta: session.meta.clone(),
                                            });
                                        }
                                    }
                                }
                                ui.close();
                            }
                        }
                        if ui
                            .add_enabled(
                                matches!(self.state, AppState::Active(_)),
                                egui::Button::new("❌ Close Recording"),
                            )
                            .clicked()
                        {
                            if let AppState::Active(ref mut ws) = self.state {
                                if let Some(id) = ws.active_recording_id.take() {
                                    ws.recordings.remove(&id);
                                }
                                if ws.recordings.is_empty() {
                                    self.state = AppState::Idle;
                                    self.viewport = None;
                                } else {
                                    ws.active_recording_id = ws.recordings.keys().next().cloned();
                                }
                            }
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("🚪 Quit").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });

                    ui.add_enabled_ui(matches!(self.state, AppState::Active(_)), |ui| {
                        ui.menu_button("View", |ui| {
                            for (icon, label, kind) in [
                                ("📈", "Trace View", PaneKind::TraceView),
                                ("📊", "Stacked View", PaneKind::StackedView),
                                ("🕸", "Node Graph", PaneKind::NodeGraph),
                                ("📅", "Raster Plot", PaneKind::RasterPlot),
                                ("⬢", "PCA Projection", PaneKind::PcaView),
                                ("〰", "Waveform Overlay", PaneKind::WaveformOverlay),
                                ("🧬", "Spike Sorting", PaneKind::SpikeSorting),
                            ] {
                                let is_open = self
                                    .viewport
                                    .as_ref()
                                    .map(|vp| vp.panes.values().any(|p| p.kind == kind))
                                    .unwrap_or(false);
                                if ui
                                    .selectable_label(is_open, format!("{icon} {label}"))
                                    .clicked()
                                {
                                    if is_open {
                                        if let Some(vp) = &self.viewport {
                                            if let Some(id) = vp
                                                .panes
                                                .iter()
                                                .find(|(_, p)| p.kind == kind)
                                                .map(|(id, _)| *id)
                                            {
                                                self.command_queue.push(
                                                    BlueprintCommand::RemoveContents(
                                                        crate::blueprint::Contents::Pane(id),
                                                    ),
                                                );
                                            }
                                        }
                                    } else {
                                        self.command_queue.push(BlueprintCommand::AddPane {
                                            pane: crate::blueprint::PaneBlueprint::new(kind),
                                            parent_container: None,
                                            position_in_parent: None,
                                        });
                                    }
                                    ui.close();
                                }
                            }
                            ui.separator();
                            if ui.button("↺ Reset Layout").clicked() {
                                self.viewport =
                                    Some(crate::blueprint::auto_layout::default_blueprint());
                                ui.close();
                            }
                        });
                    });

                    ui.menu_button("Help", |ui| {
                        if ui.button("❓ About").clicked() {
                            self.show_about_dialog = true;
                            ui.close();
                        }
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Right panel toggle
                        let resp = crate::icons::RIGHT_PANEL_TOGGLE
                            .as_button()
                            .selected(self.show_right_panel)
                            .ui(ui);
                        if resp.clicked() {
                            self.show_right_panel = !self.show_right_panel;
                        }
                        resp.on_hover_text("Toggle Right Panel (Selection & Config)");

                        // Bottom panel toggle
                        let resp = crate::icons::BOTTOM_PANEL_TOGGLE
                            .as_button()
                            .selected(self.show_bottom_panel)
                            .ui(ui);
                        if resp.clicked() {
                            self.show_bottom_panel = !self.show_bottom_panel;
                        }
                        resp.on_hover_text("Toggle Bottom Panel (Timeline)");

                        // Left panel toggle
                        let resp = crate::icons::LEFT_PANEL_TOGGLE
                            .as_button()
                            .selected(self.show_left_panel)
                            .ui(ui);
                        if resp.clicked() {
                            self.show_left_panel = !self.show_left_panel;
                        }
                        resp.on_hover_text("Toggle Left Panel (Blueprint Tree)");
                    });
                });
            });

            egui::Panel::bottom("status_panel").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if let Some(msg) = &self.status_msg {
                        ui.label(msg);
                    } else {
                        ui.label("Ready");
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.separator();
                        ui.label("Backend Active");
                    });
                });
            });
        }

        if self.show_left_panel && matches!(self.state, AppState::Active(_)) {
            egui::Panel::left("sources_blueprint_panel")
                .resizable(true)
                .default_size(220.0)
                .show(ctx, |ui| {
                    if let AppState::Active(ref mut workspace) = self.state {
                        if let Some(vp) = &self.viewport {
                            self.blueprint_tree
                                .show(ui, workspace, vp, &self.command_queue, &mut self.add_view_modal);
                        }
                    }
                });
        }

        if self.show_right_panel && matches!(self.state, AppState::Active(_)) {
            egui::Panel::right("selection_panel")
                .resizable(true)
                .default_size(250.0)
                .show(ctx, |ui| {
                    if let AppState::Active(ref mut workspace) = self.state {
                        if let Some(vp) = &self.viewport {
                            crate::components::panels::selection_panel::SelectionPanel::show(ui, workspace, vp, &self.blueprint_tree.selected);
                        }
                    }
                });
        }

        if let AppState::Active(ref mut workspace) = self.state {
            if self.show_bottom_panel {
                egui::Panel::bottom("timeline_panel")
                    .resizable(true)
                    .min_size(55.0)
                    .default_size(self.timeline_height)
                    .show(ctx, |ui| {
                        let sample_rate = workspace
                            .active_recording_id
                            .as_ref()
                            .and_then(|id| workspace.recordings.get(id))
                            .map(|s| s.meta.sample_rate)
                            .unwrap_or(40000.0);
                        crate::components::panels::timeline::TimelinePanel::show(
                            ui,
                            workspace,
                            sample_rate,
                            &self.bridge,
                        );
                    });
            }
        }

        if self.show_about_dialog {
            egui::Window::new("About DSP Studio")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.heading("DSP Studio");
                        ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                        ui.separator();
                        ui.label("A high-performance digital signal processing environment.");
                        ui.label("Built with Rust and egui.");
                        ui.add_space(10.0);
                        if ui.button("Close").clicked() {
                            self.show_about_dialog = false;
                        }
                    });
                });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            match &mut self.state {
                AppState::Idle => {
                    if let Some(action) = self.welcome_screen.show(ui) {
                        match action {
                            WelcomeAction::OpenFolder => {
                                if let Some(path) = pick_zarr_file() {
                                    self.open_file(path);
                                }
                            }
                            WelcomeAction::OpenFile(path) => {
                                self.open_file(path);
                            }
                            WelcomeAction::RemoveFile(path) => {
                                self.welcome_screen.recent.remove(&path);
                            }
                        }
                    }
                }
                AppState::CheckingFile => {
                    ui.centered_and_justified(|ui| {
                        ui.spinner();
                    });
                }
                AppState::PeakBuildDialog { zarr_path } => {
                    let path = zarr_path.clone();
                    ui.vertical_centered(|ui| {
                        ui.heading("Peak Pyramid Missing");
                        ui.label("This recording has no LOD peak data. Generate it now for smooth scrolling?");
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.button("Generate").clicked() {
                                self.bridge.send(IoRequest::BuildPeakPyramid(path.clone()));
                                self.state = AppState::BuildingPeaks { progress: 0.0 };
                            }
                            if ui.button("Cancel").clicked() {
                                self.state = AppState::Idle;
                            }
                        });
                    });
                }
                AppState::BuildingPeaks { progress } => {
                    ui.vertical_centered(|ui| {
                        ui.heading("Building Peak Pyramid…");
                        ui.add(egui::ProgressBar::new(*progress).show_percentage());
                    });
                }
                AppState::Active(workspace) => {
                    // Drive fetches every frame even when the timeline panel is hidden.
                    crate::components::panels::timeline::TimelinePanel::orchestrate_fetch(
                        workspace, &self.bridge, ui.available_width() as u32,
                    );

                    if let Some(vp) = &mut self.viewport {
                        let mut behavior = AppBehavior {
                            workspace,
                            panes: &mut vp.panes,
                            command_queue: &self.command_queue,
                            blueprint_tree: &mut self.blueprint_tree,
                            bridge: &self.bridge,
                            node_graphs: &mut self.node_graphs,
                            spike_sortings: &mut self.spike_sortings,
                            focus_mode: &mut self.focus_mode,
                            show_right_panel: self.show_right_panel,
                            user_edited: false,
                        };

                        // egui_tiles 0.15 has no built-in maximized support.
                        // We short-circuit the tree and render the single pane directly.
                        if let Some(mut maximized_id) = vp.maximized {
                            let tile_id = crate::blueprint::pane_id_to_tile_id(maximized_id);
                            let _ = behavior.pane_ui(ui, tile_id, &mut maximized_id);
                        } else {
                            vp.tree.ui(&mut behavior, ui);
                        }

                        // Ctrl+M: toggle maximized for the selected pane.
                        if ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::M)) {
                            if let Some(crate::components::panels::blueprint_tree::ItemId::View(pane_id)) = behavior.blueprint_tree.selected {
                                let new_maximized = if vp.maximized == Some(pane_id) { None } else { Some(pane_id) };
                                self.command_queue.push(BlueprintCommand::SetMaximized(new_maximized));
                            }
                        }
                        if ui.input(|i| i.key_pressed(egui::Key::Escape)) && vp.maximized.is_some() {
                            self.command_queue.push(BlueprintCommand::SetMaximized(None));
                        }

                        if behavior.user_edited {
                            vp.auto_layout = false;
                        }

                        let commands = self.command_queue.drain();
                        crate::blueprint::ViewportViewModel::apply_commands(vp, commands);
                        vp.tree.simplify(&tree_simplification_options());
                    }
                }
            }
        });

        if self.show_remote_dialog {
            egui::Window::new("Connect to Remote").show(ctx, |ui| {
                ui.label("Server URL:");
                ui.text_edit_singleline(&mut self.remote_url);
                ui.horizontal(|ui| {
                    if ui.button("Connect").clicked() {
                        self.bridge = IoBridge::new(BackendConfig::Remote(self.remote_url.clone()));
                        self.show_remote_dialog = false;
                        self.set_status(format!("Connected to {}", self.remote_url));
                    }
                    if ui.button("Cancel").clicked() {
                        self.show_remote_dialog = false;
                    }
                });
            });
        }
    }
}

struct AppBehavior<'a> {
    workspace: &'a mut WorkspaceState,
    panes: &'a mut std::collections::BTreeMap<PaneId, PaneBlueprint>,
    command_queue: &'a BlueprintCommandQueue,
    blueprint_tree: &'a mut BlueprintTree,
    bridge: &'a IoBridge,
    node_graphs: &'a mut HashMap<PaneId, NodeGraphState>,
    spike_sortings: &'a mut HashMap<PaneId, SpikeSortingView>,
    focus_mode: &'a mut TimeseriesFocusMode,
    show_right_panel: bool,
    /// Set to true by on_edit when the user drags/resizes a tile.
    user_edited: bool,
}

impl<'a> Behavior<PaneId> for AppBehavior<'a> {
    fn tab_title_for_pane(&mut self, pane_id: &PaneId) -> egui::WidgetText {
        if let Some(pane) = self.panes.get(pane_id) {
            let name = if let Some(name) = &pane.display_name {
                name.clone()
            } else {
                match pane.kind {
                    PaneKind::TraceView => "📈 Trace View".to_string(),
                    PaneKind::StackedView => "📊 Stacked View".to_string(),
                    PaneKind::RasterPlot => "📅 Raster Plot".to_string(),
                    PaneKind::RecordingInfo => "ℹ Metadata".to_string(),
                    PaneKind::NodeGraph => "🕸 Node Graph".to_string(),
                    PaneKind::PcaView => "⬢ PCA Projection".to_string(),
                    PaneKind::WaveformOverlay => "〰 Waveform Overlay".to_string(),
                    PaneKind::SpikeSorting => "🧬 Spike Sorting".to_string(),
                    PaneKind::TimeseriesMultichannel => "〜 Timeseries Multichannel".to_string(),
                }
            };

            let mut text = egui::RichText::new(name);
            if !pane.visible {
                text = text.weak();
            }
            if self.blueprint_tree.selected == Some(crate::components::panels::blueprint_tree::ItemId::View(*pane_id)) {
                text = text.strong();
            }
            text.into()
        } else {
            "Unknown".into()
        }
    }

    fn is_tab_closable(&self, _tiles: &egui_tiles::Tiles<PaneId>, _tile_id: TileId) -> bool {
        true
    }

    fn on_tab_close(&mut self, tiles: &mut egui_tiles::Tiles<PaneId>, tile_id: TileId) -> bool {
        if let Some(tile) = tiles.get(tile_id) {
            if let egui_tiles::Tile::Pane(pane_id) = tile {
                self.command_queue.push(BlueprintCommand::RemoveContents(
                    crate::blueprint::Contents::Pane(*pane_id),
                ));
            }
        }
        true
    }

    fn pane_ui(&mut self, ui: &mut egui::Ui, tile_id: TileId, pane_id: &mut PaneId) -> UiResponse {
        // 2.5 Selection sync: Clicking a tab in the viewport should update the blueprint tree selection.
        if ui.input(|i| i.pointer.any_pressed()) && ui.rect_contains_pointer(ui.max_rect()) {
            self.blueprint_tree.selected = Some(crate::components::panels::blueprint_tree::ItemId::View(*pane_id));
        }

        ui.interact(ui.max_rect(), ui.id(), egui::Sense::click()).context_menu(|ui| {
            ui.label(format!("View: {:?}", pane_id));
            ui.separator();
            if ui.button("Remove").clicked() {
                self.command_queue.push(BlueprintCommand::RemoveContents(
                    crate::blueprint::Contents::Pane(*pane_id),
                ));
                ui.close();
            }
        });

        let Some(pane) = self.panes.get_mut(pane_id) else {
            return UiResponse::None;
        };

        let mut dataset_id = pane.dataset_id.clone();
        if dataset_id.is_none() {
            dataset_id = self.workspace.active_recording_id.clone();
        }

        match pane.kind {
            PaneKind::TraceView => {
                TimeseriesMultichannelView::show(
                    ui, self.workspace, self.bridge, 60.0,
                    crate::blueprint::pane::YAxisRange::Auto, self.focus_mode,
                );
            }
            PaneKind::StackedView => {
                let spacing = if let PaneConfig::StackedView { channel_spacing } = pane.config {
                    channel_spacing
                } else {
                    100.0
                };
                TimeseriesMultichannelView::show(
                    ui, self.workspace, self.bridge, spacing,
                    crate::blueprint::pane::YAxisRange::Auto, self.focus_mode,
                );
            }
            PaneKind::RasterPlot => {
                let row_height = if let PaneConfig::RasterPlot { row_height } = pane.config {
                    row_height
                } else {
                    30.0
                };
                RasterView::new().show(ui, tile_id, self.workspace, &mut dataset_id, row_height);
            }
            PaneKind::RecordingInfo => {
                crate::components::views::recording_info::RecordingInfoView::show(
                    ui, self.workspace, self.bridge, &dataset_id,
                );
            }
            PaneKind::NodeGraph => {
                let state = self.node_graphs.entry(*pane_id).or_insert_with(NodeGraphState::default);
                NodeGraphView::new().show(ui, self.workspace, self.bridge, state);
            }
            PaneKind::PcaView => {
                PcaView::show(ui, tile_id, self.workspace, self.bridge, &mut dataset_id);
            }
            PaneKind::WaveformOverlay => {
                let mut mode = if let PaneConfig::WaveformOverlay { mode } = pane.config {
                    mode
                } else {
                    Default::default()
                };
                WaveformOverlayView::show(ui, tile_id, self.workspace, self.bridge, &mut mode);
                // Always write back — WaveformOverlayView::show may mutate `mode` via the mode selector.
                pane.config = PaneConfig::WaveformOverlay { mode };
            }
            PaneKind::SpikeSorting => {
                self.spike_sortings
                    .entry(*pane_id)
                    .or_insert_with(SpikeSortingView::new)
                    .show(ui, tile_id, self.workspace, self.bridge);
            }
            PaneKind::TimeseriesMultichannel => {
                let (row_height, y_range) = if let PaneConfig::TimeseriesMultichannel {
                    row_height,
                    y_range,
                } = pane.config
                {
                    (row_height, y_range)
                } else {
                    (60.0, crate::blueprint::pane::YAxisRange::Auto)
                };
                TimeseriesMultichannelView::show(
                    ui,
                    self.workspace,
                    self.bridge,
                    row_height,
                    y_range,
                    self.focus_mode,
                );
                pane.config = PaneConfig::TimeseriesMultichannel {
                    row_height,
                    y_range,
                };
            }
        }

        if dataset_id != pane.dataset_id {
            pane.dataset_id = dataset_id;
        }

        UiResponse::None
    }

    fn retain_pane(&mut self, pane_id: &PaneId) -> bool {
        self.panes.contains_key(pane_id)
    }

    fn on_edit(&mut self, edit_action: egui_tiles::EditAction) {
        if matches!(
            edit_action,
            egui_tiles::EditAction::TileDropped | egui_tiles::EditAction::TileResized
        ) {
            self.user_edited = true;
        }
    }

    fn simplification_options(&self) -> SimplificationOptions {
        tree_simplification_options()
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn pick_zarr_file() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Select Zarr Recording Folder")
        .pick_folder()
}
