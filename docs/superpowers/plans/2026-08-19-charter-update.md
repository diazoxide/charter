# `charter update` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One command that moves charter to a newer version on any harness, then tells you what that version brings and offers to adopt it.

**Architecture:** The CLI decides and acts; the skill only converses. `Harness.upgrade()` makes "how does this harness's artifact move?" one question with one answer. News entries ship inside the wheel as flat-frontmatter markdown carrying an optional in-process probe, so "have you adopted this?" is checked rather than assumed.

**Tech Stack:** Python 3.11+, stdlib only (charter has zero dependencies). `unittest` (`python -m unittest discover -s tests`). hatchling force-include for packaging.

**Spec:** `docs/superpowers/specs/2026-08-19-charter-update-skill-design.md`

## Global Constraints

- **Zero dependencies.** No YAML, no requests. Frontmatter parses via `charter.persona.parse`; version comparison via `charter.update._parse`.
- **ONE source of truth.** If two code paths answer the same question they call the same function. `upgrade()` reuses `opencode.refresh_shim`; news ordering reuses `update._parse`; entry resolution mirrors `docsrc`.
- **Additive-only.** Never delete or rename a user's file. Name the blocker and do everything unblocked.
- **Fail toward no change.** An unrecognised value falls back to the behaviour that alters nothing.
- **A check that could not run is `unknown`, never `ok` and never `pending`** (ADR 0013, `doctor._NOT_CHECKED_HINT`).
- **Comments record the WHY** — usually the reason a previous, more obvious approach failed.
- **Probes and adopts are charter subcommands only**, dispatched in-process. No shell, ever.
- Tests live in `tests/test_*.py`, use `unittest`, and isolate via `tests/_isolation.py`.

---

### Task 1: `Harness.upgrade()` and one answer for "how does this harness move?"

**Files:**
- Modify: `charter/harness/base.py` (add `upgrade`)
- Modify: `charter/harness/claude_code.py`, `charter/harness/opencode.py`, `charter/harness/codex.py`
- Modify: `charter/commands.py:1856-1906` (`cmd_version_sync` routes through it)
- Test: `tests/test_harness_upgrade.py`

**Interfaces:**
- Produces: `Harness.upgrade(self, root: Path) -> tuple[str, str]` with status in
  `{"moved", "current", "manual", "absent"}`. `"manual"`'s detail is the command to run.

- [ ] **Step 1: Write the failing test** — every registered harness answers; Claude Code names the plugin command; opencode moves its shim; Codex admits it does not know.
- [ ] **Step 2: Run it, expect AttributeError**
- [ ] **Step 3: Add `upgrade()` to `base.Harness`** returning `("absent", …)` by default, with the docstring explaining that inventing a command is a claim.
- [ ] **Step 4: Implement per harness.** Claude Code → `("manual", update.PLUGIN_SYNC_CMD)`. opencode → map `refresh_shim(global_dir())`: `refreshed`/`created` → `moved`, `current` → `current`, `not-ours` → `manual`. Codex → `("absent", …)`.
- [ ] **Step 5: Delete dead `_WIRING`** (`codex.py:44`) — defined, referenced nowhere.
- [ ] **Step 6: Route `cmd_version_sync` through `upgrade()`** so opencode stops being told to run a Claude Code command.
- [ ] **Step 7: Run full suite, commit.**

---

### Task 2: `charter/news.py` + `charter news`

**Files:**
- Create: `charter/news.py`, `docs/news/` (+ backfilled entries), `tests/test_news.py`
- Modify: `charter/cli.py` (subcommand), `charter/commands.py` (`cmd_news`), `pyproject.toml` (force-include), `CONTEXT.md` (glossary)

**Interfaces:**
- Produces: `news.Entry` (`version`, `slug`, `headline`, `check`, `adopt`, `body`),
  `news.all() -> list[Entry]`, `news.between(lo, hi)`, `news.probe(entry) -> str` in
  `{"adopted", "pending", "unknown"}`, `news.render_body(version) -> str`.

- [ ] **Step 1: Write failing tests** — parse; `unreleased` never surfaces; ordering via `update._parse`; probe returns `unknown` for an unresolvable subcommand; every shipped entry's `check:`/`adopt:` parses against the live argparse parser.
- [ ] **Step 2: Run, expect ImportError**
- [ ] **Step 3: Implement `news.py`** — `_PACKAGED`/`_CHECKOUT` resolution mirroring `docsrc`; `persona.parse` for frontmatter; in-process dispatch for probes with stdout/stderr captured.
- [ ] **Step 4: Wire `charter news [--pending] [--since] [--for]`**
- [ ] **Step 5: Force-include `docs/news` → `charter/_news`; add the backfill entries; add the CONTEXT.md glossary entry.**
- [ ] **Step 6: Run suite, commit.**

---

### Task 3: `charter update`

**Files:**
- Create: `charter/commands_update.py`, `tests/test_update_command.py`
- Modify: `charter/cli.py`, `charter/doctor.py` (pending-adoptions row)

**Interfaces:**
- Consumes: `Harness.upgrade` (Task 1), `news.between`/`news.probe` (Task 2)
- Produces: `cmd_update(args) -> int`; `installer.detect() -> tuple[str, list[str]]`

- [ ] **Step 1: Write failing tests** — the three pin cases; refuses in a charter checkout; baseline stamped before any move; handoff failure names the command; outside a harness reports *not checked*.
- [ ] **Step 2: Run, expect ImportError**
- [ ] **Step 3: Implement installer detection** (uv / pipx / pip / ambiguous→name-it).
- [ ] **Step 4: Implement the 7-step sequence** with the handoff doubling as verification.
- [ ] **Step 5: Add the `doctor` row.**
- [ ] **Step 6: Run suite, commit.**

---

### Task 4: Release gate *(release persona)*

**Files:** `.github/workflows/release.yml`, `charter/commands.py` (`news stamp`), `personas/release/persona.md`, `CONTRIBUTING.md`, `.github/pull_request_template.md`, `tests/test_news_gate.py`

- [ ] **Step 1:** Failing test — an entry exists whose `version:` equals `__version__`.
- [ ] **Step 2:** `charter news stamp <version>` renames `unreleased-*` and stamps frontmatter.
- [ ] **Step 3:** `guard` job asserts an entry for the tag; new post-publish job creates the GitHub Release from `charter news --for`.
- [ ] **Step 4:** Release charter gains the stamp step; CONTRIBUTING + PR template gain the prompt.
- [ ] **Step 5:** Run suite, commit.

---

### Task 5: The skill

**Files:** `skills/update/SKILL.md`

- [ ] **Step 1:** Write it with `mattpocock-skills:writing-for-agents`.
- [ ] **Step 2:** Verify no persona declares it in `skills:`.
- [ ] **Step 3:** Commit.
