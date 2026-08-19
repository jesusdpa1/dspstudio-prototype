use serde::{Deserialize, Serialize};

/// Defines the spatial relationship between channels in a high-density probe.
/// 
/// Following the SpikeInterface philosophy, sparsity allows us to only extract
/// and process channels that are physically near a detection event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SparsityMask {
    /// All channels are extracted for every spike.
    Global,
    /// Only the primary channel where the spike was detected is extracted.
    Single,
    /// A fixed subset of neighbors is extracted for each channel.
    /// `neighbors[i]` contains the channel indices near channel `i`.
    Neighborhood {
        neighbors: Vec<Vec<u16>>,
    },
    /// Only channels within a certain radius (in microns) are extracted.
    /// Requires channel coordinates.
    Radius {
        radius_um: f32,
        coords: Vec<(f32, f32)>,
    },
}

impl SparsityMask {
    /// Returns the indices of channels that should be extracted for a given primary channel.
    pub fn get_channels_for(&self, primary_channel: u16, total_channels: u16) -> Vec<u16> {
        match self {
            SparsityMask::Global => (0..total_channels).collect(),
            SparsityMask::Single => vec![primary_channel],
            SparsityMask::Neighborhood { neighbors } => {
                neighbors.get(primary_channel as usize).cloned().unwrap_or(vec![primary_channel])
            }
            SparsityMask::Radius { radius_um, coords } => {
                if let Some(&(x1, y1)) = coords.get(primary_channel as usize) {
                    coords.iter().enumerate().filter_map(|(idx, &(x2, y2))| {
                        let dx = x1 - x2;
                        let dy = y1 - y2;
                        let dist = (dx*dx + dy*dy).sqrt();
                        if dist <= *radius_um {
                            Some(idx as u16)
                        } else {
                            None
                        }
                    }).collect()
                } else {
                    vec![primary_channel]
                }
            }
        }
    }
}
