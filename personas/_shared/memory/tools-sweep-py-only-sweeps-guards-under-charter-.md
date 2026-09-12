# tools/sweep.py only sweeps guards under charter/, never tests/. A change

_2026-09-10 17:44 · persistent_

tools/sweep.py only sweeps guards under charter/, never tests/. A change to test infrastructure (tests/_planeguard.py, tests/_isolation.py and similar) gets 0 mutations from the sweep, which proves nothing; it needs a hand sweep: delete each new guard line in memory (python3 -B, an exported copy, never the worktree) and confirm a test goes red. On 2026-09-10 the #944 fix's hand sweep found two survivors the tool could not see, and its re-review needed a 19-layout real-git table to prove the plane-write pin was never narrower than main.
