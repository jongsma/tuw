// SPDX-License-Identifier: GPL-3.0-or-later

//! A band-limited preview oscillator for wavetables.
//!
//! Each frame is prepared once as a set of *levels* (the same waveform, band-limited to fewer
//! and fewer harmonics), and the oscillator reads the richest level that stays below Nyquist.
//! This module holds the constants and the error type shared by all parts.

mod levels;

use std::fmt;

pub use levels::{LevelWeights, level_harmonics, level_length, level_weights};

/// Most samples (all levels of all grid positions) a table may need to be previewed.
pub const MAX_PREVIEW_SAMPLES: u64 = 1 << 24;

/// All levels of one frame together hold at most this many times the frame size in samples.
pub const MAX_MEMORY_FACTOR: usize = 8;

/// A level is fully on while its top harmonic is at most `FADE_START` times Nyquist; above
/// that it fades out linearly, reaching zero at Nyquist.
pub const FADE_START: f32 = 0.8;

/// Number of taps of the interpolation kernel.
pub const KERNEL_TAPS: usize = 16;

/// Number of fractional offsets the interpolation kernel is tabulated at.
pub const KERNEL_PHASES: usize = 512;

/// Kaiser window shape parameter of the interpolation kernel.
pub const KERNEL_BETA: f32 = 10.0;

/// Length of the ramp that smooths playhead and frequency changes, in seconds.
pub const SMOOTHING_SECONDS: f32 = 0.02;

/// Length of the crossfade when the table is swapped while sounding, in seconds.
pub const SWAP_FADE_SECONDS: f32 = 0.01;

/// Length of the gain ramp for play and stop, in seconds.
pub const PLAY_FADE_SECONDS: f32 = 0.005;

/// Why the preview could not do what was asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewError {
    /// The table is too large to preview (not an invalid table; it is just unsupported here).
    Unsupported {
        /// Samples the preview would need.
        samples: u64,
        /// The most it supports.
        limit: u64,
    },
    /// The sample rate is not finite or not positive.
    InvalidSampleRate,
    /// The command ring is full; try again later.
    Busy,
}

impl fmt::Display for PreviewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PreviewError::Unsupported { samples, limit } => write!(
                f,
                "unsupported for preview: needs {samples} samples, the limit is {limit}"
            ),
            PreviewError::InvalidSampleRate => {
                write!(f, "the sample rate must be finite and greater than zero")
            }
            PreviewError::Busy => write!(f, "the preview is busy, try again later"),
        }
    }
}

impl std::error::Error for PreviewError {}
