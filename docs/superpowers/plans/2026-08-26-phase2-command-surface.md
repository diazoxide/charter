# Phase 2 — the command surface

> **For agentic workers:** REQUIRED SUB-SKILL: `superpowers:subagent-driven-development`.

**Goal:** one mechanism with three faces — a palette, direct keys, and pickers — replacing
`display-menu` entirely; and the slot vocabulary retired so a component id is the frame's
currency end to end.

**Spec:** `docs/superpowers/specs/2026-08-25-agentic-ide-foundation.md` §4–§4k. **§4i and §4k
override anything earlier they contradict.**
**Measurements:** `docs/superpowers/specs/2026-08-26-tmux-input-findings.md` — raw bytes.

## What the measurements already decided

- **Full-pane palette everywhere** (§4k). `display-popup` is a 3.3-gated enhancement, not the
  mechanism. A 3.2 popup dies on any client resize.
- **A popup's own program never receives focus events.** Only the pane underneath, from 3.6.
- **`focus-events` is off by default in tmux and gates the whole path** — and with it off
  `#{client_flags}` still reads `attached,focused`, so a guard written against that flag passes
  with the feature dead. §4f's `focus`/`blur` does not exist until `conf_text` sets
  `set -t <session> focus-events on`.
- **`click` must admit a release with no matching press** — a drag from a border into a pane
  delivers exactly one release. The first component keeping press state wedges on it.
- **Charter's panels receive pointer events only while the ACTIVE pane requests reporting**, and
  charter does not control what the harness requests. "Clickable panels" is not a property
  charter can promise for its panes; the palette, being the active surface while open, is
  different.
- **charter's nine-row menu cap is charter's, not tmux's.** tmux 3.1c drew 20 rows fine.

## Global constraints

- `dependencies = []`, stdlib only. stdlib `unittest`, never pytest.
- Run the suite in **two environments** and say which; they must agree. Pin after Phase 1: **5880**.
- `PersonaIso`; new `patch.dict(os.environ, …)` MUST pass `clear=True`.
- `tui.width()` never `len()`. `contain.one_line` **before** width arithmetic.
- Mutation-test every guard: apply, RED, restore, GREEN, `__pycache__` cleared. **Report the
  mutation actually run and its actual result.**
- **A committed value reaching an overlay row is the `[frame] hotkey` class** — a newline there
  once achieved code execution at launch. Contain every name before it is drawn.
- No version bump, no stamping, no tag.

## The deletion sweep — required before any PR in this phase

Round two found **thirty unpinned guards across two branches**, every one by the same move:
**delete the guard, run the FULL suite, and see whether it stays green.** Each was correct
code with no test behind it, so a later refactor could remove it silently.

Examples of what that missed, each measured with a real consequence:

- `layout.harness_rows`' edge check — reverting it charged a provider's **12 columns to the
  harness as rows** (39 → 26 rows on a 50-row window), live on every resize and relayout.
- `panel._component_text`'s `width=slots._width()` — replacing it with a constant `80` made a
  provider's output **wrap and destroy the frame** in a 40-column pane. The three drawing tests
  all used a payload short enough that either width passed.
- `overlay.close_argvs`' refusal guard — without it, an empty pane id emits `kill-pane -t ""`,
  and the module's own docstring records measuring that this **kills the pane the command is
  running against**. The one measurement the module leads with was undefended at the one call
  site that can produce it.

**So: for every `if` you add that refuses, clamps, contains or falls back, write the test that
goes RED when that line is deleted.** Then run the sweep yourself — delete each new guard in
turn, run the full suite, and report any that stayed green **before** submitting.

A guard nothing pins is a comment with a runtime cost.

### Assert the reason, not just the refusal

A sweep that only checks the exit code is itself a guard with nothing behind it. Measured on
`release.yml`'s version check (#558): deleting the `-z "$claimed"` refusal — the one that
catches a `workflow_dispatch` with no version input — leaves the run **still exiting 1**,
because the mismatch check below it then catches the empty string instead:

