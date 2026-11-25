---
name: gotcha-sweep
description: Go through the commits since the previous sweep looking for lessons not yet in the AGENTS.md Gotchas, and check whether existing gotchas still hold. Proposes changes for the maintainer to approve. Meant for /loop 2h /gotcha-sweep or the end of a working day.
---

# gotcha-sweep: compounding engineering

Every corrected mistake should become a lasting lesson, so the next session doesn't make it
again. This sweep catches what slipped through. `AGENTS.md` takes a gotcha only for a real
mistake specific to this project and only with the maintainer's approval, so the sweep
**proposes**; it never edits `AGENTS.md` on its own.

1. **Determine the range:** read the hash in `.claude/state/last-gotcha-sweep` (missing: take the
   last 20 commits). Look at `git log --stat <hash>..origin/main` and at the unpushed loop
   branches in `.claude/state/ledger.md`. No new commits: report "nothing to sweep" and stop.
2. **Look for lessons** in those commits:
   - `fix:` commits and PRs that repair an earlier PR (as #17 fixed #16);
   - `blocked` and `question` lines in the ledger and the *Proposed gotcha* entries in
     `.claude/state/batch-report.md`;
   - repeated fixes of the same kind of error;
   - the known classes in `AGENTS.md`: doc-link ambiguity, `Metadata` construction, `serde_json`
     feature unification, parsers stricter than the spec, untested DSP prose.
   Not yet in the Gotchas? Draft it in the existing format (bold one-line rule, what happened,
   the fix) with the commit hash as source.
3. **Check existing gotchas:**
   - Do they name files, functions, features or values that no longer exist? Draft the change or
     the removal (`AGENTS.md`: remove entries whose cause is gone).
   - Are two rules really the same? Draft the merge.
4. **Promote:** does a lesson belong to a fixed workflow (verification, planning, the loop)?
   Propose moving it into the right skill under `.claude/skills/`, leaving a one-line reference.
5. **Wrap up:** write the proposals to `.claude/state/gotcha-proposals.md` (each as the exact text
   to add, change or remove, with its source), and `git rev-parse origin/main` to
   `.claude/state/last-gotcha-sweep`. Nothing is committed.
6. **Report:** number proposed to add / adjust / remove, in one line, and point to the proposals
   file. When the maintainer approves some, apply them on a `docs/gotchas-<date>` branch
   (S track); push only when they say so.
