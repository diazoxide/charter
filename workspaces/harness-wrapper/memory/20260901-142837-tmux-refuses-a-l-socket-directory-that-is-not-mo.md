# tmux refuses a -L socket directory that is not mode 0700: 'directory <pa

_2026-09-01 14:28 · persistent_

tmux refuses a -L socket directory that is not mode 0700: 'directory <path>/tmux-<uid> has unsafe permissions', rc 1. Measured on 3.7c while giving tests/test_the_suite_reaps_its_own_tmux_servers its own $TMUX_TMPDIR for #781 — Path.mkdir() at the default umask makes 0755 and every plant failed. The two scan classes in the same file create that directory without mode= and are fine because they never start a server in it. Also: #781's race reproduces cold at 5 failures in 6 runs when the reaper class shares /tmp/tmux-<uid> with a second suite, in BOTH directions (a sibling reaped my plant; I reaped a sibling's) — the second direction only shows once the assertions are sharpened from assertIn to assertEqual, which the private directory is what makes correct.
