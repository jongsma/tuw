// SPDX-License-Identifier: GPL-3.0-or-later

//! Turning a [`Wavetable`] into band-limited levels the oscillator can play.
//!
//! Everything here runs on the UI thread: it allocates, and it may take a while for a big
//! table. The audio thread only ever reads the finished [`PreparedTable`].

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use rustfft::FftPlanner;
use rustfft::num_complex::Complex;

use super::{MAX_PREVIEW_SAMPLES, PreviewError, level_harmonics, level_length};
use crate::table::{Frame, Wavetable};

/// One frame, prepared: the same waveform band-limited to fewer and fewer harmonics.
///
/// Level `l` keeps harmonics `1..=H_l` of the frame (`H_l` from
/// [`PreparedTable::levels`]) and nothing else: no DC, no Nyquist bin. It holds
/// [`level_length`]`(H_l)` samples of one period.
pub struct PreparedFrame {
    levels: Vec<Vec<f32>>,
}

impl PreparedFrame {
    /// The samples of level `index`; empty when there is no such level.
    pub fn level(&self, index: usize) -> &[f32] {
        self.levels.get(index).map_or(&[], Vec::as_slice)
    }

    /// Number of levels.
    pub fn level_count(&self) -> usize {
        self.levels.len()
    }
}

impl fmt::Debug for PreparedFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PreparedFrame({} levels)", self.levels.len())
    }
}

/// A whole table, prepared: one [`PreparedFrame`] per grid position.
///
/// Positions whose source frames are the same buffer share one prepared frame.
pub struct PreparedTable {
    frame_size: u32,
    y_size: u32,
    z_size: u32,
    levels: Vec<u32>,
    /// In TUW3 order: index `z × y_size + y`.
    frames: Vec<Arc<PreparedFrame>>,
    sample_count: usize,
}

impl PreparedTable {
    /// Number of frames along Y.
    pub fn y_size(&self) -> u32 {
        self.y_size
    }

    /// Number of frames along Z.
    pub fn z_size(&self) -> u32 {
        self.z_size
    }

    /// Samples per frame of the source table.
    pub fn frame_size(&self) -> u32 {
        self.frame_size
    }

    /// Harmonics kept by each level, richest first (the same for every frame).
    pub fn levels(&self) -> &[u32] {
        &self.levels
    }

    /// The prepared frame at (`y`, `z`), or `None` outside the grid.
    pub fn frame(&self, y: u32, z: u32) -> Option<&Arc<PreparedFrame>> {
        if y >= self.y_size || z >= self.z_size {
            return None;
        }
        self.frames
            .get(z as usize * self.y_size as usize + y as usize)
    }

    /// Samples in all levels of all grid positions. A frame shared by several positions is
    /// counted at each, so this bounds the memory the table could take.
    pub fn sample_count(&self) -> usize {
        self.sample_count
    }
}

impl fmt::Debug for PreparedTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedTable")
            .field("frame_size", &self.frame_size)
            .field("y_size", &self.y_size)
            .field("z_size", &self.z_size)
            .field("levels", &self.levels)
            .finish_non_exhaustive()
    }
}

/// Prepares tables for the preview, on the UI thread.
///
/// Keeps its FFT plans between calls. Frames that appear at several positions of one table
/// (a silent table shares one buffer) are built once, and a frame that is the same buffer as in
/// the previous successful `prepare` is reused, so editing one frame rebuilds only that one.
pub struct Preparer {
    /// f64, so that frames with samples up to ±`f32::MAX` cannot overflow.
    planner: FftPlanner<f64>,
    /// Keyed by [`Frame::addr`]. The entry holds a clone of the frame, so its address can't be
    /// reused by another buffer while it is cached.
    cache: HashMap<usize, (Frame, Arc<PreparedFrame>)>,
    frames_built_last: usize,
}

impl Default for Preparer {
    fn default() -> Preparer {
        Preparer::new()
    }
}

impl fmt::Debug for Preparer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Preparer")
            .field("cached_frames", &self.cache.len())
            .field("frames_built_last", &self.frames_built_last)
            .finish_non_exhaustive()
    }
}

impl Preparer {
    /// A preparer with no plans and nothing cached.
    pub fn new() -> Preparer {
        Preparer {
            planner: FftPlanner::new(),
            cache: HashMap::new(),
            frames_built_last: 0,
        }
    }

