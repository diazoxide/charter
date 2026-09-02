"""§4b: switching workspace moves the CLIENT, kills nothing, and never leaves this plane.

The operator's own requirement, which is the whole specification:

> switching workspace — it means keep opened chat opened in background, so user can
> simultaneously run many harnesses in one charter environment. Changing workspace or
> changing sessions does not mean stopping old chat session. User can manually close or
> stop session.

**Three things had to be true and each has its own class here.**

* **The switch happens at all.** #789 made `switch.to_workspace` an unconditional refusal
  — correct about §4j, and it left the `workspaces` bar a fifteen-row listing where every
  tab refused. :class:`TheSwitchDecision` and :class:`TheSwitchItself` are the two halves
  of what replaces it.
* **It cannot cross planes.** `commands_frame.SOCKET` is one tmux server per MACHINE, not
  per plane: measured on the operator's own socket while this was written, eleven sessions
  from three different projects, and `default` — a name every plane has whether anybody
  chose it or not — among them. A `switch-client -t default` decided on a name can put an
  operator in another project's frame, across every isolation boundary charter has.
  :class:`TwoPlanesOnOneMachine` is that arrangement on a real server, and it is the only
  class here that two planes are needed for — a single-plane test is structurally blind to
  it.
* **It kills nothing.** :class:`ARealSwitchOnARealServer` moves a real client between two
  real workspace sessions and reads back every pane's pid.

**The plane marker and the pane record are two mechanisms and each covers the other's
gap**, which is why both are measured: a session an older charter created carries no
`@charter_plane`, so an absent marker decides nothing and the pane records still answer;
a pane id recorded for a chat that is over can name another plane's live pane after a
`kill-server`, and the marker refuses that.
"""

from __future__ import annotations

import contextlib
import os
import pty
import shutil
import subprocess
import tempfile
import time
import unittest
from pathlib import Path
from unittest import mock

from charter import commands_frame, config, workspace
from charter.frame import chats, state, switch
from tests import _tmuxreap, _tmuxsocket
from tests._isolation import PersonaIso

_HAS_TMUX = shutil.which("tmux") is not None

#: This module's own server, unique per test PROCESS — `tests/_tmuxreap.py`'s namespace,
#: so a run the deletion sweep kills mid-flight is recognised and collected by the next
#: one rather than left running for days.
SOCKET = _tmuxreap.name("ws-switch")

#: The FALLBACK path for :data:`SOCKET`'s file. tmux is the authority on its own socket
#: path and the teardown asks it; this copy of tmux's rule is spent only on the teardown
#: that has no server left to ask, and nothing is ever asserted about it.
SOCKET_PATH = os.path.join(os.environ.get("TMUX_TMPDIR") or "/tmp",
                           f"tmux-{os.getuid()}", SOCKET)

#: The workspace name BOTH planes have. Any shared name reproduces the collision;
#: `shared` is spelled out rather than `default` so a reader is not left wondering whether
#: the fallback name is doing something special. It is not — every name is shared.
SHARED = "shared"

_DEADLINE = 20.0

_TERM_CANDIDATES = tuple(dict.fromkeys(
    ([os.environ["TERM"]] if os.environ.get("TERM", "dumb") != "dumb" else [])
    + ["xterm-256color", "screen", "vt100"]))


def _completed(argv, rc=0, out="", err=""):
    return subprocess.CompletedProcess(argv, rc, stdout=out, stderr=err)


#: "the plane this test is standing in", resolved when the row is built rather than when
#: the module is imported — `config.STATE_DIR` is re-pointed per test by `PersonaIso` and
#: per plane by `_plane`, which is the whole point of both.
_MINE = object()


def _seat(session: str, pane: str, plane=_MINE) -> str:
    """One `commands_frame._PANE_SEAT_FORMAT` row, in tmux's own spelling.

    *plane* is `_MINE` for a session this plane's launcher marked, ``None`` for one an
    older charter created and left unmarked, and a string for another plane's.
    """
    marker = str(config.STATE_DIR) if plane is _MINE else (plane or "")
    return f"{session}\t{pane}\t{marker}"


def _a_chat(fid: str, *, ws: str, pane: str | None,
            server: str = commands_frame.SOCKET) -> None:
    """A chat directory on THIS plane, in the shape a launcher leaves one."""
    state.frame_dir(fid, create=True)
    state.record_workspace(fid, ws)
    state.record_server(fid, server)
    if pane is not None:
        state.record_harness_pane(fid, pane)


