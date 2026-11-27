// SPDX-License-Identifier: GPL-3.0-or-later

//! Replacing frames and metadata (AC3, AC4).

mod common;

use common::{assert_same, finite_f32, tuw3_table};
use proptest::prelude::*;
use tuw_core::{EditError, Frame, Wavetable};
use tuw3::limits::MAX_META_LEN;
use tuw3::{Metadata, Table, TableError};

fn random_table() -> Wavetable {
    let samples: Vec<f32> = (0..8 * 3 * 2).map(|i| i as f32 * 0.01).collect();
    Wavetable::from(Table::new(8, 3, 2, samples, Metadata::default()).unwrap())
}

fn bits(samples: &[f32]) -> Vec<u32> {
    samples.iter().map(|s| s.to_bits()).collect()
}

proptest! {
    #[test]
    fn replacing_one_frame_shares_the_others(
        t in tuw3_table(3),
        new in proptest::collection::vec(finite_f32(), 600),
    ) {
        let a = Wavetable::from(t);
        let (y, z) = (a.y_size() - 1, a.z_size() - 1);
        let new: Vec<f32> = new.into_iter().take(a.frame_size() as usize).chain(std::iter::repeat(0.0)).take(a.frame_size() as usize).collect();
        let before = a.to_tuw3();
        let b = a.with_frame(y, z, &new).unwrap();
        assert_same(&a.to_tuw3(), &before);
        prop_assert_eq!(bits(b.frame(y, z).unwrap().samples()), bits(&new));
        for (fy, fz, frame) in a.frames() {
            let same = Frame::ptr_eq(frame, b.frame(fy, fz).unwrap());
            prop_assert_eq!(same, (fy, fz) != (y, z));
        }
    }
}

#[test]
fn replacing_one_frame_shares_the_others_fixed() {
    let a = random_table();
    let before = a.to_tuw3();
    let new = vec![0.25f32; 8];
    let b = a.with_frame(1, 1, &new).unwrap();
    assert_same(&a.to_tuw3(), &before);
    assert_eq!(bits(b.frame(1, 1).unwrap().samples()), bits(&new));
    for z in 0..2 {
        for y in 0..3 {
            let same = Frame::ptr_eq(a.frame(y, z).unwrap(), b.frame(y, z).unwrap());
            assert_eq!(same, (y, z) != (1, 1), "frame ({y}, {z})");
        }
    }
}

#[test]
fn invalid_frames_are_refused_and_change_nothing() {
    let a = random_table();
    let before = a.to_tuw3();
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut s = vec![0.0f32; 8];
        s[3] = bad;
        assert_eq!(
            a.with_frame(0, 0, &s).unwrap_err(),
            EditError::NonFiniteSample { y: 0, z: 0, x: 3 }
        );
        assert_same(&a.to_tuw3(), &before);
    }
    assert_eq!(
        a.with_frame(0, 0, &[0.0; 7]).unwrap_err(),
        EditError::FrameLength {
            expected: 8,
            actual: 7
        }
    );
    assert_same(&a.to_tuw3(), &before);
    assert_eq!(
        a.with_frame(3, 0, &[0.0; 8]).unwrap_err(),
        EditError::OutsideGrid { y: 3, z: 0 }
    );
    assert_eq!(
        a.with_frame(0, 2, &[0.0; 8]).unwrap_err(),
        EditError::OutsideGrid { y: 0, z: 2 }
    );
    assert_same(&a.to_tuw3(), &before);
}

#[test]
fn a_failed_batch_changes_nothing() {
    let a = random_table();
    let before = a.to_tuw3();
    let mut bad = vec![0.0f32; 8];
    bad[0] = f32::NAN;
    let result = a.with_frames(&[
        (0, 0, &[1.0; 8][..]),
        (1, 0, &[1.0; 8]),
        (2, 0, &[1.0; 8]),
        (0, 1, &bad),
    ]);
    assert_eq!(
        result.unwrap_err(),
        EditError::NonFiniteSample { y: 0, z: 1, x: 0 }
    );
    assert_same(&a.to_tuw3(), &before);
}

#[test]
fn duplicate_positions_in_a_batch_last_wins() {
    let a = random_table();
    let b = a
        .with_frames(&[(0, 0, &[1.0; 8][..]), (0, 0, &[2.0; 8])])
        .unwrap();
    assert_eq!(bits(b.frame(0, 0).unwrap().samples()), bits(&[2.0; 8]));
}

#[test]
fn metadata_over_the_limit_is_refused() {
    let a = random_table();
    let before = a.to_tuw3();
    let mut long = Metadata::default();
    long.description = Some("a".repeat(MAX_META_LEN as usize));
    assert!(matches!(
        a.with_metadata(long),
        Err(EditError::Limits(TableError::MetadataTooLong { .. }))
    ));
    assert_same(&a.to_tuw3(), &before);

    let mut short = Metadata::default();
    short.name = Some("saw".to_string());
    let b = a.with_metadata(short).unwrap();
    assert_eq!(b.to_tuw3().metadata().name.as_deref(), Some("saw"));
    for (y, z, frame) in a.frames() {
        assert!(Frame::ptr_eq(frame, b.frame(y, z).unwrap()));
    }
}
