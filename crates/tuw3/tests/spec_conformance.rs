// SPDX-License-Identifier: GPL-3.0-or-later

//! The written bytes match `SPEC.md`: layout (AC1), alignment (AC2) and frame order (AC3).

mod common;

use common::{RawFile, position_table};
use tuw3::{Metadata, Table};

/// AC1: `write` produces exactly the bytes the spec describes, and `read` reverses it.
#[test]
fn golden_file_matches_spec_layout() {
    let samples = vec![0.0, 0.5, 1.0, 0.5, 0.0, -0.5, -1.0, -0.25];
    let mut metadata = Metadata::default();
    metadata.name = Some("a".to_owned());
    let table = Table::new(8, 1, 1, samples.clone(), metadata).unwrap();

    // Assembled by hand from the layout table in SPEC.md.
    let mut expected = Vec::new();
    expected.extend_from_slice(b"TUW3"); //                 0: magic
    expected.extend_from_slice(&[0, 0]); //                 4: version 0
    expected.extend_from_slice(&[0, 0]); //                 6: flags 0
    expected.extend_from_slice(&[8, 0, 0, 0]); //           8: frame_size 8
    expected.extend_from_slice(&[1, 0, 0, 0]); //          12: y_size 1
    expected.extend_from_slice(&[1, 0, 0, 0]); //          16: z_size 1
    expected.extend_from_slice(&[12, 0, 0, 0]); //         20: meta_len 12
    expected.extend_from_slice(br#"{"name":"a"}"#); //     24: metadata, 12 bytes
    expected.extend_from_slice(&[0; 12]); //               36: padding to 48
    for sample in &samples {
        expected.extend_from_slice(&sample.to_le_bytes()); // 48: samples
    }
    assert_eq!(expected.len(), 80);

    assert_eq!(tuw3::write(&table), expected);
}

/// AC1: the test helper agrees with the hand-assembled layout, so other tests can rely on it.
#[test]
fn raw_file_helper_matches_writer() {
    let raw = RawFile::valid();
    let table = Table::new(8, 1, 1, raw.samples.clone(), Metadata::default()).unwrap();
    assert_eq!(raw.to_bytes(), tuw3::write(&table));
}

/// AC2: whatever the metadata length, the samples start on a 16-byte boundary after at most
/// 15 zero bytes of padding.
#[test]
fn sample_block_is_16_byte_aligned() {
    // `{"name":"…"}` is 11 bytes plus the name, so names of 0..=64 characters cover
    // meta_len 11..=75; empty metadata covers meta_len 0.
    let mut cases = vec![Metadata::default()];
    for n in 0..=64 {
        let mut metadata = Metadata::default();
        metadata.name = Some("x".repeat(n));
        cases.push(metadata);
    }
    for metadata in cases {
        let table = Table::new(8, 1, 1, vec![0.25; 8], metadata).unwrap();
        let bytes = tuw3::write(&table);
        let meta_len = u32::from_le_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]) as usize;
        let sample_offset = bytes.len() - 8 * 4;
        assert_eq!(sample_offset % 16, 0, "meta_len {meta_len}");
        let padding = &bytes[24 + meta_len..sample_offset];
        assert!(padding.len() <= 15, "meta_len {meta_len}");
        assert!(padding.iter().all(|&b| b == 0), "meta_len {meta_len}");
    }
}

/// AC3: frames are stored Z-major, and `frame(y, z)` finds each one.
#[test]
fn frames_are_stored_z_major() {
    let (frame_size, y_size, z_size) = (8, 3, 2);
    let table = position_table(frame_size, y_size, z_size);
    for z in 0..z_size {
        for y in 0..y_size {
            let frame = table.frame(y, z).unwrap();
            let expected: Vec<u32> = (0..frame_size)
                .map(|x| ((z * 10_000 + y * 100 + x) as f32).to_bits())
                .collect();
            let actual: Vec<u32> = frame.iter().map(|s| s.to_bits()).collect();
            assert_eq!(actual, expected, "frame (y={y}, z={z})");
        }
    }
    // The same order in the written bytes: frame (y=2, z=0) comes right before (y=0, z=1).
    let bytes = tuw3::write(&table);
    let sample_at = |index: usize| {
        let offset = 32 + index * 4; // no metadata: samples start at 32
        f32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ])
    };
    assert_eq!(sample_at(3 * 8 - 1).to_bits(), 207.0f32.to_bits()); // z0 y2 x7
    assert_eq!(sample_at(3 * 8).to_bits(), 10_000.0f32.to_bits()); // z1 y0 x0
}

/// AC3: positions outside the grid give `None`, the last frame on each axis does not.
#[test]
fn frame_outside_the_table_is_none() {
    let table = position_table(8, 3, 2);
    assert!(table.frame(2, 1).is_some());
    assert!(table.frame(3, 0).is_none());
    assert!(table.frame(0, 2).is_none());
    assert!(table.frame(u32::MAX, u32::MAX).is_none());
}
