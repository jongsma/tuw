// SPDX-License-Identifier: GPL-3.0-or-later

//! [`read`]: bytes to [`Table`], following *Reading a file* in `SPEC.md`.

use crate::error::ReadError;
use crate::layout;
use crate::limits::MAX_META_LEN;
use crate::metadata::Metadata;
use crate::table::{Table, check_sizes};
use crate::{FIXED_HEADER_LEN, MAGIC, VERSION};

/// Reads a TUW3 file from its bytes.
///
/// Accepts exactly the files `SPEC.md` allows. The header and the file length are checked
/// before anything sized by the header is allocated, so a forged header cannot make this
/// function reserve more memory than the input itself.
///
/// # Errors
///
/// Returns a [`ReadError`] for every case listed under *Reading a file* in `SPEC.md`.
///
/// # Example
///
/// ```no_run
/// let bytes = std::fs::read("table.tuw3")?;
/// let table = tuw3::read(&bytes)?;
/// println!("{} × {} frames", table.y_size(), table.z_size());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn read(bytes: &[u8]) -> Result<Table, ReadError> {
    // 1. The fixed header must be there.
    if bytes.len() < FIXED_HEADER_LEN {
        return Err(ReadError::TooShort { len: bytes.len() });
    }

    // 2–4. Magic, version, flags.
    let magic = [bytes[0], bytes[1], bytes[2], bytes[3]];
    if magic != MAGIC {
        return Err(ReadError::BadMagic { found: magic });
    }
    let version = u16_at(bytes, 4);
    if version != VERSION {
        return Err(ReadError::UnsupportedVersion(version));
    }
    let flags = u16_at(bytes, 6);
    if flags != 0 {
        return Err(ReadError::UnsupportedFlags(flags));
    }

    // 5–6. Sizes and meta_len within the limits.
    let frame_size = u32_at(bytes, 8);
    let y_size = u32_at(bytes, 12);
    let z_size = u32_at(bytes, 16);
    let meta_len = u32_at(bytes, 20);
    let total = check_sizes(frame_size, y_size, z_size)?;
    if meta_len > MAX_META_LEN {
        return Err(ReadError::MetadataTooLong { len: meta_len });
    }

    // 7. The input must be exactly as long as the header says. After this check every
    //    offset below lies inside `bytes`, and every allocation is bounded by its length.
    let expected = layout::file_len(meta_len, total);
    let actual = bytes.len() as u64;
    if actual < expected {
        return Err(ReadError::Truncated { expected, actual });
    }
    if actual > expected {
        return Err(ReadError::TrailingBytes { expected, actual });
    }

    // 8. Metadata.
    let meta_end = FIXED_HEADER_LEN + meta_len as usize;
    let metadata = Metadata::from_json(&bytes[FIXED_HEADER_LEN..meta_end])?;

    // 9. Padding must be zero.
    let sample_offset = layout::sample_offset(meta_len) as usize;
    if let Some(position) = bytes[meta_end..sample_offset].iter().position(|&b| b != 0) {
        return Err(ReadError::NonZeroPadding {
            offset: (meta_end + position) as u64,
        });
    }

    // 10. Samples; `Table::new` checks that they are finite.
    let samples: Vec<f32> = bytes[sample_offset..]
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();
    Ok(Table::new(frame_size, y_size, z_size, samples, metadata)?)
}

/// The little-endian u16 at `offset`. The caller has checked that it is in bounds.
fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

/// The little-endian u32 at `offset`. The caller has checked that it is in bounds.
fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}
