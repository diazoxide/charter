"""The ended step: a harness exited, and its tab offers a choice instead of closing.

**Every harness exit used to be final.** `pane-died[1]` was `kill-window`, so `/exit`,
Ctrl-D, a double Ctrl+C or a crash took the chat's window and `state.reap` took its
directory. Decision 4 ruled otherwise: no harness exit destroys a chat. This module is what
happens instead, and ADR 0018's 2026-09-15 amendment is the boundary it keeps.

**Two presentations, chosen from the exit code and nothing else.**

* **A clean exit** (`CLEAN`) respawns the proven pane into the profile selector, with the
  chat's own resume row on it. The pane is charter's again, which is precisely what the
  amendment's present-tense question answers.
* **Anything else** — a crash, a signal, or the empty status tmux reports for a signal death
  (`commands_frame._UNKNOWN_DEATH_CODE`) — leaves the dead pane exactly as tmux keeps it, so
  the harness's last lines stay on screen, and opens the same choice in a drawer split
  beneath it. Charter does not draw in the dead pane at all.

**Nothing here acts on a record alone** (the #933 and #1103 rule, and the spec's *What proves
a pane before charter acts on it*). A record says where to LOOK; :func:`proof` is one
`list-panes` whose answer says what may be touched, and every respawn, split and kill in this
module is aimed at a pane id that listing reported under this chat and this plane. A respawn
never passes `-k`, so tmux itself refuses a pane whose harness is still running.

**And one exit is presented once.** The `pane-died` hook and `_launch`'s own late check both
answer a harness that died while its chat was being laid out. `state.claim_ended` is an
``O_EXCL`` create, so the loser does nothing at all rather than drawing a second surface over
the first.
"""

from __future__ import annotations

import os
from typing import NamedTuple

from .. import config, util
from . import chats, overlay, palette, state, tmuxctl

#: The exit code that means the operator ended the harness on purpose. Everything else —
#: a crash, a signal, an unreadable status — gets the drawer, because the pane holds
#: something worth reading.
CLEAN = 0

#: The drawer's three rows, by id. The colon keeps them out of `frame/action.py`'s alphabet
#: (`component.usable_id`), so no provider can ship an action that takes one of these
#: keypresses — `frame/leave.py` and `frame/tabmenu.py` use the same trick for the same
#: reason. None of them is ever drawn.
RESUME_ID, FRESH_ID, CLOSE_ID = "ended:resume", "ended:fresh", "ended:close"

#: The pane option that marks a drawer as THIS chat's, written the moment tmux reports the
#: pane id. What makes `drop_drawer` able to kill a drawer and nothing else.
DRAWER_OPTION = "@charter_drawer"

#: How tall the drawer is. Three rows, a heading and a footer — the confirmation drawer's
#: own size (#921, #927), so the two surfaces a chat can grow are the same shape.
DRAWER_ROWS = 6

#: The one listing every write in this module is aimed by. Five fields, tab-separated: the
#: pane, whether it is dead, and the three markers that say whose it is.
PROOF_FORMAT = ("#{pane_id}\t#{pane_dead}\t#{@charter_chat}\t#{@charter_plane}"
                "\t" + "#{" + DRAWER_OPTION + "}")

#: How many fields a row of :data:`PROOF_FORMAT` has. A row of any other width is dropped
#: rather than unpacked: a window NAME may contain a tab and `list-panes` does not quote it,
#: so a row charter cannot assign is one it must not act on (`tmuxctl.live_pane_by_pid`
#: drops the same shape for the same reason).
_PROOF_FIELDS = 5


class Proof(NamedTuple):
    """What ONE listing proves about a chat's panes, and the whole of what may be touched.

    :attr:`harness` is ``""`` whenever the listing does not put the chat's recorded pane
    under this chat AND this plane, alive or dead — which covers another plane's pane
    carrying the same id, a pane with no plane marker at all, and a record naming a pane the
    listing puts under a different chat. Each of those is a real state, and each would be a
    respawn or a kill aimed at somebody else's window.
    """

    #: The proven harness pane id, or ``""`` when the listing proved none.
    harness: str
    #: Whether that pane is dead. A LIVE pane is proven and still must not be respawned —
    #: `layout.respawn_argv(kill=False)` leaves tmux to refuse it, and this says so before
    #: charter even asks.
    harness_dead: bool
    #: Every pane the listing proves is a drawer of this chat, on this plane.
    drawers: tuple[str, ...]


