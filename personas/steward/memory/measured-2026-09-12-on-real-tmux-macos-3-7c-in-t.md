# Measured 2026-09-12 on real tmux (macOS, 3.7c) in tests/test_a_new_chat_

_2026-09-12 20:12 · persistent_

Measured 2026-09-12 on real tmux (macOS, 3.7c) in tests/test_a_new_chat_starts_at_the_profile_selector.TheSelectorOnARealServer: palette.own_the_tty's raw mode works fine in a LIVE chat pane — the selector paints, send-keys Enter picks a row and the launcher execs the harness keeping its pid, and send-keys Escape exits 130 and takes the workspace session with the window. So ruling 42's tcsetattr hazard (a launcher killed by a signal on a Linux runner, empty pane_dead_status) was about a pane the chat teardown was ALREADY killing, not about raw mode as such. Also: a launch with attach=False returns as soon as the window exists, so a cancelled selector's 130 is read off state.exit_code(fid) — written by the pane-died hook — never off _launch's return value.
