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

#: The order slots are dropped in as the terminal shrinks. The side first — a side panel
#: costs the harness columns, so it goes as soon as space is tight in EITHER dimension,
#: not only when columns themselves are the short one — then `repos`, whose table simply
#: cannot be drawn in a pane narrower than `statusline._LEFT_W`, then the top, whose row
#: is worth less than the status strip's alerts and which only goes when rows are the
#: tight dimension.
#:
#: **`bottom` is not here, and it is the one slot that never is.** It is the attention
#: strip — the one alert and the command that fixes it — which is the whole reason a
#: frame is worth drawing at all on a terminal too small for anything else.
#:
#: **`left` is not here any more, and #488 is why.** It drew repo rows recomposed for a
#: 22-column pane; `repos` now draws the same rows as the full-width table the status
#: line draws, so the sidebar's only remaining job was a lesser copy of its neighbour's.
#: Retiring it hands those 22 columns back to the harness at every density.
_DROP_ORDER = ("right", "repos", "top")

#: Rows a horizontal panel occupies, and columns a vertical one does.
#:
#: **`repos`' entry is a FLOOR, not its size** (#488, moved off `bottom` by #515). Every
#: other slot is fixed: `top` says one row's worth of identity, `bottom` is the one-row
#: attention strip, `right` is a column of persona chips. `repos` carries the table,
#: which is as many rows as there are repos — so its real height is :func:`repos_rows`,
#: and this is what it never goes below (and what it is, on a plane with no clones at
#: all, where the pane says so in one line). Callers ask :func:`slot_sizes` rather than
#: indexing this directly, so nothing has to remember which of the two questions it is
#: asking.
SLOT_SIZE = {"top": 1, "bottom": 1, "repos": 1, "right": 22}

#: Rows the harness keeps whatever the repo table would like. The frame exists to show
#: the plane's state around an agent session, and a session squeezed into three rows is
#: not one anybody can read — so the table gives up rows before the harness does.
#:
#: Not hypothetical, and this number is what stands between the operator and it: measured
#: against tmux 3.7c, `resize-pane -t <the table pane> -y 40` in a 20-row window left the harness
#: pane **1 row tall**. tmux clamps to what the window has and takes the remainder from
#: its neighbour without complaint, so an over-large height is not refused — it is
#: granted, out of the one pane that matters.
HARNESS_MIN_ROWS = 12

#: One row per pane border. tmux charges a horizontal split one row for the divider on
#: top of the pane's own height — measured on 3.7c: a 50-row window split `-l 8` reads
#: back as a 41-row harness and an 8-row panel, which is 49. Counted explicitly rather
#: than folded into :data:`HARNESS_MIN_ROWS` so the arithmetic in :func:`repos_rows`
#: says what it is doing.
_BORDER_ROWS = 1

#: One column per pane border, the vertical counterpart of :data:`_BORDER_ROWS` and
#: measured the same way — on tmux 3.7c, a 110-column window split `-h -l 22` reads back
#: as an 87-column harness and a 22-column sidebar, which is 109.
_BORDER_COLS = 1

#: Slots that take COLUMNS off the pane they are split from rather than rows — the `-h`
#: half of :func:`panel_argvs`' `direction`. Kept as a name rather than an inline
#: ``== "right"`` because :func:`repos_cols` and `panel_argvs` are two places that have
#: to agree about which splits cost columns, and the next side slot must not have to be
#: remembered in both.
_COLUMN_SLOTS = ("right",)

#: The horizontal strips whose height is a CONSTANT, and therefore the rows
#: :func:`repos_rows` has to subtract before it may spend what is left. `right` is not
#: here because it costs columns, and `repos` is not here because it is the slot being
#: sized.
#:
#: **`bottom` joined this list in #515 and that is the whole arithmetic change.** It used
#: to be the variable-height slot itself, so the only fixed strip to subtract was `top`;
#: it is now the one-row attention strip it was before #488, and the table it used to
#: carry is `repos`. A `repos_rows` still subtracting `top` alone would hand the table
#: two rows the status strip and its own border are already using, and tmux grants an
#: over-large height out of the neighbour rather than refusing it — the harness.
_FIXED_ROW_SLOTS = ("top", "bottom")

