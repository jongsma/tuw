// SPDX-License-Identifier: GPL-3.0-or-later

//! The document: a wavetable with a selection and an undo/redo history.
//!
//! A [`Document`] is UI-thread code: it allocates and must never be touched from an audio
//! callback. Hand the audio side a clone of [`Document::finished`] instead; it changes only
//! when [`Document::change_id`] does.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{DocumentError, EditError, Selection, Wavetable};

/// A sensible history limit to pass to [`Document::new`]: 200 undo steps.
pub const DEFAULT_HISTORY_LIMIT: usize = 200;

/// Identifies one version of a document's content. It changes whenever the finished version
/// changes (a recorded step, undo, redo), never while a step is open, and is unique across all documents in the process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChangeId(u64);

impl ChangeId {
    /// A value no earlier call returned, from one process-wide counter.
    fn fresh() -> ChangeId {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        ChangeId(NEXT_ID.fetch_add(1, Ordering::Relaxed))
    }
}

/// One recorded version of the table.
struct State {
    version: Wavetable,
    /// What the step that produced this version was called; `None` for the first state.
    label: Option<String>,
    /// Never reused, so "saved" can be tracked by value.
    serial: u64,
}

/// A step that has begun and not yet finished or cancelled.
struct OpenStep {
    label: String,
    /// The result of the last successful update; the starting version until there is one.
    result: Wavetable,
}

/// A wavetable being edited: the current version, a selection and the history of versions.
///
/// This is UI-thread code (it allocates); nothing in it is reachable from an audio callback.
pub struct Document {
    /// Oldest first; never empty.
    states: VecDeque<State>,
    /// Index into `states` of the finished version.
    cursor: usize,
    history_limit: usize,
    change_id: ChangeId,
    selection: Selection,
    /// Serial of the state last saved.
    saved: u64,
    /// The serial the next recorded state gets.
    next_serial: u64,
    open: Option<OpenStep>,
}

impl Document {
    /// A document holding `table` as its only state, with room for `history_limit` undo steps.
    /// The selection starts as [`Selection::all`]; call [`set_selection`](Self::set_selection)
    /// right after for another one.
    pub fn new(table: Wavetable, history_limit: usize) -> Document {
        let selection = Selection::all(&table);
        let mut states = VecDeque::new();
        states.push_back(State {
            version: table,
            label: None,
            serial: 0,
        });
        Document {
            states,
            cursor: 0,
            history_limit,
            change_id: ChangeId::fresh(),
            selection,
            saved: 0,
            next_serial: 1,
            open: None,
        }
    }

    /// The version to show and edit: the open step's result if there is one, else the finished
    /// version.
    pub fn current(&self) -> &Wavetable {
        match &self.open {
            Some(step) => &step.result,
            None => self.finished(),
        }
    }

    /// The finished version: the state under the history cursor.
    pub fn finished(&self) -> &Wavetable {
        &self.states[self.cursor].version
    }

    /// Identifies the content of [`finished`](Self::finished); a new value after every recorded
    /// change, undo and redo.
    pub fn change_id(&self) -> ChangeId {
        self.change_id
    }

    /// The selection, always fitted to [`current`](Self::current).
    pub fn selection(&self) -> Selection {
        self.selection
    }

    /// Stores `selection`, fitted to [`current`](Self::current) with [`Selection::fit`].
    /// Selecting is not undoable.
    pub fn set_selection(&mut self, selection: Selection) {
        self.selection = selection.fit(self.current());
    }

    /// The most undo steps kept.
    pub fn history_limit(&self) -> usize {
        self.history_limit
    }

    /// Whether a step is open (see the step methods).
    pub fn is_step_open(&self) -> bool {
        self.open.is_some()
    }

    /// Opens a step called `label`. Until it is finished or cancelled, [`update_step`] replaces
    /// [`current`](Self::current) while [`finished`](Self::finished) stays as it was. The step
    /// starts from the finished version.
    ///
    /// [`update_step`]: Self::update_step
    ///
    /// # Errors
    ///
    /// [`DocumentError::StepOpen`] when a step is already open.
    pub fn begin_step(&mut self, label: impl Into<String>) -> Result<(), DocumentError> {
        if self.open.is_some() {
            return Err(DocumentError::StepOpen);
        }
        self.open = Some(OpenStep {
            label: label.into(),
            result: self.finished().clone(),
        });
        Ok(())
    }

    /// Runs `edit` on the step's *starting* version (not on the previous update's result, so a
    /// drag can be recomputed from where it began) and makes its result the current version.
    ///
    /// # Errors
    ///
    /// [`DocumentError::NoOpenStep`] when no step is open; [`DocumentError::Edit`] when `edit`
    /// refuses. In both cases the document is unchanged and an open step stays open.
    pub fn update_step<F>(&mut self, edit: F) -> Result<(), DocumentError>
    where
        F: FnOnce(&Wavetable) -> Result<Wavetable, EditError>,
    {
        let step = self.open.as_mut().ok_or(DocumentError::NoOpenStep)?;
        step.result = edit(&self.states[self.cursor].version)?;
        self.refit();
        Ok(())
    }

