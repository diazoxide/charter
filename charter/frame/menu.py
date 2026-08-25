"""The frame's menu, built so no name ever reaches tmux's parser.

`display-menu` takes commands **tmux** itself parses and executes (so does `display-popup
-E`, which charter does not use). Every
interesting label in charter — a workspace, a repo, a branch, a persona — is read out of a
committed file or `.git/HEAD`, which is exactly the input class that made `gh -F` open a
file on someone's machine (`docs/news/unreleased-forge-argv-encoding.md`). Quoting it for
tmux would be an arms race against a parser with `;` separation, `#{}` expansion and two
quote styles.

So the command tmux runs is always `charter frame-action a<N>`, and charter looks the real
argv up in its own state — never re-derives it, never re-quotes it.

**A label is a tmux FORMAT, not inert text.** `display-menu`'s own docs: "The name and
command are formats, see the FORMATS and STYLES sections." `#(shell command)` runs the
command and substitutes its output; `#{variable}` substitutes a value — both fire during
FORMAT EVALUATION, which happens whether or not the resulting menu goes on to display
successfully. Verified through the real production path (`bind` -> `run-shell` ->
`charter frame-menu` -> `display-menu`), not a lab stand-in, including with a real git
branch literally named `#(id>/tmp/...)` — a shape `git checkout -b` accepts without
complaint and `.git/HEAD` holds unchanged: the job ran and the branch name itself never
appeared anywhere in the rendered menu.

**There is no accidental defence anywhere in that path.** An earlier version of this
docstring claimed a label that is ENTIRELY a silent `#(...)` job (nothing visible before
or after it, and the command writing no stdout — `touch`, say) expands to an empty
string and so REFUSES THE WHOLE MENU. Re-measured against tmux 3.7c through a real bind,
with a real attached client: `display-menu` returned **0**, the menu **opened**, the
sibling item stayed **selectable**, and the canary **fired**. The mechanism is that tmux
counts a command's arguments BEFORE expanding formats, so a name that expands to nothing
still occupies its own slot and nothing desynchronises. The refusal is real for a
LITERALLY empty name (`""` — rc 1, tmux's own text is `not enough arguments`, see
`record`), and that is a different case from a name that merely expands to empty. Both
claims mattered: this one was cited as a reason the hole was partly self-limiting, and
it never was.

An earlier version of this docstring also said "a label is drawn, never executed"; that
was false too, and a wrong invariant asserted by a passing test is worse than no test at
all — see `_safe_label` for the fix (`#` -> `##`, tmux's own escape for a literal `#`,
applied before tmux ever sees the label, which closes it regardless of how many times it
is later collapsed for display — pre-doubled payloads (`##(...)`, `####(...)`) were
tried by hand against the fix and found no hole) and
`tests/test_frame_menu.py`/`tests/test_frame_tmux_integration.py` for the canaries that
prove it closed.

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
from typing import NamedTuple

from . import state, tmuxctl

#: The top-level menu — what `F2` opens, and what every table written before groups
#: existed is read as. Named rather than spelled `"main"` at four call sites.
MAIN = "main"

#: The two submenus (#517). Named rather than spelled at each call site, and indexed by
#: name rather than by position in :data:`GROUPS` — a list a future group is appended to
#: must not silently re-point the workspace menu at the persona one.
WORKSPACES = "workspace"
PERSONAS = "persona"

#: The closed set of menu groups, and **the only thing between a group name and tmux's own
#: parser**: a submenu opener's command is a template with the group interpolated into it
#: (`menu_argv`), so a group is in exactly the position `_ACTION_ID_RE` guards an action id
#: in. These are charter's own constants — no committed value ever becomes one — and the
#: membership test below is the guard at the join, for `_ACTION_ID_RE`'s own stated reason:
#: a hand-edited or otherwise corrupted `actions.json` is not bound by what `record` would
#: have written.
GROUPS = (MAIN, WORKSPACES, PERSONAS)

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


class Entry(NamedTuple):
    """One row of one menu, in the shape `record` stores and `menu_argv` renders.

    Constructible from a plain tuple of two, three or four values, so every caller that
    only ever wanted a label and an argv (`("Detach", ["true"])`) keeps working unchanged
    — `record` normalises through ``Entry(*e)``.

    ``group`` says which menu the row belongs to; ``opens`` makes the row a **submenu
    opener** — a row that draws another group instead of running an argv. The two are
    mutually exclusive in practice: an opener has no argv, because opening a menu is a
    tmux command and not something `cmd_action` could run through `subprocess.run`.
    """

    label: str
    argv: tuple = ()
    group: str = MAIN
    opens: str | None = None


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


def record(*, fid: str, entries) -> None:
    """Store this frame's menus as ``{id: {label, argv, group, opens}}``. The only place
    ids are minted.

    **One table, one id space, several menus.** *entries* holds every row of every group
    (:data:`GROUPS`) in one list, and an id is minted from position across the whole of
    it — so `resolve` and `cmd_action` never have to know which menu a selection came
    from, and a submenu costs no second file, no second id space and no second thing that
    can go stale against the first. `build` filters by group at read time.

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
    placeholder rather than an empty string: `display-menu` treats a LITERALLY empty NAME
    as a separator line, which desynchronises every `label key command` triple after it
    and fails the whole menu outright — rc 1, tmux's own text `not enough arguments`,
    re-confirmed against tmux 3.7c with a real attached client. The hotkey would then
    silently do nothing, for a reason with no message anywhere to explain it.

    Distinct from a name that merely EXPANDS to empty (`#(touch x)`, `#{no_such_thing}`),
    which does NOT refuse: tmux counts a command's arguments before expanding formats, so
    such a name still occupies its slot and the menu opens normally. See the module
    docstring — that difference used to be recorded the wrong way round.
    """
    path = _table(fid, create=True)
    if path is None:
        return
    data = {}
    for i, raw in enumerate(entries):
        e = raw if isinstance(raw, Entry) else Entry(*raw)
        clean = e.label.replace("\n", " ")[:_MAX_LABEL] or "(untitled)"
        row = {"label": clean, "argv": list(e.argv)}
        # Only written when they say something — a table with no submenus in it stays
        # byte-for-byte what it was before groups existed, so nothing has to migrate and
        # a frame running across the upgrade keeps the menu it already has.
        if e.group != MAIN:
            row["group"] = e.group
        if e.opens:
            row["opens"] = e.opens
        data[f"a{i}"] = row
    tmp = path.with_name(path.name + ".tmp")
    try:
        tmp.write_text(json.dumps(data))
        os.replace(tmp, path)
    except OSError:
        # Same "must not raise" promise `state.py` makes for its own writes: this can run
        # from `cmd_launch`, where a menu the operator never opens is not worth losing the
        # frame over.
        return


def build(fid: str, group: str = MAIN) -> list[tuple[str, str]]:
    """``(label, opaque id)`` for each entry of *group*, in the order `record` stored them.

    Thin over :func:`rows` — the same read, dropping the two fields a caller that only
    wants to name the rows does not need. See :func:`rows` for what is checked and why.
    """
    return [(e.label, action_id) for e, action_id in rows(fid, group)]


def rows(fid: str, group: str = MAIN) -> list[tuple[Entry, str]]:
    """``(entry, opaque id)`` for every row of *group*, in the order `record` stored them.

    `json.loads` rebuilds the dict in the order its keys appeared in the text, which is
    the same order `record`'s own dict comprehension wrote them in — no `sorted()` here on
    purpose: sorting the id STRINGS lexicographically would put ``"a10"`` before ``"a2"``,
    silently reordering the menu the moment a frame ever grows past nine entries.

    Any key not shaped `a<N>` is dropped rather than passed through — see
    `_ACTION_ID_RE`'s own docstring for why this is checked here, at the join, and not
    only trusted from `record`.

    A label that is missing, not a string, or empty gets `record`'s own `"(untitled)"`
    placeholder rather than being passed through as-is: `record` only writes a `str`
    today (`or "(untitled)"` already guards the empty case there), but `build` reads
    whatever is actually on disk, and a hand-edited or otherwise corrupted table is not
    bound by what `record` would have written. `{"label": 123}` reaching `menu_argv`
    unguarded raised `AttributeError` there (`int` has no `.replace`) — the SAME class
    of defect `_ACTION_ID_RE` exists to close for the key, applied consistently to the
    value next to it, so a corrupted label degrades the one row it belongs to rather
    than crashing the hotkey for the whole menu.

    **`opens` is checked against :data:`GROUPS` here, at the join**, for exactly
    `_ACTION_ID_RE`'s reason and not because `record` might write something else: it is
    interpolated into tmux command text by `menu_argv`, so a corrupted table holding
    ``"opens": "x'; run-shell \\"...\\""`` would otherwise reach that f-string untouched.
    An unknown `opens` costs the row only its submenu, degrading it to an ordinary row,
    because that field is decoration on a row that is otherwise complete.

    **`group` is NOT checked against `GROUPS`, deliberately.** It is a filter key and
    nothing else — it reaches no parser, no argv and no screen — so a row claiming a
    group this charter does not have is already unreachable by the `g != group`
    comparison below, and adding a membership test next to it would be a guard no
    mutation could turn red: this repo's own "a guard passing because a DIFFERENT guard
    caught it". Confirmed by mutating it out and watching the covering test stay green.

    A MISSING `group` is `main`: that is every table written before groups existed, and a
    frame running across the upgrade keeps its menu.
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
    out = []
    for k, v in data.items():
        if not isinstance(v, dict) or not _ACTION_ID_RE.fullmatch(k):
            continue
        g = v.get("group", MAIN)
        if g != group:
            continue
        label = v.get("label")
        opens = v.get("opens")
        argv = v.get("argv")
        out.append((Entry(
            label=label if isinstance(label, str) and label else "(untitled)",
            argv=tuple(argv) if isinstance(argv, list) else (),
            group=g,
            opens=opens if opens in GROUPS else None), k))
    return out


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

    2. **A leading `-` refuses the WHOLE menu, not just that row.** `display-menu`'s
       own docs describe a name starting with `-` as merely "shown dim and may not be
       chosen" — verified by hand that this is not what actually happens: tmux's own
       argument parser reads a name beginning with `-` as an unrecognised FLAG of its
       own (`command display-menu: unknown flag -m` for a label starting `-my-branch`,
       confirmed against a real, attached client), and refuses the entire `display-menu`
       call outright (rc 1) — no item in the menu opens, not only the one whose name
       triggered it. Worse than the docs suggest, and this guard is correspondingly more
       necessary than its own comment once claimed. A leading space keeps the text
       intact and stops it from ever reaching tmux's own flag position.

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


