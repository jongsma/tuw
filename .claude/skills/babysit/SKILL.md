---
name: babysit
description: Keep tuw healthy while the maintainer or a loop is working. Runs the gate on main and the loop's item branches, checks CI on main and open PRs, fixes only deterministic small things on loop branches, reports the rest. Meant for /loop 30m /babysit.
---

# babysit: keep the branches green

Small and careful. Meant to run alongside manual work or the `next-item` loop. Never pushes,
never comments on GitHub.

1. **Refresh:** in the main checkout `git status`, `git log --oneline -3`, `git worktree list`,
   `git fetch origin main`.
   - Uncommitted changes in a checkout? Someone is working there (the maintainer, a loop or a
     subagent). **Touch nothing** in that checkout; report "working tree busy, skipped".
2. **CI:** `gh run list --branch main --limit 3` and `gh pr list --json number,headRefName,statusCheckRollup`.
   A red run: `gh run view <id> --log-failed | tail -50`.
3. **`main` is report-only.** Never commit on `main`. Red on `main` or on someone's PR: report the
   first failure and the commit that caused it.
4. **Loop branches** (`done` in `.claude/state/ledger.md`, not pushed yet): in a fresh worktree,
   `tuw-verify` with `BASE=origin/main`. `main` moved and the branch no longer merges cleanly?
   Report it; don't rebase.
5. **Only fix what is deterministic and small,** and only on an unpushed loop branch:
   - formatting (`cargo fmt --all`),
   - a clear clippy warning with an unambiguous fix,
   - a test failing because of a trivial, identifiable cause in the last commit (a renamed
     symbol, a stale literal).
   Everything else (design errors, unclear failures, float tolerances, anything on `main`):
   don't fix, report.
6. After a fix: `tuw-verify`, then one commit `fix(<crate>): <what>` on that branch. Never push.
   Remove the worktree.
7. **Gotcha check:** can the cause happen again? Add a *Proposed gotcha* to
   `.claude/state/batch-report.md`; never edit `AGENTS.md` (gotchas need the maintainer's
   approval).
8. **Report, one line** if all is well: `babysit: green (main <hash>, <n> loop branches)`.
   Otherwise briefly what is red and what you did or didn't do.
