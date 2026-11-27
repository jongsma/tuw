// SPDX-License-Identifier: GPL-3.0-or-later

//! Errors for tables and their edits.

use std::fmt;

/// One of the two grid axes that rows and columns lie along.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// Along Y: the frames of one row (one Z).
    Y,
    /// Along Z: the frames of one column (one Y).
    Z,
}

impl fmt::Display for Axis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Axis::Y => "Y",
            Axis::Z => "Z",
        })
    }
}

/// Why an operation on a table was refused. A refused operation changes nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    /// The result would break a TUW3 limit: sizes, total samples or metadata length.
    Limits(tuw3::TableError),
    /// A frame was given with the wrong number of samples.
    FrameLength {
        /// The table's frame size.
        expected: u32,
        /// The number of samples given.
        actual: usize,
    },
    /// A sample is NaN or infinite.
    NonFiniteSample {
        /// Frame position along Y.
        y: u32,
        /// Frame position along Z.
        z: u32,
        /// Sample position within the frame.
        x: u32,
    },
    /// A frame position is outside the grid.
    OutsideGrid {
        /// Position along Y.
        y: u32,
        /// Position along Z.
        z: u32,
    },
    /// A row or column index is outside the range allowed for the operation.
    IndexOutOfRange {
        /// The axis the index is on.
        axis: Axis,
        /// The index given.
        index: u32,
        /// The size of the axis.
        size: u32,
    },
    /// An insert or remove with a count of zero.
    ZeroCount,
    /// A remove that would leave the axis without any frame.
    WouldRemoveAll {
        /// The axis that would be emptied.
        axis: Axis,
    },
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EditError::Limits(e) => write!(f, "outside the TUW3 limits: {e}"),
            EditError::FrameLength { expected, actual } => {
                write!(f, "frame has {actual} samples, expected {expected}")
            }
            EditError::NonFiniteSample { y, z, x } => {
                write!(f, "sample {x} of frame (y {y}, z {z}) is not finite")
            }
            EditError::OutsideGrid { y, z } => {
                write!(f, "frame position (y {y}, z {z}) is outside the grid")
            }
            EditError::IndexOutOfRange { axis, index, size } => {
                write!(
                    f,
                    "index {index} is out of range on axis {axis} (size {size})"
                )
            }
            EditError::ZeroCount => f.write_str("the count must be at least 1"),
            EditError::WouldRemoveAll { axis } => {
                write!(f, "cannot remove every frame along axis {axis}")
            }
        }
    }
}

impl std::error::Error for EditError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            EditError::Limits(e) => Some(e),
            _ => None,
        }
    }
}

/// Why an operation on a [`Document`](crate::Document) was refused. A refused operation
/// changes nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentError {
    /// Undo, redo, begin or apply while a step is open.
    StepOpen,
    /// Update, finish or cancel with no open step.
    NoOpenStep,
    /// There is no earlier state to go back to.
    NothingToUndo,
    /// There is no later state to go forward to.
    NothingToRedo,
    /// An update's edit was refused.
    Edit(EditError),
}

impl fmt::Display for DocumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DocumentError::StepOpen => f.write_str("a step is open"),
            DocumentError::NoOpenStep => f.write_str("no step is open"),
            DocumentError::NothingToUndo => f.write_str("nothing to undo"),
            DocumentError::NothingToRedo => f.write_str("nothing to redo"),
            DocumentError::Edit(e) => write!(f, "the edit was refused: {e}"),
        }
    }
}

impl std::error::Error for DocumentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DocumentError::Edit(e) => Some(e),
            _ => None,
        }
    }
}

impl From<EditError> for DocumentError {
    fn from(e: EditError) -> Self {
        DocumentError::Edit(e)
    }
}
