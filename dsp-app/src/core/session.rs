use dsp_io::recording_meta::{RecordingMeta, VirtualChannelMeta, Annotation};
use dsp_io::processing_graph::ChannelId;
use dsp_io::transmission::ui::{ViewResponse, ClusterData};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// Per-channel display state owned by the UI.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChannelDisplay {
    pub visible: bool,
    pub color: [u8; 4], // Serialized as RGBA
}

impl ChannelDisplay {
    fn new(index: usize) -> Self {
        let color = channel_color(index);
        Self {
            visible: true,
            color: [color.r(), color.g(), color.b(), color.a()],
        }
    }

    pub fn egui_color(&self) -> egui::Color32 {
        egui::Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], self.color[3])
    }
}

/// How the x-axis is labelled and what units are used for coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum XAxisMode {
    Samples,
    Seconds,
}

impl XAxisMode {
    pub fn label(self) -> &'static str {
        match self {
            XAxisMode::Samples => "Sample",
            XAxisMode::Seconds => "Time (s)",
        }
    }
}

/// Spike sorting UI state, shared between PCA and Waveform views.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SpikeSortingState {
    pub track_name: String,
    pub selected_labels: HashSet<u32>,
    pub max_waveforms: u32,
    pub snippet_before: u32,
    pub snippet_after: u32,
}

impl Default for SpikeSortingState {
    fn default() -> Self {
        Self {
            track_name: "sorted_spikes".to_string(),
            selected_labels: HashSet::from([1]),
            max_waveforms: 500,
            snippet_before: 20,
            snippet_after: 28,
        }
    }
}

/// Holds all state and cache for a single loaded recording.
#[derive(Debug)]
pub struct RecordingSession {
    pub zarr_path: PathBuf,
    pub meta: RecordingMeta,
    pub display: Vec<ChannelDisplay>,
    pub virtual_display: Vec<ChannelDisplay>,

    // ── Data Cache ───────────────────────────────────────────────────────────
    pub cache: Option<ViewResponse>,
    /// Sample offset of the current cache.
    pub cache_x_start: u64,
    pub fetch_pending: bool,

    // ── Overview (resident coarse fallback) ──────────────────────────────────
    /// Coarsest whole-recording LOD, fetched once and kept resident. Drawn as a
    /// background layer so scroll/zoom never shows a blank while the detailed
    /// `cache` is being (re)fetched.
    pub overview: Option<ViewResponse>,
    pub overview_pending: bool,

    // ── Epoch Cache ──────────────────────────────────────────────────────────
    /// Events indexed by (track_name, channel_idx).
    pub event_cache: HashMap<(String, u16), Vec<dsp_core::signal::Event>>,
    /// Range [start, end) currently held in event_cache.
    pub event_cache_range: Option<[u64; 2]>,
    /// In-flight event fetch requests — prevents request flooding.
    pub events_fetch_pending: HashSet<(String, u16)>,

    /// Subsampled PCA/waveforms for clusters, indexed by (track_name, label_id).
    pub cluster_cache: HashMap<(String, u32), ClusterData>,

    pub sorting_state: SpikeSortingState,

    // ── Epoch Visibility ─────────────────────────────────────────────────────
    /// Per-track visibility toggle for epoch/event tracks.
    pub event_track_visible: HashMap<String, bool>,
}

impl RecordingSession {
    pub fn new(zarr_path: PathBuf, meta: RecordingMeta) -> Self {
        let n = meta.n_channels as usize;
        let display = (0..n).map(ChannelDisplay::new).collect();
        let virtual_display: Vec<ChannelDisplay> = meta
            .virtual_channels
            .iter()
            .enumerate()
            .map(|(i, _)| ChannelDisplay::new(n + i))
            .collect();
        let event_track_visible = meta.event_tracks()
            .map(|t| (t.name.clone(), true))
            .collect();
        Self {
            zarr_path,
            meta,
            display,
            virtual_display,
            cache: None,
            cache_x_start: 0,
            fetch_pending: false,
            overview: None,
            overview_pending: false,
            event_cache: HashMap::new(),
            event_cache_range: None,
            events_fetch_pending: HashSet::new(),
            cluster_cache: HashMap::new(),
            sorting_state: SpikeSortingState::default(),
            event_track_visible,
        }
    }

