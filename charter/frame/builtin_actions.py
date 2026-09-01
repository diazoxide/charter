"""Charter's own actions — the rows the menu used to hold, expressed as the public seam.

`builtins.py` is this module one contract over: charter's own panels go through the
component seam a provider gets, and charter's own commands go through the action seam a
provider gets. There is no private table of rows beside the registry the palette lists.

**Everything the menu offered is reachable, and nothing else was invented.** Detach and
the three densities are here; the workspace and persona lists are one keypress further on,
in `frame/choose.py`, and the palette carries one row per noun that opens each. That is
Task 6, and it is a correction to what this module shipped in Task 4 rather than an
addition to it: a workspace is a *name*, not a thing this contract can honestly describe.
An `Action`'s promise is fire-and-report — ``run`` starts work and returns — and forty
names registered as forty actions meant forty ``run``s that each started a whole second
charter process to write two files. A picker chooses the name first and switches once, in
the pane the operator is already looking at.

The two things that were tmux's problem rather than charter's are still gone either way:
the twelve-name cap (the overlay scrolls) and the digit shortcut that ran out at nine.

**An action starts a DETACHED process, and that is not incidental.** §4g's fire-and-report
says `run` returns having started work; here it must also OUTLIVE the pane it was started
from, because the palette closes the instant it has invoked — `kill-pane` on the overlay
hands SIGHUP to that pane's process group, and a worker thread in charter's own palette
process dies with it. `start_new_session=True` is what puts the child outside that group.
Measured, before it was there: `density: minimal` from the palette re-laid-out nothing at
all, because the `charter frame-density` child was killed between `Popen` and its first
tmux call.

**Availability is read off state, never off a subprocess.** Every row's `available` is
asked once each time the palette opens, and one tmux round trip per action is a palette
that takes a visible moment to draw. The two refusals charter can actually report — a
frame with no recorded harness pane, and a `$CHARTER_WORKSPACE`/`$CHARTER_PERSONA` pin —
are both files the launcher already wrote.

**Ids are charter's, titles carry the operator's names.** `frame.action.Action` holds an
id to the component alphabet because it reaches a `bind` line, and a workspace may be
named `my-repo.v2`, which is not in that alphabet. That is why the picker's own rows are
not actions at all — see `frame/choose.py`.
"""

from __future__ import annotations

import os
import subprocess

from .. import util
from . import action, actions, choose, state, tmuxctl

#: What marks the density a frame is currently on. **One constant, not two**: it is
#: `frame/choose.py`'s, which marks the workspace and the persona a frame is on for the
#: identical reason, and a second copy here would be a second answer to what "the one you
#: are on" looks like. See that module for why it is ASCII.
#:
#: Nothing in this module composes it into a title any more — `action.Action.mark` says
#: which row is the one in effect and `frame/overlay.py` draws it, which is #749. The
#: name is kept because it is what a reader looking for "how is the current one marked"
#: comes here for, and because it is still the width `frame/slots.py` measures its own
#: rows against.
MARK = choose.MARK


def _spawn(argv: list[str], *, fid: str) -> None:
    """Start *argv* so it outlives the pane the palette is drawn in. Never waits.

    `start_new_session=True` is the whole point — see the module docstring. The three
    standard streams go to `DEVNULL` for the second half of the same reason: the child's
    stdout would otherwise still be the overlay pane's tty, which charter is about to
    kill, and a write to a closed pty is an `EIO` in a process that was working correctly.

    `$CHARTER_SESSION_ID` is set explicitly rather than inherited. It IS inherited today,
    session-scoped by `commands_frame._session_id_env_argv` — but "which frame am I" is
    the one value every one of these commands resolves itself from, and a palette that
    passed it implicitly would be trusting a tmux server shared between every frame on the
    machine to have tied it to the right session (`_session_id_env_argv` measures what
    happens when it has not).
    """
    subprocess.Popen(argv, start_new_session=True,
                     stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                     stderr=subprocess.DEVNULL,
                     env=dict(os.environ, CHARTER_SESSION_ID=fid))


