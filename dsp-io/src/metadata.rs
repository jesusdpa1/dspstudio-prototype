/// Metadata for a specific Level of Detail (LOD).
/// Each level represents a decimated version of the source signal.
#[derive(Debug, Clone)]
pub struct LodLevel {
    pub level: u8,
    /// The decimation ratio (e.g., 16 means 1 point represents 16 source samples)
    pub ratio: u32,
}

/// Metadata describing the full Zarr hierarchy for a recording.
#[derive(Debug, Clone)]
pub struct DatasetMetadata {
    pub total_samples: u64,
    pub lod_chain: Vec<LodLevel>,
}

impl DatasetMetadata {
    /// Creates a new metadata set with a power-of-2 LOD chain.
    /// 
    /// This uses a 16x decimation factor per level:
    /// Level 0: 1:1 (Source)
    /// Level 1: 1:16
    /// Level 2: 1:256 ...
    /// 
    /// # Examples
    /// ```
    /// use dsp_io::metadata::DatasetMetadata;
    /// let meta = DatasetMetadata::new_power_of_two(40000 * 3600); // 1 hour
    /// assert!(meta.lod_chain.len() > 0);
    /// ```
    pub fn new_power_of_two(total_samples: u64) -> Self {
        let mut lod_chain = Vec::new();
        let mut level = 0;
        
        // Decimate by 16x at each level (2^4)
        while (total_samples >> (level * 4)) > 1024 && level < 8 {
            lod_chain.push(LodLevel {
                level: level as u8,
                ratio: 1 << (level * 4),
            });
            level += 1;
        }

        Self {
            total_samples,
            lod_chain,
        }
    }
}
