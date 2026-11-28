// SPDX-License-Identifier: GPL-3.0-or-later

//! One preview voice: a prepared table played at a playhead and a note.
//!
//! [`Voice::render`] is real-time safe: it allocates nothing, frees nothing, takes no lock and
//! cannot panic. Everything it needs (the kernel) is made in [`Voice::new`]; the levels belong
//! to the [`PreparedTable`].

use std::sync::Arc;

use super::kernel::Kernel;
use super::{
    LevelWeights, PLAY_FADE_SECONDS, PreparedTable, PreviewError, SMOOTHING_SECONDS,
    SWAP_FADE_SECONDS, level_weights,
};

/// Weights meaning "nothing sounds".
const SILENT: LevelWeights = LevelWeights {
    first: 0,
    w_first: 0.0,
    w_second: 0.0,
};

/// A linear ramp of one value, advanced once per output sample.
///
/// Each value is computed from the start of the ramp, not accumulated, so rounding never
/// drifts; the last step assigns the target, so the ramp ends on it exactly.
#[derive(Debug, Clone, Copy)]
struct Ramp {
    value: f32,
    start: f32,
    target: f32,
    /// Samples the ramp takes, and how many have passed; the ramp runs while `done < len`.
    len: u32,
    done: u32,
}

impl Ramp {
    fn at(value: f32) -> Ramp {
        Ramp {
            value,
            start: value,
            target: value,
            len: 0,
            done: 0,
        }
    }

    /// Sets `value` to `target` at once, ending any ramp.
    fn jump(&mut self, target: f32) {
        *self = Ramp::at(target);
    }

    /// Starts a ramp from the current value to `target` over `len` samples (a jump if `len`
    /// is 0). Asking for the target it already has (same bits) does not restart a running
    /// ramp: it keeps its arrival sample.
    fn to(&mut self, target: f32, len: u32) {
        if len == 0 {
            self.jump(target);
        } else if target.to_bits() != self.target.to_bits() {
            *self = Ramp {
                value: self.value,
                start: self.value,
                target,
                len,
                done: 0,
            };
        }
    }

    fn is_running(&self) -> bool {
        self.done < self.len
    }

    /// Clamps the ramp into `0..=last`: value, start and target. A running ramp goes on from
    /// the clamped value to the clamped target over the samples it has left, so it stays
    /// linear and arrives when it would have.
    fn clamp_to(&mut self, last: f32) {
        let value = self.value.clamp(0.0, last);
        let target = self.target.clamp(0.0, last);
        *self = Ramp {
            value,
            start: value,
            target,
            len: self.len - self.done,
            done: 0,
        };
    }

    /// One sample further.
    fn advance(&mut self) {
        if !self.is_running() {
            return;
        }
        self.done += 1;
        self.value = if self.done >= self.len {
            self.target
        } else {
            let t = f64::from(self.done) / f64::from(self.len);
            (f64::from(self.start) + (f64::from(self.target) - f64::from(self.start)) * t) as f32
        };
    }
}

/// `seconds × sample_rate`, rounded, as a sample count; 0 (no ramp) when `seconds` is not
/// finite or not positive.
fn ramp_samples(seconds: f32, sample_rate: f32) -> u32 {
    if !(seconds.is_finite() && seconds > 0.0) {
        return 0;
    }
    let n = (f64::from(seconds) * f64::from(sample_rate)).round();
    // `as` saturates, so a huge time gives the longest ramp a u32 can count.
    n as u32
}

