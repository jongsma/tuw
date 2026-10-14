// SPDX-License-Identifier: GPL-3.0-or-later

//! Reader and writer for **TUW3**, the open 3D wavetable file format.
//!
//! A TUW3 file holds a grid of single-cycle frames along two morph axes (Y and Z).
//! The format is defined in [`docs/SPEC.md`](https://github.com/jongsma/tuw/blob/main/docs/SPEC.md);
//! this crate follows the spec, never the other way round.
//!
//! The spec is still a draft (version 0). Only settled parts are implemented here.
//! Points still open in the spec are tracked as GitHub issues and must not be decided
//! silently in code.
//!
//! This crate deliberately has no DSP, GUI or plugin dependencies.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod error;
pub mod limits;

pub use error::{MetadataError, ReadError, SizeField, TableError};

/// Magic bytes at the start of every TUW3 file (`SPEC.md`, offset 0).
pub const MAGIC: [u8; 4] = *b"TUW3";

/// The format version this crate reads and writes (`SPEC.md`, offset 4).
pub const VERSION: u16 = 0;

/// Length in bytes of the fixed header fields that precede the metadata block:
/// `magic` (4) + `version` (2) + `flags` (2) + `frame_size` (4) + `y_size` (4) +
/// `z_size` (4) + `meta_len` (4).
pub const FIXED_HEADER_LEN: usize = 24;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magic_is_ascii_tuw3() {
        assert_eq!(&MAGIC, b"TUW3");
    }

    #[test]
    fn fixed_header_len_matches_spec_field_sizes() {
        // Field sizes from the layout table in SPEC.md, in order.
        let fields = [4, 2, 2, 4, 4, 4, 4];
        assert_eq!(fields.iter().sum::<usize>(), FIXED_HEADER_LEN);
    }
}