def menu_argv(fid: str, socket: str, client: str, group: str = MAIN) -> list[str]:
    """The `display-menu` invocation for one of this frame's menus. Ids only — never a name.

    **A submenu is a row whose command opens another `display-menu`, and the client it
    opens on is carried by tmux itself.** An opener's command is a second fixed template,
    ``run-shell '"$CHARTER_PY" -m charter frame-menu #{client_name} --group <g>'`` — the
    same shape the `F2` bind already uses (`commands_frame.conf_text`), with `<g>` taken
    from the closed set :data:`GROUPS` and nothing else varying. Measured against tmux
    3.7c with **two real ptys attached to one session**: `#{client_name}` inside a menu
    item's own command expands to the client the menu was drawn on (`-c`), never to
    whichever client tmux would otherwise call current — pressing on client A ran the
    item with A's name, pressing on B ran it with B's, in the same session. That is the
    property `cmd_menu`'s docstring says was NOT available from `list-clients`, and it is
    what lets a submenu reach the presser's screen without charter storing a client
    anywhere or guessing at one.

    The opener cannot go through `frame-action` like every other row: `cmd_action` runs a
    stored argv through `subprocess.run`, which is a list and is never seen by tmux, so a
    `#{client_name}` in it would be four literal characters. Opening a menu is a tmux
    command; only text tmux parses can carry the format that names the screen.

    `-c client` is what selects WHICH ATTACHED TERMINAL the menu is drawn on —
    `display-menu`'s own docs: "Display a menu on target-client. target-pane gives the
    target for any commands run from the menu." `-t` (kept below) never did that; it only
    scopes FORMAT EVALUATION for the item's own command text. Verified by hand against
    tmux 3.7c with two frames attached in two separate terminals: `-t fid` alone rendered
    frame B's menu on frame A's screen when B's own hotkey was pressed — B's operator saw
    nothing, and selecting the item there would have run B's action from A's terminal.

    *client* arrives as `#{client_name}`, expanded by tmux inside the hotkey BIND's own
    `run-shell` text and handed to `charter frame-menu` as a plain argv value, which
    `commands_frame.cmd_menu` passes straight here (see `conf_text`'s own docstring).
    Nothing queries it: an earlier version of this module said the client was resolved
    "via `list-clients -t fid`", and that command was removed from charter entirely when
    the guess it fed turned out to pick the wrong client with two attached. `menu.py`
    still makes no subprocess calls of its own — every function here is pure file/state
    access — but that is now true because there is no query left anywhere, not because
    one was moved next door.

    `-t fid` stays for a DIFFERENT reason: it is what scopes the ITEM's own `run-shell`
    command to this session's `$CHARTER_SESSION_ID` and `$CHARTER_PY` (verified by hand:
    an item fired from `display-menu -t <session>` inherits that session's own
    `set-environment` values even though its own `run-shell` text carries no `-t` — see
    `_session_id_env_argv`'s own docstring in `commands_frame.py` for why those values
    have to be there at all).

    Every label passes through `_safe_label` — see its own docstring and the module
    docstring for why a label is not inert text. Every item's own ACTION is the fixed
    template ``run-shell '"$CHARTER_PY" -m charter frame-action a<N>'`` — the opaque id
    is the only thing that varies, and `build` already refuses anything not shaped `a<N>`
    before it ever reaches here.

    `"$CHARTER_PY" -m charter`, never a bare `charter`: the `charter` on the tmux
    server's own `$PATH` may be a different install or none at all, and `run-shell`
    reports a 127 by printing it INTO THE HARNESS PANE and dropping that pane into
    copy-mode — charter drawing in the one rectangle ADR 0018 says it never draws. The
    interpreter is carried session-scoped by `commands_frame._charter_py_env_argv`, which
    keeps this template free of any per-machine path; embedding `sys.executable` here
    instead would put an absolute path back inside a nested tmux-quote layer, the exact
    construction the `commands_frame` module docstring bans for `status_path`.
    """
    title = "charter" if group == MAIN else f"charter · {group}"
    cmd = ["tmux", "-L", socket, "display-menu", "-t", fid, "-c", client, "-T", title]
    for i, (entry, action_id) in enumerate(rows(fid, group)):
        if entry.opens:
            # `entry.opens` is already confined to `GROUPS` by `rows`, at the read — the
            # same discipline `_ACTION_ID_RE` gets, and for the same reason: this is an
            # f-string into text tmux re-parses.
            action = (f'run-shell \'"${tmuxctl.CHARTER_PY_ENV}" -m charter '
                      f'frame-menu #{{client_name}} --group {entry.opens}\'')
        else:
            action = (f'run-shell \'"${tmuxctl.CHARTER_PY_ENV}" -m charter '
                      f'frame-action {action_id}\'')
        cmd += [_safe_label(entry.label), _key(i), action]
    return cmd


