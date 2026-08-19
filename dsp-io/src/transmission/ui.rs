use crate::zarr::StorageManager;
use crate::virtual_channel::VirtualChannelStore;
use crate::metadata::DatasetMetadata;
use crate::processing_graph::ChannelId;
use anyhow::{Result, Context};

/// Returned by [`UiService::fetch_view`].
///
/// Data layout:
/// - `lod_level == 0` (raw): `[ch0_s0, ch0_s1, …, ch1_s0, …]`
///   — `points_per_channel` floats per channel.
/// - `lod_level > 0` (peaks): `[ch0_min0, ch0_max0, ch0_min1, ch0_max1, …, ch1_min0, …]`
///   — `points_per_channel * 2` floats per channel (interleaved min/max pairs).
#[derive(Debug, Clone)]
pub struct ViewResponse {
    pub data: Vec<f32>,
    pub lod_level: u8,
    pub decimation_ratio: u64,
    pub points_per_channel: usize,
    pub channels_returned: Vec<ChannelId>,
    /// The actual first sample included in `data` after LOD-boundary snapping.
    /// May be ≤ the requested `start_sample`. Use this — not the request's
    /// `start_sample` — as the x-axis origin when rendering.
    pub actual_start: u64,
}

/// Returned by [`UiService::fetch_cluster_data`].
#[derive(Debug, Clone, Default)]
pub struct ClusterData {
    pub label_id: u32,
    pub pca_pc1: Vec<f32>,
    pub pca_pc2: Vec<f32>,
    pub waveforms: Vec<f32>,
    pub mean_waveform: Vec<f32>,
    pub std_waveform: Vec<f32>,
    pub snippet_len: usize,
    pub n_spikes: usize,
}

/// Resolution-aware viewport service for the UI layer.
pub struct UiService<'a> {
    storage: &'a StorageManager,
    virtual_store: Option<&'a mut VirtualChannelStore>,
}

impl<'a> UiService<'a> {
    pub fn new(storage: &'a StorageManager, virtual_store: Option<&'a mut VirtualChannelStore>) -> Self {
        Self { storage, virtual_store }
    }

    /// Selects the coarsest LOD level that still provides ≥1 peak per pixel.
    pub fn get_optimal_zarr_lod(&self, metadata: &DatasetMetadata, count: u64, width_px: u32) -> u8 {
        // Guard against a degenerate width (e.g. an unlaid-out panel): a 0 here
        // would make every level satisfy `>= 0` and pick the coarsest LOD.
        let width_px = width_px.max(1) as u64;
        let mut best_level = 0u8;
        for lod in metadata.lod_chain.iter().rev() {
            if (count / lod.ratio as u64) >= width_px {
                best_level = lod.level;
                break;
            }
        }
        best_level
    }

    /// Fetches a viewport of data, automatically resolving physical vs virtual channels.
    ///
    /// Uses pre-calculated LODs for physical channels and decimates virtual
    /// channels on-the-fly to the same ratio to ensure alignment.
    pub fn fetch_view(
        &mut self,
        metadata: &DatasetMetadata,
        start_sample: u64,
        count: u64,
        width_px: u32,
        channels: &[ChannelId],
    ) -> Result<ViewResponse> {
        let lod_level = self.get_optimal_zarr_lod(metadata, count, width_px);
        let ratio = 16u64.pow(lod_level as u32);
        
        // Snap the window to the LOD ratio boundaries to ensure efficient Zarr reads.
        let snapped_start = (start_sample / ratio) * ratio;
        let snapped_end = ((start_sample + count + ratio - 1) / ratio) * ratio;
        let snapped_count = snapped_end - snapped_start;
        
        let lod_start = snapped_start / ratio;
        let lod_count = snapped_count / ratio;

        let mut merged_data = Vec::new();
        
        for id in channels {
            match id {
                ChannelId::Physical(idx) => {
                    if lod_level == 0 {
                        let data = self.storage.read_raw_window_masked(snapped_start, snapped_count, &[*idx])?;
                        merged_data.extend(data);
                    } else {
                        let data = self.storage.read_peak_window_masked(lod_level, lod_start, lod_count, &[*idx])?;
                        merged_data.extend(data);
                    }
                }
                ChannelId::Virtual(name) => {
                    let store = self.virtual_store.as_mut().context("Virtual store missing")?;
                    let raw = store.read_window(name, snapped_start, snapped_count, metadata.total_samples)?;
                    
                    if lod_level == 0 {
                        merged_data.extend(raw);
                    } else {
                        // Decimate the virtual channel on-the-fly to match the Zarr LOD ratio.
                        let peaks = dsp_core::util::resampling::generate_peaks_parallel(&raw, 1, ratio as usize);
                        
                        // Flatten peaks into [min0, max0, min1, max1, ...]
                        let mut flat: Vec<f32> = peaks.iter()
                            .flat_map(|p| [p.min, p.max])
                            .collect();
                        
                        // Ensure the decimated data exactly matches the expected lod_count.
                        // Padding with zeros if necessary (shouldn't happen with snapped coords but for safety).
                        if flat.len() < lod_count as usize * 2 {
                            flat.resize(lod_count as usize * 2, 0.0);
                        } else if flat.len() > lod_count as usize * 2 {
                            flat.truncate(lod_count as usize * 2);
                        }
                        
                        merged_data.extend(flat);
                    }
                }
            }
        }

        Ok(ViewResponse {
            data: merged_data,
            lod_level,
            decimation_ratio: ratio,
            points_per_channel: lod_count as usize,
            channels_returned: channels.to_vec(),
            actual_start: snapped_start,
        })
    }

