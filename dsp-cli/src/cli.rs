use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Path to the Zarr recording to serve (legacy, for backward compatibility)
    pub path: Option<PathBuf>,

    /// Address to serve the gRPC TransmissionService on
    #[arg(short, long)]
    pub serve: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Create a new Zarr recording with specified parameters
    CreateRecording {
        /// Sampling rate in Hz
        #[arg(short, long, default_value_t = 40000.0)]
        sampling_rate: f32,

        /// Type of recording (emg, ecog, ephys)
        #[arg(short, long, value_enum, default_value = "emg")]
        recording_type: RecordingType,

        /// Duration in seconds
        #[arg(short, long, default_value_t = 10.0)]
        duration: f64,

        /// Number of channels
        #[arg(short, long, default_value_t = 16)]
        channels: u16,

        /// Output file location (will be a .zarr directory)
        #[arg(short, long)]
        output: PathBuf,

        /// Generate mock epoch data (random events)
        #[arg(long)]
        with_epochs: bool,
    },
    /// Re-encode an existing recording into a new zstd-compressed Zarr store
    Reencode {
        /// Existing .zarr recording to read
        #[arg(short, long)]
        input: PathBuf,

        /// Destination .zarr path (must not exist)
        #[arg(short, long)]
        output: PathBuf,

        /// Zstd compression level
        #[arg(short, long, default_value_t = 3)]
        level: i32,
    },
    /// Start the TUI to visualize metrics
    Tui {
        /// Address to serve the gRPC TransmissionService on
        #[arg(short, long, default_value = "127.0.0.1:50051")]
        serve: String,

        /// Optional path to a recording to serve
        #[arg(short, long)]
        path: Option<PathBuf>,
    },
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
pub enum RecordingType {
    Emg,
    Ecog,
    /// Extracellular electrophysiology: Poisson-firing units with biphasic spike waveforms.
    Ephys,
}

impl std::fmt::Display for RecordingType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecordingType::Emg => f.write_str("EMG"),
            RecordingType::Ecog => f.write_str("ECoG"),
            RecordingType::Ephys => f.write_str("Ephys"),
        }
    }
}
