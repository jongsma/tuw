# Editable table model with rectangular selection

- **Issue:** #24
- **Status:** Clarifying (spec approved; plan in progress)
- **Crates:** tuw-core (plus one small additive API in tuw3, see *Open questions*)
- **Format change:** no

---

## Spec

### Problem

Every Phase 1 operation (drawing, shapes, fill, grid editing, frame operations) changes a table,
and the prototype window, undo (#25) and the preview oscillator (#26) all read it. `tuw3::Table`
can't serve as that table: it is validated once and immutable, and editing it means taking it
apart and building a new one. `tuw-core` has no table type at all yet.

This is the first of the three Phase 1 foundations (`23-phase-1-scope.md`, approach C): the part
everything else rests on, built and tested headless before anything leans on it. A wrong choice
here means reworking every later slice.

### Behaviour

**The table**

- A table has a frame size, a Y × Z grid of frames and metadata (`tuw3::Metadata`, unknown keys
  included).
- **Always saveable.** A table obeys the TUW3 rules at all times (`SPEC.md`, *Size limits* and
  *Samples*): frame size 8 ..= 65 536, each axis 1 ..= 4 096 frames, at most 2²⁸ samples in total,
  every sample finite, metadata at most 1 MiB encoded. An operation that would break a rule is
  refused with a typed error and changes nothing. Converting to `tuw3::Table` therefore never
  fails.
- **The frame size is fixed** for the life of a table: chosen at creation (or by the file opened)
  and never changed.
- **New tables start silent:** every sample 0.0. There is no "empty" grid cell.
- **Converting** from a `tuw3::Table` and back gives the same table bit for bit: sizes, every
  sample's bit pattern (including `-0.0`), and every metadata key, unknown keys included (D14: no
  operation changes samples unless asked).

**Versions and editing (approach B, chosen by the maintainer)**

- A table is an **immutable value**: a version. Every change produces a new version and leaves the
  old one untouched and usable.
- A **frame** is a buffer of `frame_size` samples that never changes once made, held through a
  shared, reference-counted pointer. A version holds the sizes, the metadata and a grid of
  `y_size × z_size` frame pointers, in TUW3 order (Z outer, Y inner).
- A change copies only the frames it touches; all other frames are **shared** between the old and
  the new version. Example: drawing on one frame of a 256 × 2048 table costs one new 8 KiB frame
  plus a new list of 256 pointers, not a 2 MiB copy.
- Adding or removing rows and columns only rearranges pointers (new rows and columns are silent
  frames). The operations themselves are exposed here as primitives; the user-facing grid editing
  is #35.
- The primitives this issue provides (the minimum later issues build on):
  - create a silent table with given sizes;
  - read: sizes, metadata, one frame by (y, z), iterate frames;
  - replace one frame, or a set of frames, with new samples;
  - insert and remove rows (along Z) and columns (along Y) at a position;
  - replace the metadata;
  - convert from and to `tuw3::Table`.
- Versions and frames can be sent to and shared with other threads (`Send + Sync`), so #26 can
  hand a version to the audio thread. *How* it is handed over, and that the audio thread never
  frees the last reference to an old version, are #26's job; this issue only makes it possible.

**Size and speed**

- Every valid table converts, edits and converts back correctly, up to the TUW3 limits. Nothing
  valid is refused as unsupported.
- Speed is only promised up to **65 536 frames** (for example 256 × 256), far beyond real tables
  (Serum tables have up to 256 frames). Bigger grids work, but every edit copies the whole list of
  frame pointers and opening makes one allocation per frame, so they are slow. This is a known,
  accepted limit (maintainer); grouping pointers per row is the fix if it ever matters.

**The selection**

- A selection is a **rectangle** on the grid: a Y range and a Z range, each at least one frame.
  One frame, a row (all Y frames at one Z), a column (all Z frames at one Y) and a block are all
  rectangles.
