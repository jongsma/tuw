# Phase 1: scope and order of work

- **Issue:** #23 (tracking issue; milestone *Phase 1*)
- **Status:** Planned
- **Crates:** tuw-core / tuwe (plus a possible `tuw-wav`, see #9)
- **Format change:** no

This is not a single feature but the plan for the whole of Phase 1 ("TUWE editor core and the
TUW3 spec", README roadmap). It fixes what Phase 1 delivers and in which order. Each item in
[Order of work](#order-of-work) becomes its own GitHub issue in a *Phase 1* milestone, and gets
its own feature doc (L track) or acceptance criteria (M track) before it is built.

---

## Spec

### Intent

At the end of Phase 1 there is a **rough, working prototype of TUWE** in which every basic
operation works. It is the foundation of the application, not its final form: the UI is
deliberately throwaway, just enough controls to use each operation. The real interface is
designed later, once the end product is clearer.

Why: the maintainer wants to use the editor, see and hear what it does, and learn from that
before designing the real interface. Phase 2 (algorithmic generation) and TUWS (Phase 4) build on
the same engine, so the engine has to be right even where the UI is rough.

### Goals

1. All operations live in `tuw-core` as a documented, tested API. The egui app in `tuwe` is a thin
   shell over it, cheap to replace.
2. The three parts everything else rests on are settled first and tested headless:
   the in-memory table model, the edit/undo model, and the band-limited preview oscillator with
   a lock-free hand-off from the UI.
3. From the walking skeleton onwards (step 2 of the order) the prototype is usable: it can be
   seen, heard and saved.
4. Phase 1 is done when one session can run end to end (see [Done when](#done-when)).

### Non-goals

- A designed UI, a 3D (waterfall) view, theming, accessibility polish.
- MIDI input, polyphony, envelopes, effects: the preview is one voice.
- Formulas or expressions for frames, spectral morphing (moving harmonics instead of
  crossfading), converting a whole table to another frame size.
- Anything algorithmic (Phase 2), the plugin (Phase 4), release builds and the CI matrix
  (Phase 5).

### Anti-goals

Phase 1 fails, even if every button works, if:

- the engine is tangled into the egui code, so Phase 2 or TUWS can't reuse it;
- the preview misleads: aliasing, clicks or zipper noise that the table itself doesn't have;
- an operation silently changes samples (D14: only actions the author asks for change samples;
  opening, saving, viewing and playing never do).

### Constraints

- Everything in `AGENTS.md`: crate direction `tuwe` → `tuw-core` → `tuw3`; `tuw-core` without GUI
  or plugin dependencies; real-time rules for everything the audio callback reaches; tests first
  for DSP, float comparisons with justified tolerances, the edge cases (last frame on an axis,
  `y_size` or `z_size` of 1, frame sizes that are not a power of two).
- TUW3 as specified in `docs/SPEC.md`, including its size limits. `tuw3` may gain additive public
  functions (for example a metadata size check, #24), but no change to the format or to existing
  behaviour.
- D14 (samples are use-neutral; DC removal, phase canonicalisation and normalisation are editor
  actions on request; warnings appear in TUWE) and D15 (TUW3 holds full-band frames; recognised
  mip copies are dropped on import with a message).
- D4 stack: Rust, egui, `rustfft`, `hound`. Audio output needs a new GPL-compatible dependency
  (e.g. `cpal`), checked with `cargo deny`.

### What Phase 1 contains

**Tables and files**

- New table: frame size and Y × Z grid, within the TUW3 size limits; frames start silent.
- Open and save TUW3 (file I/O in `tuwe` or `tuw-core`, never in `tuw3`).
- Edit metadata (name, author and the other keys `SPEC.md` defines); unknown keys survive.
- Import and export Serum-style WAV with a `clm` chunk (`AGENTS.md`), after #9 is decided.
  - **Import** opens as a new table (Y holds the frames, Z = 1), or goes into a chosen row of the
    open table as one undoable edit. If the frame size differs from the open table's, the author
    explicitly chooses to resample (as with single-cycle import); nothing is resampled silently.
    Recognised mip copies are dropped with a message (D15).
  - **Export** writes one chosen row (one Z slice) as a Serum WAV. Its frame size must be 2048;
    any other size is refused with a message, because resampling on export would change samples
    the author didn't ask to change (D14).
- One table is open at a time.
- Warnings about a table (for example DC offset, samples outside [-1, 1]), shown in TUWE, never
  changing anything (D14).

**Making a frame**

- Basic shapes: sine, saw, square/pulse (with width), triangle.
- From harmonics: the harmonics bar chart shows the selected frame's harmonics, and editing it
  changes their levels while keeping each harmonic's phase. "Frame from harmonics" is the same
  chart starting from a silent frame (phases from the canonicalisation schedule, §2.1). One tool,
  not a separate generator.
- Drawing by hand: freehand and straight lines on the waveform, plus a smooth action.
- Import one single-cycle WAV as a frame. A different length (an AKWF file has 600 samples) is
  resampled to the table's frame size.

**Working on the grid**

- A selection is any rectangle of frames on the Y × Z grid: one frame, a row, a column or a
  block. A *row* is all Y frames at one Z; a *column* is all Z frames at one Y.
- Copy, paste, clear the selection.
- Add or remove rows and columns (changes `y_size` / `z_size`).
- Fill between frames: in a selection one frame thick, a crossfade between its two end frames;
  in a block, bilinear from its four corner frames. This is the same maths as playback, so what is filled is
  what is heard.

**Changing frames (on a selection)**

- Gain, invert, reverse. Gain and normalisation on several frames use one shared factor for the
  whole selection by default, which keeps designed level differences (D14); "each frame
  separately" is an option.
- DC removal, phase canonicalisation (DESIGN-NOTES §2.1, low-crest-factor schedule) and
  normalisation (D14), only when the author asks.

**Undo / redo**

- Every edit that changes samples, the grid or metadata is one undoable step. Opening, saving and
  playing are not undoable.

**Preview (clean, minimal)**

- One voice at one note, chosen with a slider or the computer keyboard; play/stop.
- Alias-free across the keyboard, in the strict sense: no harmonic above Nyquist at any note,
  with a crossfade between adjacent band-limited levels so there are no timbre jumps: the oscillator builds its own band-limited copies of the frames
  (D15), interpolates bilinearly between the four frames around the playhead, and picks the same
  band-limited level for all four (DESIGN-NOTES §2.2).
- Moving the playhead (y, z) morphs live, smoothed so there is no zipper noise (DESIGN-NOTES §2.4).
- Edits are heard without restarting playback, after each finished step (the end of a stroke or
  a drag, i.e. each undo step), not during the gesture. Changes reach the audio thread lock-free.
- The preview plays the table as sound, so it removes DC on its own copy (D14.3). A safety
  limiter protects the audio output only; the table is never changed.
- TUWE may set a lower size limit for preview than `SPEC.md` allows and report bigger tables as
  *unsupported* for preview (`AGENTS.md` gotcha: never *invalid*); they still open, edit and save.
- No MIDI, no envelope, no polyphony.

**The prototype window**

```
┌─────────────────────────────┬──────────────────────┐
│ Grid (Y × Z) of small        │ Selected frame:      │
│ thumbnails, click to select  │ big waveform (draw   │
│ ▢▢▢▢▢▢                       │ here) + harmonics    │
│ ▢▢▣▢▢▢   playhead ●          │ bar chart            │
│ ▢▢▢▢▢▢                       │                      │
├─────────────────────────────┴──────────────────────┤
│ Tools · operations · Y/Z playhead · note · ▶ play  │
└────────────────────────────────────────────────────┘
```

Layout, styling and the exact controls are throwaway; only that each operation is reachable
matters.

**Open questions in the milestone**

- **#9** (WAV interop: which crate, fallback without `clm`) is decided early, before WAV import
  and export. Leaning, to be settled in the issue: a small `tuw-wav` crate, and asking for the
  frame size when there is no `clm` chunk rather than guessing.
- **#21** (optional binary data) and **#6** (licence of the spec) stay in the milestone, but no
  prototype work waits on them. They must be settled before a public release.

### Approaches

How to order the work. Three were considered:

- **A. Engine first, UI last.** All operations headless in `tuw-core`, the egui shell at the end.
  Cleanest API and easiest for the loop, but nothing to see or hear until the end, and the API is
  designed without being used.
- **B. Thin vertical slices from day one.** Each issue one operation from engine to button.
  Usable early, but the foundations (table model, edit/undo model, audio hand-off) get decided in
  passing inside small slices; a wrong guess there means reworking every slice.
- **C. Foundations first, then slices.** *Chosen by the maintainer.* The three hard-to-change
  parts are built and tested headless first, then a walking skeleton makes the app usable, then
  every remaining operation is a small slice on top. The foundations are right before anything
  leans on them, the app is in use from step 2, and most slices are small, well-defined issues that
  suit the build loop.

### Order of work

Each line becomes one issue (track in brackets). Within a step, the order is a suggestion; across
steps it is a dependency.

**Step 1: foundations (headless, in `tuw-core`)**

1. (#24) [L] Table model: an editable in-memory table (frame size, Y × Z grid of frames, metadata),
   converting to and from `tuw3::Table` without changing samples; a rectangular selection.
2. (#25) [L] Edit model: every change is a command applied to the table, recorded as one undo step;
   undo and redo.
3. (#26) [L] Preview oscillator (one voice, not built ahead for TUWS): band-limited copies per frame, bilinear morph with one shared level
   choice, smoothed playhead, real-time safe; a lock-free way to hand it a changed table and a new
   playhead from the UI.
4. (#27) [M] Audio output: the device stream that runs the oscillator (new dependency, `cargo deny`).

**Step 2: walking skeleton (`tuwe`)**

5. (#28) [L] Prototype window: the grid, the frame view, new table, open and save TUW3, the playhead,
   note and play. From here on the app is usable.

**Step 3: slices (engine + UI each)**

6. (#9) [Decision] WAV interop.
7. (#29) [L] Serum WAV import and export (after 6), including dropping recognised mip copies (D15).
8. (#30) [M] Metadata editing.
9. (#31) [M] Basic shapes.
10. (#32) [L] Harmonics bar chart: view and edit a frame's harmonics, and make a frame from them.
11. (#33) [M] Drawing (freehand, lines, smooth).
12. (#34) [M] Single-cycle WAV import as a frame, with resampling (after 6).
13. (#35) [M] Grid editing: copy, paste, clear; add and remove rows and columns.
14. (#36) [L] Fill between frames (crossfade and bilinear).
15. (#37) [M] Frame operations: gain, invert, reverse, DC removal, normalisation.
16. (#38) [L] Phase canonicalisation.
17. (#39) [M] Warnings.

**Any time before a release (not blocking the prototype)**

18. (#21) [Decision] optional binary data.
19. (#6) [Decision] licence of the spec.

### Done when

Phase 1 is done when, in one session:

1. a new table is started;
2. frames are made and edited with the tools above, and undone and redone;
3. the morph is heard while moving the playhead, without aliasing, clicks or zipper noise;
4. the table is saved as TUW3 and reopened with every sample bit-identical;
5. the table is exported as a Serum WAV that loads in another wavetable synth (for example Vital).

Every operation has automated tests in `tuw-core`. Points 3 and 5 are also checked by hand by the
maintainer (the sound, and the other synth).

### Calls made in the design

Settled during the builder check, not discussed in the planning session; the maintainer checks
them while reading:

- **Selection** is a rectangle on the grid; fill uses the selection's ends or corners.
- **The preview removes DC** on its own copy, as D14.3 requires for sound, and has an output-only
  safety limiter.
- **"Without aliasing"** is strict: no harmonic above Nyquist, crossfaded levels.
- **Preview size limit:** big tables may be *unsupported for preview*, never refused as files.
- **The preview oscillator is one voice** and not shaped for TUWS's needs (`AGENTS.md`: don't build
  ahead); Phase 3 stabilises the engine API for TUWS.

### What is left to the builder

Per item, in its own feature doc: data structures, the undo mechanism (snapshots or inverse
commands), the band-limiting method and level count, the smoothing time, the egui layout and
widgets, where file I/O lives (`tuw-core` or `tuwe`), the resampling method for single-cycle
import, the phase schedule for canonicalisation (within DESIGN-NOTES §2.1), and the default frame
size and grid of a new table (2048 is the recommended frame size, `SPEC.md`).
