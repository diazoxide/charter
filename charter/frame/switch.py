"""Moving a running frame to another workspace or persona — #517, and the half of #518
that happens once a name has been chosen.

One mechanism, four steps, and every surface that switches goes through it: **list the
names, contain them, perform the switch, repaint the panels.** The palette
(`frame/builtin_actions._register_names` → `cmd_switch`) and the launch picker
(`frame/picker.py` → `cmd_launch`) are two front doors onto the same rooms.

**A switch is a write to the frame's OWN identity, never a pointer somebody might read.**
#411 is the whole reason this file is not four lines: panels followed `charter ws use`
only through the `$CHARTER_SESSION_ID` collision, and #412 made that identity explicit on
tmux's `-e`. `state.workspace_for` is the one rule every frame surface asks, and its rungs
are, in order: the `$CHARTER_WORKSPACE` pin, `workspace.for_session(fid)`,
`state.frame_workspace(fid)`, a local resolve. So a workspace switch writes rungs 1 and 2
— the per-session pointer under the FRAME's id, and the frame's recorded workspace — and
refuses outright when rung 0 is occupied, because nothing this process can write outranks
a variable already sitting in a live pane's environment.

**Which is exactly why the pin is a refusal and not a best effort.** A panel pane was
started with `-e CHARTER_WORKSPACE=<pin>` (`commands_frame._frame_identity_env`); that
value is in the pane's process environment for as long as the pane lives, and no file
charter writes can take it out. Reporting "switched" and drawing the old workspace is the
failure mode #411 was filed for. Reporting "this frame is pinned" is the honest one.

**The session lock is overridden, deliberately, and said out loud.** `workspace.set_active`
locks the session to what it selected, so the switcher's own first write takes the lock and
a second switch would be refused by the switcher's own doing — a switcher that works once
is worse than none. The lock exists so a workspace cannot be swapped out from under a
running task; an operator pressing a key and choosing a name off the palette is not that. So
:func:`to_workspace` forces, and :data:`Outcome.message` names the workspace the lock was
taken for, so nothing moves without the operator being told what moved.

**And nothing here writes a TERMINAL pointer**, which is #411 arriving on this command.
Both `workspace.set_active` and `persona.set_active` normally write one, keyed on
`$TERM_SESSION_ID`/`$TMUX_PANE`/`$STY`/`$SSH_TTY` — and in a `run-shell` child of charter's
private tmux server those names belong to whichever launcher STARTED that server, which is
shared between every frame on the machine and may be days old. A switch inside frame B
would have moved the workspace of the terminal that launched frame A. Both calls below
pass `terminal_id=""`: this process genuinely has no terminal to speak for.

**Nothing here creates anything.** `charter workspace create` is a real side effect on the
plane (#518: "a picker that creates on a typo leaves litter"), so an unknown name is a
refusal with the existing names beside it, never an implicit create. Creating is the
launch picker's own explicitly-confirmed step, in `commands_frame`, on a name the operator
typed.

Every name that reaches a message goes through `contain.one_line` first — a workspace or
persona name is a committed value, and a message is a line of charter's own output that a
newline in a name could forge a second line of (`contain.py`, #453).

There is no longer a tmux side to that rule. Until #729 the outcome went out as a
`display-message`, whose argument tmux parses as a FORMAT, and `frame/tmuxctl.inert_format`
was what made the line inert before it got there. The outcome is now written to the frame's
own state and drawn by its attention panel into a pane charter owns, so nothing on this
path reaches a tmux parser and `contain.one_line` is the whole of the containment.

No tmux call is made from here at all. That keeps this module testable without a server —
`commands_frame` owns the one call that puts a refusal on screen.
"""

from __future__ import annotations

from typing import NamedTuple

from .. import contain
from . import state


class Outcome(NamedTuple):
    """What a switch did, and the one line the operator is shown for it.

    ``ok`` is whether the frame moved. ``message`` is always set — a refusal that says
    nothing is the failure #517 calls "worse than no menu" — and is already contained,
    so a caller may put it straight into a line of its own output.
    """

    ok: bool
    message: str


