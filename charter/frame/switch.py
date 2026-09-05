"""Moving a running frame to another workspace or persona — #517, and the half of #518
that happens once a name has been chosen.

One mechanism, four steps, and every surface that switches goes through it: **list the
names, contain them, perform the switch, repaint the panels.** The palette
(`frame/builtin_actions._register_names` → `cmd_switch`) and the launch picker
(`frame/picker.py` → `cmd_launch`) are two front doors onto the same rooms.

**A WORKSPACE is still not one of the things a switch may MOVE, and that is the
correction this file carries.** Spec §4j settled it on 2026-08-26 — *"a chat belongs to
its workspace for life; `{workspace}-{hash}` is identity, not a property"* — and Phase
5's §3.6 and §8 both reaffirm it. This module was written on 2026-08-25, one day earlier,
when a frame **was** a workspace one-to-one and moving it was coherent. Phase 5 then made
a frame a **chat**, nobody re-read the sentence, and "moves the frame" quietly became
"moves the chat"; #733 and #788 are the two directions of the strand that opened, and
#789 closed it by refusing.

**§4b then names the operation that refusal was standing in for, and it is a different
one.** *"Switching workspace means keep the opened chat open in the background, so a user
can run many harnesses in one charter environment"* — the operator's own words. That is
`switch-client`: **the client moves and nothing else does.** The chat left behind keeps
its harness, its pid, its window and its workspace; so does every chat in the workspace
arrived at. :func:`to_workspace` is therefore a CHECK — the refusals that need no server
— and `commands_frame._switch_client` is the tmux half, exactly as `chats.check` and
`commands_frame.cmd_chat` already split one noun down.

**A switch is a write to the frame's OWN identity, never a pointer somebody might read.**
#411 is the whole reason this file is not four lines: panels followed `charter ws use`
only through the `$CHARTER_SESSION_ID` collision, and #412 made that identity explicit on
tmux's `-e`. What a switch may write is a persona — who is reading — and a change — what
one panel is looking at. Neither is a plane the harness's own cwd, files and history are
about, which is the whole of §4j's argument and the whole of why the workspace is the one
noun this file writes nothing for: a workspace switch has no file to write, because what
it moves is a tmux client.

**A pin is a refusal and not a best effort**, which is now the persona's rule alone. A
panel pane was started with `-e CHARTER_PERSONA=<pin>` (`commands_frame._frame_identity_env`);
that value is in the pane's process environment for as long as the pane lives, and no file
charter writes can take it out. Reporting "switched" and drawing the old persona is the
failure mode #411 was filed for. Reporting "this frame is pinned" is the honest one.

**And nothing here writes a TERMINAL pointer**, which is #411 arriving on this command.
`persona.set_active` normally writes one, keyed on
`$TERM_SESSION_ID`/`$TMUX_PANE`/`$STY`/`$SSH_TTY` — and in a `run-shell` child of charter's
private tmux server those names belong to whichever launcher STARTED that server, which is
shared between every frame on the machine and may be days old. A switch inside frame B
would have moved the persona of the terminal that launched frame A. The call below
passes `terminal_id=""`: this process genuinely has no terminal to speak for.

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


def workspaces(fid: str | None = None) -> list[str]:
    """Every workspace a switcher may offer, name-checked on the way out and — asked about
    a FRAME — **ordered by last use, once, and then held still for that frame's life**
    (#903).

    `workspace.list_workspaces` reads directory names off the plane, so the check is the
    same floor `state.frame_workspace` applies to its own read: these names go on to a
    `workspace_dir()` join and onto a palette row, and #442 is what an unchecked one in that
    position already cost. `config.DEFAULT_WORKSPACE` is folded in whether or not its
    directory exists yet, matching `commands_workspace.cmd_workspace_use` — it is
    documented as the always-present workspace and `workspace.ensure` makes it on demand,
    and a fresh plane that has never made one would otherwise offer an empty list.

    **The order is not this function's, and it is the same split :func:`personas` already
    makes** (#882). `chats.touched_by_workspace` owns the recency and `state.tab_order`
    owns the holding-still; what happens here is that the first process to ask about a
    frame writes the answer down and every process afterwards reads it. So the strip, the
    palette and the launcher's own sizing pass (`slots.bar_rows_wanted`) cannot draw the
    same roster in two orders, and a switch — which tears every panel down and re-splits it
    — redraws the identical columns.

    *"active last used tabs should be in first order, then olds."* A switch updates the
    mtimes this reads for the NEXT frame; it moves nothing now. `state.record_tab_order`
    carries the argument about why that is the whole of the feature and why live reordering
    was refused twice before it.

    **The recorded order is an ORDER and not a roster**, which is what keeps a stale line
    from resurrecting a deleted workspace: the names on the strip are always
    `list_workspaces`' answer, and the record only says how to sort them. A workspace
    created since goes on the end, in name order, where it cannot move a column an operator
    is already aiming at; a workspace deleted since simply is not there.

    **No frame is the plain alphabetical answer**, and `""` is no frame exactly as `None`
    is — `choose.names_of` defaults its *fid* to the empty string and the launch picker
    (`commands_frame._choose_workspace`) runs before any frame exists. Both want membership
    and a list to read, neither is about which order a strip draws, and a falsy id would
    otherwise reach `state.tab_order`, be answered "nothing recorded" by
    `state.frame_dir`'s own refusal, and recompute the recency on every call — live
    reordering, arriving through the one caller that has no frame to hold an order for.
    One test, not two, for `if fid is None` and `if not fid` cannot both be observable.

    Written as a default rather than a second function, so there is one place that decides
    what the set of workspaces IS.
    """
    from .. import config, workspace as ws_mod
    names = [n for n in ws_mod.list_workspaces() if ws_mod.valid_name(n)]
    if config.DEFAULT_WORKSPACE not in names:
        names.append(config.DEFAULT_WORKSPACE)
    if not fid:
        return sorted(names)
    return _by_use(fid, names)


def _by_use(fid: str, names: list[str]) -> list[str]:
    """*names* in the order frame *fid* draws them — the recorded one, computed and
    recorded here the first time it is asked for.

    **The ORDER *names* arrives in is not read, and that is a correction the deletion sweep
    forced.** The caller used to hand this `sorted(names)` and this docstring claimed the
    sortedness was "load-bearing twice over" — the fallback for a name charter has never
    seen used, and the tie-break for two workspaces written in the same clock tick. Both
    are false: each of those is done by the `n` in the sort KEY below, on a total order over
    distinct names, so the incoming order cannot reach them. `tools/sweep.py` reported the
    caller's `sorted` as a survivor and it was right to — every use of *names* here is
    either re-sorted, or turned into a set, or walked in `recorded`'s order.

    Every use but ONE, and that one is why the sort moved rather than went. The names this
    record does not carry are appended, and they are appended **in name order** — a
    workspace made while the frame is open goes on the end where it cannot move a column an
    operator is already aiming at, and two of them go on in an order that is a function of
    the plane rather than of `os.scandir`. That promise is kept HERE now, where it is made.
    It used to be kept by accident: `workspace.list_workspaces` happens to answer in name
    order, so the caller's sort was doing nothing the leftovers could not have got from the
    filesystem — until a record that omits a name (a hand-edited `tab_order`, a record
    written before `config.DEFAULT_WORKSPACE` was folded in) makes one of them a leftover
    and the accident stops holding.

    **Sorted by the timestamp DESCENDING, so the newest is leftmost**, which is where an
    operator looks first and where `slots._compose` starts its first page. Reversing the
    list after an ascending sort was the alternative and is not equivalent for the ties:
    negating the key keeps equal timestamps in name order, where reversing puts them in
    reverse name order — a difference no test of a two-workspace plane can see and one that
    would show on the day two directories share a second.

    The record is written even when it changes nothing (a plane with one workspace, a plane
    charter has no timestamp for at all), because what is being written down is not "this
    order is interesting" but "this frame has decided" — and a frame that re-decided on its
    next repaint would be the live reordering this exists to refuse.
    """
    from . import chats as chats_mod
    recorded = state.tab_order(fid)
    if not recorded:
        touched = chats_mod.touched_by_workspace()
        recorded = sorted(names, key=lambda n: (-touched.get(n, 0.0), n))
        state.record_tab_order(fid, recorded)
    known = set(names)
    kept = [n for n in recorded if n in known]
    seen = set(kept)
    return kept + sorted(n for n in names if n not in seen)


def personas() -> list[str]:
    """Every persona a switcher may offer, name-checked on the way out and **ordered by
    use**: the plane's declared default first, then most-dispatched first, ties broken by
    the larger memory count, then by name.

    Same name-check as :func:`workspaces`, one noun over — `persona.list_personas` globs
    the plane's `personas/` directory, and `persona.valid_name` is the rule
    `persona.dir_of`'s own join depends on. No fold-in here: there is no always-present
    persona, and "no personas" is a real and ordinary answer for a plane that has none.

    **The order is not this function's, and that is deliberate** (#882). `persona.by_use`
    owns it and states the whole argument — why dispatches rather than memories, why the
    default is pinned, why the memory count is read only where dispatch counts tie. The
    other switcher onto the same names is the frame's sidebar persona column
    (`statusline._persona_chip_cells`, clickable through `frame/builtins._persona_events`),
    which used to lift the ACTIVE persona to the top; both ask one function now, so a
    picker and the column beside it cannot draw the same roster in two orders.

    **The name-check is applied BEFORE the ordering, not after.** `by_use` reads a
    dispatch tally and, on a tie, a memory directory per name; handing it a name
    `valid_name` refuses would be spending those reads on a row no switcher may draw, and
    `persona.memory_dir`'s join is the very thing that check stands in front of (#442).
    """
    from .. import persona as p_mod
    return p_mod.by_use([n for n in p_mod.list_personas() if p_mod.valid_name(n)])


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
    every frame on the machine, so this process's own `$CHARTER_PERSONA` may be another
    frame's (`state.record_identity` measures exactly that). Empty is what
    `_frame_identity_env` emits for a name the launch did not have, and empty is what
    every charter reader already treats as absent — so an empty value is not a pin.
    """
    val = state.identity(fid).get(name, "").strip()
    return val or None


def to_workspace(fid: str, name: str) -> Outcome:
    """May chat *fid*'s client be moved to workspace *name*? — the whole of the decision
    that does not need a tmux server, and none of the decision that does.

    **This is a CHECK, in `chats.check`'s sense, and that is what §4b changed about it.**
    Until #789 the same call was "re-point this chat at that workspace", which §4j forbids
    for life; #789 made it an unconditional refusal, which was right about the chat and
    left the operator's `workspaces` bar a listing where every tab refused. §4b names the
    operation that was missing: **switching moves the CLIENT, not the chat.**
    `switch-client -c <client> -t <the other workspace's session>` puts this terminal on
    another workspace of this plane; the chat it left keeps its harness, its window, its
    pid and its workspace, and so does every chat in the workspace it arrived in. Nothing
    is re-pointed, so §4j is untouched — measured by
    `tests/test_a_chat_belongs_to_its_workspace_for_life.py`, which now asserts the
    invariant across a switch that SUCCEEDS.

    **Three refusals, and each is a thing the operator can act on**: a name that cannot
    name a workspace (`workspace.valid_name`, the one rule), a name this plane does not
    have — a question, never an implicit create (#518: "a picker that creates on a typo
    leaves litter") — and the workspace this frame is already in, which is not an error
    and is still not a switch. They are asked in that order for :func:`to_persona`'s
    reason: the narrower answer is the one the operator can do something with.

    **"Is it open" is deliberately not asked here, because it is tmux's — and since a tab
    OPENS a workspace it is no longer a refusal at all.** A name that passes the three
    checks above is a workspace this plane has, and `commands_frame._switch_client` either
    finds its session or starts one (`_open_workspace`, §4k). Either way the answer belongs
    to the instant the switch runs rather than the instant a palette opened —
    `chats.check`'s own split, one noun out — and the readings it needs, which of this
    plane's sessions is `ws` and whether its NAME is already taken on this shared machine
    by a plane that is not this one, are ones only a server can give.

    **`$CHARTER_WORKSPACE` is no longer a pin on this noun, and its absence is a
    decision.** The variable pins which workspace THIS CHAT is in
    (`state.own_workspace`'s first rung) and it still does; a switch that moves a client
    to another session does not touch it, does not contradict it, and leaves the pinned
    chat pinned and running. Refusing on it would be refusing to look at another workspace
    because this chat cannot leave the one it is in, which is two different nouns wearing
    one name — the confusion #789's own refusal was made of.

    Nothing is written and nothing is bumped: a switch's only effect is on the tmux
    client, and the panels that repaint are re-laid-out by the caller that moved it.
    """
    from .. import workspace as ws_mod
    shown = contain.one_line(name)
    if not ws_mod.valid_name(name):
        return Outcome(False, f"'{shown}' cannot name a workspace")
    known = workspaces()
    if name not in known:
        return Outcome(False, f"no workspace '{shown}' — have: {_some(known)}")
    if name == current_workspace(fid):
        return Outcome(False, f"already in workspace '{shown}'")
    return Outcome(True, f"workspace → {shown}")


def to_persona(fid: str, name: str) -> Outcome:
    """Adopt persona *name* in frame *fid*, or say why it did not.

    Three refusals, each a thing the operator can act on: a name that cannot name a
    persona (`persona.valid_name`, the one rule), an unknown name — a question, never an
    implicit create, with the existing names beside it — and a `$CHARTER_PERSONA` pin
    carried from the launch, which is in every panel pane's environment and outranks
    anything written here. There is no lock: personas have never had one
    (`persona.set_active` writes and returns a reach), so nothing is forced and nothing is
    overridden.

    **This noun stays where :func:`to_workspace` leaves, and §4j is why.** A persona is
    who is reading the plane, not which plane it is: adopting another one changes no
    cwd, no file and no history, so nothing about the harness's own context stops being
    true. That is the whole of the argument that removes the workspace and keeps this.

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

    **This moves what the frame is LOOKING at, not what it is.** A persona switch moves
    who is reading the plane; this moves one panel's subject. So there is no `set_active`,
    no lock, no identity to rewrite and no gather refresh — the change rows are already in
    the snapshot every panel shares, and re-gathering to choose which of them to draw
    would be a second reading of a plane that has not moved. A change belongs to one
    workspace, so pointing at one crosses no plane and §4j has nothing to say about it.

    Two refusals, and each is a thing the operator can act on: a slug that cannot name a
    change (`instance.change_name_ok`, the one rule), and a change this workspace does not
    have. An unknown name is a question, never an implicit create — #518's rule ("a picker
    that creates on a typo leaves litter"), and the existing names go in the message so
    the refusal answers the question the typo was asking.
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
