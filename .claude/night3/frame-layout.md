# frame-layout
PR: https://github.com/diazoxide/charter/pull/500 — commit 7e4f395, mergeable, CI pass on 3.11-3.14. Round-2 disposition posted as https://github.com/diazoxide/charter/pull/500#issuecomment-5402635204. Not merged.
Branch: frame-layout-488
Weaker: Not weaker. Round-2 changes only tighten, surface by surface: `bottom_rows_wanted` and `_reassert_sizes` each gained a REQUIRED keyword argument, so a caller that omits the width is a TypeError at the call site rather than a silent default of "wide" (pinned by test_the_width_is_required_and_cannot_be_confused_with_the_row_count). `_table_cap` returns strictly fewer rows than the old repo-count answer at every input, never more. Nothing new reaches a tmux argv: `window_cols` comes from `_window_size`, which already isdigit-validates tmux's own display-message output and falls back to `_FALLBACK_SIZE`, and it is used only in arithmetic — it never reaches hook action text, and the only thing that reaches `resize-pane` is still `str(int)` out of `layout.slot_sizes`. Every pre-existing guard is untouched: `_PANE_ID_RE` on every pane id, the `slot not in _RESIZE_FLAG: continue` filter, `_relayout`'s shape check hoisted above the `want` branch, `_action_word_is_safe`/`_FRAME_ID_RE` on the resize hook. No new file is read or written; no parsing widened. A smaller `bottom` cannot weaken anything — `bottom_rows`' floor is still SLOT_SIZE["bottom"] = 1, so it never asks tmux for a 0-row split.

## Bypass

YES — (A). With `[frame] slots` naming `right` before `bottom`, the sizer is still handed the WINDOW's width while the pane gets the window's width minus `right`'s 22 columns and its border, so `bottom` is again split for a table it then refuses to draw. Reproduced end to end on tmux 3.7c with real `charter panel` processes: window 110x40, `slots = ["right", "top", "bottom"]`, 6 repos in the gather cache → `_launch_sizes` returns `bottom: 7`, tmux reports the bottom pane as **87x7**, and the panel draws **1 line** with 6 rows blank. Control, same window, shipped order: pane 110x7, draws 7. Worst measured: 14 repos → 15-row pane, 1 line drawn, 14 blank; harness 22 rows where it should have 36.

## Blocking

### 1

(A) `bottom` is STILL sized for a table it refuses to draw, whenever `[frame] slots` puts `right` before `bottom`. Reproduced end-to-end on real tmux 3.7c with real `charter panel` processes, in an isolated plane (`config.ROOT` printed as a scratchpad dir, never the worktree).

Mechanism: `layout.panel_argvs` splits every slot off the harness pane in LIST order, and `instance.frame_of` preserves the operator's order verbatim (`kept = [s for s in value if s in FRAME_SLOTS]`). Split `right` first and the harness pane is already 22+1 columns narrower, so the `bottom` pane that comes off it next is `cols - 23` wide — not `cols`. `_launch_sizes(fid, want, cols, rows)` / `_relayout(window_cols=…)` / `_reassert_sizes(window_cols=…)` all pass the WINDOW's width to `bottom_rows_wanted`, so `_table_cap` answers `_MAX_REPO_LINES` while `_bottom`, measuring its own pane, answers 0.

Measured (tmux 3.7c, private socket, `PYTHONPATH` on a `git archive` extraction of 7e4f395, `CHARTER_ROOT` on a temp plane, 6 repos in the gather cache):
  window 110x40, slots [right, top, bottom] -> `_launch_sizes` = {right:22, top:1, bottom:7}; tmux reports the bottom pane 87x7; the panel draws ONE line ('0 todos · F2 menu'); 6 rows blank.
  control, same window, shipped order [top, bottom, right] -> pane 110x7, draws 7 lines. 
  14 repos: pane 87x15, draws 1, 14 blank; harness 22 rows where it should have 36.

Break band is window widths 100..117 inclusive: below 100 `min-cols` drops `right` and `bottom` is full width again; at 118+ the pane is >= `_LEFT_W` (95) and the table draws. Sweep at 6 and 14 repos: 200/130/120/118 clean, 117/110/100 blank, 99 clean.

