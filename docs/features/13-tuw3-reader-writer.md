# TUW3 reader and writer (version 0)

- **Issue:** #13
- **Status:** Done
- **Crates:** tuw3
- **Format change:** yes (`SPEC.md` gains the duplicate-key rule (D12) and bounded JSON (D13);
  version bump: no)

---

## Spec

### Problem

TUW3 version 0 is specified in `docs/SPEC.md` (decision D11), but nothing can read or write it
yet. TUWE needs to save and open tables, TUWS will later load them, and authors of other tools
need a reference implementation they can read next to the spec. Until `tuw3` can do this,
no other Phase 1 work can store a table.

### Behaviour

**Scope.** `tuw3` turns a table into TUW3 bytes and back. It does no file or other I/O, no DSP
and no editing; callers read and write files themselves (`std::fs::read` / `std::fs::write`).

**Table.** A `Table` holds the dimensions (`frame_size`, `y_size`, `z_size`), the samples in spec
order (Z-major) and the metadata. A `Table` is always valid:

- it can only be created by `Table::new(frame_size, y_size, z_size, samples, metadata)` or by
  `read`, and both apply the same checks: dimensions within the spec limits, a sample count of
  exactly `frame_size × y_size × z_size`, every sample finite, serialised metadata at most
  1 MiB;
- it gives read-only access: the dimensions, `frame(y, z)` (one frame, or `None` outside the
  grid), `samples()` (the whole block) and `metadata()`;
- to change a table, the caller takes it apart (`into_parts()`) and builds a new one.

Editing models belong to `tuw-core`, which converts to a `Table` to save or play.

**Metadata.** `Metadata` has one public `Option<String>` per known spec field (`name`,
`author`, `license`, `y_label`, `z_label`, `description`). Unknown keys from a file are kept
privately and written back unchanged in content; they are not exposed in the API.
`Metadata::default()` is empty. A known field must be a JSON string when present: `null`, a
number or any other type is a wrong type (absent and `null` are not the same).

**Reading.** `read(bytes: &[u8]) -> Result<Table, ReadError>` accepts exactly the files the spec
allows and rejects every case in the spec's *Reading a file* section. The one exception is the
one the spec permits: valid metadata that would exceed 1 MiB once re-encoded is reported as
unsupported (`ReadError::MetadataGrowsTooLong`, see *Notes*). It checks the fixed header and
the exact file length before it allocates anything sized by the header, so a forged header
cannot cause a large allocation. Each failure is a distinct error variant with context (for
example the field, value and allowed range, or the `(y, z, x)` position of a non-finite
sample).

**Writing.** `write(table: &Table) -> Vec<u8>` cannot fail. Its output is deterministic:

- metadata is compact JSON; known fields in spec order, fields that are `None` left out
  (`Some("")` is written as `""`), then unknown keys in alphabetical order (also inside nested
  objects);
- a table without any metadata is written with `meta_len = 0` (no `{}`);
- padding bytes are zero.

**Errors.** Three typed errors, each implementing `Display` and `std::error::Error`:

- `TableError`: what makes a table invalid, whether it comes from a file or from code
  (size out of range, too many samples, sample count mismatch, non-finite sample, metadata
  too long);
- `MetadataError`: not UTF-8, invalid JSON, not an object, known field of the wrong type,
  duplicate key;
- `ReadError`: file-level failures (too short, bad magic, unsupported version, unsupported
  flags, `meta_len` too long, truncated, trailing bytes, non-zero padding) and wrappers for
  `TableError` and `MetadataError`.

**Constants.** `MAGIC`, `VERSION`, `FIXED_HEADER_LEN` and the spec limits are public, so tools
can use the same numbers (for example to bound an input field in TUWE).

**Spec change.** `SPEC.md` *Metadata* gains: keys must be unique within every JSON object, at
every nesting level; a file with a duplicate key is invalid. The reason (one file meaning
different things in different tools, e.g. two `license` values) goes into `DECISIONS.md`.

**Round trips.**

- `read(write(t))` equals `t` for every valid table, bit for bit.
- `write(read(b))` equals `b` only for files already in this writer's canonical form. A valid
  file from another tool (e.g. pretty-printed JSON) keeps its content but is re-written in
  canonical form.

### Acceptance criteria

Tests are written before the code. Float comparisons in these tests use `f32::to_bits`: the
format does no arithmetic, so every bit must survive, and `==` would equate `-0.0` and `0.0`.

- [x] **AC1 (layout):** `write` of a small hand-built table produces exactly the bytes assembled
      in the test from the `SPEC.md` layout table (header, metadata, padding, samples), and
      `read` of those bytes returns that table.
- [x] **AC2 (alignment):** for every `meta_len` from 0 to 64, the sample block starts at a
      multiple of 16 and the padding is at most 15 zero bytes.
- [x] **AC3 (frame order):** in a table whose samples encode their own `(y, z, x)`,
      `frame(y, z)` returns the right frame for every position, including the last frame on
      each axis; positions outside the grid return `None`.
- [x] **AC4 (round trip):** `read(write(t))` returns `t` for: `y_size = 1`, `z_size = 1`, both 1,
      `frame_size` 8 and 600; samples including `-0.0`, subnormals and `±f32::MAX`; metadata that
      is empty, complete, contains Unicode and escapes, and contains nested unknown keys.
- [x] **AC5 (determinism):** writing the same table twice gives identical bytes; empty metadata
      gives `meta_len = 0`; known fields appear in spec order before unknown keys, which are
      sorted.
- [x] **AC6 (table validity):** `Table::new` rejects, each with its own `TableError`: every
      dimension just outside its limit (and accepts the limit itself), a total above 2²⁸
      samples, a wrong sample count, NaN, +Inf and −Inf (including at the last sample), and
      metadata that serialises to more than 1 MiB.
- [x] **AC7 (rejection):** `read` returns the matching `ReadError` for each case in the spec's
      *Reading a file*: too short, bad magic, version ≠ 0, any flag bit set, sizes out of range,
      `meta_len` over 1 MiB, truncated, trailing bytes, non-zero padding, metadata that is not
      UTF-8 / not JSON / not an object / has a known field of the wrong type (including `null`),
      non-finite sample.
- [x] **AC8 (forged header):** a 24-byte file whose header claims the maximum sizes is rejected
      as truncated, without allocating memory sized by the header.
- [x] **AC9 (duplicate keys):** metadata with a duplicate key, at the top level or inside a
      nested unknown value, is rejected; `SPEC.md` and `DECISIONS.md` state the rule.
- [x] **AC10 (unknown keys):** unknown keys of any JSON type survive `read` → `write` → `read`
      with the same content.
- [x] **AC11 (robustness):** property tests (`proptest`): random valid tables always round-trip,
      and `read` on random bytes never panics.
- [x] **AC12 (crate rules):** `tuw3` has no I/O, DSP, GUI or plugin code; its only new runtime
      dependency is `serde_json` (with `serde`); `proptest` is a dev-dependency only;
      `cargo deny check` passes; every public item has a doc comment.

### Out of scope

- File I/O, streaming readers/writers (`io::Read`/`io::Write`), memory-mapping and lazy
  per-slice loading. The bytes API is designed so these can be added later without breaking it.
