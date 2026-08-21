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

### Known gap, filed not fixed

diazoxide/charter#314 — `commands_update._handoff` spawns a FRESH charter process running
`charter news --since <baseline>`, which probes. So a `check: update …` re-enters across a
process boundary where a process-local counter is blind, and the probe would run a real
`uv tool install` on the way. It terminates today only by accident (`_stamp_baseline` runs
first, so the child's `between(installed, installed)` is empty) and nothing asserts that.
Everything else self-spawning is bounded by fixed argv with no back-edge.

## Glossary

<!-- Task/domain vocabulary so a teammate or a fork isn't lost: `term` — definition. -->

_Nothing yet._

## Log

Chronological "what was done" lives in the task memo — `memory/notes.md`
(append with `charter workspace note "…"`).
