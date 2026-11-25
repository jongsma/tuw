// SPDX-License-Identifier: GPL-3.0-or-later

//! The rectangular selection on the Y × Z grid.

use std::ops::RangeInclusive;

use crate::{Axis, EditError, Wavetable};

/// A rectangle of frames on the grid: an inclusive Y range and an inclusive Z range, each with at
/// least one frame.
///
/// A selection is a plain value, separate from the table: selecting is not an edit. Build one
/// for a table with the constructors; after an insert or remove, adjust it with
/// [`after_insert`](Selection::after_insert) or [`after_remove`](Selection::after_remove) so it
/// keeps lying inside the grid. [`fit`](Selection::fit) is the fallback for any other change of
/// grid size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    y_start: u32,
    y_end: u32,
    z_start: u32,
    z_end: u32,
}

/// Clamps an inclusive range into `0..size` (`size >= 1`), computed in `u64` so callers may pass
/// out-of-range values.
fn fit_range(start: u64, end: u64, size: u32) -> (u32, u32) {
    let last = u64::from(size.max(1) - 1);
    // Both values are at most `last`, which fits in `u32`.
    (start.min(last) as u32, end.min(last) as u32)
}

/// Selection `[s, e]` on one axis after inserting `count` indices at `at`.
fn insert_range(s: u32, e: u32, at: u32, count: u32) -> (u64, u64) {
    let (s, e, at, count) = (u64::from(s), u64::from(e), u64::from(at), u64::from(count));
    if at <= s {
        (s + count, e + count)
    } else if at <= e {
        (s, e + count)
    } else {
        (s, e)
    }
}

/// Selection `[s, e]` on one axis after removing `count` indices from `at`. If no selected index remains, the result is `s` (clamped by the caller).
fn remove_range(s: u32, e: u32, at: u32, count: u32) -> (u64, u64) {
    let (s, e, at, count) = (u64::from(s), u64::from(e), u64::from(at), u64::from(count));
    let removed_end = at + count; // first index after the removed block
    let start = if s < at {
        Some(s)
    } else if e >= removed_end {
        Some(s.max(removed_end) - count)
    } else {
        None
    };
    let end = if e >= removed_end {
        Some(e - count)
    } else if s < at {
        Some(e.min(at - 1))
    } else {
        None
    };
    match (start, end) {
        (Some(a), Some(b)) => (a, b),
        // Nothing remains: the single index now at the old start (the caller clamps it).
        _ => (s, s),
    }
}

impl Selection {
    /// Selects the single frame at `(y, z)`.
    ///
    /// # Errors
    ///
    /// [`EditError::OutsideGrid`] if the position is outside the table.
    pub fn frame(table: &Wavetable, y: u32, z: u32) -> Result<Selection, EditError> {
        Selection::rect(table, (y, z), (y, z))
    }

    /// Selects a whole row: all Y frames at one Z.
    ///
    /// # Errors
    ///
    /// [`EditError::IndexOutOfRange`] on the Z axis if `z` is outside the table.
    pub fn row(table: &Wavetable, z: u32) -> Result<Selection, EditError> {
        if z >= table.z_size() {
            return Err(EditError::IndexOutOfRange {
                axis: Axis::Z,
                index: z,
                size: table.z_size(),
            });
        }
        Ok(Selection {
            y_start: 0,
            y_end: table.y_size() - 1,
            z_start: z,
            z_end: z,
        })
    }

    /// Selects a whole column: all Z frames at one Y.
    ///
    /// # Errors
    ///
    /// [`EditError::IndexOutOfRange`] on the Y axis if `y` is outside the table.
    pub fn column(table: &Wavetable, y: u32) -> Result<Selection, EditError> {
        if y >= table.y_size() {
            return Err(EditError::IndexOutOfRange {
                axis: Axis::Y,
                index: y,
                size: table.y_size(),
            });
        }
        Ok(Selection {
            y_start: y,
            y_end: y,
            z_start: 0,
            z_end: table.z_size() - 1,
        })
    }