def workspaces() -> list[str]:
    """Every workspace a switcher may offer, name-checked on the way out.

    `workspace.list_workspaces` reads directory names off the plane, so the check is the
    same floor `state.frame_workspace` applies to its own read: these names go on to a
    `workspace_dir()` join and onto a palette row, and #442 is what an unchecked one in that
    position already cost. `config.DEFAULT_WORKSPACE` is folded in whether or not its
    directory exists yet, matching `commands_workspace.cmd_workspace_use` — it is
    documented as the always-present workspace and `workspace.ensure` makes it on demand,
    and a fresh plane that has never made one would otherwise offer an empty list.
    """
    from .. import config, workspace as ws_mod
    names = [n for n in ws_mod.list_workspaces() if ws_mod.valid_name(n)]
    if config.DEFAULT_WORKSPACE not in names:
        names.append(config.DEFAULT_WORKSPACE)
    return sorted(names)


def personas() -> list[str]:
    """Every persona a switcher may offer, name-checked on the way out.

    Same reasoning as :func:`workspaces`, one noun over — `persona.list_personas` globs
    the plane's `personas/` directory, and `persona.valid_name` is the rule
    `persona.dir_of`'s own join depends on. No fold-in here: there is no always-present
    persona, and "no personas" is a real and ordinary answer for a plane that has none.
    """
    from .. import persona as p_mod
    return sorted(n for n in p_mod.list_personas() if p_mod.valid_name(n))


#: How many names an "I do not have that one, here is what I have" message lists. A
#: refusal is drawn on a status line — one row, and tmux truncates what does not fit
#: without saying it did — so the message is kept short BY CONSTRUCTION rather than left
#: to be cut off mid-name. Rendered against a 100-column client and read back: the
#: longest message this file can produce fits.
_SOME = 5


def _some(names: list[str]) -> str:
    """*names*, contained, capped at :data:`_SOME`, saying how many were left out."""
    shown = [contain.one_line(n) for n in names[:_SOME]]
    if len(names) > len(shown):
        shown.append(f"…{len(names) - len(shown)} more")
    return ", ".join(shown) if shown else "none"


def _pin(fid: str, name: str) -> str | None:
    """The value *fid* was LAUNCHED with for environment variable *name*, if it is a
    usable one — the rung nothing this module writes can outrank.

    Read from `state.identity`, which the launcher wrote, never from `os.environ`: this
    runs as a `run-shell` child of charter's private tmux server, which is shared between
    every frame on the machine, so this process's own `$CHARTER_WORKSPACE` may be another
    frame's (`state.record_identity` measures exactly that). Empty is what
    `_frame_identity_env` emits for a name the launch did not have, and empty is what
    every charter reader already treats as absent — so an empty value is not a pin.
    """
    val = state.identity(fid).get(name, "").strip()
    return val or None


