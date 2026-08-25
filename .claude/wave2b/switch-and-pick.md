# switch-and-pick
PR: https://github.com/diazoxide/charter/pull/534
Branch: frame-switch-and-pick

## Blocking

### 1

(A) `charter/frame/menu.py:418` — `_key` returns `"-"` for rows past the ninth, and `-` is a REAL tmux key, not "no shortcut". The docstring says `-` is "tmux's own spelling for a row with no key bound, still selectable with the arrow keys", and `docs/frame.md:538` repeats it ("Rows past the ninth have no number key; the arrow keys still reach them"). Measured against tmux 3.7c on an attached pty: a menu of `row-a 1 …`, `row-b - …`, `row-c - …`, `row-d '' …` renders as `row-a (1)`, `row-b (-)`, `row-c (-)`, `row-d` (no key) — and pressing `-` RAN row-b's command. Empty string is tmux's actual no-key, and I confirmed an empty-key row is still arrow-selectable (Down Down Enter fired row-c). So in the workspace submenu I rendered, a stray `-` keystroke performs a real workspace switch to `ws08`, and the three rows below it advertise `(-)` as a shortcut that does nothing. `tests/test_frame_switch.py:332` (`test_the_tenth_row_gets_no_key_rather_than_an_impossible_one`) asserts `keys[9:] == ["-", "-"]`, encoding the wrong value — my "no cap at all" mutation went RED against it, so the test is real, it just pins the wrong constant. Fix in this commit: `return str(i + 1) if i < 9 else ""`, plus the `_key` docstring, the `docs/frame.md` sentence, and that assertion.

### 2

(A) `charter/commands_frame.py:3234` `cmd_switch` never re-records the menu, so after a switch the F2 menu keeps naming the workspace the frame LEFT. Measured on a real plane: after `switch.to_workspace(fid, "ws05")` with `state.workspace_for(fid) == "ws05"`, `menu.build(fid, MAIN)` still returns `workspace: ws00  ▸` and `menu.build(fid, "workspace")` still marks `* ws00`. The panels repaint correctly; the menu lies. This is exactly the rule the branch already applies one command over — `cmd_density` re-records with the comment "Re-recorded so the menu's own mark moves with the frame" (`commands_frame.py:3146-3156`) — and it was not applied to the command the PR is about. The same staleness means a workspace or persona created after launch never appears in the submenu at all. Fix: on a successful outcome in `cmd_switch`, mirror `cmd_density`'s `menu.record(fid=fid, entries=_menu_entries(fid, socket, current=_current_density(fid)))`. No test covers this; add one asserting the mark moves.

