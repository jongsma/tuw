# AGENTS.md

Guidance for AI coding agents (and humans) working in this repository.
Read this before making changes. If anything here conflicts with a task description,
follow this file and point out the conflict.

## Project at a glance

- **TUWE**: wavetable editor (`crates/tuwe`)
- **TUWS**: VST3/CLAP synthesizer plugin, built with NIH-plug (`crates/tuws`, planned)
- **wavetable-core**: shared engine and TUW3 reader/writer (`crates/wavetable-core`)
- **TUW3**: open 3D wavetable file format, specified in `docs/SPEC.md`

Background reading, before larger changes:

- `docs/DECISIONS.md`: settled decisions and why (licence, hosting, stack, format)
- `docs/DESIGN-NOTES.md`: design concepts, DSP pitfalls and research notes (not binding)

Current phase: **Phase 1**, the TUWE editor core and the TUW3 spec. Don't build ahead into later
phases (plugin, CI matrix, algorithmic generation) unless the task asks for it.

## Commands

All of these must pass before a change is considered done:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Other commands:

- TUWS standalone (real-time listening): `cargo run -p tuws --features standalone`
- License/dependency check: `cargo deny check`

## Hard rules

### Licensing

- Every new source file starts with `// SPDX-License-Identifier: GPL-3.0-or-later`.
- New dependencies must have a GPLv3-compatible license (MIT, Apache-2.0, ISC, BSD, MPL-2.0, …).
  Run `cargo deny check` after adding one.
- Never copy code from projects with an incompatible or unknown license.
- Never copy code from proprietary synths or editors.

### Real-time audio safety

These rules apply to everything reachable from an audio callback (`process()` and anything it calls):

- No heap allocation (`Vec::push`, `Box::new`, `String`, `format!`, cloning owned data, …)
- No locks (`Mutex`, `RwLock`), no blocking channels, no syscalls, no file or network I/O
- No logging or `println!` on the audio thread
- No `panic!`, `unwrap()` or `expect()` on values that can fail at runtime; handle the failure explicitly
- Preallocate buffers in `initialize()`/`reset()`, sized for the maximum block size
- Use lock-free mechanisms (atomics, NIH-plug parameter smoothing, ring buffers) for GUI ↔ audio communication
- Guard against denormals and NaN/Inf in feedback paths

If you are unsure whether code runs on the audio thread, assume it does.

### DSP correctness

- Write the test first for DSP code: the expected numeric behaviour is the specification.
- Compare floats with an explicit tolerance and justify it in a comment; never use `==` on floats.
- Prefer tests that check properties (e.g. bilinear interpolation at frame corners returns
  the exact frame; morphing is continuous; output stays within [-1, 1] for normalised input)
  over hard-coded sample dumps.
- Interpolation, FFT and resampling code must handle edge cases:
  - the last frame on an axis
  - `y_size` or `z_size` of 1
  - frame sizes that are not powers of two (unless the spec forbids them)

### TUW3 file format

- `docs/SPEC.md` is the source of truth. The reader/writer in `wavetable-core` follows the spec,
  never the other way round.
- To change the format, update `SPEC.md` first, in the same PR, and state whether the change
  needs a `version` bump.
- All multi-byte fields are little-endian; samples are `f32` LE.
- Readers must validate untrusted input:
  - check the magic bytes and the version
  - bound `meta_len` and the dimensions, and reject sizes that would overflow
    `frame_size × y_size × z_size × 4`
  - reject truncated files
  - never trust declared lengths for allocation without a sane upper limit
- Every format feature needs a round-trip test (write → read → identical).
- Points still marked as open in `SPEC.md` (metadata format, frame order, compression) are
  **not decided**. Don't settle them silently in code. Raise them instead.

### Serum-style WAV interop

- Keep import/export of WAV with the `clm` chunk working; it is the 1D compatibility path.
- Don't guess the frame size when a `clm` chunk is present; when it is absent, require it
  explicitly or document the fallback.

## Code style and structure

- Keep `wavetable-core` free of GUI and plugin dependencies (no `egui`, no `nih-plug`).
  It must be usable by any tool.
- Errors: use typed errors in `wavetable-core`; reserve `anyhow`-style errors for binaries.
- Public items in `wavetable-core` get doc comments; the crate is meant to be read and learned from.
- Prefer clear code over clever code. This project exists to share knowledge.

## Working method

1. For non-trivial changes, check whether an issue exists or describe the approach first.
2. Write or update tests, then implement.
3. Keep changes focused: one concern per PR, no drive-by refactors.
4. Run the commands above before declaring something done.
   Report honestly what you did and did not verify (e.g. "not tested in a DAW").

## Don't

- Don't add telemetry, network access or update checks.
- Don't commit large binary wavetable sets. Reference datasets (AKWF etc.) by source and license.
- Don't change the license, the SPDX headers or `deny.toml` without explicit maintainer approval.
