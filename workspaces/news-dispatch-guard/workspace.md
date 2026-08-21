# news-dispatch-guard

> **Living charter** for this workspace — its north star and shared context.
> Keep it current as the work evolves (edit this file, or `charter workspace vision "…"`).
> It's committed + shared for LIVE workspaces, and a fork inherits it — so anyone
> can pick up the task with full context. Never put secrets here (vault only).

## Vision

charter doctor must never be able to hang: a news entry's check: probe cannot be allowed to re-enter the command running it (diazoxide/charter#311).

## Context & decisions

<!-- Key facts, constraints, and design/architecture decisions found while working —
     the durable "why", not a chronological log. Grow this as you learn. -->

### Status — DELIVERED (2026-08-20)

diazoxide/charter#311 fixed and closed by PR #313, squash-merged as `c7a665a`. 2876 tests OK
(2864 before).

**Released as 0.47.1** on 2026-08-21 — PR #315 merged as `8ec78b4`, tag `v0.47.1`, both workflows
green, live on PyPI (2 artifacts). Machine upgraded: CLI 0.47.1, plugin 0.47.0 → 0.47.1 (project
scope; restart applies it). Smoke-tested on the INSTALLED CLI: `doctor` rc=0 in 1.19s.

Patch, not minor: both halves are fixes adding no surface, and the only altered output corrects a
self-contradicting message. The freshness gate's arithmetic was run against both candidates
(`0.47.1: lag=1 pass` / `0.48.0: lag=2 FAIL`) and deliberately did NOT decide the version — stated
in the commit and PR so it is not later read as expedience. **The asset slack is now spent: 0.48.0
will fail `test_asset_freshness` until `demo.svg`, `personas.svg` and `statusline.svg` are
regenerated.**

Two near-misses caught during the cut, both worth more than the release: a counterfactual that was
**vacuous and looked like a pass** (`news._is_checkout` needs a `pyproject.toml` two levels above
`docs/news`; a scratch tree without one makes `news.all()` empty, so unguarded code "passes" in
0.78s printing `✓ nothing to adopt` — indistinguishable from a working guard; assert the planted
entry is in `released()` before trusting any timing), and a **void baseline suite run** caused by
editing version files while a run was in flight (module cached at the old version against a freshly
written `plugin.json` produces five failures that look exactly like real drift).

### The defect

`news._dispatch` re-enters the CLI in-process, and `charter doctor` runs every RELEASED entry's
`check:` through it — so `check: doctor` recursed unbounded, each level a full sweep. Two
properties made it worth a guard rather than a rule about entries: **dormant until release**
(`version: unreleased` is never probed, so it sleeps through review/CI/merge and arms at
`charter news stamp`) and **CI cannot catch it before the tag burns** (`news --for <v>` passes
in 0.1s; only the `test` job fails, after the tag push has fired PyPI).

### Decisions

- **The guard is two halves, and only one is about the loop.** Refusing the nested call bounds
  the recursion; *clearing the outer call's exit code* is what keeps the answer honest. A
  nested `doctor` exits 0 whenever nothing is broken, so an outer probe reading that code would
  report the entry ADOPTED — hidden forever by the very bug it triggers. Bounded ≠ correct
  (ADR 0013). A re-entered probe reports `unknown` with a reason naming the entry.
- **A plain global, not a ContextVar.** A ContextVar reads its default in every new thread, so
  a probing command that fanned out to a pool would walk straight past the guard — the exact
  failure being fixed. The global's failure mode is the cheap one (two racing probes make each
  other `unknown`) and is unreachable today: `doctor` and `charter news` each walk entries in
  one thread.
- **Three reason strings, not one** (`_IN_FLIGHT`, `_PROBES`, `_NOT_RUN`). "A probe is already
  in flight" is a different fact from "this entry's `check:` probes", and during a nested sweep
  the entry being refused is often not the guilty one — one string would emit false diagnostics.
- **The flag clears on the way IN, not out**, before the early returns: every top-level dispatch
  must start clean, or a refusal recorded while probing the previous entry is read as this
  entry's answer. Release sits in a `finally` because argparse raises `SystemExit` on every
  malformed `check:`.
- **No static ban on `check:` values.** The set of probing commands cannot be stated statically
  without lying — `news --pending` and `news --since` probe, `news --for` does not. With the
  guard, a miss costs an honest `unknown` instead of a hang.
- **An existing lie was removed.** `charter news --pending` printed `✓ nothing pending — every
  entry with a probe reports adopted` *directly beneath* its own warning that an entry could not
  be checked. A green tick under the warning it contradicts is how the warning stops being read.

### The cross-process hole — #314, fixed by PR #318 (open, CI green)

