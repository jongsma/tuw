# Editable table model with rectangular selection

- **Issue:** #24
- **Status:** Done
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

- [x] **AC1 (round trip):** for random valid `tuw3::Table`s (property test), converting to the
  editable table and back gives sizes, metadata and samples bit-identical to the input,
  including `-0.0` and subnormal samples, `y_size = 1`, `z_size = 1`, both 1, and frame sizes that
  are not a power of two.
- [x] **AC2 (new table):** a new table has the requested sizes, every sample is `0.0`, and it
  converts to a `tuw3::Table` that `tuw3::write` / `tuw3::read` round-trips. Every size outside
  the TUW3 limits is refused with its own error.
- [x] **AC3 (immutability and sharing):** after replacing one frame, the old version is unchanged
  (bit for bit), the new version differs only in that frame, and every other frame is the *same
  buffer* in both versions (pointer equality), not a copy.
- [x] **AC4 (always valid):** each operation that would break a TUW3 rule is refused with a typed
  error and the table is unchanged: a non-finite sample (NaN, ±∞), a wrong frame length,
  growing an axis past 4 096, growing past 2²⁸ samples, removing the last row or column,
  metadata over 1 MiB, a position outside the grid.
- [x] **AC5 (rows and columns):** inserting and removing rows and columns at the start, the
  middle and the end (including the last frame on an axis) puts every existing frame at the
  expected (y, z) and shares its buffer; new frames are silent; property test: inserting then
  removing the same row or column gives back an equal table.
- [x] **AC6 (selection):** each way of setting a selection gives the expected rectangle; a
  selection outside the grid is refused; after every insert and remove from AC5 the selection is
  adjusted exactly as described under *Behaviour* (insert before, inside, after; partial and
  full removal, at both ends) and lies inside the grid; fitting any selection to any valid grid
  gives a selection inside it (property tests over random grids, selections and edits).
- [x] **AC7 (threads):** versions and frames are `Send + Sync` (compile-time check).
- [x] **AC8 (crate rules):** `tuw-core` gains no GUI, plugin or file I/O dependency; every public
  item is documented; all `AGENTS.md` commands pass.
