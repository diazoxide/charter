"""The frame's menu, built so no name ever reaches tmux's parser.

`display-menu` and `display-popup -E` take commands **tmux** parses and executes. Every
interesting label in charter — a workspace, a repo, a branch, a persona — is read out of a
committed file or `.git/HEAD`, which is exactly the input class that made `gh -F` open a
file on someone's machine (`docs/news/unreleased-forge-argv-encoding.md`). Quoting it for
tmux would be an arms race against a parser with `;` separation, `#{}` expansion and two
quote styles.

So the command tmux runs is always `charter frame-action a<N>`, and charter looks the real
argv up in its own state — never re-derives it, never re-quotes it. Labels are still
shown — a label is drawn, never executed — but they are sanitised of the one thing that
could confuse the menu's own layout: newlines.

**`charter frame-action`, not `charter frame action`.** `charter/cli.py`'s
`_split_frame_argv` treats every `charter frame ...` invocation as the launcher's own
escape hatch and grafts everything past `frame` onto the harness's verbatim argv *before*
`argparse` ever sees a subcommand to dispatch on — exactly the reason `charter panel` is
already registered as a top-level sibling of `frame` rather than nested under it. A
`frame action` subcommand would never be reached for the identical reason; `frame-action`
is a different literal token, so it passes through untouched. See `cli.py`'s own
`_add_frame_parsers` for where this is wired.
"""

from __future__ import annotations

import json
import os

from . import state

#: The bound on one label's length, and the reason for it: a `display-menu` row is one
#: line, and a label long enough to wrap would corrupt the menu's own layout rather than
#: merely look ugly — the same "safe by construction" standard this module's other
#: sanitisation (stripping newlines) already applies.
_MAX_LABEL = 60


def _table(fid: str, *, create: bool = False):
    """Path to *fid*'s menu table, or ``None`` when `state.frame_dir` refuses *fid*.

    Delegates entirely to `state.frame_dir`, which resolves through `contain.child`
    rather than sanitising a hostile id (see its own docstring) — rewriting one here
    instead would invent a second identity for exactly the id `frame_dir` exists to
    refuse, and silently write this frame's menu somewhere no other part of the frame
    would ever look for it.
    """
    d = state.frame_dir(fid, create=create)
    return None if d is None else d / "actions.json"


def record(*, fid: str, entries: list[tuple[str, list[str]]]) -> None:
    """Store this frame's menu as ``{id: {label, argv}}``. The only place ids are minted.

    Ids are ``a0``, ``a1``, … — insertion order, never re-derived from the label — so
    `resolve` can answer purely from the id with no dependency on what a label happens to
    say. Written via a temp file + `os.replace`, matching `state.bump`/`record_exit`'s own
    atomic-write shape: `build`/`resolve` are read from a `run-shell` fired by an
    operator's own keypress, which has no way to retry a table caught mid-write.

    A label is drawn by tmux, never executed — see the module docstring — but it is still
    operator-visible text read from a committed file, so newlines are stripped (a menu row
    cannot hold one without corrupting the menu's own layout) and the length is bounded
    (`_MAX_LABEL`) for the same reason.
    """
    path = _table(fid, create=True)
    if path is None:
        return
    data = {f"a{i}": {"label": label.replace("\n", " ")[:_MAX_LABEL], "argv": list(argv)}
            for i, (label, argv) in enumerate(entries)}
    tmp = path.with_name(path.name + ".tmp")
    try:
        tmp.write_text(json.dumps(data))
        os.replace(tmp, path)
    except OSError:
        # Same "must not raise" promise `state.py` makes for its own writes: this can run
        # from `cmd_launch`, where a menu the operator never opens is not worth losing the
        # frame over.
        return


def build(fid: str) -> list[tuple[str, str]]:
    """``(label, opaque id)`` for each entry, in the order `record` stored them.

    `json.loads` rebuilds the dict in the order its keys appeared in the text, which is
    the same order `record`'s own dict comprehension wrote them in — no `sorted()` here on
    purpose: sorting the id STRINGS lexicographically would put ``"a10"`` before ``"a2"``,
    silently reordering the menu the moment a frame ever grows past nine entries.
    """
    path = _table(fid)
    if path is None:
        return []
    try:
        data = json.loads(path.read_text())
    except (OSError, ValueError):
        return []
    if not isinstance(data, dict):
        return []
    return [(v.get("label", ""), k) for k, v in data.items() if isinstance(v, dict)]


def resolve(fid: str, action_id: str) -> list[str] | None:
    """The real argv for *action_id*, or ``None``. The only place an id becomes a command.

    Reads the same table `build` does and nothing more — no name, hostile or not, is ever
    on the path from a menu selection to this return value; `cmd_action` runs whatever
    comes back through `subprocess.run` as a list, never a shell.
    """
    path = _table(fid)
    if path is None:
        return None
    try:
        data = json.loads(path.read_text())
    except (OSError, ValueError):
        return None
    if not isinstance(data, dict):
        return None
    entry = data.get(action_id)
    if not isinstance(entry, dict):
        return None
    argv = entry.get("argv")
    return argv if isinstance(argv, list) and all(isinstance(a, str) for a in argv) else None


def menu_argv(fid: str, socket: str) -> list[str]:
    """The `display-menu` invocation for this frame. Ids only — never a name.

    `-t fid` targets THIS frame's own session explicitly, always. Without it,
    `display-menu` defaults to "the client that fired this command" — exactly right the
    instant a bind's own action runs `display-menu` directly, but nothing about that
    default survives being invoked a second time removed, which is what actually happens
    here: the hotkey bind runs `charter frame-menu` (see `commands_frame.cmd_menu`), and
    THAT process is what calls this function and runs its result as a brand-new `tmux`
    client invocation — a different process from whichever client's keypress triggered
    it. `fid` is the one thing this function is always trusted to carry (it is the
    session's own name, minted by `state.frame_id`'s restricted alphabet — see its
    docstring), so passing it explicitly is free and removes an ambiguity that would
    otherwise only show up with two frames attached in two terminals at once.

    Every item's own action is the fixed template ``run-shell 'charter frame-action
    a<N>'`` — the opaque id is the only thing that varies, and it is never derived from
    the label sitting right next to it in this same argv.
    """
    cmd = ["tmux", "-L", socket, "display-menu", "-t", fid, "-T", "charter"]
    for i, (label, action_id) in enumerate(build(fid)):
        cmd += [label, str(i + 1), f"run-shell 'charter frame-action {action_id}'"]
    return cmd
