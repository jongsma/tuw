---
name: spec-item
description: Spec out a new piece of tuw work with the maintainer and make it ready for the loop (issue, feature doc, `loop:ready` label). Use whenever new work should go to the loop ("queue X for the loop", "let's do X next").
---

# spec-item: nothing gets `loop:ready` without a spec

The loop builds on its own, so the spec is where the maintainer's intent gets captured. This part
is done together, never by a loop. It follows `docs/WORKFLOW.md`; the only addition is the
`loop:ready` label at the end.

1. **Track:** pick S, M, L or Format with the maintainer (`docs/WORKFLOW.md` §1; when in doubt,
   the bigger one). Decisions (`type:question`) never go to the loop: they are settled with the
   maintainer and recorded in `DECISIONS.md`.
2. **Issue:** find the existing issue or draft a new one from `.github/ISSUE_TEMPLATE/`, with the
   area, type and phase labels. Show it to the maintainer before `gh issue create`.
3. **Spec:**
   - **M:** testable acceptance criteria in the issue body.
   - **L / Format:** `superpowers:brainstorming` with the maintainer (one question at a time). The
     result goes into the *Spec* section of `docs/features/<n>-<slug>.md`, copied from
     `docs/features/TEMPLATE.md` (never `docs/superpowers/` or `docs/plans/`). Acceptance
     criteria are testable properties; what only the ear can judge goes into a hand check, not
     into the criteria. Every open question is answered or deferred to an issue.
   - **Format:** the exact `SPEC.md` text and the `version` bump decision go into the feature doc
     (as in `docs/features/7-frame-normalisation.md`).
   - Status *Clarifying* until the maintainer approves the doc.
4. **Branch:** L / Format: `git switch -c <type>/<n>-<slug> origin/main`, commit the feature doc
   as the branch's first commit (`docs(<crate>): spec <topic> (#<n>)`), switch back. Never push.
5. **Ready:** the track in the issue body, then, with the maintainer's OK, add the `loop:ready`
   label (`gh label create loop:ready --description "Specced; the Claude loop may build it"` the
   first time). The loop picks it up on its next iteration.

Split work that is really several features into several issues, each with its own acceptance
criteria, in the order the maintainer wants.