- The selection belongs to the open table but is **not part of its content**: selecting is not
  an edit (no undo step, #25) and is never saved.
- The selection is a separate value, not stored in a version (versions are content; undo in
  #25 swaps versions and must not restore old selections). The type that holds "current version
  + selection" together is #25's; this issue provides the selection value and its rules.
- It always lies inside the grid. Per axis, with an insert or remove at index `p` (an inserted
  row or column takes index `p`):
  - **insert** at `p` ≤ the selection's start: the selection moves along by the inserted count;
    start < `p` ≤ end: it grows by the inserted count; `p` > end: unchanged;
  - **remove**: the selection keeps the frames that remain and shrinks accordingly; if all its
    frames on that axis are removed, it becomes the single row or column now at its old start
    index, or the last one if that index no longer exists.
- **Fit to a grid:** a general rule for any change of grid size (for example when undo swaps in
  an older version): each range is clamped into the grid, keeping at least one frame. Inserts
  and removes use the precise rules above; fitting is the fallback.
- A table always has at least one frame, so a selection always exists.
- It can be set to: one frame, a whole row, a whole column, a rectangle between two corners, the
  whole table.

### Acceptance criteria

- [ ] **AC1 (round trip):** for random valid `tuw3::Table`s (property test), converting to the
  editable table and back gives sizes, metadata and samples bit-identical to the input,
  including `-0.0` and subnormal samples, `y_size = 1`, `z_size = 1`, both 1, and frame sizes that
  are not a power of two.
- [ ] **AC2 (new table):** a new table has the requested sizes, every sample is `0.0`, and it
  converts to a `tuw3::Table` that `tuw3::write` / `tuw3::read` round-trips. Every size outside
  the TUW3 limits is refused with its own error.
- [ ] **AC3 (immutability and sharing):** after replacing one frame, the old version is unchanged
  (bit for bit), the new version differs only in that frame, and every other frame is the *same
  buffer* in both versions (pointer equality), not a copy.
- [ ] **AC4 (always valid):** each operation that would break a TUW3 rule is refused with a typed
  error and the table is unchanged: a non-finite sample (NaN, ±∞), a wrong frame length,
  growing an axis past 4 096, growing past 2²⁸ samples, removing the last row or column,
  metadata over 1 MiB, a position outside the grid.
- [ ] **AC5 (rows and columns):** inserting and removing rows and columns at the start, the
  middle and the end (including the last frame on an axis) puts every existing frame at the
  expected (y, z) and shares its buffer; new frames are silent; property test: inserting then
  removing the same row or column gives back an equal table.
- [ ] **AC6 (selection):** each way of setting a selection gives the expected rectangle; a
  selection outside the grid is refused; after every insert and remove from AC5 the selection is
  adjusted exactly as described under *Behaviour* (insert before, inside, after; partial and
  full removal, at both ends) and lies inside the grid; fitting any selection to any valid grid
  gives a selection inside it (property tests over random grids, selections and edits).
- [ ] **AC7 (threads):** versions and frames are `Send + Sync` (compile-time check).
- [ ] **AC8 (crate rules):** `tuw-core` gains no GUI, plugin or file I/O dependency; every public
  item is documented; all `AGENTS.md` commands pass.
- [ ] **AC9 (many small frames):** a table with many tiny frames (for example 4 096 × 16 frames
  of 8 samples) converts, edits one frame and converts back correctly. Correctness only, no
  timing.

### Out of scope

- Undo and redo, and the command model (#25).
- Handing versions to the audio thread, and freeing old ones off it (#26).
- User-facing operations: drawing, shapes, grid editing UI, fill, frame operations (#31–#39).
- File I/O: opening and saving files is `tuwe`'s job (#28); this issue converts to and from
  `tuw3::Table`, which converts to and from bytes.
- Changing a table's frame size.
- Several open tables, or copy and paste between tables.

### Open questions

- *Fixed or changeable frame size?* Fixed per table (maintainer, this session).
- *Can a grid cell be empty?* No; new frames are silent (maintainer).
- *Is the selection undoable or saved?* Neither (maintainer).
- *Storage layout?* Approach B: one shared, immutable buffer per frame, a version is a grid of
  references (maintainer). See *Approaches*.
- *What happens to the selection when its frames are removed?* See *Behaviour* (my call, see
  *Calls made*).
- *May `tuw3` gain a public API for the metadata check?* Yes: additive public functions are
  allowed, with no change to the format or to existing behaviour (maintainer). `tuw3` gets one
  small, documented check of the encoded metadata length (for example
  `Metadata::encoded_len`), instead of a throwaway `tuw3::Table` built only to validate metadata.
- *How must extreme but valid grids behave?* Correct up to the TUW3 limits, fast up to 65 536
  frames (maintainer). See *Size and speed*.

### Approaches

- **A. One block of samples in TUW3 order.** Open and save without copying. But inserting or
  removing a row shifts everything after it, undo has to copy each changed region, and handing
  the table to the audio thread means copying all of it or sharing mutable memory (forbidden by the
  real-time rules).
- **B. One shared, immutable buffer per frame; a version is a grid of references.** *Chosen.*
  Rows and columns move as pointers; an edit copies only the frames it touches and shares the
  rest, which makes undo steps cheap (#25) and lets the audio thread get a new version without a
  big copy (#26). Costs one copy at open and one at save (well under a millisecond for a typical
  2 MiB table) and one real-time rule for #26: the audio thread must not free the last reference
  to an old version.
- *Dropped:* one block with copy-on-write per region. It combines the downsides of A and B and is
  harder to reason about.

### Calls made

Not discussed with the maintainer; check while reading:

- **Where the selection lives:** a standalone value with insert, remove and fit-to-grid rules;
  the owner of "version + selection" is #25 (builder check).
- **Selection edge cases:** insert before moves it, inside grows it, after leaves it; full
  removal falls back to the row or column now at its old start (builder check).
- **"Always valid" includes the position check:** reading or replacing a frame outside the grid
  is an error, not a panic.

### What is left to the builder

Type and module names; whether a version is itself behind an `Arc`; the exact primitive
signatures (single frame versus batch replacement, error enum layout); iterators; how the
selection is stored (inclusive ranges or corners); `Debug` output for large tables.

---

## Plan

*To be written with `writing-plans` once the spec is approved.*

---

## Tasks

*To be written with the plan.*

---

## Notes
