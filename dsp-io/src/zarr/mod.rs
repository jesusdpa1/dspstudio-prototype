use crate::config::StorageConfig;
use crate::metadata::DatasetMetadata;
use dsp_core::signal::Event;
use dsp_core::util::resampling::generate_peaks_parallel;
use zarrs::array::{Array, ArrayBuilder, DataType, FillValue, ArraySubset, BytesToBytesCodecTraits};
use zarrs::array::data_type::{Float32DataType, UInt32DataType, UInt64DataType};
use zarrs::array::codec::ZstdCodec;
use zarrs::group::GroupBuilder;
use zarrs_filesystem::FilesystemStore;
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use anyhow::{Context, Result};

/// Points-per-chunk (along the time axis) for every peak/LOD array, independent
/// of the LOD ratio. Previously the chunk length was `chunk_size / ratio`, which
/// collapsed toward 1 point per chunk at coarse levels — shattering a small array
/// into thousands of tiny files and making any read do far more I/O (one file
/// open + zstd decode per chunk) than the data volume warranted. A fixed length
/// keeps every level chunked sensibly: e.g. lod_4 goes from ~2197 chunks to 1.
const PEAK_CHUNK_POINTS: u64 = 4096;

pub struct StorageManager {
    config: StorageConfig,
    store: Arc<FilesystemStore>,
    /// Cache of opened (read-only) array handles keyed by zarr path. Opening an
    /// array re-reads and parses its `zarr.json`; caching avoids doing that on
    /// every fetch (and previously, once per channel inside a read loop).
    arrays: RwLock<HashMap<String, Arc<Array<FilesystemStore>>>>,
}

impl StorageManager {
    pub fn new(config: StorageConfig) -> Result<Self> {
        let store = Arc::new(FilesystemStore::new(config.raw_archive_path.clone())?);
        Ok(Self { config, store, arrays: RwLock::new(HashMap::new()) })
    }

    pub fn store(&self) -> &Arc<FilesystemStore> {
        &self.store
    }

    /// Returns a cached, shareable handle to the array at `path`, opening it on
    /// first use. Safe for concurrent reads (`retrieve_array_subset` is `&self`).
    fn cached_array(&self, path: &str) -> Result<Arc<Array<FilesystemStore>>> {
        if let Some(a) = self.arrays.read().get(path) {
            return Ok(a.clone());
        }
        let array = Arc::new(Array::open(self.store.clone(), path)?);
        self.arrays.write().insert(path.to_string(), array.clone());
        Ok(array)
    }

    /// Bytes-to-bytes codec pipeline applied to every bulk array at creation.
    /// A single zstd codec at the configured level; neural waveforms and peak
    /// pyramids compress well, cutting both disk footprint and read latency.
    fn compression_codecs(&self) -> Vec<Arc<dyn BytesToBytesCodecTraits>> {
        vec![Arc::new(ZstdCodec::new(self.config.compression_level, false))]
    }

    // ── Reads ────────────────────────────────────────────────────────────────

    /// Reads a window of raw data for the requested channels.
    /// Returns a channel-major `Vec<f32>`: `[ch0_s0..ch0_sN, ch1_s0..ch1_sN, …]`.
    ///
    /// The `/raw` chunk spans all channels (`[channels, chunk_size]`), so a
    /// per-channel subset would force zarrs to decode the same chunk once per
    /// channel. We instead read each maximal run of consecutive channel indices
    /// in a single subset, decoding each intersecting chunk only once. The
    /// row-major result of a `[run_len, count]` subset is already channel-major.
    pub fn read_raw_window_masked(&self, start: u64, count: u64, channels: &[u16]) -> Result<Vec<f32>> {
        let array = self.cached_array("/raw")?;
        let mut result = Vec::with_capacity((channels.len() as u64 * count) as usize);

        for run in contiguous_runs(channels) {
            let subset = ArraySubset::new_with_start_shape(
                vec![run.start as u64, start],
                vec![run.len as u64, count],
            )?;
            let run_data = array.retrieve_array_subset::<Vec<f32>>(&subset)?;
            result.extend(run_data);
        }
        Ok(result)
    }

