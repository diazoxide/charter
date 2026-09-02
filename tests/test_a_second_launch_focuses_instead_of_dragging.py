"""`charter -w foo` opens or focuses — §4k, and §3.3 is why it is matched the way it is.

**The defect this replaces has no small fix.** Both clients of a tmux session share one
current window (§2.10, measured on 3.7c and at the 3.2 floor alike), so a second launch's
`select-window` drags the client that was already there onto the new chat, and
`_drop_panels` then tears down the panels it was reading. The only mechanism that does not
drag is a session group, which would stop a workspace being a tmux session — the
foundation everything since #488 rests on. So charter stops making the situation instead
of fixing it: **a workspace somebody already has open is attached to, not added to.**

**And it is matched on this plane's own chat directories, never on a live session name.**
One tmux server serves every plane on this machine, session names are bare workspace names,
and `default` is a name EVERY plane has (§2.13). `charter -w default` in one plane already
adds a window to another plane's session today; open-or-focus matched on a name would make
that the *advertised* behaviour. `_workspace_to_focus` therefore starts on disk — this
plane's `.charter/frame/` — and asks tmux only about `%<pane>` and `$<session>` ids, which
are minted by the server and cannot be two planes' at once.

`TwoPlanesOnOneMachine` is the test §7 asks for and the only one here that can fail for
§3.3's reason: a single plane cannot be confused with anybody, so a single-plane test is
structurally blind to it. It runs against a REAL tmux and is skipped, never failed, where
the machine has none — which per §2.12 is every CI job charter has.
"""

from __future__ import annotations

import contextlib
import io
import os
import pty
import shutil
import subprocess
import tempfile
import time
import unittest
from contextlib import redirect_stderr
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame, config, workspace
from charter.frame import state
from tests import _tmuxreap
from tests._isolation import PersonaIso

_HAS_TMUX = shutil.which("tmux") is not None

#: This module's own server, unique per test PROCESS — `tests/_tmuxreap.py`'s namespace,
#: so a run the deletion sweep kills mid-flight is recognised and collected by the next
#: one rather than left running for days.
SOCKET = _tmuxreap.name("open-or-focus")

#: The FALLBACK path for :data:`SOCKET`'s file. tmux is the authority on its own socket
#: path and the teardown asks it; this copy of tmux's rule is spent only on the teardown
#: that has no server left to ask, and nothing is ever asserted about it.
SOCKET_PATH = os.path.join(os.environ.get("TMUX_TMPDIR") or "/tmp",
                           f"tmux-{os.getuid()}", SOCKET)

#: The workspace name BOTH planes have. Any shared name reproduces §2.13; `default` is
#: the one every plane has whether anybody chose it or not.
SHARED = "shared"

_DEADLINE = 20.0

_TERM_CANDIDATES = tuple(dict.fromkeys(
    ([os.environ["TERM"]] if os.environ.get("TERM", "dumb") != "dumb" else [])
    + ["xterm-256color", "screen", "vt100"]))


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


def _completed(argv, rc=0, out="", err=""):
    return subprocess.CompletedProcess(argv, rc, stdout=out, stderr=err)


#: "the plane this test is standing in", resolved when the row is built rather than when
#: the module is imported — `config.STATE_DIR` is re-pointed per test by `PersonaIso` and
#: per plane by `_plane`, which is the whole point of both.
_MINE = object()


def _seat(session: str, pane: str, plane=_MINE) -> str:
    """One `commands_frame._PANE_SEAT_FORMAT` row, in tmux's own spelling.

    *plane* is `_MINE` for a session this plane's launcher marked (what every launch
    writes since §4b), ``None`` for one an older charter created and left unmarked, and a
    string for another plane's.
    """
    marker = str(config.STATE_DIR) if plane is _MINE else (plane or "")
    return f"{session}\t{pane}\t{marker}"