#: The slots whose height is a function of their content rather than a constant — the
#: ones :func:`slot_sizes` answers with :func:`repos_rows` instead of :data:`SLOT_SIZE`.
#:
#: **It is also which pane is the DEPENDENT one when sizes are re-applied**, and that is
#: not a second meaning bolted on. tmux's `resize-pane -y` moves exactly one boundary, so
#: in a vertical stack of N panes only N-1 heights can be asserted; assert them all and
#: the result depends on the order, which is how a re-assertion in split order came out
#: with the table one row tall and the attention strip six (measured on 3.7c at 200x50:
#: `top,bottom,repos` left `%3 h=1` and `%2 h=6` — the two sizes swapped panes). The pane
#: that must be left to take the remainder is the one whose size is already a function of
#: everything else, which is this one. See `commands_frame._reassert_sizes`.
VARIABLE_ROW_SLOTS = frozenset({"repos"})


def _table_min_cols() -> int:
    """The narrowest pane the repo table can be drawn in at all — `statusline._LEFT_W`,
    READ rather than copied.

    `slots._table_cap` answers 0 below this width and `slots._table_lines` refuses
    outright ("too narrow for the table is NO table, not a cut one"), so a `repos` pane
    split narrower than this is a bordered rectangle with nothing in it — which reads as
    "this workspace has no repos" on a plane that has fourteen. :func:`visible_slots`
    drops the slot instead, and it must drop it at exactly the width the RENDERER stops
    drawing at: a second copy of the number here would be a guard matching a spelling,
    and the two would come apart the first time a column width moved.

    Imported inside the call rather than at module scope, the way `frame/slots.py`
    already reaches for the same module: this file is imported by every launch path and
    `statusline` pulls in `config`, whose import resolves the plane root.
    """
    from .. import statusline as sl
    return sl._LEFT_W


def repos_cols(slots: list[str] | tuple[str, ...], *, window_cols: int) -> int:
    """How wide the `repos` pane actually is, in a *window_cols*-column window whose
    slots were split in *slots*' order — **not the window's own width** (#500).

    `panel_argvs` splits every slot off the HARNESS pane in list order, so a side slot
    split before `repos` has already narrowed the pane `repos` is then carved out of.
    Measured on tmux 3.7c, window 120x40, `-l 22` for `right` and `-l 6` for the table:

    * ``["top", "bottom", "repos", "right"]`` (the shipped order) — the table reads back
      **120** wide; `right` is split off the harness afterwards and sits beside the
      harness only (`%3 top=32 h=6 w=120`, `%4 top=2 left=98 h=29 w=22`).
    * ``["right", "top", "bottom", "repos"]`` — the table reads back **97**, which is
      ``120 - 22 - 1``.

    That difference is a promise, not an accident: `instance.frame_of` keeps an
    operator's `[frame] slots` order verbatim and
    `tests/test_frame_config.py::test_the_operators_own_slot_order_is_kept_exactly`
    pins it, precisely because the order IS the geometry. What #500 got wrong was not the
    order — it was handing `slots.repos_rows_wanted` the window's width regardless, so a
    table pane that had been inset beside the sidebar was sized for a table its own pane
    was too narrow to draw. Below :func:`_table_min_cols` the renderer draws no table at
    all, so a 110-column frame with `right` first got a seven-row pane and one line in
    it, six rows taken off the harness and left blank.

    Order in, order out — this reads *slots* rather than a set, and stops at `repos`.
    A slot split AFTER `repos` costs it nothing (measured above), and killing one gives
    the columns back (`kill-pane` on `right` widened the same pane from 87 to 110),
    which is why the list a caller passes has to be the order its panes were actually
    split in: at launch that is the drawable slot list, and for a running frame it is the
    recorded pane map's own order, survivors first and later splits appended.

    With no `repos` in *slots* this answers the width one WOULD be split to, which is
    what :func:`visible_slots` asks it before deciding whether the slot survives at all.
    """
    out = window_cols
    for slot in slots:
        if slot == "repos":
            break
        if slot in _COLUMN_SLOTS:
            out -= SLOT_SIZE[slot] + _BORDER_COLS
    return max(0, out)


