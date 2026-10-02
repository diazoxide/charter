# Every performance budget names the job that measures it, and CI holds charter's own cost by regression

**Accepted 2026-10-01** by the dispatcher under the operator's delegation of 2026-10-01 (decision
D-0086), drafted for program-map ticket SC-17 (#685). It follows these of the
operator's rulings:

- **V8:** *"Performance budgets. Q1's targets are stated per RAM class. An idle-CPU budget; polls
  driven by the file watcher and paused while windows are hidden. A free-disk admission guard and
  "clean build outputs of landed pieces". Copy-on-write seeding of build dirs (APFS clonefile,
  reflink) replaces GL-15's shared target. A per-chat "context tax" budget in CI, and a dispatcher
  (one sub-agent or MCP lookup) once a plane has more than about 25 personas. Lazy restore at
  relaunch. `stress.yml` becomes a required check on macOS and Linux, asserting descriptor
  counts. FR-8 covers the X11/tiling-WM case."*
- **Q1:** *"Target: teams, about 200 open chats per machine (about 50 hot, the rest hibernated),
  unbounded through remote hosts, budgets measured in CI."*
- **TS3**, in part: *"a test for every ticket's acceptance"*.
- **V7**, in part: *"FD-5's acceptance includes the ADR 0026 speed limits, run in CI."*
- **O1**, in part: *"The audit log is never sampled."*
- **V25b**, in part: *"Retention defaults are 1 year and 2 GiB."*
- **D-0082a:** *"Q1's "about 50 hot" is the count for the top RAM class."* **D-0082b:** *"No
  public scale number is published until SC-8 and a release measurement confirm it."*
- **X22:** *"Every new ruling must name which concept it belongs to."*

It builds on [ADR 0026](0026-the-apps-stack-is-locked-by-what-m0-measured.md) (the spec's limits
and what M0 measured), [ADR 0068](0068-a-chat-lives-in-charterd-and-the-app-is-its-client.md)
(`charterd`), [ADR 0075](0075-an-audit-entry-is-metadata-in-a-store-of-its-own-and-telemetry-never-reads-it.md)
(the audit's throughput, group commit and disk cap),
[ADR 0076](0076-a-run-moves-only-by-a-named-cause-and-a-chats-state-is-read-from-its-runs.md)
(run states and hibernation), [ADR 0079](0079-search-runs-on-derived-sqlite-indexes-one-per-project-clone-and-one-per-machine.md)
(the search budgets) and, most of all,
[ADR 0082](0082-charter-serves-one-persons-agents-first-and-its-scale-is-a-hot-chat-count-per-ram-class.md)
(the hot and open targets per RAM class, their formula, and what is measured where). It
**amends `docs/spec.md`**, in a section of its own below, and does not amend ADR 0082. SC-1,
SC-4, SC-8, SC-15, SC-16, SC-18, SC-20, FD-5, FD-9, KN-22, KN-30 and #814 (the release scale
run) build on it. Its concept is **Chat**, as ADR 0082's is: the budgets are what charter's chats
cost the device they run on.

**Units.** As in ADR 0082, every memory size is binary.

## Where charter is today

- **`docs/spec.md`'s Limits** are seven felt-speed and memory rows, *"measured on the operator's
  machine, in the scenario harness"*, plus ADR 0082's two scale rows, Open chats and Hot chats. ADR 0026 measured them once, by hand,
  with `tools/bench.mjs`. No CI job runs the bench.
- **`stress.yml` ("fifty tabs")** opens fifty fake-harness tabs for three rounds on macOS and
  Ubuntu. It asserts thread slack after each close (`THREAD_SLACK = 40`) and a round's time, and
  no memory or latency budget. Its own header says it is *"Not a required check"*, and its
  `paths:` filter is why it cannot be one as written.
- **`ci.yml`'s `app builds` job runs `tools/coldstart-linux.sh`** on three Linux desktops. It
  fails when the median of five launches is past 2 s or any launch past 2.5 s. That is the one
  budget CI holds today.
- **Budgets are scattered across records.** ADR 0075 sets the audit's 1,000 entries per second
  and group commit. ADR 0079 sets the search and SessionStart budgets at 50,000 memories. ADR
  0082 sets the hot and open targets. Ticket rows set others: SC-18's idle CPU, SC-20's relaunch,
  FD-5's reattach, KN-30's context tax. No document lists them together, and none says, for
  each, which job measures it.
- **What was measured** is ADR 0082's table (*What was measured, where and when*), and ADR 0026's.
  This record reuses those numbers and measures nothing new.

## What the rulings already settle

| SC-17 asks | Settled by |
|---|---|
| The hot and open targets per RAM class, and the formula behind them | ADR 0082 §3 (V8, D-0082a). This record states every budget at those targets and re-decides none of them |
| charter's own base in that formula, 2 GB | ADR 0082 §3. §2 below splits it into rows and keeps its total |
| What CI can and cannot measure (a fake harness; no harness footprint) | ADR 0082 §5 |
| A target is a budget, never a cap on the operator | ADR 0082 §4 |
| No public scale number until SC-8 and a release measurement confirm it | D-0082b |
| `stress.yml` required on macOS and Linux, asserting descriptor counts | V8 |
| The ADR 0026 speed limits in CI through `charterd` | V7, and SC-16's row: a regression of more than 20% cannot merge |
| An idle-CPU budget, a context-tax budget, lazy restore, a disk-space guard | V8. SC-18, KN-30, SC-20 and GL-15 carry the numbers |
| The audit's throughput, group commit and disk cap | ADR 0075 §7 and §8, V25b |
| The search and SessionStart budgets at 50,000 | ADR 0079 §4 |

What is left for this record: **what a performance budget is**, **the two ways CI checks one and
the one way a release does**, **the rows the spec is missing**, **how a row scales with the RAM
class**, **where the table lives**, and **the words**.

## The decision

**A performance budget is one row: what is measured, its value, its kind, and the job that
measures it. There are three kinds. A *CI absolute* row is a count or a size CI can measure
exactly, or one of the few timings §1 names, and going past it fails the job. A *CI relative* row is a timing or a memory figure
CI's shared runners measure with noise, and it fails the job when it is more than 20% worse than
the last value `main` recorded, through github-action-benchmark. A *release absolute* row is
measured with real harnesses on the operator's machine each release, and a miss is a bug.
Every row is stated at the device's hot target, as charter's own base plus a cost per hot chat and
a cost per hibernated chat, so that each RAM class's totals follow from ADR 0082's formula. The
table of record is in `docs/spec.md`. Every row is a target, never a promise (ADR 0082 §3 and
§4), and never published (D-0082b).**

### 1. Three kinds of row

| Kind | What it suits | Where | What a miss does |
|---|---|---|---|
| **CI absolute** | counts and sizes CI measures exactly: threads, descriptors, bytes per entry, tokens. And, as the one exception below, a timing whose median sits far inside its budget | the row's CI job, on every pull request the job runs for and on every push to `main` | fails the job |
| **CI relative** | timings and memory far enough above a shared runner's noise for a 20% change to mean something | the same | fails the job on a regression (below) |
| **Release absolute** | what needs a real harness, a real display or the operator's own machine: felt speed, the harness footprint, the web content peak, macOS energy | the release scale run, #814, at the operator's machine's RAM class | a bug, filed before the release notes are written. The release is not blocked by it, because a row is a target (ADR 0082 §3) and never a cap (§4) |

**A row may have two kinds.** Most felt-speed rows are *release absolute* against the spec's
value and *CI relative* against `main`, because a shared runner cannot hold a 50 ms keystroke to
the millisecond, and can tell when a change made it 30% worse.

**The one exception: an absolute timing in CI, as a median with a wide margin.** A timing is CI
absolute only when its budget is many times what it measures, so that a shared runner's noise
cannot reach it, and only as the median of the job's own samples, never one sample. It covers
exactly these rows: **L5** (the hook, a budget of 50 ms), **L6** (cold start on Linux, a median of
five against 2 s with a 2.5 s ceiling, as `app builds` already runs it), **T1** and **T2** (rates
of 1,000 a second), **E1** (idle CPU on Linux, SC-18's row), and **ADR 0079 §4's search and index
budgets**. Any other timing is CI relative or release absolute.

**The relative rule, through a standard tool.** CI relative rows are recorded and compared by
**github-action-benchmark** (`benchmark-action/github-action-benchmark`), the way it is meant to be
used:

- each push to `main` appends the row's value to the tool's data on a branch of its own,
  `benchmarks`, in this repo. Only a push to `main` writes it; a pull request's run reads it and
  never holds a token that can write the repository, so a pull request from a fork changes no
  baseline;
- a pull request's run compares its values with the latest on that branch, with
  `alert-threshold: "120%"` and `fail-on-alert: true`, and comments the comparison;
- each value the tool sees is a median of the job's own samples (`bench.mjs` already takes 30), so
  one slow sample is not a regression.

20% is SC-16's number. **A row is relative only when it is far above the runner's noise.** The
first five green runs of a new row on `main` measure that noise. A row whose run-to-run spread
over those five is more than a third of the threshold (more than about 7%) does not gate: it is
recorded as evidence, and its owner ticket either makes it steadier (more samples) or moves it to
release absolute. So the noise limit is a starting value, and those first five runs set each row's
place, once for each operating system.

### 2. The rows

Every row is stated at **the device's hot target** (ADR 0082 §3) with the rest of the **200 open
chats hibernated**, unless the row says otherwise. CI measures at the top class's 50 hot plus 150
hibernated with the fake harness (ADR 0082 §5).

**Memory: charter's own, with no harness.**

| # | What | Budget | Kind and job | Owner |
|---|---|---|---|---|
| M1 | charter's own base: web content peak plus the native side, at 200 open chats, scrollback excluded | **≤ 2 GB** (ADR 0082 §3's base) | release absolute (#814) | SC-1, #814 |
| M2 | the web content process, peak, at the hot target | ≤ 1.5 GB. **At risk, expected to miss:** the only measurement is 1,529 MB, at 12 chats and not at the hot target | release absolute (#814); CI relative (`stress`, once SC-1 adds the field) | SC-1, #814 |
| M3 | the native side (the app and `charterd` together) with no chats | ≤ 384 MB (measured 183 MB on macOS, 311 MB on Ubuntu) | CI absolute and CI relative (`stress`) | SC-8 |
| M4 | the native side's cost of one hot chat, scrollback excluded | ≤ 4 MB (measured 3.1 MB on macOS, 0.12 MB on Ubuntu) | CI absolute (`stress`) | SC-8 |
| M5 | the native side's cost of one hibernated chat | ≤ 1 MB, and no harness process (SC-4's acceptance) | CI absolute (`stress`, with SC-8's 150 hibernated) | SC-4, SC-8 |
| M6 | an idle hidden session's scrollback at the shipped cap | ≤ 50 MB (the spec; measured 20.2 MB, ADR 0026) | release absolute (`bench.mjs`); CI relative (`bench`, SC-16) | SC-16, SC-5 |
| M7 | no leak across rounds: after the first round, each later round's close leaves memory within 32 MB of the first round's close, and threads within 40 of the base | **passes today**: on macOS, 334.6 MB with all closed after round 1 against a 182.6 MB base, then flat in rounds 2 and 3; on Ubuntu, 321.7 MB after round 3; threads back at 45 | CI absolute (`stress`): threads asserted today; the memory half is SC-8's to add | SC-8 |

**M1 is ADR 0082's base, and it binds.** The parts' ceilings add to more than it: M2 + M3 +
150 × M5 + 50 × M4 is 1.5 GB + 384 MB + 150 MB + 200 MB, about 2.2 GB. They cannot all be at
their ceilings at once, and M1 is the row that says so. At the loads measured so far the same sum
is about 1.8 GB (ADR 0082's 1,529 MB at 12 chats and 337 MB at 50), and M2 at the hot target
is not yet known: it is the row most likely to break M1 (*At risk* above). A change that pushes the measured M1 past 2 GB moves ADR
0082's table, which is recomputed from its formula (ADR 0082 §3). This record does not raise the
2 GB.

**Counts.**

| # | What | Budget | Kind and job | Owner |
|---|---|---|---|---|
| C1 | threads | ≤ 64 at no chats; ≤ 5 per hot chat; none per hibernated chat (measured 49 and 24 at none; 3.9 and 4.0 a chat) | CI absolute (`stress`) | SC-8 |
| C2 | descriptors | ≤ 64 at no chats; ≤ 4 per hot chat; none per hibernated chat (measured 54 at none, 3 a chat on Ubuntu) | CI absolute (`stress`), on macOS once SC-15 counts them there (V8) | SC-8, SC-15 |
| C3 | the open-file limit the app and `charterd` raise to | `min(hard, OPEN_MAX)`, and 200 fake chats open in a launchd-started macOS app | CI absolute (`stress`, macOS) | SC-15 |
| C4 | hook processes spawned per second at the busy load | recorded, no budget | evidence only (`stress`) | SC-8 |

**CPU and energy.**

| # | What | Budget | Kind and job | Owner |
|---|---|---|---|---|
| E1 | idle: window hidden, 3 projects open, no chats | ≤ 0.5% of a core and ≤ 1 wakeup a second (SC-18's row) | CI absolute on Linux (`stress`); release absolute on macOS by `powermetrics` (#814) | SC-18 |
| E2 | idle with chats: window hidden, the hot target's chats all at a turn boundary, harnesses excluded | ≤ 1% of a core, an initial value | CI relative (`stress`); release absolute (#814) | SC-18 |

**Felt speed** (the spec's limits, unchanged in value).

| # | What | Budget | Kind and job | Owner |
|---|---|---|---|---|
| L1 | keystroke to screen, while the hot target minus one other chats stream | ≤ 50 ms | release absolute (`bench.mjs`, #814); CI relative (`bench`) | SC-16 |
| L2 | tab or pane switch | ≤ 100 ms | the same | SC-16 |
| L3 | 2 MB and 13 MB output bursts | the UI never freezes; input and other panes stay responsive | the same, on the burst's longest frame and the keystroke beside it | SC-16 |
| L4 | synchronized-output animation | smooth, ≥ 30 fps (measured 52.4 and 52.0 draws a second against a 60 fps display, ADR 0026) | release absolute (`bench.mjs`) | SC-16 |
| L5 | a hook call (`charter hook …`), p95, at 50,000 memories and the hot target | ≤ 50 ms. **Not yet measured** at that load | CI absolute, the exception in §1 (`stress`, KN-22's fixture) | KN-22 |
| L6 | cold start to the first frame, no chats | ≤ 2 s | CI absolute on Linux (`app builds`: median of five ≤ 2 s; at most one past 2.5 s, reported and not gated; V39); release absolute on macOS (#814) | FR-8 |
| L7 | reattach after the window restarts with `charterd` up: first paint of the focused pane, with the hot target's chats | ≤ 1 s (V7, FD-5's row) | CI relative (`bench`, through the host); release absolute (#814) | FD-5, FD-7 |
| L8 | relaunch with the hot target's chats to put back: interactive | ≤ 3 s (SC-20's row) | CI relative (`bench`); release absolute (#814) | SC-20 |

**The event log, the audit and disk.**

| # | What | Budget | Kind and job | Owner |
|---|---|---|---|---|
| T1 | event log throughput | ≥ 1,000 events a second sustained, with L5 still inside its budget | CI absolute (`stress`) | FD-9 |
| T2 | the audit's throughput and group commit | 1,000 entries a second, at most 100 ms between commits (ADR 0075 §7) | CI absolute (`stress`) | AU-3 |
| D1 | bytes written per event-log event, and per audit entry | ≤ 1 KB, and ≤ 512 B (ADR 0075's estimate is about 400 B), before compression | CI absolute (`stress`) | FD-9, AU-3 |
| D2 | charter's own disk writes over a busy day at the top class: 50 hot chats at 0.3 tool calls a second for 8 hours | ≤ 100 MB compressed, the event log and the audit together | CI absolute, computed from D1 and T1's run (`stress`) | FD-9 |
| D3 | a hibernated chat's scrollback snapshot on disk | ≤ 1 MB, an initial value | CI absolute (`stress`) | SC-4 |
| D4 | every store that grows is bounded | the audit: 1 year and 2 GiB (V25b). The event log: the retention FD-24 states (ADR 0066), which this record does not set. `.charter/sessions`, traces and reports: SC-7's rule. Indexes: rebuildable (ADR 0079) | CI absolute (`rust`: each store's prune is tested) | FD-24, SC-7 |

The disk-space guard (GL-15: a warning when less than 20 GB is left, and no new branch folder
when less than 5 GB is) is a guard, not a budget. It is listed in the spec beside these rows and keeps GL-15's numbers.

**Context.**

| # | What | Budget | Kind and job | Owner |
|---|---|---|---|---|
| K1 | the context tax: tokens charter injects at a chat's start (briefing, agent files, MCP tool schemas, packs), per harness | ≤ 3,000 on the 50,000-memory, 1,000-persona fixture (KN-30's row) | CI absolute (`stress`, on KN-22's fixture) | KN-30 |

**The formula's inputs** (ADR 0082 §3 asked SC-17 for a row naming them).

| # | What | Value used now | Kind and job | Owner |
|---|---|---|---|---|
| F1 | each harness's memory footprint, per process, with its child runs | Claude Code 385 MB (ADR 0082, by `ps` on the operator's machine). Codex and opencode not yet measured | release absolute (#814) | SC-1, #814 |
| F2 | the scrollback term: one hot chat's scrollback at the shipped cap | 20.2 MB (ADR 0026), the measured value of M6 | release absolute (#814) | #814 |
| F3 | charter's own base | M1 | release absolute (#814) | #814 |

When #814 records a new F1 or F3, ADR 0082 §3's table is recomputed from its formula with the
largest measured F1, as that record says. These rows have values, not budgets: they are what
the targets are computed from.

**By reference.** ADR 0079 §4's six search and index budgets are rows of this table as they
stand, with the jobs that record names.

### 3. A row and the RAM class

**A row is stated at the device's hot target, and CI measures the top class.** A count or a size
is stated as a base plus a cost per hot chat and a cost per hibernated chat (C1, C2, M3 to M5).
That is the shape of ADR 0082's formula, so each class's totals are arithmetic and need no run on
a machine of that class. At today's ceilings:

| RAM class | Hot target (ADR 0082) | Descriptors (C2) | Threads (C1) | Native side, scrollback excluded (M3 + M4 + M5) |
|---|---|---|---|---|
| under 8 GB | 1 | ≤ 68 | ≤ 69 | ≤ 587 MB |
| 8 GB | 5 | ≤ 84 | ≤ 89 | ≤ 599 MB |
| 16 GB | 15 | ≤ 124 | ≤ 139 | ≤ 629 MB |
| 32 GB | 35 | ≤ 204 | ≤ 239 | ≤ 689 MB |
| 64 GB | 50 | ≤ 264 | ≤ 314 | ≤ 734 MB |

**A timing row holds at each device's own hot target.** The spec already says so (as ADR 0082
amended it). CI measures it at 50 on a shared runner. The release scale run measures it at the
operator's machine's class. The smaller classes' timings are not measured on a machine of that
class: a report from one is the signal (see *What this costs*).

**A row that does not scale** says so: the hook, cold start, idle with no chats, the context tax,
bytes per entry.

### 4. Where the table lives

**`docs/spec.md` is the table of record.** A new section, *Performance budgets of record*,
follows *Limits (acceptance)*. It holds every row of §2 with its kind, its job and a *Last
measured* column. The release scale run (#814) fills that column in its release PR, because CI
artifacts expire. When this record and the spec disagree, the spec is right: this record's §2 is
a snapshot taken on 2026-10-01, as ADR 0069's table of stores is.

**A row is added in the pull request that adds its job.** A ticket that adds a budget adds its
row to the spec with its kind, job and owner, or it has no budget (TS3).

**Nothing in this record is published.** No row's value goes into public copy until SC-8 and a
release measurement confirm it (D-0082b).

### 5. What becomes of the jobs

- **`stress` becomes required on macOS and Linux** (V8). SC-8 makes it run on every pull request
  with no `paths:` filter, as its own header says it must, and asserts every CI row whose job is
  `stress`. Until SC-8 lands it stays evidence only.
- **`bench` is SC-16's new job in `ci.yml`**: `tools/bench.mjs` on a shared runner, in relative
  mode, for the rows whose job is `bench`. It is required on pull requests that touch the session
  layer (SC-16's row).
- **`app builds`** keeps the Linux cold start (L6) as it is.
- **`rust`** holds D4's prune tests, like any unit test.
- **The release scale run** (#814) is a script on the operator's machine, a step in the release
  checklist, and the *Last measured* column.

**Stores.** This record adds no store. The relative baselines are github-action-benchmark's data
on this repo's `benchmarks` branch, written by CI on pushes to `main`: tier **None**, a branch of
the repo and not a store charter writes, as `stress.jsonl` is a CI artifact (ADR 0082 §5). The *Last
measured* column is text in `docs/spec.md`, a document of this repo and not a store charter
writes.

## `docs/spec.md`, amended

- **The Limits section's opening line** (*"Measured on the operator's machine, in the scenario
  harness"*) now reads: *each limit is also a row of the Performance budgets of record below,
  which says which job measures it.* The limits' values are unchanged.
- **A new section, *Performance budgets of record*,** holds §2's rows, the relative rule, the
  per-class shape (§3) and GL-15's guard, each marked with this record's number.

These edits are in this PR.

## What changes where

The code does not change with this record.

| Where | What changes |
|---|---|
| `docs/spec.md` | The Limits line and the new section (in this PR) |
| `CONTEXT.md` | Gains **Performance budget** (in this PR) |
| `stress.yml` | SC-8: required on macOS and Linux, no `paths:` filter, the `stress` rows asserted, its relative rows through github-action-benchmark |
| `ci.yml` | SC-16: the `bench` job and its rows, through github-action-benchmark. The `benchmarks` branch, written only from `main` |
| `stress.jsonl` | SC-1: web content and harness memory. SC-15: descriptors on macOS. C4's spawn count |
| #814 | The release scale run: its script, its checklist step and the *Last measured* column |
| FD-9 | T1, D1, D2 |
| FD-24 | The event log's retention, which D4 requires to exist (ADR 0066) |
| SC-4, KN-22, KN-30, SC-18, SC-20, FD-5 | Each puts its row's test in its row's job |

## What this costs

- **Most felt speed is held only relatively in CI.** A change can make keystrokes 15% slower on
  each of several pull requests and pass each one, since each is compared with the last value
  `main` recorded. The tool's chart on the `benchmarks` branch shows the drift, and the release
  run finds the absolute miss, a release later at worst.
- **A noisy row does not gate.** A row whose first five runs spread more than about 7% is evidence
  until its owner makes it steadier.
- **A smaller machine's timings are never measured by charter.** An 8 GB laptop is held to the
  same limits at 5 hot chats by arithmetic and by reports.
- **`stress` and `bench` cost runner minutes on every pull request** once required: about sixteen
  runner-minutes for `stress` today (its header), and `bench`'s cost is SC-16's to measure.
- **The release run needs the operator's machine and time** each release.
- **Two kinds on one row is two numbers to keep.** The spec carries the absolute value; the
  relative one is implicit in the rule.

## What was rejected

- **Absolute budgets for most timings in CI.** Shared runners vary more than the margins of the
  50 ms and 100 ms limits. P3-01 proposed relative mode, and SC-16 adopted it. §1's exception
  keeps only timings whose budget is many times their median.
- **Relative budgets for counts.** Threads and descriptors are exact. A relative rule would let a
  leak of three descriptors a chat through.
- **Custom artifact diffing** (a script that downloads `main`'s last five artifacts and compares
  medians with a floor). It is what the first draft proposed. CLAUDE.md's second priority is
  *"Never build custom tooling where a standard tool exists"*, and github-action-benchmark is the
  standard GitHub Actions tool for exactly this: it stores each `main` value, compares a pull
  request with it at a stated threshold, fails the job and comments. What the custom script had
  and the tool lacks, a median of several baselines and an absolute floor, is replaced by taking
  each value as a median of the job's own samples and by gating only rows that are steady (§1).
- **Bencher.** It is a mature continuous-benchmarking tool with statistical thresholds over a
  window of runs, which would give back the median of several baselines. It needs Bencher Cloud, an
  outside service holding the results, or a Bencher server to run, for what one branch in this
  repo does. It is the next step if github-action-benchmark's single baseline proves too noisy
  (*Later decisions*).
- **A committed baseline file.** Someone would have to refresh it, and it would drift from what
  `main` measures.
- **Blocking a release on a release-absolute miss.** A row is a target (ADR 0082 §3) and never a
  cap (ADR 0082 §4), and a miss with a filed bug is more useful than a held release.
- **Restating ADR 0082's targets per row.** The rows are stated at the hot target, and the
  targets stay ADR 0082's alone.
- **The table in this record only.** A record is not edited after it is accepted, and the table
  changes with every ticket that adds a job.

## Decided in drafting

Each of these is inside the rulings above and easy to change later, so it is decided here, with
its reason:

1. **Three kinds of row**, and a row may have two. Counts suit CI absolute, timings CI relative,
   and what needs a real harness the release run.
2. **The relative rule is github-action-benchmark's: more than 20% worse than `main`'s last value
   fails the job.** 20% is SC-16's number, and the tool is the standard one (CLAUDE.md's second
   priority).
3. **A row gates only when steady: a spread of at most about 7% over its first five green runs on
   `main`.** It is a starting value. Those first five runs set each row's place, per OS, and no
   noise data exists yet to set it otherwise.
4. **Absolute timings in CI only as a median far inside the budget**: L5, L6, T1, T2, E1 and ADR
   0079's rows.
5. **Every row is stated at the hot target, as a base plus per-hot and per-hibernated costs.**
   It matches ADR 0082's formula, so classes are arithmetic.
6. **M1 to M5 split ADR 0082's 2 GB base without raising it.**
7. **The initial values: M3 384 MB, M4 4 MB, M5 1 MB, C1 and C2 64 at none, 5 and 4 a hot chat,
   E2 1% of a core, D1 1 KB and 512 B, D2 100 MB a day, D3 1 MB.** Each is above today's
   measurement where one exists, and each owner ticket may lower it. M2 keeps 1.5 GB and is marked
   at risk rather than raised, because ADR 0082's 2 GB base has no room for more. The event log's
   retention is FD-24's (ADR 0066), and is not set here.
8. **The spec is the table of record**, with a *Last measured* column filled by the release run.
9. **A release-absolute miss is a bug filed before the release notes, not a block** (ADR 0082 §3
   and §4).
10. **The release scale run is a ticket of its own (#814)**, filed with this PR, as ADR 0082 §5
    asked.

## For the operator's ruling

None. Every point is inside V8, Q1, V7, ADR 0082 and the rows of the tickets this record cites.

## Decided (dispatcher, 2026-10-01)

- **D-0086:** *"Accepted under the delegation, with no major question. Relative timing rows use
  github-action-benchmark: a row gates only after five green runs on main spread by at most about
  7%, and fails a PR more than 20% worse than main's last value. Writes only come from main.
  Retention is left to FD-24. M2 is marked at risk; #814 measures it."*

## Later decisions

- **Lowering the initial values** once each owner ticket has measured its row.
- **Bencher**, if github-action-benchmark's single baseline proves too noisy for the rows that
  should gate.
- **A flood row** (ten panes printing at full speed, a keystroke in an eleventh), which SC-12's
  measurement decides.
- **`charter doctor --perf`**, showing these figures to the operator on their own machine, which is
  an OB ticket's.

## ADR 0086, amended (2026-10-02)

**Settled by V39:** *"the Linux cold-start gate ignores one outlier. It amends ADR 0086 row L6.
The median of five must still be ≤ 2 s. At most one launch may go past the 2.5 s ceiling, and
that one is reported, not gated. Two or more launches past the ceiling fail. Why: two red mains in
one day, each from a single slow runner launch (2687 ms, 2537 ms) with medians of 1373 and
1560 ms."*

- **Row L6** now reads: median of five ≤ 2 s; at most one launch past 2.5 s, reported and not
  gated. Two or more past it fail the job.
- **§1's exception is unchanged in kind.** L6 is still held by its median. The ceiling stays as a
  second check that catches a slow tail of two or more launches, which a median of five can hide.
- **Where it lives:** `tools/coldstart-gate.mjs` decides, `tools/bench.mjs` prints the median,
  the worst launch and the ceiling, and names the launch it let through. Its tests are
  `tools/coldstart-gate.test.mjs`, run by the `web` job.
- **Without `--ceiling`**, `bench.mjs` still holds every launch to the limit, as before.

## ADR 0086, amended for SC-16 (2026-10-02)

**Settled by V62:** *"ADR 0086 is amended for SC-16. The latency gate measures each PR against
main built in the same job, in alternating rounds, and fails only on a sustained, repeated
slowdown above 20%. It is evidence only (reported, never gating) until five main runs show a tight
spread, then it becomes a required check. github-action-benchmark keeps main's history for the
graphs."*

**Why.** §1's rule held a pull request to the last value main recorded, on another runner. The
same build on two ubuntu-24.04 runners can differ by more than the 20% the rule draws its line
at, so one stored value is a coin toss at that line. main built and run in turn in the same job
shares the runner's speed with the change, and the speed cancels out of their ratio. In SC-16's
first runs, main against itself in one job came to ratios of 0.997 to 1.031.

**What changes in §1, for the rows the `bench` job holds:**

- **The baseline** is main as it is when the pull request is measured: the merge commit's first
  parent, built in the job. The change's bench source is built against main's crate and lockfile,
  so both sides run the same scenarios and only the code under them differs. When main's crate
  cannot build the change's bench, the row has no baseline and is reported only.
- **The statistic.** Each run's value is the median of its own samples (200 keystrokes, 40 bursts
  of 2 MB, 10 of 13 MB). A pass is five rounds, each one run of each build, in alternating order,
  after one discarded run of each. A row's ratio is the median of its rounds' ratios, the change
  over main.
- **"Sustained, repeated":** a row fails only when its ratio is past 1.20, all its rounds but at
  most one are past 1.20, and a second pass of its own says the same. One pass past it is
  reported as unconfirmed and passes.
- **Too noisy to judge:** a row passes, reported as noisy, when main's own rounds, with the
  slowest and the fastest set aside, spread by more than 20%.
- **Evidence first.** The job is `continue-on-error` and is not a required check. Once five runs on
  main show a tight spread, the gate becomes a required check (#932). §1's spread of about 7% is
  the starting value for "tight".
- **github-action-benchmark** keeps main's history: each push to main appends the rows' medians to
  the `benchmarks` branch, and that is the only job with `contents: write`. It no longer decides a
  pull request.

`tools/latency-gate.mjs` decides and `tools/latency-gate.test.mjs` tests it. `tools/bench.mjs
--only host --baseline` runs the rounds, and `charter-session-bench`
(`crates/session-protocol`) measures.

**The rows.** SC-16's job measures L1 and L3 at the session layer: a keystroke under ten flooding
panes, and 2 MB and 13 MB bursts, over a `charterd.sock` with no window. Their window half (keystroke
to screen, the burst's longest frame) stays release absolute. L2, M6 and L9's CI relative halves
go to #931. The other `bench` rows, L7 and L8, stay with FD-5 and SC-20.