`commands_update._handoff` spawns a FRESH charter running `charter news --since`, which
probes, so a `check: update …` re-entered where a process-local counter is blind. Measured
on 0.47.1: **four** process boundaries crossed, every hop reporting `adopted`, stopped only
by the test's own cap. Guarded: one spawned charter, refused, `unknown`, 0.086s.

- **The guard travels in `CHARTER_NEWS_PROBE`**, value `<pid>:<markpath>`, set for the
  length of a probe. Down: any charter started underneath one declines to probe. **Back up**:
  a refused descendant touches `<markpath>` so the outer probe withholds its exit code —
  without that the loop is bounded and the entry still comes back `adopted`, one process
  further out than #311. Both halves cross, or only the cheap one does.
- **The PID is what stops the marker being the worse bug.** An environment belongs to a
  process, it is restored in the same `finally` as the counter, and a marker naming a dead
  process is debris — ignored, not believed. Liveness via `os.kill(pid, 0)` **on POSIX only**:
  on Windows that maps to TerminateProcess and would kill whatever the marker named.
- **No marker can reach the frightening half.** `check: update` runs `uv tool install` in the
  process that IS the probe (depth 1, permitted by design), never in a child. So `cmd_update`
  asks `news.probing()` itself and calls `news.refuse_mutation()` → `unknown`, not `pending`
  (pending invents a chore on a plane that may already have adopted the entry).
- **The accident is written down.** `_stamp_baseline` before the move makes the child's
  `between(installed, installed)` empty. True, and arithmetic two modules away — named in
  `_handoff`'s docstring and pinned by `TheHandoffBound`, as the second line now.

### Follow-on work — all closed (2026-08-21)

The guard shipped in 0.47.1 turned out to be one third of the job. Both remaining holes were
found by surveying for the SAME SHAPE rather than waiting for a report, and both are closed:

- **#314 → PR #318 (`7c059f4`), released in 0.47.2.** The guard was a module-level counter, so
  blind to `commands_update._handoff` spawning a fresh charter process that probes. Fixed with a
  `<pid>:<markpath>` env marker — ignored when the pid is gone, so it self-heals rather than
  silently disabling probing forever (POSIX only: `os.kill` is a question on POSIX and an ANSWER
  on Windows). Two corrections to the original framing: the `uv tool install` runs in the process
  that IS the probe, at depth 1, so no marker in a child could reach it — `cmd_update` asks
  `news.probing()` itself; and bounding alone left the OUTER probe reading the spawner's exit code
  as `adopted`, so the refusal travels back up through a mark file.
- **#317 → PR #319 (`021c239`), released in 0.47.2.** `news.py`'s docstring claimed an entry
  "cannot become an arbitrary-command primitive". False: `secret` is registered with a bare
  `nargs="*"`, so `check: secret exec <vault> curl …` handed entry-controlled argv to
  `subprocess.run` WITH THE VAULT'S SECRET in the child env — and `doctor` runs every released
  entry's `check:` unprompted on a SessionStart hook. Now held to
  `_PROBEABLE = {doctor, news, persona lint}`.
- **#321 filed, not fixed** — a shell-syntax `check:` is refused a gate earlier and reports
  `_NOT_RUN` ("did not run *here*"), blaming the reader's machine for a defect in the entry.
  Cosmetic; no process starts either way.

**Why an allow-list and not a structural argparse rule.** The invariant has TWO clauses — *a probe
is read-only, AND its argv is charter's, never the entry's*. argparse can be asked whether a
command takes a pass-through positional; it cannot be asked whether a command WRITES.
`check: update --to X` reaching a real installer has no pass-through positional at all, so a
structural rule waves it through. The structural check still exists, moved to CI where it is total
instead of partial: entries, list and parser ship in one wheel, so a test across the three is a
proof rather than a sample — and it fails the PR that adds the drift, not the machine that installs
it. Two traps found alongside: `version` must NOT be listed (always exits 0 → reports adopted
everywhere forever), and argparse READS PAST A FLAG to find a subcommand, so
`news --pending stamp 9.9.9` really reached `cmd_news_stamp` — demonstrated on 0.47.1 by a canary
file that was actually renamed on disk.

### Superseded gap (kept for the reasoning)

diazoxide/charter#317 — `secret exec` takes a pass-through argv, so a `check:` naming it
reaches ANY binary with a vault's credential in its environment (`news._tokens` only rejects
shell metacharacters and an unregistered first token; verified `secret exec v curl …`
resolves). That contradicts news.py's own docstring, which #318 corrects to say so. #318's
marker closes the re-entrancy half (the spawned charter declines to probe); the argv is #317's.

## Glossary

<!-- Task/domain vocabulary so a teammate or a fork isn't lost: `term` — definition. -->

_Nothing yet._

## Log

Chronological "what was done" lives in the task memo — `memory/notes.md`
(append with `charter workspace note "…"`).