    /// Reads a window of peak pairs for the requested channels at a given LOD level.
    /// Returns a channel-major `Vec<f32>`:
    /// `[ch0_min0, ch0_max0, ch0_min1, ch0_max1, …, ch1_min0, ch1_max0, …]`.
    pub fn read_peak_window_masked(
        &self,
        level: u8,
        start: u64,
        count: u64,
        channels: &[u16],
    ) -> Result<Vec<f32>> {
        let path = format!("/peaks/lod_{}", level);
        let array = self.cached_array(&path)?;
        let mut result = Vec::with_capacity((channels.len() as u64 * count * 2) as usize);

        // Same chunk-reuse concern as `read_raw_window_masked`: read each run of
        // consecutive channels in one subset so each peak chunk decodes once.
        for run in contiguous_runs(channels) {
            let subset = ArraySubset::new_with_start_shape(
                vec![run.start as u64, start, 0],
                vec![run.len as u64, count, 2],
            )?;
            let run_data = array.retrieve_array_subset::<Vec<f32>>(&subset)?;
            result.extend(run_data);
        }
        Ok(result)
    }

    // ── Writes ───────────────────────────────────────────────────────────────

    pub fn init_hierarchy(&self, metadata: &DatasetMetadata) -> Result<()> {
        let f32_type = DataType::new(Float32DataType);
        GroupBuilder::new().build(self.store.clone(), "/")?.store_metadata()?;

        let raw_array = ArrayBuilder::new(
            vec![self.config.channels as u64, metadata.total_samples],
            vec![self.config.channels as u64, self.config.chunk_size as u64],
            f32_type.clone(),
            FillValue::from(0.0f32),
        )
        .bytes_to_bytes_codecs(self.compression_codecs())
        .build(self.store.clone(), "/raw")?;
        raw_array.store_metadata()?;

        GroupBuilder::new().build(self.store.clone(), "/peaks")?.store_metadata()?;

        for lod in &metadata.lod_chain {
            if lod.level == 0 { continue; }
            let decimated_samples = metadata.total_samples / lod.ratio as u64;
            // Chunk length is fixed in points (not chunk_size/ratio) so coarse
            // levels aren't fragmented into ~1-point chunks. Capped at the array
            // length so tiny coarse arrays stay a single chunk.
            let peak_chunk_pts = PEAK_CHUNK_POINTS.min(decimated_samples).max(1);
            let peak_array = ArrayBuilder::new(
                vec![self.config.channels as u64, decimated_samples, 2],
                vec![
                    self.config.channels as u64,
                    peak_chunk_pts,
                    2,
                ],
                f32_type.clone(),
                FillValue::from(0.0f32),
            )
            .bytes_to_bytes_codecs(self.compression_codecs())
            .build(self.store.clone(), &format!("/peaks/lod_{}", lod.level))?;
            peak_array.store_metadata()?;
        }
        Ok(())
    }

    pub fn write_raw_chunk(&self, chunk_index: u64, data: &[f32]) -> Result<()> {
        let array = Array::open(self.store.clone(), "/raw")?;
        let expected_size = (self.config.channels as usize) * (self.config.chunk_size as usize);
        if data.len() == expected_size {
            array.store_chunk(&vec![0, chunk_index], data)?;
        } else {
            let mut padded = vec![0.0f32; expected_size];
            padded[..data.len()].copy_from_slice(data);
            array.store_chunk(&vec![0, chunk_index], &padded)?;
        }
        Ok(())
    }