    pub fn display_name(&self) -> &str {
        &self.meta.recording_name
    }

    pub fn visible_channel_ids(&self) -> Vec<ChannelId> {
        let mut out = Vec::new();
        for (i, d) in self.display.iter().enumerate() {
            if d.visible {
                out.push(ChannelId::Physical(i as u16));
            }
        }
        for (vc, d) in self.meta.virtual_channels.iter().zip(self.virtual_display.iter()) {
            if d.visible {
                out.push(ChannelId::Virtual(vc.name.clone()));
            }
        }
        out
    }

    /// Every channel id (physical + virtual), regardless of visibility. Used for
    /// the resident overview so toggling a channel on never leaves it blank.
    pub fn all_channel_ids(&self) -> Vec<ChannelId> {
        let mut out: Vec<ChannelId> = (0..self.meta.n_channels)
            .map(|i| ChannelId::Physical(i as u16))
            .collect();
        for vc in &self.meta.virtual_channels {
            out.push(ChannelId::Virtual(vc.name.clone()));
        }
        out
    }

    pub fn merge_virtual_channels(&mut self, new_channels: Vec<VirtualChannelMeta>) {
        let phys_n = self.meta.n_channels as usize;
        for new_vc in new_channels {
            let existing_pos = self.meta.virtual_channels.iter().position(|vc| vc.name == new_vc.name);
            match existing_pos {
                Some(i) => { self.meta.virtual_channels[i] = new_vc; }
                None => {
                    let color_idx = phys_n + self.meta.virtual_channels.len();
                    self.meta.virtual_channels.push(new_vc);
                    self.virtual_display.push(ChannelDisplay::new(color_idx));
                }
            }
        }
    }
}

pub type SessionState = RecordingSession;

/// Playback engine state.
#[derive(Debug)]
pub struct PlaybackState {
    pub playing: bool,
    pub loop_enabled: bool,
    pub play_speed: f32,
    pub step_samples: u64,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self { playing: false, loop_enabled: false, play_speed: 1.0, step_samples: 1 }
    }
}

/// UI state for the timeline panel.
#[derive(Debug, Default)]
pub struct TimelineState {
    pub selected_annotation: Option<u64>,
    pub pending_annotation: Option<Annotation>,
    /// Key of the currently highlighted/selected track row ("ch::<rec>::<idx>", etc.)
    pub selected_track: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SelectionItem {
    Channel(dsp_io::processing_graph::ChannelId),
    Node(egui_snarl::NodeId),
    Cluster(u32),
}

#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum GlobalSelection {
    #[default]
    None,
    TimeRange([u64; 2]),
    Component {
        view_id: crate::blueprint::PaneId,
        item: SelectionItem,
    },
}

/// The global workspace state, holding multiple synchronized recordings.
#[derive(Debug)]
pub struct WorkspaceState {
    /// Registry of all open recording sessions, keyed by their unique display ID.
    pub recordings: HashMap<String, RecordingSession>,
    /// The ID of the currently "selected" or "active" recording for certain global UI tasks.
    pub active_recording_id: Option<String>,

    pub x_axis_mode: XAxisMode,

    pub timeline: TimelineState,

    // ── Global Temporal Navigation (Synced by sample number) ─────────────────
    /// Left-most sample currently visible
    pub view_start: u64,
    /// Number of samples visible in the window
    pub view_width: u64,

    // ── Playhead ─────────────────────────────────────────────────────────────
    pub playhead: u64,
    pub playback: PlaybackState,

    // ── Global Selection ─────────────────────────────────────────────────────
    pub selection: GlobalSelection,
}

impl WorkspaceState {
    pub fn new() -> Self {
        Self {
            recordings: HashMap::new(),
            active_recording_id: None,
            x_axis_mode: XAxisMode::Seconds,
            timeline: TimelineState::default(),
            view_start: 0,
            view_width: 1000,
            playhead: 0,
            playback: PlaybackState::default(),
            selection: GlobalSelection::None,
        }
    }

    pub fn time_selection(&self) -> Option<[u64; 2]> {
        match self.selection {
            GlobalSelection::TimeRange(range) => Some(range),
            _ => None,
        }
    }

