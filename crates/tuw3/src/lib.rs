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
//! This crate deliberately has no DSP, GUI or plugin dependencies, and does no I/O: it turns
//! bytes into a [`Table`] and back, and the caller reads or writes the file.
//!
//! ```
//! let mut metadata = tuw3::Metadata::default();
//! metadata.name = Some("Saw".to_owned());
//! let samples: Vec<f32> = (0..8).map(|x| x as f32 / 4.0 - 1.0).collect();
//! let table = tuw3::Table::new(8, 1, 1, samples, metadata)?;
//!
//! let bytes = tuw3::write(&table);
//! let back = tuw3::read(&bytes)?;
//! assert_eq!(back.metadata().name.as_deref(), Some("Saw"));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod error;
mod layout;
pub mod limits;
mod metadata;
mod read;
mod table;
mod write;

pub use error::{MetadataError, ReadError, SizeField, TableError};
pub use metadata::Metadata;
pub use read::read;
pub use table::Table;
pub use write::write;

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
