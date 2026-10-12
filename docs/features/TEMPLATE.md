# <Feature name>

- **Issue:** #<number>
- **Status:** Draft | Clarifying | Planned | In progress | Done
- **Crates:** tuw3 / tuw-core / tuwe / tuws
- **Format change:** no | yes (`SPEC.md` updated, version bump: yes/no)

Copy this file to `docs/features/<issue>-<slug>.md`. See `docs/WORKFLOW.md`, section 2.

---

## Spec

What the feature does and why. No implementation details here.

### Problem

Who needs this, and what can't they do today?

### Behaviour

What the user (or calling code) observes. Include edge cases.

### Acceptance criteria

Each criterion must be checkable by a test or a clearly described manual check.

- [ ] AC1:
- [ ] AC2:

### Out of scope

-

### Open questions

Answer or defer (with an issue) before writing the plan.

-

---

## Plan

How it will be built: affected crates and modules, data structures, real-time considerations,
test strategy. Must not contradict `AGENTS.md`.

---

## Tasks

Small steps, each ending with a green `cargo test`. Reference the acceptance criteria.

- [ ] T1 (AC1): write failing test for …, then implement
- [ ] T2 (AC2):

---

## Notes

Things learned during implementation; spec changes made along the way.