def proof(fid: str, *, socket: str) -> Proof | None:
    """One `list-panes -a` on *socket*, read as what may be acted on. ``None`` to refuse.

    **``None`` is a server that did not answer, and it proves nothing** (#1100, and the
    second spec review's ruling): a timeout or any other failure is not "this chat has no
    panes", and treating it as one would reap, respawn or kill on a reading charter never
    got. Only a listing that came back rc 0 answers a :class:`Proof`.

    One call, not three, because the three questions are about the same listing: a proof
    assembled from two round trips could describe a pane that died between them.
    """
    from .. import commands_frame

    out = tmuxctl.run("asking tmux what this chat's panes are",
                      tmuxctl.server_argv(socket, "list-panes", "-a", "-F", PROOF_FORMAT),
                      report=False)
    if out.returncode != 0:
        return None
    # **Planes are compared by RESOLVED path, because two spellings of one plane are not
    # the same string.** That is #812's finding one option over, and it was measured here
    # rather than argued: the marker is written by the LAUNCH and read back in the hook's
    # own child process, and the two do not derive the plane the same way — `root.find_root`
    # calls `resolve()` on `$CHARTER_ROOT`, while a plane pointed at directly keeps the
    # spelling it was given. On macOS `/var` is a symlink to `/private/var`, so the same
    # directory reached the two sides as `/var/…/.charter` and `/private/var/…/.charter`,
    # every row failed the plane test, and the ended step declined for EVERY exit — silently,
    # because it is best-effort, leaving the tab dead and offering nothing.
    #
    # `realpath` and not `resolve()`: this must never raise on a plane that has been removed
    # under a running frame, and it normalises a path that no longer exists rather than
    # refusing it.
    me = os.path.realpath(commands_frame._this_plane())
    recorded = chats.pane_of(fid)
    harness, dead, drawers = "", False, []
    for line in out.stdout.splitlines():
        fields = line.split("\t")
        if len(fields) != _PROOF_FIELDS:
            continue
        pane, is_dead, chat, plane, drawer_of = fields
        if chat != fid or not plane or os.path.realpath(plane) != me:
            # Not this chat's, or not this plane's, or a pane an older charter created and
            # never marked. A session charter did not mark is left alone (the spec's
            # *Limits*): no presentation, no drawer, no respawn.
            continue
        if drawer_of == fid:
            drawers.append(pane)
        elif not drawer_of and pane == recorded:
            harness, dead = pane, is_dead == "1"
    return Proof(harness=harness, harness_dead=dead, drawers=tuple(drawers))