def to_workspace(fid: str, name: str) -> Outcome:
    """Move frame *fid* to workspace *name*, or say why it did not.

    Refusals, in the order they are checked, and each of them is a thing the operator can
    act on:

    * **not a workspace name** — `workspace.valid_name`, the one rule
      (`instance.workspace_name_ok`). A name off a palette charter built cannot fail this;
      one typed at `charter frame-switch` can.
    * **no such workspace** — an unknown name is a question, never an implicit create
      (see the module docstring). The existing names go in the message, which is the same
      shape `charter workspace use` already answers an unknown name with.
    * **pinned** — `$CHARTER_WORKSPACE` was set at launch, so it is in every panel pane's
      environment and outranks anything written here. See the module docstring.

    On success: the per-session pointer under the frame's id
    (`workspace.set_active(..., session_id=fid)`, rung 1 of `state.workspace_for`), the
    frame's recorded workspace (`state.record_workspace`, rung 2 — so a panel respawned
    later, and `commands_frame._relayout_pane_env`, agree with the panels that are up),
    then the gather cache, then the bump.

    **The cache is refreshed before the bump, not after** — `frame/notify.py` settles the
    same ordering for the same reason: a panel's poll loop reads the version first and the
    cache second, so bumping first opens a window where a panel repaints the new
    workspace's name over the old workspace's repo table. This is a keypress, not a tick,
    so it does not touch the idle-cost property
    (`test_an_idle_tick_costs_exactly_one_stat_and_nothing_else`) — nothing here runs per
    repaint.

    `state.identity` is deliberately NOT rewritten. It records what the LAUNCH put on
    tmux's `-e`, and `_relayout_pane_env` replays it onto panes split later; an empty
    `CHARTER_WORKSPACE` there means "resolve locally", which lands on the pointer this
    just wrote. Writing the new name into it would pin the frame — taking `charter
    workspace use` away from the agent inside it, which `state.record_workspace`'s own
    docstring rules out in as many words.
    """
    from .. import workspace as ws_mod
    from . import gather
    shown = contain.one_line(name)
    if not ws_mod.valid_name(name):
        return Outcome(False, f"'{shown}' cannot name a workspace")
    known = workspaces()
    if name not in known:
        return Outcome(False, f"no workspace '{shown}' — have: {_some(known)}")
    pinned = _pin(fid, "CHARTER_WORKSPACE")
    if pinned:
        return Outcome(False, "cannot switch: $CHARTER_WORKSPACE pins this frame to "
                              f"'{contain.one_line(pinned)}'")
    # `force=True`: see the module docstring. The lock is reported whether or not it was
    # this switcher's own earlier write that took it, because the operator cannot tell
    # those apart from the outside and the one that matters — an agent's `charter
    # workspace use` mid-task — looks identical.
    was = ws_mod.is_locked(fid)
    ws_mod.set_active(name, session_id=fid, force=True, terminal_id="")
    state.record_workspace(fid, name)
    gather.refresh(fid, workspace=name)
    state.bump(fid)
    if was and was != name:
        return Outcome(True, f"workspace → {shown}  "
                             f"(lock moved from '{contain.one_line(was)}')")
    return Outcome(True, f"workspace → {shown}")


def to_persona(fid: str, name: str) -> Outcome:
    """Adopt persona *name* in frame *fid*, or say why it did not.

    The same three refusals as :func:`to_workspace`, one noun over — `persona.valid_name`,
    an unknown name, and a `$CHARTER_PERSONA` pin carried from the launch. There is no
    lock: personas have never had one (`persona.set_active` writes and returns a reach),
    so nothing is forced and nothing is overridden.

    `session_id=fid` is stated rather than inherited, for `_pin`'s reason: this runs as a
    child of a tmux server shared between frames, and `session.current()` would answer
    from an environment that is not reliably this frame's. It is what makes every panel
    see the change — `statusline._persona_chip_cells` asks `persona.resolve_active`, whose
    session rung is keyed on `$CHARTER_SESSION_ID`, which inside a frame is the frame's id
    (ADR 0019).

    No gather refresh: the gather cache holds repos, and a persona does not change which
    repos a workspace has. The bump alone is what repaints.
    """
    from .. import persona as p_mod
    shown = contain.one_line(name)
    if not p_mod.valid_name(name):
        return Outcome(False, f"'{shown}' cannot name a persona")
    known = personas()
    if name not in known:
        return Outcome(False, f"no persona '{shown}' — have: {_some(known)}")
    pinned = _pin(fid, "CHARTER_PERSONA")
    if pinned:
        return Outcome(False, "cannot switch: $CHARTER_PERSONA pins this frame to "
                              f"'{contain.one_line(pinned)}'")
    p_mod.set_active(name, session_id=fid, terminal_id="")
    state.bump(fid)
    return Outcome(True, f"persona → {shown}")


