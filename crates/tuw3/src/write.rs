// SPDX-License-Identifier: GPL-3.0-or-later

//! [`write`]: [`Table`] to bytes, following *Layout* in `SPEC.md`.

use crate::layout;
use crate::table::Table;
use crate::{MAGIC, VERSION};

/// Writes a table as a TUW3 file.
///
/// This cannot fail: a [`Table`] is always valid. The output is deterministic: the same table
/// always gives the same bytes.
///
/// # Example
///
/// ```no_run
/// let table = tuw3::Table::new(8, 1, 1, vec![0.0; 8], tuw3::Metadata::default())?;
/// std::fs::write("table.tuw3", tuw3::write(&table))?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn write(table: &Table) -> Vec<u8> {
    let metadata = table.metadata_json();
    // `Table::new` guarantees the metadata is at most 1 MiB, so this fits in u32.
    let meta_len = metadata.len() as u32;
    let samples = table.samples();
    let file_len = layout::file_len(meta_len, samples.len() as u64) as usize;

    let mut out = Vec::with_capacity(file_len);
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // flags: none in version 0
    out.extend_from_slice(&table.frame_size().to_le_bytes());
    out.extend_from_slice(&table.y_size().to_le_bytes());
    out.extend_from_slice(&table.z_size().to_le_bytes());
    out.extend_from_slice(&meta_len.to_le_bytes());
    out.extend_from_slice(metadata);
    out.resize(layout::sample_offset(meta_len) as usize, 0); // zero padding
    for sample in samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    debug_assert_eq!(out.len(), file_len);
    out
}