class _FakeServer:
    """The three answers a focus decision reads, and a record of everything it asked.

    Deliberately not `tests/test_frame_launcher._FakeTmux`: that fake models a whole
    launch, and every test here is about the handful of calls that happen BEFORE a launch
    would start anything. What this fake is for is the half a real server cannot show —
    which questions were asked, in which order, and which were not asked at all.
    """

    def __init__(self, *, panes=(), clients=(), panes_rc=0, clients_rc=0,
                 sessions=(), chats=()):
        #: `#{session_id}\t#{pane_id}` rows, exactly as `list-panes -a -F` prints them.
        self.panes = list(panes)
        #: One `#{client_name}` per client attached to the session asked about.
        self.clients = list(clients)
        self.panes_rc = panes_rc
        self.clients_rc = clients_rc
        self.sessions = list(sessions)
        self.chats = list(chats)
        self.calls: list[list[str]] = []
        #: What `list-clients -t` was aimed at, so a test can assert it was an id.
        self.clients_target = None

    @staticmethod
    def _lines(rows) -> str:
        """What tmux prints for *rows*: one per line with a trailing newline, and NOTHING
        at all for none — not a bare `"\\n"`, which is what a naive join produces and what
        would make "no clients attached" look like one client with a blank name."""
        return "".join(f"{r}\n" for r in rows)

    def __call__(self, cmd, **kwargs):
        self.calls.append(list(cmd))
        if "list-panes" in cmd:
            return _completed(cmd, self.panes_rc, self._lines(self.panes))
        if "list-clients" in cmd:
            self.clients_target = cmd[cmd.index("-t") + 1]
            return _completed(cmd, self.clients_rc, self._lines(self.clients))
        if "list-sessions" in cmd:
            return _completed(cmd, 0, self._lines(self.sessions))
        if "list-windows" in cmd:
            return _completed(cmd, 0, self._lines(self.chats))
        return _completed(cmd, 0)

    def asked(self, verb: str) -> int:
        return sum(1 for c in self.calls if verb in c)


def _a_chat(fid: str, *, ws: str, pane: str | None,
            server: str = commands_frame.SOCKET) -> None:
    """A chat directory on THIS plane, in the shape a launcher leaves one.

    *server* is named rather than assumed because the real-tmux class below runs on a
    server of its own: a `server` marker naming charter's production socket would be a
    fixture asserting something about a machine's live frames.
    """
    state.frame_dir(fid, create=True)
    state.record_workspace(fid, ws)
    state.record_server(fid, server)
    if pane is not None:
        state.record_harness_pane(fid, pane)


