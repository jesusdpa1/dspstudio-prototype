use std::fs::OpenOptions;
use std::path::Path;
use memmap2::MmapMut;
use anyhow::{Context, Result};

/// Provides a "slice-like" interface to a shadow memory-mapped file.
pub struct MmapEngine {
    mmap: MmapMut,
    total_samples: u64,
}

impl MmapEngine {
    pub fn new(path: &Path, channels: u16, total_samples: u64) -> Result<Self> {
        let size = (channels as u64 * total_samples * 4) as usize; 
        
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)?;

        file.set_len(size as u64)?;
        let mmap = unsafe { MmapMut::map_mut(&file)? };
        
        Ok(Self { mmap, total_samples })
    }

    /// Reads a window for specific channels from the mmap buffer.
    /// This mimics the StorageManager interface for the Transmission services.
    pub fn read_window_masked(&self, start: u64, count: u64, channels: &[u16]) -> Result<Vec<f32>> {
        let mut result = Vec::with_capacity((channels.len() as u64 * count) as usize);
        
        for &ch in channels {
            let ch_slice = self.get_channel_slice(ch, self.total_samples)?;
            let end = (start + count).min(self.total_samples);
            if start < end {
                result.extend_from_slice(&ch_slice[start as usize..end as usize]);
            } else {
                result.extend(vec![0.0; count as usize]);
            }
        }
        Ok(result)
    }

    pub fn get_channel_slice(&self, channel: u16, total_samples: u64) -> Result<&[f32]> {
        let offset = (channel as u64).checked_mul(total_samples)
            .context("Channel offset overflow")?;

        let start_bytes = offset.checked_mul(4).context("Byte offset overflow")?;
        let len_bytes = total_samples.checked_mul(4).context("Byte length overflow")?;

        if start_bytes + len_bytes > self.mmap.len() as u64 {
            anyhow::bail!(
                "Mmap read out of bounds: channel {} at {} samples exceeds mmap length {}",
                channel, total_samples, self.mmap.len()
            );
        }

        let offset_usize = usize::try_from(offset)
            .context("Channel offset exceeds usize on this platform")?;
        let len_usize = usize::try_from(total_samples)
            .context("Sample count exceeds usize on this platform")?;

        // SAFETY: `offset_usize` and `len_usize` are bounds-checked against
        // `self.mmap.len()` above. The returned slice lifetime is tied to `&self`,
        // so the mapping outlives the slice. No mutable alias is possible while
        // `&self` is held (Rust prevents simultaneous `&mut self` via borrow rules).
        let raw_ptr = self.mmap.as_ptr() as *const f32;
        Ok(unsafe { std::slice::from_raw_parts(raw_ptr.add(offset_usize), len_usize) })
    }

    pub fn get_channel_slice_mut(&mut self, channel: u16, total_samples: u64) -> Result<&mut [f32]> {
        let offset = (channel as u64).checked_mul(total_samples)
            .context("Channel offset overflow")?;

        let start_bytes = offset.checked_mul(4).context("Byte offset overflow")?;
        let len_bytes = total_samples.checked_mul(4).context("Byte length overflow")?;

        if start_bytes + len_bytes > self.mmap.len() as u64 {
            anyhow::bail!(
                "Mmap write out of bounds: channel {} at {} samples exceeds mmap length {}",
                channel, total_samples, self.mmap.len()
            );
        }

        let offset_usize = usize::try_from(offset)
            .context("Channel offset exceeds usize on this platform")?;
        let len_usize = usize::try_from(total_samples)
            .context("Sample count exceeds usize on this platform")?;

        // SAFETY: `offset_usize` and `len_usize` are bounds-checked above. The
        // returned slice lifetime is tied to `&mut self`, preventing any other
        // access to the mapping while the mutable slice is live.
        let raw_ptr = self.mmap.as_mut_ptr() as *mut f32;
        Ok(unsafe { std::slice::from_raw_parts_mut(raw_ptr.add(offset_usize), len_usize) })
    }

    pub fn flush(&self) -> Result<()> {
        self.mmap.flush()?;
        Ok(())
    }
}
