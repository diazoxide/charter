# Working in charter-app

Read the spec before changing behaviour: [`docs/spec.md`](docs/spec.md). The decisions and their
reasons are in [`docs/adr/`](docs/adr/), starting at ADR 0025; the plane on disk is
[`docs/plane-format.md`](docs/plane-format.md). ADRs 0001 to 0024 are the Python charter's and stay
in `diazoxide/charter` as history (ADR 0044). A new decision is the next number in `docs/adr/`.

## Priorities, in order

1. **Development experience.** Quick to change, quick to check.
2. **Robustness through standard practice.** Use mature, standard tools the way they are meant to
   be used. Never build custom tooling where a standard tool exists.
3. **Speed a person can notice.** Optimise only against the spec's limits.

## Rules that are easy to break

- **The core never depends on Tauri or the UI.** `charter-core` is plain Rust. The app and the
  CLI call into it.
- **Nothing parses harness output to decide anything.** A session's state comes from hooks only.
- **The plane on disk has the format `docs/plane-format.md` records.** Never change it here
  without that document changing first.
- **Nothing shipped depends on the Python charter.** No message, doc page or code path in the
  app or the `charter` binary tells anyone to install or run it. It is allowed only as the
  differential oracle in CI (ADR 0044, ADR 0045).
- **No `unsafe`, with one audited exception** (`unsafe_code = "deny"` workspace-wide). The
  exception is `charter_core::executor::inherit_nothing_else`: the `pre_exec` hook that closes
  every descriptor above 2 in an extension's program, because nothing but code run between
  `fork` and `exec` can. The operator's ruling, 2026-09-23: *"Allow one audited block."* It has
  a `// SAFETY:` comment (clippy's `undocumented_unsafe_blocks` is denied), and
  `crates/charter-core/tests/one_unsafe_block.rs` fails if `unsafe` or an allow of the lint
  appears anywhere else. A second block is a new ruling, not an edit.
- **UI is built from Radix primitives, never hand-rolled markup**, and never behind a charter API
  of our own — no `<Modal>`, no `<Field>`. A shadcn/ui component's source **copied into the repo
  is allowed** and is not that layer (ADR 0037, amended 2026-09-22).
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
