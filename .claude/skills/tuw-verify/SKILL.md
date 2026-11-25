---
name: tuw-verify
description: Run the tuw verification gate (fmt, clippy, tests, docs, cargo deny, SPDX headers, crate direction) and assess the result. Use after every code change and as the last step of every loop item.
---

# tuw-verify

The definition of done from `AGENTS.md` and `docs/WORKFLOW.md` §5, as a reusable step. A change
is only done when this is green. Run it in the checkout you are verifying; compare against the
commit the branch started from (`BASE`, normally `origin/main`).

1. `cargo fmt --all`. Formatting is not an error, but a change to commit along with the rest.
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace` (the whole workspace, never only `-p <crate>`: some guards only run
   under workspace feature unification, see the `serde_json` gotcha in `AGENTS.md`). Note the
   test count.
4. `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`
5. Only if `Cargo.toml` or `Cargo.lock` changed (`git diff --name-only BASE...HEAD`):
   `cargo deny check`, and `cargo tree -e features -i serde_json` (no `arbitrary_precision`,
   no `preserve_order`).
6. Only if `Cargo.toml` changed in a way that could affect the minimum Rust version (new
   dependency, new language feature): `cargo +1.85 check --workspace --all-targets` when that
   toolchain is installed; otherwise say "MSRV not checked locally, CI will".
7. Every source file added since `BASE` starts with `// SPDX-License-Identifier: GPL-3.0-or-later`:
   `git diff --name-only --diff-filter=A BASE...HEAD -- '*.rs' | xargs -r grep -L 'SPDX-License-Identifier: GPL-3.0-or-later'`
   prints nothing.
8. Crate direction (`tuwe`/`tuws` → `tuw-core` → `tuw3`): `tuw3` and `tuw-core` gained no
   dependency on a crate above them, and no `egui` / `nih-plug` / file I/O in `tuw3`.
9. Report briefly:
   - `VERIFY: GREEN (<n> tests)` or `VERIFY: RED — <first error in one line>`
   - which optional steps ran (5, 6) and which did not.

Rules:
- Don't silently fix something outside the item's scope to get the gate green. If it was
  already red on `BASE`, say so.
- Disabling a test, adding `#[allow]`, loosening a float tolerance or an acceptance criterion
  is only allowed when named in the report as a deliberate choice with a reason (and recorded
  as a `Ruling:`).
- What only the ear can judge (TUWS in a host, real-time listening) is never claimed as
  verified: it goes on the batch hand-check list ("not tested in a DAW").
