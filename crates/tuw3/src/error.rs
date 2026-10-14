// SPDX-License-Identifier: GPL-3.0-or-later

//! Error types.
//!
//! - [`TableError`]: what makes a table invalid, whether it comes from a file or from code.
//! - [`MetadataError`]: what makes a metadata block invalid.
//! - [`ReadError`]: everything [`read`](crate::read) can reject, including the two above.

use std::fmt;

/// One of the three size fields of a table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeField {
    /// `frame_size`: samples per frame.
    FrameSize,
    /// `y_size`: frames along Y.
    YSize,
    /// `z_size`: frames along Z.
    ZSize,
}

impl fmt::Display for SizeField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            SizeField::FrameSize => "frame_size",
            SizeField::YSize => "y_size",
            SizeField::ZSize => "z_size",
        })
    }
}

/// Why a table is invalid. Returned by [`Table::new`](crate::Table::new), and by
/// [`read`](crate::read) wrapped in [`ReadError::Table`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableError {
    /// A size field is outside the limits in [`limits`](crate::limits).
    SizeOutOfRange {
        /// Which field.
        field: SizeField,
        /// Its value.
        value: u32,
        /// Smallest allowed value.
        min: u32,
        /// Largest allowed value.
        max: u32,
    },
    /// `frame_size × y_size × z_size` is above [`MAX_TOTAL_SAMPLES`](crate::limits::MAX_TOTAL_SAMPLES).
    TooManySamples {
        /// The product of the three sizes.
        total: u64,
    },
    /// The number of samples given is not `frame_size × y_size × z_size`.
    SampleCountMismatch {
        /// `frame_size × y_size × z_size`.
        expected: u64,
        /// Number of samples given.
        actual: u64,
    },
    /// A sample is NaN or ±Infinity. The position is the first such sample in spec order.
    NonFiniteSample {
        /// Frame position along Y.
        y: u32,
        /// Frame position along Z.
        z: u32,
        /// Sample position within the frame.
        x: u32,
    },
    /// The metadata serialises to more than [`MAX_META_LEN`](crate::limits::MAX_META_LEN) bytes.
    MetadataTooLong {
        /// Length of the serialised metadata in bytes.
        len: u64,
    },
}

impl fmt::Display for TableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TableError::SizeOutOfRange {
                field,
                value,
                min,
                max,
            } => write!(f, "{field} is {value}, allowed range is {min}..={max}"),
            TableError::TooManySamples { total } => write!(
                f,
                "table has {total} samples, the maximum is {}",
                crate::limits::MAX_TOTAL_SAMPLES
            ),
            TableError::SampleCountMismatch { expected, actual } => {
                write!(f, "expected {expected} samples, got {actual}")
            }
            TableError::NonFiniteSample { y, z, x } => {
                write!(f, "sample {x} of frame (y={y}, z={z}) is NaN or infinite")
            }
            TableError::MetadataTooLong { len } => write!(
                f,
                "metadata is {len} bytes, the maximum is {}",
                crate::limits::MAX_META_LEN
            ),
        }
    }
}

impl std::error::Error for TableError {}

/// Why a metadata block is invalid (`SPEC.md`, *Metadata*).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetadataError {
    /// The block is not valid UTF-8.
    NotUtf8,
    /// The block is not valid JSON. Holds the parser's description of the problem.
    InvalidJson(String),
    /// The block is valid JSON, but not a JSON object.
    NotAnObject,
    /// A known field (such as `name`) is present but is not a string.
    WrongType {
        /// The field's key.
        key: String,
    },
    /// A key occurs twice in the same JSON object, at any nesting level.
    DuplicateKey {
        /// The repeated key.
        key: String,
    },
}

impl fmt::Display for MetadataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MetadataError::NotUtf8 => f.write_str("metadata is not valid UTF-8"),
            MetadataError::InvalidJson(reason) => write!(f, "metadata is not valid JSON: {reason}"),
            MetadataError::NotAnObject => f.write_str("metadata is not a JSON object"),
            MetadataError::WrongType { key } => {
                write!(f, "metadata field \"{key}\" must be a string")
            }
            MetadataError::DuplicateKey { key } => {
                write!(f, "metadata key \"{key}\" occurs more than once")
            }
        }
    }
}

