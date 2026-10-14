// SPDX-License-Identifier: GPL-3.0-or-later

//! Helpers shared by the integration tests.
//!
//! [`RawFile`] assembles TUW3 bytes straight from the layout table in `SPEC.md`, without using
//! `tuw3::write`. Tests that compare the two catch mistakes the reader and writer share.

#![allow(dead_code)] // each test file uses a different subset

use tuw3::Table;

/// A TUW3 file, field by field. Every field can be set to an invalid value on purpose.
#[derive(Clone)]
pub struct RawFile {
    pub magic: [u8; 4],
    pub version: u16,
    pub flags: u16,
    pub frame_size: u32,
    pub y_size: u32,
    pub z_size: u32,
    /// `None`: use `metadata.len()`.
    pub meta_len: Option<u32>,
    pub metadata: Vec<u8>,
    /// `None`: the zero padding the spec requires.
    pub padding: Option<Vec<u8>>,
    pub samples: Vec<f32>,
    pub trailing: Vec<u8>,
}

impl RawFile {
    /// A valid file: `frame_size` 8, one frame, no metadata.
    pub fn valid() -> RawFile {
        RawFile {
            magic: *b"TUW3",
            version: 0,
            flags: 0,
            frame_size: 8,
            y_size: 1,
            z_size: 1,
            meta_len: None,
            metadata: Vec::new(),
            padding: None,
            samples: vec![0.0, 0.5, 1.0, 0.5, 0.0, -0.5, -1.0, -0.5],
            trailing: Vec::new(),
        }
    }

    /// The bytes, laid out as in the *Layout* table of `SPEC.md`.
    pub fn to_bytes(&self) -> Vec<u8> {
        let meta_len = self.meta_len.unwrap_or(self.metadata.len() as u32);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.magic);
        bytes.extend_from_slice(&self.version.to_le_bytes());
        bytes.extend_from_slice(&self.flags.to_le_bytes());
        bytes.extend_from_slice(&self.frame_size.to_le_bytes());
        bytes.extend_from_slice(&self.y_size.to_le_bytes());
        bytes.extend_from_slice(&self.z_size.to_le_bytes());
        bytes.extend_from_slice(&meta_len.to_le_bytes());
        bytes.extend_from_slice(&self.metadata);
        match &self.padding {
            Some(padding) => bytes.extend_from_slice(padding),
            None => {
                let pad = (16 - (24 + self.metadata.len()) % 16) % 16;
                bytes.resize(bytes.len() + pad, 0);
            }
        }
        for sample in &self.samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        bytes.extend_from_slice(&self.trailing);
        bytes
    }
}

/// A table whose sample values encode their own position: `z·10000 + y·100 + x`.
/// All values are integers below 2²⁴, so `f32` holds them exactly.
pub fn position_table(frame_size: u32, y_size: u32, z_size: u32) -> Table {
    let mut samples = Vec::new();
    for z in 0..z_size {
        for y in 0..y_size {
            for x in 0..frame_size {
                samples.push((z * 10_000 + y * 100 + x) as f32);
            }
        }
    }
    Table::new(
        frame_size,
        y_size,
        z_size,
        samples,
        tuw3::Metadata::default(),
    )
    .unwrap()
}

/// Asserts that two tables are identical, comparing samples bit for bit.
///
/// The format does no arithmetic, so every bit must survive a round trip. A tolerance would
/// hide real errors, and `==` on `f32` would treat `-0.0` and `0.0` as equal.
pub fn assert_same_table(a: &Table, b: &Table) {
    assert_eq!(
        (a.frame_size(), a.y_size(), a.z_size()),
        (b.frame_size(), b.y_size(), b.z_size())
    );
    assert_eq!(a.metadata(), b.metadata());
    let bits =
        |table: &Table| -> Vec<u32> { table.samples().iter().map(|s| s.to_bits()).collect() };
    assert_eq!(bits(a), bits(b));
}