def repos_fits(order: list[str] | tuple[str, ...], *, window_cols: int) -> bool:
    """Would a `repos` pane split in *order*, in a *window_cols*-column window, be wide
    enough to draw a table in at all?

    **One rule, two callers, and the second one is why it is a function.**
    :func:`visible_slots` asks it at launch, where *order* is the configured slot list.
    `commands_frame._relayout` asks it for a density change, where *order* is the
    surviving panes in their recorded split order followed by what is about to be split —
    which is NOT the level's own list, and which is the whole of #500's round 3. A frame
    that already has `right` and grows a table gets that table split off a harness pane
    the sidebar has already narrowed by 23 columns, so the level's list says 110 and the
    pane is 87.

    Without this shared, both said different things and the disagreement was visible: the
    launch filter kept `repos` (from the level's order, full width) and the sizer floored
    it at one row (from the pane's order, too narrow) — a bordered rectangle with nothing
    in it, which is exactly the "no repos" lie #515 split the pane to avoid.
    """
    return repos_cols(order, window_cols=window_cols) >= _table_min_cols()


def visible_slots(slots: list[str], cols: int, rows: int,
                  min_cols: int, min_rows: int) -> list[str]:
    """Which of *slots* fit in a *cols* x *rows* terminal.

    Degradation, not refusal: below the floor the harness simply gets the whole terminal,
    which is the same choice `statusline.render` makes when it runs out of width. Follows
    `_DROP_ORDER`: `right` is the first to go, on ANY shortage — a terminal that is short
    on rows cannot spare a side panel's own divider any more than a narrow one can spare
    its columns — then `repos`, and then `top`, only when rows specifically are the tight
    dimension.

    **`repos` goes on a width its own renderer cannot draw in, and that width is read
    from the renderer** (:func:`_table_min_cols`). This is the drop `bottom` did not need
    while it carried both things: below `statusline._LEFT_W` the table refuses to draw at
    all, so the pane kept drawing the attention row above it and the operator saw no
    difference. Split apart (#515), the same case is a bordered rectangle with nothing in
    it — a pane that says "no repos" on a plane full of them, which is the false-clean
    reading the frame refuses everywhere else. The test is the PANE's width, not the
    window's: a `[frame] slots` naming `right` first insets this pane by 23 columns, so
    :func:`repos_cols` is asked with the slots that survived the line above.

    `bottom` never goes here, and it is the only slot that never does: it is the one
    alert and the command that fixes it, which is why a frame is worth drawing on a
    terminal with room for nothing else.
    """
    keep = list(slots)
    if cols < min_cols or rows < min_rows:
        keep = [s for s in keep if s != "right"]
    if rows < min_rows:
        keep = [s for s in keep if s != "top"]
    if "repos" in keep and not repos_fits(keep, window_cols=cols):
        keep = [s for s in keep if s != "repos"]
    if cols < min_cols // 2 or rows < min_rows // 2:
        keep = []
    return [s for s in slots if s in keep]