class TheFocusDecision(PersonaIso, unittest.TestCase):
    """`_workspace_to_focus` — three answers, and which calls each one costs."""

    def _decide(self, fake):
        with mock.patch("charter.commands_frame.subprocess.run", side_effect=fake):
            return commands_frame._workspace_to_focus(commands_frame.SOCKET, ws=SHARED)

    def test_a_workspace_this_plane_has_never_opened_reaches_no_tmux_call_at_all(self):
        """The ordinary first launch, and the property that keeps this free: with no chat
        directory there is nothing to compare against, so the decision is made on disk and
        the server is never asked anything."""
        fake = _FakeServer(panes=[_seat("$0", "%0")], clients=["/dev/ttys001"])
        self.assertIsNone(self._decide(fake))
        self.assertEqual(fake.calls, [])

    def test_a_chat_with_no_recorded_pane_reaches_no_tmux_call_either(self):
        """A directory a launcher never got as far as recording a pane in has nothing
        that could identify it on the server — a chat id would, and a chat id is exactly
        what two planes can both hold (§3.3)."""
        _a_chat("shared.1", ws=SHARED, pane=None)
        fake = _FakeServer(panes=[_seat("$0", "%0")], clients=["/dev/ttys001"])
        self.assertIsNone(self._decide(fake))
        self.assertEqual(fake.calls, [])

    def test_a_recorded_pane_that_is_not_live_stops_before_asking_about_clients(self):
        """Every chat of this workspace is cold. There is nothing to focus, and the
        second question is not worth a subprocess."""
        _a_chat("shared.1", ws=SHARED, pane="%4")
        fake = _FakeServer(panes=[_seat("$0", "%0"), _seat("$1", "%2")],
                           clients=["/dev/ttys001"])
        self.assertIsNone(self._decide(fake))
        self.assertEqual(fake.asked("list-panes"), 1)
        self.assertEqual(fake.asked("list-clients"), 0)

    def test_a_live_workspace_with_nobody_attached_is_opened_rather_than_focused(self):
        """Deliberate, and it is the launch an operator reopening a detached workspace
        makes: with no client on the session there is nobody to drag, so the chat is
        added and selected exactly as it shipped."""
        _a_chat("shared.1", ws=SHARED, pane="%0")
        fake = _FakeServer(panes=[_seat("$0", "%0")], clients=[])
        self.assertIsNone(self._decide(fake))
        self.assertEqual(fake.asked("list-clients"), 1)

    def test_a_live_workspace_somebody_is_looking_at_is_focused(self):
        _a_chat("shared.1", ws=SHARED, pane="%0")
        fake = _FakeServer(panes=[_seat("$3", "%0")], clients=["/dev/ttys001"])
        self.assertEqual(self._decide(fake), ("$3", "shared.1"))

    def test_the_session_is_targeted_by_its_id_and_never_by_its_name(self):
        """#693's rule, and here it is the correctness argument rather than a convenience:
        a session NAME is a name every plane on the machine may have."""
        _a_chat("shared.1", ws=SHARED, pane="%0")
        fake = _FakeServer(panes=[_seat("$3", "%0")], clients=["/dev/ttys001"])
        self._decide(fake)
        self.assertEqual(fake.clients_target, "$3")

    def test_only_this_workspaces_chats_are_considered(self):
        """`chats.of_workspace` reads each directory's own `workspace` file, so a live
        chat of a DIFFERENT workspace on this same plane focuses nothing."""
        _a_chat("elsewhere.1", ws="elsewhere", pane="%0")
        fake = _FakeServer(panes=[_seat("$3", "%0")], clients=["/dev/ttys001"])
        self.assertIsNone(self._decide(fake))
        self.assertEqual(fake.calls, [])

    def test_a_server_that_will_not_list_its_panes_focuses_nothing(self):
        _a_chat("shared.1", ws=SHARED, pane="%0")
        fake = _FakeServer(panes=[], panes_rc=1, clients=["/dev/ttys001"])
        self.assertIsNone(self._decide(fake))

    def test_a_client_list_that_is_only_whitespace_is_no_client(self):
        """A server answering a bare newline has told us about no clients, and reading it
        as one would focus a workspace nobody is looking at — the exact launch that SHOULD
        open a chat and select it. Pins `.split()`, which is what asks "is there a client"
        in a form a test can go red on (see the comment at the line)."""
        _a_chat("shared.1", ws=SHARED, pane="%0")
        fake = _FakeServer(panes=[_seat("$3", "%0")], clients=["", "   "])
        self.assertIsNone(self._decide(fake))

    def test_a_server_that_will_not_list_that_sessions_clients_focuses_nothing(self):
        """A session the server no longer has between the two calls answers rc 1 — and a
        launch that cannot tell whether anybody is there opens its own chat, which is the
        behaviour that shipped."""
        _a_chat("shared.1", ws=SHARED, pane="%0")
        fake = _FakeServer(panes=[_seat("$3", "%0")], clients=["/dev/ttys001"],
                           clients_rc=1)
        self.assertIsNone(self._decide(fake))

    def test_a_row_with_too_few_fields_is_not_read_as_a_pane(self):
        """`_chat_being_left`'s rule: exactly the field count the format asks for, so a
        server answering something else cannot have half a row read as a pane id."""
        _a_chat("shared.1", ws=SHARED, pane="%0")
        fake = _FakeServer(panes=["$3"], clients=["/dev/ttys001"])
        self.assertIsNone(self._decide(fake))

    def test_a_row_with_too_many_fields_is_not_read_as_a_pane_either(self):
        """Its own case, because the two halves of one field-count check are two
        different servers: a row that is short raises where a row that is long merely
        lies, and only the second can be mistaken for a match."""
        _a_chat("shared.1", ws=SHARED, pane="%0")
        fake = _FakeServer(panes=[_seat("$3", "%0") + "\textra"], clients=["/dev/ttys001"])
        self.assertIsNone(self._decide(fake))

    def test_a_server_that_cannot_be_listed_is_not_reported_on_every_launch(self):
        """`report=False` on both calls, for `_live_sessions`' own reason: on a machine
        with no charter server running yet, "could not list the panes" is the ORDINARY
        answer, and charter must not print an error in front of every launch that happens
        to be the first one."""
        _a_chat("shared.1", ws=SHARED, pane="%0")
        fake = _FakeServer(panes=[], panes_rc=1)
        said = io.StringIO()
        with redirect_stderr(said):
            self.assertIsNone(self._decide(fake))
        self.assertEqual(said.getvalue(), "")

    def test_a_session_whose_clients_cannot_be_listed_is_not_reported_either(self):
        _a_chat("shared.1", ws=SHARED, pane="%0")
        fake = _FakeServer(panes=[_seat("$3", "%0")], clients_rc=1)
        said = io.StringIO()
        with redirect_stderr(said):
            self.assertIsNone(self._decide(fake))
        self.assertEqual(said.getvalue(), "")

    def test_a_recorded_pane_that_is_not_a_pane_id_matches_nothing(self):
        """The recorded value is COMPARED against tmux's answer and never sent to it, so
        a state file holding something that is not a pane id is refused by the comparison
        itself rather than by a shape check that could never fire."""
        _a_chat("shared.1", ws=SHARED, pane="; kill-server")
        fake = _FakeServer(panes=[_seat("$3", "%0")], clients=["/dev/ttys001"])
        self.assertIsNone(self._decide(fake))


