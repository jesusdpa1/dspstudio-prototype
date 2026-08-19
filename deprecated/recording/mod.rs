pub mod info_panel;
pub mod multi_recording_view;
pub mod recording_view;
pub mod sidebar;

use crate::components::views::waveform::spike_view::SpikeViewMode;

/// Tile variants used by the egui_tiles Behavior.
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum Pane {
    /// Individual channel rows with dedicated Y-axes for a specific dataset.
    TraceView { dataset_id: Option<String> },
    /// Offset stacked channels in a single plot for a specific dataset.
    StackedView { 
        dataset_id: Option<String>,
        #[serde(default)]
        channel_spacing: f32,
    },
    /// Rug plot of event tracks for a specific dataset.
    RasterPlot { dataset_id: Option<String> },
    /// Global metadata info panel.
    RecordingInfo,
    /// Shared DSP processing graph.
    NodeGraph,
    /// 2D PCA cluster projection.
    PcaView { dataset_id: Option<String> },
    /// Multi-mode spike waveform viewer.
    SpikeView {
        #[serde(default)]
        mode: SpikeViewMode,
    },
    /// Integrated spike sorting view (PCA + Waveforms + Unified Controls).
    SpikeSorting,
}
