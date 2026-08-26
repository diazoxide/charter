# overlay
PR: https://github.com/diazoxide/charter/pull/554 (OPEN, not merged)
Branch: phase2-task2-overlay-surface

## Blocking

### 1

cmd_launch's escape-hatch arming is entirely unpinned. I deleted the whole `hatch = overlay.arm_hatch_argv(SOCKET, harness=harness_pane)` block (charter/commands_frame.py:2508-2520, both the None-warn branch and the tmuxctl.run) and the FULL suite stayed GREEN: 5933 tests, OK. I then measured the shipped consequence on real tmux 3.7c - with @charter_hatch unset, `bind -n F12 run-shell -C '#{@charter_hatch}'` expands to nothing and F12 is a silent no-op (no pane killed, no focus move, no error, session intact). The five real-tmux tests do not catch it because each arms the option itself (test_with_no_overlay_open_the_key_still_returns_to_the_harness calls overlay.arm_hatch_argv directly; the others go through modal_argvs), so they verify tmux's half of the mechanism and never charter's launch half. Needed: a test in tests/test_frame_launcher.py asserting cmd_launch issues the arm argv naming the harness pane id, and that it is issued BEFORE the first panel split - the ordering the overlay module docstring calls load-bearing. Mutation to keep RED: remove that block.

