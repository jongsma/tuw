# Frame normalisation: what a TUW3 file promises about its samples

- **Issue:** #7
- **Status:** In progress (spec approved by the maintainer, 2026-11-12)
- **Crates:** tuw3 (tests and crate docs only)
- **Format change:** yes (`SPEC.md` gains *Uses of a table* and sample-range advice; X no longer
  means "one single cycle"; version bump: no)

---

## Spec

### Problem

`SPEC.md` says how samples are stored, but not what they should look like: their level, whether
a frame is centred on zero (DC offset), and whether harmonics have consistent phases between
neighbouring frames. Issue #7 asked whether TUW3 should promise phase-canonical and/or DC-free
frames, or leave that to the editor.

The answer depends on what a table is for. The project's defining idea is that **one table can
be used for anything**: an oscillator, an LFO, an envelope or a filter curve (DESIGN-NOTES §1.2).
Properties that help one use break another:

- A DC offset is unwanted in an oscillator, but it is the whole point of a unipolar LFO
  (0 to 1), an envelope, or a filter curve read as gains.
- Phase canonicalisation keeps an oscillator's sound but destroys the shape of an LFO,
  envelope or filter curve.
- Normalising each frame to a peak of 1 erases loudness or gain differences that the author
  designed on purpose.

So the file can't promise any of these without ruling out a use. The spec must instead
promise that **samples arrive exactly as the author made them**, and each use prepares the
table for itself.

### Decisions

Agreed with the maintainer in the brainstorming session (2026-11-12):

1. **A table's samples are use-neutral.** TUW3 stores what the author made. Readers, loaders
   and libraries must not change the samples: no DC removal, no phase rewriting, no rescaling.
2. **What a table is used for is not stored in TUW3.** It belongs in the TUWS preset format
   (Phase 4), which says how a preset uses a table ("table A as oscillator, table B as filter").
   No metadata field in TUW3 for it.
3. **DC is always removed for sound, by the oscillator.** TUWS's oscillator removes the
   per-frame DC offset from its own playback copy (for example while it builds band-limited
   versions). LFO, envelope and filter uses read the untouched values.
4. **Phase canonicalisation, DC removal and normalisation are TUWE editor actions**, offered
   where they make sense. Warnings about a table (DC offset, peak above 1, phases that would
   dip during a morph) appear in TUWE, never in the plugin.
5. **No requirements on level, DC or phase.** The spec only recommends a sample range (below).
   No existing version 0 file becomes invalid, so there is **no version bump**.

### Changes to `SPEC.md`

Exact wording; the builder may fix typos and links, but not change the meaning.

**Overview**, first sentence, replace:

> A TUW3 file holds a three-dimensional wavetable: a grid of single-cycle frames along two
> morph axes (Y and Z).

with:

