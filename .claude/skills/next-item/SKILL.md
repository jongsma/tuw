---
name: next-item
description: One iteration of the tuw loop. Picks the next `loop:ready` GitHub issue, plans it in its feature doc, builds it test-first with subagents on its own local branch, verifies, drafts the PR description and records it in the batch report. Use in /loop or standalone ("do the next item").
---

# next-item: one issue, one iteration, one local branch ready for a PR

The loop follows `docs/WORKFLOW.md` up to, but not including, the push: every item ends as a
local branch `<type>/<issue>-<slug>` with green checks and a drafted PR description. The
maintainer reads the batch report, then says which branches to push and open as PRs.

Hard limits, every iteration:
- **Never push, never open a PR, never merge, never commit on `main`.**
- **GitHub is read-only for the loop:** `gh issue list/view` only. No labels, comments or
  issue edits; the loop's state lives in `.claude/state/` (git-ignored).
- `AGENTS.md` is the constitution. A plan or ruling that contradicts it is wrong.

Paths: the main checkout is the repo root; each item gets a worktree in `../tuw-wt/<issue>-<slug>`.
State, in the main checkout's `.claude/state/`:
- `ledger.md`: one line per issue the loop has touched: `#<n> <branch> <status> <one-line note>`,
  status `in-progress`, `done`, `blocked`, `question`.
- `batch-report.md`: what the maintainer reads at the end.
- `pr/<n>.md`: the drafted PR description per item.

## 1. Refresh the state (always first)
Never work from what you remember of a previous iteration; read again:
- In the main checkout: `git status`, `git worktree list`, `git branch --list '*/*'`,
  `git fetch origin main`.