def repos_rows(*, content_rows: int, window_rows: int,
               slots: list[str] | tuple[str, ...] = ()) -> int:
    """How many rows the `repos` pane gets: what its content wants, floored and capped.

    Pure arithmetic, deliberately — this is the whole of #488's "how tall is the table?"
    and it is decided here, with no tmux and no filesystem, so both callers that need an
    answer (a launch, and the `window-resized` hook's own recompute) necessarily get the
    same one.

    * **The floor is `SLOT_SIZE["repos"]`.** A workspace with no clones still has one
      line to draw — that it has none, and the command that gets it one
      (`slots._empty_lines`). Never zero: a zero-row pane is one tmux refuses to split at
      all.
    * **The cap leaves the harness :data:`HARNESS_MIN_ROWS`.** *window_rows* is the whole
      window, *slots* is what else is being drawn in it, and every horizontal strip in
      :data:`_FIXED_ROW_SLOTS` costs its own height plus :data:`_BORDER_ROWS`. `right`
      costs columns, not rows, so it is not counted here — asking for the whole slot list
      rather than a pre-computed number is what keeps that decision in one place instead
      of at each call site.
    * **Between the two, the content wins.** *content_rows* is what `slots._repos` would
      actually fill (`slots.repos_rows_wanted`), so a two-repo plane gets a two-row strip
      rather than a fourteen-row one padded with blanks.

    The cap can come out below the floor — a 16-row window has no rows to spare at all —
    and the floor wins then, because `panel_argvs` has to be able to split the pane at
    all. What protects the harness in that terminal is :func:`visible_slots`, which drops
    every slot below half the size floors.
    """
    floor = SLOT_SIZE["repos"]
    other = sum(SLOT_SIZE[s] + _BORDER_ROWS
                for s in slots if s in _FIXED_ROW_SLOTS)
    cap = window_rows - other - _BORDER_ROWS - HARNESS_MIN_ROWS
    return max(floor, min(content_rows, cap))


def slot_sizes(slots: list[str], *, window_rows: int, content_rows: int) -> dict[str, int]:
    """Every slot in *slots* mapped to the size it should be given — rows for the
    horizontal strips, columns for the side.

    The one place `repos`' variable height and the other slots' fixed sizes are answered
    together, so a caller never has to know which kind a slot is. `panel_argvs` splits
    with it, `commands_frame._reassert_sizes` re-applies it, and the `window-resized`
    recompute calls it again with the window's NEW row count — which is the whole reason
    it takes *window_rows* rather than closing over a launch-time value.

    Unknown slot names are dropped rather than raised on, matching `visible_slots`'
    filter-don't-refuse discipline: `[frame] slots` is committed, untrusted input, and by
    the time a list reaches here it has already been through `instance.FRAME_SLOTS`.
    """
    out: dict[str, int] = {}
    for slot in slots:
        if slot in VARIABLE_ROW_SLOTS:
            out[slot] = repos_rows(content_rows=content_rows,
                                   window_rows=window_rows, slots=slots)
        elif slot in SLOT_SIZE:
            out[slot] = SLOT_SIZE[slot]
    return out


