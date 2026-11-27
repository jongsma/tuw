// SPDX-License-Identifier: GPL-3.0-or-later

//! The document: a wavetable with a selection and an undo/redo history.
//!
//! A [`Document`] is UI-thread code: it allocates and must never be touched from an audio
//! callback. Hand the audio side a clone of [`Document::current`] instead.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{Selection, Wavetable};

/// The history limit [`Document`] users get unless they choose another: 200 undo steps.
pub const DEFAULT_HISTORY_LIMIT: usize = 200;

/// Identifies one version of a document's content. It changes whenever the content changes
/// (a recorded step, undo, redo) and is unique across all documents in the process.
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
        }
    }

    /// The version to show and edit: the open step's result if there is one, else the finished
    /// version.
    pub fn current(&self) -> &Wavetable {
        self.finished()
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
        false
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

    /// Whether the content differs from the state last marked saved. A new document is not
    /// modified.
    pub fn is_modified(&self) -> bool {
        self.states[self.cursor].serial != self.saved
    }
}