/// A single-threaded preview voice.
///
/// All frames share one phase accumulator, so morphing never jumps in phase. Each output
/// sample is the bilinear mix of the four frames around the playhead, each read from the same
/// one or two band-limited levels (chosen by [`level_weights`] from the note alone).
///
/// # Smoothing and fades
///
/// While the voice sounds (its play gain is above 0, or ramping up from 0 after
/// [`play`](Voice::play)), [`set_playhead`](Voice::set_playhead) and
/// [`set_frequency`](Voice::set_frequency) ramp linearly to the new value over
/// [`SMOOTHING_SECONDS`]; a new value mid-ramp starts a new ramp from where it is. While the
/// voice is silent, they jump. [`play`](Voice::play) and [`stop`](Voice::stop) ramp the gain
/// over [`PLAY_FADE_SECONDS`]; after the stop fade the output is exact zeros. An unusable note
/// (see [`set_frequency`](Voice::set_frequency)) fades the note out the same way, and a usable
/// one fades it back in.
///
/// # Table swaps
///
/// [`set_table`](Voice::set_table) into a voice without a table installs it. With a table
/// and silent, it replaces the table at once and *retires* the old one. With a table and
/// sounding, the new table fades in and the old one fades out linearly over
/// [`SWAP_FADE_SECONDS`]; both play at the same phase, playhead and note, and the old one is
/// retired when the fade ends. Taking a table clamps the playhead into its grid at once; the
/// fading table keeps the playhead it had, so it plays on without a jump (each table still
/// clamps the playhead into its own grid). A retired table waits for
/// [`take_retired`](Voice::take_retired). The voice never clones or drops a table: it only
/// moves the ones it is given.
pub struct Voice {
    sample_rate: f32,
    kernel: Kernel,
    /// The table playing (fading in during a swap).
    current: Option<Arc<PreparedTable>>,
    /// The table fading out during a swap.
    fading: Option<Arc<PreparedTable>>,
    /// A table the voice no longer needs, waiting for [`Voice::take_retired`].
    retired: Option<Arc<PreparedTable>>,
    /// Position in the period, `0.0..1.0`.
    phase: f64,
    /// Smoothed playhead (finite). Clamped into the grid of `current` when the voice takes a
    /// table; `set_playhead` may move it past the grid later, so each table still clamps it.
    y: Ramp,
    z: Ramp,
    /// The playhead of `fading` during a swap: the playhead as it was before the swap clamped
    /// it, moved by `set_playhead` like `y` and `z`, so the old table plays on without a jump.
    fading_y: Ramp,
    fading_z: Ramp,
    /// Smoothed note in Hz: the last usable note (0 before any), kept while the note is muted.
    frequency: Ramp,
    /// Note gain, 0 (muted by an unusable note, or no note yet) to 1; multiplies `gain`.
    note_gain: Ramp,
    /// Output gain, 0 (stopped) to 1 (playing).
    gain: Ramp,
    /// Weight of `current` during a swap, ramping 0 to 1; `fading` gets the rest.
    mix: Ramp,
    smoothing_len: u32,
    swap_len: u32,
    play_len: u32,
    last_weights: LevelWeights,
}

impl std::fmt::Debug for Voice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Voice")
            .field("sample_rate", &self.sample_rate)
            .field("has_table", &self.current.is_some())
            .field("is_swapping", &self.is_swapping())
            .field("playhead", &self.playhead())
            .field("frequency", &self.frequency())
            .field("gain", &self.gain.value)
            .finish_non_exhaustive()
    }
}

impl Voice {
    /// A stopped voice with no table, at the playhead (0, 0) and no note, smoothing over
    /// [`SMOOTHING_SECONDS`] and fading over [`SWAP_FADE_SECONDS`] and [`PLAY_FADE_SECONDS`].
    ///
    /// Allocates the interpolation kernel; call it off the audio thread.
    ///
    /// # Errors
    ///
    /// [`PreviewError::InvalidSampleRate`] when `sample_rate` is not finite or not positive.
    pub fn new(sample_rate: f32) -> Result<Voice, PreviewError> {
        Voice::with_times(
            sample_rate,
            SMOOTHING_SECONDS,
            SWAP_FADE_SECONDS,
            PLAY_FADE_SECONDS,
        )
    }