def _laid_out(fid: str) -> bool:
    """Whether charter still knows which pane this frame's harness is in.

    The exact condition `commands_frame.cmd_density` refuses on, asked here so the row
    says so instead of the keypress doing nothing. A frame launched by a charter that
    predates `state.record_harness_pane`, or one whose state directory was truncated, has
    nothing to split a re-layout against — and `frame/layout.py`'s module docstring
    measures what guessing at a pane id costs.
    """
    return bool(tmuxctl.PANE_ID_RE.fullmatch(state.harness_pane(fid) or ""))


#: What a frame with no recorded harness pane is told instead of nothing happening.
NO_LAYOUT = ("charter has no record of this frame's harness pane, so it cannot move the "
             "panels — relaunch the frame")


def _server(fid: str) -> str:
    """Which tmux *fid* lives on — its own record, else charter's private one.

    The same fallback every other frame command spells (`state.frame_server(fid) or
    SOCKET`), asked in one place here so an action that ASKS about the server and an
    action that COMMANDS it cannot answer differently. A frame with no recorded server is
    a frame launched by a charter that predates `state.record_server`, and charter's own
    socket is where it will be — never "nowhere", which would make `_detachable` report an
    operator's tmux for a frame that is not in one.

    Imported lazily: `commands_frame` imports this module at load, so the name has to be
    reached at call time or the two would be a cycle. It is defined there because that is
    where the server is STARTED.
    """
    from ..commands_frame import SOCKET
    return state.frame_server(fid) or SOCKET


def _detach(fid: str):
    """`detach-client -s <fid>`, the spec's own "Detach is allowed".

    `-s` and never `-t`: `detach-client`'s `-s` targets every client attached to a
    SESSION, `-t` a single CLIENT. A frame normally has exactly one, and `-s` is still
    the correct answer if it ever has more.
    """
    _spawn(tmuxctl.server_argv(_server(fid), "detach-client", "-s", fid), fid=fid)
    return "detaching — the harness keeps running"


def _detachable(fid: str) -> bool:
    """Whether `detach-client -s <fid>` names anything.

    Inside an operator's own tmux a frame is a WINDOW, not a session, so `-s <fid>`
    targets a session that does not exist — the same fact `cmd_density` records for the
    menu it declined to write there. The row stays, with the operator's own prefix key
    named in its reason, because "there is no charter key here" is exactly what somebody
    pressing `F2` needs to be told.
    """
    return not tmuxctl.is_operator_socket(_server(fid))


def _register_detach(reg: actions.ActionRegistry) -> None:
    reg.register(action.Action(
        id="frame.detach", title="detach — leave the harness running",
        run=lambda ctx: _detach(ctx.fid),
        available=lambda ctx: _detachable(ctx.fid),
        reason_unavailable=lambda ctx: (
            "this frame is a window in your own tmux, not a charter session — detach "
            "with your own prefix key")))


#: What a plane with nothing to select is told instead of a row that does nothing.
#: Its own sentence rather than :data:`NO_LAYOUT`'s or :data:`NO_PANES`': those are about
#: records charter lost, and this is about a workspace that has no clones in it yet — a
#: state that is ordinary and fixable, so the row carries the fix the way `_empty_lines`
#: does in the pane it is about.
NO_REPOS = ("this workspace has no clones for the repo table to select from — "
            "charter clone <repo>")

#: Which way each row walks the table, in the order the palette lists them, and where a
#: walk with no selection under it STARTS — the row before the first for `next`, the row
#: after the last for `previous`, so that `(start + step) % len` lands on the end the
#: direction is coming from.
#:
#: **Spelled per direction rather than derived from the step's sign**, and the deletion
#: sweep is why. `-1 if step > 0 else 0` reads fine and cannot be tested: a step here is
#: only ever `+1` or `-1`, so `step > 0` and `step >= 0` answer identically for every value
#: that reaches them, and a comparison no test can distinguish is a line this repository
#: deletes rather than documents. Written down, the two starts are data and the arithmetic
#: below has no branch left in it.
_SELECT_STEPS = (("next", 1, -1), ("previous", -1, 0))