class TheFocusItself(PersonaIso, unittest.TestCase):
    """`_focus_workspace` — the attach, and the two things around it."""

    def _focus(self, fake, *, picked=False):
        with mock.patch("charter.commands_frame.subprocess.run", side_effect=fake):
            return commands_frame._focus_workspace("$3", "shared.1", ws=SHARED,
                                                   picked=picked)

    def test_a_workspace_the_operator_picked_is_still_written_down(self):
        """#518's property survives the new branch: an operator who answered the picker
        and was focused into their answer must not be asked again next launch. Written
        under the chat being focused, which is the frame session the pointer is about."""
        fake = _FakeServer()
        self.assertEqual(self._focus(fake, picked=True), 0)
        self.assertEqual(workspace.is_locked("shared.1"), SHARED)

    def test_a_launch_that_answered_nothing_writes_no_pointer(self):
        """The other half of `_pin_workspace`'s own rule, carried over unchanged: a launch
        that resolved silently must not move any terminal's workspace."""
        fake = _FakeServer()
        self.assertEqual(self._focus(fake), 0)
        self.assertIsNone(workspace.is_locked("shared.1"))

    def test_a_detach_says_the_harness_is_still_running(self):
        """The same sentence a launch that detached prints, and true for the same reason:
        this client left, the session did not. Silence here would return an operator to
        their shell with agents running and nothing saying so."""
        said = io.StringIO()
        with redirect_stderr(said):
            self._focus(_FakeServer())
        self.assertIn("still running", said.getvalue())
        self.assertIn(f"attach -t {SHARED}", said.getvalue())


