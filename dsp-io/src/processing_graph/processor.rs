use crate::processing_history::ProcessingHistory;
use crate::recording_meta::VirtualChannelMeta;
use crate::transmission::processing::ProcessingService;
use crate::virtual_channel::VirtualChannelStore;
use crate::zarr::StorageManager;
use anyhow::{Result, bail};
use dsp_core::signal::Event;
use rayon::prelude::*;
use super::{ProcessingGraphSpec, SpecNode, ChannelId, SignalValue, uf_union, uf_find};

/// Executes a [`ProcessingGraphSpec`] over an entire recording using a
/// chunk-by-chunk sliding window, writing results to a [`VirtualChannelStore`].
pub struct GraphProcessor {
    pub spec: ProcessingGraphSpec,
}

impl GraphProcessor {
    pub fn new(spec: ProcessingGraphSpec) -> Self {
        Self { spec }
    }

    /// Pre-compute all designed IIR filter coefficients (Butterworth, Cheby, etc.)
    pub fn compile_filters(
        &self,
    ) -> Result<std::collections::HashMap<usize, dsp_core::filter::FilterDesign>> {
        let fs = self.spec.sample_rate as f64;
        let mut map = std::collections::HashMap::new();
        for (idx, node) in self.spec.nodes.iter().enumerate() {
            let design = match node {
                SpecNode::Butterworth {
                    order, response, ..
                } => Some(dsp_core::filter::butterworth(*order, *response, fs)),
                SpecNode::ChebyshevI {
                    order,
                    ripple_db,
                    response,
                    ..
                } => Some(dsp_core::filter::chebyshev1(
                    *order, *ripple_db, *response, fs,
                )),
                SpecNode::ChebyshevII {
                    order,
                    atten_db,
                    response,
                    ..
                } => Some(dsp_core::filter::chebyshev2(
                    *order, *atten_db, *response, fs,
                )),
                SpecNode::Bessel {
                    order, response, ..
                } => {
                    if *order < 1 || *order > 8 {
                        return Err(anyhow::anyhow!(
                            "Bessel filter at node {} requires order 1–8, got {}",
                            idx, order
                        ));
                    }
                    Some(dsp_core::filter::bessel(*order, *response, fs))
                }
                SpecNode::Notch { freq_hz, q, .. } => {
                    Some(dsp_core::filter::notch(*freq_hz, *q, fs))
                }
                SpecNode::PeakEq {
                    freq_hz,
                    q,
                    gain_db,
                } => Some(dsp_core::filter::peak_eq(*freq_hz, *q, *gain_db, fs)),
                _ => None,
            };
            if let Some(d) = design {
                // Validate that the design produced finite coefficients.
                for section in &d.sos {
                    if section
                        .b
                        .iter()
                        .chain(section.a.iter())
                        .any(|&v| !v.is_finite())
                    {
                        return Err(anyhow::anyhow!(
                            "Filter design at node {} produced non-finite coefficients",
                            idx
                        ));
                    }
                }
                map.insert(idx, d);
            }
        }
        Ok(map)
    }

