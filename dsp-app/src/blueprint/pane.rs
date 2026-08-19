use crate::blueprint::ids::PaneId;
use crate::components::views::waveform_overlay::WaveformOverlayMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum PaneKind {
    TraceView,
    StackedView,
    RasterPlot,
    RecordingInfo,
    NodeGraph,
    PcaView,
    WaveformOverlay,
    SpikeSorting,
    TimeseriesMultichannel,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PaneBlueprint {
    pub id: PaneId,
    pub kind: PaneKind,
    pub display_name: Option<String>,
    pub visible: bool,
    pub dataset_id: Option<String>,
    pub config: PaneConfig,
}

impl PaneBlueprint {
    pub fn new(kind: PaneKind) -> Self {
        Self {
            id: PaneId::new(),
            kind,
            display_name: None,
            visible: true,
            dataset_id: None,
            config: PaneConfig::from_kind(kind),
        }
    }
}

/// Y-axis range mode for the timeseries view.
///
/// Auto scans the visible cache data and fits ±max_abs + 5% margin.
/// Manual uses explicit user-specified bounds.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum YAxisRange {
    Auto,
    Manual { min: f32, max: f32 },
}

impl Default for YAxisRange {
    fn default() -> Self { Self::Auto }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum PaneConfig {
    TraceView,
    StackedView { channel_spacing: f32 },
    RasterPlot { row_height: f32 },
    RecordingInfo,
    NodeGraph,
    PcaView,
    WaveformOverlay { mode: WaveformOverlayMode },
    SpikeSorting,
    TimeseriesMultichannel { row_height: f32, y_range: YAxisRange },
}

impl PaneConfig {
    pub fn from_kind(kind: PaneKind) -> Self {
        match kind {
            PaneKind::TraceView       => Self::TraceView,
            PaneKind::StackedView     => Self::StackedView { channel_spacing: 100.0 },
            PaneKind::RasterPlot      => Self::RasterPlot  { row_height: 30.0 },
            PaneKind::RecordingInfo   => Self::RecordingInfo,
            PaneKind::NodeGraph       => Self::NodeGraph,
            PaneKind::PcaView         => Self::PcaView,
            PaneKind::WaveformOverlay => Self::WaveformOverlay { mode: Default::default() },
            PaneKind::SpikeSorting    => Self::SpikeSorting,
            PaneKind::TimeseriesMultichannel => Self::TimeseriesMultichannel { row_height: 60.0, y_range: YAxisRange::Auto },
        }
    }
}