> A TUW3 file holds a three-dimensional wavetable: a grid of frames along two morph axes
> (Y and Z). A frame is a list of `frame_size` samples, usually one cycle of a waveform; see
> [Uses of a table](#uses-of-a-table) for other uses.

**Axes**, replace the X bullet:

> - **X** is the sample position within a frame: one single cycle, `frame_size` samples long.

with:

> - **X** is the sample position within a frame, `frame_size` samples long. What X stands for
>   (one cycle, time, frequency) depends on how the table is used, not on the file.

**Samples**, replace the paragraph "The level or range of the samples is not specified yet
(see #7)." with:

> Writers should keep samples within [-1, 1]. Values outside that range are valid, and readers
> must accept them.
>
> Nothing else about the samples is required: a frame may have a DC offset (a non-zero mean),
> any peak level, and any phases between its harmonics. Readers and libraries must return the
> samples exactly as stored and must not normalise, re-centre or otherwise change them while
> reading. See [Uses of a table](#uses-of-a-table) for why.

**New section** after *Samples*, before *Metadata*:

> ## Uses of a table
>
> A table does not say what it is for. The same file can serve as, for example:
>
> - an **oscillator**: each frame is one cycle of a waveform;
> - an **LFO**: each frame is one cycle of a modulation shape, possibly unipolar (0 to 1);
> - an **envelope**: each frame is a curve over time with a start and an end;
> - a **filter**: each frame describes a gain curve over frequency.
>
> Each use prepares the table in its own way. An oscillator, for example, should remove each
> frame's DC offset from its playback copy, while an LFO, envelope or filter needs the values
> as stored. How a use reads a frame (whether it wraps around at the end, how it maps a frame
> onto frequencies) is up to that use and is not part of this format.
>
> Editors may offer to remove DC offsets, align harmonic phases between frames or normalise
> levels, but should apply these only when the author asks, since each of them can break a use
> other than the one the author has in mind.

**Open questions**: remove the #7 bullet.

### Other documentation

- `DECISIONS.md`: new entry **D14. TUW3 samples are use-neutral (Nov 2026)**, recording
  decisions 1–5 above and the why from *Problem*.
- `DESIGN-NOTES.md`:
  - §1.1: "a grid of single-cycle frames" becomes "a grid of frames", plus one sentence pointing
    to *Uses of a table* in `SPEC.md`.
  - §1.2: mention that LFOs and envelopes are uses too, and that the use is recorded in the
    TUWS preset, not the table (D14).
  - §2.1: replace the *Open* paragraph with the outcome: an editor action only, offered for
    tables used as sound (D14). Also reword the two sentences that contradict D14:
    "Frames should also be DC-free" becomes DC-free *for oscillator use*, and "at import, or
    as an editor action" becomes an editor action applied only when the author asks.
    *(Raised by the design review; approved with the spec.)*
  - New **[parked]** note for TUWS (Phase 4), e.g. as §1.3 *Per-use conditioning controls*:
    TUWS may offer manual, per-use controls for imported tables: a **DC offset** control
    that shifts a frame's curve up or down (removing an offset is one setting of it; adding
    one turns a bipolar LFO unipolar) and a **phase** control (e.g. align harmonic phases).
    They work on TUWS's own copy, are saved in the preset (with the table's use, decision 2),
    and never change the file (decision 1). The oscillator's automatic DC removal
    (decision 3) stays on regardless, since an offset is never useful in audio.
    *(Raised by the maintainer, 2026-11-12; exact defaults to be designed in Phase 4.)*
- `crates/tuw3/src/lib.rs`: the crate doc says "a grid of single-cycle frames"; reword it to
  match the new *Overview*.

### Acceptance criteria

- [ ] AC1: `SPEC.md`, `DECISIONS.md`, `DESIGN-NOTES.md` and the `tuw3` crate doc contain the
  changes above; `SPEC.md` no longer lists #7 as open, and nothing in `docs/` or `crates/`
  still calls a frame "single-cycle" as a requirement. Merged feature docs (such as
  `13-tuw3-reader-writer.md`) are historical records and stay unchanged; describing *source
  material* as single-cycle (DESIGN-NOTES §3, AKWF) is correct and stays.
- [ ] AC2: a test reads back, bit for bit, a table whose frames have a large DC offset
  (all samples in 0.5 ..= 1.0), and one that is entirely negative.
- [ ] AC3: a test reads back, bit for bit, a table with samples outside [-1, 1]
  (e.g. ±4.0 and ±1000.0); `Table::new` and `read` both accept it.
- [ ] AC4: a property test: for any table of finite samples, `read(write(table))` returns the
  same samples bit for bit (compare `f32::to_bits`, so −0.0 and subnormals count too).
  Already covered by `valid_tables_round_trip` in `tests/properties.rs` (random finite bit
  patterns, compared with `f32::to_bits`); AC4 is met by pointing to it.
- [ ] AC5: all commands in `AGENTS.md` pass.

### Out of scope

- How a filter maps a frame onto frequencies (raw samples vs. harmonic magnitudes, linear vs.
  log axis): TUWS filter design, see #10 and DESIGN-NOTES §1.2.
- How envelopes and LFOs read a frame (wrap vs. stop at the end, one-shot vs. loop): TUWS.
- The TUWS preset format that records a table's use: Phase 4.
- Implementing the TUWE editor actions and warnings, and the oscillator's DC removal: later
  features in `tuwe` / `tuw-core`. `tuw-core` has no loader yet; when it gets one, decision 1
  applies to it.
- Mip levels: #8.

### Open questions

- *Should the file promise DC-free / phase-canonical / normalised frames?* No; decided above
  (decision 5).
- *Should a table say what it is meant for?* No; the TUWS preset does (decision 2).
- *Is a version bump needed?* No; nothing that was valid becomes invalid (decision 5).

---

## Plan

> **For agentic workers:** use superpowers:subagent-driven-development or
> superpowers:executing-plans to work through the tasks below. Steps use checkboxes.

**Goal:** make `SPEC.md` say that TUW3 samples are use-neutral and stored exactly as authored,
and pin that promise with tests.

**Architecture:** tests first, then docs. The new tests document behaviour that the reader
already has (`Table::new` only rejects non-finite samples; `read` hands samples to
`Table::new`), so they are expected to **pass on the first run**. If one fails, that is a reader
bug: fix it in `crates/tuw3/src/` in this PR before going on. No other production code changes.

**Tech stack:** Rust (workspace `tuw3` crate, existing `proptest` dev-dependency), Markdown.

**Spec:** the *Spec* section above.

### Global constraints

- Every new source file starts with `// SPDX-License-Identifier: GPL-3.0-or-later` (none are
  expected; tests go into an existing file).
- Compare floats bit for bit via `assert_same_table` (it uses `f32::to_bits`), never with `==`.
- Doc links to `write` in `tuw3` are written ``[`write()`]`` (AGENTS.md, *Gotchas*).
- Branch `docs/7-frame-normalisation`; Conventional Commits with the crate as scope; no push
  or PR unless the maintainer asks.

### Review focus

1. NaN/±Infinity must still be rejected after the wording change ("values outside [-1, 1] are
   valid"). Already pinned by `non_finite_sample_is_rejected` (`tests/rejection.rs`) and
   `non_finite_samples_are_rejected_with_their_position` (`tests/table.rs`); keep both green.
2. The new anchor `#uses-of-a-table` must resolve: the heading text must be exactly
   `## Uses of a table`. Checked in Task 2, step 3.
3. `write()` must not alter samples either; covered by the round trips in Task 1 (they go
   through `write()` and `read`).
4. No text left that promises DC-free or single-cycle frames as a requirement. Checked by the
   grep in Task 2, step 3.

---

## Tasks

### Task 1: Pin "samples arrive as stored" in tests (AC2, AC3, AC4)

**Files:**
- Modify: `crates/tuw3/tests/round_trip.rs` (append after `special_finite_values_round_trip`;
  update the module doc line to mention #7)

**Interfaces:**
- Consumes: `round_trip(&Table)` (local helper in that file), `tuw3::Table::new(frame_size,
  y_size, z_size, samples: Vec<f32>, metadata) -> Result<Table, TableError>`, `Metadata::default()`.
- Produces: nothing used later.

- [ ] **Step 1: Write the tests**

```rust
/// #7 AC2: a DC offset is data, not damage (unipolar LFOs, envelopes, filter curves).
#[test]
fn offset_frames_round_trip() {
    // Frame 0 entirely in 0.5 ..= 1.0, frame 1 entirely negative.
    let mut samples: Vec<f32> = (0..8).map(|x| 0.5 + x as f32 / 14.0).collect();
    samples.extend((0..8).map(|x| -0.25 - x as f32 / 10.0));
    round_trip(&Table::new(8, 2, 1, samples, Metadata::default()).unwrap());
}

/// #7 AC3: samples outside [-1, 1] are valid; writers only *should* avoid them.
#[test]
fn out_of_range_samples_round_trip() {
    let samples = vec![4.0, -4.0, 1000.0, -1000.0, 1.000_000_1, -1.000_000_1, 2.0, -2.0];
    round_trip(&Table::new(8, 1, 1, samples, Metadata::default()).unwrap());
}
```

AC4 needs no new test: `valid_tables_round_trip` in `tests/properties.rs` already round-trips
random finite bit patterns and compares with `f32::to_bits`.

- [ ] **Step 2: Run them**

Run: `cargo test -p tuw3 --test round_trip`
Expected: PASS, including `offset_frames_round_trip` and `out_of_range_samples_round_trip`.
If either fails, fix the reader/`Table::new` so it accepts any finite sample, then re-run.

- [ ] **Step 3: Commit**

```bash
git add crates/tuw3/tests/round_trip.rs
git commit -m "test(tuw3): samples with a DC offset or outside [-1, 1] round-trip unchanged"
```

### Task 2: Spec, decision and notes (AC1)

**Files:**
- Modify: `docs/SPEC.md` (*Overview*, *Axes*, *Samples*, new *Uses of a table*, *Open questions*)
- Modify: `docs/DECISIONS.md` (append D14 after D13)
- Modify: `docs/DESIGN-NOTES.md` (§1.1, §1.2, §2.1, new parked §1.3)
- Modify: `crates/tuw3/src/lib.rs:14` (crate doc)
- Modify: this feature doc (status, task boxes)

**Interfaces:** none.

- [ ] **Step 1: Apply the `SPEC.md` text** exactly as given under *Changes to `SPEC.md`*, and
  drop the #7 bullet from *Open questions*.

- [ ] **Step 2: Write the rest**
  - `DECISIONS.md` **D14. TUW3 samples are use-neutral (Nov 2026)**, in the D12/D13 style:
    a **Decision:** paragraph listing decisions 1–5 (stating "No `version` bump"), and a
    **Why:** list taken from *Problem* (one table, many uses; each property helps one use and
    breaks another; the use lives in the TUWS preset). Close with a reference to #7.
  - `DESIGN-NOTES.md` as listed under *Other documentation*. The new §1.3 is tagged
    **[parked]**.
  - Crate doc line 14 becomes: "A TUW3 file holds a grid of frames along two morph axes
    (Y and Z). The samples are returned exactly as stored: what a frame means (a cycle, a curve
    over time or over frequency) is up to the caller."

- [ ] **Step 3: Check**

```sh
grep -rn -i "single.cycle" docs/SPEC.md docs/DECISIONS.md docs/DESIGN-NOTES.md crates/
grep -n "^## Uses of a table$" docs/SPEC.md
grep -n "#uses-of-a-table" docs/SPEC.md
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

Expected: the first grep only hits DESIGN-NOTES §3 ("single-cycle WAVs", source material);
the second finds exactly one heading; the third finds the two links from *Overview* and
*Samples*; the doc build passes.

- [ ] **Step 3b: Fresh DSP review.** Per AGENTS.md *Gotchas* ("DSP prose in
  `DESIGN-NOTES.md` is untested"), have a fresh reviewer check the new and changed DSP claims
  (DC and phase effects per use, §1.3, *Uses of a table*) before committing.

- [ ] **Step 4: Commit**

```bash
git add docs/SPEC.md docs/DECISIONS.md docs/DESIGN-NOTES.md crates/tuw3/src/lib.rs docs/features/7-frame-normalisation.md
git commit -m "docs(tuw3): TUW3 samples are use-neutral (#7)"
```

### Task 3: Full check (AC5)

- [ ] **Step 1: Run all `AGENTS.md` commands**

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

Expected: all pass, no `cargo fmt` diff.

- [ ] **Step 2: Mark the feature doc *Status: Done*,** tick the acceptance criteria, and commit
  that as `docs(tuw3): mark #7 done`. Opening the PR (which closes #7)
  waits for the maintainer.

---

## Notes