    /// Runs the graph over the full recording.
    pub fn run_full_recording(
        &self,
        manager: &StorageManager,
        total_samples: u64,
        start_sample: u64,
        count: u64,
        batch_size: u64,
        surplus: u64,
        store: &mut VirtualChannelStore,
        zarr_path: Option<&std::path::Path>,
        mut progress: impl FnMut(f32),
    ) -> Result<Vec<VirtualChannelMeta>> {
        // Validate Fork names.
        for node in &self.spec.nodes {
            if let SpecNode::Fork { name, .. } = node {
                if name.ends_with("_drv") {
                    bail!("Fork channel name '{}' must not end with '_drv'", name);
                }
            }
        }

        let total_batches = count.div_ceil(batch_size);

        for (name, _) in self.spec.declared_outputs() {
            store.open_or_create(&name, total_samples)?;
        }

        // Events accumulators keyed by (track_name, channel_idx).
        let mut events_accum: std::collections::HashMap<(String, u16), Vec<Event>> =
            std::collections::HashMap::new();
        for (track_name, channel_idx, _) in self.spec.declared_events_outputs() {
            events_accum.entry((track_name, channel_idx)).or_default();
        }

        let compiled_filters = self.compile_filters()?;
        let required_surplus = self.spec.required_surplus_compiled(&compiled_filters);
        let surplus = surplus.max(required_surplus);
        let needed = self.needed_channels();

        let mut service = ProcessingService::new(manager, Some(store));

        for batch_idx in 0..total_batches {
            let core_start = start_sample + batch_idx * batch_size;
            let core_len = batch_size.min(count - batch_idx * batch_size);

            let fetched = service.fetch_package_with_surplus(
                core_start as i64,
                core_len,
                surplus,
                total_samples,
                &needed,
            )?;
            let fetched_len = (core_len + 2 * surplus) as usize;

            let raw: std::collections::HashMap<ChannelId, Vec<f32>> = needed
                .iter()
                .enumerate()
                .map(|(slot, id)| {
                    let s = &fetched[slot * fetched_len..(slot + 1) * fetched_len];
                    (id.clone(), s.to_vec())
                })
                .collect();

            let node_signals = self.evaluate_graph(&raw, fetched_len, &compiled_filters)?;

            let surplus_us = surplus as usize;
            let pre_offset: i64 = core_start as i64 - surplus as i64;

            for (node_idx, node) in self.spec.nodes.iter().enumerate() {
                let (name, pin): (String, usize) = match node {
                    SpecNode::Output { source_id } => (source_id.drv_name(), 0),
                    SpecNode::Fork { name, .. } => (name.clone(), 0),
                    SpecNode::MultiChannelOutput { names, .. } => {
                        for (pin, name) in names.iter().enumerate() {
                            if let Some(sv) = node_signals.get(&(node_idx, pin)) {
                                if let Some(sigs) = sv.as_waveform() {
                                    let core = &sigs[surplus_us
                                        ..(surplus_us + core_len as usize).min(sigs.len())];
                                    service.virtual_store.as_mut().unwrap().write_window(
                                        name,
                                        core_start,
                                        total_samples,
                                        core,
                                    )?;
                                }
                            }
                        }
                        continue;
                    }
                    SpecNode::EventsOutput {
                        track_name,
                        channel_idx,
                        ..
                    } => {
                        if let Some(sv) = node_signals.get(&(node_idx, 0)) {
                            if let Some(evs) = sv.as_events() {
                                let bucket = events_accum
                                    .entry((track_name.clone(), *channel_idx))
                                    .or_default();
                                for ev in evs {
                                    let abs = pre_offset + ev.sample_offset as i64;
                                    if abs >= core_start as i64
                                        && abs < (core_start + core_len) as i64
                                    {
                                        bucket.push(Event::new(abs as u64, ev.label_id));
                                    }
                                }
                            }
                        }
                        continue;
                    }
                    _ => continue,
                };

                if let Some(sv) = node_signals.get(&(node_idx, pin)) {
                    if let Some(sigs) = sv.as_waveform() {
                        let core =
                            &sigs[surplus_us..(surplus_us + core_len as usize).min(sigs.len())];
                        service.virtual_store.as_mut().unwrap().write_window(
                            &name,
                            core_start,
                            total_samples,
                            core,
                        )?;
                    }
                }
            }

            progress((batch_idx + 1) as f32 / total_batches as f32);
        }

        service.virtual_store.as_mut().unwrap().flush_all()?;

        if !events_accum.is_empty() {
            let mut by_track: std::collections::HashMap<
                String,
                std::collections::BTreeMap<u16, Vec<Event>>,
            > = std::collections::HashMap::new();
            for ((track_name, channel_idx), events) in events_accum {
                by_track
                    .entry(track_name)
                    .or_default()
                    .insert(channel_idx, events);
            }

            if let Some(path) = zarr_path {
                if let Ok(mut meta) = crate::recording_meta::RecordingMeta::load(path) {
                    for (track_name, channel_map) in by_track {
                        let max_ch = *channel_map.keys().max().unwrap_or(&0);
                        let mut per_channel: Vec<Vec<Event>> =
                            vec![Vec::new(); max_ch as usize + 1];
                        for (ch, evs) in channel_map {
                            per_channel[ch as usize] = evs;
                        }
                        let _ = manager.write_events_track(&track_name, &per_channel);

                        let channel_indices: Vec<u16> = (0..=max_ch).collect();
                        let mut vocab = dsp_core::signal::LabelVocabulary::default();
                        vocab.get_or_insert("Positive");
                        vocab.get_or_insert("Negative");
                        vocab.get_or_insert("Enter");
                        vocab.get_or_insert("Exit");

                        let new_track = crate::recording_meta::TrackMeta::events(
                            track_name.clone(),
                            channel_indices,
                            vocab,
                        );

                        if let Some(existing) =
                            meta.tracks.iter_mut().find(|t| t.name == track_name)
                        {
                            *existing = new_track;
                        } else {
                            meta.tracks.push(new_track);
                        }
                    }
                    let _ = meta.save(path);
                }
            } else {
                for (track_name, channel_map) in by_track {
                    let max_ch = *channel_map.keys().max().unwrap_or(&0) as usize;
                    let mut per_channel: Vec<Vec<Event>> = vec![Vec::new(); max_ch + 1];
                    for (ch, evs) in channel_map {
                        per_channel[ch as usize] = evs;
                    }
                    let _ = manager.write_events_track(&track_name, &per_channel);
                }
            }
        }

        if let Some(path) = zarr_path {
            let spec_json = serde_json::to_string(&self.spec).unwrap_or_default();
            let label = self.spec.auto_label();
            let mut history = ProcessingHistory::load(path).unwrap_or_default();
            for (name, _) in self.spec.declared_outputs() {
                history.append(&name, &label, &spec_json);
            }
            let _ = history.save(path);
        }

        let meta = self
            .spec
            .declared_outputs()
            .into_iter()
            .map(|(name, src)| {
                let src_idx = match src {
                    ChannelId::Physical(idx) => idx,
                    ChannelId::Virtual(_) => 0,
                };
                VirtualChannelMeta::new(name, src_idx)
            })
            .collect();
        Ok(meta)
    }