def _register_selection(reg: actions.ActionRegistry) -> None:
    """Two rows that move the `repos` table's selected row — **the keyboard's half.**

    **`[frame] mouse` is off on most planes and charter does not own the harness**, so a
    component whose only route to a piece of state is a click has no route to it there —
    `component.EVENT_KINDS` states that to a provider and then asks for the one thing that
    fixes it: *give every pointer affordance a key as well.* The `repos` table's selection
    is charter's own first pointer affordance, so this is charter keeping its own rule. On
    a plane with the flag off these rows are the ONLY way to move it, and on a plane with
    it on they are still the way to move it without reaching for the mouse.

    **The palette is the keyboard, and arrow keys are how it is driven.** `F2` opens it,
    `overlay.Surface` moves its selection on `up`/`down`, and `Enter` chooses — which is
    the vocabulary the pointer half is deliberately held to as well (a click SELECTS, only
    a keypress chooses). Charter cannot put a bare arrow key in front of the repo table
    itself: a `bind -n Up` is server-wide and would intercept the arrow BEFORE the harness
    the frame exists to protect, and `frame/events.py` does not deliver `key` to a panel
    for the matching reason — tmux routes typing to the ACTIVE pane, which is the harness's.

    **They walk the table's DISPLAY order, which is the gather's**, not the ranking the
    pane's window is cut from (`statusline._pick_rows` re-sorts its pick back into cache
    order for exactly this reason: a row that moves as its state changes stops being a
    place you can look). So "next" is the row under the one that is highlighted, as read.

    **It wraps, and no-wrap was the alternative.** Stepping off the end and stopping makes
    a palette row that visibly does nothing, which reads as broken and costs the operator
    a whole `F2` to find out; wrapping makes every press move something. Nothing here is
    irreversible, so there is no half of this a wrap could do damage with.

    **Nothing is selected yet is not a failure**, and it is the common case the first time
    this is pressed: the walk starts at the top of the table for `next` and at the bottom
    for `previous`, which is what an operator who pressed a direction key on a list with no
    cursor in it means. A selection naming a repo this plane no longer has is the same
    case — it is not in the list, so the walk starts from the end the direction came from
    rather than from a position that does not exist.

    Both `state.record_selection` and `state.bump` run HERE, in the palette's own process,
    rather than through :func:`_spawn` like the density and surface rows. Those two have to
    outlive the pane because they re-lay-out a frame through several tmux calls; this is one
    atomic file write and one more, and `cmd_palette` joins the invocation before it closes
    the pane (`_ACTION_START_GRACE`), so a subprocess would buy nothing and cost a whole
    interpreter start.
    """
    for word, step, start in _SELECT_STEPS:
        reg.register(action.Action(
            id=f"repo.{word}", title=f"repo: select the {word} row",
            touches=("repos",), repeat=True,
            run=(lambda ctx, st=step, sr=start: _select(ctx, st, sr)),
            available=lambda ctx: bool(ctx.repos),
            reason_unavailable=lambda ctx: NO_REPOS))


def _select(ctx, step: int, start: int):
    """Move this frame's repo selection *step* rows through the table's display order.

    Split out of the row above so the walk can be exercised against a list of names
    without a palette, a tmux server or a frame directory — the same reason
    `slots._fit_fields` is not inside `slots._bottom`.

    The modulo is what wraps. *start* is where a walk with no selection under it begins,
    and it is :data:`_SELECT_STEPS`' third column rather than something worked out here —
    **one starting point for both directions was the first version and was wrong**: `next`
    has to land on the first row and `previous` on the last, and a single sentinel can only
    be right for one of them (`-1` gives `next` its 0 and gives `previous` the row second
    from the bottom, a cursor appearing in the middle of a list nobody had put one in).
    Deriving it from the step's SIGN was the second version and could not be tested; see
    that constant for why.

    `ctx.repos` is never empty here: `available` refuses the row on a plane with no clones,
    and `ActionRegistry._check` builds ONE ctx and hands that same one to `run` — so the
    list this indexes is the list that was asked about, and a guard for an empty one would
    be a line nothing could turn red.
    """
    names = [r.get("name") for r in ctx.repos]
    chosen = state.selection(ctx.fid)
    here = names.index(chosen) if chosen in names else start
    name = names[(here + step) % len(names)]
    state.record_selection(ctx.fid, name)
    # The `repos` pane redraws its highlight and the `attention` pane redraws the detail,
    # and neither is this process — see `frame/builtins._repos_events` for the same two
    # lines said from the pointer's side.
    state.bump(ctx.fid)
    return f"selected {name}"