```
shipped:  rc=1  this run did not say which version it publishes (the version input <none>)
mutant:   rc=1  this run names  (the version input <none>) but pyproject.toml says 0.53.0
```

Same exit code, different reason, and the second one is a worse answer to a different
question. **Two guards in sequence mask each other**, so an exit-code assertion cannot tell
them apart and stays green over a real deletion.

So a refusal test asserts **which** refusal fired. This is the same failure the sweep exists
to catch, one level up: a test that matches a *symptom* rather than the *property*, which is
the shape behind every bypass this repo has shipped.

---

### Task 1: Retire the slot vocabulary

**Carried from Phase 1**, where it was found to be why a provider can be placed but never drawn.
`Registry.place` has zero production callers, and **nine sites still speak the four committed
slot names**: `component_tables`' `SLOT_OF` check, `frame_of`'s `FRAME_SLOTS` filter,
`_placement`'s `SLOT_OF[cid]`, `layout._derive`, `SLOT_SIZE`/`SLOT_EDGE` as import-time
constants, `panel_command`'s `charter panel <slot>` argv, `cli.py`'s `panel` subparser,
`panel.run`'s unknown-slot refusal, and `slots.SLOTS`.

- [ ] **Step 1:** write the failing test — `charter panel <component-id>` runs a provider's
      component in a pane.
- [ ] **Step 2:** run it, confirm it fails.
- [ ] **Step 3:** make a component id the currency at all nine sites. A panel process must be
      able to build enough registry to find a provider's component; today it builds none.
- [ ] **Step 4:** `[frame] slots` keeps working — it is now shorthand for built-in ids. Test the
      repo's own committed `charter.toml` resolves unchanged.
- [ ] **Step 5:** full suite, two environments, **no existing test modified**.
- [ ] **Step 6:** mutation — make `panel.run` accept an id it cannot resolve; confirm RED.
- [ ] **Step 7:** commit.

---

### Task 2: The overlay surface

- [x] **Step 1:** write the failing test — an overlay renders rows into a pane charter owns,
      returns a selection, and restores what was there.
- [x] **Step 2:** run it, confirm it fails.
- [x] **Step 3:** implement a **full-pane** overlay: charter draws it, owns its input loop, and
      is modal. Not `display-popup`. Not `display-menu`.
- [x] **Step 4:** input — keys always; mouse **only when the overlay's own pane requests
      reporting**, per §4c. Handle a release with no press.
- [x] **Step 5:** **the escape hatch.** One key returns to the harness unconditionally, from any
      state, including a wedged renderer. Operate it at the **tmux level** (§4e) so it works
      when charter's own loop is stuck. Test it against a deliberately hung overlay.
- [x] **Step 6:** resize — the overlay must survive `window-resized`, redraw, and keep selection.
- [x] **Step 7:** mutations — remove the escape hatch; make the overlay non-modal; drop the
      release-with-no-press tolerance. Each RED.
- [x] **Step 8:** commit.

---

### Task 3: The action registry

- [ ] **Step 1:** write the failing test — an action declares `id`, `title`, `available()`,
      `reason_unavailable()`, `run()`; an unavailable action is listed **with its reason**.
- [ ] **Step 2:** run it, confirm it fails.
- [ ] **Step 3:** implement, mirroring `charter/frame/registry.py`. Entry-point group
      **`charter.actions`**, separate from `charter.components` (§4d), same integer API version
      refused at load, same id-collision refusal.
- [ ] **Step 4:** **actions are fire-and-report, never blocking** (§4g). An action returns
      immediately having started work; progress surfaces through `inflight`. A blocking action
      in a TUI is indistinguishable from a hang.
- [ ] **Step 5:** **a stricter contract than components.** An action *does* rather than draws:
      anything reaching a vault, a forge token or a shell goes **through** the existing guards,
      never around them. Test that an action cannot reach a vault value.
