# Mip levels: full-band frames only

- **Issue:** #8
- **Status:** Done (spec approved by the maintainer, 2026-11-12)
- **Crates:** none (docs only)
- **Format change:** yes (`SPEC.md` states that files hold full-band frames only; version bump:
  no)

---

## Spec

### Problem

To play a table across the keyboard without aliasing, an oscillator needs band-limited copies of
each frame: roughly one per octave, with fewer harmonics in each. These copies are called mip
levels, from *multum in parvo* ("much in little"), a name borrowed from texture mipmapping in
computer graphics. Issue #8 asked whether a TUW3 file may carry precomputed mip levels, or
whether it holds only full-band frames and the player builds the copies itself.

### Decision

Agreed with the maintainer in the brainstorming session (2026-11-12):

1. **A TUW3 file stores only full-band frames.** Version 0 has no mip levels.
2. **A use that needs band-limited copies builds them itself.** For example, an oscillator
   builds them at load time, or lazily for large tables. This follows D14 (#7): only the
   oscillator needs mip levels, so only the oscillator makes them, from its own copy. An
   envelope, a filter or an LFO at sub-audio rates has no use for them.
3. **No version bump:** nothing changes for existing files.
4. **Hand-made mip levels are deferred, not rejected.** An author shaping the top octaves by
   hand is a real but niche benefit. It may come back later, either as optional data in the
   file that only an oscillator reads or in another form (for example a TUWS preset setting).
   How that could work is tracked in [#21](https://github.com/jongsma/tuw/issues/21); the
   answer there may differ from what is described here.
5. **Settle how optional binary data is added before the first public release.** Small optional
   data already has a place: unknown metadata keys are ignored and preserved (D11), up to
   1 MiB of JSON. Bulk sample data such as mip levels doesn't fit there. Today a reader must
   reject unknown flag bits and any bytes after the samples, so optional binary data would
   need a version bump or a "safe to ignore" mechanism. While no files exist outside this
   project, either is cheap. After a public release it isn't. *(Claude's call, from review:
   "binary" added, since the metadata block already covers small optional data.)*

6. **Importing a source that already has mip levels keeps the full-band frames only.** TUW3
   can't store the copies, and the oscillator rebuilds them anyway. If an importer can tell
   that a source holds mip levels (for example a separate chunk or a documented layout), it
   keeps the full-band level and drops the rest, and TUWE tells the user that it did so.
   If the copies are indistinguishable from ordinary frames (just more frames in a WAV), the
   importer can't know, and they arrive as ordinary frames that the author can delete. Keeping
   hand-made levels on import comes back with #21. *(Detail of the import rules belongs to the
   TUWE import work, e.g. #9.)*

**Why** (for D15):

- Building the copies takes one forward FFT per frame plus one inverse FFT per frame and level
  (about 10–11 levels for 2048-sample frames). For a typical table (256 frames of 2048 samples)
  that is roughly 3,000 FFTs: tens of milliseconds on one thread. Tables near the size limit
  take seconds and their copies need gigabytes of memory; building lazily spreads the time but
  doesn't save that memory, and storing the copies in the file wouldn't either. Storing them
  would save little load time.
- Stored copies would roughly double the file (halving the size per level: 1 + ½ + ¼ + … ≈ 2),
  or grow it about elevenfold (full size per level), which would also hit the 2²⁸-sample limit
  that much sooner.
- They can go stale: a tool that edits frames without knowing about the copies leaves copies of
  frames that no longer exist.
- The spec would have to pin down exactly how many levels there are, which harmonics each
  keeps, how they're stored, and how frame sizes that aren't a power of two are handled. Every
  writer would have to match that exactly, or files from different tools would sound different.
- Mip levels only make sense for one use, the oscillator. A file that carries them leans towards
  one use, against D14.

### Changes to `SPEC.md`

Exact wording; the builder may fix typos and links, but not change the meaning.

**Uses of a table**: insert a new paragraph after the one that starts "Each use prepares the
table in its own way.":

> A file stores each frame once, at full bandwidth. Played as an oscillator, a frame of
> `frame_size` samples can hold harmonics up to `frame_size / 2` (rounded down). Band-limited
> versions of the frames (mip levels), which an oscillator typically uses to play high notes
> without aliasing, are not stored. A use that needs them builds them from the frames.

**Open questions**: remove the #8 bullet. Add:

> - How optional binary data (for example hand-made mip levels) can be added to a file without
>   breaking existing readers. To be settled before the first public release ([#21](https://github.com/jongsma/tuw/issues/21))

### Other documentation

- `DECISIONS.md`: new entry **D15. TUW3 files hold full-band frames only (Nov 2026)**, recording
  decisions 1–6 and the why above, closing with references to #8 and #21.
- `DESIGN-NOTES.md` §2.2: replace the *Proposal* paragraph with the outcome: TUW3 stores only
  full-band frames, and the oscillator computes mip levels at load or lazily (D15). Hand-made
  mip levels may return in another form ([#21](https://github.com/jongsma/tuw/issues/21)). Keep the
  first paragraph (one level per octave, crossfading, identical level choice for all four
  corners) unchanged.

### Acceptance criteria

- [x] AC1: `SPEC.md`, `DECISIONS.md` and `DESIGN-NOTES.md` contain the changes above, and
  `SPEC.md` no longer lists #8 as open.
- [x] AC2: all commands in `AGENTS.md` pass. No code changes; `tuw3` already reads and writes
  only full frames.

### Out of scope

- How TUWS builds, stores and crossfades mip levels (Phase 4; DESIGN-NOTES §2.2).
- Designing the optional-data mechanism itself (#21).
- Spec licence (#6).

### Open questions

- *May files carry mip levels?* Not in version 0 (decision 1).
- *Version bump?* No (decision 3).
- *How is optional binary data added later?* Deferred to #21, to be settled before the
  first public release (decision 5).

---

## Plan

Docs only; executed natively (as for #7). No production code or tests change: `tuw3` already
reads and writes only full-band frames, and nothing in this decision adds behaviour to test.

1. Apply the `SPEC.md` text exactly as given above; remove the #8 bullet, add the #21 bullet.
2. Write D15 in the D12–D14 style (**Decision:** with decisions 1–6 and "No `version` bump",
   **Why:** from the list above, references to #8 and #21).
3. Update DESIGN-NOTES §2.2 as listed.
4. Have a fresh reviewer check the DSP claims (AGENTS.md *Gotchas*): FFT counts, file growth,
   harmonics up to `frame_size / 2`.
5. Run the `AGENTS.md` commands; commit as `docs(tuw3): TUW3 files hold full-band frames only
   (#8)`; open the PR (closes #8).

---

## Tasks

- [x] T1 (AC1): `SPEC.md`
- [x] T2 (AC1): `DECISIONS.md` D15, DESIGN-NOTES §2.2
- [x] T3 (AC1): fresh DSP review, fixes
- [x] T4 (AC2): full check, commit, PR

---

## Notes

- The fresh DSP review found no errors, only loose wording, which was fixed: the FFT count is
  per frame *and* level; 10–11 levels holds for 2048-sample frames; LFOs at audio rate alias
  too, so only sub-audio LFOs skip mip levels. In `SPEC.md`, "which an oscillator needs" became
  "typically uses", since oversampling is another way. Skipped: noting that for even frame
  sizes the top harmonic can only be a cosine at a fixed phase (too fine for the spec).
