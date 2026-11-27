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
//!   [`Selection::after_insert`] or [`Selection::after_remove`].
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

mod error;
mod selection;
mod table;

pub use error::{Axis, EditError};
pub use selection::Selection;
pub use table::{Frame, Wavetable};
pub use tuw3;
