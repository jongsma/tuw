# Way of working

How changes move from idea to `main`. It applies to humans and AI agents alike.
The hard rules (licensing, real-time safety, format rules) are in [`AGENTS.md`](../AGENTS.md);
this file is about *process*.

In short: **GitHub issues are the backlog, `AGENTS.md` is the constitution, a feature doc is
the spec for anything bigger than a fix, tests come first, and everything lands through a PR.**

## 1. Pick the track by size

| Track | When | Steps |
|---|---|---|
| **S: trivial** | Typo, doc wording, comment, dependency bump | branch → PR |
| **M: change** | Bug fix or small change, one crate, behaviour is clear | issue → branch → test first → PR |
| **L: feature** | User-visible behaviour, several steps, or more than one crate | issue → feature doc → plan → test-first tasks → PR |
| **Format** | Anything that changes TUW3 on disk | as L, **plus** `docs/SPEC.md` updated in the same PR and a stated decision on the `version` bump |
| **Decision** | An open question, no code yet | issue labelled `type:question` → discuss → record the outcome |

When in doubt, take the bigger track. Open an issue before you start on M or L work, so the
approach can be discussed before code exists.

## 2. Spec-kit lite

We borrow spec-kit's ideas without its tooling or folder structure.

1. **`AGENTS.md` is the constitution.** No feature doc or plan may contradict it. If a feature
   needs a rule changed, change `AGENTS.md` first, in its own PR.
2. **One file per feature, not three.** An L feature gets one doc,
   `docs/features/<issue>-<slug>.md` (e.g. `docs/features/12-tuw3-reader.md`), copied from
   [`docs/features/TEMPLATE.md`](features/TEMPLATE.md). It holds the spec, the plan and the
   task list.
3. **Spec first, without implementation.** The *Spec* section says what and why: behaviour,
   acceptance criteria that can be tested, what is out of scope, open questions.
4. **Clarify before planning.** Every open question is answered or explicitly deferred (with
   an issue) before the *Plan* section is written.
5. **Plan in small, testable tasks.** Each task names the acceptance criterion it serves and
   ends with a green `cargo test`.
6. **Tests come from the acceptance criteria.** Property tests where possible (see `AGENTS.md`).
7. **The doc is living.** If behaviour changes during implementation, update the spec in the
   same PR. Set the status to *Done* in the PR's last commit (nobody comes back after the merge);
   the doc stays as a record.
8. **Two kinds of spec, kept apart.** `docs/SPEC.md` specifies the TUW3 *file format* for any
   implementer. `docs/features/` describes behaviour of *our tools*. Never mix the two.

The feature doc is committed as the first commit of the PR, so review can start on the spec
before the code exists.

## 3. With the superpowers skills

Agents using the [superpowers](https://github.com/obra/superpowers) skills map them onto the
tracks above. Contributors without them follow the same steps by hand.

| Step | Skill | Notes |
|---|---|---|
| Shape the idea | `brainstorming` | Output goes into the *Spec* section of the feature doc, **not** the skill's default location |
| Plan | `writing-plans` | Output goes into the *Plan* and *Tasks* sections of the same feature doc |
| Isolate the work | `using-git-worktrees` | Optional; a normal branch is fine |
| Build | `test-driven-development` with `executing-plans` or `subagent-driven-development` | Red → green → refactor per task |
| Debug | `systematic-debugging` | Reproduce with a failing test first |
| Before the PR | `verification-before-completion` | Means: the three commands in §5 pass, and you report what was *not* verified |
| Review | `requesting-code-review` | Check against `AGENTS.md` and the acceptance criteria |
| Finish | `finishing-a-development-branch` | Always choose "open a PR", never merge to `main` locally |

**Path override:** superpowers writes specs and plans to its own default folders. In this
repo they belong in `docs/features/<issue>-<slug>.md`. Don't create `docs/plans/`,
`docs/superpowers/` or similar.

## 4. Git and GitHub

**Branches:** `<type>/<issue>-<slug>`, e.g. `feat/12-tuw3-reader`, `fix/31-morph-nan`.
S-track branches without an issue drop the number: `docs/fix-readme-typo`.

**Types:** `feat`, `fix`, `docs`, `spec` (format change), `test`, `refactor`, `chore`, `ci`.

**Commits and PR titles:** [Conventional Commits](https://www.conventionalcommits.org/), with
the crate as scope: `feat(tuw3): read fixed header`, `fix(tuw-core): clamp morph position`,
`docs: add workflow`. PRs are squash-merged, so the PR title becomes the commit on `main`.

**PRs:** link the issue (`Closes #12`), fill in the template, keep one concern per PR.
CI must be green before merging.

**Labels:**

| Group | Labels |
|---|---|
| Area | `area:tuw3`, `area:core`, `area:tuwe`, `area:tuws`, `area:docs`, `area:ci` |
| Type | `type:feature`, `type:bug`, `type:question`, `type:spec-change`, `type:chore` |
| Phase | `phase:1` … `phase:5` (see the roadmap in the README) |

**Handy `gh` commands:**

```sh
gh issue list --label phase:1                     # the current backlog
gh issue create --label type:question,area:tuw3   # raise an open question
gh issue develop 12 --checkout --name feat/12-tuw3-reader
gh pr create --fill                                # then add "Closes #12" to the body
gh pr checks --watch
gh pr merge --squash --delete-branch
```

## 5. Definition of done

- [ ] `cargo fmt --all`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] `cargo deny check` if dependencies changed
- [ ] New source files start with the SPDX header
- [ ] Feature doc updated (L track), `SPEC.md` updated (format track)
- [ ] Settled decisions recorded in `docs/DECISIONS.md`
- [ ] PR description says honestly what was and wasn't verified (e.g. "not tested in a DAW")

## 6. Decisions and open questions

- An open question is an issue labelled `type:question`. Don't settle it silently in code.
- When it is decided, record it in [`DECISIONS.md`](DECISIONS.md) (and in `SPEC.md` if it is
  about the format), link the PR from the issue, and close it.
- Ideas and research that aren't decisions go in [`DESIGN-NOTES.md`](DESIGN-NOTES.md).
