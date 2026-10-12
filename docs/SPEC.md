# TUW3 — 3D Wavetable File Format

**Status:** draft, version 0. Everything here may still change.

## Overview

A TUW3 file holds a three-dimensional wavetable: a grid of single-cycle frames along two
morph axes (Y and Z). A player addresses the table with a 2D coordinate (y, z) and
interpolates bilinearly between the four surrounding frames.

TUW3 is created and edited with **TUWE** and played back by **TUWS**, the reference
implementations. The format is meant to be implementable by any tool.

## Layout (draft)

| Offset | Size | Field | Description |
|---|---|---|---|
| 0 | 4 | `magic` | ASCII `TUW3` |
| 4 | 2 | `version` | Format version, u16 LE |
| 6 | 4 | `frame_size` | Samples per frame, u32 LE |
| 10 | 4 | `y_size` | Number of frames along Y, u32 LE |
| 14 | 4 | `z_size` | Number of frames along Z, u32 LE |
| 18 | 4 | `meta_len` | Length of the metadata block in bytes, u32 LE |
| 22 | `meta_len` | `metadata` | UTF-8 metadata (TOML or JSON, to be decided) |
| … | `frame_size × y_size × z_size × 4` | `samples` | f32 LE samples |

## Frame order

To be specified (proposal: Z-major, then Y, then samples within a frame; see
[#2](https://github.com/jongsma/tuw/issues/2)).

## Metadata

Planned fields: `name`, `author`, `license`, `y_label`, `z_label`, `description`.

## Open questions

Each is discussed in a GitHub issue; outcomes are recorded here and in `DECISIONS.md`.

- TOML or JSON for the metadata block ([#1](https://github.com/jongsma/tuw/issues/1))
- Frame order ([#2](https://github.com/jongsma/tuw/issues/2))
- Alignment of the sample block; endianness is little-endian throughout ([#3](https://github.com/jongsma/tuw/issues/3))
- Size limits and allowed frame sizes ([#4](https://github.com/jongsma/tuw/issues/4))
- Optional compression ([#5](https://github.com/jongsma/tuw/issues/5))
- A separate, permissive license for this specification so any tool can implement it ([#6](https://github.com/jongsma/tuw/issues/6))
- Frame normalisation conventions: phase-canonical, DC-free ([#7](https://github.com/jongsma/tuw/issues/7))
- Whether files may carry precomputed mip levels ([#8](https://github.com/jongsma/tuw/issues/8))
