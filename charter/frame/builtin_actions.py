"""Charter's own actions — the rows the menu used to hold, expressed as the public seam.

`builtins.py` is this module one contract over: charter's own panels go through the
component seam a provider gets, and charter's own commands go through the action seam a
provider gets. There is no private table of rows beside the registry the palette lists.

**Everything the menu offered is here, and nothing else was invented.** Detach, the three
densities, every workspace and every persona — the same four groups `_menu_entries` and
`_switch_entries` built, minus the two things that were tmux's problem rather than
charter's: the twelve-name cap (the overlay scrolls) and the submenu (a filter reaches a
name in fewer keystrokes than a nested list does).

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
named `my-repo.v2`, which is not in that alphabet. So a workspace's row is
`workspace.w<N>` with the NAME in the title, contained by `Action.__post_init__` before
anything measures it. The id is never drawn and never committed — this registry is rebuilt
every time the palette opens, and the invocation happens against the same object — so
nothing depends on `w3` meaning the same workspace twice.
"""

from __future__ import annotations

import os
import subprocess

from .. import contain, util
from . import action, actions, state, switch, tmuxctl

#: What marks the workspace, persona or density a frame is currently on.
#:
#: **ASCII, deliberately** — the same rule `statusline._persona_chips` records having
#: been broken twice by an East-Asian *Ambiguous* glyph, and `overlay.Surface.render`
#: restates for its own marker. `*` is what `git branch` already uses for "the one you
#: are on" and is unambiguously narrow everywhere.
MARK = ("* ", "  ")

#: What a workspace row's id counts up, and what a persona row's does. Charter's own
#: prefixes, in the action alphabet, with the position appended — see the module
#: docstring for why the name cannot be the id.
WORKSPACE_ID = "workspace.w{}"
PERSONA_ID = "persona.p{}"


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


def _register_switches(reg: actions.ActionRegistry, fid: str) -> None:
    """Every workspace and every persona this plane has, as rows.

    **Every one of them.** The menu capped each list at twelve because a `display-menu` is
    drawn inside the terminal and tmux does not scroll it; this surface scrolls and
    filters, so the cap would only hide names an operator can already reach faster by
    typing three letters of one.

    Two registrations rather than one loop over a table of two, because the two differ in
    every part that matters — which lister, which pin, which command flag — and a table
    would be a shared shape hiding four differences.
    """
    _register_names(reg, WORKSPACE_ID, switch.workspaces(),
                    current=switch.current_workspace(fid), noun="workspace",
                    pin="CHARTER_WORKSPACE")
    _register_names(reg, PERSONA_ID, switch.personas(),
                    current=switch.current_persona(fid), noun="persona",
                    pin="CHARTER_PERSONA")


def _register_names(reg: actions.ActionRegistry, template: str, names: list[str], *,
                    current, noun: str, pin: str) -> None:
    """One row per *name*, each running `charter frame-switch --<noun> <name>`.

    **The pin is the availability rule, and it is the one refusal `switch.py` states as
    permanent.** `$CHARTER_WORKSPACE` was set at launch, so it sits in every panel pane's
    environment and nothing charter writes can outrank it; a row that offered to switch
    anyway would report "switched" and draw the old workspace, which is the failure #411
    was filed for. The session LOCK is not an unavailability — `to_workspace` overrides it
    deliberately and says so in its own message — so it is not reported as one here.

    `contain.one_line` before the name is measured or padded (#472), and again by
    `Action.__post_init__` on the title it lands in. Twice, because they are two different
    joins: this one is where a name meets charter's own marker column, and that one is
    where a title meets the contract.
    """
    for i, name in enumerate(names):
        on = MARK[0] if name == current else MARK[1]
        reg.register(action.Action(
            id=template.format(i),
            title=f"{on}{noun}: {contain.one_line(name)}",
            run=(lambda ctx, n=name, v=noun: _run_switch(ctx.fid, v, n)),
            available=(lambda ctx, p=pin: not switch._pin(ctx.fid, p)),
            reason_unavailable=(lambda ctx, p=pin: _pinned_reason(ctx.fid, p))))


def _pinned_reason(fid: str, pin: str) -> str:
    """Why a pinned frame will not switch, naming the value that pins it.

    The same sentence `switch.to_workspace` refuses with, built from the same read, so the
    row and the keypress cannot describe one frame two ways.
    """
    return (f"cannot switch: ${pin} pins this frame to "
            f"'{contain.one_line(switch._pin(fid, pin) or '')}'")


def _run_switch(fid: str, noun: str, name: str):
    _spawn(util.self_relaunch_argv("frame-switch", f"--{noun}", name), fid=fid)
    return f"{noun} → {contain.one_line(name)}"


def build(fid: str, *, current_density: str) -> actions.ActionRegistry:
    """Charter's own actions, plus every action an installed provider supplies.

    A fresh registry per call, for `builtins.build`'s reason: this is a snapshot of a
    plane that moves, and the marks, the names and the availability are all resolved
    against the moment the palette opened.

    Charter's own are registered FIRST, so `frame.detach` and the densities are at the top
    of a palette that has not been typed into, and a provider cannot push them down the
    list by installing something alphabetically earlier.

    **A provider's failure costs its own row and never the palette** — that is
    `ActionRegistry.add`'s contract and the whole reason a failed provider finally has
    somewhere to speak. Nothing here catches anything: `add` turns a missing distribution,
    a bad import, a version charter does not speak and an id collision each into a row
    that says which.
    """
    reg = actions.ActionRegistry()
    _register_detach(reg)
    _register_density(reg, current=current_density)
    _register_switches(reg, fid)
    for aid in reg.providers.ids():
        reg.add(aid)
    return reg
