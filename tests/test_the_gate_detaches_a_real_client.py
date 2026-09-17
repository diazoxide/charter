"""The gate detaches the terminal that pressed it — on a REAL tmux, with two real clients.

**Nothing below could have been settled by a unit test, and #1097 is the proof.** The unit
half (`tests/test_one_gate_closes_charter.py`) says which argv charter builds; what it
cannot say is what tmux does with it. The shipped row spelled `detach-client -s <chat id>`,
and measured on tmux 3.7c and at the 3.2 floor that command is rc 0 in every case: on a chat
window with one pane it resolves to a pane that does not exist and detaches nobody, and on a
chat window with any strip at all it resolves to the WORKSPACE's session and detaches every
client attached to it. Both answers look identical from inside charter.

So this file attaches **two** real clients over two real ptys to one real frame, drives a
real keypress into one of them, and asks tmux afterwards who is still attached. The answer
has to be *exactly one, and it is the other one*.

Everything here is real: a real tmux server on a throwaway socket, the frame's own private
config sourced from `commands_frame.conf_text` (so the `F10` and `F2` binds under test are
the ones charter really writes), a real detached `charter frame-palette` carving a real pane
off the harness, and a real `charter frame-palette --pane` drawing the surface inside it.

**The plane marker is written with `commands_frame._plane_option_argv`**, not by hand: the
detach refuses a session this plane did not mark, so a fixture that stamped the option
itself would be proving the guard against a value the launcher does not write.
"""

from __future__ import annotations

import fcntl
import itertools
import os
import shutil
import struct
import subprocess
import termios
import time
import unittest
from pathlib import Path

from charter import commands_frame, config
from charter.frame import gate, layout, slots, state, tmuxctl

from tests import _tmuxreap
from tests._isolation import PersonaIso, make_plane

_HAS_TMUX = shutil.which("tmux") is not None

_REPO_ROOT = Path(__file__).resolve().parents[1]

_SERVERS = itertools.count()

#: How long a real gesture is given. Longer than the unit half's patience because every
#: case here waits on a chain of real processes — a `run-shell` child, a `split-window`, a
#: Python interpreter importing charter, and a detached `tmux detach-client`.
_DEADLINE = 30.0

_TERM_CANDIDATES = tuple(dict.fromkeys(
    ([os.environ["TERM"]] if os.environ.get("TERM", "dumb") != "dumb" else [])
    + ["xterm-256color", "screen", "vt100"]))

#: Wide enough for the identity row to keep the version AND the button, so a case that
#: asserts the button is there is not measuring a starved row.
COLS = 120

#: `F10`, as a terminal sends it. Measured in task 4's Step 0 (G1) on tmux 3.7c and at the
#: 3.2 floor: written into an attached client's pty, this fires the root-table `F10` bind
#: and the action reports that client's own `#{client_name}`.
F10 = b"\x1b[21~"

#: `F2` in its SS3 form, measured the same way in the same reading.
F2 = b"\x1bOQ"


def _await(predicate, timeout: float = _DEADLINE) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(0.05)
    return predicate()


def _fork_pty(rows: int, cols: int) -> tuple[int, int]:
    """`pty.fork` with the window size set BEFORE the exec — #648's fix, borrowed.

    A client born at the kernel's 80x24 default attaching to a wider session makes tmux
    resize the window, which is a SIGWINCH in every pane and a repaint no case here asked
    for.
    """
    master, slave = os.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
    pid = os.fork()
    if pid == 0:
        os.close(master)
        os.login_tty(slave)
        return 0, -1
    os.close(slave)
    return pid, master