    /// Writes a window of peak pairs for all channels at a decimated sample
    /// offset. `peaks` is channel-major interleaved
    /// `[ch0 min,max,min,max…, ch1 …]` with `pts_per_channel` points per channel.
    ///
    /// Uses a subset write rather than `store_chunk`, so it is independent of the
    /// array's chunk shape — the peak chunking ([`PEAK_CHUNK_POINTS`]) no longer
    /// has to match the raw chunk size.
    pub fn write_peak_window(
        &self,
        lod_level: u8,
        dec_offset: u64,
        pts_per_channel: u64,
        peaks: &[f32],
    ) -> Result<()> {
        if pts_per_channel == 0 {
            return Ok(());
        }
        let array = self.cached_array(&format!("/peaks/lod_{}", lod_level))?;
        let subset = ArraySubset::new_with_start_shape(
            vec![0, dec_offset, 0],
            vec![self.config.channels as u64, pts_per_channel, 2],
        )?;
        array.store_array_subset(&subset, peaks)?;
        Ok(())
    }

    // ── Peak pyramid ─────────────────────────────────────────────────────────

    // ── Events (Epochs::Events tracks) ───────────────────────────────────────
    //
    // Layout under the zarr root:
    //   /events/{track_name}/sample_offsets   shape [n_channels, n_events]  u64
    //   /events/{track_name}/label_ids        shape [n_channels, n_events]  u32
    //
    // All channels share the same n_events dimension (the maximum across
    // channels).  Unused slots are padded with u64::MAX / 0.  The actual
    // count per channel is recovered by scanning for u64::MAX sentinels or
    // by reading from the RecordingMeta sidecar.
    //
    // This write-once layout suits post-processing output (spike detection
    // ran to completion → write results).  Streaming appends (Task 4) will
    // extend this with a separate mmap-backed buffer.

    /// Write all events for every channel of one track in a single operation.
    ///
    /// `events_per_channel[c]` is the sorted list of events for channel `c`.
    /// All channels must be sorted by `sample_offset` before calling.
    pub fn write_events_track(
        &self,
        track_name: &str,
        events_per_channel: &[Vec<Event>],
    ) -> Result<()> {
        let n_channels = events_per_channel.len();
        if n_channels == 0 {
            return Ok(());
        }
        let n_events = events_per_channel.iter().map(|v| v.len()).max().unwrap_or(0);
        if n_events == 0 {
            return Ok(());
        }

        let group_path = format!("/events/{}", track_name);
        GroupBuilder::new()
            .build(self.store.clone(), &group_path)?
            .store_metadata()?;

        // Build flat padded arrays: [n_channels * n_events]
        let mut offsets = vec![u64::MAX; n_channels * n_events];
        let mut labels  = vec![0u32;    n_channels * n_events];

        for (c, ch_events) in events_per_channel.iter().enumerate() {
            for (e, ev) in ch_events.iter().enumerate() {
                offsets[c * n_events + e] = ev.sample_offset;
                labels [c * n_events + e] = ev.label_id;
            }
        }

        let chunk_events = 1024usize.min(n_events);

        // sample_offsets array
        let offsets_path = format!("{}/sample_offsets", group_path);
        let off_array = ArrayBuilder::new(
            vec![n_channels as u64, n_events as u64],
            vec![1, chunk_events as u64],
            DataType::new(UInt64DataType),
            FillValue::from(u64::MAX),
        )
        .bytes_to_bytes_codecs(self.compression_codecs())
        .build(self.store.clone(), &offsets_path)
        .context("building sample_offsets array")?;
        off_array.store_metadata()?;
        off_array.store_array_subset(
            &ArraySubset::new_with_shape(vec![n_channels as u64, n_events as u64]),
            offsets.as_slice(),
        )?;

        // label_ids array
        let labels_path = format!("{}/label_ids", group_path);
        let lbl_array = ArrayBuilder::new(
            vec![n_channels as u64, n_events as u64],
            vec![1, chunk_events as u64],
            DataType::new(UInt32DataType),
            FillValue::from(0u32),
        )
        .bytes_to_bytes_codecs(self.compression_codecs())
        .build(self.store.clone(), &labels_path)
        .context("building label_ids array")?;
        lbl_array.store_metadata()?;
        lbl_array.store_array_subset(
            &ArraySubset::new_with_shape(vec![n_channels as u64, n_events as u64]),
            labels.as_slice(),
        )?;

        Ok(())
    }

