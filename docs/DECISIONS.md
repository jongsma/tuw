# Decision log

Settled decisions for TUW (TUWE, TUWS, TUW3) and the reasoning behind them.
Newest context first within each entry. When a decision changes, keep the old one
and mark it **superseded**, so the history stays readable.

Open technical questions about the file format live in [`SPEC.md`](SPEC.md#open-questions),
not here. Ideas and research notes live in [`DESIGN-NOTES.md`](DESIGN-NOTES.md).

---

## D1. Fully open source, never sold (Oct 2026)

**Decision:** the whole project, including the TUWS plugin, is open source and will not be sold.

**Why:** selling a plugin brings a lot of work (licensing, support, payments, piracy)
that the maintainer doesn't want. The goal is to share knowledge, build together with
interested people, and end up with a synth whose inner workings are fully transparent.

**Supersedes:** the original plan (June 2026) of an open-source editor plus a *commercial*
closed plugin.

## D2. License: GPL-3.0-or-later (Oct 2026)

**Decision:** all code is `GPL-3.0-or-later`; every source file carries an SPDX header.

**Why:** NIH-plug's VST3 bindings are GPLv3, so a VST3 build of TUWS has to be GPLv3
anyway. Using the same license everywhere keeps it simple and keeps derivatives open.

**Still open:** the TUW3 *specification* itself may get a separate permissive license so
proprietary tools can implement the format too (see `SPEC.md`).

## D3. Names (Oct 2026)

- **TUW** = *Totally Useless Wavetables*, the umbrella name and repo name.
- **TUWE** = the editor (`crates/tuwe`).
- **TUWS** = the synth plugin (`crates/tuws`). The crate name `tuws` was free on crates.io.
- **TUW3** = the 3D wavetable file format (magic bytes `TUW3`).

Before the first TUWS release, fix the CLAP ID and VST3 class ID and never change them
afterwards: DAWs use them to find the plugin in saved projects.

## D4. Language and frameworks

- **Rust** for everything.
- **NIH-plug** for the plugin layer (VST3 + CLAP).
- **egui** for the cross-platform GUI.
- **`rustfft`** for spectral work, **`hound`** for WAV I/O.

JUCE was considered but is C++ only. NIH-plug's standalone mode covers what JUCE's
AudioPluginHost would have been used for.

## D5. Workspace and crate names (Oct 2026)

**Decision:** one Cargo workspace with crates named after the TUW family:

| Crate | Role | May depend on |
|---|---|---|
| `tuw3` | TUW3 bytes ↔ table conversion only, no file I/O | std (plus a metadata parser once that is decided) |
| `tuw-core` | Wavetable engine: frames, bilinear morph, analysis, bandlimiting | `tuw3` |
| `tuwe` | The editor (binary, egui) | `tuw-core` |
| `tuws` | The plugin (NIH-plug), Phase 4 | `tuw-core` |

**Why:** the generic name `wavetable-core` didn't match TUW/TUWE/TUWS/TUW3. Splitting the format
into its own crate keeps it dependency-light, so other tools can read and write TUW3 without
pulling in our DSP; it also leaves room to give the format crate the same permissive licence as
the spec later, if that is decided. All names were free on crates.io at the time.

The shared crates exist from the start instead of being extracted later, so roadmap step 3
became "stabilise the `tuw-core` and `tuw3` APIs for TUWS".

**Supersedes:** a single `wavetable-core` crate holding engine and format, extracted in Phase 3.

## D6. Hosting: GitHub main, Codeberg read-only mirror (Oct 2026)

**Decision:** `github.com/jongsma/tuw` is the single main repository (editor, core and plugin).
`codeberg.org/jongsma/tuw` is a read-only mirror, synced by `.github/workflows/mirror.yml`
using a Codeberg access token stored as the `CODEBERG_TOKEN` GitHub secret. Issues and PRs
are disabled on Codeberg.

**Why:** values-wise Codeberg is preferred, but TUWS needs a macOS + Windows build matrix.
GitHub Actions has free macOS/Windows runners; Codeberg (Woodpecker) and GitLab don't, and a
self-hosted macOS runner is extra risk and hassle. The mirror keeps the code available for
people who avoid GitHub.

**Supersedes:** editor on Codeberg + plugin on GitHub (June 2026). The old argument "Codeberg
doesn't fit a commercial plugin" no longer applies since D1.

## D7. Platforms

macOS first, Windows later (low priority). The maintainer has a Mac and an Apple Developer
account, so macOS code signing and notarization are covered.

## D8. An own file format: TUW3

**Decision:** define an open binary format instead of only using WAV.

**Why:** the Serum-style WAV with a `clm` chunk is the de facto wavetable standard, but it is
fundamentally 1D, carries almost no metadata, and readers often have to *guess* the frame size.
A 3D table with two morph axes, axis labels and licence metadata needs its own container.
Serum-style WAV import/export stays as the 1D compatibility path.

The format spec is a first-class Phase 1 deliverable. Draft: magic `TUW3`, explicit
`frame_size` / `y_size` / `z_size`, f32 LE samples, a metadata block (name, author, license,
axis labels). See [`SPEC.md`](SPEC.md).

## D9. Testing layers

1. `cargo test`: headless DSP and format tests (the default; numbers are the spec)
2. NIH-plug standalone (`--features standalone`): main real-time listening loop
3. `pluginval`: DAW compatibility
4. Carla or Reaper: external host checks when needed

## D10. Way of working: superpowers + spec-kit lite (Oct 2026)

The process is written down in [`WORKFLOW.md`](WORKFLOW.md): GitHub issues as backlog, tracks
by size (S/M/L/Format), one feature doc per larger feature in `docs/features/`, test first,
everything through a squash-merged PR, CI (fmt, clippy, test, cargo-deny) on Linux.
The background to that choice follows below.

### `AGENTS.md` as the project's rule set

Project-wide rules for AI agents and humans (real-time safety, licensing, format rules) live in
[`AGENTS.md`](../AGENTS.md); `CLAUDE.md` just imports it. This borrows spec-kit's "constitution"
idea without adopting spec-kit itself, because its per-feature spec/plan/task files would
overlap with the existing agent workflow and confuse contributors (there would be two kinds of
"spec" next to `SPEC.md`).

Instead, a *lite* version: one file per feature (spec, plan and tasks together) in
`docs/features/`, written with the superpowers `brainstorming` and `writing-plans` skills, and
clearly separate from the format spec in `SPEC.md`. Full spec-kit can still be tried on a single,
well-bounded editor feature (3D view, import flow, undo) if the lite version falls short.

## D11. TUW3 version 0 layout (Oct 2026)

**Decision:** the open format questions #1–#5 are settled as follows; the full rules are in
[`SPEC.md`](SPEC.md). The format stays at `version` 0, since no files exist yet.

| Question | Outcome |
|---|---|
| Frame order ([#2](https://github.com/jongsma/tuw/issues/2)) | Z-major: X (samples) fastest, then Y, then Z |
| Alignment ([#3](https://github.com/jongsma/tuw/issues/3)) | 24-byte fixed header; zero padding after the metadata so samples start on a 16-byte boundary |
| Compression ([#5](https://github.com/jongsma/tuw/issues/5)) | None in v0; a u16 `flags` field (must be 0) reserves room for it |
| Size limits ([#4](https://github.com/jongsma/tuw/issues/4)) | `frame_size` 8–65 536, not necessarily a power of two; `y_size`/`z_size` 1–4 096; at most 2²⁸ samples; `meta_len` ≤ 1 MiB |
| Metadata ([#1](https://github.com/jongsma/tuw/issues/1)) | One UTF-8 JSON object; known fields are optional strings; unknown keys ignored and preserved |
| Trailing bytes | Rejected: the header fixes the exact file length |
| Non-finite samples | NaN and ±Infinity are not allowed; readers reject them |

**Why:**

- *Z-major:* every Z-slice is a contiguous 1D wavetable, so Serum-style WAV import/export is a
  plain copy and a reader can lazily load one slice with one seek. Bilinear playback needs two
  adjacent rows whichever axis is outer, so playback doesn't favour either order.
- *Alignment and flags:* a 16-byte aligned sample block allows memory-mapping, zero-copy
  `&[f32]` access, SIMD-friendly loads and lazy loading of large libraries (DESIGN-NOTES §2.5),
  at a cost of at most 15 bytes. A field inserted later would shift the whole header, so
  `flags` is added now; it also makes the fixed header 24 bytes.
- *No compression:* lossless compression of full-band `f32` frames saves comparatively little,
  and it would rule out zero-copy and per-slice lazy loading and force every implementer to
  carry a compression library. On current hardware raw size is acceptable.
- *Limits:* readers must bound allocations from untrusted input; the total-sample cap is the
  real memory guard. Powers of two aren't required so sources such as AKWF (600 samples per
  frame) are stored as they are; `tuw-core` handles any frame size.
- *JSON:* the block lives inside a binary file, so TOML's hand-editing advantage doesn't apply,
  while JSON has a parser in practically every language, which matters for an open format.
- *Trailing bytes and non-finite samples:* data in an unexpected place means corruption, a
  writer bug, an unknown newer format or hidden data; none of these should be accepted
  silently. Rejecting NaN/Inf at load time means a player never has to check samples on the
  audio thread.

## D12. TUW3 metadata: duplicate keys are invalid (Oct 2026)

**Decision:** a metadata block in which any JSON object, at any nesting level, has the same
key twice makes the file invalid. No `version` bump: the format is still at version 0 and no
files exist yet.

**Why:** RFC 8259 only says keys *should* be unique and leaves duplicates to the parser: some
take the first value, some the last, some reject the file. One file could then mean different
things in different tools, for example `"license"` given twice with a free and a non-free
licence. As with trailing bytes (D11), data with two meanings is rejected.

## D13. TUW3 metadata: bounded JSON (Oct 2026)

**Decision:** valid metadata is nested at most 64 levels deep, has only numbers that fit in a
64-bit float, and has no unpaired surrogate escapes; anything else makes the file invalid.
A reader may report a file as unsupported when its re-encoded metadata would exceed 1 MiB.
No `version` bump: the format is still at version 0 and no files exist yet.

**Why:** RFC 8259 allows all of these but lets parsers limit them (§6, §8.2, §9), and common
parsers do: many stop at 128 levels or fewer, read numbers as 64-bit floats and cannot hold a
lone surrogate in a string. Left open, a file could load in one tool and fail in another. As
with duplicate keys (D12), the spec bounds the JSON so that "valid" means the same in every
tool. Numbers and nesting are bounded well past anything a wavetable's metadata needs.
Re-encoding growth cannot be written as a tool-independent rule, because it depends on how a
tool spells numbers, so it uses the existing *Size limits* rule: report as unsupported.