- `.claude/state/ledger.md` and `.claude/state/batch-report.md` (missing: create them; the
  report starts with a heading `# Batch report`, the date and `origin/main`'s hash).
- `gh issue list --label loop:ready --state open --json number,title,labels,body`.
- The *Gotchas* section in `AGENTS.md`, the *Open questions* in `docs/SPEC.md`, and the current
  phase in `AGENTS.md`.

A ledger line `in-progress`, or a worktree under `../tuw-wt/` with uncommitted changes? That is an
unfinished iteration. Resume it (the feature doc's task boxes and the branch's commits tell you
where) or block it (step 6) before starting anything new.

## 2. Pick the item
The lowest-numbered open `loop:ready` issue that is not `done`, `blocked` or `question` in the
ledger. It must pass every check below; if one fails, add a `question` ledger line with the
reason in one line and go to the next issue. Never fix a missing spec yourself: specs are made
with the maintainer (`spec-item`).
- Not labelled `type:question` (those are decisions, never loop work).
- Its `phase:` label is the current phase (`AGENTS.md`: don't build ahead).
- It names its track (S, M, L or Format) in the body.
- **M:** the issue body has testable acceptance criteria.
- **L / Format:** a local branch `<type>/<n>-<slug>` exists with `docs/features/<n>-<slug>.md`
  committed, its *Spec* section has testable acceptance criteria, and its *Open questions* are
  all answered or deferred to an issue.
- **Format:** the feature doc gives the exact `SPEC.md` text and the `version` bump decision.
  The loop applies that text; it never writes format text of its own.
- It does not depend on an open `type:question` issue or on a point in `SPEC.md`'s *Open
  questions*.

Add the ledger line `#<n> <branch> in-progress`.

## 3. Plan (no human approval needed)
- Worktree: `git worktree add ../tuw-wt/<n>-<slug> <branch>` (L / Format: the existing branch);
  M: `git worktree add ../tuw-wt/<n>-<slug> -b <type>/<n>-<slug> origin/main`.
- Feature doc already *Planned* (the maintainer approved a plan during `spec-item`): use that
  plan as it is and go straight to the plan review below. Otherwise:
  `superpowers:writing-plans`, written into the feature doc's *Plan* and *Tasks* sections (never
  `docs/plans/` or `docs/superpowers/`). M has no feature doc: keep the plan in the subagent
  ledger only. Set the feature doc's status to *Planned*.
- Plan review: dispatch one fresh reviewer (most capable model) with the spec, the plan and
  `AGENTS.md`: does every acceptance criterion have a task and a test, are tests written first,
  float comparisons with justified tolerances, edge cases (last frame on an axis, `y_size` /
  `z_size` of 1, non-power-of-two frames), real-time rules, crate direction? Fix the plan from
  its findings once. Record every call you made as a `Ruling:` (one line, with what it costs if
  wrong).
- A question only the maintainer can answer (it changes what gets built, or settles an open
  point in `SPEC.md`): don't guess. Set the ledger line to `question` with the concrete question
  in one line, commit the feature doc as it is on the branch, remove the worktree and go to the
  next item.

## 4. Build
- `superpowers:subagent-driven-development` with the plan, in the worktree: an implementer and
  a task reviewer per task, red → green → refactor, the fix loop, then one whole-branch review
  (`superpowers:requesting-code-review`, against `AGENTS.md` and the acceptance criteria) and
  one fix wave.
- Commits on the branch: Conventional Commits with the crate as scope (`feat(tuw3): …`),
  following the task list; each ends with the attribution lines the session gives.
- DSP prose (`DESIGN-NOTES.md`, doc comments that make DSP claims) gets its own fresh reviewer
  before the item is done (`AGENTS.md` gotcha).
- Docs as the workflow asks: the feature doc's task boxes and status; `DECISIONS.md` only for a
  decision the spec already records as settled; `README.md` when a user would notice.

## 5. Verify
Run the `tuw-verify` skill in the worktree with `BASE=origin/main`. Red? Fix (through the
implementer, not yourself) and verify again.

## 6. No progress = stop, don't keep trying
If the same failure comes back **three times** in a row, or the fix loop goes in circles (the
same diff back and forth), or a load-bearing review finding can't be resolved:
- Commit what is there on the branch as `wip(<crate>): <item> (blocked)`; never stash in a
  worktree you will remove.
- Set the ledger line to `blocked` with the failure in one line.
- Remove the worktree (the branch stays).

## 7. Wrap up
- Set the feature doc's status to *Done* and tick its acceptance criteria in the last commit
  (`docs/WORKFLOW.md` §2.7).
- Write `.claude/state/pr/<n>.md` from `.github/pull_request_template.md`: the PR title (the
  squash-merge commit, Conventional Commits, under 70 characters), `Closes #<n>`, the track,
  the checklist ticked only for what `tuw-verify` actually ran, and *Verification* saying what was
  and wasn't checked. End it with the attribution line the session gives for PRs.
- **Gotcha check** (mandatory): did something go wrong or get corrected that a next session would
  also run into? `AGENTS.md` only takes gotchas with the maintainer's approval, so **don't edit
  it**: add a *Proposed gotcha* to the batch report (the rule in one bold line, what happened,
  the commit hash). Otherwise note "gotchas checked: nothing new".
- Append the item to `.claude/state/batch-report.md`: issue and branch, what it does in one or two
  plain sentences, the rulings, the test count, and a hand check when only a person can judge the
  result (listening in a host, the editor's feel).
- Set the ledger line to `done`, remove the worktree (the branch stays).

## 8. Report (2-4 lines)
Issue · branch · VERIFY result · rulings count · gotcha proposed or not · next ready item.

## When the loop should stop
In a self-paced `/loop`, end the loop (no new wakeup) when:
- there is no ready item left (every `loop:ready` issue is `done`, `blocked` or `question`), or
- two iterations in a row blocked an item.
The last report is the batch report from `.claude/state/batch-report.md`, condensed: what was done,
every ruling, the proposed gotchas, the hand checks as short steps, the `question` and `blocked`
lines, and the branches ready to push, each with its command:
`git push -u origin <branch> && gh pr create --head <branch> --title "<title>" --body-file .claude/state/pr/<n>.md`.
When the maintainer has pushed the batch, move the report to `.claude/state/batch-reports/<date>.md`
so the next batch starts fresh.
