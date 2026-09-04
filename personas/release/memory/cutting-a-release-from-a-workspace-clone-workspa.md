# Cutting a release from a workspace clone (workspaces/<ws>/charter) is th

_2026-09-04 10:32 · persistent_

Cutting a release from a workspace clone (workspaces/<ws>/charter) is the right place — the plane root REFUSES branch creation ('would create in the PLANE ROOT'). But 4 tests fail there for environmental reasons alone, identically at the unmodified base commit: tests.test_plane_write_guard (2 failures — asserts the checkout IS the real plane root) and tests.test_statusline_crash_guard (2 errors — the tripwire refuses a gl-refresh spawn because $CHARTER_ROOT points at the operator's real plane). Always run the control experiment (git stash, rerun, git stash pop) before believing a release broke something. Also: news.stamp() must be called IN-PROCESS against the clone's own source; the installed CLI's checkout_dir() points at the installed package.