class TheSwitchDecision(PersonaIso, unittest.TestCase):
    """`switch.to_workspace` — which names a client may be aimed at, decided with no
    server in the room.

    `frame/switch.py` makes no tmux call at all, which is what lets the palette ask it on
    its own open and a panel ask it on a repaint. What it therefore cannot answer is
    whether that workspace is *open*, and :class:`TheSwitchItself` is where that lives.
    """

    FID = "alpha.1"

    def setUp(self):
        super().setUp()
        self.enterContext(mock.patch.dict(os.environ, {}, clear=True))
        for n in ("alpha", "beta"):
            (config.WORKSPACES_DIR / n).mkdir(parents=True, exist_ok=True)
        _a_chat(self.FID, ws="alpha", pane="%1")

    def test_another_workspace_of_this_plane_is_a_yes(self):
        self.assertTrue(switch.to_workspace(self.FID, "beta").ok)

    def test_the_sentence_names_the_workspace_the_operator_is_going_to(self):
        """It is said on the chat the switch LANDS on, by `_switch_client`, so it has to
        make sense read there — beside a harness in a workspace the operator has just
        arrived in, not beside the one they left."""
        self.assertEqual(switch.to_workspace(self.FID, "beta").message,
                         "workspace → beta")

    def test_nothing_at_all_is_written(self):
        """The whole function is a decision. A write here would be #411's shape — an
        outcome computed for one noun and performed on another — and there are two rungs
        to lose: the per-session pointer under the chat's id and the launch record."""
        was = state.version(self.FID)
        switch.to_workspace(self.FID, "beta")
        self.assertEqual(state.workspace_for(self.FID), "alpha")
        self.assertIsNone(workspace.for_session(self.FID))
        self.assertEqual(state.version(self.FID), was)