    /// Fetches subsampled PCA and waveforms for a specific cluster.
    pub fn fetch_cluster_data(
        &self,
        track_name: &str,
        label_id: u32,
        max_waveforms: u32,
        _snippet_before: u32,
        _snippet_after: u32,
    ) -> Result<ClusterData> {
        if !self.storage.has_flat_artifacts(track_name) {
            return Err(anyhow::anyhow!("Track '{}' has no flat spike artifacts.", track_name));
        }

        // ── PATH A: Read pre-processed flat artifacts ────────────────────
        let matching_indices = self.storage.read_flat_indices_for_label(track_name, label_id)?;

        if matching_indices.is_empty() {
            return Ok(ClusterData::default());
        }

        let n_total = matching_indices.len();
        let n_to_fetch = (max_waveforms as usize).min(n_total);
        let selected_indices = if n_to_fetch < n_total {
            let stride = n_total as f32 / n_to_fetch as f32;
            (0..n_to_fetch).map(|i| matching_indices[(i as f32 * stride) as usize]).collect::<Vec<_>>()
        } else {
            matching_indices
        };

        let wf_array = zarrs::array::Array::open(self.storage.store().clone(), &format!("/events/{}/waveforms", track_name))?;
        let snippet_len = wf_array.shape()[1] as usize;

        let pca_array = zarrs::array::Array::open(self.storage.store().clone(), &format!("/events/{}/pca", track_name))?;
        let n_pcs = pca_array.shape()[1] as usize;

        let (waveforms, pca_features) = self.storage.read_spike_artifacts_subset(track_name, &selected_indices, snippet_len, n_pcs)?;

        let mut pc1 = vec![0.0f32; n_to_fetch];
        let mut pc2 = vec![0.0f32; n_to_fetch];
        if n_pcs >= 2 {
            for i in 0..n_to_fetch {
                pc1[i] = pca_features[i * n_pcs];
                pc2[i] = pca_features[i * n_pcs + 1];
            }
        }

        // Compute Stats
        let mut mean_waveform = vec![0.0f32; snippet_len];
        let mut std_waveform = vec![0.0f32; snippet_len];

        for t in 0..snippet_len {
            let mut sum = 0.0f64;
            for s in 0..n_to_fetch {
                sum += waveforms[s * snippet_len + t] as f64;
            }
            let mean = (sum / n_to_fetch as f64) as f32;
            mean_waveform[t] = mean;

            let mut sum_sq_diff = 0.0f64;
            for s in 0..n_to_fetch {
                let val = waveforms[s * snippet_len + t] as f64;
                let diff = val - mean as f64;
                sum_sq_diff += diff * diff;
            }
            std_waveform[t] = (sum_sq_diff / n_to_fetch as f64).sqrt() as f32;
        }

        Ok(ClusterData {
            label_id,
            pca_pc1: pc1,
            pca_pc2: pc2,
            waveforms,
            mean_waveform,
            std_waveform,
            snippet_len,
            n_spikes: n_to_fetch,
        })
    }
}
