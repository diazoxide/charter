# Phase 1 — the component registry

> **For agentic workers:** REQUIRED SUB-SKILL: use `superpowers:subagent-driven-development`
> to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** replace the four-string `slots` list with a component registry that charter's own
panels consume through the same public seam a third-party provider will — with behaviour
byte-identical at the end of Task 6.

**Architecture:** a component declares identity, edge, size policy, what it reads (`needs`)
and what events it accepts; the registry answers "what is on screen and how big"; `ctx` is
built *from* `needs` so cost is enforced rather than reviewed. tmux keeps panes, resize and
pane-focus; charter keeps everything inside a pane.

**Spec:** `docs/superpowers/specs/2026-08-25-agentic-ide-foundation.md` — read §4 through §4h
before starting. Nineteen decisions are recorded there; this plan implements them and does not
revisit them.

**Depends on:** Phase 0 (#519 #521 #528 #527 #532 #494 #507 #531). Until the suite gives the
same answer in every environment, no verification in this plan means anything.

## Global constraints

- `dependencies = []`. stdlib only — `importlib.metadata` is stdlib and is how providers are
  discovered. Any non-stdlib import is an automatic reject.
- stdlib `unittest`, never pytest. Full suite, ambient variables cleared:
  `env -u CHARTER_SESSION_ID -u CLAUDE_CODE_SESSION_ID -u TMUX -u TMUX_PANE python3 -m unittest discover -s tests 2>&1 | grep -E "^(OK|FAILED|Ran |FAIL:|ERROR:)"`
- `PersonaIso` for plane-touching tests. New `patch.dict(os.environ, …)` MUST pass `clear=True`.
- Measure width with `tui.width()`, never `len()`. Contain committed values with
  `contain.one_line` **before** width arithmetic.
- **Order is geometry.** Split order determines pane width — measured at 200 columns vs 154.
  The registry stores it explicitly; nothing derives it from list position by accident.
- The idle-cost property (`test_an_idle_tick_costs_exactly_one_stat_and_nothing_else`) must
  hold at every task boundary.
- Mutation-test every guard: apply, RED, restore, GREEN, `__pycache__` cleared. **Report the
  mutation actually run and its actual result.**
- No version bump, no news stamping, no tag.

---

### Task 1: Measure what tmux actually does (spike, no production code)

Two hypotheses in the spec are unverified and both change Phase 2's design if wrong. Answer
them before anything is built on them. **This task ships a findings document, not code.**

**Files:** Create `docs/superpowers/specs/2026-08-26-tmux-input-findings.md`

- [ ] **Step 1: mouse routing.** With tmux's own `mouse` OFF, have a program in a pane request
      SGR mouse reporting (`\x1b[?1006h\x1b[?1000h`) and record whether it receives click and
      scroll events. Repeat with tmux's `mouse` ON. Repeat both inside the operator's own
      server where their `.tmux.conf` may already set `mouse on`.
- [ ] **Step 2: what scroll does** in a pane with scrollback, under each of the four
      combinations above — does it reach the program or enter copy-mode?
- [ ] **Step 3: `display-popup`.** Confirm the version it appeared in against the shipped
      CHANGES file, then measure: does a program in a popup get its own tty, can it request
      mouse reporting, does it receive focus events, and what happens to the popup on
      `window-resized`?
- [ ] **Step 4: the sub-3.2 band.** `tmuxctl.below_floor_message` promises charter still
      launches there. Record what is available instead of `display-popup`.
- [ ] **Step 5: write the findings**, each with the exact commands run and the raw bytes
      observed. State plainly where a hypothesis was **wrong** — that is the valuable half.
- [ ] **Step 6: commit.**

---

### Task 2: The component contract

**Files:** Create `charter/frame/component.py`, `tests/test_component_contract.py`

- [ ] **Step 1: write the failing test.**

```python
def test_a_component_declares_what_it_reads(self):
    c = component.Component(id="demo", title="Demo", edge="right",
                            size=component.Fixed(3), needs=("gather",),
                            events=(), render=lambda ctx: ["x"])
    self.assertEqual(c.needs, ("gather",))
    self.assertEqual(component.API_VERSION, 1)
```

- [ ] **Step 2: run it, confirm it fails** (`ModuleNotFoundError`).
- [ ] **Step 3: implement.** A frozen dataclass with `id`, `title`, `edge`, `size`, `needs`,
      `events`, `render`, and a module-level `API_VERSION = 1`. Size policies as three types:
      `Fixed(n)`, `Content(cap=None)`, `Fill()`.
- [ ] **Step 4: id validation.** An id must match a namespaced pattern; validate at
      construction. A component id reaches menu rows and pane titles, so this is a
      containment boundary, not a style rule — see `instance._HOTKEY_RE` for the precedent
      and why it exists.
- [ ] **Step 5: run tests, confirm they pass.**
- [ ] **Step 6: mutation** — remove the id validation, confirm RED, restore, confirm GREEN.
- [ ] **Step 7: commit.**

---

### Task 3: The registry, serving built-ins only

**Files:** Create `charter/frame/registry.py`, `tests/test_component_registry.py`

- [ ] **Step 1: write the failing test** — registering two components and asking for those on
      an edge returns them in registration order, and registering a duplicate id raises
      naming both.
- [ ] **Step 2: run it, confirm it fails.**
- [ ] **Step 3: implement** `register()`, `get(id)`, `on_edge(edge)`, `all()`. Registration
      order IS split order — say so in the docstring with the 200-vs-154 measurement.
- [ ] **Step 4: the composite.** A component may declare `children=(...)` of leaf ids. Exactly
      one child may be `Fill()`; more than one raises at registration, naming both. One level
      only — a child that is itself composite raises.
- [ ] **Step 5: run tests.**
- [ ] **Step 6: mutations** — allow duplicate ids; allow two `Fill()`; allow nested composites.
      Each RED, each restored GREEN.
- [ ] **Step 7: commit.**

---

### Task 4: `ctx` constructed from `needs`

**Files:** Create `charter/frame/ctx.py`, `tests/test_component_ctx.py`

- [ ] **Step 1: write the failing test.**

```python
def test_a_component_gets_only_what_it_declared(self):
    c = _ctx_for(needs=("gather",), width=80, height=10)
    self.assertEqual(c.width, 80)
    self.assertIsNotNone(c.gather)
    with self.assertRaises(AttributeError):
        c.personas          # not declared -> not present
```

- [ ] **Step 2: run it, confirm it fails.**
- [ ] **Step 3: implement.** `build(needs, *, width, height, fid, snapshot)` returns an object
      exposing geometry, identity, and exactly the declared slices. **Absent, not disabled** —
      a component asking for something undeclared gets `AttributeError` naming what to add.
- [ ] **Step 4: no escape hatches.** `ctx` exposes no filesystem handle, no subprocess factory,
      no network client. Add a test asserting the attribute set is exactly what was declared
      plus the fixed geometry keys — so a future field cannot be added without a test change.
- [ ] **Step 5: run tests.**
- [ ] **Step 6: mutations** — hand over the whole snapshot regardless of `needs`; add an
      undeclared attribute. Each RED, restored GREEN.
- [ ] **Step 7: commit.**

---

### Task 5: Express today's components in the registry

**Files:** Modify `charter/frame/slots.py`, `charter/frame/layout.py`; create
`tests/test_builtin_components.py`

- [ ] **Step 1: write the failing test** — the registry holds `identity`, `attention`,
      `repos`, `personas`, `todos`, and a composite `sidebar` of `personas` + `todos`; each
      declares the `needs` its current renderer actually reads.
- [ ] **Step 2: run it, confirm it fails.**
- [ ] **Step 3: register the built-ins**, wrapping the existing renderers unchanged. This task
      changes no output.
- [ ] **Step 4: route `layout` through the registry** for edges and sizes, keeping
      `SLOT_SIZE`'s current values as each component's declared size.
- [ ] **Step 5: run the full suite** — it must be green with **no test changes**. If a test
      needed changing, the refactor was not behaviour-preserving; find out why before
      proceeding.
- [ ] **Step 6: render before and after** at 200x50 and 80x24 and diff them. Byte-identical, or
      explain precisely why not.
- [ ] **Step 7: commit.**

---

### Task 6: Config maps onto the registry, losslessly

**Files:** Modify `charter/instance.py`; create `tests/test_component_config.py`

- [ ] **Step 1: write the failing test** — every `[frame] slots` value in use today resolves to
      the same visible components and the same split order as before, and each shipped
      `density` resolves to a named arrangement.
- [ ] **Step 2: run it, confirm it fails.**
- [ ] **Step 3: implement the mapping.** `slots` is shorthand for placing built-ins on their
      default edges in the given split order; `density` is a named arrangement. Both keep
      working exactly as they do.
- [ ] **Step 4: add the `[[frame.component]]` form** — `use`, `edge`, `size`, `visible`. File
      order is split order. Every value is validated at the config boundary and degrades to
      the default, the way `frame_of` already does.
- [ ] **Step 5: the operator's own `charter.toml`** must resolve to the same frame it draws
      today. Test it against the committed file specifically — a change that silently removes
      the repo table from charter's own plane has already been caught once, in #535.
- [ ] **Step 6: run the full suite.**
- [ ] **Step 7: mutation** — make `slots` ignore order; confirm the geometry test goes RED.
- [ ] **Step 8: commit.**

---

### Task 7: Provider discovery

**Files:** Modify `charter/frame/registry.py`; create `tests/test_component_providers.py`

- [ ] **Step 1: write the failing tests** — a provider declaring `API_VERSION` loads; one
      declaring a different integer does NOT, and the message names the provider, its version
      and charter's; two providers claiming one id refuse BOTH and name both; a component
      named in config with no provider installed produces a message and the rest of the frame.
- [ ] **Step 2: run them, confirm they fail.**
- [ ] **Step 3: implement discovery** via
      `importlib.metadata.entry_points(group="charter.components")`. **Import lazily** — an
      installed-but-unplaced provider costs nothing.
- [ ] **Step 4: failure isolation.** A provider that raises on import, or whose `render`
      raises, costs its own pane and never the session. The pane says which component failed
      and why — a blank pane is the confidently-wrong output the left sidebar was retired for.
- [ ] **Step 5: contain the output.** A provider's returned lines go through `contain.one_line`
      and `tui.width` applied **by charter**, not trusted to the provider.
- [ ] **Step 6: run the full suite.**
- [ ] **Step 7: mutations** — accept a mismatched API version; let one of two colliding ids
      win; let a raising provider propagate; skip the output containment. Each RED, restored
      GREEN.
- [ ] **Step 8: commit.**

---

## Exit criteria for Phase 1

- The frame renders byte-identically to `origin/main` for every shipped config.
- `[frame] slots`, `[frame] density` and `[[frame.component]]` all resolve through **one**
  code path.
- charter's own components are registered through the **same public seam** a provider uses —
  no private back door.
- A provider can be installed, placed by config, and drawn — and a broken one costs its pane
  and nothing else.
- The idle-cost property still holds, verified rather than asserted.
- Task 1's findings say whether §4c's mouse hypothesis and §4g's popup hypothesis survived.
