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
- **UI is built from Radix primitives, never hand-rolled markup**, and never behind a charter API
  of our own — no `<Modal>`, no `<Field>`. A shadcn/ui component's source **copied into the repo
  is allowed** and is not that layer (charter ADR 0037, amended 2026-09-22).
  `docs/ui-primitives.md` says which, why, and what it costs; `docs/design-system.md` says what a
  copy has to satisfy.
- **No colour is written anywhere but `app/src/theme/`.** A theme is a data file; the CSS
  custom properties and xterm's theme object are both generated from it. Semantic tokens only,
  no arbitrary Tailwind values, and a test fails the build on either.
  `docs/design-system.md` says why.

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
gates a PR. It is a report, so the only thing that matters about it is that its red is readable:

- **`baseline` red** — charter-core's own tests do not pass. Nothing else in the run is evidence.
- **`core (N)` red** — shard N did not finish. Its verdict says whether the budget was too small
  (add shards; the crate went from 3,640 mutants to 6,555 in a day in September) or the runner
  went away.
- **`survivors` red** — a change to charter-core that no test notices, and that was NOT there
  before. This is the one to read. The table is on the run's summary page.

`.github/mutants-survivors.txt` is the backlog of survivors already known, and only a survivor
missing from it turns `survivors` red. Adding a line to it is a decision with a reason, not a
way to make a run green — and a mutation that provably cannot change any answer does not go in
it at all: prove it and write it into the source, as `realpath` in `pypath.rs` does.

When the nightly is not clean it keeps one issue in this repo up to date, and closes it when the
nightly is clean again. Five consecutive red nights went unread in September 2026 while fifty
PRs merged past them; that is what the issue is for.
