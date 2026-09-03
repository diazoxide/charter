# Removing a rung from a ladder silently de-fangs the tests that pinned th

_2026-09-01 13:44 · persistent_

Removing a rung from a ladder silently de-fangs the tests that pinned the OTHER rungs. #791 dropped the per-session pointer from state.own_workspace and four tests near it stopped measuring their own guards while staying green: the pin's .strip(), workspace_for's rung 0 (twice) and of_workspace's is_dir() filter. Their fixtures had all leaned on the removed rung being the disagreeing value. Fix: after removing a rung, re-run each neighbouring guard's mutation by hand — do not trust green.
