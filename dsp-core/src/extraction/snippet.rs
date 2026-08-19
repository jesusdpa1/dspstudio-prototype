//! Waveform snippet extraction from continuous streams.

use crate::signal::dataset::SparsityMask;
use crate::signal::Event;

/// Extracts waveform snippets from a continuous stream into a flat buffer.
/// 
/// # Arguments
/// * `stream`: Flat channel-major source buffer [TotalChannels x StreamSamples].
/// * `total_channels`: Total number of channels in the source stream.
/// * `events`: Timestamps (and primary channel) for each spike.
/// * `window`: (samples_before, samples_after) relative to the event peak.
/// * `mask`: Optional sparsity mask to select which channels to extract for each event.
/// * `output`: Target buffer to fill [Sum(ChannelsPerEvent) x (Before + After)].
/// 
/// # Return
/// Returns the number of snippets successfully extracted.
pub fn extract_snippets(
    stream: &[f32],
    total_channels: usize,
    events: &[(u64, u16)], // (sample_offset, primary_channel)
    window: (usize, usize),
    mask: &SparsityMask,
    output: &mut [f32],
) -> usize {
    if stream.is_empty() || events.is_empty() || total_channels == 0 {
        return 0;
    }

    let stream_samples = stream.len() / total_channels;
    let (before, after) = window;
    let snippet_len = before + after;
    let mut out_offset = 0;
    let mut count = 0;

    for &(sample_offset, primary_ch) in events {
        let channels_to_extract = mask.get_channels_for(primary_ch, total_channels as u16);
        let n_ch = channels_to_extract.len();
        
        // Check if we have enough space in the output buffer
        if out_offset + (n_ch * snippet_len) > output.len() {
            break;
        }

        // Safety check: Can we actually extract this spike? 
        // We need 'before' samples before and 'after' samples after.
        if sample_offset < before as u64 || sample_offset + after as u64 > stream_samples as u64 {
            // Fill with zeros or skip? Let's skip and keep out_offset static for now
            // Or fill zeros to keep 1:1 mapping with events. 
            // SI usually pads with zeros.
            for i in 0..(n_ch * snippet_len) {
                output[out_offset + i] = 0.0;
            }
            out_offset += n_ch * snippet_len;
            count += 1;
            continue;
        }

        // Extract each channel in the mask
        for (i, &ch) in channels_to_extract.iter().enumerate() {
            let start_idx = ch as usize * stream_samples + (sample_offset as usize - before);
            let end_idx = start_idx + snippet_len;
            
            let dest_start = out_offset + (i * snippet_len);
            let dest_end = dest_start + snippet_len;
            
            output[dest_start..dest_end].copy_from_slice(&stream[start_idx..end_idx]);
        }

        out_offset += n_ch * snippet_len;
        count += 1;
    }

    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_single_channel_global() {
        // 1 channel, 10 samples: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
        let stream: Vec<f32> = (0..10).map(|i| i as f32).collect();
        // Spike at index 5, window (2, 2) -> expected [3, 4, 5, 6]
        let events = vec![(5u64, 0u16)];
        let mut output = vec![0.0f32; 4];
        let n = extract_snippets(&stream, 1, &events, (2, 2), &SparsityMask::Global, &mut output);
        
        assert_eq!(n, 1);
        assert_eq!(output, vec![3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn test_extract_multichannel_masked() {
        // 2 channels, 10 samples each
        // Ch0: [0..9], Ch1: [10..19]
        let mut stream: Vec<f32> = (0..10).map(|i| i as f32).collect();
        stream.extend((10..20).map(|i| i as f32));
        
        let events = vec![(5u64, 0u16)];
        let mut output = vec![0.0f32; 4]; // Only extract Single channel 0
        
        let n = extract_snippets(&stream, 2, &events, (2, 2), &SparsityMask::Single, &mut output);
        assert_eq!(n, 1);
        assert_eq!(output, vec![3.0, 4.0, 5.0, 6.0]);
    }
}
