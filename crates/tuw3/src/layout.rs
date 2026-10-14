// SPDX-License-Identifier: GPL-3.0-or-later

//! Byte positions in a TUW3 file, from the *Layout* section of `SPEC.md`.
//!
//! All results are `u64`, so they cannot overflow for any `u32` header values.

use crate::FIXED_HEADER_LEN;

/// The sample block starts on a multiple of this many bytes.
pub(crate) const SAMPLE_ALIGNMENT: u64 = 16;

/// Number of zero bytes between the metadata and the samples:
/// `pad = (16 − (24 + meta_len) mod 16) mod 16`.
pub(crate) fn padding_len(meta_len: u32) -> u64 {
    let used = (FIXED_HEADER_LEN as u64 + u64::from(meta_len)) % SAMPLE_ALIGNMENT;
    (SAMPLE_ALIGNMENT - used) % SAMPLE_ALIGNMENT
}

/// Byte offset of the first sample: `24 + meta_len + pad`.
pub(crate) fn sample_offset(meta_len: u32) -> u64 {
    FIXED_HEADER_LEN as u64 + u64::from(meta_len) + padding_len(meta_len)
}

/// Exact length of a valid file: `sample_offset + total_samples × 4`.
pub(crate) fn file_len(meta_len: u32, total_samples: u64) -> u64 {
    sample_offset(meta_len) + total_samples * 4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_offset_is_aligned_for_every_small_meta_len() {
        for meta_len in 0..=64 {
            let offset = sample_offset(meta_len);
            assert_eq!(offset % 16, 0, "meta_len {meta_len}");
            assert!(padding_len(meta_len) <= 15, "meta_len {meta_len}");
            assert_eq!(offset, 24 + u64::from(meta_len) + padding_len(meta_len));
        }
    }

    #[test]
    fn no_padding_when_already_aligned() {
        // 24 + 8 = 32 is a multiple of 16.
        assert_eq!(padding_len(8), 0);
        assert_eq!(sample_offset(8), 32);
    }

    #[test]
    fn file_len_does_not_overflow_for_maximum_header_values() {
        assert_eq!(
            file_len(u32::MAX, u64::from(u32::MAX)),
            sample_offset(u32::MAX) + u64::from(u32::MAX) * 4
        );
    }
}
