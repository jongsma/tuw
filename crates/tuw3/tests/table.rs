// SPDX-License-Identifier: GPL-3.0-or-later

//! `Table::new` accepts exactly the valid tables (AC6).

use tuw3::limits::{MAX_AXIS_SIZE, MAX_FRAME_SIZE, MAX_META_LEN, MIN_FRAME_SIZE};
use tuw3::{Metadata, SizeField, Table, TableError};

fn new(frame_size: u32, y_size: u32, z_size: u32) -> Result<Table, TableError> {
    let total = (frame_size * y_size * z_size) as usize;
    Table::new(
        frame_size,
        y_size,
        z_size,
        vec![0.0; total],
        Metadata::default(),
    )
}

#[test]
fn sizes_at_the_limits_are_accepted() {
    assert!(new(MIN_FRAME_SIZE, 1, 1).is_ok());
    assert!(new(MAX_FRAME_SIZE, 1, 1).is_ok());
    assert!(new(8, MAX_AXIS_SIZE, 1).is_ok());
    assert!(new(8, 1, MAX_AXIS_SIZE).is_ok());
}

#[test]
fn sizes_just_outside_the_limits_are_rejected() {
    let cases = [
        (7, 1, 1, SizeField::FrameSize, 7),
        (65_537, 1, 1, SizeField::FrameSize, 65_537),
        (8, 0, 1, SizeField::YSize, 0),
        (8, 4_097, 1, SizeField::YSize, 4_097),
        (8, 1, 0, SizeField::ZSize, 0),
        (8, 1, 4_097, SizeField::ZSize, 4_097),
    ];
    for (frame_size, y_size, z_size, field, value) in cases {
        // The size check comes before the sample count, so no samples are needed.
        let result = Table::new(frame_size, y_size, z_size, Vec::new(), Metadata::default());
        assert!(
            matches!(result, Err(TableError::SizeOutOfRange { field: f, value: v, .. }) if f == field && v == value),
            "{frame_size}×{y_size}×{z_size}: {result:?}"
        );
    }
}

#[test]
fn too_many_samples_are_rejected() {
    // 65 536 × 4 096 × 2 = 2²⁹, twice the limit. No samples are needed to find out.
    let result = Table::new(65_536, 4_096, 2, Vec::new(), Metadata::default());
    assert_eq!(
        result.unwrap_err(),
        TableError::TooManySamples { total: 1 << 29 }
    );
}

#[test]
fn wrong_sample_count_is_rejected() {
    let result = Table::new(8, 2, 1, vec![0.0; 15], Metadata::default());
    assert_eq!(
        result.unwrap_err(),
        TableError::SampleCountMismatch {
            expected: 16,
            actual: 15
        }
    );
}

#[test]
fn non_finite_samples_are_rejected_with_their_position() {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        // The very last sample: frame (y=2, z=1), x=7.
        let mut samples = vec![0.0; 8 * 3 * 2];
        *samples.last_mut().unwrap() = bad;
        let result = Table::new(8, 3, 2, samples, Metadata::default());
        assert_eq!(
            result.unwrap_err(),
            TableError::NonFiniteSample { y: 2, z: 1, x: 7 },
            "{bad}"
        );
    }
}

#[test]
fn metadata_up_to_one_mebibyte_is_accepted() {
    // `{"name":"…"}` is 11 bytes plus the name.
    let fits = MAX_META_LEN as usize - 11;
    let mut metadata = Metadata::default();
    metadata.name = Some("x".repeat(fits));
    assert!(Table::new(8, 1, 1, vec![0.0; 8], metadata.clone()).is_ok());

    metadata.name = Some("x".repeat(fits + 1));
    assert_eq!(
        Table::new(8, 1, 1, vec![0.0; 8], metadata).unwrap_err(),
        TableError::MetadataTooLong {
            len: u64::from(MAX_META_LEN) + 1
        }
    );
}

#[test]
fn into_parts_returns_what_went_in() {
    let mut metadata = Metadata::default();
    metadata.name = Some("n".to_owned());
    let samples: Vec<f32> = (0..8).map(|x| x as f32).collect();
    let table = Table::new(8, 1, 1, samples.clone(), metadata.clone()).unwrap();
    let (back_samples, back_metadata) = table.into_parts();
    let bits = |s: &[f32]| s.iter().map(|v| v.to_bits()).collect::<Vec<_>>();
    assert_eq!(bits(&back_samples), bits(&samples));
    assert_eq!(back_metadata, metadata);
}
