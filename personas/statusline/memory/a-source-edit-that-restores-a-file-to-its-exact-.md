# A source edit that RESTORES a file to its exact previous size within the

_2026-08-20 13:25 · persistent_

A source edit that RESTORES a file to its exact previous size within the same second (a mutation test, a revert) leaves a stale .pyc valid — pyc freshness is (mtime-seconds, size), and both match. The suite then tests code that is no longer on disk. Clear __pycache__ after any in-place mutate-then-restore.
