# TUW3 — 3D Wavetable File Format

**Status:** draft, version 0. Everything here may still change.

## Overview

A TUW3 file holds a three-dimensional wavetable: a grid of frames along two morph axes
(Y and Z). A frame is a list of `frame_size` samples, usually one cycle of a waveform; see
[Uses of a table](#uses-of-a-table) for other uses. A player addresses the table with a 2D
coordinate (y, z) and interpolates bilinearly between the four surrounding frames.

TUW3 is created and edited with **TUWE** and played back by **TUWS**, the reference
implementations. The format is meant to be implementable by any tool.

The words **must**, **must not**, **should** and **may** are used as in RFC 2119.

## Axes

- **X** is the sample position within a frame, `frame_size` samples long. What X stands for
  (one cycle, time, frequency) depends on how the table is used, not on the file.
- **Y** is the first morph axis: `y_size` frames.
- **Z** is the second morph axis: `z_size` frames.

A table with `z_size = 1` is a classic 1D wavetable; one with `y_size = z_size = 1` is a single
waveform. Both are valid TUW3 files.

## Layout

All multi-byte fields are little-endian.

| Offset | Size | Field | Description |
|---|---|---|---|
| 0 | 4 | `magic` | ASCII `TUW3` |
| 4 | 2 | `version` | Format version, u16. This document describes version `0`. |
| 6 | 2 | `flags` | Feature flags, u16. Must be `0` in version 0 (see [Flags](#flags)). |
| 8 | 4 | `frame_size` | Samples per frame, u32 |
| 12 | 4 | `y_size` | Number of frames along Y, u32 |
| 16 | 4 | `z_size` | Number of frames along Z, u32 |
| 20 | 4 | `meta_len` | Length of the metadata block in bytes, u32 |
| 24 | `meta_len` | `metadata` | UTF-8 JSON metadata (see [Metadata](#metadata)) |
| 24 + `meta_len` | `pad` | `padding` | Zero bytes up to the next 16-byte boundary |
| `sample_offset` | `frame_size × y_size × z_size × 4` | `samples` | f32 samples |

The fixed header is 24 bytes. The padding and sample offset are:

```
pad           = (16 − (24 + meta_len) mod 16) mod 16      // 0..=15
sample_offset = 24 + meta_len + pad                       // a multiple of 16
file_length   = sample_offset + frame_size × y_size × z_size × 4
```

The padding lets a reader memory-map the file and use the sample block directly as an aligned
`f32` array (zero-copy, SIMD-friendly loads), and load parts of it lazily.

## Flags

`flags` is reserved for optional features that change how the file is read (for example
compression in a later version). In version 0 it must be `0`. A reader must reject a file in
which any flag bit it does not understand is set.

Version 0 has no compression.

## Size limits

| Field | Allowed range |
|---|---|
| `frame_size` | 8 ..= 65 536 |
| `y_size` | 1 ..= 4 096 |
| `z_size` | 1 ..= 4 096 |
| `frame_size × y_size × z_size` | ≤ 2²⁸ (268 435 456 samples, 1 GiB) |
| `meta_len` | 0 ..= 1 048 576 (1 MiB) |

`frame_size` does not have to be a power of two. 2048 is the recommended frame size.

Writers must not produce files outside these limits. A reader may impose lower limits of its
own (for example to bound memory use), but must then report the file as unsupported rather
than fail in any other way.

## Frame order

Frames are stored **Z-major**: X (samples) varies fastest, then Y, then Z. Sample `x` of
frame `(y, z)` is at index

```
index = (z × y_size + y) × frame_size + x
```

in the sample block, i.e. at byte offset `sample_offset + index × 4`. Each Z-slice (all Y
frames for one `z`) is therefore one contiguous block, laid out like a 1D wavetable.

## Samples

Samples are IEEE 754 binary32 (`f32`), little-endian. Every sample must be finite: NaN and
±Infinity are not allowed. Writers must not write them; readers must reject a file that
contains them.

Writers should keep samples within [-1, 1]. Values outside that range are valid, and readers
must accept them.

Nothing else about the samples is required: a frame may have a DC offset (a non-zero mean),
any peak level, and any phases between its harmonics. Readers and libraries must return the
samples exactly as stored and must not normalise, re-centre or otherwise change them while
reading. See [Uses of a table](#uses-of-a-table) for why.

## Uses of a table

A table does not say what it is for. The same file can serve as, for example:

- an **oscillator**: each frame is one cycle of a waveform;
- an **LFO**: each frame is one cycle of a modulation shape, possibly unipolar (0 to 1);
- an **envelope**: each frame is a curve over time with a start and an end;
- a **filter**: each frame describes a gain curve over frequency.

Each use prepares the table in its own way. An oscillator, for example, typically removes DC
from its playback copy, while an LFO, envelope or filter needs the values as stored. How a use
reads a frame (whether it wraps around at the end, how it maps a frame onto frequencies, what
negative values or values above 1 mean) is up to that use and is not part of this format.

Editors may offer to remove DC offsets, align harmonic phases between frames or normalise
levels, but should apply these only when the author asks, since each of them can break a use
other than the one the author has in mind.

## Metadata

The metadata block is either empty (`meta_len = 0`, no metadata) or a single JSON object
([RFC 8259](https://www.rfc-editor.org/rfc/rfc8259)), encoded as UTF-8 without a byte order
mark.

Known fields, all optional:

| Key | Type | Meaning |
|---|---|---|
| `name` | string | Name of the table |
| `author` | string | Author(s) |
| `license` | string | Licence of the table, preferably an [SPDX identifier](https://spdx.org/licenses/) such as `CC-BY-4.0` |
| `y_label` | string | What the Y axis represents |
| `z_label` | string | What the Z axis represents |
| `description` | string | Free text |

- A known field with a value of the wrong type makes the file invalid.
- Keys must be unique within every JSON object, at every nesting level. A file with a duplicate
  key is invalid.
- Readers must ignore unknown keys. Editors should preserve them when re-saving the file.
- RFC 8259 lets JSON parsers set their own limits. So that every reader can parse every valid
  file, the metadata must also stay within these bounds, or the file is invalid:
  - Objects and arrays are nested at most 64 levels deep. The top-level object is level 1,
    so `{"a":[{}]}` is nested 3 levels deep.
  - Every number fits in a 64-bit IEEE 754 float: after rounding to the nearest value, it is
    finite. `1e400` does not fit; `1e-400` rounds to zero and does.
  - Every string is valid Unicode text: a `\uD800`–`\uDFFF` escape must be part of a
    surrogate pair (a high surrogate directly followed by a low one).
- Editors may re-encode the metadata when they re-save a file: whitespace, key order and the
  spelling of numbers can change (`1e5` may become `100000.0`), as RFC 8259 allows. If the
  re-encoded metadata would exceed the 1 MiB limit, a reader may report the file as
  unsupported (see [Size limits](#size-limits)).

Example:

```json
{
  "name": "Vowel cube",
  "license": "CC-BY-4.0",
  "y_label": "Vowel",
  "z_label": "Brightness"
}
```

## Reading a file

A reader must reject the file if any of the following holds:

1. It is shorter than the 24-byte fixed header.
2. `magic` is not `TUW3`, or `version` is not a version the reader supports.
3. `flags` has a bit set that the reader does not understand (in version 0: any bit).
4. A size field is outside the [size limits](#size-limits). The product
   `frame_size × y_size × z_size` must be computed without overflow.
5. The metadata is not valid according to [Metadata](#metadata).
6. A padding byte is not zero.
7. The file length is not exactly `file_length`: it is truncated, or has trailing bytes.
8. A sample is NaN or ±Infinity.

A reader should check the header and the file length before allocating memory for the samples.

## Open questions

Each is discussed in a GitHub issue; outcomes are recorded here and in `DECISIONS.md`.

- A separate, permissive license for this specification so any tool can implement it ([#6](https://github.com/jongsma/tuw/issues/6))
- Whether files may carry precomputed mip levels ([#8](https://github.com/jongsma/tuw/issues/8))
