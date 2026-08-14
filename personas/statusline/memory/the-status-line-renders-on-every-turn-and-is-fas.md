# The status line renders on EVERY turn and is fast, but it is NOT subproc

_2026-08-14 12:03 · persistent_

The status line renders on EVERY turn and is fast, but it is NOT subprocess-free — the module docstring claimed that for years and it was never true. _run_state runs 'git status --porcelain --branch' per tree, and origin's URL costs another. What is true: BRANCHES never fork (util.branch_of reads .git/HEAD), and nothing new may add a subprocess per ROW — bounded by repos, never by rows. Corrected 2026-08-14 after a test written from the docstring failed.
