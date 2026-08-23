# Frame content parity — implementation plan (#385)

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development.

**Goal:** the frame shows what the status line shows, by composing the renderers that already exist rather than reimplementing them.

**Architecture:** one gather, many slot renderers. `charter/frame/gather.py` owns the expensive, correctness-critical part (repo states, branches, gl state) and writes a cache; panels are pure readers; the hook that already bumps the version refreshes the cache, because a bump is exactly "plane state changed".

**Spec:** issue #385, plus the design settled by grilling and recorded in this plan's Global Constraints.

## Global Constraints

- `dependencies = []`. stdlib `unittest`, never pytest.
- **Share the gather, not the composition.** `_repo_rows` returns `tui.Node`s laid out for a wide boxed frame; a 22-column pane needs its own composition. Reuse `states`/`branches`/`gl`; write narrow renderers.
- **Panels are pure readers.** `_run_state` shells out to `git status --porcelain --branch` per repo; four panels each gathering means four identical sweeps per repaint. Panels read a cache; the bumping hook writes it. The frame must still cost a `stat` at idle.
- **`render()` never raises** — a panel that dies leaves a hole in the frame.
- **Panels measure their own tty**, never `$COLUMNS`. `tui.term_width()` reads the env first, which is right for the status line and wrong in a pane.
- **`tui.width` counts display cells, not characters** — a wide glyph that fits by `len()` still wraps a pane and pushes the frame apart.
- **Do not modify `charter/statusline.py`'s render path.** The status line keeps working exactly as it does; this plan only *calls* its helpers.
- Only `tests/test_frame_tmux_integration.py` may start a real tmux, and it must **probe capability, not presence** — CI is Ubuntu with tmux 3.4 and `TERM=dumb`.
- Every test verified by mutation: apply it, confirm red, restore, confirm green. Clear `__pycache__` between runs. A test asserting on anything the test process did not itself set is not trusted until mutated — this branch's predecessor shipped three vacuous tests, two from ambient environment.

## Slot assignment

Follows the zones `statusline.py` already argues for, so the frame does not invent a second information architecture:

| slot | content |
| --- | --- |
| `top` | identity: workspace, pin, persona, version, context/session when available |
| `left` | repo rows, piece summaries |
| `right` | persona chips, memory badges, in-flight badges, vault dots |
| `bottom` | alerts, news, todo count, hotkey hint |

## Tasks

### Task 1 — `charter/frame/gather.py`: one scan, cached
Extract the gather `statusline.render` performs (`_repo_trees`, `_repo_states`, `_branch`, `glstate.read_for`) into a module that returns a plain data structure and can write/read it as JSON under the frame's directory. Pure of layout. Tests: the cache round-trips; a corrupt cache degrades to a fresh gather rather than raising; a missing cache is not an error.

### Task 2 — the bumping hook refreshes the cache
`notify.plane_changed()` already fires from seven hook sites, debounced. It writes the version; make it also refresh the gather cache for the running frame. Must remain never-raising and must not add measurable cost to `posttooluse-bash`. Tests: a bump refreshes; a failure to gather does not raise; the debounce still holds.

### Task 3 — `left` and `right` renderers
Add them to `slots.SLOTS`, composed narrow from the cache. `unimplemented()` shrinks accordingly. Tests: each renders inside its pane width measured in display cells; each degrades to a readable line when the cache is empty; a CJK-heavy workspace name still fits.

### Task 4 — enrich `top` and `bottom`
Compose the remaining helpers into the two existing slots. Tests: content appears; each still fits one row; `render` still never raises.

### Task 5 — real-tmux integration
A four-edge frame comes up with all four panels alive and showing real content, and repaints after a `state.bump`. Capability-probed, skip-if-absent, leaks nothing.
