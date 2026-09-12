# tmux numbers panes per SERVER: a fresh server first pane is %0 again (me

_2026-09-10 22:57 · persistent_

tmux numbers panes per SERVER: a fresh server first pane is %0 again (measured 3.7c), and a server born from a client env hands that env (e.g. TERM_SESSION_ID) to every later pane even when the opening client lacks it. So any pointer keyed on TMUX_PANE or an inherited TERM_SESSION_ID is read later by an unrelated chat; workspace._prune keeps terminal pointers 30 days. Filed as #953 for persona use.