impl std::error::Error for MetadataError {}

/// Why [`read`](crate::read) rejected a file (`SPEC.md`, *Reading a file*).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    /// The input is shorter than the 24-byte fixed header.
    TooShort {
        /// Length of the input in bytes.
        len: usize,
    },
    /// The first four bytes are not `TUW3`.
    BadMagic {
        /// The bytes found instead.
        found: [u8; 4],
    },
    /// The file has a format version this reader does not support.
    UnsupportedVersion(u16),
    /// The file has flag bits set that this reader does not understand.
    UnsupportedFlags(u16),
    /// `meta_len` is above [`MAX_META_LEN`](crate::limits::MAX_META_LEN).
    MetadataTooLong {
        /// The declared `meta_len`.
        len: u32,
    },
    /// The input is shorter than the header says the file must be.
    Truncated {
        /// File length required by the header.
        expected: u64,
        /// Actual length of the input.
        actual: u64,
    },
    /// The input is longer than the header says the file must be.
    TrailingBytes {
        /// File length required by the header.
        expected: u64,
        /// Actual length of the input.
        actual: u64,
    },
    /// A padding byte between the metadata and the samples is not zero.
    NonZeroPadding {
        /// Byte offset of the first non-zero padding byte.
        offset: u64,
    },
    /// The metadata block is invalid.
    Metadata(MetadataError),
    /// The sizes or samples do not form a valid table.
    Table(TableError),
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadError::TooShort { len } => write!(
                f,
                "file is {len} bytes, shorter than the {}-byte header",
                crate::FIXED_HEADER_LEN
            ),
            ReadError::BadMagic { found } => {
                write!(f, "not a TUW3 file (magic bytes are {found:02x?})")
            }
            ReadError::UnsupportedVersion(version) => {
                write!(f, "unsupported TUW3 version {version}")
            }
            ReadError::UnsupportedFlags(flags) => {
                write!(f, "unsupported TUW3 flags {flags:#06x}")
            }
            ReadError::MetadataTooLong { len } => write!(
                f,
                "meta_len is {len} bytes, the maximum is {}",
                crate::limits::MAX_META_LEN
            ),
            ReadError::Truncated { expected, actual } => write!(
                f,
                "file is truncated: {actual} bytes, the header requires {expected}"
            ),
            ReadError::TrailingBytes { expected, actual } => write!(
                f,
                "file has trailing bytes: {actual} bytes, the header requires {expected}"
            ),
            ReadError::NonZeroPadding { offset } => {
                write!(f, "padding byte at offset {offset} is not zero")
            }
            ReadError::Metadata(error) => error.fmt(f),
            ReadError::Table(error) => error.fmt(f),
        }
    }
}

// The wrapped errors are already part of the `Display` text, so they are not also returned
// as `source()`; error reporters would otherwise print them twice.
impl std::error::Error for ReadError {}

impl From<MetadataError> for ReadError {
    fn from(error: MetadataError) -> Self {
        ReadError::Metadata(error)
    }
}

impl From<TableError> for ReadError {
    fn from(error: TableError) -> Self {
        ReadError::Table(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_carry_context() {
        let size = TableError::SizeOutOfRange {
            field: SizeField::FrameSize,
            value: 7,
            min: 8,
            max: 65_536,
        };
        assert_eq!(
            size.to_string(),
            "frame_size is 7, allowed range is 8..=65536"
        );

        let sample = ReadError::Table(TableError::NonFiniteSample { y: 2, z: 1, x: 7 });
        assert_eq!(
            sample.to_string(),
            "sample 7 of frame (y=2, z=1) is NaN or infinite"
        );

        let duplicate = ReadError::Metadata(MetadataError::DuplicateKey {
            key: "license".to_owned(),
        });
        assert_eq!(
            duplicate.to_string(),
            "metadata key \"license\" occurs more than once"
        );

        let magic = ReadError::BadMagic { found: *b"RIFF" };
        assert_eq!(
            magic.to_string(),
            "not a TUW3 file (magic bytes are [52, 49, 46, 46])"
        );
    }
}
