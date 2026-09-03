# A git worktree of the charter repo is NOT an isolated control plane: con

_2026-09-02 23:35 · persistent_

A git worktree of the charter repo is NOT an isolated control plane: config.ROOT and config.STATE_DIR still resolve to the MAIN checkout (measured at 0.55.0 — from a worktree under /private/tmp, STATE_DIR was /Users/aharon/IdeaProjects/charter/.charter). So running the 'charter' CLI from a release worktree mutates the operator's real plane state. Cut a release by calling news.stamp()/news.render_body() in-process instead — news.checkout_dir() IS worktree-relative (it derives from __file__), so the library writes the worktree's docs/news while the CLI would touch the real plane.
