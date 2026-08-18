# Version bumps can be defeated by stale bytecode: 0.32.0 -> 0.33.0 is the

_2026-08-16 20:06 · persistent_

Version bumps can be defeated by stale bytecode: 0.32.0 -> 0.33.0 is the SAME byte length, so if the edit lands in the same second Python's (mtime, size) .pyc check passes and the old __pycache__ is reused — 'charter --version' and the lockstep tests then report the OLD version from a file that on disk says the new one. Clear __pycache__ after a bump, or verify with a fresh interpreter before trusting the suite.