#: What a workspace with nothing open is told instead of a row that does nothing.
#: :data:`NO_REPOS`' shape and its argument — the row carries the fix, because a refusal
#: that names no way out is a row an operator reads once and never again. `charter
#: workspace todo "…"` with a quoted argument RECORDS one; bare it lists (`cli._wire`),
#: which is why the sentence quotes the recording form and not the reading one.
NO_TODOS = ("this workspace has nothing open to read out — "
            "charter workspace todo \"<what to do>\"")


def _register_todos(reg: actions.ActionRegistry) -> None:
    """One row that reads this workspace's open todos out, one press at a time — **the
    keyboard route to the sidebar's `…(+N more)`** (#742).

    **The defect this closes is a row with no route beside it.** `slots._todo_rows` draws
    `…(+5 more)` on a sidebar too short for seven todos, and until this row those five were
    reachable only by typing `charter workspace todo` into the harness — the surface the
    frame exists to replace. The sidebar's persona column got its route in #765 (a click
    switches, a click on the badges explains); the todo list had no gesture at all, and its
    own overflow line must go on having none, because a row standing for items it does not
    name cannot resolve to one (`slots._Chips.hit`'s rule).

    **A KEY and not a wheel, and that ordering is the whole design.** `[frame] mouse` ships
    off, so a pointer-only answer is inert on exactly the planes this was reported from —
    and `docs/frame.md`'s rule runs the other way anyway: a pointer affordance always has a
    key or a palette row beside it, so the keyboard route is the half that must exist. A
    wheel over the section can be added on top of this later; it cannot stand in for it.

    **Why the palette's HEADER rather than a list surface of its own.** `commands_frame
    ._again` argues this in full for the repo-selection rows and every word applies: the
    overlay pane is zoomed over the whole window, so the sidebar is not on screen behind
    the palette, and `Invocation.note` on the header is the surface the operator is
    actually looking at. A read-only list surface would be the nicer answer and is a real
    change — a fifth `frame/choose.py` noun breaks that module's "a noun is a thing you
    switch to" contract at the point where picking a row tries to switch to it.

    **`repeat=True`, so five hidden todos cost five keypresses and one palette.** Without
    it each Enter closes the pane, kills the process and re-splits for the next — the
    fourteen-keystroke, three-cycle cost #746 measured on the repo rows.

    **The cursor lives in a CELL closed over here, and that is not laziness about state.**
    `_select` records the repo selection through `state` because a PANE has to redraw from
    it: two processes, one fact. Nothing redraws from this — the answer is the sentence
    `run` returns — so a `state` write plus the `state.bump` that wakes every panel would
    be four panels repainting per keypress to move a number nobody else reads. The
    registry is rebuilt per palette (`build`'s "a fresh registry per call") and
    `commands_frame._again` invokes through that same registry, so the cell lives exactly
    as long as the palette the operator is reading and starts again at the top next time —
    which is what an operator who opened `F2` to read their todos means.

    A list rather than an `itertools.count`: :func:`_read_todo` needs the value it is
    about to report, wrapped against a list whose length it only learns from `ctx`.
    """
    seen = [0]
    reg.register(action.Action(
        id="todo.next", title="todo: read the next open todo",
        touches=("todos", "gather"), repeat=True,
        run=(lambda ctx: _read_todo(ctx, seen)),
        available=lambda ctx: bool(_open_todos(ctx)),
        reason_unavailable=lambda ctx: NO_TODOS))


