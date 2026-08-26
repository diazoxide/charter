"""Developer tooling. Not shipped — `pyproject.toml` names `charter` as the only package.

A package rather than a bare directory so `tests/test_sweep.py` can `from tools import
sweep` under `python -m unittest discover -s tests -t .`, the way every other test in
this repository imports the thing it is about.
"""