    /// Read all events for a single channel from an events track.
    ///
    /// Sentinel entries (`sample_offset == u64::MAX`) are stripped.
    pub fn read_events_channel(
        &self,
        track_name: &str,
        channel_idx: u16,
    ) -> Result<Vec<Event>> {
        let group_path = format!("/events/{}", track_name);
        let off_array = self.cached_array(&format!("{}/sample_offsets", group_path))?;

        let shape = off_array.shape();
        let n_events = shape[1];

        let subset = ArraySubset::new_with_start_shape(
            vec![channel_idx as u64, 0],
            vec![1, n_events],
        )?;

        let offsets = off_array.retrieve_array_subset::<Vec<u64>>(&subset)?;

        let lbl_array = self.cached_array(&format!("{}/label_ids", group_path))?;
        let labels = lbl_array.retrieve_array_subset::<Vec<u32>>(&subset)?;

        let events = offsets
            .into_iter()
            .zip(labels)
            .filter(|(off, _)| *off != u64::MAX)
            .map(|(off, lbl)| Event::new(off, lbl))
            .collect();

        Ok(events)
    }

    /// Write pre-extracted waveforms, PCA features, offsets and labels for a track (flat layout).
    ///
    /// Offsets: [n_spikes]
    /// Labels: [n_spikes]
    /// Waveforms: [n_spikes, snippet_len]
    /// PCA Features: [n_spikes, n_pcs]
    pub fn write_spike_artifacts(
        &self,
        track_name: &str,
        offsets: &[u64],
        labels: &[u32],
        waveforms: &[f32],
        pca_features: &[f32],
        n_spikes: usize,
        snippet_len: usize,
        n_pcs: usize,
    ) -> Result<()> {
        let group_path = format!("/events/{}", track_name);
        if !self.events_track_exists(track_name) {
            GroupBuilder::new()
                .build(self.store.clone(), &group_path)?
                .store_metadata()?;
        }

        let f32_type = DataType::new(Float32DataType);
        let u64_type = DataType::new(UInt64DataType);
        let u32_type = DataType::new(UInt32DataType);

        let chunk_size = 1024.min(n_spikes) as u64;

        // 1. Offsets array
        let off_array = ArrayBuilder::new(
            vec![n_spikes as u64],
            vec![chunk_size],
            u64_type,
            FillValue::from(u64::MAX),
        )
        .bytes_to_bytes_codecs(self.compression_codecs())
        .build(self.store.clone(), &format!("{}/flat_offsets", group_path))?;
        off_array.store_metadata()?;
        off_array.store_array_subset(&ArraySubset::new_with_shape(vec![n_spikes as u64]), offsets)?;

        // 2. Labels array
        let lbl_array = ArrayBuilder::new(
            vec![n_spikes as u64],
            vec![chunk_size],
            u32_type,
            FillValue::from(0u32),
        )
        .bytes_to_bytes_codecs(self.compression_codecs())
        .build(self.store.clone(), &format!("{}/flat_labels", group_path))?;
        lbl_array.store_metadata()?;
        lbl_array.store_array_subset(&ArraySubset::new_with_shape(vec![n_spikes as u64]), labels)?;

        // 3. Waveforms array
        let waveforms_path = format!("{}/waveforms", group_path);
        let wf_array = ArrayBuilder::new(
            vec![n_spikes as u64, snippet_len as u64],
            vec![chunk_size, snippet_len as u64],
            f32_type.clone(),
            FillValue::from(0.0f32),
        )
        .bytes_to_bytes_codecs(self.compression_codecs())
        .build(self.store.clone(), &waveforms_path)
        .context("building waveforms array")?;
        wf_array.store_metadata()?;
        wf_array.store_array_subset(
            &ArraySubset::new_with_shape(vec![n_spikes as u64, snippet_len as u64]),
            waveforms,
        )?;

        // 4. PCA array
        let pca_path = format!("{}/pca", group_path);
        let pca_array = ArrayBuilder::new(
            vec![n_spikes as u64, n_pcs as u64],
            vec![chunk_size, n_pcs as u64],
            f32_type.clone(),
            FillValue::from(0.0f32),
        )
        .bytes_to_bytes_codecs(self.compression_codecs())
        .build(self.store.clone(), &pca_path)
        .context("building pca array")?;
        pca_array.store_metadata()?;
        pca_array.store_array_subset(
            &ArraySubset::new_with_shape(vec![n_spikes as u64, n_pcs as u64]),
            pca_features,
        )?;

        Ok(())
    }

