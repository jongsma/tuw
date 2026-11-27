// SPDX-License-Identifier: GPL-3.0-or-later

//! The immutable, cheaply shared wavetable value.

use std::fmt;
use std::sync::Arc;

use tuw3::limits::{
    MAX_AXIS_SIZE, MAX_FRAME_SIZE, MAX_META_LEN, MAX_TOTAL_SAMPLES, MIN_AXIS_SIZE, MIN_FRAME_SIZE,
};
use tuw3::{Metadata, SizeField, Table, TableError};

use crate::error::{Axis, EditError};

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

    /// Checks one replacement frame and builds it: position, then length, then finiteness.
    fn checked_frame(&self, y: u32, z: u32, samples: &[f32]) -> Result<Frame, EditError> {
        if y >= self.y_size || z >= self.z_size {
            return Err(EditError::OutsideGrid { y, z });
        }
        if samples.len() != self.frame_size as usize {
            return Err(EditError::FrameLength {
                expected: self.frame_size,
                actual: samples.len(),
            });
        }
        if let Some(x) = samples.iter().position(|s| !s.is_finite()) {
            return Err(EditError::NonFiniteSample { y, z, x: x as u32 });
        }
        Ok(Frame(samples.into()))
    }

    /// A new table with the frame at (`y`, `z`) replaced; every other frame is shared.
    /// The samples are copied into the new frame.
    ///
    /// # Errors
    ///
    /// [`EditError::OutsideGrid`], [`EditError::FrameLength`] or [`EditError::NonFiniteSample`]
    /// (the first bad sample), checked in that order. Nothing changes on error.
    pub fn with_frame(&self, y: u32, z: u32, samples: &[f32]) -> Result<Wavetable, EditError> {
        self.with_frames(&[(y, z, samples)])
    }

    /// A new table with several frames replaced at once. All entries are checked before
    /// anything is built; when a position appears twice, the later entry wins.
    ///
    /// # Errors
    ///
    /// As for [`with_frame`](Wavetable::with_frame), for the first bad entry.
    pub fn with_frames(&self, frames: &[(u32, u32, &[f32])]) -> Result<Wavetable, EditError> {
        let checked = frames
            .iter()
            .map(|&(y, z, samples)| Ok((y, z, self.checked_frame(y, z, samples)?)))
            .collect::<Result<Vec<_>, EditError>>()?;
        let mut result = self.clone();
        for (y, z, frame) in checked {
            result.frames[z as usize * self.y_size as usize + y as usize] = frame;
        }
        Ok(result)
    }

    /// The number of frames along `axis`.
    fn axis_size(&self, axis: Axis) -> u32 {
        match axis {
            Axis::Y => self.y_size,
            Axis::Z => self.z_size,
        }
    }

    /// Builds a table of the new axis size whose frame at (`y`, `z`) is `pick(y, z)`:
    /// the frame at that old position, or `None` for a new silent frame.
    fn regrid(
        &self,
        axis: Axis,
        new_size: u32,
        pick: impl Fn(u32, u32) -> Option<(u32, u32)>,
    ) -> Wavetable {
        let (y_size, z_size) = match axis {
            Axis::Y => (new_size, self.z_size),
            Axis::Z => (self.y_size, new_size),
        };
        // Created on first use, so every new frame of one edit shares one buffer and a remove
        // allocates none.
        let mut silence: Option<Frame> = None;
        let mut frames = Vec::with_capacity(y_size as usize * z_size as usize);
        for z in 0..z_size {
            for y in 0..y_size {
                frames.push(match pick(y, z) {
                    Some((oy, oz)) => {
                        self.frames[oz as usize * self.y_size as usize + oy as usize].clone()
                    }
                    None => silence
                        .get_or_insert_with(|| Frame(vec![0.0f32; self.frame_size as usize].into()))
                        .clone(),
                });
            }
        }
        Wavetable {
            frame_size: self.frame_size,
            y_size,
            z_size,
            metadata: Arc::clone(&self.metadata),
            frames,
        }
    }

    /// A new table with `count` silent rows ([`Axis::Z`]) or columns ([`Axis::Y`]) inserted, so
    /// that the first new one has index `at`. Existing frames move but stay shared. `at == size`
    /// appends.
    ///
    /// # Errors
    ///
    /// [`EditError::ZeroCount`]; [`EditError::IndexOutOfRange`] when `at` is above the axis size;
    /// [`EditError::Limits`] when the new size or the total breaks a TUW3 limit. Nothing changes
    /// on error.
    pub fn insert(&self, axis: Axis, at: u32, count: u32) -> Result<Wavetable, EditError> {
        if count == 0 {
            return Err(EditError::ZeroCount);
        }
        let size = self.axis_size(axis);
        if at > size {
            return Err(EditError::IndexOutOfRange {
                axis,
                index: at,
                size,
            });
        }
        // u64 so that `size + count` cannot overflow.
        let new_size = u64::from(size) + u64::from(count);
        if new_size > u64::from(MAX_AXIS_SIZE) {
            return Err(EditError::Limits(TableError::SizeOutOfRange {
                field: match axis {
                    Axis::Y => SizeField::YSize,
                    Axis::Z => SizeField::ZSize,
                },
                value: u32::try_from(new_size).unwrap_or(u32::MAX),
                min: MIN_AXIS_SIZE,
                max: MAX_AXIS_SIZE,
            }));
        }
        let new_size = new_size as u32; // at most MAX_AXIS_SIZE
        let (y_size, z_size) = match axis {
            Axis::Y => (new_size, self.z_size),
            Axis::Z => (self.y_size, new_size),
        };
        check_sizes(self.frame_size, y_size, z_size).map_err(EditError::Limits)?;
        Ok(self.regrid(axis, new_size, |y, z| {
            let pos = if axis == Axis::Y { y } else { z };
            let old = if pos < at {
                pos
            } else if pos < at + count {
                return None;
            } else {
                pos - count
            };
            Some(if axis == Axis::Y { (old, z) } else { (y, old) })
        }))
    }

    /// A new table with `count` rows ([`Axis::Z`]) or columns ([`Axis::Y`]) removed, starting at
    /// index `at`. Remaining frames move but stay shared.
    ///
    /// # Errors
    ///
    /// [`EditError::ZeroCount`]; [`EditError::IndexOutOfRange`] when `at + count` is above the
    /// axis size; [`EditError::WouldRemoveAll`] when no row or column would remain. Nothing
    /// changes on error.
    pub fn remove(&self, axis: Axis, at: u32, count: u32) -> Result<Wavetable, EditError> {
        if count == 0 {
            return Err(EditError::ZeroCount);
        }
        let size = self.axis_size(axis);
        // u64 so that `at + count` cannot overflow.
        if u64::from(at) + u64::from(count) > u64::from(size) {
            return Err(EditError::IndexOutOfRange {
                axis,
                index: at,
                size,
            });
        }
        if count == size {
            return Err(EditError::WouldRemoveAll { axis });
        }
        Ok(self.regrid(axis, size - count, |y, z| {
            let pos = if axis == Axis::Y { y } else { z };
            let old = if pos < at { pos } else { pos + count };
            Some(if axis == Axis::Y { (old, z) } else { (y, old) })
        }))
    }

    /// A new table with other metadata; every frame is shared.
    ///
    /// # Errors
    ///
    /// [`EditError::Limits`] with [`TableError::MetadataTooLong`] when the encoded metadata
    /// would exceed the TUW3 limit.
    pub fn with_metadata(&self, metadata: Metadata) -> Result<Wavetable, EditError> {
        let len = metadata.encoded_len();
        if len as u64 > u64::from(MAX_META_LEN) {
            return Err(EditError::Limits(TableError::MetadataTooLong {
                len: len as u64,
            }));
        }
        let mut result = self.clone();
        result.metadata = Arc::new(metadata);
        Ok(result)
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
