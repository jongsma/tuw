// SPDX-License-Identifier: GPL-3.0-or-later

//! One preview voice: a prepared table played at a playhead and a note.
//!
//! [`Voice::render`] is real-time safe: it allocates nothing, frees nothing, takes no lock and
//! cannot panic. Everything it needs (the kernel) is made in [`Voice::new`]; the levels belong
//! to the [`PreparedTable`].

use std::sync::Arc;

use super::kernel::Kernel;
use super::{LevelWeights, PreparedTable, PreviewError, level_weights};

/// Weights meaning "nothing sounds".
const SILENT: LevelWeights = LevelWeights {
    first: 0,
    w_first: 0.0,
    w_second: 0.0,
};

/// A single-threaded preview voice.
///
/// All frames share one phase accumulator, so morphing never jumps in phase. Each output
/// sample is the bilinear mix of the four frames around the playhead, each read from the same
/// one or two band-limited levels (chosen by [`level_weights`] from the note alone).
pub struct Voice {
    sample_rate: f32,
    kernel: Kernel,
    current: Option<Arc<PreparedTable>>,
    /// Position in the period, `0.0..1.0`.
    phase: f64,
    /// Playhead as last set (finite, not yet clamped: each table clamps it to its own grid).
    y: f32,
    z: f32,
    /// Note in Hz; 0 when the last value set was not a usable frequency (silence).
    frequency: f32,
    /// Output gain: 1 while playing, 0 while stopped.
    gain: f32,
    last_weights: LevelWeights,
}

impl std::fmt::Debug for Voice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Voice")
            .field("sample_rate", &self.sample_rate)
            .field("has_table", &self.current.is_some())
            .field("y", &self.y)
            .field("z", &self.z)
            .field("frequency", &self.frequency)
            .field("gain", &self.gain)
            .finish_non_exhaustive()
    }
}

impl Voice {
    /// A stopped voice with no table, at the playhead (0, 0) and no note.
    ///
    /// Allocates the interpolation kernel; call it off the audio thread.
    ///
    /// # Errors
    ///
    /// [`PreviewError::InvalidSampleRate`] when `sample_rate` is not finite or not positive.
    pub fn new(sample_rate: f32) -> Result<Voice, PreviewError> {
        if !(sample_rate.is_finite() && sample_rate > 0.0) {
            return Err(PreviewError::InvalidSampleRate);
        }
        Ok(Voice {
            sample_rate,
            kernel: Kernel::new(),
            current: None,
            phase: 0.0,
            y: 0.0,
            z: 0.0,
            frequency: 0.0,
            gain: 0.0,
            last_weights: SILENT,
        })
    }

    /// Plays `table` from now on and gives back the table it replaces, if any.
    ///
    /// The voice never drops a table: the caller disposes of the one it gets back (off the
    /// audio thread).
    pub fn set_table(&mut self, table: Arc<PreparedTable>) -> Option<Arc<PreparedTable>> {
        self.current.replace(table)
    }

    /// Moves the playhead to (`y`, `z`), in frames. Each coordinate is clamped into the grid
    /// of the table being played; a coordinate that is not finite is ignored (the previous
    /// value stays).
    pub fn set_playhead(&mut self, y: f32, z: f32) {
        if y.is_finite() {
            self.y = y;
        }
        if z.is_finite() {
            self.z = z;
        }
    }

    /// Sets the note to `hz`. A value that is not finite or not positive means silence: no
    /// level sounds and the phase holds.
    pub fn set_frequency(&mut self, hz: f32) {
        self.frequency = if hz.is_finite() && hz > 0.0 { hz } else { 0.0 };
    }

    /// Starts sounding.
    pub fn play(&mut self) {
        self.gain = 1.0;
    }

    /// Stops sounding; the output is silence until the next [`play`](Voice::play).
    pub fn stop(&mut self) {
        self.gain = 0.0;
    }

    /// The level weights of the current table at the last rendered sample (silent weights
    /// before the first sample or without a table). For tests: they show that all four frames
    /// use one level choice.
    pub fn last_weights(&self) -> LevelWeights {
        self.last_weights
    }

    /// Fills `out` with the next samples. Real-time safe.
    pub fn render(&mut self, out: &mut [f32]) {
        let step = f64::from(self.frequency) / f64::from(self.sample_rate);
        let max = f64::from(f32::MAX);
        for sample in out.iter_mut() {
            let mut sum = 0.0f64;
            if let Some(table) = &self.current {
                let (value, weights) = render_table(
                    &self.kernel,
                    table,
                    self.phase,
                    self.y,
                    self.z,
                    self.frequency,
                    self.sample_rate,
                );
                sum += value;
                self.last_weights = weights;
            } else {
                self.last_weights = SILENT;
            }
            *sample = (sum * f64::from(self.gain)).clamp(-max, max) as f32;
            let next = self.phase + step;
            self.phase = next - next.floor();
        }
    }
}

/// One sample of `table` at `phase`, playhead (`y`, `z`) and note `frequency`, and the level
/// weights it used. Real-time safe.
///
/// The weights are computed once, before the four corners, so all four frames read the same
/// levels. A corner or level with weight 0 is skipped (it would add nothing).
fn render_table(
    kernel: &Kernel,
    table: &PreparedTable,
    phase: f64,
    y: f32,
    z: f32,
    frequency: f32,
    sample_rate: f32,
) -> (f64, LevelWeights) {
    let weights = level_weights(table.levels(), frequency, sample_rate);
    if weights.w_first <= 0.0 && weights.w_second <= 0.0 {
        return (0.0, weights);
    }
    let (y0, y1, a) = axis(y, table.y_size());
    let (z0, z1, b) = axis(z, table.z_size());
    let corners = [
        (y0, z0, (1.0 - a) * (1.0 - b)),
        (y1, z0, a * (1.0 - b)),
        (y0, z1, (1.0 - a) * b),
        (y1, z1, a * b),
    ];
    let levels = [
        (weights.first, f64::from(weights.w_first)),
        (weights.first + 1, f64::from(weights.w_second)),
    ];
    let mut sum = 0.0f64;
    for (cy, cz, corner) in corners {
        if corner <= 0.0 {
            continue;
        }
        let Some(frame) = table.frame(cy, cz) else {
            continue;
        };
        for (index, weight) in levels {
            if weight <= 0.0 {
                continue;
            }
            let level = frame.level(index);
            let x = phase * level.len() as f64;
            sum += corner * weight * kernel.read(level, x);
        }
    }
    (sum, weights)
}

/// The two frame indices around `position` on an axis of `size` frames, and how far past the
/// first it is: `position` clamped to `0..=size-1`, `i0 = floor`, `i1 = min(i0 + 1, size - 1)`.
fn axis(position: f32, size: u32) -> (u32, u32, f64) {
    let last = size.saturating_sub(1);
    let p = f64::from(position).clamp(0.0, f64::from(last));
    let i0 = (p.floor() as u32).min(last);
    let i1 = i0.saturating_add(1).min(last);
    (i0, i1, p - f64::from(i0))
}
