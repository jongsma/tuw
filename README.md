# TUW: Totally Useless Wavetables

TUW is a family of open-source wavetable tools. **TUWE** is the wavetable editor, and
**TUWS** is the synthesizer plugin built on the same engine. Both are written in Rust. Together they introduce
**TUW3**, an open file format for *3D wavetables*: tables with two independent morph axes
(Y and Z), played back with bilinear interpolation between four neighbouring frames.

A wavetable can play two roles in TUWS: it can drive an **oscillator**, and it can shape a
**filter**, with each frame acting as a spectral envelope, for effects like morphing vowel
formants.

> **Status:** early development (Phase 1). Nothing is usable yet. Ideas and contributions are welcome.

## Goals

- **TUWE**: an editor for creating and editing 3D wavetables
- **TUW3**: an open, documented file format ([`docs/SPEC.md`](docs/SPEC.md)) that any tool can implement
- **TUWS**: a free VST3/CLAP synthesizer plugin (macOS first, Windows later) built on the same engine
- Import/export of Serum-style WAV wavetables for compatibility with existing tools

## Why open source?

TUWE and TUWS are, and will remain, free of charge. The project's goal is to share knowledge
openly, develop the software collaboratively, and deliver a synthesizer whose inner workings
are fully transparent. Everyone is welcome to study, learn from, modify and improve the code.

## Roadmap

1. TUWE editor core and the TUW3 format spec
2. Algorithmic wavetable generation (FFT analysis of existing wavetable libraries)
3. Extract the `wavetable-core` crate
4. TUWS, the VST3/CLAP plugin
5. Cross-platform CI and releases

## Repository layout

| Path | Contents |
|---|---|
| `crates/wavetable-core` | Wavetable engine and TUW3 reader/writer, shared by TUWE and TUWS |
| `crates/tuwe` | TUWE, the editor application |
| `crates/tuws` | TUWS, the synthesizer plugin (planned) |
| `docs/SPEC.md` | TUW3 file format specification |

## Building

```sh
cargo build --workspace
cargo test --workspace
```

## Where to find it

- **GitHub** (main repository): `https://github.com/jongsma/tuw`. Issues, pull requests,
  discussions and releases all happen here.
- **Codeberg** (read-only mirror): `https://codeberg.org/jongsma/tuw`. Automatically kept
  in sync, for those who prefer not to use GitHub.

## Contributing

Please open issues and pull requests on **GitHub**, so everything stays in one place.
See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

TUWE and TUWS are licensed under the **GNU General Public License v3.0 or later**.
See [LICENSE](LICENSE).

You may use, study, change and share them. If you distribute a modified version, it must
also be released under the GPLv3 with its source code.

TUWS is built with [NIH-plug](https://github.com/robbert-vdh/nih-plug), whose VST3
bindings are GPLv3-licensed. This is one of the reasons the whole project uses GPLv3.