    /// Like [`Voice::new`], with the smoothing time and the swap and play/stop fade times
    /// given in seconds. Meant for tests and experiments (a time of 0 shows the click the
    /// fade prevents); the preview itself uses [`Voice::new`]. A time that is not finite or
    /// not positive means no ramp at all.
    ///
    /// # Errors
    ///
    /// [`PreviewError::InvalidSampleRate`] when `sample_rate` is not finite or not positive.
    pub fn with_times(
        sample_rate: f32,
        smoothing_seconds: f32,
        swap_fade_seconds: f32,
        play_fade_seconds: f32,
    ) -> Result<Voice, PreviewError> {
        if !(sample_rate.is_finite() && sample_rate > 0.0) {
            return Err(PreviewError::InvalidSampleRate);
        }
        Ok(Voice {
            sample_rate,
            kernel: Kernel::new(),
            current: None,
            fading: None,
            retired: None,
            phase: 0.0,
            y: Ramp::at(0.0),
            z: Ramp::at(0.0),
            fading_y: Ramp::at(0.0),
            fading_z: Ramp::at(0.0),
            frequency: Ramp::at(0.0),
            note_gain: Ramp::at(0.0),
            gain: Ramp::at(0.0),
            mix: Ramp::at(1.0),
            smoothing_len: ramp_samples(smoothing_seconds, sample_rate),
            swap_len: ramp_samples(swap_fade_seconds, sample_rate),
            play_len: ramp_samples(play_fade_seconds, sample_rate),
            last_weights: SILENT,
        })
    }

    /// Whether the voice sounds: its gain is above 0 or heading there.
    fn is_sounding(&self) -> bool {
        self.gain.value > 0.0 || self.gain.target > 0.0
    }

    /// Plays `table` from now on (see *Table swaps* on [`Voice`]). Returns `None` when the
    /// voice took the table, and gives it back unused (`Some`) while
    /// [`is_swapping`](Voice::is_swapping): try again later.
    pub fn set_table(&mut self, table: Arc<PreparedTable>) -> Option<Arc<PreparedTable>> {
        if self.is_swapping() {
            return Some(table);
        }
        // Not swapping, so `fading` and `retired` are both empty.
        let Some(old) = self.current.take() else {
            self.clamp_playhead(&table);
            self.current = Some(table);
            return None;
        };
        if self.is_sounding() && self.swap_len > 0 {
            self.fading = Some(old);
            self.fading_y = self.y;
            self.fading_z = self.z;
            self.mix.jump(0.0);
            self.mix.to(1.0, self.swap_len);
        } else {
            self.retired = Some(old);
        }
        self.clamp_playhead(&table);
        self.current = Some(table);
        None
    }

    /// Clamps the playhead ramps into `table`'s grid, so a later move starts from where the
    /// table really plays (a ramp from far past the grid would sit at the edge and then step).
    fn clamp_playhead(&mut self, table: &PreparedTable) {
        self.y.clamp_to(last_index(table.y_size()));
        self.z.clamp_to(last_index(table.z_size()));
    }

    /// Whether a swap is fading, or a retired table waits for
    /// [`take_retired`](Voice::take_retired). [`set_table`](Voice::set_table) refuses a table
    /// meanwhile.
    pub fn is_swapping(&self) -> bool {
        self.fading.is_some() || self.retired.is_some()
    }

    /// Hands out the table a swap retired, if any; the caller disposes of it (off the audio
    /// thread).
    pub fn take_retired(&mut self) -> Option<Arc<PreparedTable>> {
        self.retired.take()
    }

    /// Moves the playhead to (`y`, `z`), in frames: smoothly while sounding, at once while
    /// silent. Each coordinate is clamped into the grid of the table being played; a
    /// coordinate that is not finite is ignored (the previous value, or ramp, stays). When the
    /// voice takes a table, the playhead is clamped into its grid at once, so a move after a
    /// swap to a smaller grid ramps over the whole smoothing time.
    pub fn set_playhead(&mut self, y: f32, z: f32) {
        let len = self.ramp_len();
        let fading = self.fading.is_some();
        if y.is_finite() {
            self.y.to(y, len);
            if fading {
                self.fading_y.to(y, len);
            }
        }
        if z.is_finite() {
            self.z.to(z, len);
            if fading {
                self.fading_z.to(z, len);
            }
        }
    }

