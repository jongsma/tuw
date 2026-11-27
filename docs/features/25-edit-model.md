# Edit model with undo and redo

- **Issue:** #25
- **Status:** Clarifying (spec approved; plan to be written by the loop)
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

Written by the loop (or with the maintainer), after this spec is approved.

---

## Tasks

- [ ] T1 …

---

## Notes

-
