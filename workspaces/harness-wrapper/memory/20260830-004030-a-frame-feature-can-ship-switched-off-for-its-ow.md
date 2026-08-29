# A frame feature can ship switched off for its own operator: #658's repo-

_2026-08-30 00:40 · persistent_

A frame feature can ship switched off for its own operator: #658's repo-table scroll was measured only on many-clones planes, and the control plane it shipped from is one clone with many worktrees, where the repo-counted bound is always 0 (#663). When a renderer's unit (rows) and a bound's unit (repos) differ, test BOTH plane shapes — tests/test_a_real_click_reaches_the_real_repo_table.py now has a base class so the gather cache is what a subclass supplies. Probe: inject SGR reports with 'tmux -L <sock> send-keys -t <pane> -l $'\033[<65;10;5M'' — a panel reads events off its own pane's fd, and a tmux pane's 0/1/2 are one pty.