    /// Prepares every frame of `table`. The table itself is only read.
    ///
    /// # Errors
    ///
    /// [`PreviewError::Unsupported`] when the table has more than [`MAX_PREVIEW_SAMPLES`]
    /// samples. The table is valid; it is just too big to preview. Nothing is built and the
    /// preparer is left as it was.
    pub fn prepare(&mut self, table: &Wavetable) -> Result<Arc<PreparedTable>, PreviewError> {
        let (frame_size, y_size, z_size) = (table.frame_size(), table.y_size(), table.z_size());
        let samples = u64::from(frame_size) * u64::from(y_size) * u64::from(z_size);
        if samples > MAX_PREVIEW_SAMPLES {
            return Err(PreviewError::Unsupported {
                samples,
                limit: MAX_PREVIEW_SAMPLES,
            });
        }

        let levels = level_harmonics(frame_size);
        let per_frame: usize = levels.iter().map(|&h| level_length(h)).sum();
        let positions = y_size as usize * z_size as usize;

        // Nothing below can fail, so taking the old cache out is safe: the size check above
        // is the only early return.
        let previous = std::mem::take(&mut self.cache);
        let mut cache = HashMap::new();
        let mut built = 0;
        let mut frames = Vec::with_capacity(positions);
        for (_, _, frame) in table.frames() {
            // This table's frames first, then the previous table's.
            let prepared = if let Some((_, prepared)) = cache.get(&frame.addr()) {
                Arc::clone(prepared)
            } else {
                let prepared = match previous.get(&frame.addr()) {
                    Some((_, prepared)) => Arc::clone(prepared),
                    None => {
                        built += 1;
                        Arc::new(self.build(frame.samples(), &levels))
                    }
                };
                cache.insert(frame.addr(), (frame.clone(), Arc::clone(&prepared)));
                prepared
            };
            frames.push(prepared);
        }

        self.cache = cache;
        self.frames_built_last = built;
        Ok(Arc::new(PreparedTable {
            frame_size,
            y_size,
            z_size,
            levels,
            frames,
            sample_count: per_frame * positions,
        }))
    }

    /// Frames built (not reused) by the last successful [`prepare`](Preparer::prepare).
    pub fn frames_built_last(&self) -> usize {
        self.frames_built_last
    }

    /// Builds the levels of one frame by FFT resynthesis.
    ///
    /// One forward FFT of the frame (length `N`); then per level, bins `1..=H` and their
    /// conjugates go into an `M`-point spectrum with everything else zero, and an inverse FFT
    /// of length `M` gives one period resampled to `M` samples. rustfft does not normalise,
    /// so the result is scaled by `1 / N`.
    ///
    /// The FFTs run in `f64`: a frame may hold any finite `f32`, up to ±`f32::MAX`, and its
    /// spectrum (up to `N × f32::MAX`) would overflow `f32`. Each stored sample is clamped to
    /// ±`f32::MAX` (band-limiting can raise the peak), so finite input always gives finite
    /// levels.
    fn build(&mut self, samples: &[f32], levels: &[u32]) -> PreparedFrame {
        let n = samples.len();
        let mut spectrum: Vec<Complex<f64>> = samples
            .iter()
            .map(|&s| Complex::new(f64::from(s), 0.0))
            .collect();
        self.planner.plan_fft_forward(n).process(&mut spectrum);
        let scale = 1.0 / n as f64;
        let max = f64::from(f32::MAX);

        let levels = levels
            .iter()
            .map(|&h| {
                let m = level_length(h);
                let mut buf = vec![Complex::new(0.0f64, 0.0); m];
                // h <= (n - 1) / 2 and m >= 4 h, so bins k and m - k never collide and
                // spectrum[k] exists.
                for (k, &bin) in spectrum.iter().enumerate().take(h as usize + 1).skip(1) {
                    buf[k] = bin;
                    buf[m - k] = bin.conj();
                }
                self.planner.plan_fft_inverse(m).process(&mut buf);
                buf.iter()
                    .map(|c| (c.re * scale).clamp(-max, max) as f32)
                    .collect()
            })
            .collect();
        PreparedFrame { levels }
    }
}
