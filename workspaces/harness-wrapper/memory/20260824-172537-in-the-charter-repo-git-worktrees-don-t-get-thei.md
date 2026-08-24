# In the charter repo, git worktrees don't get their own control plane: ch

_2026-08-24 17:25 · persistent_

In the charter repo, git worktrees don't get their own control plane: charter/root.py's find_root() redirects a linked worktree with no reachable charter.toml back to the MAIN tree's charter.toml. So any test/script run from inside a worktree that doesn't fully isolate config (PersonaIso, or explicit config.use()) reads config.UPDATE/config.ROOT from the operator's REAL live plane, not a fixture. Bit test_statusline_brand.py:UpdateIndicator and test_version_lock.py:AutoSync (#459): they patch config.ROOT directly but never touch config.UPDATE, so they fail for real once the real plane declares [update] channel = "dev" — reproduces even in single-test isolation, confirmed identical on a clean origin/main worktree.
