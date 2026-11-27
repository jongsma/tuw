// SPDX-License-Identifier: GPL-3.0-or-later

//! Inserting and removing rows and columns (AC4, AC5, AC9).

mod common;

use common::{assert_same, tuw3_table};
use proptest::prelude::*;
use tuw_core::{Axis, EditError, Frame, Wavetable};
use tuw3::{SizeField, TableError};

/// 8 × 4 × 3 table whose frame (y, z) is filled with `z * 10 + y + 1`.
fn fixture() -> Wavetable {
    let mut t = Wavetable::silent(8, 4, 3).unwrap();
    for z in 0..3 {
        for y in 0..4 {
            let v = (z * 10 + y + 1) as f32;
            t = t.with_frame(y, z, &[v; 8]).unwrap();
        }
    }
    t
}

fn is_silent(f: &Frame) -> bool {
    f.samples().iter().all(|s| s.to_bits() == 0)
}

/// Checks `new` against `old` after inserting `count` at `at` on `axis`.
fn check_inserted(old: &Wavetable, new: &Wavetable, axis: Axis, at: u32, count: u32) {
    let (grow_y, grow_z) = match axis {
        Axis::Y => (count, 0),
        Axis::Z => (0, count),
    };
    assert_eq!(new.y_size(), old.y_size() + grow_y);
    assert_eq!(new.z_size(), old.z_size() + grow_z);
    for (y, z, frame) in new.frames() {
        let pos = if axis == Axis::Y { y } else { z };
        if pos < at {
            assert!(Frame::ptr_eq(frame, old.frame(y, z).unwrap()));
        } else if pos < at + count {
            assert!(is_silent(frame), "new frame ({y}, {z}) is not silent");
        } else {
            let (oy, oz) = if axis == Axis::Y {
                (y - count, z)
            } else {
                (y, z - count)
            };
            assert!(Frame::ptr_eq(frame, old.frame(oy, oz).unwrap()));
        }
    }
}

#[test]
fn inserting_columns_moves_frames() {
    let t = fixture();
    let before = t.to_tuw3();
    for at in [0, 2, 4] {
        let n = t.insert(Axis::Y, at, 2).unwrap();
        check_inserted(&t, &n, Axis::Y, at, 2);
    }
    assert_same(&t.to_tuw3(), &before);
}

#[test]
fn inserting_rows_moves_frames() {
    let t = fixture();
    let before = t.to_tuw3();
    for at in [0, 1, 3] {
        let n = t.insert(Axis::Z, at, 2).unwrap();
        check_inserted(&t, &n, Axis::Z, at, 2);
    }
    assert_same(&t.to_tuw3(), &before);
}

#[test]
fn removing_rows_and_columns_moves_frames() {
    let t = fixture();
    let before = t.to_tuw3();
    // (axis, size of axis)
    for (axis, size) in [(Axis::Y, 4u32), (Axis::Z, 3)] {
        for count in [1u32, 2] {
            for at in [0, 1, size - count] {
                let n = t.remove(axis, at, count).unwrap();
                match axis {
                    Axis::Y => {
                        assert_eq!((n.y_size(), n.z_size()), (4 - count, 3));
                    }
                    Axis::Z => {
                        assert_eq!((n.y_size(), n.z_size()), (4, 3 - count));
                    }
                }
                for (y, z, frame) in n.frames() {
                    let (oy, oz) = match axis {
                        Axis::Y => (if y < at { y } else { y + count }, z),
                        Axis::Z => (y, if z < at { z } else { z + count }),
                    };
                    assert!(Frame::ptr_eq(frame, t.frame(oy, oz).unwrap()));
                }
            }
        }
    }
    assert_same(&t.to_tuw3(), &before);
}

