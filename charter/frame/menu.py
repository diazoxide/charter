"""The frame's menu, built so no name ever reaches tmux's parser.

`display-menu` and `display-popup -E` take commands **tmux** parses and executes. Every
interesting label in charter — a workspace, a repo, a branch, a persona — is read out of a
committed file or `.git/HEAD`, which is exactly the input class that made `gh -F` open a
file on someone's machine (`docs/news/unreleased-forge-argv-encoding.md`). Quoting it for
tmux would be an arms race against a parser with `;` separation, `#{}` expansion and two
quote styles.

So the command tmux runs is always `charter frame-action a<N>`, and charter looks the real
argv up in its own state — never re-derives it, never re-quotes it.

**A label is a tmux FORMAT, not inert text — it is drawn, but "drawn" still means
tmux expands it first.** `display-menu`'s own docs: "The name and command are formats,
see the FORMATS and STYLES sections." `#(shell command)` runs the command and substitutes
its output; `#{variable}` substitutes a value — both fire the moment tmux RENDERS the
menu, no selection needed. Verified by hand against tmux 3.7c, in a real frame: a label of
`#(touch CANARY)` created `CANARY` the instant the menu was drawn, and the hostile row
itself was invisible in the rendered menu — nothing about the menu LOOKED wrong. An
earlier version of this module's docstring said "a label is drawn, never executed"; that
was false, and a wrong invariant asserted by a passing test is worse than no test at all
— see `_safe_label` for the fix (`#` -> `##`, tmux's own escape for a literal `#`) and
`tests/test_frame_menu.py` for the canary that proves it closed.

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
import re

from . import state

#: The bound on one label's length, and the reason for it: a `display-menu` row is one
#: line, and a label long enough to wrap would corrupt the menu's own layout rather than
#: merely look ugly — the same "safe by construction" standard this module's other
#: sanitisation (stripping newlines) already applies.
_MAX_LABEL = 60

#: What `record` ever mints — `a0`, `a1`, … — and the ONLY shape `build` will hand back to
#: a caller that turns it into a command (`menu_argv`). Checked again here, at the point
#: an id is about to reach an argv, rather than trusted because `record` is the only
#: writer TODAY: the guard belongs at the join, not at whichever writer happens to exist
#: first — a hand-edited or otherwise corrupted `actions.json` key of `a0'; run-shell
#: "..."` would otherwise reach `menu_argv`'s f-string untouched and become exactly the
#: injection this whole module exists to refuse.
_ACTION_ID_RE = re.compile(r"^a[0-9]+$")


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

    A label is still operator-visible text read from a committed file (see the module
    docstring for why "drawn" does not mean "inert"), so newlines are stripped (a menu row
    cannot hold one without corrupting the menu's own layout) and the length is bounded
    (`_MAX_LABEL`) for the same reason. The tmux-format escaping (`#` -> `##`) and the
    leading-`-`/trailing-`#` guards live in `menu_argv` instead, at the point a label
    actually reaches tmux's argv — this function's own job is making the STORED text
    sane, not making it safe for a parser it has not met yet.

    An empty result (the label was empty, or entirely newlines) falls back to a
    placeholder rather than an empty string: `display-menu` treats an empty NAME as a
    separator line, which desynchronises every `label key command` triple after it and
    fails the whole menu outright with "not enough arguments" — the hotkey would then
    silently do nothing, for a reason with no message anywhere to explain it.
    """
    path = _table(fid, create=True)
    if path is None:
        return
    data = {}
    for i, (label, argv) in enumerate(entries):
        clean = label.replace("\n", " ")[:_MAX_LABEL] or "(untitled)"
        data[f"a{i}"] = {"label": clean, "argv": list(argv)}
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

    Any key not shaped `a<N>` is dropped rather than passed through — see
    `_ACTION_ID_RE`'s own docstring for why this is checked here, at the join, and not
    only trusted from `record`.
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
    return [(v.get("label", ""), k) for k, v in data.items()
            if isinstance(v, dict) and _ACTION_ID_RE.fullmatch(k)]


