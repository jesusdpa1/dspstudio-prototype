use anyhow::Result;
use dsp_core::detection::{SingleThresholdDetector, CrossingDirection, DetectionDetector};
use dsp_core::extraction::snippet::extract_snippets;
use dsp_core::filter::iir::butterworth;
use dsp_core::filter::FilterResponse;
use dsp_core::math::interpolation::shift_waveform_cubic;
use dsp_core::math::projection::pca::project_pca;
use dsp_core::signal::dataset::SparsityMask;
use dsp_core::spatial::whitening::{apply_whitening, estimate_covariance};
use dsp_io::config::StorageConfig;
use dsp_io::transmission::processing::ProcessingService;
use dsp_io::zarr::StorageManager;
use dsp_io::processing_graph::ChannelId;
use std::time::Instant;
use console::style;

fn main() -> Result<()> {
    println!("LAB_14  {}", style("End-to-End Spike Sorting Pipeline").bold().cyan());

    // Prerequisite: run lab_13_ephys_emulator first.
    let zarr_path = "data/lab13/ephys_emulator.zarr";
    let config = StorageConfig {
        sample_rate: 40_000,
        channels: 16,
        chunk_size: 32768,
        raw_archive_path: zarr_path.into(),
        ..Default::default()
    };
    let manager = StorageManager::new(config.clone())?;
    let mut service = ProcessingService::new(&manager, None);

    let total_samples = 40_000 * 600u64; // 10 minutes
    let batch_size = 40_000u64;           // 1 second
    let surplus = 1024u64;
    let n_batches = 10;
    let channel_ids: Vec<ChannelId> = (0..16).map(ChannelId::Physical).collect();
    let channel_mask: Vec<u16> = (0..16).collect();

    println!("Pipeline Configuration:");
    println!("  Channels:      16");
    println!("  Batch Size:    1.0s (40,000 samples)");
    println!("  Surplus:       {} samples", surplus);
    println!("  Total Time:    {}s", n_batches);

    // Step 1: Estimate whitening matrix from the first batch.
    println!("\nStep 1: Computing Whitening Matrix...");
    let training_data = service.fetch_package_with_surplus(0, batch_size, surplus, total_samples, &channel_ids)?;

    let start_cov = Instant::now();
    let cov = estimate_covariance(&training_data, &channel_mask, 16);
    let cov_time = start_cov.elapsed();
    println!("  Covariance Estimation: {:?}", cov_time);

    // Build a simple diagonal-inverse whitening matrix from the covariance diagonal.
    // Full eigen-decomposition would be more accurate; this is sufficient for the benchmark.
    let mut whitening_matrix = vec![0.0_f32; 16 * 16];
    for i in 0..16 {
        let var = cov[i * 16 + i];
        whitening_matrix[i * 16 + i] = if var > 1e-9 { 1.0 / var.sqrt() } else { 1.0 };
    }

    // Step 2: Define processing primitives.
    let hp_filter = butterworth(3, FilterResponse::HighPass { cutoff: 300.0 }, 40_000.0);
    let detector = SingleThresholdDetector::new(-1.0, CrossingDirection::Negative, 40, 0, 0);
    let window = (20usize, 28usize);             // 48 samples total
    let snippet_len = 48;

    // Step 3: Training Phase (First 5 seconds to get PCA components)
    println!("\nStep 2: Training PCA on first 5s...");
    let mut training_snippets = Vec::new();
    let n_training_batches = 5;

    for i in 0..n_training_batches {
        let start_sample = i as u64 * batch_size;
        let data = service.fetch_package_with_surplus(start_sample as i64, batch_size, surplus, total_samples, &channel_ids)?;
        let filtered = hp_filter.filter_channels_flat(&data, 16, true);
        let mut processed = filtered;
        apply_whitening(&mut processed, &whitening_matrix, &channel_mask, 16);
        
        let events = detector.detect(&processed, 16, start_sample);
        if events.is_empty() { continue; }

        let fetch_start = (start_sample as i64 - surplus as i64).max(0) as u64;
        let extract_events: Vec<(u64, u16)> = events.iter().map(|e| (e.sample - fetch_start, e.channel)).collect();
        let mut snippets = vec![0.0_f32; events.len() * snippet_len];
        extract_snippets(&processed, 16, &extract_events, window, &SparsityMask::Single, &mut snippets);

        for j in 0..events.len() {
            let snip = &snippets[j * snippet_len .. (j+1) * snippet_len];
            // Simple peak alignment
            let mut min_val = 0.0;
            let mut min_idx = 0;
            for (t, &v) in snip.iter().enumerate() {
                if v < min_val { min_val = v; min_idx = t; }
            }
            
            let mut aligned = vec![0.0_f32; snippet_len];
            let shift = 20.0 - min_idx as f32; // Align peak to index 20
            shift_waveform_cubic(snip, shift, &mut aligned);
            training_snippets.extend(aligned);
        }
        if training_snippets.len() > 5000 * snippet_len { break; }
    }

    let n_train = training_snippets.len() / snippet_len;
    println!("  Collected {} snippets for training.", n_train);

    let mut pca_components = vec![0.0_f32; 3 * snippet_len];
    if n_train > 10 {
        // Simple Power Method / SVD on covariance matrix to find top 3 PCs
        let mut cov = vec![0.0_f64; snippet_len * snippet_len];
        for i in 0..n_train {
            let snip = &training_snippets[i * snippet_len .. (i+1) * snippet_len];
            for r in 0..snippet_len {
                for c in 0..snippet_len {
                    cov[r * snippet_len + c] += snip[r] as f64 * snip[c] as f64;
                }
            }
        }
        for x in &mut cov { *x /= n_train as f64; }

        // Find 3 PCs using power iteration with deflation
        let mut current_cov = cov;
        for p in 0..3 {
            let mut v = vec![0.0_f64; snippet_len];
            v[p] = 1.0; // Initial guess
            for _ in 0..20 {
                let mut next_v = vec![0.0_f64; snippet_len];
                for r in 0..snippet_len {
                    for c in 0..snippet_len {
                        next_v[r] += current_cov[r * snippet_len + c] * v[c];
                    }
                }
                let norm = next_v.iter().map(|&x| x*x).sum::<f64>().sqrt();
                for i in 0..snippet_len { v[i] = next_v[i] / norm; }
            }
            // Store PC
            for i in 0..snippet_len { pca_components[p * snippet_len + i] = v[i] as f32; }
            // Deflate
            let lambda = 0.0_f64; // We don't strictly need lambda for deflation if we orthogonalize later, but let's do it right
            let mut Av = vec![0.0_f64; snippet_len];
            for r in 0..snippet_len {
                for c in 0..snippet_len { Av[r] += current_cov[r * snippet_len + c] * v[c]; }
            }
            let lambda = v.iter().zip(Av.iter()).map(|(a, b)| a*b).sum::<f64>();
            for r in 0..snippet_len {
                for c in 0..snippet_len {
                    current_cov[r * snippet_len + c] -= lambda * v[r] * v[c];
                }
            }
        }
        println!("  PCA components trained.");
    } else {
        println!("  Warning: Not enough spikes for PCA training, using mock components.");
        pca_components[0..snippet_len].fill(1.0);
    }

    // Step 4: Run pipeline.
    println!("\nStep 3: Executing Pipeline...");
    let start_pipeline = Instant::now();
    let mut total_spikes = 0usize;

    let mut all_offsets = Vec::new();
    let mut all_labels = Vec::new();
    let mut all_waveforms = Vec::new();
    let mut all_features = Vec::new();
    let mut events_per_channel: Vec<Vec<dsp_core::signal::Event>> = vec![Vec::new(); 16];

    for i in 0..n_batches {
        let start_sample = i as u64 * batch_size;

        // A. Fetch with surplus window
        let data = service.fetch_package_with_surplus(
            start_sample as i64, batch_size, surplus, total_samples, &channel_ids,
        )?;

        // B. High-pass filter
        let filtered = hp_filter.filter_channels_flat(&data, 16, true);

        // C. Spatial whitening (diagonal approximation)
        let mut processed = filtered;
        apply_whitening(&mut processed, &whitening_matrix, &channel_mask, 16);

        // D. Threshold detection
        let events = detector.detect(&processed, 16, start_sample);
        let n_events = events.len();
        total_spikes += n_events;

        if n_events > 0 {
            let fetch_start = (start_sample as i64 - surplus as i64).max(0) as u64;
            let extract_events: Vec<(u64, u16)> = events
                .iter()
                .map(|e| (e.sample - fetch_start, e.channel))
                .collect();

            // E. Waveform extraction
            let mut snippets = vec![0.0_f32; n_events * snippet_len];
            extract_snippets(
                &processed, 16, &extract_events, window,
                &SparsityMask::Single, &mut snippets,
            );

            // F. Sub-sample alignment + PCA projection.
            let mut aligned  = vec![0.0_f32; n_events * snippet_len];
            let mut features = vec![0.0_f32; n_events * 3];

            for j in 0..n_events {
                let snip = &snippets[j * snippet_len..(j + 1) * snippet_len];
                let aln  = &mut aligned[j * snippet_len..(j + 1) * snippet_len];
                
                // Better peak alignment
                let mut min_val = 0.0;
                let mut min_idx = 0;
                for (t, &v) in snip.iter().enumerate() {
                    if v < min_val { min_val = v; min_idx = t; }
                }
                let shift = 20.0 - min_idx as f32;
                shift_waveform_cubic(snip, shift, aln);
            }

            project_pca(&aligned, snippet_len, &pca_components, None, &mut features);

            // G. Collect for persistence
            for (j, ev) in events.iter().enumerate() {
                all_offsets.push(ev.sample);
                let label = ev.channel as u32 + 1;
                all_labels.push(label); // Mock: label by channel
                all_waveforms.extend_from_slice(&aligned[j * snippet_len .. (j + 1) * snippet_len]);
                all_features.extend_from_slice(&features[j * 3 .. (j + 1) * 3]);
                
                events_per_channel[ev.channel as usize].push(dsp_core::signal::Event::new(ev.sample, label));
            }
        }
    }

    let elapsed = start_pipeline.elapsed();
    
    println!("\nStep 4: Persisting Results...");
    let track_name = "sorted_spikes";
    manager.write_spike_artifacts(
        track_name,
        &all_offsets,
        &all_labels,
        &all_waveforms,
        &all_features,
        total_spikes,
        48,
        3
    )?;

    // Also write standard events track for timeline/raster display
    manager.write_events_track(track_name, &events_per_channel)?;

    // Update RecordingMeta sidecar
    use dsp_io::recording_meta::{RecordingMeta, TrackMeta};
    use dsp_core::signal::LabelVocabulary;
    use std::path::Path;

    let mut meta = RecordingMeta::load(Path::new(zarr_path))?;
    
    // Add the new events track if it doesn't exist
    if !meta.tracks.iter().any(|t| t.name == track_name) {
        meta.tracks.push(TrackMeta::events(
            track_name,
            (0..16).collect(),
            LabelVocabulary::default()
        ));
        meta.save(Path::new(zarr_path))?;
        println!("  Updated RecordingMeta with track: {}", track_name);
    }

    let throughput = (n_batches as f64 * batch_size as f64 * 16.0) / elapsed.as_secs_f64();

    println!("\nFinal Metrics:");
    println!("  Total Spikes:  {}", style(total_spikes).yellow().bold());
    println!("  Elapsed Time:  {:?}", style(elapsed).green());
    println!("  Throughput:    {} samples/sec", style(format!("{:.2e}", throughput)).magenta().bold());
    println!("  Real-time X:   {}x", style(format!("{:.1}", n_batches as f64 / elapsed.as_secs_f64())).cyan().bold());

    Ok(())
}
