// SPDX-License-Identifier: GPL-3.0-or-later

//! `read` rejects every case in *Reading a file* of `SPEC.md` (AC7), forged headers (AC8) and
//! duplicate keys (AC9).

mod common;

use common::RawFile;
use tuw3::limits::MAX_META_DEPTH;
use tuw3::{MetadataError, ReadError, SizeField, TableError};

fn read(raw: &RawFile) -> Result<tuw3::Table, ReadError> {
    tuw3::read(&raw.to_bytes())
}

#[test]
fn valid_file_is_accepted() {
    assert!(read(&RawFile::valid()).is_ok());
}

#[test]
fn shorter_than_header_is_rejected() {
    let bytes = RawFile::valid().to_bytes();
    for len in [0, 1, 23] {
        assert_eq!(
            tuw3::read(&bytes[..len]).unwrap_err(),
            ReadError::TooShort { len }
        );
    }
}

#[test]
fn bad_magic_is_rejected() {
    let mut raw = RawFile::valid();
    raw.magic = *b"TUW2";
    assert_eq!(
        read(&raw).unwrap_err(),
        ReadError::BadMagic { found: *b"TUW2" }
    );
}

#[test]
fn other_versions_are_rejected() {
    let mut raw = RawFile::valid();
    raw.version = 1;
    assert_eq!(read(&raw).unwrap_err(), ReadError::UnsupportedVersion(1));
}

#[test]
fn any_flag_bit_is_rejected() {
    for flags in [1, 0x8000, 0xffff] {
        let mut raw = RawFile::valid();
        raw.flags = flags;
        assert_eq!(read(&raw).unwrap_err(), ReadError::UnsupportedFlags(flags));
    }
}

#[test]
fn sizes_out_of_range_are_rejected() {
    let mut raw = RawFile::valid();
    raw.frame_size = 7;
    raw.samples = vec![0.0; 7];
    assert!(matches!(
        read(&raw).unwrap_err(),
        ReadError::Table(TableError::SizeOutOfRange {
            field: SizeField::FrameSize,
            value: 7,
            ..
        })
    ));
    let mut raw = RawFile::valid();
    raw.z_size = 0;
    raw.samples = Vec::new();
    assert!(matches!(
        read(&raw).unwrap_err(),
        ReadError::Table(TableError::SizeOutOfRange {
            field: SizeField::ZSize,
            value: 0,
            ..
        })
    ));
}

#[test]
fn too_many_samples_are_rejected() {
    let mut raw = RawFile::valid();
    (raw.frame_size, raw.y_size, raw.z_size) = (65_536, 4_096, 2);
    raw.samples = Vec::new();
    assert_eq!(
        read(&raw).unwrap_err(),
        ReadError::Table(TableError::TooManySamples { total: 1 << 29 })
    );
}

#[test]
fn meta_len_over_one_mebibyte_is_rejected() {
    let mut raw = RawFile::valid();
    raw.meta_len = Some((1 << 20) + 1);
    assert_eq!(
        read(&raw).unwrap_err(),
        ReadError::MetadataTooLong { len: (1 << 20) + 1 }
    );
}

#[test]
fn truncated_file_is_rejected() {
    let bytes = RawFile::valid().to_bytes(); // 32 + 8 × 4 = 64 bytes
    assert_eq!(
        tuw3::read(&bytes[..63]).unwrap_err(),
        ReadError::Truncated {
            expected: 64,
            actual: 63
        }
    );
}

#[test]
fn trailing_bytes_are_rejected() {
    let mut raw = RawFile::valid();
    raw.trailing = vec![0];
    assert_eq!(
        read(&raw).unwrap_err(),
        ReadError::TrailingBytes {
            expected: 64,
            actual: 65
        }
    );
}

/// AC8: a header claiming the largest allowed table, with no data behind it, is rejected by
/// the length check before anything sized by the header is allocated.
#[test]
fn forged_header_is_rejected_as_truncated() {
    let mut raw = RawFile::valid();
    (raw.frame_size, raw.y_size, raw.z_size) = (65_536, 4_096, 1); // exactly 2²⁸ samples
    raw.meta_len = Some(1 << 20);
    raw.samples = Vec::new();
    raw.padding = Some(Vec::new());
    let bytes = raw.to_bytes();
    assert_eq!(bytes.len(), 24);
    assert_eq!(
        tuw3::read(&bytes).unwrap_err(),
        ReadError::Truncated {
            expected: 24 + (1 << 20) + 8 + (1 << 28) * 4,
            actual: 24
        }
    );
}

#[test]
fn non_zero_padding_is_rejected() {
    let mut raw = RawFile::valid();
    raw.metadata = br#"{"name":"a"}"#.to_vec(); // 12 bytes, so 12 bytes of padding
    let mut padding = vec![0; 12];
    padding[5] = 1;
    raw.padding = Some(padding);
    assert_eq!(
        read(&raw).unwrap_err(),
        ReadError::NonZeroPadding {
            offset: 24 + 12 + 5
        }
    );
}

