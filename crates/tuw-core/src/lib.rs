// SPDX-License-Identifier: GPL-3.0-or-later

//! The TUW wavetable engine, shared by **TUWE** (the editor) and **TUWS** (the synth).
//!
//! This is where the DSP lives: frames and tables in memory, bilinear morphing across the
//! Y/Z axes, spectral analysis and (later) bandlimiting. Reading and writing files is the job
//! of the [`tuw3`] crate.
//!
//! The crate is meant to be read and learned from, so every public item is documented.
//! Code reachable from an audio callback must follow the real-time rules in `AGENTS.md`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub use tuw3;