class TheSwitchItself(PersonaIso, unittest.TestCase):
    """`commands_frame._switch_client` — the tmux half, against a fake server.

    Deliberately not a real tmux: what a fake can show and a server cannot is **which
    questions were asked, in which order, and which were not asked at all** — and the
    refusals here are all about a reading charter took before it moved anything.
    :class:`ARealSwitchOnARealServer` is the other half of the pair.
    """

    FID = "alpha.1"
    HERE = "$1"
    THERE = "$2"

    def setUp(self):
        super().setUp()
        self.enterContext(mock.patch.dict(os.environ, {}, clear=True))
        for n in ("alpha", "beta"):
            (config.WORKSPACES_DIR / n).mkdir(parents=True, exist_ok=True)
        _a_chat(self.FID, ws="alpha", pane="%1")
        _a_chat("beta.1", ws="beta", pane="%2")
        self.said = self.enterContext(
            mock.patch.object(commands_frame, "_say_on_screen"))
        self.laid_out = self.enterContext(
            mock.patch.object(commands_frame, "_apply_arrangement"))
        self.enterContext(mock.patch.object(
            commands_frame, "_relayout_target",
            side_effect=lambda fid: (commands_frame.SOCKET, "%9", (3, 7))))

    class _Server:
        """The four answers a switch reads, and a record of everything it asked."""

        def __init__(self, *, place="$1\t@1", panes=(), here=(), there=(),
                     landing=(), switch_rc=0):
            self.place = place
            self.panes = list(panes)
            #: `list-clients -t` answers, keyed by the session id asked about.
            self.clients = {"$1": list(here), "$2": list(there)}
            self.landing = list(landing)
            self.switch_rc = switch_rc
            self.calls: list[list[str]] = []
            self.switched: list[tuple[str, str]] = []

        @staticmethod
        def _lines(rows) -> str:
            return "".join(f"{r}\n" for r in rows)

        def __call__(self, cmd, **kwargs):
            self.calls.append(list(cmd))
            if "display-message" in cmd:
                return _completed(cmd, 0, self.place)
            if "list-panes" in cmd:
                return _completed(cmd, 0, self._lines(self.panes))
            if "list-clients" in cmd:
                target = cmd[cmd.index("-t") + 1]
                # After the switch, the clients that moved are on the target — modelled
                # by answering the target's list from what the switches recorded, which
                # is what makes the "did it move" reading a real reading here.
                rows = list(self.clients.get(target, []))
                rows += [c for c, t in self.switched if t == target]
                return _completed(cmd, 0, self._lines(rows))
            if "list-windows" in cmd:
                return _completed(cmd, 0, self._lines(self.landing))
            if "switch-client" in cmd:
                # Recorded only when tmux took it, which is what makes `switch_rc=1` a
                # server that refused rather than a fake that lies about itself: the
                # client list below is built from these, so a refused switch leaves the
                # target's list exactly as it was.
                if self.switch_rc == 0:
                    self.switched.append((cmd[cmd.index("-c") + 1],
                                          cmd[cmd.index("-t") + 1]))
                return _completed(cmd, self.switch_rc)
            return _completed(cmd, 0)

        def asked(self, verb: str) -> int:
            return sum(1 for c in self.calls if verb in c)

    def _switch(self, server, ws="beta", said="workspace → beta"):
        with mock.patch("charter.commands_frame.subprocess.run", side_effect=server):
            commands_frame._switch_client(self.FID, ws, said=said)
        return server

    def _ordinary(self, **kw):
        """A server where everything is in place: this chat's pane in `$1` with one
        client on it, `beta.1`'s pane in `$2`, and `$2`'s current window drawing
        `beta.1`."""
        opts = dict(place="$1\t@1", panes=[_seat("$1", "%1"), _seat("$2", "%2")],
                    here=["/dev/ttys001"], landing=["$2\t1\tbeta.1"])
        opts.update(kw)
        return self._Server(**opts)

    # -- the switch ---------------------------------------------------------------------

    def test_every_client_on_this_session_is_moved_by_name_to_the_target_session(self):
        """`-c <client>` and `-t <session id>`, and both halves matter. Without `-c` tmux
        picks its own current client, which on a socket serving eleven sessions from three
        projects is very likely somebody else's terminal; a session NAME as the target is
        the cross-plane collision this whole feature is built around."""
        s = self._switch(self._ordinary(here=["/dev/ttys001", "/dev/ttys002"]))
        self.assertEqual(s.switched, [("/dev/ttys001", "$2"), ("/dev/ttys002", "$2")])

    def test_the_chat_left_behind_loses_its_panels_and_keeps_everything_else(self):
        """#686's rule, one scope out: a background window keeps stale geometry, so panels
        left running in one are rendering at a width that is no longer their window's.
        Nothing else about the chat is touched — the harness keeps running, which is the
        operator's requirement in one sentence."""
        self._switch(self._ordinary())
        left = [c for c in self.laid_out.call_args_list if c[0][0] == self.FID]
        self.assertEqual(len(left), 1)
        self.assertEqual(left[0][1]["want"], [])
        self.assertEqual(state.workspace_for(self.FID), "alpha")

    def test_the_chat_landed_on_is_re_dressed_unconditionally(self):
        """§4b: *"a workspace switched back into must be re-dressed unconditionally,
        exactly as #686 now does for chats."* The panels there were torn down when that
        workspace went to the background, so they have to be split into the window tmux
        has just resized — there is no "has the geometry changed" question to ask."""
        self._switch(self._ordinary())
        arrived = [c for c in self.laid_out.call_args_list if c[0][0] == "beta.1"]
        self.assertEqual(len(arrived), 1)
        self.assertTrue(arrived[0][1]["want"])

    def test_which_chat_it_landed_on_is_read_off_the_server_and_never_guessed(self):
        """`switch-client` restores the target session's own last active window, which is
        not the seat `_plane_session` happened to match. Two chats in `beta`, the SECOND
        of them current: dressing the first would leave the operator looking at bare
        panes and re-laying-out a window nobody is in."""
        _a_chat("beta.2", ws="beta", pane="%3")
        self._switch(self._ordinary(landing=["$2\t0\tbeta.1", "$2\t1\tbeta.2"]))
        self.assertEqual([c[0][0] for c in self.laid_out.call_args_list],
                         [self.FID, "beta.2"])

    def test_the_outcome_is_said_on_the_chat_the_operator_landed_on(self):
        """A notice is drawn by a panel out of a frame's own state, so it has to be
        written to the frame the operator will be reading. Said on `alpha.1` it would go
        into panels this switch has just torn down."""
        self._switch(self._ordinary())
        self.said.assert_called_once_with("beta.1", "workspace → beta", ok=True)

    # -- the refusals, and what each one leaves standing ---------------------------------

    def test_a_workspace_with_no_session_is_opened_rather_than_refused(self):
        """**What a workspace nobody has opened is answered with**, and it is no longer a
        refusal. This used to print `charter <harness> --workspace beta` for the operator
        to type, on the grounds that opening ends in an `attach` and a detached switch has
        no terminal for one — measured false on 3.7c and at the 3.2 floor, and beside the
        point, because arriving is `switch-client`. The open itself is
        `tests/test_a_workspace_tab_opens_what_it_names.py`; what belongs HERE is that the
        switch reaches it, and that an open which produces nothing moves nothing."""
        with mock.patch.object(commands_frame, "_open_workspace",
                               return_value=None) as opener:
            s = self._switch(self._ordinary(panes=[_seat("$1", "%1")]))
        opener.assert_called_once()
        self.assertEqual(opener.call_args[0][1], "beta")
        self.assertEqual(s.switched, [])
        self.laid_out.assert_not_called()

    def test_a_workspace_this_plane_has_never_opened_reaches_no_tmux_call_for_it(self):
        """The ordinary case, and the property that keeps it cheap: with no chat directory
        there is nothing to compare against, so "is it open" is answered on disk without
        asking tmux to list a single pane. `_pane_place` is still asked, because charter
        has to know where it is standing before it can move — or, now, before it opens
        somewhere to move to."""
        for chat in chats.of_workspace("beta"):
            state.record_harness_pane(chat, "")
        s = self._switch(self._ordinary())
        self.assertEqual(s.asked("list-panes"), 0)
        self.assertEqual(s.switched, [])

    def test_a_client_that_did_not_move_keeps_this_chats_panels(self):
        """#684's rule, re-asked one scope out: a command that exits 0 having acted on the
        wrong thing is indistinguishable from one that acted on the right thing, so the
        teardown is gated on a READING — the clients are on the target session — and not
        on a return code. A refusal here leaves both workspaces exactly as they were."""
        s = self._switch(self._ordinary(switch_rc=1))
        self.laid_out.assert_not_called()
        self.assertIn("did not move this terminal", self.said.call_args[0][1])
        self.assertTrue(s.asked("switch-client"))

    def test_a_session_with_nobody_attached_moves_nothing_and_says_so(self):
        """There is no client to move — the operator detached, or an agent is driving this
        frame with no terminal on it. Reporting a switch that moved nothing is #411's
        shape arriving through a success."""
        s = self._switch(self._ordinary(here=[]))
        self.assertEqual(s.switched, [])
        self.laid_out.assert_not_called()
        self.assertIn("no terminal is attached", self.said.call_args[0][1])

    def test_a_frame_inside_the_operators_own_tmux_is_refused_by_name(self):
        """**A workspace is a tmux session only on charter's own server** (§2.1). Inside
        an operator's tmux every chat charter opens is a `new-window` in the session they
        were already in, whatever workspace it names, so there is no session for another
        workspace to be — and both things `switch-client` could do there are wrong. This
        is asked FIRST, before any reading, because it is about what this frame IS."""
        # `_tmuxsocket.OPERATOR_SOCKET` and never a spelled path (#601): a socket path
        # with a literal uid in it is one developer's machine written into the suite. What
        # `is_operator_socket` reads is the leading slash, and this is the real thing tmux
        # would hand this machine.
        state.record_server(self.FID, _tmuxsocket.OPERATOR_SOCKET)
        s = self._switch(self._ordinary())
        self.assertEqual(s.calls, [], "a guest frame asked the server anything at all")
        self.laid_out.assert_not_called()
        said = self.said.call_args[0][1]
        self.assertIn("a window in your own tmux", said)
        self.assertIn("charter <harness> --workspace beta", said)

    def test_charters_own_socket_written_the_long_way_is_not_that_frame(self):
        """**#812, and the pair to the case above rather than a replacement for it.**

        The refusal is right; the premise was not. `is_operator_socket` was a
        leading-slash test, and tmux writes its socket into every pane it opens as an
        ABSOLUTE path — so a chat launched from inside one of charter's own panes recorded
        `/private/tmp/tmux-<uid>/charter` for the server `-L charter` reaches, and every
        tab in it was told it was a window in somebody else's tmux. The operator's report
        is exactly that: switch to `fleet`, land in a new chat, and then no tab, including
        the one back, would move them.

        Two spellings of one socket are not the same string, so this asserts on the
        SWITCH: the same fixture as the ordinary case, with the server recorded the way
        `$TMUX` spells it, still moves the client."""
        state.record_server(self.FID, _tmuxsocket.socket_path(commands_frame.SOCKET))
        s = self._switch(self._ordinary())
        self.assertEqual(s.switched, [("/dev/ttys001", "$2")],
                         "a frame on charter's own socket was refused as a guest's")
        self.said.assert_called_once_with("beta.1", "workspace → beta", ok=True)

    def test_a_chat_whose_own_window_cannot_be_found_refuses_before_anything_moves(self):
        """`cmd_chat`'s own sentence for its own reason: with no reading of where this
        client is standing there is no way to tell afterwards whether it moved, and a
        switch that cannot establish that must not tear anything down."""
        s = self._switch(self._ordinary(place=""))
        self.assertEqual(s.switched, [])
        self.laid_out.assert_not_called()
        self.assertIn("cannot find this chat's own window", self.said.call_args[0][1])

    def test_a_target_that_resolves_to_this_very_session_is_refused(self):
        """`switch.to_workspace` refused this by NAME off `state.workspace_for`, and
        records can disagree with tmux. Switching a client to the session it is already on
        is a no-op that would then tear this chat's panels down and re-dress them for
        nothing."""
        s = self._switch(self._ordinary(panes=[_seat("$1", "%1"), _seat("$1", "%2")]))
        self.assertEqual(s.switched, [])
        self.laid_out.assert_not_called()
        self.assertIn("already in workspace 'beta'", self.said.call_args[0][1])

    def test_a_landing_chat_charter_cannot_name_is_left_undressed_and_unsaid(self):
        """A workspace session an older charter created carries no `@charter_chat` on its
        windows. The client still moved — that is the report — and charter says nothing
        rather than announcing a switch on a frame nobody is looking at. This chat's own
        panels still go, because the operator is no longer in front of them."""
        s = self._switch(self._ordinary(landing=["$2\t1\t"]))
        self.assertEqual(s.switched, [("/dev/ttys001", "$2")])
        self.assertEqual([c[0][0] for c in self.laid_out.call_args_list], [self.FID])
        self.said.assert_not_called()

    # -- §4j, across a switch that happens -----------------------------------------------

    def test_no_record_of_either_chat_is_re_pointed(self):
        """The invariant #733 and #788 are about, asserted across a SUCCESSFUL switch —
        which is the only form of it that could catch a `record_workspace` added on this
        path, and the form #789's refusal could never assert at all."""
        self._switch(self._ordinary())
        self.assertEqual(state.workspace_for(self.FID), "alpha")
        self.assertEqual(state.frame_workspace(self.FID), "alpha")
        self.assertIsNone(workspace.for_session(self.FID))
        self.assertEqual(chats.of_workspace("alpha"), [self.FID])
        self.assertEqual(chats.of_workspace("beta"), ["beta.1"])

    def test_nothing_is_killed_and_no_window_is_selected(self):
        """The operator's requirement as a list of verbs charter must not issue. A
        `kill-session` would end the harnesses they asked to keep running; a
        `select-window` would drag the OTHER workspace's client off what it was reading
        (§2.3, measured), which is the defect open-or-focus exists to avoid."""
        s = self._switch(self._ordinary())
        for verb in ("kill-session", "kill-window", "kill-pane", "select-window",
                     "new-session", "new-window", "detach-client"):
            self.assertEqual(s.asked(verb), 0, verb)