Not launch-only. `_reassert_sizes` never consults the slot order at all — it sizes from `window_cols` — so `cmd_resize` re-applies the same over-tall pane on every step of a terminal drag, exactly the shape round 1 flagged.

This is the PR's own documented geometry, not a corner I invented: `tests/test_frame_config.py::test_the_operators_own_slot_order_is_kept_exactly` says, measured on tmux 3.7c at 200x50, that `["top","right","bottom"]` gives 'a 177-column bottom row inset beside the sidebar versus a full-width 200-column one', and calls preserving that order 'a promise'. So the branch knows the pane can be narrower than the window and sizes it as if it cannot. No test covers a reordered `slots` — `BottomIsSplitForWhatItCanDraw` only ever launches with the shipped list.

Fix inside the existing seam, no widening: the launcher/relayout/resize sites already know `want`; when `right` precedes `bottom` in it, the pane's width is `cols - layout.SLOT_SIZE['right'] - 1`, and that is what `bottom_rows_wanted` must be given. (Canonicalising the split order to `FRAME_SLOTS` would also make the docstrings true, but it breaks the promise the config test pins, so passing the real width is the smaller change.) A regression test with `slots = ['right','top','bottom']` at cols 110 belongs beside `test_the_boundary_is_the_tables_own_width`.

Files: /Users/aharon/IdeaProjects/charter/charter/commands_frame.py (`_launch_sizes` :1070/:1095, `_relayout` :2491, `_reassert_sizes` :2544), /Users/aharon/IdeaProjects/charter/charter/frame/slots.py (`_table_cap` :426).

### 2

(C) Three sentences assert the exact invariant finding 1 breaks, and they are what let it through review — narrow them in the same commit as the fix.
(1) /Users/aharon/IdeaProjects/charter/charter/frame/slots.py:456, `_table_cap`: 'Same number by construction — `bottom` is split BEFORE `right` (`instance.FRAME_FIELDS`'' order is the geometry), so the pane''s width IS the window''s.' False: `FRAME_FIELDS` supplies the DEFAULT order; the operator''s own `slots` order is preserved and is the geometry, which the config test above states explicitly.
(2) /Users/aharon/IdeaProjects/charter/charter/commands_frame.py:1087, `_launch_sizes`: '`bottom` is split BEFORE `right`, so the window''s width IS the pane''s.' Same claim, same falsity — and this is the call site that passes the wrong number.
(3) /Users/aharon/IdeaProjects/charter/charter/frame/slots.py:351, `_table_lines`'' comment: '`bottom` is split BEFORE `right` (the slot order IS the geometry — `instance.FRAME_FIELDS`), so its width is the whole WINDOW''s.'
The same claim reaches the operator twice more, and both need the same correction: /Users/aharon/IdeaProjects/charter/docs/news/unreleased-the-repo-table-moves-to-the-bottom.md ('the launcher, the `window-resized` recompute and the panel itself all ask one function ... and they ask it with the same width') and /Users/aharon/IdeaProjects/charter/docs/frame.md:265 ('the launcher and the resize hook ask the same function the panel asks, with the same width and the same density').

### 3

(C) Two measured overstatements about what `minimal` costs — code is right, the sentences are not. Measured with `bottom_rows_wanted(fid, cols=200)` at both levels, isolated plane:
  repos:  0  1  2  3  4  5  8  10  14  20
  normal: 1  2  3  4  5  6  9  11  15  15
  minimal:1  2  3  4  5  5  5   5   5   5
So the `minimal` pane is at most FIVE rows (attention row + `_TERSE_ROWS`), and it is 'four rows shorter' at exactly one repo count (8) — 0 rows shorter below 5 repos, 10 rows shorter at 14.
(1) /Users/aharon/IdeaProjects/charter/docs/frame.md density table: '`minimal` | ... four rows of repo table — and a `bottom` pane that is four rows tall'. The pane is five rows tall.
(2) /Users/aharon/IdeaProjects/charter/docs/frame.md prose and the identical sentence in /Users/aharon/IdeaProjects/charter/docs/news/unreleased-the-repo-table-moves-to-the-bottom.md: 'The pane is four rows shorter to match'. It is shorter by however much the table was over `_TERSE_ROWS`, which is 0 on a small plane and 10 on a full one. 'The pane is sized for those four rows instead of all of them' says what the code does.