    pub fn needed_channels(&self) -> Vec<ChannelId> {
        let mut set = std::collections::HashSet::new();
        for node in &self.spec.nodes {
            match node {
                SpecNode::Channel { id } => {
                    set.insert(id.clone());
                }
                SpecNode::MultiChannel { ids } => {
                    set.extend(ids.iter().cloned());
                }
                _ => {}
            }
        }
        set.into_iter().collect()
    }

    pub(crate) fn independent_groups(&self) -> Vec<Vec<usize>> {
        let n = self.spec.nodes.len();
        let mut parent: Vec<usize> = (0..n).collect();

        for wire in &self.spec.wires {
            uf_union(&mut parent, wire.from_node, wire.to_node);
        }

        let mut groups: std::collections::HashMap<usize, Vec<usize>> =
            std::collections::HashMap::new();
        for i in 0..n {
            let root = uf_find(&mut parent, i);
            groups.entry(root).or_default().push(i);
        }
        groups.into_values().collect()
    }

    pub fn evaluate_graph(
        &self,
        raw: &std::collections::HashMap<ChannelId, Vec<f32>>,
        window_len: usize,
        compiled_filters: &std::collections::HashMap<usize, dsp_core::filter::FilterDesign>,
    ) -> Result<std::collections::HashMap<(usize, usize), SignalValue>> {
        let global_order = self.topological_order()?;
        let groups = self.independent_groups();

        let mut wire_map = std::collections::HashMap::new();
        for wire in &self.spec.wires {
            wire_map.insert(
                (wire.to_node, wire.to_input),
                (wire.from_node, wire.from_output),
            );
        }

        if groups.len() <= 1 {
            return self.evaluate_group(
                &global_order,
                raw,
                window_len,
                compiled_filters,
                &wire_map,
            );
        }

        let group_orders: Vec<Vec<usize>> = groups
            .iter()
            .map(|g| {
                let set: std::collections::HashSet<usize> = g.iter().cloned().collect();
                global_order
                    .iter()
                    .filter(|&&i| set.contains(&i))
                    .cloned()
                    .collect()
            })
            .collect();

        group_orders
            .par_iter()
            .map(|order| self.evaluate_group(order, raw, window_len, compiled_filters, &wire_map))
            .reduce(
                || Ok(std::collections::HashMap::new()),
                |acc, part| {
                    let mut acc = acc?;
                    acc.extend(part?);
                    Ok(acc)
                },
            )
    }

    fn evaluate_group(
        &self,
        order: &[usize],
        raw: &std::collections::HashMap<ChannelId, Vec<f32>>,
        window_len: usize,
        compiled_filters: &std::collections::HashMap<usize, dsp_core::filter::FilterDesign>,
        wire_map: &std::collections::HashMap<(usize, usize), (usize, usize)>,
    ) -> Result<std::collections::HashMap<(usize, usize), SignalValue>> {
        let mut signals: std::collections::HashMap<(usize, usize), SignalValue> =
            std::collections::HashMap::new();

        for &node_idx in order {
            super::nodes::evaluate_node(
                &self.spec,
                node_idx,
                &mut signals,
                raw,
                window_len,
                compiled_filters,
                wire_map,
            )?;
        }

        Ok(signals)
    }

    pub fn topological_order(&self) -> Result<Vec<usize>> {
        let n = self.spec.nodes.len();
        let mut in_degree = vec![0usize; n];
        let mut adj: Vec<Vec<usize>> = vec![vec![]; n];

        for wire in &self.spec.wires {
            adj[wire.from_node].push(wire.to_node);
            in_degree[wire.to_node] += 1;
        }

        let mut queue: std::collections::VecDeque<usize> =
            (0..n).filter(|&i| in_degree[i] == 0).collect();

        let mut order = Vec::with_capacity(n);
        while let Some(node) = queue.pop_front() {
            order.push(node);
            for &next in &adj[node] {
                in_degree[next] -= 1;
                if in_degree[next] == 0 {
                    queue.push_back(next);
                }
            }
        }

        if order.len() != n {
            bail!("Processing graph contains a cycle or invalid node indices");
        }

        Ok(order)
    }
}