    /// Selects the rectangle between two corners `(y, z)`, given in any order.
    ///
    /// # Errors
    ///
    /// [`EditError::OutsideGrid`] if either corner is outside the table.
    pub fn rect(table: &Wavetable, a: (u32, u32), b: (u32, u32)) -> Result<Selection, EditError> {
        for (y, z) in [a, b] {
            if y >= table.y_size() || z >= table.z_size() {
                return Err(EditError::OutsideGrid { y, z });
            }
        }
        Ok(Selection {
            y_start: a.0.min(b.0),
            y_end: a.0.max(b.0),
            z_start: a.1.min(b.1),
            z_end: a.1.max(b.1),
        })
    }

    /// Selects the whole table.
    pub fn all(table: &Wavetable) -> Selection {
        Selection {
            y_start: 0,
            y_end: table.y_size() - 1,
            z_start: 0,
            z_end: table.z_size() - 1,
        }
    }

    /// The selected Y positions.
    pub fn y_range(&self) -> RangeInclusive<u32> {
        self.y_start..=self.y_end
    }

    /// The selected Z positions.
    pub fn z_range(&self) -> RangeInclusive<u32> {
        self.z_start..=self.z_end
    }

    /// Whether the frame at `(y, z)` is selected.
    pub fn contains(&self, y: u32, z: u32) -> bool {
        self.y_range().contains(&y) && self.z_range().contains(&z)
    }

    /// The selection after `count` rows or columns were inserted at index `at` on `axis`;
    /// `table_after` is the table that resulted.
    ///
    /// An insert at or before the selection's start moves it along; an insert after its start
    /// and up to its end grows it; an insert after its end leaves it alone. The result is
    /// computed without overflow and fitted to `table_after`, so it always lies inside it.
    pub fn after_insert(
        &self,
        axis: Axis,
        at: u32,
        count: u32,
        table_after: &Wavetable,
    ) -> Selection {
        let mut next = *self;
        match axis {
            Axis::Y => {
                let (s, e) = insert_range(self.y_start, self.y_end, at, count);
                (next.y_start, next.y_end) = fit_range(s, e, table_after.y_size());
            }
            Axis::Z => {
                let (s, e) = insert_range(self.z_start, self.z_end, at, count);
                (next.z_start, next.z_end) = fit_range(s, e, table_after.z_size());
            }
        }
        next.fit(table_after)
    }

    /// The selection after `count` rows or columns were removed from index `at` on `axis`;
    /// `table_after` is the table that resulted.
    ///
    /// The selection keeps the frames that remain. If none remain, it becomes the single row or
    /// column now at its old start index, or the last one if that index no longer exists. The
    /// result is fitted to `table_after`, so it always lies inside it.
    pub fn after_remove(
        &self,
        axis: Axis,
        at: u32,
        count: u32,
        table_after: &Wavetable,
    ) -> Selection {
        let mut next = *self;
        match axis {
            Axis::Y => {
                let (s, e) = remove_range(self.y_start, self.y_end, at, count);
                (next.y_start, next.y_end) = fit_range(s, e, table_after.y_size());
            }
            Axis::Z => {
                let (s, e) = remove_range(self.z_start, self.z_end, at, count);
                (next.z_start, next.z_end) = fit_range(s, e, table_after.z_size());
            }
        }
        next.fit(table_after)
    }

    /// The selection clamped into `table`'s grid, keeping at least one frame: each range end
    /// becomes at most the last index of its axis.
    pub fn fit(&self, table: &Wavetable) -> Selection {
        let (y_start, y_end) = fit_range(self.y_start.into(), self.y_end.into(), table.y_size());
        let (z_start, z_end) = fit_range(self.z_start.into(), self.z_end.into(), table.z_size());
        Selection {
            y_start,
            y_end,
            z_start,
            z_end,
        }
    }
}