def changes(fid: str) -> list[str]:
    """Every cross-repo change frame *fid*'s workspace holds, name-checked on the way out.

    :func:`workspaces`' reasoning, one noun over, and the check matters as much: a slug
    goes on to a `changes/<slug>.json` join and onto a palette row, which is the position
    #442 already cost this project once. `change.all_for` has read every one of these
    through `change.read`, which asks `instance.change_name_ok` — so this is the same
    floor asked where the list is built rather than a second rule.

    **Keyed on the FRAME's workspace, not this process's** (#512). A palette is a
    `run-shell` child of a tmux server shared between frames, and resolving locally would
    list another plane's changes on this frame's screen. `state.workspace_for` is the one
    rule every frame surface asks.

    A record charter could not read contributes no row and takes nothing down with it —
    `change.all_for` reports those separately, and `doctor` is where they are named.
    """
    from .. import change as change_mod
    try:
        records, _refused = change_mod.all_for(state.workspace_for(fid))
    except Exception:
        return []
    return sorted(r["change"] for r in records)


def current_change(fid: str) -> str | None:
    """The change this frame is looking at, or ``None``.

    `state.frame_change` and nothing else — there is no environment pin for a change and
    no committed default, so unlike a workspace or a persona this has exactly one rung.
    That is stated rather than left implicit because `choose.pin_reason` branches on it:
    a change picker can never be refused by a launch pin, and the reason its doorway
    carries is a different one.
    """
    return state.frame_change(fid)


def to_change(fid: str, name: str) -> Outcome:
    """Point frame *fid* at change *name*, or say why it did not.

    **This moves what the frame is LOOKING at, not what it is.** A workspace switch moves
    the plane every panel reads and a persona switch moves who is reading; this moves one
    panel's subject. So there is no `set_active`, no lock, no identity to rewrite and no
    gather refresh — the change rows are already in the snapshot every panel shares, and
    re-gathering to choose which of them to draw would be a second reading of a plane
    that has not moved.

    Two refusals, and each is a thing the operator can act on: a slug that cannot name a
    change (`instance.change_name_ok`, the one rule), and a change this workspace does not
    have. An unknown name is a question, never an implicit create — `to_workspace`'s rule,
    and the existing names go in the message for its reason.
    """
    from .. import instance
    shown = contain.one_line(name)
    if not instance.change_name_ok(name):
        return Outcome(False, f"'{shown}' cannot name a change")
    known = changes(fid)
    if name not in known:
        return Outcome(False, f"no change '{shown}' — have: {_some(known)}")
    state.record_change(fid, name)
    state.bump(fid)
    return Outcome(True, f"change → {shown}")


def current_workspace(fid: str) -> str:
    """What the frame is drawing right now — `state.workspace_for`, named here so the
    palette and the switcher cannot come to disagree about which name gets the mark."""
    return state.workspace_for(fid)


def current_persona(fid: str) -> str | None:
    """The persona the frame is drawing right now, or ``None``.

    Asked with the frame's id rather than from the ambient environment, for `_pin`'s
    reason — and through the frame's own recorded pin first, so a palette built by a
    `run-shell` child marks the row the PANELS are showing rather than the row this
    process would resolve for itself off a shared server's environment. `resolve_active`
    is deliberately not called: its top rung is `$CHARTER_PERSONA` out of *this* process,
    which is exactly the value that belongs to another frame.

    The rungs are the ones a PANEL reaches, in a panel's order: the pin the launch carried
    on `-e`, the per-session pointer under the frame's id, then the plane-wide defaults.
    `config.ACTIVE_PERSONA_FILE` is skipped because `persona.set_active` writes it only
    when there is neither a session id nor a pane id — inside a frame there is always a
    session id (the frame's), so it is a rung a framed plane does not have.
    """
    from .. import persona as p_mod
    pinned = _pin(fid, "CHARTER_PERSONA")
    if pinned:
        return pinned if p_mod.valid_name(pinned) else None
    val = p_mod.for_session(fid) or p_mod.declared_default() or p_mod.default_persona()
    return val if val and p_mod.valid_name(val) else None