    /// Read a subset of spike waveforms and PCA features by their indices.
    pub fn read_spike_artifacts_subset(
        &self,
        track_name: &str,
        indices: &[usize],
        snippet_len: usize,
        n_pcs: usize,
    ) -> Result<(Vec<f32>, Vec<f32>)> {
        let group_path = format!("/events/{}", track_name);
        
        let wf_array = self.cached_array(&format!("{}/waveforms", group_path))
            .context("opening waveforms array")?;

        let pca_array = self.cached_array(&format!("{}/pca", group_path))
            .context("opening pca array")?;

        let mut waveforms = Vec::with_capacity(indices.len() * snippet_len);
        let mut pca_features = Vec::with_capacity(indices.len() * n_pcs);

        for &idx in indices {
            // Read waveform snippet
            let wf_subset = ArraySubset::new_with_start_shape(
                vec![idx as u64, 0],
                vec![1, snippet_len as u64],
            )?;
            let wf_data = wf_array.retrieve_array_subset::<Vec<f32>>(&wf_subset)?;
            waveforms.extend(wf_data);

            // Read PCA features
            let pca_subset = ArraySubset::new_with_start_shape(
                vec![idx as u64, 0],
                vec![1, n_pcs as u64],
            )?;
            let pca_data = pca_array.retrieve_array_subset::<Vec<f32>>(&pca_subset)?;
            pca_features.extend(pca_data);
        }

        Ok((waveforms, pca_features))
    }

    /// Checks if flat artifacts (waveforms/pca) exist for a track.
    pub fn has_flat_artifacts(&self, track_name: &str) -> bool {
        Array::open(self.store.clone(), &format!("/events/{}/waveforms", track_name)).is_ok() &&
        Array::open(self.store.clone(), &format!("/events/{}/pca", track_name)).is_ok()
    }

    /// Reads indices of spikes matching a label from the flat label array.
    pub fn read_flat_indices_for_label(&self, track_name: &str, label_id: u32) -> Result<Vec<usize>> {
        let lbl_array = self.cached_array(&format!("/events/{}/flat_labels", track_name))?;
        let labels = lbl_array.retrieve_array_subset::<Vec<u32>>(&ArraySubset::new_with_shape(lbl_array.shape().to_vec()))?;
        
        Ok(labels.into_iter().enumerate()
            .filter(|(_, l)| *l == label_id)
            .map(|(i, _)| i)
            .collect())
    }

    /// Read events for a channel within a sample range `[start, end)`.
    pub fn read_events_window(
        &self,
        track_name: &str,
        channel_idx: u16,
        start_sample: u64,
        end_sample: u64,
    ) -> Result<Vec<Event>> {
        let all = self.read_events_channel(track_name, channel_idx)?;
        Ok(all
            .into_iter()
            .filter(|e| e.sample_offset >= start_sample && e.sample_offset < end_sample)
            .collect())
    }

    /// Returns `true` if an events track with the given name exists in the store.
    pub fn events_track_exists(&self, track_name: &str) -> bool {
        Array::open(
            self.store.clone(),
            &format!("/events/{}/sample_offsets", track_name),
        )
        .is_ok()
    }

    /// Returns true if at least one LOD peak level exists in the store.
    pub fn peak_pyramid_exists(&self) -> bool {
        Array::open(self.store.clone(), "/peaks/lod_1").is_ok()
    }