def present(fid: str, *, socket: str) -> str:
    """Offer the choice for *fid*'s ended harness. ``"selector"``, ``"drawer"`` or ``""``.

    ``""`` is *nothing was offered*, and it is the answer for every reason there could be
    one: a chat that runs no profile (`charter frame -- <cmd>`, the escape hatch — ruled to
    keep today's ending), a chat whose frame was never drawn (#384's early death), a chat the
    operator already closed, a pane no listing proves, a pane that is still alive, a server
    that would not answer, and an exit another process has already presented.

    **Never raises.** The `pane-died` hook runs this, and a hook never breaks a turn — so the
    whole body is wrapped and a failure is simply an offer not made, which leaves the tab
    exactly as tmux left it.
    """
    from .. import commands_frame
    from . import launcher, layout

    try:
        if not chats.ID_RE.fullmatch(fid or ""):
            return ""
        profile = state.profile(fid)
        # A chat with no profile is the escape hatch, whose window closes at exit as it
        # always did (the ruling on open question 5). Not drawn yet is #384's early death,
        # which `_launch` reports and closes. Closed is the operator having ended it.
        if not profile or not state.was_drawn(fid) or state.was_closed(fid):
            return ""
        pr = proof(fid, socket=socket)
        if pr is None or not pr.harness or not pr.harness_dead:
            return ""
        # **The claim, before any write.** Whoever gets it presents; whoever does not does
        # nothing at all. This is what keeps the hook and `_launch`'s late check from
        # drawing two surfaces over one exit.
        if not state.claim_ended(fid):
            return ""
        state.bump(fid)
        commands_frame._repaint_the_other_strips(fid)

        ident = state.identity(fid)
        env = (commands_frame._guest_harness_env(ident)
               if tmuxctl.is_operator_socket(socket)
               else commands_frame._frame_identity_env(ident))
        if state.exit_code(fid) == CLEAN:
            # The pane is charter's again: put the selector back in it, with this chat's own
            # resume row. `kill=False`, so tmux refuses if the listing was already stale.
            tmuxctl.run(
                "putting the selector back in the pane a harness left",
                layout.respawn_argv(socket=socket, harness_pane=pr.harness, kill=False,
                                    env=env, cwd=state.chat_cwd(fid) or str(config.ROOT),
                                    harness_argv=launcher.argv_select(profile, ended=True)),
                report=False)
            return "selector"
        _open_drawer(fid, socket=socket, harness=pr.harness)
        return "drawer"
    except Exception:  # noqa: BLE001 - a hook never breaks a turn
        return ""


def _open_drawer(fid: str, *, socket: str, harness: str) -> None:
    """Split the crash drawer beneath *harness* and mark it as this chat's.

    The dead pane is not touched at all: tmux is keeping the harness's last lines on it
    (`remain-on-exit`), and those lines are the whole reason a crash gets a drawer rather
    than a selector drawn over them.

    The marker is written the moment tmux reports the pane id, because it is what
    :func:`drop_drawer` proves before it kills anything. A drawer whose marker never landed
    still goes: its own process closes its pane by `#{pane_pid}` on the way out (:func:`draw`).
    """
    out = tmuxctl.run(
        "opening the drawer that offers the choice",
        tmuxctl.server_argv(socket, "split-window", "-v", "-l", str(DRAWER_ROWS),
                            "-t", harness, "-P", "-F", "#{pane_id}",
                            "-e", f"CHARTER_SESSION_ID={fid}", "--",
                            *util.self_relaunch_argv("frame-palette", "--ended",
                                                     "--chat", fid)),
        report=False)
    pane = (out.stdout or "").strip()
    if out.returncode != 0 or not tmuxctl.PANE_ID_RE.fullmatch(pane):
        return
    tmuxctl.run("marking the drawer as this chat's",
                tmuxctl.server_argv(socket, "set-option", "-p", "-t", pane,
                                    DRAWER_OPTION, fid),
                report=False)
    state.record_drawer(fid, pane)
    tmuxctl.run("putting the keyboard in the drawer",
                tmuxctl.server_argv(socket, "select-pane", "-t", pane), report=False)


def reset(fid: str) -> bool:
    """Clear everything the ended state holds, because a harness is starting again.

    **The one function every start calls**, from `launcher.attempt`'s `on_exec` — a selector
    pick, the drawer's resume, a fresh start, a reopen, and every ordinary launch. Ruling 1
    of this task: one function clears the ended state and every start path calls it, rather
    than four places each remembering three files.

    Answers whether it cleared an `ended` CLAIM, which is what lets `attempt`'s undo put the
    claim back when the `exec` then raised — and only then. A start that was never ended must
    not come back claimed, or its tab would close without asking.
    """
    was = state.is_ended(fid)
    state.clear_ended(fid)
    state.clear_exit(fid)
    drop_drawer(fid)
    return was


