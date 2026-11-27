# Band-limited preview oscillator

- **Issue:** #26
- **Status:** Planned
- **Crates:** tuw-core
- **Format change:** no

---

## Spec

### Problem

TUWE's preview must let the author hear the table while editing it: one voice, at one note, with
the playhead (y, z) morphing live. It is the third Phase 1 foundation (`23-phase-1-scope.md`,
step 1.3) and the anti-goal that matters most here is "the preview misleads: aliasing, clicks or
zipper noise that the table itself doesn't have".

Playing a full-band frame directly aliases at higher notes, so the preview needs band-limited
copies of every frame. TUW3 doesn't store them (D15): the player builds them from its own copy.
`tuw-core` has no oscillator yet. #25 gives the preview what it should play: the document's
*finished version* and a *change id* that changes whenever that version does. #27 will run the
oscillator in an audio device stream; this issue is headless.

### Behaviour

**Two halves**

- A **preparer** (UI thread) turns a finished `Wavetable` version into a **prepared table**: a
  ready-to-play copy with DC removed and band-limited copies of every frame. All work, memory
  allocation and freeing happens here.
- An **oscillator** (audio thread) renders samples from a prepared table into a buffer it is
  given. It never allocates, locks, blocks, logs or frees, and never panics on runtime values
  (`AGENTS.md`, *Real-time audio safety*).
- The table the author edits is never changed: preparing reads the version and changes nothing
  in it (D14).

**Preparing a table**

- **DC removal** (D14.3): each frame's prepared copies have no DC component.
- **Band-limited copies** (D15, DESIGN-NOTES §2.2): for each frame, a series of *levels*, each
  keeping fewer harmonics than the one before, **two levels per octave** (maintainer), so the
  played bandwidth never drops below about 70% of Nyquist (about 15.5 kHz at 44.1 kHz) as the note
  moves. Each level contains no harmonic above its stated maximum, strictly.
- An N-sample frame holds N/2 − 1 full harmonics plus a Nyquist bin (an 8-sample frame: 3
  harmonics; 2048 samples: 1023). The levels drop the Nyquist bin.
- **Memory** (maintainer): a prepared table uses at most **8× the memory of the table's samples**
  (about 16 MiB for a typical 256 × 2048 table, 512 MiB at the preview limit), counting the levels
  of every frame. Opening a table at the limit takes on the order of a second.