    /// Builds the full peak pyramid from raw data and writes it to the store.
    ///
    /// Reads raw chunks, computes min-max peaks at each LOD level, and stores
    /// the results. `progress` is called with values in `0.0..=1.0`.
    ///
    /// Should be called from a `spawn_blocking` context — not the render thread.
    pub fn build_peak_pyramid(
        &self,
        metadata: &DatasetMetadata,
        progress: impl Fn(f32) + Send,
    ) -> Result<()> {
        let lod_levels: Vec<_> = metadata.lod_chain.iter()
            .filter(|l| l.level > 0)
            .collect();

        if lod_levels.is_empty() {
            return Ok(());
        }

        let total_chunks = (metadata.total_samples + self.config.chunk_size as u64 - 1)
            / self.config.chunk_size as u64;

        for (chunk_idx, chunk_start) in (0..metadata.total_samples)
            .step_by(self.config.chunk_size)
            .enumerate()
        {
            let chunk_count = (self.config.chunk_size as u64)
                .min(metadata.total_samples - chunk_start);

            // Read all channels for this raw chunk.
            let all_channels: Vec<u16> = (0..self.config.channels).collect();
            let raw = self.read_raw_window_masked(chunk_start, chunk_count, &all_channels)?;

            // Build peaks at each LOD level and write them at their decimated
            // offset (no per-chunk padding; chunk shape is fixed independently).
            let channels = self.config.channels as usize;
            for lod in &lod_levels {
                let ratio = lod.ratio as usize;

                let peaks = generate_peaks_parallel(&raw, channels, ratio);
                let pts_per_channel = peaks.len() / channels;
                if pts_per_channel == 0 {
                    continue;
                }

                // Flatten to channel-major interleaved [ch0 min,max,…, ch1 …].
                let mut flat = vec![0.0f32; channels * pts_per_channel * 2];
                for c in 0..channels {
                    let src = c * pts_per_channel;
                    let dst = c * pts_per_channel * 2;
                    for i in 0..pts_per_channel {
                        flat[dst + i * 2] = peaks[src + i].min;
                        flat[dst + i * 2 + 1] = peaks[src + i].max;
                    }
                }

                let dec_offset = chunk_start / lod.ratio as u64;
                self.write_peak_window(lod.level, dec_offset, pts_per_channel as u64, &flat)?;
            }

            progress(chunk_idx as f32 / total_chunks as f32);
        }

        progress(1.0);
        Ok(())
    }
}

/// A maximal run of consecutive channel indices within a request.
struct ChannelRun {
    start: u16,
    len: u16,
}

/// Splits a channel list into maximal runs of consecutive ascending indices,
/// preserving order. `[0,1,2,5,6]` → `[(0,3),(5,2)]`; `[3,1,2]` → `[(3,1),(1,2)]`.
/// Each run can be read from a multi-channel chunk in a single subset, so a
/// chunk that spans many channels is decoded once per run rather than per channel.
fn contiguous_runs(channels: &[u16]) -> Vec<ChannelRun> {
    let mut runs: Vec<ChannelRun> = Vec::new();
    for &ch in channels {
        match runs.last_mut() {
            Some(run) if run.start + run.len == ch => run.len += 1,
            _ => runs.push(ChannelRun { start: ch, len: 1 }),
        }
    }
    runs
}

#[cfg(test)]
mod run_tests {
    use super::contiguous_runs;

    fn flat(channels: &[u16]) -> Vec<(u16, u16)> {
        contiguous_runs(channels).iter().map(|r| (r.start, r.len)).collect()
    }

    #[test]
    fn groups_consecutive_and_preserves_order() {
        assert_eq!(flat(&[0, 1, 2, 3]), vec![(0, 4)]);
        assert_eq!(flat(&[0, 1, 2, 5, 6]), vec![(0, 3), (5, 2)]);
        assert_eq!(flat(&[5]), vec![(5, 1)]);
        assert_eq!(flat(&[]), vec![]);
        // Non-ascending input is not merged across the discontinuity.
        assert_eq!(flat(&[3, 1, 2]), vec![(3, 1), (1, 2)]);
    }
}