def harness_rows(sizes: dict[str, int], *, window_rows: int) -> int:
    """The rows left for the HARNESS pane once every horizontal strip in *sizes* has its
    own height and its own border — the number `commands_frame._reassert_sizes` asserts
    on the harness itself after a resize.

    **Why the harness is resized at all, which it never used to be.** tmux redistributes
    every pane proportionally on a window resize, so the intended sizes have to be
    re-applied; and `resize-pane -y` moves ONE boundary — the one below the pane, or the
    one above it when the pane is last in the stack. With two strips (`top` above the
    harness, `bottom` below it) every `resize-pane` therefore traded with the harness and
    asserting both was enough. #515 makes three, and the two below the harness trade with
    EACH OTHER instead: measured on tmux 3.7c at 200x50, asserting `top`, `bottom` and
    `repos` in split order left the table 1 row and the attention strip 6 — each
    assertion undoing the last, with the harness never consulted.

    So the harness is told its height explicitly and the variable slot
    (:data:`VARIABLE_ROW_SLOTS`) is left to take the remainder. Verified against tmux
    3.7c at 200x50, 200x24, 200x100 and 90x40, with and without the sidebar, and in both
    orders that put the harness before or after the strip below it: the table lands on
    exactly `repos_rows`' answer every time.

    *sizes* is :func:`slot_sizes`' map. Slots in :data:`_COLUMN_SLOTS` cost columns, not
    rows, and are not counted — the same rule :func:`repos_rows` keeps, read from the same
    constant. Floored at 1: a zero or negative `-y` is a resize tmux refuses, and a
    refused resize leaves the frame as tmux's own redistribution left it, which is worse
    than a harness squeezed to one row in a window that has no rows for it anyway.
    """
    used = sum(n + _BORDER_ROWS for slot, n in sizes.items()
               if slot not in _COLUMN_SLOTS)
    return max(1, window_rows - used)


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

    **`respawn-pane -e` ADDS to the pane's environment; it does not replace it.** This
    said the opposite ("REPLACES ... so everything the harness needs is on THIS call"),
    which is why this call carries `dict(os.environ, ...)` whole. Re-measured against tmux
    3.7c: a server started with `FOO`/`BAZ` in its environment, respawned with only
    `-e BAR=…`, produced a pane holding all three and a full-length `$PATH`. So the `-e`
    set is an OVERLAY on whatever the pane would have inherited anyway.

    That matters twice. It means a caller may carry only the variables that must differ
    (which `session_argv` and `panel_argvs` on charter's own server already did — see
    `commands_frame._frame_identity_env`, and note that a `-e` is argv and argv is
    world-readable). And it meant this call site's full-environment pass was a CHOICE
    rather than a requirement.

    **That choice is gone, and #446 is why.** This call carried `dict(os.environ, …)`
    whole into `/proc/<pid>/cmdline`: measured on one real environment, 129 argv elements,
    7,773 bytes, four live 1Password service-account tokens and an npm auth token. What it
    was buying was "the base the overlay lands on is THEIR tmux server's environment, which
    may predate this plane entirely". Two things were then measured against tmux 3.7c and
    settle it (`commands_frame._guest_harness_env` carries the numbers):

    * a pane's `$PATH` is the INVOKING CLIENT's, not the server's — charter's own, the
      one `cmd_launch`'s `shutil.which` resolved the harness against. An explicit
      `-e PATH=…` does not even survive: tmux overwrites it afterwards.
    * everything else a harness reads it reads for itself, from a server environment that
      belongs to the same operator on the same machine.

    So only `commands_frame._guest_harness_env` travels here. The cost is stated in
    `docs/frame.md` beside the other named costs of being a guest: the harness inherits
    the operator's TMUX SERVER environment rather than their current shell's — exactly
    what already happens on charter's own shared server. Do not put a credential on that
    command line: it is world-readable, and this function makes no promise about it.

    Every `-e` lands before the `--` — they are `respawn-pane`'s own options, and must
    never be grafted onto the harness's own argv.

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


#: Every environment variable name charter will ever put on a tmux command line, and the
#: whole of it.
#:
#: **The list is the promise, and it lives HERE because this is the only place a `-e`
#: is built.** #412 narrowed one call site (`session_argv`) and #446 was the other one
#: (`respawn_argv`) still passing `dict(os.environ, …)` whole — the same defect, found a
#: release apart, because the rule was enforced at the call sites rather than at the
#: single funnel every call site goes through. :func:`_env_argv` refuses anything else
#: outright, so the next call site cannot quietly leak the way that one did: a wide
#: environment is a loud `ValueError` in the test that first builds it, not 7,773 bytes
#: of `/proc/<pid>/cmdline` on somebody's laptop.
#:
#: Names, never a `CHARTER_` prefix glob, for `commands_frame._FRAME_IDENTITY`'s own
#: reason: a prefix would keep the promise for a variable nobody has invented yet.
#: `PATH` is the one non-charter name and `commands_frame._guest_harness_env` is where
#: its measurement is written down. Nothing here is ever a credential — that is the
#: property that makes an argv acceptable at all.
CARRIABLE = frozenset({
    "CHARTER_SESSION_ID", "CHARTER_HARNESS", "CHARTER_ROOT", "CHARTER_WORKSPACE",
    "CHARTER_PERSONA", "PATH",
})