def _open_todos(ctx) -> list[dict]:
    """The todos on *ctx* that are actually rows — `slots._todo_rows`' own filter.

    A gather cache is a JSON file written by another process and may hold anything;
    `ctx.todos` contains it no further than making the list a tuple (`ctx.SERVES` says the
    containment is shallow in as many words). Asked in one place so `available` and
    :func:`_read_todo` cannot disagree about whether there is a row to read — a row listed
    as available that then answers "nothing" is the shape `ActionRegistry._check` builds one
    ctx to prevent.
    """
    return [t for t in ctx.todos if isinstance(t, dict)]


def _read_todo(ctx, seen: list[int]) -> str:
    """The next open todo, as one sentence for the palette's header.

    Split out of the row above for :func:`_select`'s reason: the walk is exercised against
    a list of titles with no palette, no tmux server and no frame directory under it.

    **The total is `slots.todo_total`, READ rather than counted here**, and #742 is why it
    is worth a shared function: `gather._MAX_TODOS` bounds the LIST the cache holds while
    `todo_count` records what was there before the bound, so counting `ctx.todos` would
    report `3/20` under a sidebar heading saying `todos 400`. One answer, two surfaces.

    **It wraps, and the position is what makes the wrap readable.** `_select`'s argument
    for wrapping holds — a row that visibly does nothing at the end of a list reads as
    broken and costs a whole `F2` to find out — and `3/7` is what stops a wrap looking like
    a repeat. The clip is stated too: on a plane whose cache holds twenty of four hundred,
    `3/400` over twenty readable titles would be a promise the cache cannot keep, so the
    sentence says which list it is walking.

    `open_todos` is oldest-first and that ordering is the point of it (`slots._todo_rows`),
    so this reads in the same order the sidebar draws — the row after the last one on
    screen is the next press, not a fresh sample.

    Not contained here. `Palette.report` puts this on `Palette.heading` through
    `_headline`, which runs `contain.one_line` over it before `tui.width` sees it, and
    `frame/choose.py` records what a second containment on the way in costs: a line no test
    can turn red.
    """
    from . import slots
    items = _open_todos(ctx)
    if not items:
        # `available` refuses this row on a plane with nothing open, so the palette never
        # reaches here — but `run` is callable without that check (a provider's own
        # surface, and `tests.test_frame_palette` drives every built-in's `run` directly),
        # and `% 0` is the one way this could raise instead of answering. An empty answer
        # is `Palette.report`'s own "an action that answered nothing has nothing to
        # report", which clears the header rather than inventing a sentence about a list
        # that is not there. Unlike `_select`'s deliberately absent guard, this one is
        # reachable: the modulo needs a length and the ctx is what carries it.
        return ""
    total = slots.todo_total(ctx.gather)
    at = seen[0] % len(items)
    seen[0] = at + 1
    where = f"{at + 1}/{len(items)}"
    if total > len(items):
        where += f" of {total}"
    return f"todo {where}: {items[at].get('title') or ''}"


def _register_density(reg: actions.ActionRegistry, *, current: str) -> None:
    """One row per density level, with the level in effect marked rather than dropped.

    Marked and not filtered, for `_menu_entries`' own reason: choosing the level you are
    already on re-lays-out to the same frame, which is harmless, and a list whose rows
    move around depending on state is a list nobody learns.
    """
    from .. import instance
    for level in instance.FRAME_DENSITY:
        reg.register(action.Action(
            id=f"density.{level}", title=f"density: {level}", mark=(level == current),
            run=(lambda ctx, lv=level: _run_density(ctx.fid, lv)),
            available=lambda ctx: _laid_out(ctx.fid),
            reason_unavailable=lambda ctx: NO_LAYOUT))


def _run_density(fid: str, level: str):
    _spawn(util.self_relaunch_argv("frame-density", level), fid=fid)
    return f"density → {level}"


