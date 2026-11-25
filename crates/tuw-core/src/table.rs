// SPDX-License-Identifier: GPL-3.0-or-later

//! The immutable, cheaply shared wavetable value.

use std::fmt;
use std::sync::Arc;

use tuw3::limits::{
    MAX_AXIS_SIZE, MAX_FRAME_SIZE, MAX_TOTAL_SAMPLES, MIN_AXIS_SIZE, MIN_FRAME_SIZE,
};
use tuw3::{Metadata, SizeField, Table, TableError};

use crate::error::EditError;

/// One frame: `frame_size` finite samples that never change once made.
///
/// Cloning copies a pointer, not the samples. A `Frame` can only be made by a [`Wavetable`],
/// so every frame is valid by construction.
#[derive(Clone)]
pub struct Frame(Arc<[f32]>);

impl Frame {
    /// The samples of this frame.
    pub fn samples(&self) -> &[f32] {
        &self.0
    }

    /// Whether `a` and `b` are the same buffer (not merely equal samples).
    pub fn ptr_eq(a: &Frame, b: &Frame) -> bool {
        Arc::ptr_eq(&a.0, &b.0)
    }
}

impl fmt::Debug for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Frame({} samples)", self.0.len())
    }
}

/// An immutable wavetable: a Y × Z grid of [`Frame`]s plus metadata.
///
/// A `Wavetable` always obeys the TUW3 rules (`SPEC.md`, *Size limits* and *Samples*), so
/// [`to_tuw3`](Wavetable::to_tuw3) never fails. Cloning copies the list of frame pointers and
/// shares the frames and the metadata.
#[derive(Clone)]
pub struct Wavetable {
    frame_size: u32,
    y_size: u32,
    z_size: u32,
    metadata: Arc<Metadata>,
    /// In TUW3 order: index `z × y_size + y`.
    frames: Vec<Frame>,
}

/// Checks the three sizes against the TUW3 limits, in the same order as `tuw3` does: frame
/// size, then Y, then Z, then the total.
pub(crate) fn check_sizes(frame_size: u32, y_size: u32, z_size: u32) -> Result<(), TableError> {
    let in_range = |field, value: u32, min, max| {
        if (min..=max).contains(&value) {
            Ok(())
        } else {
            Err(TableError::SizeOutOfRange {
                field,
                value,
                min,
                max,
            })
        }
    };
    in_range(
        SizeField::FrameSize,
        frame_size,
        MIN_FRAME_SIZE,
        MAX_FRAME_SIZE,
    )?;
    in_range(SizeField::YSize, y_size, MIN_AXIS_SIZE, MAX_AXIS_SIZE)?;
    in_range(SizeField::ZSize, z_size, MIN_AXIS_SIZE, MAX_AXIS_SIZE)?;
    let total = u64::from(frame_size) * u64::from(y_size) * u64::from(z_size);
    if total > MAX_TOTAL_SAMPLES {
        return Err(TableError::TooManySamples { total });
    }
    Ok(())
}

impl Wavetable {
    /// A table of the given sizes with every sample `0.0`. All its frames share one buffer.
    ///
    /// # Errors
    ///
    /// [`EditError::Limits`] when a size is outside the TUW3 limits.
    pub fn silent(frame_size: u32, y_size: u32, z_size: u32) -> Result<Wavetable, EditError> {
        check_sizes(frame_size, y_size, z_size).map_err(EditError::Limits)?;
        let silence = Frame(vec![0.0f32; frame_size as usize].into());
        let count = y_size as usize * z_size as usize;
        Ok(Wavetable {
            frame_size,
            y_size,
            z_size,
            metadata: Arc::new(Metadata::default()),
            frames: vec![silence; count],
        })
    }

    /// Samples per frame. Fixed for the life of a table.
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

    /// The metadata, unknown keys included.
    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    /// The frame at (`y`, `z`).
    ///
    /// # Errors
    ///
    /// [`EditError::OutsideGrid`] when the position is outside the grid.
    pub fn frame(&self, y: u32, z: u32) -> Result<&Frame, EditError> {
        if y >= self.y_size || z >= self.z_size {
            return Err(EditError::OutsideGrid { y, z });
        }
        Ok(&self.frames[z as usize * self.y_size as usize + y as usize])
    }

    /// Every frame as `(y, z, frame)`, in TUW3 order (Z outer, Y inner).
    pub fn frames(&self) -> impl Iterator<Item = (u32, u32, &Frame)> {
        let y_size = self.y_size;
        self.frames
            .iter()
            .enumerate()
            .map(move |(i, f)| (i as u32 % y_size, i as u32 / y_size, f))
    }

    /// Converts to a [`tuw3::Table`], copying the samples once. Never fails.
    pub fn to_tuw3(&self) -> Table {
        let mut samples = Vec::with_capacity(self.frames.len() * self.frame_size as usize);
        for frame in &self.frames {
            samples.extend_from_slice(frame.samples());
        }
        Table::new(
            self.frame_size,
            self.y_size,
            self.z_size,
            samples,
            (*self.metadata).clone(),
        )
        // A Wavetable only ever holds sizes, samples and metadata that satisfy the TUW3 rules.
        .expect("a Wavetable always satisfies the TUW3 rules")
    }
}

impl From<tuw3::Table> for Wavetable {
    /// Converts a validated table, copying each frame once. Bit for bit, metadata included.
    fn from(table: tuw3::Table) -> Wavetable {
        let (frame_size, y_size, z_size) = (table.frame_size(), table.y_size(), table.z_size());
        let (samples, metadata) = table.into_parts();
        let frames = samples
            .chunks_exact(frame_size as usize)
            .map(|chunk| Frame(chunk.into()))
            .collect();
        Wavetable {
            frame_size,
            y_size,
            z_size,
            metadata: Arc::new(metadata),
            frames,
        }
    }
}

impl fmt::Debug for Wavetable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Wavetable")
            .field("frame_size", &self.frame_size)
            .field("y_size", &self.y_size)
            .field("z_size", &self.z_size)
            .finish_non_exhaustive()
    }
}