def _env_argv(env: dict[str, str] | None) -> list[str]:
    """`-e NAME=VALUE` per entry, sorted so the command is the same on every launch.

    One argv element each, never a joined string, and always before a command's `--`:
    these are tmux's own options, not something to graft onto the program's argv. Empty
    for an empty (or absent) *env*, so a call that has nothing to carry produces exactly
    the command it always did.

    **Every name must be in :data:`CARRIABLE`.** Raising is the point: the alternative —
    dropping the extras quietly — would let a caller believe it had handed the harness a
    variable that never arrived. A silent drop makes this guard's own failures invisible;
    it does not make them rarer. Only NAMES appear in the message; a value that does
    not belong on a command line does not belong in a traceback either.
    """
    unlisted = sorted(set(env or {}) - CARRIABLE)
    if unlisted:
        raise ValueError(
            f"tmux `-e` may only carry {sorted(CARRIABLE)} — refusing "
            f"{len(unlisted)} other name(s): {unlisted}. A `-e` is argv, and argv is "
            "world-readable; see frame/layout.CARRIABLE")
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
                env: dict[str, str] | None = None,
                sizes: dict[str, int] | None = None) -> list[list[str]]:
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

    *env* is `-e`, and it carries charter's identity and nothing else on EITHER server.
    The sentence here used to say that on somebody else's server it "carries the
    launcher's environment whole", and #446 is that sentence: a panel needs the plane it
    draws stated (`$CHARTER_ROOT`, `$CHARTER_WORKSPACE` — it inherits that server's
    otherwise, whatever their shell had when they first ran `tmux`), and it needs nothing
    else. A panel starts no subprocess at all — `frame/gather.py`, `frame/slots.py` and
    `frame/panel.py` are pure file and state reads — so it has no use for `$PATH` either,
    and its own interpreter is already absolute (`panel_command`, #390).

    **On charter's own server it carried charter's identity and nothing else already, and
    the sentence that used to be here was wrong.** It said the panels inherit the launcher's
    environment "because `new-session` is what starts that server" — true of the launch
    that starts it and of no other (#411; see :func:`session_argv`). Every later frame's
    panels inherited the FIRST launcher's, and `$CHARTER_WORKSPACE` is the sharp one:
    `workspace.resolve` ranks it above every pointer, so a panel that inherited another
    launcher's pin could not be moved by `charter ws use` at all. `$CHARTER_SESSION_ID`
    was already correct there by a different route — `commands_frame._session_id_env_argv`
    ties it to the SESSION, which tmux applies to panes split later — and this fixes the
    rest of the identity alongside it.

    Only `commands_frame._FRAME_IDENTITY` travels, never the whole environment: a `-e` is
    argv, and argv is world-readable. See `commands_frame._frame_identity_env`.

    *sizes* is :func:`slot_sizes`' answer for this window, and it exists because `repos`
    is not a fixed height (#488). ``None`` falls back to :data:`SLOT_SIZE`, which is
    `repos`' FLOOR — right for a caller that has no window to measure (a test, a
    `--probe`), wrong for a launch, which is why both launch paths pass one. Read with a
    per-slot fallback rather than replaced wholesale, so a *sizes* missing an entry
    degrades to the fixed size instead of a `KeyError` inside a launch.

    **`-b` is `top`'s alone, and the rest of the reading order is the split order run
    backwards.** Every other split is a plain `-v`, which tmux places DIRECTLY below
    the harness — so a slot split later sits ABOVE one split earlier. Measured on tmux
    3.7c in a 120x40 window, splitting `top`, `bottom`, `repos`, `right` off the
    harness in that order: `top` at row 0, harness and `right` at rows 2-30, `repos` at
    rows 32-37, `bottom` on row 39. That is the frame #515 asks for — identity, the
    session, the repo table, the attention strip on the terminal's last row — and it is
    why the shipped `slots` list names `bottom` before `repos` and not after it.
    """
    cmds: list[list[str]] = []
    for slot in slots:
        size = (sizes or SLOT_SIZE).get(slot, SLOT_SIZE[slot])
        # :data:`_COLUMN_SLOTS` rather than a second list of names: which splits take
        # COLUMNS is exactly what :func:`repos_cols` has to know to answer how wide
        # `repos` ends up, and two copies of that fact are two things to keep in step.
        direction = "-h" if slot in _COLUMN_SLOTS else "-v"
        before = ["-b"] if slot == "top" else []
        cmds.append(_tmux(socket, "split-window", "-t", harness_pane,
                          direction, *before, "-l", str(size),
                          *_env_argv(env),
                          "-P", "-F", "#{pane_id}", "--",
                          *panel_command(slot=slot, session=session)))
    return cmds