def _register_chrome(reg: actions.ActionRegistry, *, current: str) -> None:
    """One row per pane surface, with the one in effect marked rather than dropped.

    **The whole reason `[frame] chrome` can ship defaulting to `off`.** The spec's §4
    argues the default from an asymmetry — a light-terminal operator upgrading into a
    default `dark` gets a worse frame having done nothing — and the cost of that argument
    is a dark-terminal operator who wants the fill. These three rows are what they pay
    instead: one keystroke, in the palette they already open, rather than finding a
    config key in a document.

    Marked and not filtered, for `_register_density`'s reason exactly: choosing the
    surface you are already on repaints to the same frame, which is harmless, and a list
    whose rows move around depending on state is a list nobody learns.

    **`_laid_out` is NOT the availability question here**, and that is not an oversight.
    A density row re-lays-out and therefore needs a harness pane to split against; a
    surface is a pane option on panes that already exist, so what it needs is the pane
    MAP. A frame whose harness pane record was lost can still be resurfaced, and a row
    that refused it would be refusing on somebody else's precondition — the shape #512
    is. `state.panes` is the record `cmd_chrome` actually reads, so the row asks about
    that one.
    """
    from .. import instance
    for level in instance.FRAME_CHROME:
        reg.register(action.Action(
            id=f"chrome.{level}", title=f"chrome: {level}", mark=(level == current),
            run=(lambda ctx, lv=level: _run_chrome(ctx.fid, lv)),
            available=lambda ctx: bool(state.panes(ctx.fid)),
            reason_unavailable=lambda ctx: NO_PANES))


#: What a frame with no recorded panel panes is told instead of nothing happening. Its own
#: sentence rather than `NO_LAYOUT`'s: the two refusals are about different records, and a
#: row that borrowed the other's words would send an operator to relaunch over a frame
#: whose panels simply all failed to draw.
NO_PANES = ("charter has no record of this frame's panel panes, so it has nothing to "
            "resurface — relaunch the frame")


def _run_chrome(fid: str, level: str):
    _spawn(util.self_relaunch_argv("frame-chrome", level), fid=fid)
    return f"chrome → {level}"


def _register_regather(reg: actions.ActionRegistry) -> None:
    """One row that re-runs this frame's gather — **#735's route out.**

    `charter frame-gather` already existed and already fixed a broken cache instantly. What
    it wants is `--session` and `--workspace`, and an operator sitting in front of a repo
    pane that will not draw can discover neither: the frame id is charter's own
    `{workspace}-{hash}`, and the flags are required precisely because a detached child
    must never guess at them (#512, and `cmd_gather`'s own docstring). The palette knows
    both, so the row is the command with its two answers already filled in.

    **Always available, and it is the one row that has to be.** Every other refusal this
    module reports is about a record charter lost — a harness pane, a pane map — and this
    is the action an operator reaches for when a pane has already failed. `cmd_gather`
    needs nothing but the two names, and a row that refused on somebody else's
    precondition would be refusing exactly when it is needed.

    **Registered LAST among charter's own**, which is the one thing about it that is a
    choice. Nearer the top would read better on the day you need it and would move every
    density and chrome row down one — and `_register_density`'s own note is that a list
    whose rows move is a list nobody learns. This is a row you reach for once, typed at,
    by an operator who is already looking for it; the rows above it are pressed by muscle
    memory.

    **It refreshes the whole snapshot and the title says so.** `gather.refresh` writes
    repos, worktrees, todos and changes under one timestamp (§4f's clock rule), so a title
    naming only the repo table would be describing a third of what the row does — and the
    todo count in `bottom` is drawn from the same file the broken table is.
    """
    reg.register(action.Action(
        id="frame.gather",
        title="refresh — gather this workspace's repos, todos and changes again",
        run=lambda ctx: _regather(ctx.fid)))


