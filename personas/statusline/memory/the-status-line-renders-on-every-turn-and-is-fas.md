# The status line renders on EVERY turn and is fast, but it is NOT subproc

_2026-08-14 12:03 · persistent_

The status line renders on EVERY turn and is fast, but it is NOT subprocess-free — the module docstring claimed that for years and it was never true. _run_state runs 'git status --porcelain --branch' per tree, and origin's URL costs another. What is true: BRANCHES never fork (util.branch_of reads .git/HEAD), and nothing new may add a subprocess per ROW — bounded by repos, never by rows. Corrected 2026-08-14 after a test written from the docstring failed. Corrected again 2026-09-04 by #895: 'on EVERY turn' was Claude Code's statusLine calling it, and charter no longer wires that key — the render now reaches a reader through the frame's panels, `charter statusline --watch`, opencode's `/charter`, or a footer the operator wires themselves. The cost rules are unchanged and still bind: the frame's panels are built out of these same renderers.