class TheLaunchTakesTheDecision(PersonaIso, unittest.TestCase):
    """`cmd_launch`'s own branch: what a focused launch does, and what it does not."""

    def _launch(self, fake, *, workspace=SHARED, harness="claude", rest=()):
        args = SimpleNamespace(harness=harness, rest=list(rest), no_frame=False,
                               workspace=workspace, pick=False)
        stripped = {k: v for k, v in os.environ.items()
                    if k not in ("TMUX", "TMUX_PANE")}
        with mock.patch.dict(os.environ, stripped, clear=True), \
             mock.patch("charter.commands_frame.subprocess.run", side_effect=fake), \
             mock.patch("charter.commands_frame.shutil.which",
                        side_effect=lambda n, *a, **k: f"/usr/bin/{n}"), \
             mock.patch("charter.frame.tmuxctl.version", return_value=(3, 7)), \
             mock.patch("sys.stdout.isatty", return_value=True), \
             mock.patch("sys.stdin.isatty", return_value=False):
            return commands_frame.cmd_launch(args)

    def test_a_focused_launch_attaches_by_session_id_and_returns_zero(self):
        _a_chat("shared.1", ws=SHARED, pane="%0")
        fake = _FakeServer(panes=[_seat("$3", "%0")], clients=["/dev/ttys001"],
                           sessions=[SHARED], chats=["shared.1"])
        self.assertEqual(self._launch(fake), 0)
        attaches = [c for c in fake.calls if "attach" in c]
        self.assertEqual(len(attaches), 1)
        self.assertEqual(attaches[0][-2:], ["-t", "$3"])

    def test_a_focused_launch_claims_no_ordinal_and_makes_no_directory(self):
        """The whole reason the branch sits between the reap and `new_chat_id`: a focus is
        a read and an `attach`. Nothing is allocated, so nothing has to be given back if
        the operator detaches a second later."""
        _a_chat("shared.1", ws=SHARED, pane="%0")
        before = sorted(p.name for p in (config.STATE_DIR / "frame").iterdir())
        fake = _FakeServer(panes=[_seat("$3", "%0")], clients=["/dev/ttys001"],
                           sessions=[SHARED], chats=["shared.1"])
        self._launch(fake)
        after = sorted(p.name for p in (config.STATE_DIR / "frame").iterdir())
        self.assertEqual(before, after)

    def test_a_focused_launch_starts_no_window_and_selects_nothing(self):
        """`new-window` plus `select-window` IS the drag (§2.3). A focus must issue
        neither — that is the whole of what makes it not one."""
        _a_chat("shared.1", ws=SHARED, pane="%0")
        fake = _FakeServer(panes=[_seat("$3", "%0")], clients=["/dev/ttys001"],
                           sessions=[SHARED], chats=["shared.1"])
        self._launch(fake)
        for verb in ("new-window", "new-session", "select-window", "split-window"):
            self.assertEqual(fake.asked(verb), 0, verb)

    def test_a_launch_that_focuses_nothing_still_opens_a_chat(self):
        """The negative control, and it must be a launch that could have focused: same
        plane, same workspace, same live session — only nobody attached."""
        _a_chat("shared.1", ws=SHARED, pane="%0")
        fake = _FakeServer(panes=[_seat("$3", "%0")], clients=[],
                           sessions=[SHARED], chats=["shared.1"])
        with mock.patch("charter.commands_frame._spawn_gather"):
            self._launch(fake)
        self.assertEqual(fake.asked("new-window"), 1)

    def test_a_command_the_operator_named_is_run_rather_than_swallowed(self):
        """`charter frame -- <cmd>` is the escape hatch for a command charter has never
        met, and attaching cannot answer it: a focus here would silently discard the argv
        the operator typed. Same workspace, same live session, same attached client — the
        only difference is that this launch named something to RUN."""
        _a_chat("shared.1", ws=SHARED, pane="%0")
        fake = _FakeServer(panes=[_seat("$3", "%0")], clients=["/dev/ttys001"],
                           sessions=[SHARED], chats=["shared.1"])
        with mock.patch("charter.commands_frame._spawn_gather"):
            self._launch(fake, harness=None, rest=["--", "htop"])
        self.assertEqual(fake.asked("new-window"), 1)
        self.assertIn("htop", sum((c for c in fake.calls if "new-window" in c), []))

    def test_a_harness_the_operator_passed_flags_to_is_run_rather_than_swallowed(self):
        """`charter claude --resume <id>` is the same fact one harness over: `rest` is the
        operator's own flags, `launch_argv` carries them, and a focus would drop them."""
        _a_chat("shared.1", ws=SHARED, pane="%0")
        fake = _FakeServer(panes=[_seat("$3", "%0")], clients=["/dev/ttys001"],
                           sessions=[SHARED], chats=["shared.1"])
        with mock.patch("charter.commands_frame._spawn_gather"):
            self._launch(fake, rest=["--resume", "abc123"])
        self.assertEqual(fake.asked("new-window"), 1)
        self.assertIn("abc123", sum((c for c in fake.calls if "new-window" in c), []))

    def test_the_attach_failure_is_reported_rather_than_folded_into_a_zero(self):
        _a_chat("shared.1", ws=SHARED, pane="%0")

        class _RefusesToAttach(_FakeServer):
            def __call__(self, cmd, **kwargs):
                if "attach" in cmd:
                    self.calls.append(list(cmd))
                    return _completed(cmd, 3)
                return super().__call__(cmd, **kwargs)

        fake = _RefusesToAttach(panes=[_seat("$3", "%0")], clients=["/dev/ttys001"],
                               sessions=[SHARED], chats=["shared.1"])
        self.assertEqual(self._launch(fake), 3)