def _regather(fid: str):
    """Start `charter frame-gather` for *fid*, detached, and say so.

    **`state.workspace_for` and never `workspace.resolve`** — the one rule every frame
    surface asks (#512). This runs in the palette's process, which is a tmux child with
    no terminal of the operator's and a cwd that is not theirs, so resolving for itself
    would refill the cache from whatever workspace THAT process landed on. A gather that
    answered `default` on a plane that is not on it is the defect this row exists to
    repair, arriving by a different door.

    Detached through :func:`_spawn` like the density and surface rows, and for the module
    docstring's reason: the palette closes the instant it has invoked, and a git sweep
    running in the pane being killed dies with it. The argv is
    `commands_frame._spawn_gather`'s, verbatim — two ways to start the same gather that
    must not drift apart.

    **No `contain.one_line` over *ws*, and the sweep is what settled that.** The name is
    genuinely unchecked — `frame/slots.py`'s `_empty_lines` records that `workspace_for`'s
    last rung hands back `$CHARTER_WORKSPACE` stripped and otherwise untouched — so both
    places it reaches were measured rather than assumed. The argv is a LIST, so a name
    carrying a newline arrives at `cmd_gather` as one argument whatever is in it. The
    RECEIPT is contained by the contract: `actions.Invocation._work` runs
    `contain.one_line(str(note), limit=REASON_LIMIT)` over whatever this returns, which is
    the same reason `_select` hands back a bare repo name. A second call here changed no
    outcome — the deletion sweep found it surviving, correctly — and `_empty_lines`' own
    warning is exactly this shape: a guard kept for a reason that is not the true one is
    how the real one gets deleted later.
    """
    ws = state.workspace_for(fid)
    _spawn(util.self_relaunch_argv("frame-gather", "--session", fid, "--workspace", ws),
           fid=fid)
    return f"gathering {ws}…"


def build(fid: str, *, current_density: str,
          current_chrome: str) -> actions.ActionRegistry:
    """Charter's own actions, plus every action an installed provider supplies.

    A fresh registry per call, for `builtins.build`'s reason: this is a snapshot of a
    plane that moves, and the marks, the names and the availability are all resolved
    against the moment the palette opened.

    Charter's own are registered FIRST, so `frame.detach`, the densities and the surfaces
    are at the top of a palette that has not been typed into, and a provider cannot push
    them down the list by installing something alphabetically earlier.

    **Both marks are ARGUMENTS and neither is read here**, which is the same rule stated
    twice rather than a repetition: `current_density` and `current_chrome` are resolved by
    `commands_frame._current_density` and `_current_chrome`, the same two functions the
    panels and the tmux options read, so the mark beside a row and the state the frame is
    actually in cannot come from two different readings. Required keyword arguments, both
    of them: a default here would let a caller that forgot one draw a palette marking
    `off` on a frame that is surfaced, and be right about it in every test that did not
    set one.

    **A provider's failure costs its own row and never the palette** — that is
    `ActionRegistry.add`'s contract and the whole reason a failed provider finally has
    somewhere to speak. Nothing here catches anything: `add` turns a missing distribution,
    a bad import, a version charter does not speak and an id collision each into a row
    that says which.

    **`fid` is no longer read while BUILDING, and it stays in the signature anyway.** The
    workspace and persona lists left with `_register_names` (see the module docstring), and
    what remains — detach, the densities, a provider's own — resolves the frame at the
    moment it is *invoked*, off `ctx.fid`. Dropping the parameter would mean every caller
    stopped saying which frame it was building for, and `frame/actions.py` takes `fid` on
    both `offers` and `invoke` precisely because that answer must never be ambient: one
    tmux server is shared by every frame on the machine, and this process's own
    `$CHARTER_SESSION_ID` may be another frame's (`state.record_identity` measures it).
    """
    reg = actions.ActionRegistry()
    _register_detach(reg)
    _register_selection(reg)
    _register_todos(reg)
    _register_density(reg, current=current_density)
    _register_chrome(reg, current=current_chrome)
    _register_regather(reg)
    for aid in reg.providers.ids():
        reg.add(aid)
    return reg
