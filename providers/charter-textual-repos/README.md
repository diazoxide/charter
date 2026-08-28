# charter-textual-repos

**An experiment, not a product.** Charter's repo table drawn with
[Textual](https://github.com/textualize/textual), shipped as a third-party provider through
charter's existing `charter.components` entry point.

The question it was built to answer: **does charter's provider model already permit a rich
widget framework without charter taking the dependency?** The answer, the measurements
behind it, and what it would take to ship something like this are in
[`docs/superpowers/specs/2026-08-28-textual-frame-component-experiment.md`](../../docs/superpowers/specs/2026-08-28-textual-frame-component-experiment.md).

## The premise, on one line

charter's `pyproject.toml` says `dependencies = []`, asserted by
`tests/test_packaging.py::test_runtime_has_zero_dependencies`. This package's says
`dependencies = ["charter-cp", "textual>=0.80"]`. charter finds it with
`importlib.metadata`, which is stdlib. **Nothing in `charter/` changed to make this work.**

## Two components

| id | shape | what it is for |
|---|---|---|
| `textual.repos` | Textual on the **headless** driver, on a background thread; its composited screen copied out as lines | obeys `render(ctx) -> list[str]` exactly as written |
| `textual.live` | Textual on the **pane's own tty** — alternate screen, own event loop, `mouse=True` | `render` never returns; the only shape where keys, mouse and scrolling exist |

Both read `ctx.gather` and nothing else. Place either from a committed `charter.toml`:

```toml
[[frame.component]]
use = "textual.live"
edge = "bottom"
size = 12
```

## Install

```sh
uv venv --python 3.14 .venv
uv pip install --python .venv/bin/python -e . -e providers/charter-textual-repos
```

## Layout

```
src/charter_textual_repos/
  __init__.py   API_VERSION and the two Component factories the entry points name
  rows.py       the data, derived from ctx.gather and from nothing else
  ui.py         the Textual app — one class, driven two ways
  adapter.py    textual.repos: headless, screen copied out as lines
  live.py       textual.live: the takeover
tests/          pytest (this package is not charter's tree, so not unittest)
measure/        the tmux rig every number in the report was taken on
```

## Apparatus

Three env vars exist only so the experiment is reproducible. None is a feature.

| | |
|---|---|
| `CHARTER_TEXTUAL_LIVE_REFRESH=1` | the live app polls `charter.frame.state`/`gather` itself — i.e. reaches around `ctx`. Measures what "stay live" would cost a provider. |
| `CHARTER_TEXTUAL_FAULT=render\|loop` | inject a crash before the app starts, or inside Textual's message pump. Measures blast radius. |
| `CHARTER_TEXTUAL_KEEP_CAPTURE=1` | do not undo Textual's `sys.stdout` capture. Reproduces the silent blank pane described in §2 of the report. |
