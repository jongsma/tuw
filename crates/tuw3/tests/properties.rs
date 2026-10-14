// SPDX-License-Identifier: GPL-3.0-or-later

//! Property tests (AC11): random valid tables round-trip, and `read` never panics.

mod common;

use common::assert_same_table;
use proptest::prelude::*;
use tuw3::{Metadata, Table};

/// Any finite `f32`, built from random bits so subnormals and `-0.0` are included.
fn finite_f32() -> impl Strategy<Value = f32> {
    any::<u32>()
        .prop_map(f32::from_bits)
        .prop_filter("finite", |v| v.is_finite())
}

/// Metadata with random known fields; strings may contain any Unicode, including controls.
fn metadata() -> impl Strategy<Value = Metadata> {
    (
        proptest::option::of(".*"),
        proptest::option::of(".*"),
        proptest::option::of(".*"),
    )
        .prop_map(|(name, license, description)| {
            let mut metadata = Metadata::default();
            metadata.name = name;
            metadata.license = license;
            metadata.description = description;
            metadata
        })
}

/// Small random tables: sizes stay low so each case runs fast.
fn table() -> impl Strategy<Value = Table> {
    (8u32..=40, 1u32..=4, 1u32..=4)
        .prop_flat_map(|(frame_size, y_size, z_size)| {
            let total = (frame_size * y_size * z_size) as usize;
            (
                Just((frame_size, y_size, z_size)),
                proptest::collection::vec(finite_f32(), total),
                metadata(),
            )
        })
        .prop_map(|((frame_size, y_size, z_size), samples, metadata)| {
            Table::new(frame_size, y_size, z_size, samples, metadata).unwrap()
        })
}

proptest! {
    #[test]
    fn valid_tables_round_trip(table in table()) {
        let back = tuw3::read(&tuw3::write(&table)).unwrap();
        assert_same_table(&back, &table);
    }

    #[test]
    fn read_never_panics_on_random_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..256)) {
        let _ = tuw3::read(&bytes);
    }

    /// Random bytes rarely get past the magic check, so also corrupt one byte of a valid file.
    #[test]
    fn read_never_panics_on_corrupted_files(
        table in table(),
        position in any::<proptest::sample::Index>(),
        value in any::<u8>(),
    ) {
        let mut bytes = tuw3::write(&table);
        let index = position.index(bytes.len());
        bytes[index] = value;
        let _ = tuw3::read(&bytes);
    }
}
