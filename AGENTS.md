# AGENTS.md

Guidance for AI coding agents (and humans) working in this repository.
Read this before making changes. If anything here conflicts with a task description,
follow this file and point out the conflict.

## Project at a glance

- **TUWE**: wavetable editor (`crates/tuwe`)
- **TUWS**: VST3/CLAP synthesizer plugin, built with NIH-plug (`crates/tuws`, Phase 4)
- **tuw-core**: shared wavetable engine, used by TUWE and TUWS (`crates/tuw-core`)
- **tuw3**: converts between TUW3 file bytes and validated wavetables; no file I/O (`crates/tuw3`)
- **TUW3**: open 3D wavetable file format, specified in `docs/SPEC.md`

Background reading, before larger changes:

- `docs/DECISIONS.md`: settled decisions and why (licence, hosting, stack, format)
- `docs/DESIGN-NOTES.md`: design concepts, DSP pitfalls and research notes (not binding)
- `docs/WORKFLOW.md`: how work flows from issue to merged PR (tracks, feature docs, superpowers)

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

- TUWS standalone (real-time listening, once `crates/tuws` exists): `cargo run -p tuws --features standalone`
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

- `docs/SPEC.md` is the source of truth. The `read`/`write` code in `tuw3` follows the spec,
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
- Points still marked as open in `SPEC.md` (its *Open questions* section) are
  **not decided**. Don't settle them silently in code. Raise them instead.

### Serum-style WAV interop

- Keep import/export of WAV with the `clm` chunk working; it is the 1D compatibility path.
- Don't guess the frame size when a `clm` chunk is present; when it is absent, require it
  explicitly or document the fallback.

## Code style and structure

- Crate dependencies point one way: `tuwe`/`tuws` → `tuw-core` → `tuw3`. Never the reverse.
- Keep `tuw3` free of DSP, GUI and plugin dependencies, and free of file I/O: it only converts
  between TUW3 bytes and tables. `read` and `write` there mean bytes → table and table → bytes;
  the caller moves the bytes to and from disk.
- Keep `tuw-core` free of GUI and plugin dependencies (no `egui`, no `nih-plug`).
  Both must be usable by any tool.
- Errors: use typed errors in `tuw3` and `tuw-core`; reserve `anyhow`-style errors for binaries.
- Public items in `tuw3` and `tuw-core` get doc comments; they are meant to be read and learned from.
- Prefer clear code over clever code. This project exists to share knowledge.

## Working method

Full process: [`docs/WORKFLOW.md`](docs/WORKFLOW.md). The essentials:

1. Pick the track: **S** trivial (branch → PR), **M** change (issue → test first → PR),
   **L** feature (issue → feature doc in `docs/features/<issue>-<slug>.md` → plan → test-first
   tasks → PR), **Format** (as L, plus `SPEC.md` in the same PR).
2. Spec-kit lite: this file is the constitution; one feature doc holds spec, plan and tasks;
   clarify open questions before planning; acceptance criteria become tests.
3. Superpowers skills: write `brainstorming` and `writing-plans` output into the feature doc,
   not the skills' default folders. Always finish with a PR, never a local merge to `main`.
4. Branches `<type>/<issue>-<slug>`, Conventional Commits with the crate as scope, squash merge.
5. Keep changes focused: one concern per PR, no drive-by refactors.
6. Run the commands above before declaring something done.
   Report honestly what you did and did not verify (e.g. "not tested in a DAW").
7. Open questions become `type:question` issues; settled decisions go into `docs/DECISIONS.md`.

## Don't

- Don't add telemetry, network access or update checks.
- Don't commit large binary wavetable sets. Reference datasets (AKWF etc.) by source and license.
- Don't change the license, the SPDX headers or `deny.toml` without explicit maintainer approval.

## Gotchas

Lessons learned: mistakes that actually happened here, so nobody makes them twice. Add an entry
only for a real mistake that is specific to this project, and only with the maintainer's
approval. Remove entries whose cause is gone.

- **Build the docs before a PR.** CI and the commands above don't, so broken intra-doc links
  slip through: run `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`. In `tuw3`,
  link the function as ``[`write()`]``; ``[`write`]`` also matches std's `write!` macro.
- **`tuw3::Metadata` has a private field.** Outside `tuw3`, struct-update syntax doesn't
  compile: start from `Metadata::default()` and set fields. Inside `tuw3` it's the other way
  round: clippy (`field_reassign_with_default`) rejects that pattern, so use
  `Metadata { name: …, ..Metadata::default() }`.
- **Cargo feature unification can change `tuw3`'s metadata parsing.** If any crate in the
  workspace enables `serde_json`'s `arbitrary_precision` or `preserve_order`, `tuw3` parses
  and writes metadata differently without any error. The guarding tests only run with
  `cargo test --workspace`, not `-p tuw3`. After adding a dependency anywhere, check
  `cargo tree -e features -i serde_json` (see `crates/tuw3/Cargo.toml`).
- **A parser can be stricter than the spec.** When a reader rejects input that `SPEC.md`
  allows, either bound the spec (as D12 and D13 did) or report the file as *unsupported*
  (`SPEC.md`, *Size limits*); never call it invalid, and don't document the reader as accepting
  everything the spec allows. Check RFC claims before writing them down: a lone surrogate
  escape *is* valid JSON (RFC 8259 §8.2).
- **DSP prose in `DESIGN-NOTES.md` is untested.** Have a fresh reviewer check DSP claims
  before merging; PR #16 needed seven fixes in PR #17.
