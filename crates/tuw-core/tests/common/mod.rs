// SPDX-License-Identifier: GPL-3.0-or-later

//! Helpers shared by the integration tests.

#![allow(dead_code)] // each test file uses a different subset

use proptest::prelude::*;
use tuw3::{Metadata, Table};

/// Any finite `f32`, built from random bits so subnormals and `-0.0` are included.
pub fn finite_f32() -> impl Strategy<Value = f32> {
    any::<u32>()
        .prop_map(f32::from_bits)
        .prop_filter("finite", |v| v.is_finite())
}

/// Metadata with random known fields; strings may contain any Unicode.
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

/// Small random tables with axes `1..=max_axis`; 600 is a frame size that is not a power of two.
pub fn tuw3_table(max_axis: u32) -> impl Strategy<Value = Table> {
    (
        prop_oneof![8u32..=40, Just(600u32)],
        1u32..=max_axis,
        1u32..=max_axis,
    )
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

/// Asserts equal sizes, metadata and sample bits (exact, so `-0.0` and NaN-free subnormals count).
pub fn assert_same(a: &Table, b: &Table) {
    assert_eq!(a.frame_size(), b.frame_size());
    assert_eq!(a.y_size(), b.y_size());
    assert_eq!(a.z_size(), b.z_size());
    assert_eq!(a.metadata(), b.metadata());
    assert_eq!(a.samples().len(), b.samples().len());
    for (i, (x, y)) in a.samples().iter().zip(b.samples()).enumerate() {
        assert_eq!(x.to_bits(), y.to_bits(), "sample {i} differs");
    }
}

/// Bytes of a one-frame table (frame size 8) with unknown metadata keys, assembled by hand
/// from the *Layout* section of `SPEC.md`.
pub fn unknown_keys_bytes() -> Vec<u8> {
    let metadata = br#"{"name":"x","zz":[1,{"a":null}]}"#;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"TUW3");
    bytes.extend_from_slice(&0u16.to_le_bytes()); // version
    bytes.extend_from_slice(&0u16.to_le_bytes()); // flags
    for size in [8u32, 1, 1] {
        bytes.extend_from_slice(&size.to_le_bytes());
    }
    bytes.extend_from_slice(&(metadata.len() as u32).to_le_bytes());
    bytes.extend_from_slice(metadata);
    while bytes.len() % 16 != 0 {
        bytes.push(0);
    }
    for i in 0..8 {
        bytes.extend_from_slice(&(i as f32 * 0.25 - 1.0).to_le_bytes());
    }
    bytes
}

/// A table whose metadata has keys `tuw3` does not know.
pub fn table_with_unknown_keys() -> Table {
    tuw3::read(&unknown_keys_bytes()).expect("hand-assembled file is valid")
}
