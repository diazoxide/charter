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
            touches=("repos",),
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


def _register_density(reg: actions.ActionRegistry, *, current: str) -> None:
    """One row per density level, with the level in effect marked rather than dropped.

    Marked and not filtered, for `_menu_entries`' own reason: choosing the level you are
    already on re-lays-out to the same frame, which is harmless, and a list whose rows
    move around depending on state is a list nobody learns.
    """
    from .. import instance
    for level in instance.FRAME_DENSITY:
        on = MARK[0] if level == current else MARK[1]
        reg.register(action.Action(
            id=f"density.{level}", title=f"{on}density: {level}",
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
        on = MARK[0] if level == current else MARK[1]
        reg.register(action.Action(
            id=f"chrome.{level}", title=f"{on}chrome: {level}",
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
    _register_density(reg, current=current_density)
    _register_chrome(reg, current=current_chrome)
    for aid in reg.providers.ids():
        reg.add(aid)
    return reg
