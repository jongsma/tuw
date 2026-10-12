# Contributing to TUWE & TUWS

Thanks for your interest! TUWE (the editor) and TUWS (the synth plugin) are built in the
open, and every kind of contribution helps: code, DSP ideas, wavetables, documentation,
testing in different DAWs, or bug reports.

## Where to contribute

The main repository is on **GitHub**; please open issues and pull requests there.
The Codeberg repository is a read-only mirror and does not accept issues or pull requests.

## Getting started

1. Read [`docs/WORKFLOW.md`](docs/WORKFLOW.md) to see which track your change takes.
2. Open an issue first for anything beyond a trivial fix, so we can discuss the approach.
3. Fork the repo and create a branch named `<type>/<issue>-<slug>`.
4. Make sure these pass before you open a pull request:
   ```sh
   cargo fmt --all
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   ```

## Testing audio code

- `cargo test`: headless DSP tests (preferred for anything that can be checked numerically)
- TUWS standalone mode (`--features standalone`): real-time listening tests
- `pluginval`: DAW compatibility checks for TUWS
- Carla or Reaper: external host checks when needed

Background on the design and earlier decisions is in `docs/DECISIONS.md` and
`docs/DESIGN-NOTES.md`.

## Dependencies

New dependencies must have a GPLv3-compatible license (MIT, Apache-2.0, ISC, BSD, MPL-2.0,
etc.). CI checks this automatically with `cargo-deny`; see `deny.toml`.

## License of contributions

By submitting a contribution, you agree that it is licensed under the
GNU General Public License v3.0 or later, the same license as the project.

New source files should start with:

```rust
// SPDX-License-Identifier: GPL-3.0-or-later
```
