// SPDX-License-Identifier: GPL-3.0-or-later

//! The TUW wavetable engine, shared by **TUWE** (the editor) and **TUWS** (the synth).
//!
//! This is where the DSP lives: frames and tables in memory, bilinear morphing across the
//! Y/Z axes, spectral analysis and (later) bandlimiting. Reading and writing files is the job
//! of the [`tuw3`] crate.
//!
//! The crate is meant to be read and learned from, so every public item is documented.
//! Code reachable from an audio callback must follow the real-time rules in `AGENTS.md`.
//!
//! # The table model
//!
//! - [`Wavetable`] is an immutable Y × Z grid of frames plus metadata. It always obeys the
//!   TUW3 limits, so [`to_tuw3`](Wavetable::to_tuw3) never fails.
//! - [`Frame`] is one frame of finite samples, behind a shared pointer. Only a `Wavetable`
//!   can make one.
//! - Edits take `&self` and return a **new version** (`with_frame`, `insert`, `remove`, ...);
//!   the old version stays valid. Versions share the frames they did not touch, so keeping many
//!   versions (for undo, say) is cheap. An edit that would break a rule returns an
//!   [`EditError`] and changes nothing.
//! - [`Selection`] is a rectangle on the grid. It is a separate value, not part of any
//!   version, and selecting is not undoable. After an insert or remove, adjust it with
//!   [`Selection::after_insert`] or [`Selection::after_remove`]. A [`Document`] only fits its
//!   selection into the grid as a safety net; a tool that inserts or removes rows or columns
//!   still computes the adjusted selection and calls [`Document::set_selection`] itself.
//!
//! # The edit model
//!
//! A [`Document`] holds the current [`Wavetable`], a [`Selection`] and an undo history.
//!
//! - **History is snapshots.** Every finished step stores a whole version. Versions share
//!   their untouched frames, so this is cheap, and undo and redo just move a cursor.
//! - **A step is one undo entry.** [`Document::begin_step`] opens it, [`Document::update_step`]
//!   runs an edit, and [`Document::finish_step`] records it. [`Document::cancel_step`] throws
//!   it away. For a one-shot edit, [`Document::apply`] does all of that at once.
//! - **Updates start from the start.** Each update gets the version the step began with, not
//!   the result of the last update. A drag (a gain slider, say) can therefore call
//!   `update_step` on every mouse move with the total change so far, and nothing piles up.
//! - **The preview.** While a step is open, [`Document::current`] is its latest result.
//!   [`Document::finished`] is always the state under the undo cursor, and
//!   [`Document::change_id`] changes only when that state does. A preview plays
//!   `finished()`, caching work per `ChangeId`; the view shows `current()`, which also
//!   follows the open step.
//! - **Undo and redo** ([`Document::undo`], [`Document::redo`]) refuse while a step is open.
//!   A step that changed nothing records nothing.
//! - **Limit and saved marker.** The history keeps at most the limit given to
//!   [`Document::new`] as undo steps; [`DEFAULT_HISTORY_LIMIT`] is a sensible value to pass.
//!   [`Document::is_modified`] and [`Document::mark_saved`] track whether the state under the cursor is the saved one.
//! - **UI-thread code.** A `Document` allocates, so it must never be used from an audio
//!   callback. Hand the audio thread a finished [`Wavetable`] instead.
//!
//! ```
//! use tuw_core::{Document, Wavetable};
//!
//! // A table whose only frame is all 1.0.
//! let silent = Wavetable::silent(8, 1, 1)?;
//! let table = silent.with_frame(0, 0, &[1.0; 8])?;
//! let mut doc = Document::new(table, 50);
//! let start = doc.finished().clone();
//!
//! doc.begin_step("Gain")?;
//! for gain in [0.5_f32, 0.25] {
//!     // Each update starts from the step's start, so the gains do not multiply.
//!     doc.update_step(|from| {
//!         let scaled: Vec<f32> = from.frame(0, 0)?.samples().iter().map(|x| x * gain).collect();
//!         from.with_frame(0, 0, &scaled)
//!     })?;
//! }
//! doc.finish_step()?;
//!
//! // Compare bit patterns: floats are never compared with `==`.
//! let frame = doc.finished().frame(0, 0)?;
//! assert!(frame.samples().iter().all(|x| x.to_bits() == 0.25_f32.to_bits()));
//! assert_eq!(doc.undo_label(), Some("Gain"));
//!
//! doc.undo()?;
//! assert!(Wavetable::ptr_eq(doc.finished(), &start));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # Example
//!
//! ```
//! use tuw_core::Wavetable;
//!
//! let table = tuw3::Table::new(8, 1, 1, vec![0.0; 8], tuw3::Metadata::default())?;
//! let wavetable = Wavetable::from(table.clone());
//! assert_eq!(wavetable.frame_size(), 8);
//! // Compare bit patterns: floats are never compared with `==`.
//! let bits = |s: &[f32]| s.iter().map(|x| x.to_bits()).collect::<Vec<u32>>();
//! assert_eq!(bits(wavetable.to_tuw3().samples()), bits(table.samples()));
//! # Ok::<(), tuw3::TableError>(())
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod document;
mod error;
mod selection;
mod table;

pub use document::{ChangeId, DEFAULT_HISTORY_LIMIT, Document};
pub use error::{Axis, DocumentError, EditError};
pub use selection::Selection;
pub use table::{Frame, Wavetable};
pub use tuw3;
