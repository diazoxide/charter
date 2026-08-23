"""The frame's shape, decided before tmux is involved at all.

Pure on purpose. Everything that decides *what the frame looks like* lives here and
returns plain lists of strings, so the whole shape is under test on a machine with no
tmux, and so the argv rule below is enforced mechanically instead of by review.

The one thing this module reads from outside its own arguments is the running
interpreter's path, via `util.self_relaunch_argv()` in `panel_command` — no process is
started, nothing is measured, and the result is as deterministic as any other string
here. It is read HERE rather than passed in because the two callers that start a panel
otherwise each hand in their own copy of that argv, and one of them promptly got it
wrong (#390's `-P`, missing from the respawn path — see `panel_command`'s own
docstring). Same reasoning as the never-join-argv rule below: the property is worth more
enforced by construction than remembered at every call site.

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

from . import tmuxctl
from .. import util

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


#: What a frame's window runs while charter is still setting it up, before the harness
#: replaces it. `cat` with no arguments blocks on its own stdin forever and never exits
#: on its own, which is the ONLY property required of it: `window_argv`'s docstring
#: explains why the pane has to exist, and keep existing, before `remain-on-exit` can be
#: set on it. A separate argv element, never a shell string, like everything else here.
PLACEHOLDER = ["cat"]


def _tmux(socket: str, *args: str) -> list[str]:
    """*socket* is charter's own server NAME or the operator's socket PATH — see
    `tmuxctl.server_argv`, the one place that difference becomes `-L` or `-S`."""
    return tmuxctl.server_argv(socket, *args)


def window_argv(*, socket: str, session: str, window: str, cwd: str) -> list[str]:
    """The `new-window` that puts a frame in the operator's OWN tmux, not under it.

    Detached (`-d`) and appended after the current window (`-a`): the operator is
    switched to the frame once it has actually been built (`select-window`, from the
    launcher), never onto a half-drawn window still running the placeholder.

    *session* is the operator's own session, `$N` shaped, read out of `$TMUX` by
    `tmuxctl.operator_server` — measured against tmux 3.7c, `new-window -t` refuses a
    PANE id outright ("can't specify pane here"), so the session is the target that
    works and the one `$TMUX` already carries.

    *window* is the frame id, which is also what the launcher reaps against: charter's
    frames on the operator's server are windows, not sessions, so `list-windows -a -F
    '#{window_name}'` is the liveness list there that `list-sessions` is on charter's
    own server.

    Asks for BOTH ids (`-P -F '#{window_id} #{pane_id}'`). The pane id scopes every
    split and every status query, exactly as on the private-server path; the window id
    is what `kill-window` targets when the harness is done — an index would be wrong for
    both, since tmux renumbers windows on every close just as it renumbers panes on
    every split (see this module's own docstring).

    The window is created running :data:`PLACEHOLDER` rather than the harness, and that
    ordering is the whole point. `remain-on-exit` is what keeps a dead harness pane
    askable long enough to read its exit status out of, and tmux has no way to set it on
    a pane that does not exist yet: the options a new pane would inherit it from are
    global or session-scoped, and writing either on somebody else's server is precisely
    what this path exists not to do. So the pane is created with a program that cannot
    exit, `remain-on-exit` is set ON that pane, and `respawn_argv` then puts the harness
    in it. The install race the private-server path narrows with `_PLACEHOLDER_CONF` is
    not narrowed here; it is removed.

    *cwd* is `-c`, for the same reason `respawn_argv` takes one — see its docstring.
    """
    return _tmux(socket, "new-window", "-d", "-a", "-t", session, "-n", window,
                 "-c", cwd, "-P", "-F", "#{window_id} #{pane_id}", "--", *PLACEHOLDER)


def respawn_argv(*, socket: str, harness_pane: str, env: dict[str, str],
                 cwd: str, harness_argv: list[str]) -> list[str]:
    """`respawn-pane -k`: the harness replaces the placeholder, in the SAME pane.

    Verified against tmux 3.7c that the pane keeps its `%N` id across the respawn, which
    is what lets `window_argv`'s reported id go on scoping the splits and the
    exit-status query afterwards.

    *env* rides on `-e`, one `NAME=VALUE` argv element each, sorted so the command is
    the same on every launch. This is the only way charter's own variables
    (`CHARTER_SESSION_ID`, `CHARTER_HARNESS`) can reach the harness here. The
    alternative tmux offers — `set-environment -t <session>` — would reach the harness
    AND hand every new shell the operator opens in that session a frame id that is not
    theirs, which is the identity collision
    `docs/superpowers/specs/2026-08-21-harness-wrapper-design.md` removed `WINDOWID` for.

    **This used to add "the private-server path gets them because `new-session` starts
    the server and the server inherits the launcher's environment", and that was #411.**
    It is true only of the launch that actually starts the server; every later frame on
    charter's shared private server finds it running and inherits the FIRST launcher's
    environment instead. `session_argv` now carries the same `-e` for the same reason —
    see its docstring for what was measured.

    Measured against tmux 3.7c: `respawn-pane -e` REPLACES the pane's environment rather
    than adding to whatever `new-window -e` set, so everything the harness needs is on
    THIS call. Every `-e` lands before the `--` — they are `respawn-pane`'s own options,
    and must never be grafted onto the harness's own argv.

    *cwd* is `-c`, and it is not optional. A pane in a server charter did not start
    inherits its start directory from the SESSION's, which is wherever the operator
    happened to be when they first ran `tmux` — days ago, in another repo. `charter
    claude` has to run the harness where it was typed, and the panels split off this
    pane inherit its directory in turn, which is what `workspace.resolve()` reads.
    """
    return _tmux(socket, "respawn-pane", "-k", "-t", harness_pane, "-c", cwd,
                 *_env_argv(env), "--", *harness_argv)


def session_argv(*, session: str, conf: str, socket: str, cols: int, rows: int,
                 harness_argv: list[str],
                 env: dict[str, str] | None = None) -> list[str]:
    """The `new-session` command that starts the frame's tmux server, harness inside it.

    Detached (`-d`): this is launched from a script with no tty to hand tmux, not typed
    interactively, and without `-d` tmux would attach and the call would never return.

    Asks tmux to PRINT the new pane's id (`-P -F '#{pane_id}'`) rather than assuming one.
    That id is the whole fix the module docstring describes: the caller must capture it
    off stdout and pass it to `panel_argvs`, because a `session:0.0`-style index stops
    naming the harness after the very first split.

    *env* rides on `-e`, exactly as it does for :func:`respawn_argv` and
    :func:`panel_argvs`, and **#411 is what it is here for.** It used to be absent
    because passing the launcher's environment to the tmux CLIENT was believed to be
    enough — "`new-session` starts the server and the server inherits the launcher's
    environment", as `respawn_argv`'s own docstring put it. That is true of the launch
    that STARTS the server and of no other: every frame on charter's private server
    shares one server (see `commands_frame`'s module docstring), so a second frame's
    `new-session` finds it already running, and tmux builds the new pane's environment
    from the SERVER's global environment — captured from whichever launcher happened to
    start it, possibly days ago.

    Measured against tmux 3.7c, two frames on one private socket, reading the harness
    shell's own `$CHARTER_SESSION_ID`: the second frame's harness reported
    ``default-58069`` while its own session was ``default-58696``, and
    ``show-environment -g CHARTER_SESSION_ID`` held the first frame's id. Everything
    keyed on that variable then went to the wrong frame — `charter ws use` wrote the
    first frame's workspace pointer, and every hook bumped the first frame's version, so
    the second frame's panels were never told anything had changed. The panels
    themselves were already right: `commands_frame._session_id_env_argv` ties the id to
    the SESSION, which `split-window` honours (measured on the same server), so this
    closes the one pane that call cannot reach — the one `new-session` itself creates.

    ``None`` means "carry nothing", which is what a tmux below
    `tmuxctl.SESSION_ENV_FLOOR` gets: `-e` is not a flag `new-session` degrades on, it is
    one it refuses, and refusing takes the whole launch with it.
    """
    return _tmux(socket, "-f", conf, "new-session", "-d", "-s", session,
                "-x", str(cols), "-y", str(rows), "-P", "-F", "#{pane_id}",
                *_env_argv(env),
                "--", *harness_argv)


def _env_argv(env: dict[str, str] | None) -> list[str]:
    """`-e NAME=VALUE` per entry, sorted so the command is the same on every launch.

    One argv element each, never a joined string, and always before a command's `--`:
    these are tmux's own options, not something to graft onto the program's argv. Empty
    for an empty (or absent) *env*, so a call that has nothing to carry produces exactly
    the command it always did.
    """
    return [x for name in sorted(env or {}) for x in ("-e", f"{name}={env[name]}")]


def panel_command(*, slot: str, session: str) -> list[str]:
    """The command one panel pane runs — the part after `split-window`'s own `--`.

    Split out of :func:`panel_argvs` because a panel is started TWICE by two different
    modules: once by the launcher's `split-window`, and again by
    `commands_frame.cmd_respawn`'s `respawn-pane` after the pane's `pane-died` hook
    fires (#382). Two hand-written copies of this argv is exactly the drift that ends
    with a respawned panel running a slightly different command from the one the
    launcher spawned — a stale flag, a missing `--session` — and failing in a way that
    only ever reproduces after something has already died once.

    **The interpreter half is built here too, not passed in.** This function used to
    take a *charter_argv* and both callers handed it one, which left the ONE part of
    the argv that actually differs between them outside the shared helper — and it
    promptly drifted: the launcher moved to `util.self_relaunch_argv()` for #390's `-P`
    while `cmd_respawn`, written against the same seam a release earlier, kept a
    hand-built `[sys.executable, "-m", "charter"]`. That is #390's own failure with a
    delay fuse on it: `respawn-pane` starts the new process in the PANE's cwd, which for
    anyone dogfooding charter is a charter checkout, so the respawned panel would import
    that tree rather than the installed one and die again — this time for a reason
    nothing in the frame could show. Owning the whole argv here is what makes the
    extraction actually deliver the no-drift property it was created for: there is no
    longer a parameter for a caller to get wrong.
    """
    return [*util.self_relaunch_argv(), "panel", slot, "--session", session]


def panel_argvs(*, slots: list[str], session: str, socket: str,
                harness_pane: str,
                env: dict[str, str] | None = None) -> list[list[str]]:
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

    *env* is `-e`, and is only ever passed inside a tmux charter did not start. A pane
    created on somebody else's server inherits THAT SERVER's environment — whatever
    their shell had when they first ran `tmux`, possibly days ago and in another plane —
    and a panel resolves the plane it draws (`config.STATE_DIR` among it) from its own.
    On charter's own server the panels inherit the launcher's environment because
    `new-session` is what starts that server, so nothing is passed and the command is
    unchanged. See `respawn_argv` for the same mechanism carrying the same values to the
    harness.
    """
    cmds: list[list[str]] = []
    for slot in slots:
        size = SLOT_SIZE[slot]
        direction = "-v" if slot in ("top", "bottom") else "-h"
        before = ["-b"] if slot in ("top", "left") else []
        cmds.append(_tmux(socket, "split-window", "-t", harness_pane,
                          direction, *before, "-l", str(size),
                          *_env_argv(env),
                          "-P", "-F", "#{pane_id}", "--",
                          *panel_command(slot=slot, session=session)))
    return cmds
