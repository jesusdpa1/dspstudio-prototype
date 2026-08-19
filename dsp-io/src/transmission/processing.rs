use crate::zarr::StorageManager;
use crate::virtual_channel::VirtualChannelStore;
use crate::processing_graph::ChannelId;
use anyhow::{Result, Context};

/// Handles batch data requests for DSP processing kernels, pulling from both
/// raw Zarr storage and processed virtual channels.
/// 
/// `ProcessingService` implements **Surplus Windowing**. It ensures that every
/// batch returned to a DSP kernel has additional "look-ahead" and "look-behind"
/// data points to prevent digital filtering artifacts (edge effects).
pub struct ProcessingService<'a> {
    pub storage: &'a StorageManager,
    pub virtual_store: Option<&'a mut VirtualChannelStore>,
}

impl<'a> ProcessingService<'a> {
    /// Creates a service with access to the raw storage.
    pub fn new(storage: &'a StorageManager, virtual_store: Option<&'a mut VirtualChannelStore>) -> Self {
        Self { storage, virtual_store }
    }

    /// Fetches a data block with a surplus (overlap) and automated zero-padding.
    /// 
    /// Supports both physical and virtual channels.
    pub fn fetch_package_with_surplus(
        &mut self, 
        start: i64, 
        count: u64, 
        surplus: u64,
        total_samples: u64,
        channels: &[ChannelId],
    ) -> Result<Vec<f32>> {
        let requested_start = start - surplus as i64;
        let requested_end = (start + count as i64) + surplus as i64;
        
        let valid_start = requested_start.max(0) as u64;
        let valid_end = (requested_end as u64).min(total_samples);
        
        let num_channels = channels.len();
        let total_requested_samples = count + 2 * surplus;
        let mut result = vec![0.0f32; num_channels * total_requested_samples as usize];

        if valid_start >= valid_end {
            return Ok(result);
        }

        let actual_count = valid_end - valid_start;
        let start_padding = (valid_start as i64 - requested_start) as usize;

        for (slot, id) in channels.iter().enumerate() {
            let channel_data = match id {
                ChannelId::Physical(idx) => {
                    self.storage.read_raw_window_masked(valid_start, actual_count, &[*idx])?
                }
                ChannelId::Virtual(name) => {
                    let store = self.virtual_store.as_mut()
                        .context("Processing graph requires virtual channels but no VirtualChannelStore was provided")?;
                    store.read_window(name, valid_start, actual_count, total_samples)?
                }
            };

            let dst_offset = slot * total_requested_samples as usize + start_padding;
            result[dst_offset..dst_offset + actual_count as usize].copy_from_slice(&channel_data);
        }

        Ok(result)
    }
}
