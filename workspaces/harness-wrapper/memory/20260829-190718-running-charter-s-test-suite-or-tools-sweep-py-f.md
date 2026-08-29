# Running charter's test suite or tools/sweep.py from this harness: PYTHON

_2026-08-29 19:07 · persistent_

Running charter's test suite or tools/sweep.py from this harness: PYTHONSAFEPATH=1 is set in the session environment and it is -P applied globally, so tests/test_self_relaunch_shadowing.py fails (the decoy cwd can never shadow) and tools/sweep.py refuses with 'the tree is RED before any mutation'. Run them as: env -u PYTHONSAFEPATH python3 -m unittest discover -s tests -t . (the -t . is what puts the repo root on sys.path; without it or PYTHONPATH=. you get ModuleNotFoundError: charter). Verified identical on pristine main, so it is the environment and not a repo defect.