    pub fn set_time_selection(&mut self, range: Option<[u64; 2]>) {
        self.selection = match range {
            Some(r) => GlobalSelection::TimeRange(r),
            None => GlobalSelection::None,
        };
    }

    pub fn add_recording(&mut self, zarr_path: PathBuf, mut meta: RecordingMeta) -> String {
        let id = if !meta.session_id.is_empty() {
            meta.session_id.clone()
        } else {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            zarr_path.hash(&mut h);
            let id = format!("{:016x}", h.finish());
            meta.session_id = id.clone();
            id
        };
        let session = RecordingSession::new(zarr_path, meta);
        self.recordings.insert(id.clone(), session);
        if self.active_recording_id.is_none() {
            self.active_recording_id = Some(id.clone());
        }
        id
    }

    pub fn get_recording(&self, id: &str) -> Option<&RecordingSession> {
        self.recordings.get(id)
    }

    pub fn get_recording_mut(&mut self, id: &str) -> Option<&mut RecordingSession> {
        self.recordings.get_mut(id)
    }

    pub fn current_x_range(&self, sample_rate: f32) -> (f64, f64) {
        current_x_range(self.view_start, self.view_width, self.x_axis_mode, sample_rate)
    }

    pub fn draw_selection(&self, plot_ui: &mut egui_plot::PlotUi, sample_rate: f32) {
        let sel_opt = match &self.selection {
            GlobalSelection::TimeRange(r) => Some(*r),
            _ => None,
        };
        draw_selection(sel_opt, self.x_axis_mode, plot_ui, sample_rate);
    }
}

pub fn current_x_range(view_start: u64, view_width: u64, x_axis_mode: XAxisMode, sample_rate: f32) -> (f64, f64) {
    let start = view_start as f64;
    let width = view_width as f64;

    match x_axis_mode {
        XAxisMode::Samples => (start, start + width),
        XAxisMode::Seconds => {
            let hz = sample_rate as f64;
            (start / hz, (start + width) / hz)
        }
    }
}

pub fn draw_selection(selection: Option<[u64; 2]>, x_axis_mode: XAxisMode, plot_ui: &mut egui_plot::PlotUi, sample_rate: f32) {
    if let Some([s, e]) = selection {
        let hz = sample_rate as f64;
        let (x0, x1) = match x_axis_mode {
            XAxisMode::Samples => (s as f64, e as f64),
            XAxisMode::Seconds => (s as f64 / hz, e as f64 / hz),
        };
        
        let bounds = plot_ui.plot_bounds();
        let y0 = bounds.min()[1];
        let y1 = bounds.max()[1];

        let color = egui::Color32::from_rgba_unmultiplied(255, 180, 50, 40);
        plot_ui.polygon(
            egui_plot::Polygon::new(
                "selection",
                egui_plot::PlotPoints::from_iter(vec![
                    [x0, y0], [x1, y0], [x1, y1], [x0, y1]
                ])
            )
            .fill_color(color)
            .stroke(egui::Stroke::NONE)
        );
    }
}

/// Top-level application state machine.
#[derive(Debug, Default)]
pub enum AppState {
    /// No file open. Shows empty workspace.
    #[default]
    Idle,

    /// Waiting for the IO thread to open and inspect a file.
    CheckingFile,

    /// Peak pyramid is missing; waiting for the user to choose.
    PeakBuildDialog { zarr_path: PathBuf },

    /// Peak pyramid is being built; progress in 0.0..=1.0.
    BuildingPeaks { progress: f32 },

    /// Workspace is active (can contain zero or more recordings).
    Active(WorkspaceState),
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn channel_color(index: usize) -> egui::Color32 {
    const PALETTE: &[egui::Color32] = &[
        egui::Color32::from_rgb(86, 180, 233),  // sky blue
        egui::Color32::from_rgb(230, 159, 0),   // orange
        egui::Color32::from_rgb(0, 158, 115),   // teal
        egui::Color32::from_rgb(240, 228, 66),  // yellow
        egui::Color32::from_rgb(0, 114, 178),   // blue
        egui::Color32::from_rgb(213, 94, 0),    // vermillion
        egui::Color32::from_rgb(204, 121, 167), // pink
        egui::Color32::from_rgb(0, 255, 127),   // spring green
    ];
    PALETTE[index % PALETTE.len()]
}