class _ARealFrameTwoTerminalsAreLookingAt(PersonaIso):
    """One real frame on a throwaway server, with two real clients attached to it.

    **`make_plane` and not merely `PersonaIso`'s root**: every gesture here starts a CHILD
    charter that resolves its own plane, and `PersonaIso` writes no `charter.toml`, so such
    a child would go looking for a plane of its own and find the operator's live one (#527).
    """

    def setUp(self) -> None:
        super().setUp()
        v = tmuxctl.version()
        if v is None or v < tmuxctl.FLOOR:
            self.skipTest(f"the frame's floor is tmux {tmuxctl.FLOOR[0]}."
                          f"{tmuxctl.FLOOR[1]}; this machine has {v}")
        self.socket = _tmuxreap.name(f"gatepresser{next(_SERVERS)}")
        self.socket_path = os.path.join(os.environ.get("TMUX_TMPDIR") or "/tmp",
                                        f"tmux-{os.getuid()}", self.socket)
        self.addCleanup(self._teardown_socket)
        self.plane = make_plane(self, "schema = 1\n")
        (config.WORKSPACES_DIR / "alpha").mkdir(parents=True, exist_ok=True)

        self.fid = state.frame_id("gate", os.getpid())
        state.frame_dir(self.fid, create=True)

        existing = os.environ.get("PYTHONPATH", "")
        parts = [str(_REPO_ROOT)] + ([existing] if existing else [])
        self.env = dict(os.environ, CHARTER_ROOT=str(self.plane),
                        CHARTER_SESSION_ID=self.fid,
                        PYTHONPATH=os.pathsep.join(parts))
        self.env.pop("CHARTER_HOME", None)
        self.env.pop("CHARTER_PERSONA", None)
        self.env.pop("CHARTER_WORKSPACE", None)

        self.harness = self._start_session()
        self._tmux("set-environment", "-t", self.fid, "CHARTER_SESSION_ID", self.fid)
        # **The interpreter the binds run charter with**, delivered out of band exactly as
        # `commands_frame._charter_py_env_argv` delivers it — the bind text is a template
        # holding `"$CHARTER_PY"`, so a fixture that skipped this would have every keypress
        # fail with a shell error printed into the harness pane.
        self._run(commands_frame._charter_py_env_argv(socket=self.socket,
                                                      session=self.fid))
        state.record_server(self.fid, self.socket)
        state.record_harness_pane(self.fid, self.harness)
        state.record_workspace(self.fid, "alpha")
        # **The launcher's own identity record, `CHARTER_ROOT` included**, and it is not
        # fixture dressing: `commands_frame._relayout_pane_env` builds the palette pane's
        # `-e` payload out of this record and passes every unknown name as EMPTY rather
        # than omitting it (which is what stops a pane inheriting another frame's plane
        # off the shared server). So a fixture that recorded no root would hand the pane
        # `CHARTER_ROOT=`, and the surface inside it would resolve a plane from its cwd —
        # this repository's, whose plane is the operator's live one. Measured: it then
        # reads no `server` record, falls back to `tmuxctl.LEGACY_SOCKET`, and every tmux
        # command the gate makes goes to a server this test never started.
        state.record_identity(self.fid, {"CHARTER_ROOT": str(self.plane),
                                         "CHARTER_HARNESS": "",
                                         "CHARTER_WORKSPACE": "",
                                         "CHARTER_PERSONA": ""})
        # The plane marker the detach proves against, written by the launcher's own argv.
        self._run(commands_frame._plane_option_argv(socket=self.socket,
                                                    harness_pane=self.harness))
        self.assertEqual(
            self._option(self.fid, "@charter_plane"), commands_frame._this_plane(),
            "the fixture's session is not marked as this plane's, so a green detach "
            "would be measuring a guard that never ran")
        self._source_conf()

    # -- the frame ---------------------------------------------------------- #

    def _tmux(self, *args: str, env=None) -> subprocess.CompletedProcess:
        return subprocess.run(["tmux", "-L", self.socket, *args], capture_output=True,
                              text=True, timeout=20, env=env)

    def _run(self, argv) -> None:
        self.assertIsNotNone(argv, "charter refused to build the argv this fixture needs")
        done = subprocess.run(argv, capture_output=True, text=True, timeout=20)
        self.assertEqual(done.returncode, 0, done.stderr)

    def _option(self, target: str, name: str) -> str:
        return self._tmux("display-message", "-p", "-t", target,
                          f"#{{{name}}}").stdout.strip()

    def _start_session(self) -> str:
        started = self._tmux("-f", "/dev/null", "new-session", "-d", "-s", self.fid,
                             "-x", str(COLS), "-y", "24", "-P", "-F", "#{pane_id}", "cat",
                             env=self.env)
        self.assertEqual(started.returncode, 0, started.stderr)
        return started.stdout.strip()

    def _source_conf(self) -> None:
        """charter's OWN frame config — the binds under test, not binds this file wrote."""
        conf = str(self.tmp / f"{self.fid}.conf")
        with open(conf, "w") as fh:
            fh.write(commands_frame.conf_text(hotkey=config.FRAME["hotkey"], mouse=True,
                                              history_limit=100, session=self.fid))
        sourced = self._tmux("source-file", conf)
        self.assertEqual(sourced.returncode, 0, sourced.stderr)

    def _split_panel(self, slot: str, *, flag: str, size: int) -> str:
        argv = layout.panel_command(slot=slot, session=self.fid)
        r = self._tmux("split-window", "-t", self.harness, flag, "-l", str(size),
                       "-P", "-F", "#{pane_id}", "--", *argv, env=self.env)
        self.assertEqual(r.returncode, 0, r.stderr)
        pane = r.stdout.strip()
        self._run(commands_frame._panel_mark_argv(socket=self.socket, pane_id=pane))
        selected = self._tmux("select-pane", "-t", self.harness)
        self.assertEqual(selected.returncode, 0, selected.stderr)
        return pane

    # -- the two terminals -------------------------------------------------- #

    def _attach_two(self) -> None:
        """Attach A, then B, and remember which client name is which.

        Named off `list-clients` rather than off the pty, because the NAME is what the bind
        expands and what `detach-client -t` takes — asking tmux is the only reading that
        can be compared with what charter aimed at.
        """
        self.a_pid, self.a_fd, self.a = self._attach()
        self.b_pid, self.b_fd, self.b = self._attach()
        self.assertNotEqual(self.a, self.b, "both ptys attached as one client")
        self.assertEqual(sorted(self._clients()), sorted([self.a, self.b]))

    def _clients(self) -> list[str]:
        out = self._tmux("list-clients", "-t", self.fid,
                         "-F", "#{client_name}").stdout
        return [ln for ln in out.split() if ln]

    def _attach(self):
        size = self._window_size()
        self.assertNotEqual(size, (-1, -1), "tmux would not say how big its window is")
        cols, rows = size
        before = set(self._clients())
        refusals = []
        for term in _TERM_CANDIDATES:
            pid, fd = _fork_pty(rows=rows, cols=cols)
            if pid == 0:
                try:
                    os.environ["TERM"] = term
                    os.environ["CHARTER_ROOT"] = str(self.plane)
                    os.execvp("tmux",
                              ["tmux", "-L", self.socket, "attach", "-t", self.fid])
                finally:
                    os._exit(127)
            if _await(lambda: bool(set(self._clients()) - before), timeout=10.0):
                self.addCleanup(self._reap, pid, fd)
                return pid, fd, (set(self._clients()) - before).pop()
            refusals.append(term)
            self._reap(pid, fd)
        self.skipTest("no tmux client will attach on this machine, and this whole file "
                      "needs two — tried TERM=" + ", ".join(refusals))

    @staticmethod
    def _reap(pid: int, fd: int) -> None:
        for call in (lambda: os.kill(pid, 9), lambda: os.waitpid(pid, 0),
                     lambda: os.close(fd)):
            try:
                call()
            except OSError:
                pass

    def _teardown_socket(self) -> None:
        said = self._tmux("display-message", "-p", "#{socket_path}")
        was_running = said.returncode == 0
        path = said.stdout.strip()
        self._tmux("kill-server")
        for candidate in {self.socket_path,
                          path if path.startswith("/") else self.socket_path}:
            try:
                os.unlink(candidate)
            except OSError:
                pass
        if not was_running:
            return
        self.assertTrue(path.startswith("/"), "tmux would not say where its socket is")
        self.assertFalse(os.path.exists(path), f"{path} survived this test's teardown")

    # -- reading the frame back --------------------------------------------- #

    def _panes(self) -> list[str]:
        return self._tmux("list-panes", "-t", self.fid,
                          "-F", "#{pane_id}").stdout.split()

    def _shown(self, pane: str) -> str:
        return self._tmux("capture-pane", "-p", "-t", pane).stdout

    def _window_size(self) -> tuple[int, int]:
        r = self._tmux("display-message", "-p", "-t", self.fid,
                       "#{window_width} #{window_height}")
        said = r.stdout.split()
        if r.returncode != 0 or len(said) != 2:
            return (-1, -1)
        return int(said[0]), int(said[1])

    def _opened_surface(self, before):
        """The one pane a gesture carved, once something has been drawn in it."""
        self.assertTrue(_await(lambda: len(self._panes()) > len(before)),
                        "no pane was carved off the harness")
        opened = [p for p in self._panes() if p not in before]
        self.assertEqual(len(opened), 1, f"expected one new pane, got {opened}")
        return opened[0]


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class ThePresserOnARealServer(_ARealFrameTwoTerminalsAreLookingAt, unittest.TestCase):
    """Two terminals on one frame, and only the one that asked goes away."""

    def setUp(self) -> None:
        super().setUp()
        self._attach_two()

    def _choose(self, keys: bytes, *, typed: bytes = b"") -> str:
        """Press *keys* in A's terminal, type *typed* into the surface, press Enter.

        Answers what the surface had on it, so a case that detaches nothing says WHY.
        """
        before = self._panes()
        os.write(self.a_fd, keys)
        pane = self._opened_surface(before)
        self.assertTrue(
            _await(lambda: gate.DETACH_TITLE.split(" (")[0] in self._shown(pane)),
            f"the surface never drew its rows: {self._shown(pane)!r}")
        if typed:
            os.write(self.a_fd, typed)
            self.assertTrue(
                _await(lambda: gate.STOP_TITLE not in self._shown(pane)),
                f"typing narrowed nothing: {self._shown(pane)!r}")
        drawn = self._shown(pane)
        os.write(self.a_fd, b"\r")
        return drawn

    def test_f10_then_enter_detaches_only_the_terminal_that_pressed_it(self):
        """The whole feature, end to end: a key in one terminal, and the other one stays.

        The cursor is already on *Close charter (keep chats running)* when the menu opens
        (`gate.Gate`), so Enter is the whole of the second half — which is what makes this
        a two-keystroke gesture rather than a menu to read.
        """
        drawn = self._choose(F10)
        self.assertTrue(_await(lambda: self._clients() == [self.b]),
                        f"A is still attached, or B went too: {self._clients()} "
                        f"(the menu said: {drawn!r}; the frame says: "
                        f"{state.notice(self.fid)!r})")

    def test_the_other_terminal_is_still_usable_afterwards(self):
        """Not merely listed — B's own client still answers tmux, which is the difference
        between *detached one* and *detached both and tmux has not noticed yet*."""
        self._choose(F10)
        self.assertTrue(_await(lambda: self._clients() == [self.b]))
        self.assertEqual(
            self._tmux("display-message", "-p", "-c", self.b,
                       "#{client_name}").stdout.strip(), self.b)

    def test_f2_close_charter_detaches_only_the_terminal_that_pressed_it(self):
        """#1097 on the route the issue was filed against.

        **Red at `c846740`**, where this row ran `detach-client -s <chat id>`: tmux reads a
        dotted target as `session.pane`, so on this frame — whose window has exactly the
        harness in it — it resolved to a pane that does not exist, answered rc 0, and
        detached nobody while the palette reported *detaching — the harness keeps running*.
        Split one strip off the same window and the same command detaches BOTH clients.

        `keep chats` is typed rather than `close charter`, which is a substring of both
        rows: `palette.matches` filters on the title, and a query that left two rows
        standing would put the cursor wherever `Gate._refilter` pinned it rather than where
        this case means to be.
        """
        drawn = self._choose(F2, typed=b"keep chats")
        self.assertTrue(_await(lambda: self._clients() == [self.b]),
                        f"A is still attached, or B went too: {self._clients()} "
                        f"(the palette said: {drawn!r})")

    def test_escape_leaves_the_menu_and_detaches_nobody(self):
        """The control, and the commonest outcome of an `F10` somebody did not mean: the
        surface closes, both terminals stay, and the harness gets its pane back."""
        before = self._panes()
        os.write(self.a_fd, F10)
        pane = self._opened_surface(before)
        self.assertTrue(_await(lambda: gate.LABEL in self._shown(pane)))
        os.write(self.a_fd, b"\x1b")
        self.assertTrue(_await(lambda: self._panes() == before),
                        "the menu never gave the pane back")
        self.assertEqual(sorted(self._clients()), sorted([self.a, self.b]))


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class ARealClickRecordsItsPresser(_ARealFrameTwoTerminalsAreLookingAt, unittest.TestCase):
    """A real SGR press on the real `F10 close` button, in a real identity row.

    **Three processes and none of them is this one.** The row is painted by a `charter
    panel top` child, the press is written into a client's pty and routed by tmux's own
    `MouseDown1Pane` bind — which records the clicking client on the panel pane before it
    forwards — and the surface is carved by a detached `charter frame-palette` that child
    started. Step 0's G3 measured the middle link on both tmux versions; this measures the
    whole chain, with the value ending up on an argv.
    """

    def setUp(self) -> None:
        super().setUp()
        self.top = self._split_panel("top", flag="-v", size=1)
        self._attach_two()
        self.assertTrue(
            _await(lambda: slots.GATE_BUTTON in self._shown(self.top)),
            f"the identity row never drew the button: {self._shown(self.top)!r}")

    def _press(self, col: int) -> None:
        r = self._tmux("display-message", "-p", "-t", self.top,
                       "#{pane_left} #{pane_top}")
        self.assertEqual(r.returncode, 0, r.stderr)
        left, top = (int(n) for n in r.stdout.split())
        os.write(self.a_fd, b"\x1b[<0;%d;%dM" % (left + col + 1, top + 1))
        os.write(self.a_fd, b"\x1b[<0;%d;%dm" % (left + col + 1, top + 1))

    def _button_column(self) -> int:
        """Where the button is DRAWN, read off the pane rather than off `slots.DOORS`.

        That object lives in the panel's own process, not in this one — asking charter
        where it put something and then clicking there would be a test that agrees with
        itself. This is the column the operator's eye lands on.
        """
        row = self._shown(self.top).split("\n")[0].rstrip()
        at = row.index(slots.GATE_BUTTON)
        self.assertNotIn(slots.GATE_BUTTON, row[at + 1:], f"not unique in {row!r}")
        return at

    def test_a_press_on_f10_close_opens_the_gate_for_that_client(self):
        """The presser survives the whole trip: tmux expands it into a pane option, the
        panel reads its own pane back, and it lands on the argv of the pane the surface is
        drawn in.

        Both halves asserted — the option really holds A's name and not the eighteen
        characters `#{client_name}` (which is what it would hold without `set-option -F`),
        and the surface really is the gate rather than the ordinary palette.
        """
        before = self._panes()
        self._press(self._button_column())
        pane = self._opened_surface(before)
        self.assertTrue(
            _await(lambda: gate.LABEL in self._shown(pane)),
            f"the pane that opened is not the gate: {self._shown(pane)!r}")
        recorded = self._option(self.top, gate.PRESSER_OPTION)
        self.assertEqual(recorded, self.a)
        self.assertTrue(gate.CLIENT_RE.fullmatch(recorded), recorded)
        started = self._tmux("display-message", "-p", "-t", pane,
                             "#{pane_start_command}").stdout
        self.assertIn(gate.OPTION, started)
        self.assertIn(self.a, started)

    def test_the_gate_a_click_opened_detaches_only_that_client(self):
        """The pointer route's own end to end, so the value being CARRIED is not mistaken
        for the value being USED: Enter on the row the click's menu opened takes A's
        terminal and leaves B's."""
        before = self._panes()
        self._press(self._button_column())
        pane = self._opened_surface(before)
        self.assertTrue(_await(lambda: gate.DETACH_TITLE.split(" (")[0]
                               in self._shown(pane)))
        os.write(self.a_fd, b"\r")
        self.assertTrue(_await(lambda: self._clients() == [self.b]),
                        f"A is still attached, or B went too: {self._clients()}")

    def test_a_press_on_the_workspace_chip_opens_the_palette_and_not_the_gate(self):
        """The control on the other door of the same row: one strip, two destinations, and
        `_Doors` has to tell them apart — a version that answered `opens_gate` for every
        published column would pass every case above."""
        row = self._shown(self.top).split("\n")[0]
        before = self._panes()
        self._press(row.index("⬢"))
        pane = self._opened_surface(before)
        self.assertTrue(
            _await(lambda: "density" in self._shown(pane)),
            f"the pane that opened is not the ordinary palette: {self._shown(pane)!r}")
        self.assertNotIn(gate.LABEL, self._shown(pane))
