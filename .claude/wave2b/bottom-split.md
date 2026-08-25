# bottom-split
PR: https://github.com/diazoxide/charter/pull/535
Branch: frame-bottom-split-515

## Blocking

### 1

(B) `charter.toml` (repo root, unchanged by this PR) still pins `slots = ["top", "bottom", "right"]`. Verified under branch code with that exact committed file: `config.FRAME['slots']` -> `['top','bottom','right']` and `commands_frame._drawable_slots(200,50)` -> `['top','bottom','right']`. So merging this makes charter's OWN plane — the operator's, the one that reported #515 — lose the repo table entirely, with no message anywhere saying why. On origin/main the same file draws the table. The file's own comment becomes false at the same moment ("identity across the top, the wide repo table across the bottom", "`bottom` now draws properly", and the #488/#500 order paragraph, which is now about `repos`). Fix: add `repos` to that list in split order (`["top", "bottom", "repos", "right"]`) or delete the line and take the default, and re-derive the comment.

### 2

(C) `tests/test_frame_layout.py::VisibleSlots::test_the_width_the_table_needs_is_read_from_the_renderer_not_copied` cannot fail on the mutation it names. Replacing `layout._table_min_cols`' body with `return 95` passes the whole suite (I ran it: `Ran 5532 tests ... OK`). The test asserts `layout._table_min_cols() == statusline._LEFT_W`, and a literal 95 equals today's `_LEFT_W`. The report's `mutations` field lists this exact mutation under RED->GREEN and claims "No survivors"; both are wrong. The consequence is real, not cosmetic: if `_LEFT_W` ever moves, the launcher's drop and `slots._table_cap`'s refusal come apart, which is the pane-split-for-a-refusal bug #515 removed. One-line fix, verified to kill the mutant: wrap the assertion in `mock.patch.object(statusline, "_LEFT_W", statusline._LEFT_W + 7)` — the branch answers 102, the literal answers 95.

### 3

(C) The idle-cost claim overreaches. `panel._tick` reads `state.version(fid)` exactly once per 0.2s TICK for EVERY slot, animated or not — `tests/test_frame_panel.py::IdleCostsTheSameWhateverTheBottomPaneIsTallEnoughToDraw` pins that at one filesystem call per panel per tick. #515 takes the frame from three panel processes to four, so the frame's idle cost goes up by one read per tick (+33%), not "zero extra `stat`s". What is true, and is the useful half, is that `repos` is not in `slots.ANIMATED`, so `_watch`'s `animates and bool(_running(...))` short-circuits and it never pays the SECOND stat `bottom` pays. Narrow the sentence in the report, and in `slots._repos`' docstring where it makes the same point.

### 4

(C) `slots._empty_lines`' docstring justifies the absent `contain.one_line` with a claim that is false: "Every rung of `state.workspace_for` that can answer a name checks it against `instance.WORKSPACE_NAME_RE`". Rung 3 does not — it is `workspace.resolve()`, which returns `os.environ['CHARTER_WORKSPACE'].strip()` with no name check. Measured on the branch: with `CHARTER_WORKSPACE='ev\nil\x1b[31m;rm -rf /'`, `state.workspace_for(fid)` returns that string verbatim and it reaches `_empty_lines`. The PANE is still safe — `tui.truncate` calls `tui.sanitize`, which turns the newline into a space and drops non-SGR escapes, so the line count stays 1 — and `_top` has had the identical property since before this PR, so this is not a regression. But the stated reason is not the reason. Re-derive it onto `tui.sanitize`, which is what actually contains it.

### 5

(C) Stale claims and two dead symbol references left in `charter/frame/slots.py`. `layout.bottom_cols` no longer exists (it is `repos_cols`) yet is still named at lines 421 and 624. `_table_lines`' docstring (line ~369) still says "The repo table `bottom` draws under its attention row", "`bottom` is the frame's full-width slot, so the table goes here", and "*budget* is the pane's real height minus the attention row" — the budget is now the height minus the HEADING, and the attention row is another pane's. The comment at line ~416 still says `layout.visible_slots` keeps the slot "all the way down to `min_cols // 2`, so every frame between 50 and 94 columns draws the attention row and no table", which is precisely the rule #515 replaced. This is the "a spelling standing in for a property" failure the same file warns about three times.

