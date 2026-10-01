## Context & decisions

<!-- Started from charter's Python project template. Replace it with what you learn. -->

How a change in this repo is checked:

- the project's own tools first: read `pyproject.toml` for its test runner, linter and type checker
- `python -m pytest <one file>` while you work, the whole suite before you hand back
- the linter and type checker the project configures (often `ruff check` and `mypy`)

`python-engineer` makes changes and `python-reviewer` reviews them.
