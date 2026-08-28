"""Write a gather cache and bump a frame's version, so a panel has something to draw.

Stands in for the hooks that do it in production (`state.bump` from `posttooluse*`), and
for `frame/gather.py`'s own scan — which would read whatever repos happen to be on the
machine running the experiment, and therefore could not produce a repeatable screenshot.
"""
import sys, time
from charter.frame import gather, state

FID = sys.argv[1]
CI = sys.argv[2] if len(sys.argv) > 2 else "failed"

DATA = {
    "gathered_at": time.time(),
    "workspace": "harness-wrapper",
    "current_repo": "charter",
    "repos": [
        {"name": "charter", "branch": "main", "dirty": True, "tracked_dirty": True,
         "ahead": 2, "behind": 0, "ci": CI, "change": 554, "sigil": "!",
         "current": True, "worktree_count": 3},
        {"name": "easydmarc-app", "branch": "feat/dmarc-rollup", "dirty": False,
         "tracked_dirty": False, "ahead": 0, "behind": 4, "ci": "passed",
         "change": None, "sigil": "", "current": False, "worktree_count": 0},
        {"name": "infra", "branch": "main", "dirty": False, "tracked_dirty": False,
         "ahead": 0, "behind": 0, "ci": "running", "change": 88, "sigil": "#",
         "current": False, "worktree_count": 0},
        {"name": "charter-docs", "branch": "docs/frame", "dirty": True,
         "tracked_dirty": False, "ahead": 1, "behind": 1, "ci": "passed",
         "change": None, "sigil": "", "current": False, "worktree_count": 0},
        {"name": "statusline-lab", "branch": "main", "dirty": False,
         "tracked_dirty": False, "ahead": 0, "behind": 0, "ci": None, "change": None,
         "sigil": "", "current": False, "worktree_count": 2},
    ],
    "worktrees": [
        {"name": "pr-554", "branch": "pr-554", "dirty": True, "tracked_dirty": False,
         "ahead": 1, "behind": 0, "ci": "pending", "change": None, "sigil": "",
         "current": False, "worktree_count": 0},
        {"name": "pr-560", "branch": "pr-560", "dirty": False, "tracked_dirty": False,
         "ahead": 0, "behind": 0, "ci": "passed", "change": 560, "sigil": "!",
         "current": False, "worktree_count": 0},
    ],
    "todos": [{"title": "measure the alternate screen"}],
    "todo_count": 1,
}

gather.save(FID, DATA)
state.bump(FID)
print(state.version(FID))