def _key(i: int) -> str:
    """The one-key shortcut for row *i*, or tmux's own "no shortcut".

    `display-menu`'s middle argument is a KEY NAME, not a label: `"10"` is not one. Before
    submenus the menu had four fixed rows and could never reach it; a workspace list can,
    so rows past the ninth get the **empty string**, which is tmux's own spelling for a row
    with no key bound. Ten rows is also where the digits run out, not an arbitrary cap:
    continuing into letters would shadow `display-menu`'s own `q`, and a menu whose tenth
    row silently quits it is worse than one with no shortcut there.

    **Not ``-``, which is a real key.** Measured against tmux 3.7c on an attached pty, a
    four-row menu keyed ``1``, ``-``, ``-``, ``""``::

        ┌─probe─────┐
        │ row-a (1) │
        │ row-b (-) │
        │ row-c (-) │
        │ row-d     │
        └───────────┘

    Pressing ``-`` RAN row-b's command and closed the menu — so a stray hyphen on a
    workspace submenu would have performed a real switch to the tenth workspace, and
    row-c, which advertises the same ``(-)``, was unreachable by that key. The empty-key
    row is drawn with no ``(…)`` at all and is still arrow-selectable: from the same menu,
    three Downs and Enter fired row-d.
    """
    return str(i + 1) if i < 9 else ""
