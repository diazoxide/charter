# Working in charter-app

Read the spec before changing behaviour: [`docs/spec.md`](docs/spec.md). The decisions and their
reasons are in [`docs/adr/`](docs/adr/), starting at ADR 0025; the plane on disk is
[`docs/plane-format.md`](docs/plane-format.md). ADRs 0001 to 0024 are the Python charter's and stay
in `diazoxide/charter-plane` as history (ADR 0044). A new decision is the next number in
`docs/adr/`.

## Priorities, in order

1. **Development experience.** Quick to change, quick to check.
2. **Robustness through standard practice.** Use mature, standard tools the way they are meant to
   be used. Never build custom tooling where a standard tool exists.
3. **Speed a person can notice.** Optimise only against the spec's limits.

## Rules that are easy to break

- **The core never depends on Tauri or the UI.** `charter-core` is plain Rust. The app and the
  CLI call into it.
- **Nothing parses harness output to decide anything.** A run's state moves only by a named cause (ADR 0076): its hooks, or, for a chat at level 3, its structured protocol (ADR 0073); its program's exit; an act of the host, the operator or a policy; or, for a remote chat, what its vendor reports.
- **The plane on disk has the format `docs/plane-format.md` records.** Never change it here
  without that document changing first.
- **A new store names its tier before it ships** (ADR 0069): Plane, Clone state, Machine
  (syncable or device-bound) or Keyring, and rebuildable when it is derived. It goes in
  `docs/plane-format.md`, under its own heading or as a row of a table of paths, with a
  `**Tier:**` line, and
  `crates/charter-core/tests/every_store_the_plane_format_names_has_a_tier.rs` fails until it
  has one. This is part of every feature's definition of done.
- **Nothing depends on the Python charter, shipped or not.** No message, doc page or code path in
  the app or the `charter` binary tells anyone to install or run it, and nothing in CI or the
  tests installs it or contacts `diazoxide/charter-plane`: its answers are frozen into recorded
  fixtures (ADR 0044, ADR 0045, ADR 0046). No Python in the shipped path.
- **A recorded answer changes only on purpose.** `tests/fixtures/recorded/behaviour.jsonl` is
  what the Python charter answered for 404 scenarios, replayed against every build by
  `cargo test -p charter-cli --test recorded_behaviour` (add scenario names after `--` for
  fewer). When a change is meant to move one, re-record it with
  `CHARTER_RECORDED_BLESS=1 cargo test -p charter-cli --test recorded_behaviour -- <name>`, read
  the fixture's diff, and say in the PR which contract moved and why. Never re-record to make a
  red run green without that sentence (ADR 0046).
- **No `unsafe`, with one audited exception** (`unsafe_code = "deny"` workspace-wide). The
  exception is `charter_core::executor::inherit_nothing_else`: the `pre_exec` hook that closes
  every descriptor above 2 in an extension's program, because nothing but code run between
  `fork` and `exec` can. The operator's ruling, 2026-09-23: *"Allow one audited block."* It has
  a `// SAFETY:` comment (clippy's `undocumented_unsafe_blocks` is denied), and
  `crates/charter-core/tests/one_unsafe_block.rs` fails if `unsafe` or an allow of the lint
  appears anywhere else. A second block is a new ruling, not an edit.
- **UI is built from Radix primitives, never hand-rolled markup**, and never behind a charter API
  of our own — no `<Modal>`, no `<Field>`. A shadcn/ui component's source **copied into the repo
  is allowed** and is not that layer (ADR 0037, amended 2026-09-22). **One house set is the
  exception:** the five settings pieces in `app/src/settings/components.tsx` (SettingsLayout,
  SettingGroup, SettingRow, Field, Choice; ADR 0037, amended 2026-10-04, V89f). A sixth piece
  needs a new amendment.
  `docs/ui-primitives.md` says which, why, and what it costs; `docs/design-system.md` says what a
  copy has to satisfy.
- **No colour is written anywhere but `app/src/theme/`.** A theme is a data file; the CSS
  custom properties and xterm's theme object are both generated from it. Semantic tokens only,
  no arbitrary Tailwind values, and a test fails the build on either.
  `docs/design-system.md` says why.

- **Window copy follows `docs/ui-copy.md`.** Sentence case, no stock phrases, and an error says
  what happened and what to do. `app/src/copy.test.ts` fails the build on the rules a machine
  can check; a review reads the rest.

- **A change people would notice gets a changelog fragment, never a CHANGELOG.md edit.** Add
  `changes/<slug>.md` with a `### Added` / `### Changed` / `### Fixed` / `### Security` heading
  and the entry under it, as it will read in CHANGELOG.md (`changes/README.md`). Release prep
  folds the fragments in with `node tools/changelog-fold.mjs`; editing CHANGELOG.md's
  `## [Unreleased]` directly makes every open pull request conflict on it.

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

A TIMEOUT is a **hang**, listed apart and not red, when `--timeout` sits at least half again
above the slowest whole suite of the same shard: a mutant that loops for ever is caught, and no
test could ever retire it. Closer than that, or with no whole suite measured on that shard, a
TIMEOUT stays a survivor, because it may be a suite cut short. Shards run on different runners,
so one shard's suite says nothing about another's: on 2026-09-25 ten slow shards reported 432
TIMEOUTs and no MISSED mutant, and this rule is what keeps such a night red.

When the nightly is not clean it keeps one issue in this repo up to date. Sunday tests one of
13 weekly slices of the crate (whole files), so the issue records which files are not known
clean: a night that is not clean adds the files it tested, a slice that runs clean clears only
its own, and the issue closes, with any copies, only when none is left: a full cycle of clean
slices. A partial or cancelled run never closes it, and a dispatched run (even a full one)
never touches it. Five consecutive red nights went unread in September
2026 while fifty PRs merged past them; that is what the issue is for. Which issue, and what it
is told, is `tools/mutants-report.py notice`, tested in `tools/mutants-report.test.mjs`.

## Agent skills

### Issue tracker

GitHub Issues on `diazoxide/charter`, through `gh`. See `docs/agents/issue-tracker.md`.

### Triage labels

The five default roles, each label named as its role. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: `CONTEXT.md` and `docs/adr/` at the root. See `docs/agents/domain.md`.