    /// Ends the open step. If it changed the table, records one state labelled with the step's
    /// label, drops the redo states and takes a fresh [`ChangeId`]; if it did not (no update, or
    /// an update that returned the start), records nothing.
    ///
    /// # Errors
    ///
    /// [`DocumentError::NoOpenStep`] when no step is open.
    pub fn finish_step(&mut self) -> Result<(), DocumentError> {
        let step = self.open.take().ok_or(DocumentError::NoOpenStep)?;
        if Wavetable::ptr_eq(&step.result, self.finished()) {
            self.refit();
            return Ok(());
        }
        self.states.truncate(self.cursor + 1);
        self.states.push_back(State {
            version: step.result,
            label: Some(step.label),
            serial: self.next_serial,
        });
        self.next_serial += 1;
        // `states.len() - 1 > limit`, never `len > limit + 1`: the limit may be `usize::MAX`.
        while self.states.len() - 1 > self.history_limit {
            self.states.pop_front();
        }
        self.cursor = self.states.len() - 1;
        self.change_id = ChangeId::fresh();
        self.refit();
        Ok(())
    }

    /// Throws the open step away: [`current`](Self::current) is the finished version again.
    ///
    /// # Errors
    ///
    /// [`DocumentError::NoOpenStep`] when no step is open.
    pub fn cancel_step(&mut self) -> Result<(), DocumentError> {
        self.open.take().ok_or(DocumentError::NoOpenStep)?;
        self.refit();
        Ok(())
    }

    /// A whole step in one call: begin, one update, finish. If `edit` refuses, nothing changes
    /// and nothing is recorded (redo steps survive).
    ///
    /// # Errors
    ///
    /// [`DocumentError::StepOpen`] when a step is open; [`DocumentError::Edit`] when `edit`
    /// refuses.
    pub fn apply<F>(&mut self, label: impl Into<String>, edit: F) -> Result<(), DocumentError>
    where
        F: FnOnce(&Wavetable) -> Result<Wavetable, EditError>,
    {
        self.begin_step(label)?;
        if let Err(e) = self.update_step(edit) {
            // The step is open here, so cancelling cannot fail.
            let _ = self.cancel_step();
            return Err(e);
        }
        self.finish_step()
    }

    /// Moves back one state, if there is one, and takes a fresh [`ChangeId`].
    ///
    /// # Errors
    ///
    /// [`DocumentError::StepOpen`] while a step is open; [`DocumentError::NothingToUndo`] on the
    /// oldest state left.
    pub fn undo(&mut self) -> Result<(), DocumentError> {
        if self.open.is_some() {
            return Err(DocumentError::StepOpen);
        }
        if self.cursor == 0 {
            return Err(DocumentError::NothingToUndo);
        }
        self.cursor -= 1;
        self.change_id = ChangeId::fresh();
        self.refit();
        Ok(())
    }

    /// Moves forward one state, if there is one, and takes a fresh [`ChangeId`].
    ///
    /// # Errors
    ///
    /// [`DocumentError::StepOpen`] while a step is open; [`DocumentError::NothingToRedo`] on the
    /// newest state.
    pub fn redo(&mut self) -> Result<(), DocumentError> {
        if self.open.is_some() {
            return Err(DocumentError::StepOpen);
        }
        if self.cursor + 1 >= self.states.len() {
            return Err(DocumentError::NothingToRedo);
        }
        self.cursor += 1;
        self.change_id = ChangeId::fresh();
        self.refit();
        Ok(())
    }

    /// The label of the step `undo` would revert, if there is one.
    pub fn undo_label(&self) -> Option<&str> {
        if self.cursor == 0 {
            return None;
        }
        self.states[self.cursor].label.as_deref()
    }

    /// The label of the step `redo` would reapply, if there is one.
    pub fn redo_label(&self) -> Option<&str> {
        self.states.get(self.cursor + 1)?.label.as_deref()
    }

    /// Whether the finished state is another one than the state last marked saved (compared by
    /// identity, not by samples: undo then redo of the same edit still counts as modified). A new document is not
    /// modified.
    pub fn is_modified(&self) -> bool {
        self.states[self.cursor].serial != self.saved
    }

    /// Fits the selection to [`current`](Self::current); called after every change of it.
    fn refit(&mut self) {
        self.selection = self.selection.fit(self.current());
    }

    /// Marks the finished version as the saved one. [`is_modified`](Self::is_modified) is false
    /// until the content moves to another state.
    pub fn mark_saved(&mut self) {
        self.saved = self.states[self.cursor].serial;
    }
}
