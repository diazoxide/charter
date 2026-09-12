# A PIECE'S WORKTREE GETS NO CHARTER LAYER, AND THE OBVIOUS UNWIRE BREAKS

_2026-09-10 17:55 · persistent_

A PIECE'S WORKTREE GETS NO CHARTER LAYER, AND THE OBVIOUS UNWIRE BREAKS ITS CLONE (#951, measured 2026-09-10 on main 2a26c1eb). charter wt add puts a worktree at workspaces/<ws>/.worktrees/<repo>/<piece>, but workspace.guest_trees (used by _wire_clones, wire_harnesses, harness_layer) scans only DIRECT children of workspaces/<ws>/, so reinit, launch and doctor's workspace layer never see it; the tests pin a worktree at workspaces/<ws>/svc-wt, a layout charter never makes. A worktree's info/exclude is its clone's (common dir), so workspace.unwire_guest(worktree) clears the clone's charter block and the clone's git status fills with charter files. git worktree remove already takes the excluded generated files with the directory, so wt remove needs no unwire step.
