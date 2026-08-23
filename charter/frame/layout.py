"""The frame's shape, decided before tmux is involved at all.

Pure on purpose. Everything that decides *what the frame looks like* lives here and
returns plain lists of strings, so the whole shape is under test on a machine with no
tmux, and so the argv rule below is enforced mechanically instead of by review.

**Nothing here ever joins argv.** Pinned against tmux 3.7c: `new-session … printf
'hello;touch INJ'` passed as separate arguments creates no file, and the same text as one
string creates it. Workspace, repo, branch and persona names all reach a frame from
committed files or `.git/HEAD`, so a joined string would be the `gh -F` bug again.

**Every split targets the harness pane's id — never a `session:0.0`-style index.**
Measured against tmux 3.7c, and the reason this module has two entry points instead of
one `plan()` that emits every command up front: tmux renumbers pane INDICES on every
split. Starting a session whose only pane is the harness (`%0`, index 0) and running
`split-window -t session:0.0 -v -b -l 1` leaves the layout as index 0 = the new one-row
panel (`%1`) and index 1 = the harness (`%0`) — the split moved, the harness didn't, and
now sits at the index the split just vacated. A second split still targeting
`session:0.0` therefore divides the FIRST PANEL, not the harness, and once that panel is
down to a single row tmux refuses outright: `size or position no space for a new pane`.
The bottom panel is never built. Silently — nothing downstream currently checks a split's
return code — so a four-slot frame ships with one panel and no error.

Pane IDs don't have this problem: tmux never reuses or renumbers a pane's `%N` id for its
lifetime, index churn or not. `session_argv` asks tmux to print the harness's id at
creation time (`-P -F '#{pane_id}'`); the caller reads it off stdout and hands it to
`panel_argvs`, which targets it — and only it — for every split.
"""

from __future__ import annotations

#: The order slots are dropped in as the terminal shrinks. Sides first — a side panel
#: costs the harness columns, so it goes as soon as space is tight in EITHER dimension,
#: not only when columns themselves are the short one — then the top, whose row is worth
#: less than the bottom's alerts, and which only goes when rows are the tight dimension.
_DROP_ORDER = ("left", "right", "top", "bottom")

#: Rows a horizontal panel occupies, and columns a vertical one does.
SLOT_SIZE = {"top": 1, "bottom": 1, "left": 22, "right": 22}


def visible_slots(slots: list[str], cols: int, rows: int,
                  min_cols: int, min_rows: int) -> list[str]:
    """Which of *slots* fit in a *cols* x *rows* terminal.

    Degradation, not refusal: below the floor the harness simply gets the whole terminal,
    which is the same choice `statusline.render` makes when it runs out of width. Follows
    `_DROP_ORDER`: `left`/`right` are the first to go, on ANY shortage — a terminal that is
    short on rows cannot spare a side panel's own divider any more than a narrow one can
    spare its columns — then `top`, only when rows specifically are the tight dimension.
    """
    keep = list(slots)
    if cols < min_cols or rows < min_rows:
        keep = [s for s in keep if s not in ("left", "right")]
    if rows < min_rows:
        keep = [s for s in keep if s != "top"]
    if cols < min_cols // 2 or rows < min_rows // 2:
        keep = []
    return [s for s in slots if s in keep]


def _tmux(socket: str, *args: str) -> list[str]:
    return ["tmux", "-L", socket, *args]


def session_argv(*, session: str, conf: str, socket: str, cols: int, rows: int,
                 harness_argv: list[str]) -> list[str]:
    """The `new-session` command that starts the frame's tmux server, harness inside it.

    Detached (`-d`): this is launched from a script with no tty to hand tmux, not typed
    interactively, and without `-d` tmux would attach and the call would never return.

    Asks tmux to PRINT the new pane's id (`-P -F '#{pane_id}'`) rather than assuming one.
    That id is the whole fix the module docstring describes: the caller must capture it
    off stdout and pass it to `panel_argvs`, because a `session:0.0`-style index stops
    naming the harness after the very first split.
    """
    return _tmux(socket, "-f", conf, "new-session", "-d", "-s", session,
                "-x", str(cols), "-y", str(rows), "-P", "-F", "#{pane_id}",
                "--", *harness_argv)


def panel_argvs(*, slots: list[str], session: str, socket: str,
                charter_argv: list[str], harness_pane: str) -> list[list[str]]:
    """One `split-window` per slot in *slots*, each carving its rectangle off *harness_pane*.

    *harness_pane* is the id `session_argv`'s caller read off tmux's stdout, and every
    split below targets that same id — never a `session:0.0`-style index, and never a
    target derived from an earlier split in this same list. Both are the mistake the
    module docstring measures: indices move under every split tmux runs, pane ids don't.

    Also asks tmux to PRINT the new pane's id (`-P -F '#{pane_id}'`, the same flags
    `session_argv` uses for the harness pane, placed the same way — before `--`, so they
    are `split-window`'s own options and never touch the `charter panel …` argv after
    it). A caller that keeps the id for each fixed-size slot can re-assert its size on a
    `window-resized` hook (see `commands_frame._resize_hook_argv`): tmux's own layout
    engine redistributes EVERY pane proportionally on a resize, `-l size` notwithstanding
    — measured against tmux 3.7c, growing a 120x30 frame to 200x50 stretched two
    one-row panels to 8 and 7 rows, snapping back only because the resize happened to be
    an exact round trip. Without the id, nothing later could target the RIGHT pane to
    correct that (an index would renumber under the very next split, same failure the
    module docstring already measures for the harness pane).
    """
    cmds: list[list[str]] = []
    for slot in slots:
        size = SLOT_SIZE[slot]
        direction = "-v" if slot in ("top", "bottom") else "-h"
        before = ["-b"] if slot in ("top", "left") else []
        cmds.append(_tmux(socket, "split-window", "-t", harness_pane,
                          direction, *before, "-l", str(size),
                          "-P", "-F", "#{pane_id}", "--",
                          *charter_argv, "panel", slot, "--session", session))
    return cmds
