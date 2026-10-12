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
| `tuw3` | TUW3 reader/writer only | std (plus a metadata parser once that is decided) |
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