def drop_drawer(fid: str) -> None:
    """Kill *fid*'s drawer pane, if one listing proves it is one — then forget the record.

    **The recorded pane is never itself the target.** The record says where to look; the
    listing says what to kill. A pane that has been reaped and its id handed to somebody
    else's window is exactly the shape this refuses, and it is the #933 shape that once
    closed an operator's session.

    Best effort throughout: a server that will not answer leaves the drawer standing, which
    is a pane the operator can close, where a kill aimed at an unproven id is not recoverable.
    """
    if not state.drawer(fid):
        # **No record, so nothing to look for — and this is the guard that keeps the cost
        # honest.** A clean exit opens no drawer at all, and close and every harness start
        # both come through here, so without this the ordinary case pays a `list-panes` to
        # be told there is nothing to kill. The record is charter's OWN write rather than a
        # reading it has to trust, which is what makes it safe to ask first: it says only
        # whether to look, and `proof` below still decides what may be touched.
        #
        # A drawer whose record never landed is not stranded by this: its own process closes
        # its pane on the way out, proven by `#{pane_pid}` (:func:`draw`).
        return
    socket = state.frame_server(fid)
    if not socket:
        state.record_drawer(fid, "")
        return
    pr = proof(fid, socket=socket)
    if pr is None:
        # The server proved nothing, so nothing is killed AND nothing is forgotten: the
        # record is the only way back to this pane once the server answers again.
        return
    for pane in pr.drawers:
        tmuxctl.run("closing the drawer this chat was offered",
                    tmuxctl.server_argv(socket, "kill-pane", "-t", pane), report=False)
    state.record_drawer(fid, "")


def drawer_rows(fid: str) -> tuple[overlay.Row, ...]:
    """The three rows decision 4 names, in its order: resume, start fresh, close tab.

    Resume is present only when the conversation actually exists — `launcher.resume_row`
    asks `leave.conversation_exists` at the moment of offering (#1101) — so a chat nobody has
    typed in gets two rows, which is decision 4's *no conversation yet* case exactly.
    """
    from . import launcher

    rows: list[overlay.Row] = []
    resume = launcher.resume_row(fid)
    if resume is not None:
        rows.append(overlay.Row(id=RESUME_ID, title=resume.title, note=resume.note))
    rows.append(overlay.Row(id=FRESH_ID, title="start fresh",
                            note="pick a profile in this tab"))
    rows.append(overlay.Row(id=CLOSE_ID, title="close this tab",
                            note="stop this chat and do not bring it back"))
    return tuple(rows)


def choose(row, fid: str) -> None:
    """Act on the drawer row Enter landed on.

    **Each of the first two acts on a FRESH proof**, not on the one the drawer was built
    from: a drawer can sit on screen for hours, and the pane it is about may have been reaped
    or respawned in between. Only a pane this listing proves dead is respawned, and the
    respawn carries no `-k`, so tmux refuses even that if the reading has just gone stale.

    **The drawer never starts a profile of its own.** *start fresh* respawns into the
    SELECTOR, which is decision 4's *back to the profile selector* — the drawer is a choice
    about this tab, not a second place where a harness may be picked.
    """
    from . import builtin_actions, launcher, layout

    if row is None:
        return
    if row.id == CLOSE_ID:
        # Through `frame-close`, so closing from here is the same teardown as every other
        # route to it — the mark, the transcript, the manifest entry and the window.
        # `_spawn` because this pane is about to stop existing (`draw`'s `finally`).
        builtin_actions._spawn(util.self_relaunch_argv("frame-close", fid, "--chat", fid),
                               fid=fid)
        return
    socket = state.frame_server(fid)
    profile = state.profile(fid)
    if not socket or not profile:
        return
    pr = proof(fid, socket=socket)
    if pr is None or not pr.harness or not pr.harness_dead:
        return
    if row.id == RESUME_ID:
        argv = launcher.argv(profile, [], attended=True, resume=True)
    elif row.id == FRESH_ID:
        argv = launcher.argv_select(profile, ended=True, fresh=True)
    else:
        return
    ident = state.identity(fid)
    from .. import commands_frame
    env = (commands_frame._guest_harness_env(ident)
           if tmuxctl.is_operator_socket(socket)
           else commands_frame._frame_identity_env(ident))
    tmuxctl.run(
        "starting what the operator chose in the pane their harness left",
        # The chat's own directory: for a harness that resumes by looking the id up in the
        # working directory (opencode — reading O2), starting anywhere else is a resume that
        # finds nothing.
        layout.respawn_argv(socket=socket, harness_pane=pr.harness, kill=False, env=env,
                            cwd=state.chat_cwd(fid) or str(config.ROOT), harness_argv=argv),
        report=False)