#[test]
fn invalid_metadata_is_rejected() {
    // Each case: the metadata block, and a check on the error it must give.
    type Case = (&'static [u8], fn(&MetadataError) -> bool);
    let cases: [Case; 5] = [
        (&[0xff, 0xfe, 0xfd], |e| *e == MetadataError::NotUtf8),
        (b"{\"name\":", |e| {
            matches!(e, MetadataError::InvalidJson(_))
        }),
        (b"\xef\xbb\xbf{}", |e| {
            matches!(e, MetadataError::InvalidJson(_))
        }), // BOM
        (b"\"just a string\"", |e| *e == MetadataError::NotAnObject),
        (b"{\"license\":null}", |e| {
            *e == MetadataError::WrongType {
                key: "license".to_owned(),
            }
        }),
    ];
    for (metadata, expected) in cases {
        let mut raw = RawFile::valid();
        raw.metadata = metadata.to_vec();
        match read(&raw) {
            Err(ReadError::Metadata(error)) => assert!(expected(&error), "{error:?}"),
            other => panic!("{metadata:?}: {other:?}"),
        }
    }
}

/// AC9: duplicate keys, at the top level or nested in an unknown value.
#[test]
fn duplicate_keys_are_rejected() {
    let cases: [(&[u8], &str); 2] = [
        (
            br#"{"license":"CC0-1.0","license":"All rights reserved"}"#,
            "license",
        ),
        (br#"{"tool":{"deep":[{"k":1,"k":2}]}}"#, "k"),
    ];
    for (metadata, key) in cases {
        let mut raw = RawFile::valid();
        raw.metadata = metadata.to_vec();
        assert_eq!(
            read(&raw).unwrap_err(),
            ReadError::Metadata(MetadataError::DuplicateKey {
                key: key.to_owned()
            })
        );
    }
}

#[test]
fn non_finite_sample_is_rejected() {
    let mut raw = RawFile::valid();
    raw.samples[7] = f32::NAN;
    assert_eq!(
        read(&raw).unwrap_err(),
        ReadError::Table(TableError::NonFiniteSample { y: 0, z: 0, x: 7 })
    );
}

// Metadata the spec bounds beyond plain JSON (D13), and hostile metadata: an error, never a
// crash.

/// Metadata nested `depth` levels deep: the top-level object plus `depth - 1` arrays.
fn nested(depth: usize) -> Vec<u8> {
    format!(
        "{{\"x\":{}{}}}",
        "[".repeat(depth - 1),
        "]".repeat(depth - 1)
    )
    .into_bytes()
}

#[test]
fn nesting_up_to_the_limit_is_accepted() {
    let mut raw = RawFile::valid();
    raw.metadata = nested(MAX_META_DEPTH as usize);
    assert!(read(&raw).is_ok());
}

#[test]
fn nesting_beyond_the_limit_is_rejected() {
    // Just past the limit, past serde_json's own limit of 128, and far past it (which would
    // overflow the stack without a limit).
    for depth in [MAX_META_DEPTH as usize + 1, 200, 100_000] {
        let mut raw = RawFile::valid();
        raw.metadata = nested(depth);
        assert_eq!(
            read(&raw).unwrap_err(),
            ReadError::Metadata(MetadataError::NestedTooDeeply),
            "depth {depth}"
        );
    }
}

#[test]
fn lone_surrogates_and_out_of_range_numbers_are_rejected() {
    // Lone high and low UTF-16 surrogates cannot be text; 1e400 does not fit in an f64.
    for json in [
        &br#"{"x":"\ud800"}"#[..],
        br#"{"x":"\udc00"}"#,
        br#"{"x":1e400}"#,
        br#"{"x":-1e400}"#,
    ] {
        let mut raw = RawFile::valid();
        raw.metadata = json.to_vec();
        assert!(
            matches!(
                read(&raw).unwrap_err(),
                ReadError::Metadata(MetadataError::InvalidJson(_))
            ),
            "{}",
            String::from_utf8_lossy(json)
        );
    }
}

#[test]
fn surrogate_pairs_and_tiny_numbers_are_accepted() {
    // A correctly paired surrogate is one character; 1e-400 rounds to zero, which is in range.
    let mut raw = RawFile::valid();
    raw.metadata = br#"{"x":"\ud83c\udfb5","y":1e-400}"#.to_vec();
    assert!(read(&raw).is_ok());
}

/// A file's metadata is within 1 MiB, but its re-encoded form is not (`1e5` is re-written as
/// `100000.0`). The table could not be written back, so `read` reports the file as
/// unsupported instead of returning a table that `write` cannot store.
#[test]
fn metadata_that_grows_beyond_the_limit_when_re_encoded_is_unsupported() {
    let json = format!("{{\"x\":[{}]}}", vec!["1e5"; 260_000].join(","));
    assert!(json.len() <= 1 << 20);
    let mut raw = RawFile::valid();
    raw.metadata = json.into_bytes();
    assert!(matches!(
        read(&raw).unwrap_err(),
        ReadError::MetadataGrowsTooLong { len } if len > 1 << 20
    ));
}
