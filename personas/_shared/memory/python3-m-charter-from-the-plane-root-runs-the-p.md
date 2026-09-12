# python3 -m charter from the plane root runs the plane's checkout, not yo

_2026-09-11 12:20 · persistent_

LIVE-TESTING A WORKTREE'S charter FROM ANOTHER DIRECTORY SILENTLY RUNS THE WRONG CODE (measured 2026-09-11, fix for #969). 'PYTHONPATH=<worktree> python3 -m charter doctor' run with cwd=/Users/aharon/IdeaProjects/charter imported /Users/aharon/IdeaProjects/charter/charter/__init__.py: -m puts the cwd at sys.path[0], ahead of PYTHONPATH, and the plane root is itself a charter checkout. So the 'after' report was main's code, and it looked like the fix did nothing. Use 'PYTHONPATH=<worktree> python3 -P -m charter …'; -P (3.11+) keeps the cwd off sys.path. Prove it before trusting the output: python3 -P -c 'import charter; print(charter.__file__)'. CONTRIBUTING's 'run python3 -m charter from the clone' is only safe when the cwd IS the clone; a doctor row about the plane root needs cwd=plane root, and that is exactly the case -m gets wrong.