    /// Sets the note to `hz`: smoothly while sounding, at once while silent.
    ///
    /// A value that is not finite or not positive is unusable: it mutes the note, fading it
    /// out over the play/stop fade while sounding (at once while silent), with the last usable
    /// note; once fully muted, nothing sounds and the phase holds. Play/stop state does not
    /// change. A usable note after that jumps if the mute has completed (nothing sounds, so
    /// nothing clicks) and ramps otherwise, and the note fades back in.
    pub fn set_frequency(&mut self, hz: f32) {
        let fade = if self.is_sounding() { self.play_len } else { 0 };
        if !(hz.is_finite() && hz > 0.0) {
            self.note_gain.to(0.0, fade);
            return;
        }
        if self.note_gain.value > 0.0 {
            self.frequency.to(hz, self.ramp_len());
        } else {
            self.frequency.jump(hz);
        }
        self.note_gain.to(1.0, fade);
    }

    /// The smoothing ramp length now: 0 (a jump) while silent.
    fn ramp_len(&self) -> u32 {
        if self.is_sounding() {
            self.smoothing_len
        } else {
            0
        }
    }

    /// The smoothed playhead (`y`, `z`) as of the last rendered sample: clamped into the grid
    /// of the table the voice last took, but not into a grid `set_playhead` has moved it past
    /// since. For tests: it shows the ramp.
    pub fn playhead(&self) -> (f32, f32) {
        (self.y.value, self.z.value)
    }

    /// The smoothed note in Hz as of the last rendered sample: 0 before any usable note, and
    /// the last usable note while an unusable one mutes it. For tests: it shows the ramp.
    pub fn frequency(&self) -> f32 {
        self.frequency.value
    }

    /// Starts sounding, fading in from the current gain.
    pub fn play(&mut self) {
        self.gain.to(1.0, self.play_len);
    }

    /// Fades out; after the fade the output is exact zeros until the next
    /// [`play`](Voice::play).
    pub fn stop(&mut self) {
        self.gain.to(0.0, self.play_len);
    }

    /// The level weights of the current table at the last rendered sample (silent weights
    /// before the first sample or without a table). For tests: they show that all four frames
    /// use one level choice.
    pub fn last_weights(&self) -> LevelWeights {
        self.last_weights
    }

    /// Fills `out` with the next samples. Real-time safe.
    pub fn render(&mut self, out: &mut [f32]) {
        let max = f64::from(f32::MAX);
        for sample in out.iter_mut() {
            self.gain.advance();
            self.frequency.advance();
            self.note_gain.advance();
            self.y.advance();
            self.z.advance();
            self.fading_y.advance();
            self.fading_z.advance();
            self.mix.advance();
            // A fully muted note renders as no note: silent weights, phase held.
            let frequency = if self.note_gain.value > 0.0 {
                self.frequency.value
            } else {
                0.0
            };
            let (y, z) = (self.y.value, self.z.value);
            let gain = f64::from(self.gain.value) * f64::from(self.note_gain.value);
            let mix = if self.fading.is_some() {
                f64::from(self.mix.value)
            } else {
                1.0
            };

            let mut sum = 0.0f64;
            self.last_weights = SILENT;
            if let Some(table) = &self.current {
                let (value, weights) = render_table(
                    &self.kernel,
                    table,
                    self.phase,
                    y,
                    z,
                    frequency,
                    self.sample_rate,
                );
                sum += mix * value;
                self.last_weights = weights;
            }
            if let Some(old) = &self.fading {
                let (value, _) = render_table(
                    &self.kernel,
                    old,
                    self.phase,
                    self.fading_y.value,
                    self.fading_z.value,
                    frequency,
                    self.sample_rate,
                );
                sum += (1.0 - mix) * value;
            }
            // At gain 0 write +0.0 itself: a negative sum times 0 would be −0.0.
            *sample = if gain > 0.0 {
                (sum * gain).clamp(-max, max) as f32
            } else {
                0.0
            };

            // The fade has ended: the old table is retired (the slot is empty, since a swap
            // only starts while nothing is retired).
            if self.fading.is_some() && !self.mix.is_running() && self.retired.is_none() {
                self.retired = self.fading.take();
            }
            let next = self.phase + f64::from(frequency) / f64::from(self.sample_rate);
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

/// The last frame index on an axis of `size` frames, as a playhead coordinate.
fn last_index(size: u32) -> f32 {
    // A prepared table has at most `MAX_PREVIEW_SAMPLES` (2^24) samples, so `size - 1` is below
    // 2^24 and exact in f32.
    size.saturating_sub(1) as f32
}
