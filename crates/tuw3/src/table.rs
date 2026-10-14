// SPDX-License-Identifier: GPL-3.0-or-later

//! [`Table`]: a validated 3D wavetable in memory.

use crate::error::{SizeField, TableError};
use crate::limits::{
    MAX_AXIS_SIZE, MAX_FRAME_SIZE, MAX_META_LEN, MAX_TOTAL_SAMPLES, MIN_AXIS_SIZE, MIN_FRAME_SIZE,
};
use crate::metadata::Metadata;

/// A 3D wavetable: `z_size × y_size` frames of `frame_size` samples, plus metadata.
///
/// A `Table` is always valid. It can only be created by [`Table::new`] or [`read`](crate::read),
/// which both check every rule from `SPEC.md`, and it cannot be changed afterwards. To edit a
/// table, take it apart with [`Table::into_parts`] and build a new one.
#[derive(Debug, Clone)]
pub struct Table {
    frame_size: u32,
    y_size: u32,
    z_size: u32,
    /// All samples in spec order: X fastest, then Y, then Z.
    samples: Vec<f32>,
    metadata: Metadata,
}

impl Table {
    /// Creates a table from its sizes, its samples in spec order and its metadata.
    ///
    /// Sample `x` of frame `(y, z)` is at index `(z × y_size + y) × frame_size + x`.
    ///
    /// # Errors
    ///
    /// Returns a [`TableError`] if a size is outside the spec limits, the number of samples is
    /// not `frame_size × y_size × z_size`, a sample is NaN or infinite, or the metadata
    /// serialises to more than 1 MiB.
    pub fn new(
        frame_size: u32,
        y_size: u32,
        z_size: u32,
        samples: Vec<f32>,
        metadata: Metadata,
    ) -> Result<Table, TableError> {
        let total = check_sizes(frame_size, y_size, z_size)?;
        let actual = samples.len() as u64;
        if actual != total {
            return Err(TableError::SampleCountMismatch {
                expected: total,
                actual,
            });
        }
        if let Some(index) = samples.iter().position(|sample| !sample.is_finite()) {
            // `index` < total ≤ 2²⁸, so these divisions fit in u32.
            let index = index as u32;
            return Err(TableError::NonFiniteSample {
                x: index % frame_size,
                y: (index / frame_size) % y_size,
                z: index / frame_size / y_size,
            });
        }
        let metadata_len = metadata.to_json().len() as u64;
        if metadata_len > u64::from(MAX_META_LEN) {
            return Err(TableError::MetadataTooLong { len: metadata_len });
        }
        Ok(Table {
            frame_size,
            y_size,
            z_size,
            samples,
            metadata,
        })
    }

    /// Samples per frame.
    pub fn frame_size(&self) -> u32 {
        self.frame_size
    }

    /// Number of frames along Y.
    pub fn y_size(&self) -> u32 {
        self.y_size
    }

    /// Number of frames along Z.
    pub fn z_size(&self) -> u32 {
        self.z_size
    }

    /// The `frame_size` samples of frame `(y, z)`, or `None` if the position is outside the
    /// table.
    pub fn frame(&self, y: u32, z: u32) -> Option<&[f32]> {
        if y >= self.y_size || z >= self.z_size {
            return None;
        }
        let frame_size = self.frame_size as usize;
        let start = (z as usize * self.y_size as usize + y as usize) * frame_size;
        self.samples.get(start..start + frame_size)
    }

    /// All samples in spec order: X fastest, then Y, then Z.
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    /// The table's metadata.
    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    /// Takes the table apart into its samples and metadata, for example to edit them and
    /// build a new table.
    pub fn into_parts(self) -> (Vec<f32>, Metadata) {
        (self.samples, self.metadata)
    }
}

/// Checks the three sizes against the spec limits and returns `frame_size × y_size × z_size`.
///
/// Kept separate from `Table::new` so the reader can check a file's header before it
/// allocates anything; it must not allocate itself.
pub(crate) fn check_sizes(frame_size: u32, y_size: u32, z_size: u32) -> Result<u64, TableError> {
    let checks = [
        (
            SizeField::FrameSize,
            frame_size,
            MIN_FRAME_SIZE,
            MAX_FRAME_SIZE,
        ),
        (SizeField::YSize, y_size, MIN_AXIS_SIZE, MAX_AXIS_SIZE),
        (SizeField::ZSize, z_size, MIN_AXIS_SIZE, MAX_AXIS_SIZE),
    ];
    for (field, value, min, max) in checks {
        if !(min..=max).contains(&value) {
            return Err(TableError::SizeOutOfRange {
                field,
                value,
                min,
                max,
            });
        }
    }
    // Each factor is at most 2¹⁶ or 2¹², so the product is at most 2⁴⁰ and fits in u64.
    let total = u64::from(frame_size) * u64::from(y_size) * u64::from(z_size);
    if total > MAX_TOTAL_SAMPLES {
        return Err(TableError::TooManySamples { total });
    }
    Ok(total)
}