- [x] **AC9 (many small frames):** a table with many tiny frames (for example 4 096 × 16 frames
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

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** an always-valid, immutable, cheaply shared wavetable value in `tuw-core`, convertible
to and from `tuw3::Table`, plus a rectangular selection that stays inside the grid.

**Architecture:** `Frame` wraps `Arc<[f32]>` and can only be made by the table, so every frame is
valid by construction. `Wavetable` holds the three sizes, `Arc<Metadata>` and a `Vec<Frame>` in
TUW3 order (index `z × y_size + y`). Every edit takes `&self` and returns a new `Wavetable`,
sharing untouched frames. `Selection` is a separate plain value with insert, remove and fit rules.

**Tech stack:** Rust 2024 (MSRV 1.85), `std::sync::Arc`, `tuw3`; `proptest = "1.11.0"` as a
dev-dependency of `tuw-core` (same version as `tuw3`, already in `Cargo.lock`).

**Spec:** the *Spec* section above.

### Global constraints

- Every new `.rs` file starts with `// SPDX-License-Identifier: GPL-3.0-or-later`.
- `tuw-core` gets no new runtime dependency; no GUI, plugin or file I/O code.
- Every public item has a doc comment (`#![warn(missing_docs)]` is on); doc links to `tuw3`'s
  `write` are written ``[`write()`]`` (`AGENTS.md` gotcha).
- Never compare floats with `==` (workspace `float_cmp` lint): tests compare samples with
  `f32::to_bits`, which is exact and needs no tolerance.
- Limits come from `tuw3::limits` (`MIN_FRAME_SIZE` 8, `MAX_FRAME_SIZE` 65 536, `MIN_AXIS_SIZE` 1,
  `MAX_AXIS_SIZE` 4 096, `MAX_TOTAL_SAMPLES` 2²⁸, `MAX_META_LEN` 1 MiB); never repeat the numbers.
- No `unwrap`/`expect`/`panic!` on anything a caller can trigger; out-of-grid input is an `Err`.
  The one `expect` allowed is in `Wavetable::to_tuw3`, on the invariant, with a comment.
- `cargo test --workspace` (never only `-p`), plus the other `AGENTS.md` commands, before each
  commit.

### Snapshots in tests

A `Wavetable` clone shares every frame, so comparing against a clone proves nothing. Every test
that says a table is "unchanged" takes a deep snapshot first (`let before = t.to_tuw3();`) and
afterwards checks `assert_same(&t.to_tuw3(), &before)`.

### Review focus

1. **Insert at `at == size`** (append) is valid and must not be refused as out of range; `at >
   size` is. Pinned in Task 4.
2. **Batch replacement with a duplicate position** (the same `(y, z)` twice): the last one wins,
   deterministically. Pinned in Task 3.
3. **A failed batch changes nothing**, even when the bad frame is the last one. Pinned in Task 3.
4. **Removing a range that starts before the selection and ends inside it**: the selection keeps
   only its remaining frames and moves left. Pinned in Task 5.
5. **`-0.0` and subnormal samples** survive every path (conversion, replacement, insert,
   remove), compared by bits. Pinned in Tasks 2 and 4.

### Files

- Modify `crates/tuw3/src/metadata.rs`: `Metadata::encoded_len`.
- Create `crates/tuw-core/src/error.rs`: `EditError`, `Axis`.
- Create `crates/tuw-core/src/table.rs`: `Frame`, `Wavetable`.
- Create `crates/tuw-core/src/selection.rs`: `Selection`.
- Modify `crates/tuw-core/src/lib.rs`: modules, re-exports, crate doc.
- Modify `crates/tuw-core/Cargo.toml`: `proptest` dev-dependency.
- Tests: `crates/tuw3/tests/round_trip.rs` (one test), `crates/tuw-core/tests/common/mod.rs`,
  `tests/table.rs`, `tests/edits.rs`, `tests/grid.rs`, `tests/selection.rs`.

---

## Tasks

### Task 1: `tuw3::Metadata::encoded_len`

**Files:** modify `crates/tuw3/src/metadata.rs`; test in `crates/tuw3/tests/round_trip.rs`.

**Interfaces:** produces `pub fn encoded_len(&self) -> usize` on `tuw3::Metadata`: the length in
bytes of the metadata block `write()` produces (0 for empty metadata).

- [x] **Step 1: Write the failing test** `encoded_len_matches_meta_len`: for empty metadata, a
  metadata with only `name = "Saw"`, and one with all six known fields set to non-ASCII text,
  build a 8 × 1 × 1 table, write it, and assert
  `u32::from_le_bytes(bytes[20..24]) as usize == metadata.encoded_len()`. Empty metadata gives 0.
- [x] **Step 2: Run it and see it fail:** `cargo test -p tuw3 --test round_trip encoded_len` —
  fails to compile (no `encoded_len`).
- [x] **Step 3: Implement** `encoded_len` as the length of the existing `to_json()` output, with a
  doc comment that says why a caller needs it (checking `MAX_META_LEN` before building a table).
- [x] **Step 4: Run it and see it pass**, then `cargo test --workspace`.
- [x] **Step 5: Commit** `feat(tuw3): add Metadata::encoded_len (#24)`.

### Task 2: `Wavetable`, `Frame` and conversion (AC1, AC2, AC7, part of AC8)

**Files:** create `error.rs`, `table.rs`; modify `lib.rs`, `Cargo.toml`; tests
`tests/common/mod.rs`, `tests/table.rs`.

**Interfaces (produces):**

```rust
pub enum Axis { Y, Z }                       // Debug, Clone, Copy, PartialEq, Eq
pub enum EditError {                         // Debug, Clone, PartialEq, Eq; Display, Error
    Limits(tuw3::TableError),                // sizes, total samples, metadata length
    FrameLength { expected: u32, actual: usize },
    NonFiniteSample { y: u32, z: u32, x: u32 },
    OutsideGrid { y: u32, z: u32 },
    IndexOutOfRange { axis: Axis, index: u32, size: u32 },
    ZeroCount,
    WouldRemoveAll { axis: Axis },
}
pub struct Frame(Arc<[f32]>);               // Clone (cheap), Debug (length only)
impl Frame {
    pub fn samples(&self) -> &[f32];
    pub fn ptr_eq(a: &Frame, b: &Frame) -> bool;
}
pub struct Wavetable { /* frame_size, y_size, z_size: u32; metadata: Arc<Metadata>; frames: Vec<Frame> */ }
impl Wavetable {
    pub fn silent(frame_size: u32, y_size: u32, z_size: u32) -> Result<Wavetable, EditError>;
    pub fn frame_size(&self) -> u32;  pub fn y_size(&self) -> u32;  pub fn z_size(&self) -> u32;
    pub fn metadata(&self) -> &tuw3::Metadata;
    pub fn frame(&self, y: u32, z: u32) -> Result<&Frame, EditError>;   // OutsideGrid
    pub fn frames(&self) -> impl Iterator<Item = (u32, u32, &Frame)>;   // TUW3 order
    pub fn to_tuw3(&self) -> tuw3::Table;
}
impl From<tuw3::Table> for Wavetable;
```

`Wavetable` derives `Clone` (copies the pointer list, shares frames); `Debug` is written by hand
and shows only the sizes. No `PartialEq` (it would compare floats).

- [x] **Step 1: Test helpers** in `tests/common/mod.rs` (starting with `#![allow(dead_code)]`,
  as in `tuw3`): `finite_f32()` and a `tuw3_table(max_axis: u32)` strategy as in
  `crates/tuw3/tests/properties.rs` but with frame sizes 8..=40 *and* 600 among them, axes
  `1..=max_axis` (Task 2 uses 4, Task 5 uses 6); `assert_same(a: &tuw3::Table, b: &tuw3::Table)` comparing sizes, metadata
  (`==`) and every sample by `to_bits`; `table_with_unknown_keys() -> tuw3::Table`, made by
  `tuw3::read` of hand-assembled bytes (`SPEC.md` *Layout*) with metadata
  `{"name":"x","zz":[1,{"a":null}]}`.
- [x] **Step 2: Write the failing tests** in `tests/table.rs`:
  - `round_trip_is_bit_identical` (proptest, AC1): `assert_same(&Wavetable::from(t.clone()).to_tuw3(), &t)`.
  - `round_trip_keeps_negative_zero_subnormals_and_unknown_keys` (AC1): a 600 × 1 × 1 table with
    `-0.0`, `f32::MIN_POSITIVE / 2.0` and `-f32::MIN_POSITIVE / 2.0` among its samples, and
    `table_with_unknown_keys()`; both round-trip by `assert_same`, and the second's bytes via
    `tuw3::write` equal the original bytes.
  - `silent_table_is_all_zero_and_writable` (AC2): `silent(2048, 3, 2)` has those sizes, every
    sample's bits equal `0.0f32.to_bits()`, and `assert_same(&tuw3::read(&tuw3::write(&t.to_tuw3()))?, &t.to_tuw3())`.
  - `silent_table_refuses_sizes_outside_the_limits` (AC2): each of frame size 7 and 65 537, y 0
    and 4 097, z 0 and 4 097 gives `EditError::Limits(TableError::SizeOutOfRange { field, value, .. })`
    with the matching field and value; `silent(65_536, 4_096, 2)` gives
    `Limits(TableError::TooManySamples { .. })`.
  - `frame_outside_the_grid_is_an_error`: on a 8 × 2 × 3 table, `frame(2, 0)` and `frame(0, 3)`
    give `OutsideGrid`; `frame(1, 2)` is `Ok`.
  - `frames_iterate_in_tuw3_order`: on a table whose sample 0 of frame `(y, z)` is
    `(z * 10 + y + 1) as f32`, `frames()` yields `(y, z)` as (0,0),(1,0),(0,1),(1,1),(0,2),(1,2) with
    matching samples.
  - `types_are_send_and_sync` (AC7): `fn check<T: Send + Sync>() {}` called for `Wavetable`,
    `Frame`, `EditError`.
- [x] **Step 3: Run them and see them fail:** `cargo test -p tuw-core --test table` — fails to
  compile.
- [x] **Step 4: Implement** `error.rs` and `table.rs`. `From<tuw3::Table>` uses `into_parts` and
  `chunks_exact(frame_size)` (one copy); `to_tuw3` joins frames into one `Vec` with exact
  capacity and calls `Table::new(…).expect("a Wavetable always satisfies the TUW3 rules")`;
  a private `check_sizes(frame_size, y_size, z_size) -> Result<(), tuw3::TableError>` checks frame
  size, then Y, then Z (each `SizeOutOfRange` with `min`/`max` from `tuw3::limits`), then the
  total in `u64` (`TooManySamples`), the same order as `tuw3`; `silent` and Task 4 use it. Silent
  frames of one table share one buffer. `lib.rs` re-exports `Axis`,
  `EditError`, `Frame`, `Wavetable` and gains a crate-doc example converting a `tuw3::Table`.
- [x] **Step 5: Run them and see them pass**, then all `AGENTS.md` commands.
- [x] **Step 5b:** `cargo tree -e features -i serde_json`: `arbitrary_precision` and
  `preserve_order` stay off after adding `proptest` (`AGENTS.md` gotcha).
- [x] **Step 6: Commit** `feat(tuw-core): add the Wavetable value and tuw3 conversion (#24)`.

### Task 3: Replacing frames and metadata (AC3, AC4)

**Files:** modify `table.rs`; test `tests/edits.rs`.

**Interfaces:** consumes Task 2. Produces:

```rust
impl Wavetable {
    pub fn with_frame(&self, y: u32, z: u32, samples: Vec<f32>) -> Result<Wavetable, EditError>;
    pub fn with_frames(&self, frames: Vec<(u32, u32, Vec<f32>)>) -> Result<Wavetable, EditError>;
    pub fn with_metadata(&self, metadata: tuw3::Metadata) -> Result<Wavetable, EditError>;
}
```

Checks, in order per frame: position (`OutsideGrid`), length (`FrameLength`), finite
(`NonFiniteSample` with the first bad `x`). `with_frames` checks all entries before building
anything; with a duplicate position the later entry wins. `with_metadata` refuses
`encoded_len() > MAX_META_LEN` as `Limits(TableError::MetadataTooLong { len })`.

- [x] **Step 1: Write the failing tests** in `tests/edits.rs`:
  - `replacing_one_frame_shares_the_others` (AC3): on a random 8 × 3 × 2 table `a`,
    `before = a.to_tuw3()`, `b = a.with_frame(1, 1, new)`: `assert_same(&a.to_tuw3(), &before)`; `b.frame(1, 1)`
    equals `new` by bits; for every other `(y, z)`, `Frame::ptr_eq(a.frame(y,z), b.frame(y,z))`;
    and `!Frame::ptr_eq` at `(1, 1)`.
  - `invalid_frames_are_refused_and_change_nothing` (AC4): NaN at `x = 3` →
    `NonFiniteSample { y: 0, z: 0, x: 3 }`; `+∞` and `-∞` likewise; length 7 on a frame size of 8 →
    `FrameLength { expected: 8, actual: 7 }`; `(3, 0)` on a 3-wide table → `OutsideGrid`. After
    each, the original table is unchanged (bits).
  - `a_failed_batch_changes_nothing`: three valid entries then one with NaN → `Err`, and no new
    table exists; the original is unchanged.
  - `duplicate_positions_in_a_batch_last_wins`: entries `(0,0,A)` then `(0,0,B)` → frame `(0, 0)`
    is `B`.
  - `metadata_over_the_limit_is_refused` (AC4): `description` of `MAX_META_LEN` bytes →
    `Limits(TableError::MetadataTooLong { .. })`; a short name is accepted and survives
    `to_tuw3`; every frame is shared (`ptr_eq`) between the two versions.
- [x] **Step 2: Run and see them fail:** `cargo test -p tuw-core --test edits`.
- [x] **Step 3: Implement** the three methods.
- [x] **Step 4: Run and see them pass**, then all `AGENTS.md` commands.
- [x] **Step 5: Commit** `feat(tuw-core): replace frames and metadata (#24)`.

### Task 4: Inserting and removing rows and columns (AC4, AC5, AC9)

**Files:** modify `table.rs`; test `tests/grid.rs`.

**Interfaces:** consumes Tasks 2–3. Produces:

```rust
impl Wavetable {
    /// Inserts `count` silent rows (Axis::Z) or columns (Axis::Y) so the first new one has index `at`.
    pub fn insert(&self, axis: Axis, at: u32, count: u32) -> Result<Wavetable, EditError>;
    /// Removes `count` rows or columns starting at index `at`.
    pub fn remove(&self, axis: Axis, at: u32, count: u32) -> Result<Wavetable, EditError>;
}
```

A *row* is all Y frames at one Z, so inserting rows grows `z_size` (`Axis::Z`); a *column* is all
Z frames at one Y (`Axis::Y`). Errors: `count == 0` → `ZeroCount`; insert `at > size` or remove
`at + count > size` → `IndexOutOfRange { axis, index: at, size }`; new size above
`MAX_AXIS_SIZE` → `Limits(SizeOutOfRange { field: YSize | ZSize, .. })`; total above
`MAX_TOTAL_SAMPLES` → `Limits(TooManySamples { .. })`; removing every row or column →
`WouldRemoveAll { axis }`. All checks happen in `u64`, so `at + count` and `size + count` cannot overflow:
the new axis size is compared with `MAX_AXIS_SIZE` in `u64` *before* `check_sizes` is called, and
the error reports `value: u32::try_from(new_size).unwrap_or(u32::MAX)`.

- [x] **Step 1: Write the failing tests** in `tests/grid.rs`, on a 8 × 4 × 3 table whose frame
  `(y, z)` is filled with `(z * 10 + y + 1) as f32` (so every frame is identifiable):
  - `inserting_columns_moves_frames` (AC5): `insert(Axis::Y, at, 2)` for `at` in 0, 2, 4 (= size,
    append): new frames silent, every old frame at its expected new `(y, z)` and `ptr_eq` to its
    old buffer.
  - `inserting_rows_moves_frames` (AC5): the same for `Axis::Z` with `at` in 0, 1, 3.
  - `removing_rows_and_columns_moves_frames` (AC5): `remove` of 1 and of 2 at the start, middle
    and end (the last frame on the axis), both axes; remaining frames in place and shared.
  - `insert_then_remove_gives_the_same_table` (AC5, proptest over the Task 2 strategy, an axis,
    `at` in `0..=size` and `count` in 1..=3 within limits): `t.insert(a, at, n)?.remove(a, at, n)?`
    is bit-identical to `t` and shares every frame.
  - `grid_limits_are_refused` (AC4): `insert(Axis::Y, 0, 0)` → `ZeroCount`; `insert(Axis::Y, 5, 1)`
    on a 4-wide table → `IndexOutOfRange`; `remove(Axis::Z, 2, 2)` on 3 rows → `IndexOutOfRange`;
    `remove(Axis::Y, 0, 4)` on 4 columns → `WouldRemoveAll { axis: Axis::Y }`; growing a
    `silent(8, 4_096, 1)` table's Y by 1 → `Limits(SizeOutOfRange { field: YSize, value: 4_097, .. })`;
    growing `silent(65_536, 4_096, 1)` along Z by 1 → `Limits(TooManySamples { .. })`;
    `insert(Axis::Z, u32::MAX, u32::MAX)` → `IndexOutOfRange`; `insert(Axis::Z, 0, u32::MAX)` →
    `Limits(SizeOutOfRange { field: ZSize, value: u32::MAX, .. })`; `remove(Axis::Z, 1, u32::MAX)`
    → `IndexOutOfRange`; none panics. Each leaves the table unchanged (deep snapshot).
  - `many_small_frames_work` (AC9): `silent(8, 4_096, 16)`, `with_frame(4_095, 15, …)`, then
    `to_tuw3()` and back: the changed frame and a sample of others are correct by bits. No
    timing assertion.
- [x] **Step 2: Run and see them fail:** `cargo test -p tuw-core --test grid`.
- [x] **Step 3: Implement** `insert` and `remove` by building the new pointer list in TUW3
  order; new frames share one silent buffer.
- [x] **Step 4: Run and see them pass**, then all `AGENTS.md` commands.
- [x] **Step 5: Commit** `feat(tuw-core): insert and remove rows and columns (#24)`.

### Task 5: `Selection` (AC6)

**Files:** create `selection.rs`; modify `lib.rs`; test `tests/selection.rs`.

**Interfaces:** consumes `Wavetable`, `Axis`, `EditError`. Produces:

```rust
pub struct Selection { /* inclusive y_start..=y_end, z_start..=z_end */ }   // Debug, Clone, Copy, PartialEq, Eq
impl Selection {
    pub fn frame(table: &Wavetable, y: u32, z: u32) -> Result<Selection, EditError>;
    pub fn row(table: &Wavetable, z: u32) -> Result<Selection, EditError>;      // all Y at one Z
    pub fn column(table: &Wavetable, y: u32) -> Result<Selection, EditError>;   // all Z at one Y
    pub fn rect(table: &Wavetable, a: (u32, u32), b: (u32, u32)) -> Result<Selection, EditError>; // corners (y, z), any order
    pub fn all(table: &Wavetable) -> Selection;
    pub fn y_range(&self) -> std::ops::RangeInclusive<u32>;
    pub fn z_range(&self) -> std::ops::RangeInclusive<u32>;
    pub fn contains(&self, y: u32, z: u32) -> bool;
    pub fn after_insert(&self, axis: Axis, at: u32, count: u32, table_after: &Wavetable) -> Selection;
    pub fn after_remove(&self, axis: Axis, at: u32, count: u32, table_after: &Wavetable) -> Selection;
    pub fn fit(&self, table: &Wavetable) -> Selection;
}
```

Rules per axis, with selection `[s, e]` (from the spec):
- insert: `at <= s` → `[s + count, e + count]`; `s < at <= e` → `[s, e + count]`; `at > e` →
  unchanged. Computed in `u64`, then fitted to `table_after`, so inconsistent arguments can't
  overflow or leave the grid.
- remove `[at, at + count - 1]`: the selected indices that remain, each lowered by the number of
  removed indices below it, give `[s', e']`; if none remain, the single index `s` clamped to
  `size_after - 1`.
- fit: `[min(s, size - 1), min(e, size - 1)]`.
Constructors return `IndexOutOfRange` or `OutsideGrid` for positions outside the grid.

- [x] **Step 1: Write the failing tests** in `tests/selection.rs`, on a 8 × 6 × 5 table:
  - `constructors_give_the_expected_rectangle`: `frame(2, 3)` → y 2..=2, z 3..=3; `row(4)` →
    y 0..=5, z 4..=4; `column(1)` → y 1..=1, z 0..=4; `rect((4, 1), (2, 3))` → y 2..=4, z 1..=3;
    `all` → y 0..=5, z 0..=4; `frame(6, 0)` → `OutsideGrid`; `row(5)` →
    `IndexOutOfRange { axis: Axis::Z, index: 5, size: 5 }`.
  - `insert_before_inside_and_after`: with y 2..=4, `after_insert(Axis::Y, at, 2)` for `at` = 0, 2
    → 4..=6; `at` = 3, 4 → 2..=6; `at` = 5 → 2..=4. Z untouched.
  - `remove_partial_and_full`: with y 2..=4 on 6 columns: remove `(0, 3)` (covers 0..=2) → 0..=1;
    remove `(3, 2)` → 2..=2; remove `(4, 2)` → 2..=3; remove `(2, 3)` (all) → 2..=2; remove
    `(1, 5)` (all, leaves 1 column) → 0..=0; remove `(0, 1)` (before) → 1..=3; remove `(5, 1)` (after) →
    2..=4; remove `(3, 1)` (strictly inside) → 2..=3; on Z, with z 1..=3 on 5 rows, remove
    `(0, 1)` → 0..=2.
  - `fit_clamps_into_any_grid`: y 2..=4 fitted to a 3-wide table → 2..=2; to a 1-wide → 0..=0.
  - `adjusted_selections_match_a_reference_model` (proptest): random table
    (`tuw3_table(6)`), random selection, random valid insert or remove on either axis. A reference
    model maps the *set* of selected indices one by one (insert: indices ≥ `at` move by `count`,
    and inserted indices strictly inside the selection join it; remove: removed indices drop out,
    the rest shift down; nothing left: the fallback rule). `after_insert` / `after_remove` equal the
    model's `[min, max]` and lie inside the new table; `fit` of a random selection on a random
    table lies inside it.
- [x] **Step 2: Run and see them fail:** `cargo test -p tuw-core --test selection`.
- [x] **Step 3: Implement** `Selection`; re-export it from `lib.rs`.
- [x] **Step 4: Run and see them pass**, then all `AGENTS.md` commands.
- [x] **Step 5: Commit** `feat(tuw-core): add the rectangular selection (#24)`.

### Task 6: Finish (AC8)

- [x] **Step 1:** check AC8: `cargo tree -p tuw-core -e normal` shows only `tuw3` and its
  dependencies; `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` passes; the crate
  doc in `lib.rs` describes `Wavetable`, `Frame` and `Selection` in a few lines with the version
  idea (edits return new versions, untouched frames are shared).
- [x] **Step 2:** all `AGENTS.md` commands, and `cargo +1.85 check --workspace --all-targets` if
  that toolchain is installed (otherwise say CI checks it).
- [x] **Step 3:** tick AC1–AC9 above, set *Status: Done*, commit
  `docs(tuw-core): mark the table model done (#24)`.

---

## Notes

- Built by the Claude Code loop (D16), November 2026. 90 tests in the workspace at the end.
- Deviations from the plan, each recorded as a ruling in the commits: the branch was rebased onto
  `main` before building; the plan review added overflow and selection-model tests; the one
  refusal test on a 2²⁸-sample table checks sizes instead of a 1 GiB deep snapshot, because
  versions cannot change (edits take `&self`).
- Open for later: `with_frame` / `with_frames` take `Vec<f32>`; a slice would spare callers a copy.
  Worth deciding before #31 adds callers.
