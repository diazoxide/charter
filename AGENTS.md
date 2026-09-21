# Working in charter-app

Read the spec before changing behaviour: `docs/superpowers/specs/2026-09-17-charter-app.md` in the
charter repo (linked from `README.md`). ADR 0025 there holds the reasons.

## Priorities, in order

1. **Development experience.** Quick to change, quick to check.
2. **Robustness through standard practice.** Use mature, standard tools the way they are meant to
   be used. Never build custom tooling where a standard tool exists.
3. **Speed a person can notice.** Optimise only against the spec's limits.

## Rules that are easy to break

- **The core never depends on Tauri or the UI.** `charter-core` is plain Rust. The app and the
  CLI call into it.
- **Nothing parses harness output to decide anything.** A session's state comes from hooks only.
- **The plane on disk has the Python charter's format.** Never change it here without the
  plane-format spec changing first.
- **No `unsafe`** (`unsafe_code = "forbid"` workspace-wide).
- **UI is built from Radix primitives, never hand-rolled markup**, and never behind a wrapper
  layer of our own. `docs/ui-primitives.md` says which, why, and what it costs.

## Checks

Run what CI runs before pushing (commands in `README.md`). Clippy runs with `-D warnings`.
Tests describe behaviour in their names. A test is only trusted once it has been seen to fail
for the right reason.

Two that have each cost a red `main`:

- **In a React test, a precondition waits with Testing Library's `waitFor`, not `vi.waitFor`.**
  Only the first polls inside `act`, so only the first guarantees React has committed what the
  next line reads. `vi.waitFor` is fine for a closing assertion about something outside React,
  such as which commands the core was sent.
- **A scenario spec asks for a control by its `role`, never by its tag.** `input[type="radio"]`
  is a fact about the markup; `[role="radio"]` is the thing the operator and the screen reader
  get, and it survives whatever draws it. `e2e/opening.ts` holds the picker's selectors so
  there is one copy to change.

Mutation testing runs nightly on `charter-core` only (`.github/workflows/mutants.yml`) and never
gates a PR.