def _tmux(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run(["tmux", "-L", SOCKET, *args], capture_output=True, text=True,
                          timeout=15)


def _await(predicate, timeout: float = _DEADLINE) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(0.05)
    return predicate()


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class TwoPlanesOnOneMachine(unittest.TestCase):
    """§7's acceptance shape, reduced to the one question this stage decides.

    Two plane roots, one tmux server, and one workspace name they share — which is the
    only arrangement in which §3.3's flaw exists to be found. Everything asserted here is
    read back off a real server: the pane ids it minted, the session ids it minted, and
    the clients it reports.

    **Not `cmd_launch` end to end**, deliberately. A whole launch here would bring panel
    processes, hooks and an attach into a test whose subject is a decision taken before
    any of them; `TheLaunchTakesTheDecision` above already pins that the launcher takes
    this decision and what it does with each answer. What a mock cannot supply is tmux's
    own vocabulary, and that is exactly what this class asks for.
    """

    def setUp(self) -> None:
        self.addCleanup(self._teardown_socket)
        self.plane_a = Path(tempfile.mkdtemp(prefix="ide-plane-a-"))
        self.plane_b = Path(tempfile.mkdtemp(prefix="ide-plane-b-"))
        for p in (self.plane_a, self.plane_b):
            self.addCleanup(shutil.rmtree, p, True)
        # One session named for the workspace both planes have — which is what charter's
        # own launcher would create, and the collision §2.13 measures on this machine.
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
        # Plane A opened it: its chat directory records the pane the server minted.
        with _plane(self.plane_a):
            _a_chat("shared.1", ws=SHARED, pane=self.pane, server=SOCKET)
        # Plane B has a chat directory for the SAME workspace under the SAME chat id —
        # `new_chat_id` counts from 1 on each plane's own disk, so this is what two planes
        # that both opened `shared` actually look like — but its own chat is over, and the
        # pane it recorded is not on this server.
        with _plane(self.plane_b):
            _a_chat("shared.1", ws=SHARED, pane="%9000", server=SOCKET)

    def _teardown_socket(self) -> None:
        """End the server and unlink its socket, in that order and in one cleanup —
        `tests/test_frame_tmux_integration.py::_TmuxServerFixture`'s rule, which measures
        why the reverse order leaves the real server running."""
        said = _tmux("display-message", "-p", "#{socket_path}")
        path = said.stdout.strip()
        _tmux("kill-server")
        for candidate in {SOCKET_PATH, path if path.startswith("/") else SOCKET_PATH}:
            try:
                os.unlink(candidate)
            except OSError:
                pass

    def _attach(self):
        """A real client on `SHARED`, or a skip naming what tmux refused."""
        refusals = []
        for term in _TERM_CANDIDATES:
            pid, fd = pty.fork()
            if pid == 0:
                try:
                    os.environ["TERM"] = term
                    os.execvp("tmux", ["tmux", "-L", SOCKET, "attach", "-t", SHARED])
                finally:
                    os._exit(127)
            if _await(lambda: _tmux("list-clients", "-t", SHARED,
                                    "-F", "#{client_name}").stdout.strip() != ""):
                self.addCleanup(self._reap_pty, pid, fd)
                return fd
            refusals.append(f"TERM={term}")
            self._reap_pty(pid, fd)
        self.skipTest("no tmux client can attach on this machine, and open-or-focus is "
                      "a decision about an ATTACHED client — tried " + ", ".join(refusals))

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

    def _clients(self) -> list[str]:
        return [c for c in _tmux("list-clients", "-t", SHARED,
                                 "-F", "#{client_name}").stdout.split() if c]

    def _current_window(self) -> str:
        return _tmux("display-message", "-p", "-t", SHARED, "#{window_name}").stdout.strip()

    # -- the decision, on a real server ------------------------------------------------

    def test_the_plane_that_opened_it_focuses_it(self):
        self._attach()
        with _plane(self.plane_a):
            got = commands_frame._workspace_to_focus(SOCKET, ws=SHARED)
        self.assertEqual(got, (self.session_id, "shared.1"))

    def test_the_other_plane_focuses_nothing_though_every_NAME_matches(self):
        """**§3.3, and it is the only test here that two planes are needed for.** Plane B
        has a workspace called `shared`, a chat directory called `shared.1`, and a live
        tmux session called `shared` with a client on it. Every NAME lines up. The one
        thing that does not is the pane its own launcher wrote down — which is the only
        fact on this machine that belongs to one plane and not the other."""
        self._attach()
        with _plane(self.plane_b):
            self.assertIsNone(commands_frame._workspace_to_focus(SOCKET, ws=SHARED))

    def test_the_plane_that_opened_it_focuses_nothing_while_nobody_is_attached(self):
        with _plane(self.plane_a):
            self.assertIsNone(commands_frame._workspace_to_focus(SOCKET, ws=SHARED))

    # -- the tmux facts §4k rests on ---------------------------------------------------

    def test_a_second_client_attaching_by_session_id_does_not_move_the_first(self):
        """The measurement that makes focusing safe where adding a chat is not. Both
        clients share one current window (§2.10), so the question is whether ATTACHING
        moves it — and it does not, on 3.7c and at the 3.2 floor alike."""
        self._attach()
        was = self._current_window()
        _tmux("new-window", "-d", "-t", SHARED, "-n", "c2", "sleep 300")
        self.assertEqual(self._current_window(), was, "`new-window -d` moved the client")
        second = self._attach()
        self.assertEqual(len(self._clients()), 2)
        self.assertEqual(self._current_window(), was)
        self.assertGreater(second, 0)

    def test_selecting_the_new_chat_is_what_drags_the_client(self):
        """The control, and without it the test above is satisfied by a client that was
        never movable. This is §2.3 reproduced on this machine: the launcher's own
        `select-window` moves the client that was already there onto the new chat."""
        self._attach()
        was = self._current_window()
        _tmux("new-window", "-d", "-t", SHARED, "-n", "c2", "sleep 300")
        _tmux("select-window", "-t", f"{SHARED}:c2")
        self.assertTrue(_await(lambda: self._current_window() == "c2"),
                        "tmux did not drag the attached client — §2.10 no longer holds")
        self.assertNotEqual(self._current_window(), was)


class _AttachRefused:
    """*server* with `attach` answering the way it does with no terminal to attach to.

    Wrapping rather than a flag on `_FakeServer`: every other case in this module is about
    a launch that IS the operator's terminal, where an attach that fails is a different
    subject. This is the one place the failure is the point.
    """

    def __init__(self, inner):
        self.inner = inner

    def __call__(self, cmd, **kwargs):
        # The window this launch makes becomes live, which `_FakeServer` alone does not
        # model — every other case here asserts on the CALLS and never on what came back.
        # Without it `cmd_launch` reads its own new chat as already dead and answers for
        # that instead, which would make the return code below measure the fixture.
        got = self.inner(cmd, **kwargs)
        if "new-window" in cmd:
            self.inner.chats.append(cmd[cmd.index("-n") + 1])
            return _completed(cmd, 0, "%9\n")
        if "attach" in cmd:
            return _completed(cmd, 1, "", "open terminal failed: not a terminal")
        return got

    def asked(self, verb: str) -> int:
        return self.inner.asked(verb)

class ALaunchThatIsNotTheTerminalNeverFocuses(PersonaIso, unittest.TestCase):
    """`attach=False` is the third case this gate has to know about, and it was the one
    left out.

    The gate reads `if not rest and _reopening(args) is None:` — which is `_wants_attach`
    with its first question missing. `_wants_attach` exists BECAUSE "am I restoring" and
    "am I the terminal" stopped having one answer, and this site still asks only the old
    one. Its own comment records the last time it had to widen: *"And never for a reopen,
    which is the same gate one case wider."*

    **What that costs, measured on the operator's plane.** The chat bar's `+` runs
    `cmd_new_chat`, which launches with `attach=False` into the workspace it is standing
    in — so the workspace is live and attached BY CONSTRUCTION, the one state that makes
    `_workspace_to_focus` answer. The launch therefore focused instead of adding a chat,
    and focusing is attaching: in a panel process with all three streams on `/dev/null`
    tmux answered `open terminal failed: not a terminal`, `cmd_launch` returned its code,
    and the press reported `could not open another chat — the launcher returned 1`.

    A press on `+` could never add a second chat to a workspace, which is the only thing
    it does.
    """

    def _press(self, fake, *, workspace=SHARED, harness="claude", rest=()):
        """`cmd_new_chat`'s namespace, not the CLI's — `attach=False` and no terminal."""
        args = SimpleNamespace(harness=harness, rest=list(rest), no_frame=False,
                               workspace=workspace, pick=False, attach=False, size=None)
        stripped = {k: v for k, v in os.environ.items()
                    if k not in ("TMUX", "TMUX_PANE")}
        with mock.patch.dict(os.environ, stripped, clear=True), \
             mock.patch("charter.commands_frame.subprocess.run", side_effect=fake), \
             mock.patch("charter.commands_frame.shutil.which",
                        side_effect=lambda n, *a, **k: f"/usr/bin/{n}"), \
             mock.patch("charter.frame.tmuxctl.version", return_value=(3, 7)), \
             mock.patch("charter.commands_frame._spawn_gather"), \
             mock.patch("sys.stdout.isatty", return_value=False), \
             mock.patch("sys.stdin.isatty", return_value=False):
            return commands_frame.cmd_launch(args)

    def _focusable(self):
        """The exact state a `+` press is always in: this plane's workspace, live, with a
        client attached to it. Every other case in this module treats that as the state
        that MUST focus — which is right for a launch that is the terminal."""
        _a_chat("shared.1", ws=SHARED, pane="%0")
        return _FakeServer(panes=[_seat("$3", "%0")], clients=["/dev/ttys001"],
                           sessions=[SHARED], chats=["shared.1"])

    def test_a_launch_that_will_not_attach_does_not_attach(self):
        fake = self._focusable()
        self._press(fake)
        self.assertEqual(fake.asked("attach"), 0,
                         "a launch that asked not to attach attached anyway")

    def test_it_adds_the_window_the_press_was_for(self):
        """The positive half, and the one that makes this a defect rather than a tidiness:
        not attaching is worth nothing if the chat still is not made."""
        fake = self._focusable()
        self._press(fake)
        self.assertEqual(fake.asked("new-window"), 1)

    def test_it_answers_zero_where_a_focus_would_have_answered_the_attach_failure(self):
        """**The operator's actual symptom, and the assertion has to make the attach FAIL
        to be worth anything.** With a fake whose `attach` succeeds this case passes on
        the unfixed code too — it would be green at both inputs, which is no test at all.

        A panel has all three streams on `/dev/null`, so tmux answers `open terminal
        failed: not a terminal` and exits non-zero. `cmd_launch` hands that code back and
        `cmd_new_chat` prints `could not open another chat — the launcher returned 1`.
        A launch that never attaches cannot be told anything by an attach that failed."""
        fake = self._focusable()
        refuses_a_terminal = _AttachRefused(fake)
        self.assertEqual(self._press(refuses_a_terminal), 0)
        self.assertEqual(refuses_a_terminal.asked("new-window"), 1)

    def test_a_launch_that_is_the_terminal_still_focuses(self):
        """The control, in the same fixture — otherwise this class would pass just as well
        against a gate that had deleted the focus branch outright."""
        _a_chat("shared.1", ws=SHARED, pane="%0")
        fake = _FakeServer(panes=[_seat("$3", "%0")], clients=["/dev/ttys001"],
                           sessions=[SHARED], chats=["shared.1"])
        args = SimpleNamespace(harness="claude", rest=[], no_frame=False,
                               workspace=SHARED, pick=False)
        stripped = {k: v for k, v in os.environ.items()
                    if k not in ("TMUX", "TMUX_PANE")}
        with mock.patch.dict(os.environ, stripped, clear=True), \
             mock.patch("charter.commands_frame.subprocess.run", side_effect=fake), \
             mock.patch("charter.commands_frame.shutil.which",
                        side_effect=lambda n, *a, **k: f"/usr/bin/{n}"), \
             mock.patch("charter.frame.tmuxctl.version", return_value=(3, 7)), \
             mock.patch("sys.stdout.isatty", return_value=True), \
             mock.patch("sys.stdin.isatty", return_value=False):
            commands_frame.cmd_launch(args)
        self.assertEqual(fake.asked("attach"), 1)
        self.assertEqual(fake.asked("new-window"), 0)


if __name__ == "__main__":
    unittest.main()