def draw(args) -> int:
    """Be the crash drawer: draw the rows, take a choice, act on it, close this pane.

    `tabmenu.draw`'s shape and deliberately its shape — a `palette.Palette` over a different
    row source, `own_the_tty`, then act — so everything the overlay already decided holds
    here unchanged.

    **Ctrl+C does nothing here** (`cancel_keys`). A double Ctrl+C is how Claude Code exits,
    so the press that follows it lands on whatever replaced the harness; on this surface it
    must not be read as *close this tab*. Esc and end of input choose nothing either: the row
    is ``None``, and the tab stays exactly as it was.

    **The pane closes itself by pid**, which is `launcher._close_the_cancelled_chat`'s proof
    and not a record: a drawer whose `@charter_drawer` never landed still goes, and a pane
    this process is not the first process of is never touched.

    **Always 0**, for `cmd_palette`'s reason: a non-zero return from a `run-shell` child is
    printed into the harness pane and drops it into copy-mode, which is charter drawing in
    the one rectangle ADR 0018 says it never draws.
    """
    from . import tabmenu

    fid, socket, harness, own_pane = tabmenu.handback(os.environ)
    try:
        surface = palette.Palette(catalogue=drawer_rows(fid),
                                  label=f"chat {fid} ended", mouse=True,
                                  cancel_keys=("escape",))
        choose(palette.own_the_tty(surface), fid)
    except Exception:  # noqa: BLE001 - the pane must still be handed back
        pass
    finally:
        _close_this_pane(socket, fid=fid, harness=harness, own_pane=own_pane)
    return 0


def _close_this_pane(socket: str, *, fid: str, harness: str, own_pane: str) -> None:
    """Close the drawer's own pane and put the keyboard back on the harness pane.

    Proven by `#{pane_pid}` rather than by `$TMUX_PANE` or by the record: a chat's own shell
    inherits `$TMUX_PANE` from the pane it runs in, so neither names THIS process's pane.
    That is the proof `launcher._close_the_cancelled_chat` stands on, and the one an empty
    `kill-pane -t ''` — which kills the ACTIVE pane, measured on 3.7c — makes necessary.
    """
    row = tmuxctl.live_pane_by_pid(socket, os.getpid())
    pane = row.pane if row is not None else ""
    if pane:
        state.record_drawer(fid, "")
        tmuxctl.run("closing the drawer", tmuxctl.server_argv(socket, "kill-pane",
                                                              "-t", pane), report=False)
    if harness:
        tmuxctl.run("putting the keyboard back on the chat's pane",
                    tmuxctl.server_argv(socket, "select-pane", "-t", harness),
                    report=False)


def cmd_frame_ended(args) -> int:
    """`charter frame-ended --chat <id>` — what the `pane-died[1]` hook runs.

    **Always 0, and never raises.** This runs as a `run-shell -b` child of the tmux server;
    a non-zero return is printed into the pane tmux fired the hook for, which is the one
    rectangle ADR 0018 says charter never draws in.

    The chat is held to `chats.ID_RE` before anything is asked of tmux, exactly as
    `commands_frame.cmd_close` holds its own positional: the value is expanded by tmux out of
    `#{@charter_chat}` into a shell-quoted `run-shell` string, and a value charter cannot
    name a chat from is one it must not look up a server for.
    """
    fid = (getattr(args, "chat", None) or "").strip()
    if not chats.ID_RE.fullmatch(fid):
        return 0
    socket = state.frame_server(fid)
    if socket:
        present(fid, socket=socket)
    return 0
