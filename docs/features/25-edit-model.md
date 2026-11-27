# Edit model with undo and redo

- **Issue:** #25
- **Status:** Planned
- **Crates:** tuw-core
- **Format change:** no

---

## Spec

### Problem

Every Phase 1 tool changes the open table: shapes (#31), harmonics (#32), drawing (#33), single-cycle
import (#34), grid editing (#35), fills (#36), frame operations (#37, #38) and metadata (#30). Each of
those changes must be one undoable step, and undo and redo must behave the same whatever tool made
the step (`23-phase-1-scope.md`, *Undo / redo*). Opening, saving and playing are not undoable.

Many of these changes are **gestures**: a freehand stroke or a gain drag changes the table many
times before the mouse is released. The screen must follow the gesture live, yet the whole gesture
must be one undo step, and the preview must hear it only once it is finished (*Preview* in the scope
doc: edits are heard "after each finished step … not during the gesture").

#24 built the table as immutable versions that share unchanged frames, and left one job for this
issue: the type that holds "current version + selection" together. Nothing in `tuw-core` yet
records history, groups a gesture into one step, or knows whether the table has unsaved changes.

This is the second of the three Phase 1 foundations (`23-phase-1-scope.md`). Every tool issue
builds on it, so the behaviour here sets how all of them feel.

### Behaviour

**The document**

- One type (the builder names it; "document" below) holds what the editor works on: the **current
  version** of the table (#24's `Wavetable`), the **selection** (#24's `Selection`), the **undo
  history**, the **open step** if there is one, and the **saved marker**.
- A document is created from a table (a new silent one, or one just opened) and a **history limit**
  (see below). It starts with empty history, no open step, and is *not modified*: opening a file is
  never an undo step.
- The selection starts as the whole table (`Selection::all`) unless the caller gives one.

**Steps: every change goes through one**

- A **step** is one undoable change. A tool makes one in three calls: **begin** the step with a
  short label ("Draw", "Gain", "Insert rows"), send **updates**, then **finish** it or **cancel** it.
- An **update** is a function the tool hands the document: *given the table as it was when the step
  began, return the new table*, using #24's edit functions (`with_frame`, `with_frames`, `insert`,
  `remove`, `with_metadata`). This function is the "command" of the issue text.
  - Every update starts from the **step's starting version**, never from the previous update, so
    each update *replaces* the last one. Example: a gain drag through 1.3, 1.5 and back to 1.2 ends
    at gain 1.2 applied once, not 1.3 × 1.5 × 1.2.
  - After a successful update, the current version is its result: everything that reads the
    document (the view, the harmonics chart, warnings) sees the gesture live.
  - If an update returns an error (for example a non-finite sample, #24's `EditError`), the error is
    returned to the tool, the current version stays at the last good update (or the start), and the
    step stays open.
- **Finish** records one undo step, however many updates the step had, and makes its result the
  *finished version* (below). A step finished without any successful update records nothing,
  and so does a step whose result is the very same version it started from (same sizes, every
  frame the same buffer, the same metadata; checked by pointer, never by comparing samples). A tool
  that knows an update changes nothing returns the starting version unchanged.
- **Cancel** puts the current version back to the step's starting version (the very same version,
  frames shared, not a copy) and records nothing.
- A **one-shot** call does begin, one update and finish together, for edits that are not gestures
  (insert a shape, clear a frame). If its update fails, nothing changes and nothing is recorded.
- While a step is open, **undo, redo and beginning another step are refused** with a typed error and
  change nothing: the tool must finish or cancel first.

**Undo and redo**

- **Snapshots** (approach 1, chosen): each history entry holds a whole earlier version of the table
  together with its step's label. Versions share every frame their step didn't change (#24), so an
  entry costs only the frames its step changed plus one list of frame pointers (16 bytes per frame
  in the grid). Undo makes the
  version before the step current again; redo makes the step's own result current again. Both are
  exact, bit for bit, because they restore the very version that was there.
- **Linear history.** Finishing a new step after one or more undos discards the steps that could
  have been redone, as in most editors.
- **Labels.** The document tells the editor the label of the step undo would revert and the step
  redo would reapply (for "Undo Draw" / "Redo Draw"), or that there is none.
- **History limit.** The caller sets the maximum number of undo steps when creating the document;
  `tuw-core` offers a documented default (the builder picks the number, around 100–200). When a new
  step would exceed the limit, the oldest step is dropped and can no longer be undone.

**The selection is not history**

- Selecting is not a step and is never undone (#24). The document's selection can be set at any
  time, including while a step is open.
- Whenever the grid of the current version changes (an update, finish, cancel, undo or redo), the
  document **fits** the selection into the grid (#24's `Selection::fit`), so the selection always
  lies inside the grid.
- The precise insert and remove rules (#24's `after_insert` / `after_remove`) need to know what
  the step did, which only the tool knows. A tool that inserts or removes rows or columns (#35)
  sets the adjusted selection itself; fitting is only the safety net.

**The finished version (for the preview)**

- The document exposes the **finished version**: the table as of the last finished step, undo or
  redo. During an open step it stays at the step's starting version; a successful finish, an undo
  and a redo change it; a cancel, a step with no update, and selection changes don't.
- It comes with a **change id** that gets a new value each time the finished version changes, so
  the preview (#26) can see cheaply that there is something new to hear. Ids are unique across all
  documents in the process (a new document gets a fresh one), so replacing the document, for
  example by opening a file (#28), is also seen as a change. How the version reaches the audio
  thread is #26's job.

**Size and speed**

- The same promise as #24: correct up to the TUW3 limits, fast up to 65 536 frames. Each history
  entry and each update of an open step builds one list of frame pointers, 16 bytes per frame in
  the grid: 4 KiB for a 256-frame table, 1 MiB at 65 536 frames (so 100 such entries hold about
  100 MiB of pointers). Bigger grids work but cost more per step; grouping pointers per row (#24,
  *Size and speed*) is the fix if it ever matters.

**Unsaved changes**

- `mark_saved` records the finished version as the saved one; the editor calls it after a
  successful save (#28). *Modified* means: the finished version is not the saved one.
- A new document is not modified. After a step it is modified; undoing back to the saved version
  makes it unmodified again; redoing away makes it modified again.
- The saved version is identified by its place in the history, never by comparing samples.
- If the saved version falls out of the history (the history limit), no undo or redo can reach it
  any more, so the document stays modified until the next `mark_saved`.
- An open step doesn't count: modified and `mark_saved` both refer to the finished version.

### Acceptance criteria

- [ ] **AC1 (undo and redo are exact):** property test over random sequences of steps (frame
  replacements, inserts, removes, metadata) mixed with undos and redos, checked against a simple
  reference model (a list of deep snapshots, `to_tuw3()`): after every operation the current version
  equals the reference bit for bit, and undo or redo with nothing to undo or redo is refused (typed
  error, nothing changes).
- [ ] **AC2 (updates start from the step's start):** in one step, updates applying gain 1.3, 1.5 and
  1.2 to the starting frame (each computed from the starting version the document passes in) leave
  the current version equal, bit for bit, to the starting version with gain 1.2 applied once; after
  each update the current version is that update's result.
- [ ] **AC3 (one step per gesture):** a step with many updates, once finished, is undone by a
  single undo back to the starting version, and redone by a single redo; the history grows by one.
- [ ] **AC4 (cancel and empty steps):** cancel makes the starting version current again (every
  frame the same buffer, `Frame::ptr_eq`) and records nothing; a step finished without a successful
  update records nothing, and so does a step whose result is the starting version itself;
  none of these changes the finished version or the change id, or makes the document modified.
- [ ] **AC5 (failed updates):** an update that returns an error leaves the current version at the
  last good update (or the start), returns the error, and keeps the step open; a later good update
  and finish still work. A one-shot call whose update fails changes nothing and records nothing.
- [ ] **AC6 (open step guards):** while a step is open, undo, redo and beginning another step are
  each refused with a typed error, and the document is unchanged.
- [ ] **AC7 (linear history):** finishing a new step after undos discards the redo steps: redo is
  then refused.
- [ ] **AC8 (history limit):** with limit `n`, after `n + k` finished steps exactly `n` undos are
  possible and the oldest `k` steps are gone; the first step can no longer be undone once dropped.
  Tested with small limits, including 1.
- [ ] **AC9 (labels):** after finishing steps labelled "A" then "B", the undo label is "B"; after
  one undo the undo label is "A" and the redo label is "B"; with empty history there is none.
- [ ] **AC10 (selection):** after every update, finish, cancel, undo and redo that changes the grid
  size, the selection lies inside the grid (property test over random steps and selections); a
  selection set by the caller stays as set when the grid doesn't change; selection changes never add
  history, never change the finished version and never make the document modified.
- [ ] **AC11 (finished version and change id):** during an open step the finished version is the
  starting version (same buffers) and the change id stays the same; finish, undo and redo each give
  a new change id and make the finished version the new current version; two documents never share
  a change id, including two new documents made one after the other.
- [ ] **AC12 (unsaved changes):** a new document is not modified; the sequence step → modified,
  undo → not modified, redo → modified, `mark_saved` → not modified, undo → modified holds; a
  saved version dropped by the history limit leaves the document modified until `mark_saved`.
- [ ] **AC13 (crate rules):** `tuw-core` gains no dependency (no GUI, plugin or file I/O); every
  public item is documented; all `AGENTS.md` commands pass.

### Out of scope

- The edits themselves (each tool issue, #30–#39, adds its own steps) and any UI: menus, keyboard
  shortcuts, the history display.
- Handing the finished version to the audio thread, and freeing old versions off it (#26).
- Opening and saving files (#28); this issue only offers `mark_saved` and *modified*.
- A memory budget for history (maintainer: a fixed number of steps; a budget can come later if real
  use asks for it).
- Branching history, merging separate steps into one after the fact, and keeping history across
  sessions or in files.
- Several open documents.

### Open questions

Each settled in this session, with the maintainer unless marked as my call:

- *Where are gestures handled?* In `tuw-core`, as an open step that tools update and then finish
  or cancel (maintainer: option B, "similar behaviour across all components"). The rejected option
  was each tool keeping its own scratch copy and sending one edit at the end.
- *How is history stored?* Snapshots of whole versions (maintainer, "the one people expect to
  happen"). See *Approaches*.
- *How much history?* A fixed number of steps, set by the caller, with a default (maintainer).
- *Unsaved-changes tracking here or in #28?* Here (maintainer).
- *Does undo restore an old selection?* No; the selection is not history and is fitted into the
  grid (#24, maintainer).

### Approaches

- **1. Snapshots: each entry is a whole earlier version.** *Chosen.* Undo makes that version
  current again. Versions share unchanged frames (#24), so an entry costs only its step's changed
  frames plus one pointer list (about 4 KiB for 256 frames, plus 8 KiB per changed 2048-sample
  frame). No tool writes undo code, and undo is exact because it restores what was there.
- **2. Inverse commands: each step knows how to reverse itself.** The classic approach, and lean on
  memory when there is no sharing. Here it loses on every count: every tool must write and test a
  reverse, floating-point reversals don't give back the exact samples (gain 1.5 then 1/1.5), which
  breaks D14, and some steps (normalise, clear) can't be reversed without storing the old frames
  anyway.
- **3. Deltas: each entry stores only the changed frames and where they were.** Saves the pointer
  list per entry, which #24's sharing already makes small, at the cost of a second description of
  every change kept in step with #24's edit functions, plus bookkeeping for inserts and removes.

### Calls made

Not discussed with the maintainer; check while reading:

- **One document type** owns version, selection, history, open step and saved marker.
- **Refusing undo, redo and a second begin while a step is open** (typed error) rather than
  finishing or cancelling the open step silently.
- **A failed update keeps the step open** on its last good state, so a tool can carry on (the next
  mouse move may be valid again); the tool can still cancel.
- **The document only fits the selection**; precise adjustment after inserts and removes is the
  tool's job, because a snapshot step doesn't say what it did.
- **The finished version and a change id** as the hook for #26, defined now so #26 doesn't
  need to change this design. The id is unique across documents, so opening a file is seen as a
  change without extra signalling (builder check).
- **A step whose result is its starting version records nothing**, checked by pointer identity,
  never by comparing samples (builder check).
- **Large grids keep #24's promise** (fast up to 65 536 frames) rather than reopening #24's
  storage for history (builder check).
- **The default selection** of a new document is the whole table.

### What is left to the builder

Type, module and method names; the error type for refused document operations (a new enum or
variants added to `EditError`); the default history limit and whether a limit of 0 is refused or
means "no history"; how the update function is passed (a closure, a trait); how history entries
and the saved marker are stored; whether labels are `&'static str` or `String`.

---

## Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** a `Document` in `tuw-core` that records every change to a `Wavetable` as one undoable
step, with open steps for gestures, linear snapshot undo and redo, a history limit, a selection
that always fits the grid, a finished version with a change id, and unsaved-changes tracking.

**Architecture:** the document keeps a list of *states* (a `Wavetable` version, the label of the
step that made it, and a serial number) and a cursor on the current one; undo and redo move the
cursor. An open step holds its label and its current result; its starting version is the state
under the cursor, so cancel just drops the open step. Every update calls the tool's closure with
that starting version. Change ids come from one process-wide atomic counter, so two documents
never share one.

**Tech stack:** Rust 2024 (MSRV 1.85), `std` only (`std::collections::VecDeque`,
`std::sync::atomic::AtomicU64`); `proptest` (already a dev-dependency) for property tests.

**Spec:** the *Spec* section above.

### Global constraints

- Every new `.rs` file starts with `// SPDX-License-Identifier: GPL-3.0-or-later`.
- `tuw-core` gets no new dependency; no GUI, plugin or file I/O code.
- Every public item has a doc comment (`#![warn(missing_docs)]`).
- Never compare floats with `==`: tests compare samples by `f32::to_bits` (exact, no tolerance
  needed) or tables with `assert_same` on deep snapshots (`to_tuw3()`).
- A clone shares frames, so "unchanged" is shown with a deep snapshot, never against a clone
  (`AGENTS.md` gotcha); "the very same version" is shown with `Wavetable::ptr_eq`.
- No `unwrap`/`expect`/`panic!` on anything a caller can trigger: every refusal is an `Err`.
- The document is UI-thread code (it allocates); its doc comment says so. Nothing in it is
  reachable from an audio callback.
- `cargo test --workspace` (never only `-p`), plus the other `AGENTS.md` commands, before each
  commit.

### Interfaces (all tasks)

In `crates/tuw-core/src/table.rs`:

```rust
impl Wavetable {
    /// Same sizes, the same metadata allocation and every frame the same buffer.
    pub fn ptr_eq(a: &Wavetable, b: &Wavetable) -> bool;
}
```

In `crates/tuw-core/src/error.rs`, a new enum next to `EditError`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentError {
    StepOpen,      // undo, redo, begin_step or apply while a step is open
    NoOpenStep,    // update_step, finish_step or cancel_step with no open step
    NothingToUndo,
    NothingToRedo,
    Edit(EditError), // an update's closure refused the edit
}
```

with `Display` and `std::error::Error` (`source` returns the `EditError`).

In a new `crates/tuw-core/src/document.rs`:

```rust
pub const DEFAULT_HISTORY_LIMIT: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChangeId(u64); // fresh values from one process-wide AtomicU64

pub struct Document { /* private */ }

impl Document {
    pub fn new(table: Wavetable, history_limit: usize) -> Document;
    pub fn current(&self) -> &Wavetable;   // the open step's result, else the finished version
    pub fn finished(&self) -> &Wavetable;  // the state under the cursor
    pub fn change_id(&self) -> ChangeId;
    pub fn selection(&self) -> Selection;
    pub fn set_selection(&mut self, selection: Selection); // stored fitted to current()
    pub fn history_limit(&self) -> usize;
    pub fn is_step_open(&self) -> bool;
    pub fn begin_step(&mut self, label: impl Into<String>) -> Result<(), DocumentError>;
    pub fn update_step<F>(&mut self, edit: F) -> Result<(), DocumentError>
    where
        F: FnOnce(&Wavetable) -> Result<Wavetable, EditError>;
    pub fn finish_step(&mut self) -> Result<(), DocumentError>;
    pub fn cancel_step(&mut self) -> Result<(), DocumentError>;
    pub fn apply<F>(&mut self, label: impl Into<String>, edit: F) -> Result<(), DocumentError>
    where
        F: FnOnce(&Wavetable) -> Result<Wavetable, EditError>;
    pub fn undo(&mut self) -> Result<(), DocumentError>;
    pub fn redo(&mut self) -> Result<(), DocumentError>;
    pub fn undo_label(&self) -> Option<&str>;
    pub fn redo_label(&self) -> Option<&str>;
    pub fn is_modified(&self) -> bool;
    pub fn mark_saved(&mut self);
}
```

`lib.rs` re-exports `ChangeId`, `DEFAULT_HISTORY_LIMIT`, `Document` and `DocumentError`.

Semantics the signatures don't show:

- **History limit `n`:** at most `n` undo steps, so at most `n + 1` states. `0` means no undo
  history: every finished step is recorded and at once becomes the only state.
- **`finish_step`:** if the open step's result is `ptr_eq` to its starting version (no
  successful update, or an update that returned the start), it records nothing and leaves the
  change id alone. Otherwise it drops the states after the cursor (redo steps), pushes the new
  state, drops the oldest state while `states.len() - 1 > n` (never `n + 1`: `n` may be
  `usize::MAX`), and takes a fresh change id.
- **`apply`:** refuses with `StepOpen` while a step is open; otherwise runs one update and
  finishes. If the update fails, the step is cancelled, so nothing changes and nothing is
  recorded, and the error is returned.
- **Labels:** `undo_label` is `None` when the cursor is on the oldest state left, otherwise the
  label of the state under the cursor; `redo_label` is the label of the state after the cursor,
  or `None`. (After the limit drops states, the oldest state left still carries a label; it must
  not be shown, since `undo()` refuses.)
- **Saved marker:** a plain `u64`. `mark_saved` stores the serial of the state under the cursor;
  `is_modified` is `serial(cursor) != saved`. Serials are never reused, so a saved state dropped
  by the limit leaves the document modified until the next `mark_saved`.
- **Selection:** after every change to `current()` (a successful update, cancel, finish, undo,
  redo) the stored selection becomes `selection.fit(current())`. Added in Task 4 only, so its
  property test is seen failing first; Tasks 2 and 3 don't touch the selection.
- **Starting selection:** `Selection::all`; a caller that wants another calls `set_selection`
  right after `new` (said in `new`'s doc comment).

### Review focus

1. **An update after a failed update** still starts from the step's starting version, not from
   the last good update. Pinned in Task 2 (`failed_update_keeps_the_step_open`).
2. **Undo right after a step that was dropped by a limit of 1**: one undo works, the second is
   `NothingToUndo`. Pinned in Task 3 (`history_limit_drops_oldest`).
3. **`apply` with a failing closure while there are redo steps**: the redo steps survive. Pinned
   in Task 2 (`failed_apply_changes_nothing`).
4. **Cancel after an update that removed rows**: the selection fits the restored grid again
   without moving (the restored grid is bigger, so `fit` keeps it). Pinned in Task 4
   (`selection_always_fits`).
5. **The saved state on a discarded redo branch**: after step A, `mark_saved`, undo and a new
   step B, no state counts as saved any more. Pinned in Task 4 (`saved_state_is_by_serial`).

### Files

- Modify `crates/tuw-core/src/table.rs`: `Wavetable::ptr_eq`.
- Modify `crates/tuw-core/src/error.rs`: `DocumentError`.
- Create `crates/tuw-core/src/document.rs`: `Document`, `ChangeId`, `DEFAULT_HISTORY_LIMIT`.
- Modify `crates/tuw-core/src/lib.rs`: module, re-exports, an *Edit model* section in the crate
  doc with a doctest.
- Tests: `crates/tuw-core/tests/document.rs` (steps, selection, saved marker, change id) and
  `crates/tuw-core/tests/history.rs` (undo, redo, limit, the reference-model property test).

---

## Tasks

### Task 1: the document's skeleton

**Files:** modify `table.rs`, `error.rs`, `lib.rs`; create `document.rs`; test `tests/document.rs`.

**Interfaces:** produces `Wavetable::ptr_eq`, `DocumentError`, `ChangeId`,
`DEFAULT_HISTORY_LIMIT`, and `Document::{new, current, finished, change_id, selection,
set_selection, history_limit, is_step_open, undo_label, redo_label, is_modified}` as above.

- [ ] **Step 1: Write the failing tests** in `tests/document.rs`:
  - `ptr_eq_means_same_buffers`: `t.clone()` is `ptr_eq` to `t`; `t.with_frame(0, 0, &[0.5; 8])`
    is not; two separately built `Wavetable::silent(8, 2, 2)` are not; a table and itself after
    `with_metadata(t.metadata().clone())` is not (new metadata allocation).
  - `a_new_document` (AC11, AC12 in part): `Document::new(t, 10)`: `current()` and `finished()`
    are `ptr_eq` to `t`; `selection() == Selection::all(&t)`; `!is_step_open()`;
    `undo_label()` and `redo_label()` are `None`; `!is_modified()`; `history_limit() == 10`.
  - `change_ids_are_unique_across_documents` (AC11): two `Document::new` calls on the same table
    have different `change_id()`s.
  - `set_selection_fits` (AC10): on a 3 × 2 table, setting `Selection::all` of a 5 × 5 table
    stores `Selection::all` of the 3 × 2 table; setting `Selection::frame(&t, 1, 1)` keeps it.
- [ ] **Step 2: Run** `cargo test --workspace`: fails to compile (no `Document`).
- [ ] **Step 3: Implement** the interfaces of this task, plus `impl From<EditError> for
  DocumentError` (into `Edit`). One state, cursor 0, serial 0 and `saved = 0`; the change id from a `static NEXT_ID: AtomicU64` (`fetch_add(1, Relaxed)`).
- [ ] **Step 4: Run** `cargo test --workspace`: passes.
- [ ] **Step 5: Commit** `feat(tuw-core): add the document skeleton (#25)`.

### Task 2: open steps (begin, update, finish, cancel, apply)

**Files:** modify `document.rs`; test `tests/document.rs`.

**Interfaces:** consumes Task 1. Produces `begin_step`, `update_step`, `finish_step`,
`cancel_step`, `apply` as above; `finish_step` records a state (history list and redo dropping as
in *Semantics*; the limit itself is Task 3).

- [ ] **Step 1: Write the failing tests:**
  - `updates_start_from_the_start` (AC2): table with frame (0, 0) = `[0.1, 0.2, …, 0.8]`;
    `begin_step("Gain")`, then three updates each `|s| s.with_frame(0, 0, &gain(s, g))` with `g`
    = 1.3, 1.5, 1.2, where `gain` multiplies the starting frame's samples by `g`; after each, the
    bits of `current().frame(0, 0)` equal the starting samples × `g` (computed the same way, so
    bit-exact). Each closure asserts its argument is `ptr_eq` to the starting table.
  - `a_gesture_is_one_step` (AC3): 50 updates then `finish_step()`: `undo_label() == Some("Gain")`,
    `!is_step_open()`, and `finished()` and `current()` are both `ptr_eq` to the 50th update's
    result (keep a clone of `current()` after it). The single-undo half of AC3 is in Task 3.
  - `cancel_restores_the_start` (AC4): after updates, `cancel_step()` makes `current()` `ptr_eq` to
    the starting table; `finished()` is `ptr_eq` to it too; `undo_label()` stays `None`;
    `change_id()` unchanged; `!is_modified()`.
  - `empty_and_no_op_steps_record_nothing` (AC4): begin + finish; begin + an update returning
    `Ok(s.clone())` + finish: both leave `undo_label() == None`, `change_id()` unchanged,
    `finished()` `ptr_eq` to the start, `!is_modified()`.
  - `failed_update_keeps_the_step_open` (AC5, review focus 1): a failing *first* update leaves
    `current()` `ptr_eq` to the start with the step open; then a good update (gain 2), a failing one
    (a NaN sample) returns `Err(DocumentError::Edit(EditError::NonFiniteSample { .. }))`, then
    `current()` is still the gain-2 result and `is_step_open()`; a following good update's
    closure still receives the starting table (`ptr_eq`); finishing records one step.
  - `failed_apply_changes_nothing` (AC5): on a fresh document, a failing `apply` returns the
    error, `!is_step_open()`, `current()` is `ptr_eq` to the original, `undo_label() == None`.
    (With redo steps present: Task 3, `failed_apply_keeps_redo`.)
  - `open_step_guards` (AC6): with a step open, `begin_step` and `apply` return
    `Err(DocumentError::StepOpen)` and the open step is untouched (same `current()`);
    without one, `update_step`, `finish_step` and `cancel_step` return `Err(NoOpenStep)`.
- [ ] **Step 2: Run** `cargo test --workspace`: the new tests fail.
- [ ] **Step 3: Implement** the five methods per *Semantics*.
- [ ] **Step 4: Run** `cargo test --workspace`: passes.
- [ ] **Step 5: Commit** `feat(tuw-core): record changes as steps (#25)`.

### Task 3: undo, redo and the history limit

**Files:** modify `document.rs`; test `tests/history.rs`.

**Interfaces:** consumes Tasks 1–2. Produces `undo`, `redo` and the limit as in *Semantics*.

- [ ] **Step 1: Write the failing tests** in `tests/history.rs`:
  - `undo_and_redo_match_a_reference` (AC1): proptest over a `tuw3_table(3)` and 1–40 operations
    drawn from: replace frame `(y % y_size, z % z_size)` with random finite samples; insert 1–2
    rows or columns at `at % (size + 1)`; remove one row or column at `at % size` (skipped when
    the axis has one frame); set `metadata.name`; undo; redo. Every edit goes through `apply`
    and always makes a new version (new samples, a new metadata allocation), so the reference
    can push on every edit. The reference model is a `Vec<tuw3::Table>` of deep snapshots plus a
    cursor, with linear history; the document uses `DEFAULT_HISTORY_LIMIT` (200 > 40 operations,
    so nothing is dropped). After each operation, `assert_same(&doc.current().to_tuw3(),
    &reference[cursor])`; an undo or redo the reference can't do must return `NothingToUndo` /
    `NothingToRedo` and change nothing.
  - `a_gesture_undoes_in_one` (AC3): 50 updates, finish, one `undo()` gives `current()` `ptr_eq`
    to the start; one `redo()` gives the finished result back (`ptr_eq`); after that redo, one
    `undo()` succeeds and the next returns `NothingToUndo`.
  - `undo_and_redo_refused_while_open` (AC6): with a step open, `undo()` and `redo()` return
    `StepOpen` and `current()` is unchanged.
  - `a_new_step_drops_redo` (AC7): steps A, B, undo, step C: `redo()` returns `NothingToRedo`,
    `undo_label() == Some("C")`.
  - `history_limit_drops_oldest` (AC8, review focus 2): for limits 0, 1 and 3, finish `limit + 2`
    steps; exactly `limit` undos succeed and the next returns `NothingToUndo`; the oldest
    reachable version is the one after step 2 (deep snapshot taken when it was made; for limit 0
    it is the only state left), and then `undo_label() == None`.
  - `labels_follow_the_cursor` (AC9): steps "A", "B": undo label "B", redo `None`; after undo:
    undo "A", redo "B"; after another undo: undo `None`, redo "A".
  - `failed_apply_keeps_redo` (AC5, review focus 3): steps A, B, undo, failing `apply`: `redo()`
    still succeeds.
- [ ] **Step 2: Run** `cargo test --workspace`: the new tests fail.
- [ ] **Step 3: Implement** `undo`, `redo` (move the cursor, take a fresh change id) and the limit
  (drop from the front of a `VecDeque`, moving the cursor with it).
- [ ] **Step 4: Run** `cargo test --workspace`: passes.
- [ ] **Step 5: Commit** `feat(tuw-core): add undo, redo and the history limit (#25)`.

### Task 4: selection, finished version and unsaved changes

**Files:** modify `document.rs`; test `tests/document.rs`.

**Interfaces:** consumes Tasks 1–3. Produces `mark_saved`, `is_modified` (beyond the new-document
case) and selection fitting on every change of `current()`.

- [ ] **Step 1: Write the failing tests:**
  - `selection_always_fits` (AC10, review focus 4): proptest over a `tuw3_table(4)`, a random
    selection and 1–20 operations (updates that insert or remove rows and columns, finish, cancel,
    undo, redo, `set_selection`): after each, `selection()` lies inside `current()`'s grid
    (`y_range().end() < y_size`, same for Z). Plus a fixed case: select frame (1, 1) on a 3 × 3
    table, a step that only replaces frames: the selection is still exactly (1, 1).
  - `selection_is_not_history` (AC10): `set_selection` leaves `undo_label()`, `change_id()`,
    `finished()` (`ptr_eq`) and `is_modified()` unchanged; undo doesn't restore an earlier
    selection.
  - `finished_version_and_change_id` (AC11): during an open step with updates, `finished()` is
    `ptr_eq` to the start and `change_id()` doesn't change; `finish_step`, `undo` and `redo` each
    give a `change_id()` different from all earlier ones (collect them in a `HashSet`) and leave
    `finished()` `ptr_eq` to `current()`.
  - `modified_follows_the_saved_state` (AC12): new → not modified; step → modified; undo → not;
    redo → modified; `mark_saved` → not; undo → modified; and with a step open, `is_modified()`
    reports the finished version (open step after `mark_saved`: not modified).
  - `saved_state_dropped_by_the_limit` (AC12): limit 1, `mark_saved` on the new document, two
    steps, undo once: modified (the saved state is gone), and stays so until `mark_saved`.
  - `saved_state_is_by_serial` (review focus 5): step A, `mark_saved`, undo, step B: modified
    (B is not A); undo: modified (the state before A was never saved); redo: modified (B again,
    and A can't be reached any more).
- [ ] **Step 2: Run** `cargo test --workspace`: the new tests fail. First give `mark_saved` an
  empty body so the file compiles, and check that `selection_always_fits` itself fails.
- [ ] **Step 3: Implement** the saved marker and the fit after every change of `current()`.
- [ ] **Step 4: Run** `cargo test --workspace`: passes.
- [ ] **Step 5: Commit** `feat(tuw-core): fit the selection and track unsaved changes (#25)`.

### Task 5: crate docs

**Files:** modify `lib.rs`.

- [ ] **Step 1:** add an *Edit model* section to the crate doc (AC13): the document, steps
  (begin, update from the start, finish or cancel; `apply` for one-shot edits), snapshot undo,
  the finished version and change id for the preview, and that the document is UI-thread code.
  Add a doctest: a document on a silent table, a gesture of two gain updates, finish, undo, with
  `ptr_eq` assertions (no float `==`); it returns `Result<(), Box<dyn std::error::Error>>`
  because it uses both `EditError` and `DocumentError`.
- [ ] **Step 2: Run** the four `AGENTS.md` commands: all pass, no `missing_docs` warning.
- [ ] **Step 3: Commit** `docs(tuw-core): describe the edit model in the crate docs (#25)`.

---

## Notes

-
