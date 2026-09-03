# Mutation testing in this repo must run with 'python3 -B' (or clear __pyc

_2026-08-31 00:09 · persistent_

Mutation testing in this repo must run with 'python3 -B' (or clear __pycache__ between runs): a perl-edit that reorders characters keeps the file SIZE, and if the edit and the restore land in the same mtime second, Python reuses the mutant's .pyc. Cost me an hour chasing three phantom test failures on #708 that were the M3 mutant's bytecode still executing while tracebacks printed the restored source.
