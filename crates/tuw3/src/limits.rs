// SPDX-License-Identifier: GPL-3.0-or-later

//! Size limits from the *Size limits* section of `SPEC.md`.
//!
//! Writers must stay within these limits and readers reject files outside them. They are public
//! so that tools can use the same numbers, for example to bound an input field.

/// Smallest allowed `frame_size`, in samples.
pub const MIN_FRAME_SIZE: u32 = 8;

/// Largest allowed `frame_size`, in samples. It does not have to be a power of two.
pub const MAX_FRAME_SIZE: u32 = 65_536;

/// Smallest allowed `y_size` and `z_size`, in frames.
pub const MIN_AXIS_SIZE: u32 = 1;

/// Largest allowed `y_size` and `z_size`, in frames.
pub const MAX_AXIS_SIZE: u32 = 4_096;

/// Largest allowed `frame_size × y_size × z_size`: 2²⁸ samples, 1 GiB of `f32`.
pub const MAX_TOTAL_SAMPLES: u64 = 1 << 28;

/// Largest allowed `meta_len`, in bytes: 1 MiB.
pub const MAX_META_LEN: u32 = 1 << 20;

/// Deepest allowed nesting of JSON objects and arrays in the metadata. The top-level object is
/// level 1, so `{"a":[{}]}` is nested 3 levels deep.
pub const MAX_META_DEPTH: u32 = 64;
