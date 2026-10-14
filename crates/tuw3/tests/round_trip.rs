// SPDX-License-Identifier: GPL-3.0-or-later

//! Round trips (AC4), deterministic output (AC5) and unknown metadata keys (AC10).

mod common;

use common::{RawFile, assert_same_table, position_table};
use tuw3::{Metadata, Table};

fn round_trip(table: &Table) {
    let back = tuw3::read(&tuw3::write(table)).unwrap();
    assert_same_table(&back, table);
}

fn full_metadata() -> Metadata {
    let mut metadata = Metadata::default();
    metadata.name = Some("Vowel cube".to_owned());
    metadata.author = Some("Someone".to_owned());
    metadata.license = Some("CC-BY-4.0".to_owned());
    metadata.y_label = Some("Vowel".to_owned());
    metadata.z_label = Some("Brightness".to_owned());
    metadata.description = Some("a-e-i-o-u along Y".to_owned());
    metadata
}

/// AC4: sizes at the edges of what the spec allows, including axes of size 1 and a frame size
/// that is not a power of two.
#[test]
fn edge_sizes_round_trip() {
    for (frame_size, y_size, z_size) in [(8, 1, 1), (8, 5, 1), (8, 1, 5), (600, 3, 2), (2048, 2, 2)]
    {
        round_trip(&position_table(frame_size, y_size, z_size));
    }
}

/// AC4: unusual but finite sample values survive bit for bit.
#[test]
fn special_finite_values_round_trip() {
    let samples = vec![
        -0.0,
        0.0,
        f32::MIN_POSITIVE / 2.0, // subnormal
        -f32::MIN_POSITIVE / 2.0,
        f32::MAX,
        f32::MIN,
        f32::EPSILON,
        1.0,
    ];
    round_trip(&Table::new(8, 1, 1, samples, Metadata::default()).unwrap());
}

/// AC4: metadata that is empty, complete, or full of characters that need escaping.
#[test]
fn metadata_round_trips() {
    let mut unicode = Metadata::default();
    unicode.name = Some("é 🎹 \" \\ \n \t \u{0}".to_owned());
    unicode.description = Some(String::new());
    for metadata in [Metadata::default(), full_metadata(), unicode] {
        round_trip(&Table::new(8, 1, 1, vec![0.5; 8], metadata).unwrap());
    }
}

/// AC5: the same table always gives the same bytes.
#[test]
fn output_is_deterministic() {
    let table = Table::new(8, 2, 1, vec![0.5; 16], full_metadata()).unwrap();
    assert_eq!(tuw3::write(&table), tuw3::write(&table.clone()));
}

/// AC5: no metadata is written as `meta_len = 0`, not as `{}`.
#[test]
fn empty_metadata_has_zero_length() {
    let table = Table::new(8, 1, 1, vec![0.5; 8], Metadata::default()).unwrap();
    let bytes = tuw3::write(&table);
    assert_eq!(&bytes[20..24], &[0, 0, 0, 0]);
    // A file with `{}` reads as the same, empty metadata.
    let mut raw = RawFile::valid();
    raw.metadata = b"{}".to_vec();
    assert_eq!(
        tuw3::read(&raw.to_bytes()).unwrap().metadata(),
        &Metadata::default()
    );
}

/// AC5 and AC10: known fields in spec order, then unknown keys sorted (also nested), with
/// their content unchanged after read → write → read.
#[test]
fn unknown_keys_survive_and_are_sorted() {
    let mut raw = RawFile::valid();
    raw.metadata = br#"{
        "zeta": [1, 2.5, -3, true, null, "s"],
        "description": "d",
        "alpha": {"y": {"b": false, "a": 0}, "x": []},
        "name": "n"
    }"#
    .to_vec();
    let table = tuw3::read(&raw.to_bytes()).unwrap();
    let bytes = tuw3::write(&table);

    let meta_len = u32::from_le_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]) as usize;
    let json = std::str::from_utf8(&bytes[24..24 + meta_len]).unwrap();
    assert_eq!(
        json,
        r#"{"name":"n","description":"d","alpha":{"x":[],"y":{"a":0,"b":false}},"zeta":[1,2.5,-3,true,null,"s"]}"#
    );
    assert_same_table(&tuw3::read(&bytes).unwrap(), &table);
}

/// AC10: a canonical file is written back byte for byte.
#[test]
fn canonical_file_is_rewritten_identically() {
    let table = Table::new(8, 1, 1, vec![0.5; 8], full_metadata()).unwrap();
    let bytes = tuw3::write(&table);
    assert_eq!(tuw3::write(&tuw3::read(&bytes).unwrap()), bytes);
}

/// Review focus: unknown numbers keep their value, not their spelling. Integers up to 64 bits
/// are exact; other numbers are kept as f64, which is what RFC 8259 §6 says JSON
/// implementations can rely on.
#[test]
fn unknown_numbers_keep_their_value() {
    let mut raw = RawFile::valid();
    raw.metadata = br#"{"x":1e5,"big":18446744073709551615,"neg":-9223372036854775808}"#.to_vec();
    let bytes = tuw3::write(&tuw3::read(&raw.to_bytes()).unwrap());
    let meta_len = u32::from_le_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]) as usize;
    assert_eq!(
        std::str::from_utf8(&bytes[24..24 + meta_len]).unwrap(),
        r#"{"big":18446744073709551615,"neg":-9223372036854775808,"x":100000.0}"#
    );
}
