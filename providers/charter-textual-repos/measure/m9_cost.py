"""M9 — what a repaint costs, charter's own repo table against the Textual adapter.

Both measured through `Registry.draw`, at the same rectangle, off the same snapshot, in
one process — so the number is the renderer and not the process start. The panel's tick is
`panel.TICK` = 0.2 s, so the budget for a repaint is 200 ms and neither is close to it;
what the comparison is for is the SHAPE of the cost, since a Textual repaint is a full
compositor pass over a widget tree and charter's is a string join.

Import cost and RSS are measured separately, in a fresh interpreter each, because both are
paid once per panel process and a warm import measures nothing.
"""

from __future__ import annotations

import resource
import subprocess
import sys
import time

from charter.frame import ctx as charter_ctx
from charter.frame import registry

SNAP = {
    "gathered_at": time.time(), "workspace": "harness-wrapper",
    "current_repo": "charter",
    "repos": [{"name": f"repo-{i}", "branch": "main", "dirty": i % 2 == 0,
               "tracked_dirty": True, "ahead": i, "behind": 0,
               "ci": ("failed", "passed", "running")[i % 3], "change": i or None,
               "sigil": "!", "current": i == 0, "worktree_count": i % 3}
              for i in range(8)],
    "worktrees": [], "todos": [], "todo_count": 0,
}
W, H = 120, 12


def timed(fn, n=40):
    fn()                                        # warm
    ts = []
    for _ in range(n):
        a = time.perf_counter()
        fn()
        ts.append(time.perf_counter() - a)
    ts.sort()
    return ts[len(ts) // 2] * 1000, ts[-1] * 1000


def main() -> int:
    reg = registry.Registry()
    reg.place("textual.repos")

    def textual():
        reg.draw("textual.repos", charter_ctx.build(("gather",), width=W, height=H,
                                                    fid="m9", snapshot=SNAP))

    from charter.frame import slots
    from unittest import mock

    def charter_own():
        # `slots._table_lines` is the repo table itself, minus the pane chrome around it —
        # the closest like-for-like to what the Textual widget draws.
        slots._table_lines(SNAP, W, H - 1)

    for name, fn in (("charter _table_lines", charter_own),
                     ("textual.repos draw ", textual)):
        med, worst = timed(fn)
        print(f"  {name}  median {med:6.2f} ms   worst {worst:6.2f} ms")

    from charter_textual_repos import adapter
    adapter._HOST.stop()

    print("\n  import cost and peak RSS, one fresh interpreter each:")
    for label, mod in (("charter.frame.slots", "charter.frame.slots"),
                       ("textual.app", "textual.app"),
                       ("charter_textual_repos", "charter_textual_repos.ui")):
        code = (f"import time,resource;a=time.perf_counter();import {mod};"
                f"b=time.perf_counter();"
                f"print(f'{{1000*(b-a):.0f}} "
                f"{{resource.getrusage(resource.RUSAGE_SELF).ru_maxrss//(1024*1024)}}')")
        out = subprocess.run([sys.executable, "-c", code], capture_output=True,
                             text=True).stdout.split()
        print(f"    {label:24s} {out[0]:>5s} ms   {out[1]:>3s} MB peak RSS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