- **Reuse** (maintainer, approach B): when a new version arrives, a frame that is the same buffer
  as in the previously prepared version (#24's sharing, `Frame::ptr_eq`) reuses its prepared
  copies; only new frames are built. Editing one frame rebuilds that frame only; opening a table,
  or an edit that touches every frame, builds them all.
- Frame sizes that are not a power of two (600 for AKWF) and the smallest frame size (8 samples,
  3 harmonics) work.
- **Preview size limit** (maintainer): tables with more than **2²⁴ samples** in total
  (`frame_size × y_size × z_size`) are refused by the preparer with a typed error that reports
  them as *unsupported for preview*, never as invalid (`AGENTS.md` gotcha). Such tables still open,
  edit and save; only preview is unavailable. 2²⁴ samples exactly is accepted.

**Playing**

- **Controls** the UI can send at any time: a new prepared table, the playhead (y, z) as
  continuous frame positions (`0.0 ..= y_size − 1`, `0.0 ..= z_size − 1`; clamped into the grid,
  also when a table with a different grid arrives), the note as a frequency in Hz, and play or stop. The
  oscillator is created with the output sample rate (from #27's device).
- **Bilinear morph** (DESIGN-NOTES §2.2, §2.4): the output at each sample interpolates between
  the four frames around the playhead; all frames share one phase accumulator, so morphing never
  jumps in phase. With `y_size` or `z_size` of 1, or at the last frame of an axis, the morph
  degenerates correctly (no read past the grid).
- **One level choice for all four frames**: the level (or the pair of levels being crossfaded)
  depends only on the note and the sample rate, never on which frames are read.
- **No aliasing from the levels, strictly**: at every note, every harmonic in every level that
  contributes (both levels during a crossfade), times the note frequency, is below Nyquist.
  Neighbouring levels are crossfaded as the note changes, so the timbre never jumps.
- **Rendering artefacts below −80 dB** (maintainer): reading a level between its stored samples
  (interpolation) always leaves faint images of the spectrum, some of which fold below Nyquist.
  Everything in the output that is not a harmonic of the note stays at least **80 dB below the
  fundamental**. The interpolator and the length of each stored level are chosen to meet this
  within the memory cap.
- **Near and above Nyquist**: the last level (the fundamental alone) fades out over a stated
  range just below Nyquist, so a glide upwards doesn't click; a note at or above Nyquist plays
  silence.
- **No zipper noise**: changes to the playhead and the note are smoothed over a short time
  (DESIGN-NOTES §2.4), not applied in one step. Smoothing ramps linearly and reaches its target
  exactly at the end of the smoothing time.
- **No clicks from the preview itself**:
  - a new prepared table arriving while a note plays is crossfaded in from the old one over a few
    milliseconds;
  - play and stop fade in and out briefly.
- The output is mono `f32`, at the level the table has (no normalisation). Protecting the
  speakers is #27's output-only safety limiter.
- Output is always finite; prepared tables contain only finite samples (the table model
  guarantees finite input).

**Handing things to the audio thread**

- The UI sends new prepared tables and control changes to the oscillator **lock-free**; the
  oscillator picks them up at the start of a buffer or between samples.
- **The audio thread never frees a prepared table or any part of one.** Prepared frames are
  shared between old and new prepared tables (reuse), so the rule covers every shared frame: the
  last reference to anything is never dropped on the audio thread, even when the UI side has
  already dropped its own references. Old prepared tables are handed back to the UI side and
  dropped there (the real-time rule #24 left for this issue).
- The preview plays only finished versions (#25): the UI prepares `Document::finished()` when
  `Document::change_id()` changes, never `current()`. During a gesture the preview keeps playing
  the version from before the gesture.

**Not built ahead**

- One voice, not shaped for TUWS (`AGENTS.md`: don't build ahead; Phase 3 stabilises the engine
  API for TUWS).
- Preparing runs on the UI thread. If very large tables ever need it, preparing can move to a
  background thread later (approach C, maintainer); nothing in this design may rule that out.

### Acceptance criteria

- [ ] **AC1 (levels are band-limited):** for random frames (property test, frame sizes including
  8, 600 and 2048), every level's copy, analysed by FFT, has no energy above its stated maximum
  harmonic and in the Nyquist bin beyond floating-point noise (tolerance justified in the test),
  and no DC; consecutive levels' maximum harmonics differ by a factor of about √2 (two per
  octave).
- [ ] **AC2 (no aliasing at any note):** for sample rates 44 100, 48 000 and 96 000 Hz and notes
  across the whole range (MIDI 0–127, random frequencies, and frequencies up to and above
  Nyquist), the highest harmonic of every level with a non-zero weight, times the note frequency,
  is below Nyquist. A rendered steady note from a full-band frame (all harmonics present, for
  example a saw), analysed with a window and length stated in the test, has no component that is
  not a multiple of the note frequency above **−80 dB relative to the fundamental**, at MIDI
  notes across the range and at least one note inside each crossfade range.
- [ ] **AC3 (crossfade is continuous):** the weight of each level is a continuous function of the
  note frequency: sweeping the note in small steps, up to and through Nyquist, never changes any
  level weight by more than a small bound per step; the weights sum to 1 below the last level's
  fade-out range and fall continuously to 0 across it.
- [ ] **AC4 (bilinear morph):** with the playhead exactly on a frame, the output equals rendering
  that frame alone; between frames it is the bilinear mix of the four (checked against a reference
  computed in the test); this holds with `y_size = 1`, `z_size = 1`, both 1, and at the last frame
  of each axis.
- [ ] **AC5 (one level choice):** the level weights used for the four frames are identical at
  every sample (by construction and by a test with four different frames).
- [ ] **AC6 (DC removed, table unchanged):** a frame with a constant offset renders with no DC in
  the output (mean below a justified bound over whole cycles); after preparing, the source version
  is bit-identical to a deep snapshot taken before (`to_tuw3()`).
- [ ] **AC7 (reuse):** preparing a version that differs in one frame from the previously prepared
  one builds copies for that frame only; every other frame's copies are shared (checked by
  pointer identity or a build counter).
- [ ] **AC8 (smoothing):** after a jump of the playhead or the note, the rendered morph position
  and frequency move linearly: no per-sample change exceeds the bound implied by the smoothing
  time, and they equal the target exactly once that time has passed.
- [ ] **AC9 (no clicks on swap, play, stop):** swapping in a prepared table with a different frame
  at the playhead while a note plays, and starting and stopping, never produces a sample-to-sample
  step larger than a bound derived from steady playing of the louder table (justified in the
  test).
- [ ] **AC10 (real-time safety):** rendering, picking up a new table and control changes on the
  audio side perform no heap allocation and no deallocation (checked with a counting global
  allocator in a test binary), including when the UI side has already dropped every reference it
  held to the old prepared table and to frames shared with it; old prepared tables and frames are
  dropped on the UI side, not in the render call.
- [ ] **AC11 (size limit and memory):** a table of exactly 2²⁴ samples prepares; one more frame is
  refused with the *unsupported for preview* error; neither changes the source version. A
  prepared table's sample memory (all levels of all frames, as reported by the prepared table) is
  at most 8× the source table's, for frame sizes 8, 600, 2048 and 65 536.
- [ ] **AC12 (edge notes):** a note at or above Nyquist renders silence; very low notes (below
  1 Hz) and the lowest MIDI note render finite output using the fullest level; the output is
  finite for every combination tested above.
- [ ] **AC13 (crate rules):** `tuw-core` gains `rustfft` (D4) and at most one small lock-free
  crate, both GPL-compatible and passing `cargo deny check`; no GUI, plugin, audio-device or
  file I/O dependency; every public item is documented; all `AGENTS.md` commands pass.

### Hand check

Only the ear can judge these; they go on the batch hand-check list, not into the criteria. They
need #27 (audio output) to be done:

- A saw-like table swept slowly across the keyboard: no audible aliasing, no audible timbre jumps
  between levels.
- Dragging the playhead across a table with very different frames: smooth morph, no zipper noise.
- Editing the frame under the playhead while a note plays: no click when the edit lands.

### Out of scope

- The audio device stream and the output safety limiter (#27).
- MIDI, polyphony, envelopes, effects (`23-phase-1-scope.md`, non-goals).
- Building prepared tables on a background thread (approach C, kept possible for later).
- The UI controls (#28) and deciding when the UI sends what, beyond the rule that only finished
  versions are prepared.
- Any change to the table's samples (DC removal etc. as editor actions are #37 and #38).

### Open questions

Each settled in this session, with the maintainer unless marked as my call:

- *When are band-limited copies built?* By a preparer on the UI thread, reusing the copies of
  unchanged frames (maintainer: approach B). A background thread (C) is a later option.
- *How large a table can be previewed?* Up to 2²⁴ samples; bigger ones are *unsupported for
  preview* (maintainer).
- *Which FFT library?* `rustfft` (D4, already settled).
- *How clean must the output be?* Inharmonic components at least 80 dB below the fundamental
  (maintainer). The levels themselves are strictly band-limited.
- *How much memory may a prepared table use?* At most 8× the table's samples; the preview limit
  stays at 2²⁴ (maintainer). If −80 dB can't be met within 8×, the loop stops and asks.
- *How many levels?* Two per octave, so the bandwidth never drops below about 70% of Nyquist
  (maintainer).
- *Arbitrary recordings as tables?* Not here: turning a recording into single-cycle frames is
  Phase 2 analysis work (DESIGN-NOTES §3.1); Serum WAVs (#29) and single-cycle WAVs (#34) cover
  wavetable-shaped audio in Phase 1 (discussed with the maintainer).

### Approaches

- **A. Rebuild every frame after each finished step**, on the UI thread. Simplest; about 30 ms
  per edit on a typical 256-frame table, longer on big ones.
- **B. Reuse the copies of unchanged frames.** *Chosen.* #24's sharing shows which frames are
  new, so a one-frame edit rebuilds one frame (well under a millisecond); the full build happens
  on open or when every frame changes. Costs a cache keyed by frame and its cleanup.
- **C. Build on a background thread** while the old version keeps playing. Never stalls the UI,
  but adds a thread and a short delay before an edit is heard. Kept as the way forward if large
  tables need it (maintainer).

### Calls made

Not discussed with the maintainer; check while reading:

- **Preparer and oscillator as two halves**, all allocation on the preparer side.
- **A new prepared table is crossfaded in** over a few milliseconds while a note plays, so an edit
  at the playhead doesn't click; play and stop fade.
- **Old prepared tables are returned to the UI side** to be freed there.
- **The playhead is in frame positions** (0 to size − 1) and clamped into the grid when a table
  with a different grid arrives, like the selection (builder check).
- **Smoothing is a linear ramp** that reaches its target exactly (builder check: an exponential
  smoother never reaches it, which AC4 and AC8 need).
- **The last level fades out below Nyquist** instead of cutting off (builder check: a cut-off
  would click on a glide).
- **The levels drop the Nyquist bin** (builder check).
- **The note is a frequency in Hz** (the UI maps keys to frequencies); the oscillator is created
  with the sample rate.
- **A note at or above Nyquist plays silence.**
- **Output is mono, at the table's own level**; the limiter is #27's.
- **The preparer prepares only finished versions** (rule for the UI, from #25).

### What is left to the builder

Module and type names; which harmonics each level keeps (two per octave, strictly alias-free); how
levels are stored and the interpolator (any combination that meets −80 dB within the 8× memory cap;
if none does, stop and ask); the width of the last level's fade-out below Nyquist; the crossfade
curve between levels and its range; the smoothing times and the table-swap and play/stop fade times
(a few milliseconds, stated and justified); the lock-free mechanism (std atomics or one small
GPL-compatible crate, checked with `cargo deny`); how the cache of prepared frames is keyed and
cleaned up; block-based or per-sample rendering API, as long as #27 can call it from an audio
callback with buffers of any size.

---

## Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** a `preview` module in `tuw-core`: a UI-side `Preparer` that turns a `Wavetable` into a
`PreparedTable` of strictly band-limited, DC-free levels (two per octave, reusing unchanged
frames), and a real-time `Oscillator` that renders it with bilinear morph, crossfaded levels,
smoothing and click-free swaps, fed lock-free by a `PreviewControl`.

**Architecture:** levels are built by FFT resynthesis: each level keeps harmonics `1..=H` of the
frame and is stored oversampled at least 2× (length `M = max(4H, 16)`), so a 16-tap
Kaiser-windowed sinc kernel (passband up to a quarter of the level's rate, stopband from three
quarters) reads it with images far below −80 dB. The oscillator is a plain single-threaded
`Voice` (rendering, smoothing, fades, table crossfade) wrapped by `rtrb` ring buffers: commands
go UI → audio, retired tables go audio → UI, so the audio thread never drops the last `Arc`.

**Tech stack:** Rust 2024 (MSRV 1.85); new runtime dependencies of `tuw-core`: `rustfft = "6.4"`
(D4) and `rtrb = "0.4"` (MIT OR Apache-2.0; lock-free SPSC ring buffer, needed because
`tuw-core` forbids `unsafe`); `proptest` (dev, present).

**Spec:** the *Spec* section above.

### Global constraints

- Every new `.rs` file starts with `// SPDX-License-Identifier: GPL-3.0-or-later`.
- Only `rustfft` and `rtrb` are added; after adding them run `cargo deny check` and
  `cargo tree -e features -i serde_json` (no `arbitrary_precision`, no `preserve_order`;
  `AGENTS.md` gotcha). No GUI, plugin, audio-device or file I/O dependency.
- Every public item documented (`#![warn(missing_docs)]`); `#![forbid(unsafe_code)]` stays.
- Never `==` on floats (clippy `float_cmp`): compare with a tolerance justified in a comment, or by
  bits where exactness is the claim.
- Real-time rules (`AGENTS.md`) for everything `Oscillator::render` reaches: no allocation, no
  deallocation, no locks, no syscalls, no logging, no `unwrap`/`expect`/`panic!`/indexing that can
  fail at runtime; all buffers preallocated when the `Oscillator` is made.
- Constants (in `preview`, documented): `MAX_PREVIEW_SAMPLES = 1 << 24`,
  `MAX_MEMORY_FACTOR = 8`, `FADE_START = 0.8` (a level is fully on while its top harmonic is at
  most 0.8 × Nyquist), `KERNEL_TAPS = 16`, `KERNEL_PHASES = 512`, `KERNEL_BETA = 10.0`,
  `SMOOTHING_SECONDS = 0.02`, `SWAP_FADE_SECONDS = 0.01`, `PLAY_FADE_SECONDS = 0.005`.
- `cargo test --workspace` (never only `-p`), plus the other `AGENTS.md` commands, before each
  commit. Spectral tests run in debug builds too; keep their sizes so the whole suite stays
  under a minute (`2^15`-sample renders are enough).

### Interfaces (all tasks)

Module `crates/tuw-core/src/preview/` (`mod.rs` re-exports; `lib.rs` adds `pub mod preview;`):

```rust
// levels.rs
pub fn level_harmonics(frame_size: u32) -> Vec<u32>;
// H_0 = frame_size / 2 - 1; H_{l+1} = floor(H_l * FRAC_1_SQRT_2) while it is < H_l and >= 1,
// otherwise H_l - 1 while >= 1. Strictly decreasing, ends at 1. 8 -> [3, 2, 1].
pub fn level_length(harmonics: u32) -> usize;        // max(4 * harmonics, 16)
pub struct LevelWeights { pub first: usize, pub w_first: f32, pub w_second: f32 }
// `first` = index of the richest level whose top harmonic is below Nyquist; `second` = first + 1
// (weight 0 when first is the last level). Weights per *Semantics*.
pub fn level_weights(levels: &[u32], frequency: f32, sample_rate: f32) -> LevelWeights;

// prepare.rs
pub struct PreparedFrame { /* private: one Vec<f32> per level */ }
impl PreparedFrame { pub fn level(&self, index: usize) -> &[f32]; pub fn level_count(&self) -> usize; }
pub struct PreparedTable { /* frame_size, y_size, z_size, levels: Vec<u32>,
                              frames: Vec<Arc<PreparedFrame>> in TUW3 order */ }
impl PreparedTable {
    pub fn y_size(&self) -> u32; pub fn z_size(&self) -> u32; pub fn frame_size(&self) -> u32;
    pub fn levels(&self) -> &[u32];
    pub fn frame(&self, y: u32, z: u32) -> Option<&Arc<PreparedFrame>>;
    pub fn sample_count(&self) -> usize;   // all levels of all frames
}
pub struct Preparer { /* FftPlanner, cache: HashMap<usize, (Frame, Arc<PreparedFrame>)> */ }
impl Preparer {
    pub fn new() -> Preparer;
    pub fn prepare(&mut self, table: &Wavetable) -> Result<Arc<PreparedTable>, PreviewError>;
    pub fn frames_built_last(&self) -> usize;  // frames built (not reused) by the last prepare
}

// error (in preview/mod.rs or crate error.rs)
pub enum PreviewError {
    Unsupported { samples: u64, limit: u64 }, // "unsupported for preview", never "invalid"
    InvalidSampleRate,                         // not finite or <= 0
    Busy,                                      // the command ring is full; try again later
}

// voice.rs: single-threaded, real-time safe
pub struct Voice { /* sample_rate, current + optional fading-out table, phase: f64,
                      smoothed y, z, frequency, play gain */ }
impl Voice {
    pub fn new(sample_rate: f32) -> Result<Voice, PreviewError>;
    pub fn set_table(&mut self, table: Arc<PreparedTable>) -> Option<Arc<PreparedTable>>;
    // returns a table the voice no longer needs (the caller disposes of it); see Semantics
    pub fn set_playhead(&mut self, y: f32, z: f32);
    pub fn set_frequency(&mut self, hz: f32);
    pub fn play(&mut self); pub fn stop(&mut self);
    pub fn is_swapping(&self) -> bool;
    pub fn take_retired(&mut self) -> Option<Arc<PreparedTable>>; // a table faded out
    pub fn render(&mut self, out: &mut [f32]);
}

// handoff.rs
pub fn preview(sample_rate: f32, capacity: usize) -> Result<(PreviewControl, Oscillator), PreviewError>;
pub struct PreviewControl { /* rtrb Producer<Command>, Consumer<Arc<PreparedTable>> */ }
impl PreviewControl {
    pub fn set_table(&mut self, t: Arc<PreparedTable>) -> Result<(), PreviewError>; // Busy if full
    pub fn set_playhead(&mut self, y: f32, z: f32) -> Result<(), PreviewError>;
    pub fn set_frequency(&mut self, hz: f32) -> Result<(), PreviewError>;
    pub fn play(&mut self) -> Result<(), PreviewError>;
    pub fn stop(&mut self) -> Result<(), PreviewError>;
    pub fn collect(&mut self) -> usize; // drops returned tables on this thread; returns how many
}
pub struct Oscillator { /* Voice, Consumer<Command>, Producer<Arc<PreparedTable>>,
                           fixed-capacity holding slots for tables the return ring can't take */ }
impl Oscillator { pub fn render(&mut self, out: &mut [f32]); }
```

`table.rs` gains `pub(crate) fn addr(&self) -> usize` on `Frame` (`Arc::as_ptr` as `usize`), for
the cache key only.

### Semantics the signatures don't show

- **Levels.** Level `l` of a frame: forward FFT of the frame (length `N = frame_size`), keep bins
  `1..=H_l` and their conjugates, zero DC, the Nyquist bin and everything above `H_l`; inverse FFT of
  length `M_l = level_length(H_l)` (place the kept bins in an `M_l`-point spectrum), scale by
  `1 / N` (rustfft is unnormalised). The result is the frame's band-limited waveform resampled
  exactly to `M_l` samples. Sum of `M_l` ≈ 6.8 N, below `MAX_MEMORY_FACTOR × N` for all frame sizes
  8..=65 536 (check this in a test, not in prose).
- **Weights.** `ny = sample_rate / 2`. A level `l` is *on* to degree
  `on(l) = clamp((ny - H_l × f) / ((1 - FADE_START) × ny), 0, 1)` (0 at and above Nyquist).
  `first` = the smallest `l` with `on(l) > 0`; `w_first = on(first)`;
  `w_second = 1 - w_first` if `first + 1` exists, else 0 (the last level fades to silence).
  If no level is on (fundamental at or above Nyquist), both weights are 0. Because every
  consecutive ratio `H_{l+1} / H_l` ≤ 0.7072 < `FADE_START`, `on(first + 1) = 1` whenever
  `w_first < 1`, so the weights are continuous.
- **Kernel.** `h(t) = sinc(t) × kaiser(t / 8, KERNEL_BETA)` for `|t| < 8` (16 taps), tabulated
  at `KERNEL_PHASES + 1` fractional offsets when the `Voice` is made; reading a level at a
  fractional position interpolates linearly between the two nearest phase rows and sums 16 taps
  with circular indexing. A level whose length is below 16 still indexes circularly.
- **Rendering one sample.** Phase `φ ∈ [0, 1)` (f64), one accumulator for all frames:
  `φ += f / fs`, wrapped. Playhead `y` clamped to `0..=y_size-1`; `y0 = floor(y)` clamped to
  `y_size - 1`, `y1 = min(y0 + 1, y_size - 1)`, `a = y - y0`; same for `z`. Output =
  Σ over the four corner frames of the bilinear weight × (`w_first` × level(first) read at
  `φ × M_first` + `w_second` × level(second) read at `φ × M_second`), times the play gain.
  The same `LevelWeights` for all four frames.
- **Smoothing.** `set_playhead` / `set_frequency` start a linear ramp from the current value to
  the target over `SMOOTHING_SECONDS × fs` samples; at the end the value equals the target exactly.
  A new target mid-ramp starts a new ramp from where it is. The first `set_frequency` before any
  `play` jumps (nothing is sounding).
- **Play/stop.** A gain ramp over `PLAY_FADE_SECONDS`; `stop` ramps to 0 and then holds silence.
- **Table swap.** With no table, `set_table` installs it (no fade). While playing with a table,
  the new table fades in and the old fades out linearly over `SWAP_FADE_SECONDS`; afterwards the
  old one becomes *retired* (`take_retired`). A `set_table` while a swap is running returns the
  new table to the caller unused (`Some`), and `Oscillator` keeps that command queued (it peeks
  and doesn't pop `SetTable` while `is_swapping()`), so swaps run one after another.
- **Disposal.** The `Oscillator` pushes every retired or replaced table into the return ring; if
  the ring is full it keeps it in one of `capacity` preallocated holding slots
  (`[Option<Arc<_>>; N]`-style, or a `Vec` with reserved capacity made in `preview()`) and retries
  at the next `render`. It never drops an `Arc<PreparedTable>`. If every slot is full it stops
  accepting `SetTable` commands until slots free up.
- **Cache.** Keyed by `Frame::addr`; each entry also holds a clone of the source `Frame`, so its
  address can't be reused while cached. After each `prepare` the cache holds exactly the frames
  of that table.

### Review focus

1. **Notes exactly where a level's top harmonic hits Nyquist and `FADE_START × ny`**: weights at
   the boundaries are continuous and the level at Nyquist has weight exactly 0. Pinned in Task 1.
2. **`y_size` or `z_size` of 1 with a fractional playhead** (for example `y = 0.7` on a 1-wide
   grid): clamps to the only frame, no read past the grid. Pinned in Task 4.
3. **A swap to a table with a different grid** while the playhead points past the new grid:
   clamped per table, no panic. Pinned in Task 5.
4. **The UI drops its last references before the audio side retires a table**: the drop still
   happens on the UI thread (`collect`), and `render` deallocates nothing. Pinned in Task 6.
5. **Frame size 8** (3 harmonics, levels `[3, 2, 1]`, every level at the minimum length 16):
   prepares, renders and stays within memory. Pinned in Tasks 2 and 4.

### Files

- Modify `Cargo.toml` (workspace) only if adding to `[workspace.dependencies]`; otherwise
  `crates/tuw-core/Cargo.toml`: `rustfft = "6.4"`, `rtrb = "0.4"`.
- Modify `crates/tuw-core/src/table.rs` (`Frame::addr`), `lib.rs` (`pub mod preview;`, crate doc).
- Create `crates/tuw-core/src/preview/{mod.rs, levels.rs, prepare.rs, kernel.rs, voice.rs,
  handoff.rs}`.
- Tests: `crates/tuw-core/tests/preview_levels.rs`, `preview_prepare.rs`, `preview_render.rs`,
  `preview_voice.rs`, `preview_realtime.rs` (its own binary: it installs a counting
  `#[global_allocator]`), with a shared spectrum helper in `tests/common/spectrum.rs`
  (Blackman–Harris 4-term window, magnitude spectrum by `rustfft`, "largest non-harmonic bin"
  relative to the fundamental, excluding ±6 bins around each multiple of the note).

---

## Tasks

### Task 1: dependencies and level math

**Files:** `crates/tuw-core/Cargo.toml`; create `preview/mod.rs`, `preview/levels.rs`; `lib.rs`;
test `tests/preview_levels.rs`.

**Interfaces:** produces `level_harmonics`, `level_length`, `LevelWeights`, `level_weights`, the
constants, `PreviewError`.

- [ ] **Step 1: Write the failing tests:**
  - `harmonics_per_frame_size`: `level_harmonics(8) == [3, 2, 1]`; for 600, 2048 and 65 536 the
    first is `N/2 - 1`, the list strictly decreases, ends at 1, and every consecutive ratio is
    ≤ 0.7072.
  - `memory_stays_under_the_cap` (AC11 part): for every frame size 8..=4096 and 65 536,
    `Σ level_length(H) ≤ MAX_MEMORY_FACTOR × N`.
  - `no_level_above_nyquist` (AC2 part): for fs ∈ {44 100, 48 000, 96 000}, MIDI notes 0..=127,
    1000 random frequencies in (0, 2 × fs) and frequencies exactly at Nyquist and at each
    `ny / H_l` and `FADE_START × ny / H_l`: every level with a non-zero weight has
    `H × f < ny` (review focus 1), and at or above Nyquist both weights are 0.
  - `weights_are_continuous` (AC3): sweeping f from 1 Hz to 1.2 × ny in 10⁵ geometric steps,
    the full weight vector (weight per level index) never changes by more than 0.01 per step;
    weights sum to 1 (within 1e-6, f32 rounding) while f × H_last < `FADE_START × ny`, and to
    `on(last)` across the last fade-out.
- [ ] **Step 2: Run** `cargo test --workspace`: fails (no module).
- [ ] **Step 3: Implement** as in *Semantics*; add the dependencies; run `cargo deny check` and
  the `serde_json` feature check.
- [ ] **Step 4: Run** the four `AGENTS.md` commands: pass.
- [ ] **Step 5: Commit** `feat(tuw-core): add the preview level math (#26)`.

### Task 2: the preparer

**Files:** `preview/prepare.rs`, `table.rs`; test `tests/preview_prepare.rs`.

**Interfaces:** consumes Task 1. Produces `PreparedFrame`, `PreparedTable`, `Preparer::{new,
prepare}` (reuse is Task 3: in this task every frame is built), `Frame::addr`.

- [ ] **Step 1: Write the failing tests:**
  - `levels_are_band_limited` (AC1): proptest over random frames (sizes 8, 600, 2048; samples
    from `finite_f32` scaled into [-1, 1]): for each level, FFT of length `M_l`: every bin above
    `H_l` (up to `M_l / 2`), and bin 0, has magnitude ≤ 1e-4 × (largest bin of the level) +
    1e-6 (f32 FFT round-off on lengths ≤ 2¹⁶; justify in a comment).
  - `levels_keep_the_harmonics` (AC1): for a frame made of known sines (harmonics 1, 3, 7 with
    amplitudes 1, 0.5, 0.25), every level with `H_l ≥ 7` has exactly those three magnitudes
    (relative error 1e-4), and lower levels keep only the harmonics ≤ `H_l`.
  - `preparing_changes_nothing` (AC6 part): deep snapshot (`to_tuw3`) before and after
    `prepare` are `assert_same`.
  - `size_limit` (AC11): a table of exactly 2²⁴ samples (8 × 2048 × 1024, silent) prepares;
    8 × 2049 × 1024 … use `Wavetable::silent(8, 2048, 1025)` → `Err(Unsupported { samples,
    limit: 1 << 24 })`; frame size 65 536 × 1 × 1 prepares and its `sample_count() ≤ 8 × 65 536`.
  - `frame_size_8` (review focus 5): prepares with levels `[3, 2, 1]`, `sample_count() == 48`.
- [ ] **Step 2: Run** `cargo test --workspace`: fails.
- [ ] **Step 3: Implement** per *Semantics* (one `FftPlanner<f32>` kept in the `Preparer`).
- [ ] **Step 4: Run** the gate: pass (the 2²⁴ test: building 2 048 × 1 024 silent frames of 8 is
  fine; if it takes more than a few seconds in debug, mark only that test `#[ignore]` with a
  reason and run it in the verification step with `--ignored`; record a ruling).
- [ ] **Step 5: Commit** `feat(tuw-core): prepare band-limited levels for the preview (#26)`.

### Task 3: reusing unchanged frames

**Files:** `preview/prepare.rs`; test `tests/preview_prepare.rs`.

- [ ] **Step 1: Write the failing tests** (AC7):
  - `one_changed_frame_builds_one`: prepare a 4 × 3 table, then a version with one frame
    replaced: `frames_built_last() == 1`, every other `PreparedTable::frame` is `Arc::ptr_eq` to
    the earlier one, the changed one is not.
  - `first_prepare_builds_all`: `frames_built_last() == y × z`; a silent table (all frames one
    buffer) builds 1.
  - `cache_holds_only_the_latest`: after preparing A then an unrelated B, preparing A again builds
    all of A's frames.
- [ ] **Step 2: Run:** fails. **Step 3: Implement** the cache per *Semantics*. **Step 4: Run** the
  gate. **Step 5: Commit** `feat(tuw-core): reuse prepared frames that did not change (#26)`.

### Task 4: rendering (kernel, bilinear morph, levels)

**Files:** `preview/kernel.rs`, `preview/voice.rs` (rendering at fixed controls only; no ramps
yet: `set_playhead`/`set_frequency` set values directly in this task); tests
`tests/preview_render.rs`, `tests/common/spectrum.rs`.

**Interfaces:** consumes Tasks 1–3. Produces `Voice::{new, set_table, set_playhead,
set_frequency, play, stop, render}` at fixed values.

- [ ] **Step 1: Write the failing tests:**
  - `no_aliasing_rendered` (AC2): naive saw frame (`N = 2048`, sample i = 2i/N − 1), one frame;
    fs ∈ {44 100, 48 000, 96 000}; notes MIDI 24, 48, 69, 96, 108, 120 and, per fs, one frequency
    inside a crossfade (`(1 + FADE_START) / 2 × ny / H_l` for some mid level): render 1 024 warm-up
    samples, then 2¹⁵; largest non-harmonic bin ≤ −80 dB relative to the fundamental's bin
    (`spectrum.rs` helper; Blackman–Harris sidelobes are −92 dB, below the bound).
  - `bilinear_on_a_frame` (AC4): 3 × 2 table with distinct frames; playhead exactly on (2, 1)
    (last frame of both axes) renders, sample for sample within 1e-6, the same as a 1 × 1 table of
    that frame alone at the same note.
  - `bilinear_between_frames` (AC4): at y = 0.25, z = 0.5 on a 2 × 2 table, the output equals the
    bilinear mix of the four single-frame renders (within 1e-5: f32 summation order).
  - `one_by_one_and_fractional` (AC4, review focus 2): `y_size = 1` with y = 0.7, `z_size = 1`,
    and both: renders the only frames, finite, no panic.
  - `one_level_choice` (AC5): four frames with different spectra at a crossfade note: each
    corner's contribution equals that frame rendered alone with the same `LevelWeights` (check by
    rendering the 2 × 2 at each exact corner and comparing to the single-frame render).
  - `dc_removed` (AC6): a frame of 0.5 + sine: mean of 2¹⁴ rendered samples over whole cycles
    ≤ 1e-4.
  - `edge_notes` (AC12): f = ny and 1.5 × ny render all zeros; f = 0.5 Hz and MIDI 0 render finite
    output using level 0 (`level_weights(..).first == 0`); frame size 8 renders finite output.
- [ ] **Step 2: Run:** fails. **Step 3: Implement** the kernel and the render loop per
  *Semantics*. **Step 4: Run** the gate. **Step 5: Commit**
  `feat(tuw-core): render the preview with bilinear morph (#26)`.

### Task 5: smoothing, play/stop and table swaps

**Files:** `preview/voice.rs`; test `tests/preview_voice.rs`.

- [ ] **Step 1: Write the failing tests:**
  - `playhead_and_note_ramp` (AC8): after `set_playhead` from (0, 0) to (3, 0) and
    `set_frequency` from 220 to 440, a test-visible accessor (`Voice::playhead()`,
    `Voice::frequency()`, documented, `pub`) after each sample moves by at most
    `target_delta / (SMOOTHING_SECONDS × fs) + 1e-6` and equals the target exactly (`to_bits`)
    once the ramp's samples have passed.
  - `no_click_on_swap` (AC9, review focus 3): playing table A (saw), swap to B (inverted saw, a
    3 × 1 grid while the playhead is at y = 2 on A's 4 × 1 grid... clamp): the largest
    sample-to-sample step during and after the swap ≤ 1.5 × the largest step of steady A or B
    playback at that note.
  - `no_click_on_play_stop` (AC9): the first and last samples around `play` and `stop` ramp: no
    step larger than 1.5 × steady playback's largest step; after the fade, exact zeros.
  - `swap_queue` : `set_table` during a swap returns the table unused; `take_retired` yields the
    old table once the swap is done.
- [ ] **Step 2–5:** run (fails), implement per *Semantics*, run the gate, commit
  `feat(tuw-core): smooth the preview and fade table swaps (#26)`.

### Task 6: lock-free hand-off and real-time safety

**Files:** `preview/handoff.rs`; test `tests/preview_realtime.rs` (own binary).

- [ ] **Step 1: Write the failing tests:**
  - `render_allocates_nothing` (AC10, review focus 4): a counting `#[global_allocator]` that
    counts allocations and deallocations only while a `const`-initialised thread-local flag is
    set; on a spawned "audio" thread, set the flag only around `Oscillator::render` calls. The UI
    thread prepares A, sends it, plays, the audio thread renders; the UI prepares B (one frame
    changed, so B shares frames with A), sends it, **drops every `Arc` it holds to A and B and
    clears the `Preparer`**, then the audio thread renders through the whole swap; then the UI
    calls `collect()` until A comes back. Assert zero allocations and deallocations inside
    `render`, and that `collect()` returned ≥ 1.
  - `busy_when_full`: with `capacity` 2, a third command without rendering returns `Busy`.
  - `controls_arrive`: `set_frequency`, `set_playhead`, `play`, `stop` sent through
    `PreviewControl` change the rendered output as the `Voice` tests expect (one smoke check).
- [ ] **Step 2–5:** run (fails), implement per *Semantics*, run the gate, commit
  `feat(tuw-core): hand the preview to the audio thread lock-free (#26)`.

### Task 7: crate docs

**Files:** `lib.rs`, `preview/mod.rs` docs.

- [ ] **Step 1:** module doc for `preview`: the two halves, levels (two per octave, strictly
  band-limited, Nyquist bin dropped, stored ≥ 2× oversampled), the −80 dB bound and why
  interpolation can't be perfect, the hand-off and the never-free-on-audio rule, that only
  `Document::finished()` should be prepared, the size limit as *unsupported for preview*. A
  doctest: prepare a silent 2048 × 1 × 1 table, `preview(48_000.0, 8)`, send it, play, render one
  buffer on the same thread, `collect()`. Add a *The preview* line to the crate doc.
- [ ] **Step 2:** the gate; every DSP claim in the prose matches a test (the DSP reviewer checks
  this, `AGENTS.md` gotcha).
- [ ] **Step 3: Commit** `docs(tuw-core): describe the preview oscillator (#26)`.

---

## Notes

-