- Public access to unknown metadata keys (can be added later without breaking the API).
- Preserving the original order or formatting of a foreign file's JSON.
- Serum-style WAV import/export (#9), normalisation (#7), mip levels (#8).
- Fuzzing with `cargo-fuzz` (needs nightly Rust).
- Using `tuw3` from `tuw-core` or TUWE.

### Open questions

There are no open questions left. The questions below came up while brainstorming the design
(Oct 2026) and are all **closed**; each line gives the decision:

- API: bytes in, bytes out; no streams. Typical tables are a few MiB; streaming would only save
  the temporary raw copy, and the memory-saving path for very large tables (mmap, lazy loading)
  works on `&[u8]`.
- JSON via `serde_json` rather than a hand-written parser (correctness and a nesting limit on
  untrusted input).
- Metadata: typed known fields, unknown keys preserved but private; `serde_json` stays out of
  the public API.
- Duplicate keys are rejected (spec change, see above).
- `Table` enforces its own validity; `write` is infallible.

---

## Plan

> **For agentic workers:** REQUIRED SUB-SKILL: use superpowers:subagent-driven-development (recommended)
> or superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`)
> syntax for tracking. All commands run from the repository root on branch
> `feat/13-tuw3-reader-writer`.

**Goal:** `tuw3` reads and writes TUW3 version 0 exactly as `docs/SPEC.md` specifies.

**Architecture:** a bytes-in, bytes-out API (`read(&[u8]) -> Result<Table, ReadError>`,
`write(&Table) -> Vec<u8>`). `Table` validates itself on creation, so `write` cannot fail and
every `Table` in memory is valid. Metadata is parsed with `serde_json` through a small custom
visitor that rejects duplicate keys; unknown keys are kept privately and written back sorted.

**Tech stack:** Rust 2024 (MSRV 1.85), `serde` 1.0.229 and `serde_json` 1.0.154 (runtime),
`proptest` 1.11.0 (tests only).

**Spec:** the *Spec* section above. Every acceptance criterion (AC1–AC12) maps to a task below.

All code in this plan was built and run in a scratch copy before the plan was written: each
task's end state passes `cargo fmt --check`, `cargo clippy --all-targets -D warnings` and
`cargo test` on its own.

### Global constraints

- Every new `.rs` file starts with `// SPDX-License-Identifier: GPL-3.0-or-later`.
- `tuw3` has no I/O, DSP, GUI or plugin code. Runtime dependencies: `serde = "1.0.229"`,
  `serde_json = "1.0.154"` (default features); dev-dependency: `proptest = "1.11.0"`.
- `#![forbid(unsafe_code)]` and `#![warn(missing_docs)]` stay; every public item has a doc comment.
- Typed errors only (no `anyhow`). No `unwrap`, `expect` or `panic!` on runtime values in library
  code; the single `expect` in `metadata.rs` is on an operation that cannot fail and says why.
- Tests compare floats with `f32::to_bits` (bit-exact, justified in `tests/common/mod.rs`),
  never with `==`.
- `docs/SPEC.md` wins over code. Open spec questions (#6, #7, #8) stay open.
- Definition of done per task: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`; `cargo deny check` whenever `Cargo.toml` changes.
- Conventional Commits with scope `tuw3`; one commit per task; the PR says `Closes #13` and is
  squash-merged.

### Review focus

Inputs the spec implies but no acceptance criterion names. Each has a test in the task that owns
the code.

1. **Deeply nested metadata** (`[[[[…` 100 000 levels): `read` returns `InvalidJson` instead of
   overflowing the stack (serde_json's nesting limit of 128). Test in Task 5.
2. **JSON that Rust cannot represent** (a lone surrogate `"\ud800"`, the number `1e400`):
   `InvalidJson`, no panic. Test in Task 5.
3. **Metadata that grows when re-written:** a block under 1 MiB whose canonical form is over
   1 MiB (`1e5` becomes `100000.0`). `read` reports `Table(MetadataTooLong)` rather than
   returning a table that cannot be stored. Test in Task 5.
4. **Unknown numbers keep their value, not their spelling:** integers up to 64 bits stay exact,
   other numbers become `f64` (`1e5` is written back as `100000.0`), as RFC 8259 §6 allows.
   Test in Task 6.
5. **A valid file with one corrupted byte:** `read` never panics, wherever the byte is. Property
   test in Task 6.

### Files

| File | Responsibility | Task |
|---|---|---|
| `crates/tuw3/Cargo.toml` | dependencies | 2, 4, 6 |
| `crates/tuw3/src/lib.rs` | modules, re-exports, constants, crate docs | 1, 2, 3, 5, 7 |
| `crates/tuw3/src/limits.rs` | public size limits from the spec | 1 |
| `crates/tuw3/src/error.rs` | `SizeField`, `TableError`, `MetadataError`, `ReadError` | 1 |
| `crates/tuw3/src/metadata.rs` | `Metadata`; deterministic JSON writing (Task 2), parsing with duplicate detection (Task 4) | 2, 4, 5 |
| `crates/tuw3/src/table.rs` | `Table` and its validation (`check_sizes`) | 2, 3 |
| `crates/tuw3/src/layout.rs` | padding, sample offset, file length | 3 |
| `crates/tuw3/src/write.rs` | `write` | 3 |
| `crates/tuw3/src/read.rs` | `read` | 5 |
| `crates/tuw3/tests/common/mod.rs` | `RawFile` (bytes built from the spec), `position_table`, `assert_same_table` | 3 |
| `crates/tuw3/tests/table.rs` | AC6 | 2 |
| `crates/tuw3/tests/spec_conformance.rs` | AC1–AC3 | 3, 5 |
| `crates/tuw3/tests/rejection.rs` | AC7–AC9, review focus 1–3 | 5 |
| `crates/tuw3/tests/round_trip.rs` | AC4, AC5, AC10, review focus 4 | 6 |
| `crates/tuw3/tests/properties.rs` | AC11, review focus 5 | 6 |
| `docs/SPEC.md`, `docs/DECISIONS.md` | duplicate-key rule (AC9) | 4 |

---

## Tasks

### Task 1: Size limits and error types

Serves: AC6 and AC7 (the error variants they check), AC12 (typed errors).

**Files:**
- Create: `crates/tuw3/src/limits.rs`, `crates/tuw3/src/error.rs`
- Modify: `crates/tuw3/src/lib.rs`

**Interfaces:**
- Produces: `tuw3::limits::{MIN_FRAME_SIZE, MAX_FRAME_SIZE, MIN_AXIS_SIZE, MAX_AXIS_SIZE, MAX_TOTAL_SAMPLES: u64, MAX_META_LEN}`
  (all `u32` except `MAX_TOTAL_SAMPLES`); `tuw3::VERSION: u16`; `tuw3::{SizeField, TableError, MetadataError, ReadError}`
  with the variants shown below. `ReadError` implements `From<MetadataError>` and `From<TableError>`.

- [ ] **Step 1: Write the failing test.** Create `crates/tuw3/src/error.rs` containing only this
  test module (the SPDX line first):

```rust
// SPDX-License-Identifier: GPL-3.0-or-later

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
```

  and add `mod error;` below `#![warn(missing_docs)]` in `crates/tuw3/src/lib.rs`.

- [ ] **Step 2: Run it and see it fail.**
  `cargo test -p tuw3 --lib messages_carry_context`
  Expected: compile error, `TableError`, `SizeField`, `ReadError`, `MetadataError` not found.

- [ ] **Step 3: Implement.** Create `crates/tuw3/src/limits.rs`:

```rust
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
```

  Replace `crates/tuw3/src/error.rs` with the complete file (the test module from step 1 stays at the bottom):

```rust
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
```

  In `crates/tuw3/src/lib.rs`, replace the line `mod error;` from step 1 with this block (directly below `#![warn(missing_docs)]`):

```rust
mod error;
pub mod limits;

pub use error::{MetadataError, ReadError, SizeField, TableError};
```

  and add after `MAGIC`:

```rust
/// The format version this crate reads and writes (`SPEC.md`, offset 4).
pub const VERSION: u16 = 0;
```

- [ ] **Step 4: Run the test and see it pass.** `cargo test -p tuw3 --lib messages_carry_context` — PASS.

- [ ] **Step 5: Run all checks.**

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
Expected: no formatting changes, no clippy output, all tests pass.

- [ ] **Step 6: Commit.**

```sh
git add crates/tuw3/src
git commit -m "feat(tuw3): add size limits and error types"
```
(End the message with the `Co-Authored-By` trailer when an agent commits.)

### Task 2: `Metadata` and the self-validating `Table`

Serves: AC6, AC5 (known fields in spec order, `meta_len = 0` for empty metadata).

**Files:**
- Modify: `crates/tuw3/Cargo.toml`, `crates/tuw3/src/lib.rs`
- Create: `crates/tuw3/src/metadata.rs`, `crates/tuw3/src/table.rs`, `crates/tuw3/tests/table.rs`

**Interfaces:**
- Consumes: Task 1's limits and `TableError`/`SizeField`.
- Produces: `tuw3::Metadata` (pub `Option<String>` fields `name, author, license, y_label, z_label, description`;
  private `extra: serde_json::Map<String, Value>`; `Default`, `Clone`, `PartialEq`, `Debug`);
  `pub(crate) fn Metadata::to_json(&self) -> Vec<u8>`;
  `tuw3::Table` with `pub fn new(frame_size: u32, y_size: u32, z_size: u32, samples: Vec<f32>, metadata: Metadata) -> Result<Table, TableError>`,
  `frame_size()/y_size()/z_size() -> u32`, `frame(y: u32, z: u32) -> Option<&[f32]>`, `samples() -> &[f32]`,
  `metadata() -> &Metadata`, `into_parts(self) -> (Vec<f32>, Metadata)`;
  `pub(crate) fn table::check_sizes(frame_size: u32, y_size: u32, z_size: u32) -> Result<u64, TableError>`.

- [ ] **Step 1: Add the dependency.** `cargo add -p tuw3 serde_json@1.0.154`, then `cargo deny check`
  (expected: `advisories ok, bans ok, licenses ok, sources ok`).

- [ ] **Step 2: Write the failing tests.** Create `crates/tuw3/tests/table.rs`:

```rust
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
```

- [ ] **Step 3: Run them and see them fail.** `cargo test -p tuw3 --test table`
  Expected: compile error, `tuw3::Metadata` and `tuw3::Table` not found.

- [ ] **Step 4: Implement.** Create `crates/tuw3/src/metadata.rs` (its unit tests are at the bottom):

```rust
// SPDX-License-Identifier: GPL-3.0-or-later

//! The metadata block (`SPEC.md`, *Metadata*): deterministic serialisation.

use serde_json::{Map, Value};

/// Descriptive information about a table. Every field is optional.
///
/// Unknown keys found in a file are kept privately and written back by
/// [`write`](crate::write), so tools can add their own keys without losing them in TUWE.
///
/// Because of that private part, create metadata with [`Metadata::default`] and then set
/// fields:
///
/// ```
/// let mut metadata = tuw3::Metadata::default();
/// metadata.name = Some("Vowel cube".to_owned());
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Metadata {
    /// Name of the table.
    pub name: Option<String>,
    /// Author(s).
    pub author: Option<String>,
    /// Licence of the table, preferably an SPDX identifier such as `CC-BY-4.0`.
    pub license: Option<String>,
    /// What the Y axis represents.
    pub y_label: Option<String>,
    /// What the Z axis represents.
    pub z_label: Option<String>,
    /// Free text.
    pub description: Option<String>,
    /// Keys this crate does not know, preserved for writing.
    extra: Map<String, Value>,
}

impl Metadata {
    /// The known fields with their keys, in the order of the spec's table.
    fn known_fields(&self) -> [(&'static str, &Option<String>); 6] {
        [
            ("name", &self.name),
            ("author", &self.author),
            ("license", &self.license),
            ("y_label", &self.y_label),
            ("z_label", &self.z_label),
            ("description", &self.description),
        ]
    }

    /// Serialises to compact JSON: known fields in spec order (`None` left out), then unknown
    /// keys sorted, also inside nested objects. Empty metadata gives an empty block.
    pub(crate) fn to_json(&self) -> Vec<u8> {
        let present: Vec<(&str, &String)> = self
            .known_fields()
            .into_iter()
            .filter_map(|(key, value)| value.as_ref().map(|text| (key, text)))
            .collect();
        if present.is_empty() && self.extra.is_empty() {
            return Vec::new();
        }

        let mut out = Vec::new();
        out.push(b'{');
        let mut first = true;
        for (key, text) in present {
            write_entry(&mut out, &mut first, key, &Value::String(text.clone()));
        }
        for (key, value) in sorted_entries(&self.extra) {
            write_entry(&mut out, &mut first, key, value);
        }
        out.push(b'}');
        out
    }
}

/// Writes `"key":value`, preceded by a comma unless it is the first entry.
fn write_entry(out: &mut Vec<u8>, first: &mut bool, key: &str, value: &Value) {
    if !*first {
        out.push(b',');
    }
    *first = false;
    write_scalar(out, &Value::String(key.to_owned()));
    out.push(b':');
    write_sorted(out, value);
}

/// Writes any JSON value with the keys of every object sorted. This does not rely on how
/// `serde_json::Map` orders keys, which changes if any crate enables its `preserve_order`
/// feature.
fn write_sorted(out: &mut Vec<u8>, value: &Value) {
    match value {
        Value::Object(object) => {
            out.push(b'{');
            let mut first = true;
            for (key, inner) in sorted_entries(object) {
                write_entry(out, &mut first, key, inner);
            }
            out.push(b'}');
        }
        Value::Array(items) => {
            out.push(b'[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(b',');
                }
                write_sorted(out, item);
            }
            out.push(b']');
        }
        scalar => write_scalar(out, scalar),
    }
}

/// Writes a string, number, bool or null.
fn write_scalar(out: &mut Vec<u8>, value: &Value) {
    // Serialising a `Value` into a `Vec` has no I/O that can fail, and a `Value` holds no
    // non-finite numbers or non-string keys, so serde_json cannot return an error here.
    serde_json::to_writer(out, value).expect("serialising a JSON scalar cannot fail");
}

/// The entries of an object, sorted by key.
fn sorted_entries(object: &Map<String, Value>) -> Vec<(&str, &Value)> {
    let mut entries: Vec<(&str, &Value)> = object
        .iter()
        .map(|(key, value)| (key.as_str(), value))
        .collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_metadata_is_an_empty_block() {
        assert_eq!(Metadata::default().to_json(), b"");
    }

    #[test]
    fn known_fields_are_written_in_spec_order() {
        let metadata = Metadata {
            description: Some("d".to_owned()),
            name: Some("n".to_owned()),
            z_label: Some("z".to_owned()),
            ..Metadata::default()
        };
        assert_eq!(
            String::from_utf8(metadata.to_json()).unwrap(),
            r#"{"name":"n","z_label":"z","description":"d"}"#
        );
    }

    #[test]
    fn empty_string_is_written() {
        let metadata = Metadata {
            name: Some(String::new()),
            ..Metadata::default()
        };
        assert_eq!(metadata.to_json(), br#"{"name":""}"#);
    }

    #[test]
    fn characters_are_escaped() {
        let metadata = Metadata {
            name: Some("\"\\\n\u{0}é".to_owned()),
            ..Metadata::default()
        };
        assert_eq!(
            String::from_utf8(metadata.to_json()).unwrap(),
            r#"{"name":"\"\\\n\u0000é"}"#
        );
    }
}
```

  Create `crates/tuw3/src/table.rs`:

```rust
// SPDX-License-Identifier: GPL-3.0-or-later

//! [`Table`]: a validated 3D wavetable in memory.

use crate::error::{SizeField, TableError};
use crate::limits::{
    MAX_AXIS_SIZE, MAX_FRAME_SIZE, MAX_META_LEN, MAX_TOTAL_SAMPLES, MIN_AXIS_SIZE, MIN_FRAME_SIZE,
};
use crate::metadata::Metadata;

/// A 3D wavetable: `z_size × y_size` frames of `frame_size` samples, plus metadata.
///
/// A `Table` is always valid. It can only be created by [`Table::new`] or [`read`](crate::read),
/// which both check every rule from `SPEC.md`, and it cannot be changed afterwards. To edit a
/// table, take it apart with [`Table::into_parts`] and build a new one.
#[derive(Debug, Clone)]
pub struct Table {
    frame_size: u32,
    y_size: u32,
    z_size: u32,
    /// All samples in spec order: X fastest, then Y, then Z.
    samples: Vec<f32>,
    metadata: Metadata,
}

impl Table {
    /// Creates a table from its sizes, its samples in spec order and its metadata.
    ///
    /// Sample `x` of frame `(y, z)` is at index `(z × y_size + y) × frame_size + x`.
    ///
    /// # Errors
    ///
    /// Returns a [`TableError`] if a size is outside the spec limits, the number of samples is
    /// not `frame_size × y_size × z_size`, a sample is NaN or infinite, or the metadata
    /// serialises to more than 1 MiB.
    pub fn new(
        frame_size: u32,
        y_size: u32,
        z_size: u32,
        samples: Vec<f32>,
        metadata: Metadata,
    ) -> Result<Table, TableError> {
        let total = check_sizes(frame_size, y_size, z_size)?;
        let actual = samples.len() as u64;
        if actual != total {
            return Err(TableError::SampleCountMismatch {
                expected: total,
                actual,
            });
        }
        if let Some(index) = samples.iter().position(|sample| !sample.is_finite()) {
            // `index` < total ≤ 2²⁸, so these divisions fit in u32.
            let index = index as u32;
            return Err(TableError::NonFiniteSample {
                x: index % frame_size,
                y: (index / frame_size) % y_size,
                z: index / frame_size / y_size,
            });
        }
        let metadata_len = metadata.to_json().len() as u64;
        if metadata_len > u64::from(MAX_META_LEN) {
            return Err(TableError::MetadataTooLong { len: metadata_len });
        }
        Ok(Table {
            frame_size,
            y_size,
            z_size,
            samples,
            metadata,
        })
    }

    /// Samples per frame.
    pub fn frame_size(&self) -> u32 {
        self.frame_size
    }

    /// Number of frames along Y.
    pub fn y_size(&self) -> u32 {
        self.y_size
    }

    /// Number of frames along Z.
    pub fn z_size(&self) -> u32 {
        self.z_size
    }

    /// The `frame_size` samples of frame `(y, z)`, or `None` if the position is outside the
    /// table.
    pub fn frame(&self, y: u32, z: u32) -> Option<&[f32]> {
        if y >= self.y_size || z >= self.z_size {
            return None;
        }
        let frame_size = self.frame_size as usize;
        let start = (z as usize * self.y_size as usize + y as usize) * frame_size;
        self.samples.get(start..start + frame_size)
    }

    /// All samples in spec order: X fastest, then Y, then Z.
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    /// The table's metadata.
    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    /// Takes the table apart into its samples and metadata, for example to edit them and
    /// build a new table.
    pub fn into_parts(self) -> (Vec<f32>, Metadata) {
        (self.samples, self.metadata)
    }
}

/// Checks the three sizes against the spec limits and returns `frame_size × y_size × z_size`.
///
/// Kept separate from `Table::new` so the reader can check a file's header before it
/// allocates anything; it must not allocate itself.
pub(crate) fn check_sizes(frame_size: u32, y_size: u32, z_size: u32) -> Result<u64, TableError> {
    let checks = [
        (
            SizeField::FrameSize,
            frame_size,
            MIN_FRAME_SIZE,
            MAX_FRAME_SIZE,
        ),
        (SizeField::YSize, y_size, MIN_AXIS_SIZE, MAX_AXIS_SIZE),
        (SizeField::ZSize, z_size, MIN_AXIS_SIZE, MAX_AXIS_SIZE),
    ];
    for (field, value, min, max) in checks {
        if !(min..=max).contains(&value) {
            return Err(TableError::SizeOutOfRange {
                field,
                value,
                min,
                max,
            });
        }
    }
    // Each factor is at most 2¹⁶ or 2¹², so the product is at most 2⁴⁰ and fits in u64.
    let total = u64::from(frame_size) * u64::from(y_size) * u64::from(z_size);
    if total > MAX_TOTAL_SAMPLES {
        return Err(TableError::TooManySamples { total });
    }
    Ok(total)
}
```

  In `crates/tuw3/src/lib.rs`, the block below `#![warn(missing_docs)]` becomes:

```rust
mod error;
pub mod limits;
mod metadata;
mod table;

pub use error::{MetadataError, ReadError, SizeField, TableError};
pub use metadata::Metadata;
pub use table::Table;
```

- [ ] **Step 5: Run the tests and see them pass.** `cargo test -p tuw3` — all pass
  (7 in `tests/table.rs`, plus the unit tests).

- [ ] **Step 6: Run all checks.**

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
Expected: no formatting changes, no clippy output, all tests pass.

- [ ] **Step 7: Commit.**

```sh
git add Cargo.lock crates/tuw3
git commit -m "feat(tuw3): add Metadata and the self-validating Table"
```
(End the message with the `Co-Authored-By` trailer when an agent commits.)

### Task 3: `write`

Serves: AC1 (writer half), AC2, AC3, AC5.

**Files:**
- Create: `crates/tuw3/src/layout.rs`, `crates/tuw3/src/write.rs`, `crates/tuw3/tests/common/mod.rs`,
  `crates/tuw3/tests/spec_conformance.rs`
- Modify: `crates/tuw3/src/table.rs`, `crates/tuw3/src/lib.rs`

**Interfaces:**
- Consumes: `Table` accessors, `Metadata::to_json` (Task 2), `FIXED_HEADER_LEN`, `MAGIC`, `VERSION`.
- Produces: `pub fn tuw3::write(table: &Table) -> Vec<u8>`;
  `pub(crate) fn layout::{padding_len(meta_len: u32) -> u64, sample_offset(meta_len: u32) -> u64, file_len(meta_len: u32, total_samples: u64) -> u64}`;
  `pub(crate) fn Table::metadata_json(&self) -> &[u8]`;
  test helpers `common::{RawFile, position_table, assert_same_table}` (used again in Tasks 5 and 6).

- [ ] **Step 1: Write the failing tests.** Create `crates/tuw3/tests/common/mod.rs`:

```rust
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
```

  Create `crates/tuw3/tests/spec_conformance.rs` (Task 5 adds the read-back check to the first test):

```rust
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
```

- [ ] **Step 2: Run them and see them fail.** `cargo test -p tuw3 --test spec_conformance`
  Expected: compile error, `tuw3::write` not found.

- [ ] **Step 3: Implement the layout helpers.** Create `crates/tuw3/src/layout.rs`:

```rust
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
```

- [ ] **Step 4: Keep the serialised metadata in `Table`.** In `crates/tuw3/src/table.rs`:

  Add a field after `metadata: Metadata,`:

```rust
    /// `metadata` serialised once here, so its length can be checked and `write` cannot fail.
    metadata_json: Vec<u8>,
```

  Replace the metadata check at the end of `Table::new`:

```rust
        let metadata_len = metadata.to_json().len() as u64;
        if metadata_len > u64::from(MAX_META_LEN) {
            return Err(TableError::MetadataTooLong { len: metadata_len });
        }
        Ok(Table {
            frame_size,
            y_size,
            z_size,
            samples,
            metadata,
        })
```

  with:

```rust
        let metadata_json = metadata.to_json();
        if metadata_json.len() as u64 > u64::from(MAX_META_LEN) {
            return Err(TableError::MetadataTooLong {
                len: metadata_json.len() as u64,
            });
        }
        Ok(Table {
            frame_size,
            y_size,
            z_size,
            samples,
            metadata,
            metadata_json,
        })
```

  and add this method after `into_parts`:

```rust
    /// The metadata block as it is written to a file.
    pub(crate) fn metadata_json(&self) -> &[u8] {
        &self.metadata_json
    }
```

- [ ] **Step 5: Implement `write`.** Create `crates/tuw3/src/write.rs`:

```rust
// SPDX-License-Identifier: GPL-3.0-or-later

//! [`write`]: [`Table`] to bytes, following *Layout* in `SPEC.md`.

use crate::layout;
use crate::table::Table;
use crate::{MAGIC, VERSION};

/// Writes a table as a TUW3 file.
///
/// This cannot fail: a [`Table`] is always valid. The output is deterministic: the same table
/// always gives the same bytes.
///
/// # Example
///
/// ```no_run
/// let table = tuw3::Table::new(8, 1, 1, vec![0.0; 8], tuw3::Metadata::default())?;
/// std::fs::write("table.tuw3", tuw3::write(&table))?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn write(table: &Table) -> Vec<u8> {
    let metadata = table.metadata_json();
    // `Table::new` guarantees the metadata is at most 1 MiB, so this fits in u32.
    let meta_len = metadata.len() as u32;
    let samples = table.samples();
    let file_len = layout::file_len(meta_len, samples.len() as u64) as usize;

    let mut out = Vec::with_capacity(file_len);
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // flags: none in version 0
    out.extend_from_slice(&table.frame_size().to_le_bytes());
    out.extend_from_slice(&table.y_size().to_le_bytes());
    out.extend_from_slice(&table.z_size().to_le_bytes());
    out.extend_from_slice(&meta_len.to_le_bytes());
    out.extend_from_slice(metadata);
    out.resize(layout::sample_offset(meta_len) as usize, 0); // zero padding
    for sample in samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    debug_assert_eq!(out.len(), file_len);
    out
}
```

  In `crates/tuw3/src/lib.rs`, the block below `#![warn(missing_docs)]` becomes:

```rust
mod error;
mod layout;
pub mod limits;
mod metadata;
mod table;
mod write;

pub use error::{MetadataError, ReadError, SizeField, TableError};
pub use metadata::Metadata;
pub use table::Table;
pub use write::write;
```

- [ ] **Step 6: Run the tests and see them pass.** `cargo test -p tuw3` — all pass
  (5 in `tests/spec_conformance.rs`; the doc example on `write` is `no_run` and only compiles).

- [ ] **Step 7: Run all checks.**

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
Expected: no formatting changes, no clippy output, all tests pass.

- [ ] **Step 8: Commit.**

```sh
git add crates/tuw3
git commit -m "feat(tuw3): write TUW3 files"
```
(End the message with the `Co-Authored-By` trailer when an agent commits.)

### Task 4: Parse metadata, reject duplicate keys (format change)

Serves: AC9 (parser half and spec text), AC10 (unknown keys kept), AC7 metadata cases.

**Files:**
- Modify: `docs/SPEC.md`, `docs/DECISIONS.md`, `crates/tuw3/Cargo.toml`, `crates/tuw3/src/metadata.rs`

**Interfaces:**
- Consumes: `MetadataError` (Task 1), `Metadata` (Task 2).
- Produces: `pub(crate) fn Metadata::from_json(bytes: &[u8]) -> Result<Metadata, MetadataError>`.
  It is first called by `read` in Task 5; until then it carries a temporary `#[allow(dead_code)]`.

- [ ] **Step 1: Update the spec first.** In `docs/SPEC.md`, section *Metadata*, add this bullet
  after "A known field with a value of the wrong type makes the file invalid.":

```markdown
- Keys must be unique within every JSON object, at every nesting level. A file with a duplicate
  key is invalid.
```

  Append to `docs/DECISIONS.md`:

```markdown
## D12. TUW3 metadata: duplicate keys are invalid (Oct 2026)

**Decision:** a metadata block in which any JSON object, at any nesting level, has the same
key twice makes the file invalid. No `version` bump: the format is still at version 0 and no
files exist yet.

**Why:** RFC 8259 only says keys *should* be unique and leaves duplicates to the parser: some
take the first value, some the last, some reject the file. One file could then mean different
things in different tools, for example `"license"` given twice with a free and a non-free
licence. As with trailing bytes (D11), data with two meanings is rejected.
```

- [ ] **Step 2: Add the dependency.** `cargo add -p tuw3 serde@1.0.229`, then `cargo deny check`.

- [ ] **Step 3: Write the failing tests.** In `crates/tuw3/src/metadata.rs`, replace the test
  module with the one at the bottom of the complete file in step 5.

- [ ] **Step 4: Run them and see them fail.** `cargo test -p tuw3 --lib metadata`
  Expected: compile error, no function `from_json` on `Metadata`.

- [ ] **Step 5: Implement.** Replace `crates/tuw3/src/metadata.rs` with:

```rust
// SPDX-License-Identifier: GPL-3.0-or-later

//! The metadata block (`SPEC.md`, *Metadata*): parsing with duplicate-key detection, and
//! deterministic serialisation.

use std::cell::Cell;
use std::fmt;

use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

use crate::error::MetadataError;

/// Descriptive information about a table. Every field is optional.
///
/// Unknown keys found in a file are kept privately and written back by
/// [`write`](crate::write), so tools can add their own keys without losing them in TUWE.
///
/// Because of that private part, create metadata with [`Metadata::default`] and then set
/// fields:
///
/// ```
/// let mut metadata = tuw3::Metadata::default();
/// metadata.name = Some("Vowel cube".to_owned());
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Metadata {
    /// Name of the table.
    pub name: Option<String>,
    /// Author(s).
    pub author: Option<String>,
    /// Licence of the table, preferably an SPDX identifier such as `CC-BY-4.0`.
    pub license: Option<String>,
    /// What the Y axis represents.
    pub y_label: Option<String>,
    /// What the Z axis represents.
    pub z_label: Option<String>,
    /// Free text.
    pub description: Option<String>,
    /// Keys this crate does not know, preserved for writing.
    extra: Map<String, Value>,
}

impl Metadata {
    /// The known fields with their keys, in the order of the spec's table.
    fn known_fields(&self) -> [(&'static str, &Option<String>); 6] {
        [
            ("name", &self.name),
            ("author", &self.author),
            ("license", &self.license),
            ("y_label", &self.y_label),
            ("z_label", &self.z_label),
            ("description", &self.description),
        ]
    }

    /// Parses a metadata block. An empty block means "no metadata".
    #[allow(dead_code)] // used by `read`, added in the next task
    pub(crate) fn from_json(bytes: &[u8]) -> Result<Metadata, MetadataError> {
        if bytes.is_empty() {
            return Ok(Metadata::default());
        }
        let text = std::str::from_utf8(bytes).map_err(|_| MetadataError::NotUtf8)?;
        let Value::Object(mut object) = parse_rejecting_duplicates(text)? else {
            return Err(MetadataError::NotAnObject);
        };

        // Take each known field out of the object; whatever is left is unknown.
        let mut take = |key: &str| match object.remove(key) {
            None => Ok(None),
            Some(Value::String(text)) => Ok(Some(text)),
            // `null`, numbers, arrays, ...: a known field must be a string when present.
            Some(_) => Err(MetadataError::WrongType {
                key: key.to_owned(),
            }),
        };
        let name = take("name")?;
        let author = take("author")?;
        let license = take("license")?;
        let y_label = take("y_label")?;
        let z_label = take("z_label")?;
        let description = take("description")?;

        Ok(Metadata {
            name,
            author,
            license,
            y_label,
            z_label,
            description,
            extra: object,
        })
    }

    /// Serialises to compact JSON: known fields in spec order (`None` left out), then unknown
    /// keys sorted, also inside nested objects. Empty metadata gives an empty block.
    pub(crate) fn to_json(&self) -> Vec<u8> {
        let present: Vec<(&str, &String)> = self
            .known_fields()
            .into_iter()
            .filter_map(|(key, value)| value.as_ref().map(|text| (key, text)))
            .collect();
        if present.is_empty() && self.extra.is_empty() {
            return Vec::new();
        }

        let mut out = Vec::new();
        out.push(b'{');
        let mut first = true;
        for (key, text) in present {
            write_entry(&mut out, &mut first, key, &Value::String(text.clone()));
        }
        for (key, value) in sorted_entries(&self.extra) {
            write_entry(&mut out, &mut first, key, value);
        }
        out.push(b'}');
        out
    }
}

/// Writes `"key":value`, preceded by a comma unless it is the first entry.
fn write_entry(out: &mut Vec<u8>, first: &mut bool, key: &str, value: &Value) {
    if !*first {
        out.push(b',');
    }
    *first = false;
    write_scalar(out, &Value::String(key.to_owned()));
    out.push(b':');
    write_sorted(out, value);
}

/// Writes any JSON value with the keys of every object sorted. This does not rely on how
/// `serde_json::Map` orders keys, which changes if any crate enables its `preserve_order`
/// feature.
fn write_sorted(out: &mut Vec<u8>, value: &Value) {
    match value {
        Value::Object(object) => {
            out.push(b'{');
            let mut first = true;
            for (key, inner) in sorted_entries(object) {
                write_entry(out, &mut first, key, inner);
            }
            out.push(b'}');
        }
        Value::Array(items) => {
            out.push(b'[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(b',');
                }
                write_sorted(out, item);
            }
            out.push(b']');
        }
        scalar => write_scalar(out, scalar),
    }
}

/// Writes a string, number, bool or null.
fn write_scalar(out: &mut Vec<u8>, value: &Value) {
    // Serialising a `Value` into a `Vec` has no I/O that can fail, and a `Value` holds no
    // non-finite numbers or non-string keys, so serde_json cannot return an error here.
    serde_json::to_writer(out, value).expect("serialising a JSON scalar cannot fail");
}

/// The entries of an object, sorted by key.
fn sorted_entries(object: &Map<String, Value>) -> Vec<(&str, &Value)> {
    let mut entries: Vec<(&str, &Value)> = object
        .iter()
        .map(|(key, value)| (key.as_str(), value))
        .collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    entries
}

/// Parses JSON text into a `Value`, rejecting duplicate keys at every nesting level.
///
/// `serde_json` on its own silently keeps the last of two equal keys, so the parse goes
/// through [`ValueSeed`], which checks each object as it is built. serde's error type cannot
/// carry our own error, so the duplicate key is passed out through a `Cell`.
fn parse_rejecting_duplicates(text: &str) -> Result<Value, MetadataError> {
    let duplicate = Cell::new(None);
    let mut deserializer = serde_json::Deserializer::from_str(text);
    let parsed = ValueSeed {
        duplicate: &duplicate,
    }
    .deserialize(&mut deserializer)
    .and_then(|value| deserializer.end().map(|()| value));
    match (parsed, duplicate.take()) {
        (_, Some(key)) => Err(MetadataError::DuplicateKey { key }),
        (Ok(value), None) => Ok(value),
        (Err(error), None) => Err(MetadataError::InvalidJson(error.to_string())),
    }
}

/// Deserialises one JSON value of any type, recording the first duplicate key it meets.
#[derive(Clone, Copy)]
struct ValueSeed<'a> {
    duplicate: &'a Cell<Option<String>>,
}

impl<'de> DeserializeSeed<'de> for ValueSeed<'_> {
    type Value = Value;

    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for ValueSeed<'_> {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("any JSON value")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Value, E> {
        Ok(Value::from(value))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Value, E> {
        Ok(Value::from(value))
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Value, E> {
        // JSON text cannot express NaN or infinity, so this only fails on a parser bug.
        Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| E::custom("number is not finite"))
    }

    fn visit_str<E>(self, value: &str) -> Result<Value, E> {
        Ok(Value::String(value.to_owned()))
    }

    fn visit_string<E>(self, value: String) -> Result<Value, E> {
        Ok(Value::String(value))
    }

    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element_seed(self)? {
            items.push(item);
        }
        Ok(Value::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut object = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if object.contains_key(&key) {
                self.duplicate.set(Some(key));
                return Err(de::Error::custom("duplicate key"));
            }
            let value = map.next_value_seed(self)?;
            object.insert(key, value);
        }
        Ok(Value::Object(object))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_block_is_empty_metadata() {
        assert_eq!(Metadata::from_json(b""), Ok(Metadata::default()));
        assert_eq!(Metadata::default().to_json(), b"");
    }

    #[test]
    fn known_fields_are_written_in_spec_order_before_sorted_unknown_keys() {
        let metadata =
            Metadata::from_json(br#"{"zz":1,"description":"d","aa":{"b":2,"a":1},"name":"n"}"#)
                .unwrap();
        assert_eq!(
            String::from_utf8(metadata.to_json()).unwrap(),
            r#"{"name":"n","description":"d","aa":{"a":1,"b":2},"zz":1}"#
        );
    }

    #[test]
    fn empty_string_is_written() {
        let metadata = Metadata {
            name: Some(String::new()),
            ..Metadata::default()
        };
        assert_eq!(metadata.to_json(), br#"{"name":""}"#);
    }

    #[test]
    fn known_fields_are_written_in_spec_order() {
        let metadata = Metadata {
            description: Some("d".to_owned()),
            name: Some("n".to_owned()),
            z_label: Some("z".to_owned()),
            ..Metadata::default()
        };
        assert_eq!(
            String::from_utf8(metadata.to_json()).unwrap(),
            r#"{"name":"n","z_label":"z","description":"d"}"#
        );
    }

    #[test]
    fn characters_are_escaped() {
        let metadata = Metadata {
            name: Some("\"\\\n\u{0}é".to_owned()),
            ..Metadata::default()
        };
        assert_eq!(
            String::from_utf8(metadata.to_json()).unwrap(),
            r#"{"name":"\"\\\n\u0000é"}"#
        );
    }

    #[test]
    fn duplicate_keys_are_rejected_at_any_depth() {
        let top = Metadata::from_json(br#"{"license":"CC0-1.0","license":"x"}"#);
        assert_eq!(
            top,
            Err(MetadataError::DuplicateKey {
                key: "license".to_owned()
            })
        );
        let nested = Metadata::from_json(br#"{"tool":[{"a":1,"a":2}]}"#);
        assert_eq!(
            nested,
            Err(MetadataError::DuplicateKey {
                key: "a".to_owned()
            })
        );
    }

    #[test]
    fn known_field_must_be_a_string() {
        for json in [r#"{"name":null}"#, r#"{"name":5}"#, r#"{"name":["a"]}"#] {
            assert_eq!(
                Metadata::from_json(json.as_bytes()),
                Err(MetadataError::WrongType {
                    key: "name".to_owned()
                }),
                "{json}"
            );
        }
    }

    #[test]
    fn invalid_blocks_are_rejected() {
        assert_eq!(
            Metadata::from_json(&[0xff, 0xfe]),
            Err(MetadataError::NotUtf8)
        );
        assert!(matches!(
            Metadata::from_json(b"{\"name\":"),
            Err(MetadataError::InvalidJson(_))
        ));
        assert!(matches!(
            Metadata::from_json(b"{} {}"),
            Err(MetadataError::InvalidJson(_))
        ));
        assert_eq!(
            Metadata::from_json(b"[1,2]"),
            Err(MetadataError::NotAnObject)
        );
    }
}
```

- [ ] **Step 6: Run the tests and see them pass.** `cargo test -p tuw3 --lib metadata` — 8 pass.

- [ ] **Step 7: Run all checks.**

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
Expected: no formatting changes, no clippy output, all tests pass.

- [ ] **Step 8: Commit.**

```sh
git add Cargo.lock crates/tuw3 docs/SPEC.md docs/DECISIONS.md
git commit -m "feat(tuw3): parse metadata and reject duplicate keys" -m "SPEC.md: keys must be unique at every nesting level (D12). No version bump."
```
(End the message with the `Co-Authored-By` trailer when an agent commits.)

### Task 5: `read`

Serves: AC1 (reader half), AC7, AC8, AC9, review focus 1–3.

**Files:**
- Create: `crates/tuw3/src/read.rs`, `crates/tuw3/tests/rejection.rs`
- Modify: `crates/tuw3/src/lib.rs`, `crates/tuw3/src/metadata.rs`, `crates/tuw3/tests/spec_conformance.rs`

**Interfaces:**
- Consumes: `check_sizes`, `Table::new` (Task 2), `layout::*` (Task 3), `Metadata::from_json` (Task 4),
  `RawFile` (Task 3).
- Produces: `pub fn tuw3::read(bytes: &[u8]) -> Result<Table, ReadError>`.

- [ ] **Step 1: Write the failing tests.** Create `crates/tuw3/tests/rejection.rs`:

```rust
// SPDX-License-Identifier: GPL-3.0-or-later

//! `read` rejects every case in *Reading a file* of `SPEC.md` (AC7), forged headers (AC8) and
//! duplicate keys (AC9).

mod common;

use common::RawFile;
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

// Review focus: hostile or unusual metadata must give an error, never a crash.

#[test]
fn deeply_nested_metadata_is_rejected_without_stack_overflow() {
    let depth = 100_000;
    let json = format!("{{\"x\":{}{}}}", "[".repeat(depth), "]".repeat(depth));
    let mut raw = RawFile::valid();
    raw.metadata = json.into_bytes();
    assert!(matches!(
        read(&raw).unwrap_err(),
        ReadError::Metadata(MetadataError::InvalidJson(_))
    ));
}

#[test]
fn unrepresentable_json_is_rejected() {
    // A lone UTF-16 surrogate cannot be a Rust string; 1e400 does not fit in an f64.
    for json in [&br#"{"x":"\ud800"}"#[..], br#"{"x":1e400}"#] {
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

/// A file's metadata is within 1 MiB, but its canonical form is not (`1e5` is re-written as
/// `100000.0`). The table could not be written back, so `read` reports it instead of
/// accepting a table that `write` cannot store.
#[test]
fn metadata_that_grows_beyond_the_limit_when_canonicalised_is_rejected() {
    let json = format!("{{\"x\":[{}]}}", vec!["1e5"; 260_000].join(","));
    assert!(json.len() <= 1 << 20);
    let mut raw = RawFile::valid();
    raw.metadata = json.into_bytes();
    assert!(matches!(
        read(&raw).unwrap_err(),
        ReadError::Table(TableError::MetadataTooLong { .. })
    ));
}
```

  In `crates/tuw3/tests/spec_conformance.rs`, change the import to
  `use common::{RawFile, assert_same_table, position_table};` and add this line at the end of
  `golden_file_matches_spec_layout`, after `assert_eq!(tuw3::write(&table), expected);`:

```rust
    assert_same_table(&tuw3::read(&expected).unwrap(), &table);
```

- [ ] **Step 2: Run them and see them fail.** `cargo test -p tuw3 --test rejection`
  Expected: compile error, `tuw3::read` not found.

- [ ] **Step 3: Implement.** Create `crates/tuw3/src/read.rs`:

```rust
// SPDX-License-Identifier: GPL-3.0-or-later

//! [`read`]: bytes to [`Table`], following *Reading a file* in `SPEC.md`.

use crate::error::ReadError;
use crate::layout;
use crate::limits::MAX_META_LEN;
use crate::metadata::Metadata;
use crate::table::{Table, check_sizes};
use crate::{FIXED_HEADER_LEN, MAGIC, VERSION};

/// Reads a TUW3 file from its bytes.
///
/// Accepts exactly the files `SPEC.md` allows. The header and the file length are checked
/// before anything sized by the header is allocated, so a forged header cannot make this
/// function reserve more memory than the input itself.
///
/// # Errors
///
/// Returns a [`ReadError`] for every case listed under *Reading a file* in `SPEC.md`.
///
/// # Example
///
/// ```no_run
/// let bytes = std::fs::read("table.tuw3")?;
/// let table = tuw3::read(&bytes)?;
/// println!("{} × {} frames", table.y_size(), table.z_size());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn read(bytes: &[u8]) -> Result<Table, ReadError> {
    // 1. The fixed header must be there.
    if bytes.len() < FIXED_HEADER_LEN {
        return Err(ReadError::TooShort { len: bytes.len() });
    }

    // 2–4. Magic, version, flags.
    let magic = [bytes[0], bytes[1], bytes[2], bytes[3]];
    if magic != MAGIC {
        return Err(ReadError::BadMagic { found: magic });
    }
    let version = u16_at(bytes, 4);
    if version != VERSION {
        return Err(ReadError::UnsupportedVersion(version));
    }
    let flags = u16_at(bytes, 6);
    if flags != 0 {
        return Err(ReadError::UnsupportedFlags(flags));
    }

    // 5–6. Sizes and meta_len within the limits.
    let frame_size = u32_at(bytes, 8);
    let y_size = u32_at(bytes, 12);
    let z_size = u32_at(bytes, 16);
    let meta_len = u32_at(bytes, 20);
    let total = check_sizes(frame_size, y_size, z_size)?;
    if meta_len > MAX_META_LEN {
        return Err(ReadError::MetadataTooLong { len: meta_len });
    }

    // 7. The input must be exactly as long as the header says. After this check every
    //    offset below lies inside `bytes`, and every allocation is bounded by its length.
    let expected = layout::file_len(meta_len, total);
    let actual = bytes.len() as u64;
    if actual < expected {
        return Err(ReadError::Truncated { expected, actual });
    }
    if actual > expected {
        return Err(ReadError::TrailingBytes { expected, actual });
    }

    // 8. Metadata.
    let meta_end = FIXED_HEADER_LEN + meta_len as usize;
    let metadata = Metadata::from_json(&bytes[FIXED_HEADER_LEN..meta_end])?;

    // 9. Padding must be zero.
    let sample_offset = layout::sample_offset(meta_len) as usize;
    if let Some(position) = bytes[meta_end..sample_offset].iter().position(|&b| b != 0) {
        return Err(ReadError::NonZeroPadding {
            offset: (meta_end + position) as u64,
        });
    }

    // 10. Samples; `Table::new` checks that they are finite.
    let samples: Vec<f32> = bytes[sample_offset..]
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();
    Ok(Table::new(frame_size, y_size, z_size, samples, metadata)?)
}

/// The little-endian u16 at `offset`. The caller has checked that it is in bounds.
fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

/// The little-endian u32 at `offset`. The caller has checked that it is in bounds.
fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}
```

  In `crates/tuw3/src/metadata.rs`, delete this line above `from_json` (it is used now):

```rust
    #[allow(dead_code)] // used by `read`, added in the next task
```

  In `crates/tuw3/src/lib.rs`, the block below `#![warn(missing_docs)]` becomes:

```rust
mod error;
mod layout;
pub mod limits;
mod metadata;
mod read;
mod table;
mod write;

pub use error::{MetadataError, ReadError, SizeField, TableError};
pub use metadata::Metadata;
pub use read::read;
pub use table::Table;
pub use write::write;
```

- [ ] **Step 4: Run the tests and see them pass.** `cargo test -p tuw3` — all pass
  (18 in `tests/rejection.rs`).

- [ ] **Step 5: Run all checks.**

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
Expected: no formatting changes, no clippy output, all tests pass.

- [ ] **Step 6: Commit.**

```sh
git add crates/tuw3
git commit -m "feat(tuw3): read and validate TUW3 files"
```
(End the message with the `Co-Authored-By` trailer when an agent commits.)

### Task 6: Round-trip and property tests

Serves: AC4, AC5, AC10, AC11, review focus 4–5. These tests check behaviour that Tasks 2–5
already built, so they are expected to pass straight away. If one fails, that is a real bug:
fix the library code, not the test, and note it under *Notes* below.

**Files:**
- Modify: `crates/tuw3/Cargo.toml`
- Create: `crates/tuw3/tests/round_trip.rs`, `crates/tuw3/tests/properties.rs`

**Interfaces:**
- Consumes: `read`, `write`, `Table`, `Metadata`, `common::{RawFile, position_table, assert_same_table}`.

- [ ] **Step 1: Add the dev-dependency.** `cargo add -p tuw3 --dev proptest@1.11.0`, then
  `cargo deny check` (expected: all ok).

- [ ] **Step 2: Write the round-trip tests.** Create `crates/tuw3/tests/round_trip.rs`:

```rust
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
```

- [ ] **Step 3: Write the property tests.** Create `crates/tuw3/tests/properties.rs`:

```rust
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
```

- [ ] **Step 4: Run them.** `cargo test -p tuw3 --test round_trip --test properties`
  Expected: PASS (8 and 3 tests). proptest runs 256 cases per property by default.

- [ ] **Step 5: Run all checks.**

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
Expected: no formatting changes, no clippy output, all tests pass.

- [ ] **Step 6: Commit.**

```sh
git add Cargo.lock crates/tuw3
git commit -m "test(tuw3): add round-trip and property tests"
```
(End the message with the `Co-Authored-By` trailer when an agent commits.)

### Task 7: Crate documentation and wrap-up

Serves: AC12, and the feature doc's status.

**Files:**
- Modify: `crates/tuw3/src/lib.rs`, `docs/features/13-tuw3-reader-writer.md`

- [ ] **Step 1: Write the crate-level example (it is a doctest).** Replace `crates/tuw3/src/lib.rs` with:

```rust
// SPDX-License-Identifier: GPL-3.0-or-later

//! Reader and writer for **TUW3**, the open 3D wavetable file format.
//!
//! A TUW3 file holds a grid of single-cycle frames along two morph axes (Y and Z).
//! The format is defined in [`docs/SPEC.md`](https://github.com/jongsma/tuw/blob/main/docs/SPEC.md);
//! this crate follows the spec, never the other way round.
//!
//! The spec is still a draft (version 0). Only settled parts are implemented here.
//! Points still open in the spec are tracked as GitHub issues and must not be decided
//! silently in code.
//!
//! This crate deliberately has no DSP, GUI or plugin dependencies, and does no I/O: it turns
//! bytes into a [`Table`] and back, and the caller reads or writes the file.
//!
//! ```
//! let mut metadata = tuw3::Metadata::default();
//! metadata.name = Some("Saw".to_owned());
//! let samples: Vec<f32> = (0..8).map(|x| x as f32 / 4.0 - 1.0).collect();
//! let table = tuw3::Table::new(8, 1, 1, samples, metadata)?;
//!
//! let bytes = tuw3::write(&table);
//! let back = tuw3::read(&bytes)?;
//! assert_eq!(back.metadata().name.as_deref(), Some("Saw"));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod error;
mod layout;
pub mod limits;
mod metadata;
mod read;
mod table;
mod write;

pub use error::{MetadataError, ReadError, SizeField, TableError};
pub use metadata::Metadata;
pub use read::read;
pub use table::Table;
pub use write::write;

/// Magic bytes at the start of every TUW3 file (`SPEC.md`, offset 0).
pub const MAGIC: [u8; 4] = *b"TUW3";

/// The format version this crate reads and writes (`SPEC.md`, offset 4).
pub const VERSION: u16 = 0;

/// Length in bytes of the fixed header fields that precede the metadata block:
/// `magic` (4) + `version` (2) + `flags` (2) + `frame_size` (4) + `y_size` (4) +
/// `z_size` (4) + `meta_len` (4).
pub const FIXED_HEADER_LEN: usize = 24;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magic_is_ascii_tuw3() {
        assert_eq!(&MAGIC, b"TUW3");
    }

    #[test]
    fn fixed_header_len_matches_spec_field_sizes() {
        // Field sizes from the layout table in SPEC.md, in order.
        let fields = [4, 2, 2, 4, 4, 4, 4];
        assert_eq!(fields.iter().sum::<usize>(), FIXED_HEADER_LEN);
    }
}
```

- [ ] **Step 2: Run the doctests.** `cargo test -p tuw3 --doc` — PASS (the crate example runs;
  the `read` and `write` examples compile only).

- [ ] **Step 3: Check AC12.** `cargo tree -p tuw3 -e normal --depth 1` lists only `serde` and
  `serde_json`; `cargo doc -p tuw3 --no-deps` shows no warnings; `cargo deny check` is all ok.

- [ ] **Step 4: Update this feature doc.** Tick AC1–AC12 in the *Spec* section, set
  **Status** to *In progress* (it becomes *Done* when the PR merges), and record anything
  learned under *Notes*.

- [ ] **Step 5: Run all checks.**

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
Expected: no formatting changes, no clippy output, all tests pass.

- [ ] **Step 6: Commit.**

```sh
git add crates/tuw3/src/lib.rs docs/features/13-tuw3-reader-writer.md
git commit -m "docs(tuw3): add crate example and update feature doc"
```
(End the message with the `Co-Authored-By` trailer when an agent commits.)

- [ ] **Step 7: Finish.** Use superpowers:finishing-a-development-branch and choose "open a PR":
  title `feat(tuw3): read and write TUW3 version 0`, body with `Closes #13`, the track (Format),
  "version bump: no", and what was not verified (no files from other tools exist yet; memory use
  was not measured; no `cargo-fuzz`).

---

## Notes

Things learned during implementation; spec changes made along the way.

- **Bounded JSON in the metadata (D13, final review, Oct 2026).** The final review found that
  `read` rejected four kinds of metadata the spec allowed, and reported them as invalid while
  *Size limits* requires "unsupported". Settled by bounding the JSON in `SPEC.md`:
  1. Numbers outside the `f64` range (such as `1e400`) are now invalid: `Metadata(InvalidJson)`.
  2. Nesting deeper than 64 levels is now invalid: `Metadata(NestedTooDeeply)`. The crate checks
     this itself (`limits::MAX_META_DEPTH`), before `serde_json`'s own limit of 128.
  3. Lone UTF-16 surrogate escapes (such as `"\ud800"`) are now invalid: `Metadata(InvalidJson)`.
  4. Re-encoding growth (many `1e5`, written back as `100000.0`, pushing the block over 1 MiB)
     cannot be a tool-independent rule, because it depends on how a tool spells numbers. The
     spec lets a reader report it as unsupported, and `read` returns
     `ReadError::MetadataGrowsTooLong`.