proptest! {
    #[test]
    fn insert_then_remove_gives_the_same_table(
        t in tuw3_table(3),
        use_z in any::<bool>(),
        at_raw in 0u32..100,
        count in 1u32..=3,
    ) {
        let a = Wavetable::from(t);
        let axis = if use_z { Axis::Z } else { Axis::Y };
        let size = if use_z { a.z_size() } else { a.y_size() };
        let at = at_raw % (size + 1);
        let before = a.to_tuw3();
        let back = a.insert(axis, at, count).unwrap().remove(axis, at, count).unwrap();
        assert_same(&back.to_tuw3(), &before);
        for (y, z, frame) in a.frames() {
            prop_assert!(Frame::ptr_eq(frame, back.frame(y, z).unwrap()));
        }
    }
}

#[test]
fn grid_limits_are_refused() {
    let t = fixture(); // 4 columns, 3 rows
    let before = t.to_tuw3();
    assert_eq!(t.insert(Axis::Y, 0, 0).unwrap_err(), EditError::ZeroCount);
    assert_eq!(t.remove(Axis::Y, 0, 0).unwrap_err(), EditError::ZeroCount);
    assert_eq!(
        t.insert(Axis::Y, 5, 1).unwrap_err(),
        EditError::IndexOutOfRange {
            axis: Axis::Y,
            index: 5,
            size: 4
        }
    );
    assert_eq!(
        t.remove(Axis::Z, 2, 2).unwrap_err(),
        EditError::IndexOutOfRange {
            axis: Axis::Z,
            index: 2,
            size: 3
        }
    );
    assert_eq!(
        t.remove(Axis::Y, 0, 4).unwrap_err(),
        EditError::WouldRemoveAll { axis: Axis::Y }
    );
    assert_eq!(
        t.insert(Axis::Z, u32::MAX, u32::MAX).unwrap_err(),
        EditError::IndexOutOfRange {
            axis: Axis::Z,
            index: u32::MAX,
            size: 3
        }
    );
    assert!(matches!(
        t.insert(Axis::Z, 0, u32::MAX).unwrap_err(),
        EditError::Limits(TableError::SizeOutOfRange {
            field: SizeField::ZSize,
            value: u32::MAX,
            ..
        })
    ));
    assert_eq!(
        t.remove(Axis::Z, 1, u32::MAX).unwrap_err(),
        EditError::IndexOutOfRange {
            axis: Axis::Z,
            index: 1,
            size: 3
        }
    );
    assert_same(&t.to_tuw3(), &before);

    let wide = Wavetable::silent(8, 4_096, 1).unwrap();
    let before = wide.to_tuw3();
    assert!(matches!(
        wide.insert(Axis::Y, 0, 1).unwrap_err(),
        EditError::Limits(TableError::SizeOutOfRange {
            field: SizeField::YSize,
            value: 4_097,
            ..
        })
    ));
    assert_same(&wide.to_tuw3(), &before);

    let big = Wavetable::silent(65_536, 4_096, 1).unwrap();
    assert!(matches!(
        big.insert(Axis::Z, 0, 1).unwrap_err(),
        EditError::Limits(TableError::TooManySamples { .. })
    ));
    // Edits take `&self` and `Wavetable` has no interior mutability, so the original cannot
    // change; a deep snapshot of this 1 GiB table would only cost memory.
    assert_eq!(
        (big.frame_size(), big.y_size(), big.z_size()),
        (65_536, 4_096, 1)
    );
}

#[test]
fn many_small_frames_work() {
    let t = Wavetable::silent(8, 4_096, 16).unwrap();
    // -0.0 and a subnormal must survive the round trip bit for bit.
    let samples = vec![-0.0, f32::from_bits(1), 1.0, -2.5, 0.25, 0.0, 3.0, -1.0];
    let t = t.with_frame(4_095, 15, &samples).unwrap();
    let back = Wavetable::from(t.to_tuw3());
    let bits = |f: &Frame| f.samples().iter().map(|s| s.to_bits()).collect::<Vec<_>>();
    let expected: Vec<u32> = samples.iter().map(|s| s.to_bits()).collect();
    assert_eq!(bits(back.frame(4_095, 15).unwrap()), expected);
    for (y, z) in [(0, 0), (1_000, 7), (4_095, 14), (0, 15)] {
        assert!(is_silent(back.frame(y, z).unwrap()));
    }
}
