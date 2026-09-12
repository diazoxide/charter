# Claude Code settings discovery, measured on 2.1.267 with claude -p (fix

_2026-09-10 18:02 · persistent_

Claude Code settings discovery, measured on 2.1.267 with claude -p (fix round 1 of #942): .claude/settings.json is read from the session cwd only (no walk-up); .claude/settings.local.json is read at the git root (main checkout for a linked worktree) AND in the starting directory. A nested clone's session ignores the outer repo's local file. So a workspace dir inside the plane repo already reads the plane's own local file; only a clone needs a generated one — and in a clone that file is co-written by the harness ('Yes, and don't ask again' lands there).
