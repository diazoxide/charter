# Fleet design (ADR 0011/0012, 2026-08-13): a piece IS a worktree and the

_2026-08-13 22:17 · persistent_

Fleet design (ADR 0011/0012, 2026-08-13): a piece IS a worktree and the claim IS its creation — but that mutex is only real if the WORKER creates its own worktree. An orchestrator that pre-creates N worktrees then starts N workers wins a race nobody ran, while the real race (two workers in one tree) goes unguarded. Pre-assignment makes git's atomicity decorative.