class ThePlaneIsWhatDecidesWhichSessionIsOurs(PersonaIso, unittest.TestCase):
    """`commands_frame._plane_session` — the resolver both §4k's focus and §4b's switch
    ask, and the two mechanisms it answers with.

    The pane record FINDS the session and the `@charter_plane` marker VETOES it. Each
    covers what the other cannot: a session an older charter created carries no marker, and
    a pane id recorded for a chat that is over can name another plane's live pane after a
    `kill-server`.
    """

    def setUp(self):
        super().setUp()
        self.enterContext(mock.patch.dict(os.environ, {}, clear=True))
        (config.WORKSPACES_DIR / SHARED).mkdir(parents=True, exist_ok=True)
        _a_chat("shared.1", ws=SHARED, pane="%0")

    def _resolve(self, rows):
        done = _completed(["tmux"], 0, "".join(f"{r}\n" for r in rows))
        with mock.patch.object(commands_frame.tmuxctl, "run", return_value=done):
            return commands_frame._plane_session(commands_frame.SOCKET, ws=SHARED)

    def test_a_session_this_plane_marked_is_ours(self):
        self.assertEqual(self._resolve([_seat("$3", "%0")]), ("$3", "shared.1"))

    def test_a_session_nobody_marked_is_still_ours_if_we_recorded_its_pane(self):
        """**Every session on the operator's own socket the day this shipped.** A marker
        that had to be present would refuse a switch into a workspace the operator is
        looking at right now, which is a migration charged to the person who upgrades."""
        self.assertEqual(self._resolve([_seat("$3", "%0", plane=None)]),
                         ("$3", "shared.1"))

    def test_a_session_another_plane_marked_is_refused_though_the_pane_id_matches(self):
        """**The residual the pane record cannot close, closed.** Pane ids restart at `%0`
        when a tmux server does, so a `%0` recorded for a chat that is over can name
        another plane's live pane — and on the switch path there is no reap in front of
        it. The marker is the only thing on this machine that says whose session it is."""
        self.assertIsNone(self._resolve([_seat("$3", "%0", plane="/somewhere/.charter")]))

    def test_a_marked_session_further_down_the_list_is_still_found(self):
        """The veto skips a row rather than ending the search: two planes can hold the
        same recorded pane id across a server restart, and the one that is really ours may
        be listed second."""
        self.assertEqual(
            self._resolve([_seat("$9", "%0", plane="/somewhere/.charter"),
                           _seat("$3", "%0")]),
            ("$3", "shared.1"))

    def test_a_row_that_is_not_three_fields_is_not_read_as_a_pane(self):
        """`_window_seats`' rule: exactly the field count the format asks for, so a server
        answering something else cannot have half a row read as a pane id.
        `_plane_option_argv` is what guarantees charter's own marker never adds a
        fourth."""
        self.assertIsNone(self._resolve(["$3\t%0"]))
        self.assertIsNone(self._resolve(["$3\t%0\t/a\textra"]))


