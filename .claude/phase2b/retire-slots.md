# retire-slots
PR: https://github.com/diazoxide/charter/pull/553 — still OPEN, not merged. New commit 76fd3ef on `worktree-wf_ce811d23-e06-1`.
Branch: worktree-wf_ce811d23-e06-1 (detached work in /Users/aharon/IdeaProjects/charter/.claude/worktrees/wf_1c5474ce-72e-1, pushed 04c545e..76fd3ef with --force-with-lease). Already contained current origin/main (a912553) — no rebase needed.

## Unpinned guards

### 1

layout.py:551 — `harness_rows`'s `_edge_of(slot) not in _COLUMN_EDGES` is unpinned. Reverting it to the pre-branch `_key(slot) not in _COLUMN_SLOTS` leaves the FULL suite at `Ran 5909 tests / OK` (log: scratchpad/adv-rs-r2/full-G21_harness_rows_old.log). Measured on a frame built by `instance.frame_of` with `[[frame.component]] use='acme.metrics', edge='right', size=12`: shipped answers 39 rows for a 50-row window, the reverted form answers 26 — the provider's 12 COLUMNS charged to the harness as rows. Live at commands_frame.py:3049 (`_reassert_sizes`), i.e. every window resize and every density relayout. No test in the suite calls `harness_rows` with a provider present (`grep harness_rows tests/` hits only test_frame_tmux_integration.py:1519, charter's own four).

### 2

layout.py:517 — `slot_sizes`'s `cells = _size_of(slot)` is unpinned. Reverting it to `SLOT_SIZE.get(_key(slot))` leaves the FULL suite at `Ran 5909 tests / OK` (full-G23_slot_sizes_old.log). Measured: the provider vanishes from the map entirely — shipped `{'top':1,'bottom':1,'repos':6,'acme.metrics':12}`, reverted `{'top':1,'bottom':1,'repos':6}` — which is then what `harness_rows` and `_reassert_sizes` size the whole stack from. `TheLauncherSplitsAPaneForIt` never calls `slot_sizes`; the only call in the new file (line 472) is `CharterOwnConfigIsUnchanged`, which has no provider in it.

### 3

panel.py:199 — `width=slots._width()` in `_component_text` is unpinned. Replacing it with a constant `80` leaves the FULL suite at `Ran 5909 tests / OK` (full-G35.log). Measured with a real installed provider whose render returns 200 'X's into a 40-column pane: shipped emits 40 columns and an ellipsis, the mutation emits 80 columns, which wraps and destroys the frame. This is §4b property 3 ('clips it to the rectangle') on the one path that actually paints a provider, and the three drawing tests all use a payload short enough that either width passes.

### 4

panel.py:204 — `except Exception` in `_component_text` is unpinned. Narrowing it to `except ZeroDivisionError` leaves the FULL suite at `Ran 5909 tests / OK` (full-G10_component_text_no_catch.log). The docstring's '**Never raises**' promise is about THIS function's own failure modes — `reg.get(cid)`, `gather.read(fid)`, `_rows()` — and no test makes any of them fail; `test_a_component_that_raises_costs_its_own_pane_and_names_itself` exercises `Registry.draw`'s catch one layer down, not this one. Under the mutation a transient `gather.read` failure escapes to `run`'s outer handler and `_hold`s the pane permanently instead of repainting the 'unavailable' line.

### 5

panel.py:198 — the `if c.needs` condition on `snapshot = gather.read(fid)` is unpinned. Deleting it (always read) leaves the FULL suite at `Ran 5909 tests / OK` (full-G11_component_text_always_gathers.log). That is §4e's idle-cost property for a provider that declared nothing, stated in the docstring as the reason the line exists, with no test asserting `gather.read` is not called.

### 6

panel.py:483 — `contain.one_line(slot)` in `run`'s failure message is unpinned. Deleting it leaves the FULL suite at `Ran 5909 tests / OK` (full-G8_panel_stopped_uncontained.log). Reachable and measured: a real `.dist-info` whose `entry_points.txt` declares the component name `acme.\x1b[2Jm` is discovered by `builtins.supplies` (metadata only, no import) and `slots.drawable` answers True, so that name reaches this line. Shipped paints `acme.\x1b[2Jm` escaped; without the call the pane shows `acme.m` — the ESC is swallowed downstream by `tui.sanitize`, so no escape reaches the terminal, but the containment itself is what nothing tests.

### 7

panel.py:210 — `contain.one_line(cid)` in `_component_text`'s failure line is unpinned. Deleting it leaves the FULL suite at `Ran 5909 tests / OK` (full-G9_component_text_uncontained.log). The docstring makes a specific claim about this call's ORDER relative to the width arithmetic ('measuring first would measure a string that is not what the terminal is about to do') and no test asks the question in either order.

### 8

layout.py:141 — `_derive`'s `_builtins.SLOT_OF.get(c.id, c.id)` fallback is unpinned. Reverting it to `SLOT_OF[c.id]` leaves the FULL suite at `Ran 5909 tests / OK` (full-G24_derive_no_fallback.log), and `layout._derive([provider_component])` then raises `KeyError: 'acme.metrics'`. The docstring calls this exact line 'the line Phase 1 could not cross'; `_derive` is called in tests only from test_builtin_components.py:174/190/205, all with charter's own components.

### 9

instance.py:649 — `out["components"] = []` set before `frame_of`'s early return is unpinned. Deleting it leaves the FULL suite at `Ran 5909 tests / OK` (full-G27_frame_of_no_early_components.log), and `instance.frame_of({})` then has no `components` key at all — the two-shapes-for-one-answer the comment on that line says it exists to prevent.

### 10

slots.py:1500 — `drawable`'s `if not isinstance(name, str): return False` is unpinned. Deleting it leaves the FULL suite at `Ran 5909 tests / OK` (full-G1_drawable_nonstr.log), and `slots.drawable(['x'])` then raises `TypeError: cannot use 'list' as a dict key` instead of answering False. `drawable` is the one answer four callers share, two of them guards on text that reaches tmux config.

### 11

builtins.py:106 — `component_id`'s `if isinstance(name, str)` guard is unpinned. Deleting it leaves the FULL suite at `Ran 5909 tests / OK` (full-G5_component_id_nonstr.log), and `component_id(['x'])` raises `TypeError` — the docstring says in as many words that a non-str 'comes back as it went in' so the refusal belongs to whatever validates the value.

### 12

layout.py:259 — `_key`'s `if isinstance(name, str)` guard is unpinned. Deleting it leaves the FULL suite at `Ran 5909 tests / OK` (full-G16_key_no_str_filter.log).

