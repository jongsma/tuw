// SPDX-License-Identifier: GPL-3.0-or-later

//! A band-limited preview oscillator for wavetables.
//!
//! Playing a wavetable at a pitch means reading each frame at a speed the frame was never
//! recorded at. A frame with many harmonics read fast puts some of them above Nyquist, where
//! they fold back as aliasing. The preview avoids that by never reading harmonics that would
//! pass Nyquist.
//!
//! # Two halves
//!
//! The UI thread holds a [`Preparer`] and a [`PreviewControl`]; the audio thread holds an
//! [`Oscillator`]. [`preview`] makes the control and oscillator pair.
//!
//! - **[`Preparer`] (UI thread).** Turns a [`Wavetable`](crate::Wavetable) into a
//!   [`PreparedTable`]. It allocates and runs FFTs, so it must never run on the audio thread.
//!   It remembers the frames it built, so preparing an edited table again only builds the frames
//!   that changed ([`Preparer::frames_built_last`]).
//! - **[`Oscillator`] (audio thread).** Renders blocks with [`Oscillator::render`], which
//!   allocates nothing, frees nothing, takes no lock and cannot panic.
//! - **[`PreviewControl`] (UI thread).** The other end of the [`Oscillator`]: it sends tables,
//!   playhead, frequency, play and stop, and takes tables back.
//!
//! Which table to prepare: only [`Document::finished`](crate::Document::finished), and only
//! when [`Document::change_id`](crate::Document::change_id) changes. An open step's
//! [`current`](crate::Document::current) changes on every mouse move and would flood the
//! preparer.
//!
//! # Levels
//!
//! Each frame is stored as several *levels*, the same waveform band-limited to fewer and fewer
//! harmonics ([`level_harmonics`]). The richest level keeps every harmonic below Nyquist of the
//! frame itself; each next level keeps at most 75% of the harmonics of the one before it, which
//! is roughly two levels per octave. A level is made by an FFT of the frame that keeps the
//! harmonics `1..=H` and zeroes everything else, so it is *strictly* band-limited: nothing
//! above `H`. DC and the Nyquist bin are dropped (a component exactly at Nyquist is not
//! below it). The level is stored with [`level_length`]`(H)` samples, at
//! least four per top harmonic and never fewer than 16, so it is at least 2× oversampled.
//!
//! While playing, [`level_weights`] picks the richest level whose top harmonic is below
//! Nyquist at the current frequency and crossfades it with the next one: a level is fully on
//! while its top harmonic is at most [`FADE_START`] times Nyquist and fades out linearly to
//! zero at Nyquist. The weights are continuous in frequency, so a pitch sweep makes no clicks.
//!
//! # Interpolation can't be perfect
//!
//! A level is read at fractional positions, and any interpolation filter is a compromise. A
//! short filter has a wide transition band, so it lets some of the spectrum's mirror images
//! through; a longer one costs more per sample. The preview uses a [`KERNEL_TAPS`]-tap sinc
//! windowed by a Kaiser window ([`KERNEL_BETA`]), tabulated at `KERNEL_PHASES + 1` fractional
//! offsets ([`KERNEL_PHASES`] steps of one sample, 0 to 1) and interpolated linearly between
//! neighbouring rows. Because every level is at least 2×
//! oversampled, its content sits in the lower half of the band and the kernel's transition band
//! falls where the level has nothing, so what remains is small but not zero. The tests render
//! a saw at 44.1, 48 and 96 kHz over five notes plus a note in a crossfade and check that
//! every spectral line that is not a harmonic of the note stays at or below -80 dB relative to
//! the fundamental. (The worst measured value, about -93 dB, sits near the
//! analysis window's own floor, so it says more about the measurement than about the
//! oscillator; the promise is the tested -80 dB.) A control test shows the analysis does catch
//! aliasing: a naive saw read with plain linear interpolation is far above the bound.
//!
//! # Hand-off and the never-free-on-audio rule
//!
//! Tables travel as `Arc<PreparedTable>` over lock-free rings: one for tables, one for
//! controls (so a table waiting for its swap never holds up `stop`), and one that returns
//! tables to the UI. Freeing memory on the audio thread can take a lock inside the allocator,
//! so the [`Oscillator`] never drops a table: when the voice has faded one out, it goes back
//! through the return ring, or waits in a preallocated holding slot while that ring is full.
//! The UI thread drops them in [`PreviewControl::collect`]. Call `collect` regularly (once
//! per frame drawn is plenty). Tables come back to the UI even when the UI has already dropped
//! all its own references and the [`Preparer`], and when frames are shared between tables; the
//! audio thread still frees nothing.
//!
//! **Dropping the [`Oscillator`] frees whatever it still holds**, so drop it off the audio
//! thread: move it back to the UI thread, or hand it to a thread that frees it.
//!
//! # Limits
//!
//! - **Size.** A table of more than [`MAX_PREVIEW_SAMPLES`] samples (`frame_size × y_size ×
//!   z_size`) is *unsupported for preview*: [`Preparer::prepare`] returns
//!   [`PreviewError::Unsupported`]. The table is valid; the preview just doesn't play it.
//! - **Memory.** All levels of one frame together hold at most [`MAX_MEMORY_FACTOR`] (8) times
//!   the frame size in samples, so a prepared table holds at most 8× the table's samples
//!   ([`PreparedTable::sample_count`]). The levels sum to about 6.8× the frame size for frame sizes from 256 up, and up to
//!   about 7.2× for small frames; never more than 8×, tested for every frame size
//!   8..=65 536.
//! - **Unusable notes.** A frequency that is not finite or not positive fades the note out
//!   over [`PLAY_FADE_SECONDS`] like `stop`, and the output stays finite.
//! - **Ring capacity.** [`preview`] clamps its capacity to `1..=65_536`.
//!
//! # Example
//!
//! ```
//! use tuw_core::Wavetable;
//! use tuw_core::preview::{Preparer, preview};
//!
//! // A silent table: one frame of 2048 samples.
//! let table = Wavetable::silent(2048, 1, 1)?;
//! let prepared = Preparer::new().prepare(&table)?;
//!
//! // UI half and audio half; here both run on this thread.
//! let (mut control, mut oscillator) = preview(48_000.0, 8)?;
//! assert!(control.set_table(prepared).is_ok());
//! control.set_frequency(440.0)?;
//! control.play()?;
//!
//! let mut block = [1.0_f32; 256];
//! oscillator.render(&mut block);
//! // Compare bit patterns: floats are never compared with `==`. Silence is all zero bits.
//! assert!(block.iter().all(|x| x.to_bits() == 0));
//!
//! // Tables the audio side has let go of are dropped here, on the UI thread.
//! control.collect();
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod handoff;
mod kernel;
mod levels;
mod prepare;
mod voice;

use std::fmt;

pub use handoff::{Oscillator, PreviewControl, preview};
pub use levels::{LevelWeights, level_harmonics, level_length, level_weights};
pub use prepare::{PreparedFrame, PreparedTable, Preparer};
pub use voice::Voice;

/// Most samples (`frame_size × y_size × z_size`) a table may have to be previewed.
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