- [ ] **Step 6:** mutations — let an action block; let it bypass a guard; drop the
      reason-unavailable. Each RED.
- [ ] **Step 7:** commit.

---

### Task 4: The palette

- [ ] **Step 1:** failing test — `F2` opens a palette listing actions, typing filters, Enter runs.
- [ ] **Step 2:** run it, confirm it fails.
- [ ] **Step 3:** implement over Tasks 2 and 3. **`F2` becomes the palette; `display-menu` and
      `frame-menu` are deleted, not left beside it** (§4h) — two answers to "how do I do a thing"
      is how the current menu became weird.
- [ ] **Step 4:** an unavailable action appears **with its reason**, not hidden. The session lock
      refuses a workspace switch today; a palette that silently omits it is worse than one that
      explains.
- [ ] **Step 5:** no row cap. charter's nine-row limit was charter's, not tmux's.
- [ ] **Step 6:** mutations — hide unavailable actions; make filtering case-sensitive; reinstate
      a cap. Each RED.
- [ ] **Step 7:** commit.

---

### Task 5: Per-component toggle keys

- [ ] **Step 1:** failing test — a component's key toggles only it, live, and the layout
      recomputes.
- [ ] **Step 2:** run it, confirm it fails.
- [ ] **Step 3:** implement. **Density becomes a named arrangement over visibility, not the
      mechanism** (§4). Every key validated at the config boundary the way `_HOTKEY_RE` is.
- [ ] **Step 4:** the layout recompute is the Phase 1 registry's, not a second path.
- [ ] **Step 5:** mutation — let a toggle key skip validation; confirm RED.
- [ ] **Step 6:** commit.

---

### Task 6: Pickers, and the containment that matters

- [ ] **Step 1:** failing test — a workspace picker lists workspaces, switching repaints every
      panel against the new plane.
- [ ] **Step 2:** run it, confirm it fails.
- [ ] **Step 3:** implement over Task 2 — same overlay, different row source. Workspace and
      persona.
- [ ] **Step 4:** **containment.** A name with a newline, U+2028, an escape sequence, a quote or
      a `#` must render as one row and run nothing. Apply `contain.one_line` **before** width
      arithmetic (#472). **Test with hostile names, do not reason about them.**
- [ ] **Step 5:** **the switch must move the frame's own identity and bump it** so every panel
      repaints (#411, #412). Writing a pointer some panels may read is the bug #411 was filed for.
- [ ] **Step 6:** a refused switch (the session lock) **says so on screen**.
- [ ] **Step 7:** mutations — skip containment; write the pointer without bumping; swallow the
      lock refusal. Each RED.
- [ ] **Step 8:** commit.

---

### Task 7: focus-events, and the honest limits

- [ ] **Step 1:** add `set -t <session> focus-events on` to `conf_text` — without it §4f's
      `focus`/`blur` does not exist. Test it is set, and that a guard cannot be written against
      `#{client_flags}`, which lies.
- [ ] **Step 2:** document what charter **cannot** promise: pointer events for panes depend on
      the active pane's request, which charter does not control (§4i). Say it where an operator
      and a provider author will each read it.
- [ ] **Step 3:** update `[frame] mouse`'s docstring — it says the trade "belongs to a later
      release that actually ships clickable panels". This is that release, and the measurement
      says the trade is unavoidable, not conditional.
- [ ] **Step 4:** commit.

---

## Exit criteria

- Every action reachable in **≤2 keystrokes**; every unavailable one **says why**.
- A hostile workspace or persona name renders as one row and runs nothing.
- **`display-menu` and `frame-menu` are gone**, not deprecated.
- The escape hatch works against a **deliberately hung** overlay.
- **A provider's component can be placed by config and drawn** — Phase 1's criterion, moved here
  because the slot vocabulary made it unreachable there.
- The suite gives the same answer in two environments.