def _tmux(*args: str, socket: str = SOCKET) -> subprocess.CompletedProcess:
    return subprocess.run(["tmux", "-L", socket, *args], capture_output=True, text=True,
                          timeout=15)


def _await(predicate, timeout: float = _DEADLINE) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(0.05)
    return predicate()


@contextlib.contextmanager
def _plane(root: Path):
    """Resolve every charter path against *root* for the duration — one plane of the two.

    `config.use`/`config.restore` is the same seam `PersonaIso` uses; entering it twice
    with two roots inside one test process is what "two planes on one machine" means when
    the machine is a test runner.
    """
    previous = config.use(root)
    try:
        yield
    finally:
        config.restore(previous)


class _RealServer(unittest.TestCase):
    """One tmux server of this module's own, and the pty plumbing the classes below share.

    Nothing here touches `commands_frame.SOCKET`: the operator's own frame lives on that
    socket with eleven sessions on it, and this suite starts, moves and kills clients.
    """

    def setUp(self) -> None:
        self.addCleanup(self._teardown_socket)
        self.kids: list[tuple[int, int]] = []

    def _teardown_socket(self) -> None:
        """End the server and unlink its socket, in that order and in one cleanup —
        `tests/test_frame_tmux_integration.py::_TmuxServerFixture`'s rule, which measures
        why the reverse order leaves the real server running."""
        for pid, fd in self.kids:
            self._reap_pty(pid, fd)
        said = _tmux("display-message", "-p", "#{socket_path}")
        path = said.stdout.strip()
        _tmux("kill-server")
        for candidate in {SOCKET_PATH, path if path.startswith("/") else SOCKET_PATH}:
            try:
                os.unlink(candidate)
            except OSError:
                pass

    def _attach(self, session: str) -> str:
        """A real client on *session*, or a skip naming what tmux refused."""
        refusals = []
        for term in _TERM_CANDIDATES:
            pid, fd = pty.fork()
            if pid == 0:
                try:
                    os.environ["TERM"] = term
                    os.execvp("tmux", ["tmux", "-L", SOCKET, "attach", "-t", session])
                finally:
                    os._exit(127)
            if _await(lambda: self._clients(session) != []):
                self.kids.append((pid, fd))
                return self._clients(session)[-1]
            refusals.append(f"TERM={term}")
            self._reap_pty(pid, fd)
        self.skipTest("no tmux client can attach on this machine, and a workspace "
                      "switch is a decision about an ATTACHED client — tried "
                      + ", ".join(refusals))

    def _reap_pty(self, pid: int, fd: int) -> None:
        try:
            os.kill(pid, 9)
            os.waitpid(pid, 0)
        except OSError:
            pass
        try:
            os.close(fd)
        except OSError:
            pass

    def _clients(self, session: str) -> list[str]:
        return [c for c in _tmux("list-clients", "-t", session,
                                 "-F", "#{client_name}").stdout.split() if c]

    def _panes(self) -> list[str]:
        return sorted(_tmux("list-panes", "-a", "-F",
                            "#{session_name}:#{window_name} dead=#{pane_dead} "
                            "pid=#{pane_pid}").stdout.splitlines())


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class ARealSwitchOnARealServer(_RealServer):
    """The tmux facts §4b rests on, measured rather than assumed.

    Two workspace sessions, one real client on a real pty, and a `switch-client` between
    them. What a fake cannot supply is tmux's own behaviour: whether the client really
    moves, whether anything dies, and which window it lands on.

    Verified on tmux 3.7c and at the 3.2 floor (`~/.local/share/charter-testing/tmux-3.2`
    first on `$PATH`), identically on both — so nothing here carries a version gate.
    """

    def setUp(self) -> None:
        super().setUp()
        self.assertEqual(_tmux("new-session", "-d", "-s", "alpha", "-n", "alpha.1",
                               "sleep 300").returncode, 0)
        _tmux("new-session", "-d", "-s", "beta", "-n", "beta.1", "sleep 300")
        # **`-a`, and the fixture is wrong without it** — `layout.chat_window_argv`'s own
        # measurement, met here: `-t <session>` resolves to that session's CURRENT window
        # and `new-window` reads a resolved window target as the INDEX to create at, so
        # without `-a` this answers `create window failed: index 0 in use` and the second
        # window never exists. A fixture that swallowed that would leave this class
        # asserting that a one-window session lands on its one window.
        second = _tmux("new-window", "-d", "-a", "-t", "beta", "-n", "beta.2",
                       "-P", "-F", "#{window_id}", "sleep 300")
        self.assertEqual(second.returncode, 0, second.stderr)
        # By `@<id>`, never `-t beta:beta.2`: tmux parses a target's dot as
        # `window.pane`, so the name a chat window actually carries cannot select it —
        # the same rule `_WINDOW_SEAT_FORMAT` records for a workspace name with a dot.
        self.assertEqual(_tmux("select-window", "-t",
                               second.stdout.strip()).returncode, 0)
        self.beta = _tmux("display-message", "-p", "-t", "beta",
                          "#{session_id}").stdout.strip()

    def test_the_client_moves_and_nothing_at_all_is_killed(self):
        """The operator's requirement, measured: *"changing workspace does not mean
        stopping the old chat session."* Every pane on the server is still there with the
        same pid and `pane_dead=0`, and the `attach` process itself is still alive."""
        client = self._attach("alpha")
        before = self._panes()
        alive = os.waitpid(self.kids[0][0], os.WNOHANG) == (0, 0)
        self.assertEqual(_tmux("switch-client", "-c", client, "-t",
                               self.beta).returncode, 0)
        self.assertTrue(_await(lambda: self._clients("beta") == [client]),
                        "the client never arrived on the other workspace")
        self.assertEqual(self._clients("alpha"), [])
        self.assertEqual(self._panes(), before, "a switch killed or replaced a pane")
        self.assertTrue(alive)
        self.assertEqual(os.waitpid(self.kids[0][0], os.WNOHANG), (0, 0),
                         "the attach process died with the switch")

    def test_it_lands_on_the_window_that_session_was_last_on(self):
        """Which chat the operator arrives at is tmux's answer and not charter's, which is
        why `_switch_client` reads it back rather than dressing the seat it matched."""
        client = self._attach("alpha")
        _tmux("switch-client", "-c", client, "-t", self.beta)
        self.assertTrue(_await(lambda: self._clients("beta") == [client]))
        self.assertEqual(_tmux("display-message", "-p", "-t", "beta",
                               "#{window_name}").stdout.strip(), "beta.2")

    def test_the_move_is_readable_from_list_clients_on_both_versions(self):
        """**The reading `_switch_client` verifies with, and the one it must not use.**
        `display-message -p -c <client> '#{session_name}'` answers an EMPTY string at the
        3.2 floor for a client that has demonstrably moved — measured — so a check built
        on it would refuse every switch on the older tmux and pass on the newer.
        `list-clients -t <session>` answers on both."""
        client = self._attach("alpha")
        _tmux("switch-client", "-c", client, "-t", self.beta)
        self.assertTrue(_await(lambda: self._clients("beta") == [client]),
                        "list-clients could not see the move this tmux performed")

    def test_a_session_option_is_readable_in_a_pane_format_and_is_not_global(self):
        """What makes `_PANE_SEAT_FORMAT` one round trip instead of two: a session-scoped
        user option resolves in a PANE's format. And it is per session — `beta` reads
        empty — which is the whole of what makes it able to say whose plane a session is.

        `show-options -v` is deliberately not how this is read: for an option nobody set it
        answers rc 1 with `invalid option:` on 3.7c and rc 0 with an empty line on 3.2, so
        a reader built on it would have to branch on the tmux version.
        """
        pane = _tmux("list-panes", "-t", "alpha", "-F", "#{pane_id}").stdout.strip()
        self.assertEqual(_tmux("set-option", "-t", pane,
                               commands_frame._PLANE_OPTION, "/p/.charter").returncode, 0)
        rows = _tmux("list-panes", "-a", "-F",
                     commands_frame._PANE_SEAT_FORMAT).stdout.splitlines()
        fields = [r.split("\t") for r in rows]
        marks = {f[0]: f[2] for f in fields if len(f) == 3}
        self.assertEqual(marks[_tmux("display-message", "-p", "-t", "alpha",
                                     "#{session_id}").stdout.strip()], "/p/.charter")
        self.assertEqual(marks[self.beta], "")


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class TwoPlanesOnOneMachine(_RealServer):
    """**The hazard at the centre of §4b, reproduced before it is fixed.**

    One tmux server, two plane roots, one workspace name they share. Session names are
    unique per server, so there is exactly ONE session called `shared` and it belongs to
    whichever plane created it — which is precisely the trap: plane B's `workspaces` bar
    lists `shared`, a live session called `shared` exists, and a switch decided on that
    name would put plane B's operator inside plane A's harnesses.

    A single-plane test cannot fail for this reason, which is why this class exists at all.
    Everything asserted here is read back off a real server: the pane ids it minted, the
    session ids it minted, and the option charter set on it.
    """

    def setUp(self) -> None:
        super().setUp()
        self.plane_a = Path(tempfile.mkdtemp(prefix="4b-plane-a-"))
        self.plane_b = Path(tempfile.mkdtemp(prefix="4b-plane-b-"))
        for p in (self.plane_a, self.plane_b):
            self.addCleanup(shutil.rmtree, p, True)
        self.assertEqual(_tmux("new-session", "-d", "-s", SHARED, "-n", "c1",
                               "sleep 300").returncode, 0)
        seats = _tmux("list-panes", "-a", "-F", "#{session_id}\t#{pane_id}").stdout
        self.session_id, self.pane = seats.split()[0], seats.split()[1]
        # The window carries the chat id a launcher would have written on it — which is
        # the SECOND name both planes have. `new_chat_id` counts from 1 on each plane's
        # own disk, so `shared.1` is not plane A's id, it is the id both planes mint
        # first. Without this the fixture would let an implementation that matched on
        # `@charter_chat` pass for the wrong reason.
        self.assertEqual(_tmux("set-option", "-w", "-t", self.pane,
                               commands_frame._CHAT_OPTION, "shared.1").returncode, 0)
        with _plane(self.plane_a):
            (config.WORKSPACES_DIR / SHARED).mkdir(parents=True, exist_ok=True)
            _a_chat("shared.1", ws=SHARED, pane=self.pane, server=SOCKET)
            self.a_state = str(config.STATE_DIR)
        # Plane B has a chat directory for the SAME workspace under the SAME chat id, but
        # its own chat is over and the pane it recorded is not on this server.
        with _plane(self.plane_b):
            (config.WORKSPACES_DIR / SHARED).mkdir(parents=True, exist_ok=True)
            _a_chat("shared.1", ws=SHARED, pane="%9000", server=SOCKET)

    def _mark(self, state_dir: str) -> None:
        """What plane *state_dir*'s launcher would have written when it made the session."""
        with mock.patch.object(commands_frame, "_this_plane", return_value=state_dir):
            argv = commands_frame._plane_option_argv(socket=SOCKET, harness_pane=self.pane)
        self.assertIsNotNone(argv)
        self.assertEqual(subprocess.run(argv, capture_output=True,
                                        text=True, timeout=15).returncode, 0)

    def test_the_plane_that_opened_it_resolves_it(self):
        with _plane(self.plane_a):
            self.assertEqual(commands_frame._plane_session(SOCKET, ws=SHARED),
                             (self.session_id, "shared.1"))

    def test_the_other_plane_resolves_nothing_though_every_NAME_matches(self):
        """Plane B has a workspace called `shared`, a chat directory called `shared.1`,
        and a live tmux session called `shared`. Every NAME lines up. The one thing that
        does not is the pane its own launcher wrote down — which is the only fact on this
        machine that belongs to one plane and not the other."""
        with _plane(self.plane_b):
            self.assertIsNone(commands_frame._plane_session(SOCKET, ws=SHARED))

    def test_the_marker_refuses_a_pane_id_that_came_back_round(self):
        """**The residual a pane record cannot close.** Plane B is made to record the pane
        id this server actually minted — which is what a `kill-server` plus a recycled
        `%0` produces — so its pane record now matches plane A's live session and every
        name matches too. The marker plane A's launcher wrote is the only thing left that
        can tell them apart, and it does."""
        self._mark(self.a_state)
        with _plane(self.plane_b):
            _a_chat("shared.1", ws=SHARED, pane=self.pane, server=SOCKET)
            self.assertIsNone(commands_frame._plane_session(SOCKET, ws=SHARED))
        with _plane(self.plane_a):
            self.assertEqual(commands_frame._plane_session(SOCKET, ws=SHARED),
                             (self.session_id, "shared.1"))

    def test_without_the_marker_the_recycled_pane_id_is_the_hazard_itself(self):
        """The negative control, and the reason the marker is not decoration: the same
        arrangement with no marker on the session resolves plane A's live frame FOR PLANE
        B. This is the failure being prevented, run to prove it is reachable."""
        with _plane(self.plane_b):
            _a_chat("shared.1", ws=SHARED, pane=self.pane, server=SOCKET)
            self.assertEqual(commands_frame._plane_session(SOCKET, ws=SHARED),
                             (self.session_id, "shared.1"))

    def test_a_switch_from_the_other_plane_refuses_by_name_and_moves_no_client(self):
        """**End to end, and it is the operator's decision made observable**: switching is
        restricted to workspaces of THIS plane, and anything else is refused by name.
        Plane B clicks `shared`; there is a live session called `shared` with plane B's
        own operator's terminal nowhere near it; the switch refuses because that session
        is not one plane B can prove is its own, and the client attached to plane A's
        session stays there.

        **Plane B is given a terminal of its own, and the fixture is not honest without
        it.** Since a tab OPENS, `_switch_client` refuses first — before spending a harness
        process — when nothing is attached to the frame doing the switching. Plane B's
        `$9000` is a session this test invented and nobody is on it, so without this the
        case would stop one refusal early and prove nothing about planes at all.
        """
        self._mark(self.a_state)
        client = self._attach(SHARED)
        windows_before = _tmux("list-windows", "-t", SHARED,
                               "-F", "#{window_id}").stdout.split()
        with _plane(self.plane_b):
            _a_chat("shared.2", ws=SHARED, pane="%9001", server=SOCKET)
            state.record_harness_pane("shared.2", "%9001")
            said = []
            with mock.patch.object(commands_frame, "_pane_place",
                                   return_value=("$9000", "@9000")), \
                 mock.patch.object(commands_frame, "_clients_on",
                                   side_effect=lambda _s, sess:
                                   ["/dev/ttyplaneb"] if sess == "$9000" else []), \
                 mock.patch.object(commands_frame, "_say_on_screen",
                                   side_effect=lambda fid, msg, **kw: said.append(msg)):
                commands_frame._switch_client("shared.2", SHARED,
                                              said="workspace → shared")
        self.assertTrue(any("is already running on this machine" in m for m in said),
                        said)
        self.assertEqual(self._clients(SHARED), [client],
                         "another plane's switch moved this plane's client")
        # **The window count is the assertion that matters now that a tab OPENS.** The
        # refusal this used to get was free; an open is not, and `cmd_launch` decides
        # between starting a session and joining one on a NAME that both planes have. A
        # missing guard would not have moved the client either — it would have quietly
        # added plane B's chat window to plane A's session and then failed the marker veto,
        # which reads on screen as "the open failed" and on the server as litter in another
        # project's frame.
        self.assertEqual(
            _tmux("list-windows", "-t", SHARED, "-F", "#{window_id}").stdout.split(),
            windows_before,
            "an open from another plane added a window to this plane's session")


if __name__ == "__main__":
    unittest.main()