def resolve(fid: str, action_id: str) -> list[str] | None:
    """The real argv for *action_id*, or ``None``. The only place an id becomes a command.

    Reads the same table `build` does and nothing more — no name, hostile or not, is ever
    on the path from a menu selection to this return value; `cmd_action` runs whatever
    comes back through `subprocess.run` as a list, never a shell. `action_id` is not
    re-checked against `_ACTION_ID_RE` here: a `dict.get` lookup is safe against a key of
    any shape (there is no parser downstream of it to confuse), unlike `menu_argv`, which
    interpolates the id into text tmux re-parses and is where that check actually matters.
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


def _safe_label(label: str) -> str:
    """*label*, made inert against tmux's own format/style parsing. Three transforms:

    1. **`#` -> `##`.** The fix for the format-expansion hole the module docstring
       describes: `#(...)`/`#{...}` in an unescaped label execute/substitute the moment
       tmux draws the menu. `##` is tmux's own escape for a literal `#` — doubling every
       occurrence closes it the same way `_pane_died_write_hook_argv` closes `$`/`"` in a
       hook action: escape every occurrence, not a scan for "looks like a format".

    2. **A leading `-` disables the item.** `display-menu`'s own docs: a name starting
       with `-` is "shown dim and may not be chosen" — the whole row, not merely its
       first character, becomes something else. That is exactly correction 4's "truncated
       into something misleading" failure, reached through a menu instead of a status
       line. A leading space keeps the text intact and un-disables the row.

    3. **A label ending in `#` gets a trailing space.** Cosmetic only — nothing here
       executes either way — but worth closing: verified by hand that a label doubled
       from a single trailing `#` (`"trailing#"` -> `"trailing##"`) collides with a
       style-reset sequence tmux appends after every item's own name, rendering as
       literal `trailing#[default]` garbage in the menu. A label with no trailing hash
       never showed it. A trailing space breaks the adjacency.
    """
    label = label.replace("#", "##")
    if label.startswith("-"):
        label = " " + label
    if label.endswith("#"):
        label = label + " "
    return label


def menu_argv(fid: str, socket: str, client: str) -> list[str]:
    """The `display-menu` invocation for this frame. Ids only — never a name.

    `-c client` is what selects WHICH ATTACHED TERMINAL the menu is drawn on —
    `display-menu`'s own docs: "Display a menu on target-client. target-pane gives the
    target for any commands run from the menu." `-t` (kept below) never did that; it only
    scopes FORMAT EVALUATION for the item's own command text. Verified by hand against
    tmux 3.7c with two frames attached in two separate terminals: `-t fid` alone rendered
    frame B's menu on frame A's screen when B's own hotkey was pressed — B's operator saw
    nothing, and selecting the item there would have run B's action from A's terminal.
    *client* is resolved by `commands_frame.cmd_menu` (via `list-clients -t fid`) before
    this function is ever called — `menu.py` makes no subprocess calls of its own (every
    other function here is pure file/state access), so the one query `-c` needs lives in
    `commands_frame.py` and the answer is simply handed in.

    `-t fid` stays for a DIFFERENT reason: it is what scopes the ITEM's own `run-shell`
    command to this session's `$CHARTER_SESSION_ID` (verified by hand: an item fired from
    `display-menu -t <session>` inherits that session's own `set-environment` values even
    though its own `run-shell` text carries no `-t` — see `_session_id_env_argv`'s own
    docstring in `commands_frame.py` for why that value has to be there at all).

    Every label passes through `_safe_label` — see its own docstring and the module
    docstring for why a label is not inert text. Every item's own ACTION is the fixed
    template ``run-shell 'charter frame-action a<N>'`` — the opaque id is the only thing
    that varies, and `build` already refuses anything not shaped `a<N>` before it ever
    reaches here.
    """
    cmd = ["tmux", "-L", socket, "display-menu", "-t", fid, "-c", client, "-T", "charter"]
    for i, (label, action_id) in enumerate(build(fid)):
        cmd += [_safe_label(label), str(i + 1), f"run-shell 'charter frame-action {action_id}'"]
    return cmd
