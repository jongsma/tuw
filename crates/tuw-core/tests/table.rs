// SPDX-License-Identifier: GPL-3.0-or-later

//! Creating, reading and converting tables (AC1, AC2, AC7).

mod common;

use common::{assert_same, table_with_unknown_keys, tuw3_table, unknown_keys_bytes};
use proptest::prelude::*;
use tuw_core::{EditError, Frame, Wavetable};
use tuw3::{SizeField, Table, TableError};

proptest! {
    #[test]
    fn round_trip_is_bit_identical(t in tuw3_table(4)) {
        assert_same(&Wavetable::from(t.clone()).to_tuw3(), &t);
    }
}

#[test]
fn round_trip_keeps_negative_zero_subnormals_and_unknown_keys() {
    let mut samples = vec![0.5f32; 600];
    samples[3] = -0.0;
    samples[4] = f32::MIN_POSITIVE / 2.0;
    samples[5] = -f32::MIN_POSITIVE / 2.0;
    let t = Table::new(600, 1, 1, samples, tuw3::Metadata::default()).unwrap();
    assert_same(&Wavetable::from(t.clone()).to_tuw3(), &t);

    let u = table_with_unknown_keys();
    let back = Wavetable::from(u.clone()).to_tuw3();
    assert_same(&back, &u);
    assert_eq!(tuw3::write(&back), unknown_keys_bytes());
}

#[test]
fn silent_table_is_all_zero_and_writable() {
    let t = Wavetable::silent(2048, 3, 2).unwrap();
    assert_eq!((t.frame_size(), t.y_size(), t.z_size()), (2048, 3, 2));
    let table = t.to_tuw3();
    assert!(
        table
            .samples()
            .iter()
            .all(|s| s.to_bits() == 0.0f32.to_bits())
    );
    assert_same(&tuw3::read(&tuw3::write(&table)).unwrap(), &table);
}

#[test]
fn silent_table_refuses_sizes_outside_the_limits() {
    let cases = [
        ((7, 1, 1), SizeField::FrameSize, 7),
        ((65_537, 1, 1), SizeField::FrameSize, 65_537),
        ((8, 0, 1), SizeField::YSize, 0),
        ((8, 4_097, 1), SizeField::YSize, 4_097),
        ((8, 1, 0), SizeField::ZSize, 0),
        ((8, 1, 4_097), SizeField::ZSize, 4_097),
    ];
    for ((f, y, z), field, value) in cases {
        match Wavetable::silent(f, y, z) {
            Err(EditError::Limits(TableError::SizeOutOfRange {
                field: got_field,
                value: got_value,
                ..
            })) => {
                assert_eq!(got_field, field);
                assert_eq!(got_value, value);
            }
            other => panic!("expected SizeOutOfRange for {f}x{y}x{z}, got {other:?}"),
        }
    }
    assert!(matches!(
        Wavetable::silent(65_536, 4_096, 2),
        Err(EditError::Limits(TableError::TooManySamples { .. }))
    ));
}

#[test]
fn frame_outside_the_grid_is_an_error() {
    let t = Wavetable::silent(8, 2, 3).unwrap();
    assert!(matches!(t.frame(2, 0), Err(EditError::OutsideGrid { .. })));
    assert!(matches!(t.frame(0, 3), Err(EditError::OutsideGrid { .. })));
    assert!(t.frame(1, 2).is_ok());
}

#[test]
fn frames_iterate_in_tuw3_order() {
    let (fs, ys, zs) = (8usize, 2u32, 3u32);
    let mut samples = vec![0.0f32; fs * (ys * zs) as usize];
    for z in 0..zs {
        for y in 0..ys {
            samples[((z * ys + y) as usize) * fs] = (z * 10 + y + 1) as f32;
        }
    }
    let t = Wavetable::from(Table::new(8, ys, zs, samples, tuw3::Metadata::default()).unwrap());
    let got: Vec<(u32, u32, u32)> = t
        .frames()
        .map(|(y, z, f)| (y, z, f.samples()[0].to_bits()))
        .collect();
    let want: Vec<(u32, u32, u32)> = [(0, 0), (1, 0), (0, 1), (1, 1), (0, 2), (1, 2)]
        .into_iter()
        .map(|(y, z)| (y, z, ((z * 10 + y + 1) as f32).to_bits()))
        .collect();
    assert_eq!(got, want);
}

#[test]
fn types_are_send_and_sync() {
    fn check<T: Send + Sync>() {}
    check::<Wavetable>();
    check::<Frame>();
    check::<EditError>();
}
