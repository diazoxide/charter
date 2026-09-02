"""Focus changes and POINTER events in a REAL tmux pane reach a REAL provider component.

Everything here is real: a real tmux server, a real attached client on a real pty, a real
installed distribution on `sys.path`, and four real `charter panel <component-id>`
processes each holding their own pane. Nothing is stubbed, because every claim these two
branches make that could be wrong is a claim about tmux:

* that tmux delivers ``\\x1b[I``/``\\x1b[O`` to a pane's PROGRAM at all (#607);
* that it delivers them to a pane that asked and to no other;
* that a panel can read them off the descriptor `frame/pane.py` claims (fd 1), with the
  pane's `stdin` never consulted;
* that the mode `frame/events.py` sets does not shear the paint that follows;
* that a `resize-window` reaches every pane, focused or not;
* that tmux routes a POINTER to the pane under it by position, pane-relative, **without
  making that pane active** — which is the entire premise of delivering `click` and
  `scroll` at all, and the one this file exists to keep true;
* that the same holds with tmux's OWN mouse on, where it does not hold by default and
  charter's `MouseDown1Pane` bind is what makes it (`APointerWithTmuxsOwnMouseOn`, #634).

The measurements this file is the standing form of, taken against **tmux 3.7c** and
re-taken at the **3.2 floor**, both identical::

    focus-events on   pane that asked      -> READ b'\\x1b[I' on select, b'\\x1b[O' on leave
                      pane that did not    -> nothing, ever
    focus-events off  pane that asked      -> nothing, ever
    resize-window     every pane           -> SIGWINCH, focused or not

    mouse off, harness active, both panes asked for 1000+1006
      click window col 100 (pane left=80) -> panel READ b'\\x1b[<0;20;5M'   active: harness
      click the border between them       -> nobody                       active: harness
      wheel over the panel                -> panel READ b'\\x1b[<64;20;5M'  active: harness
    mouse ON, charter's own `MouseDown1Pane` bind in place (#634)
      click over the panel                -> panel receives it            active: harness
      click over the HARNESS              -> harness receives it          active: harness
      click a pane the operator split     -> that pane receives it        active: THAT pane
      wheel over the panel                -> panel receives it            active: harness

and, for the tty mode, one pane per mode painting three ``\\n``-joined rows::

    cooked  ['AAA','BBB','CCC']   raw  ['AAA','   BBB','      CCC']   cbreak  ['AAA','BBB','CCC']

**The pointer cases INJECT reports into the client's pty rather than moving a mouse**, and
that is a deliberate separation rather than a shortcut: it makes these cases about where
tmux ROUTES a report, which is what charter depends on, and leaves *whether the terminal
sends one at all* — the `[frame] mouse` question, which charter does not control — to the
documentation and to `frame/events.py`'s own measurements. A case that needed a physical
mouse could assert neither.

**The client is attached on a pty born the size the session already is**, and that is
load-bearing rather than tidiness: a client that arrives disagreeing makes tmux resize the
window to fit it, every pane takes a SIGWINCH, and `acme.sized` — which counts resize
events — reads the fixture's own attach as one. `_fork_pty` carries the measurement and
#648 is what it cost before it was one.

`tests/test_a_component_event_reaches_its_handler.py` is the unit half; it can say what
the dispatcher does with a byte, and cannot say whether the byte ever arrives.
"""

from __future__ import annotations

import fcntl
import os
import shutil
import struct
import subprocess
import sys
import termios
import textwrap
import time
import unittest
from pathlib import Path

from charter import commands_frame, config
from charter.frame import layout, overlay, state, tmuxctl

from tests import _tmuxreap
from tests._isolation import PersonaIso

_HAS_TMUX = shutil.which("tmux") is not None

#: The checkout, reached through `$PYTHONPATH` rather than by prepending to `sys.path` —
#: `util.self_relaunch_argv` passes `-P` (#390) and that flag must not be weakened to make
#: a test pass. `tests/test_frame_palette_integration.py`'s rule, one module over.
_REPO_ROOT = Path(__file__).resolve().parents[1]

SOCKET = _tmuxreap.name("focus-integ")
SOCKET_PATH = os.path.join(os.environ.get("TMUX_TMPDIR") or "/tmp",
                           f"tmux-{os.getuid()}", SOCKET)

#: How long a tmux state change gets before this gives up. Generous, and spent only on the
#: way to a failure: every wait below returns the instant the state is right.
_DEADLINE = 25.0

_TERM_CANDIDATES = tuple(dict.fromkeys(
    ([os.environ["TERM"]] if os.environ.get("TERM", "dumb") != "dumb" else [])
    + ["xterm-256color", "screen", "vt100"]))

#: The four components, one per pane. Each draws ONE line whose text is a fact about what
#: it has been handed — so `capture-pane` can tell "the event fired" from "the event did
#: not", and WHERE it landed, without the test reading any charter state.
#:
#: `acme.deaf` is the negative and it is the whole reason there is a fourth: it declares
#: `key`, the one kind charter does not deliver, so its pane must keep saying `DEAF-quiet`
#: for the entire test — including while it is the ACTIVE pane and being typed into, which
#: is when a dispatcher that ignored the declaration would light it up. It declared `click`
#: until charter began delivering that; the negative has to be a kind charter really does
#: not carry, or it stops being one.
FOCUS_CID, DEAF_CID, SIZE_CID = "acme.focus", "acme.deaf", "acme.sized"
POINT_CID = "acme.pointed"

#: What each pane says when it has painted once and nothing has reached it — the fixture's
#: settled state, and the baseline every "…only the pane it is about" case below is a
#: departure from. Named once because `setUp` reads it twice, for two different reasons
#: that are both spelled out there.
_FIRST_PAINT = ((FOCUS_CID, "FOCUS-off"), (DEAF_CID, "DEAF-quiet"),
                (SIZE_CID, "SIZE-0"), (POINT_CID, "POINT-quiet"))

_PROVIDER = textwrap.dedent("""\
    from charter.frame import component

    API_VERSION = 1

    seen = {"focus": False, "key": 0, "resize": 0, "point": ""}


    def _focus(ev):
        seen["focus"] = ev.kind == "focus"
        return True


    def focus_component():
        return component.Component(
            id="acme.focus", title="Focus", edge="bottom",
            size=component.Fixed(1), events=("focus", "blur"), on_event=_focus,
            render=lambda ctx: ["FOCUS-" + ("on" if seen["focus"] else "off")])


    def _key(ev):
        seen["key"] += 1
        return True


    def deaf_component():
        return component.Component(
            id="acme.deaf", title="Deaf", edge="bottom",
            size=component.Fixed(1), events=("key",), on_event=_key,
            render=lambda ctx: ["DEAF-" + ("hit" if seen["key"] else "quiet")])


    def _resize(ev):
        seen["resize"] += 1
        return True


    def sized_component():
        return component.Component(
            id="acme.sized", title="Sized", edge="bottom",
            size=component.Fixed(1), events=("resize",), on_event=_resize,
            render=lambda ctx: ["SIZE-%d" % seen["resize"]])


    def _point(ev):
        # The whole event, spelled out: the kind, which button or direction, and the cell
        # it landed on in THIS component's own columns. A case can then tell a click from
        # a scroll, a left from a right, and a correct translation from an off-by-a-pad.
        seen["point"] = "%s:%s:%d,%d:%s" % (
            ev.kind, ev.name or "-", ev.row, ev.col, "down" if ev.pressed else "up")
        return True


    def pointed_component():
        return component.Component(
            id="acme.pointed", title="Pointed", edge="bottom",
            size=component.Fixed(1), events=("click", "scroll"), on_event=_point,
            render=lambda ctx: ["POINT-" + (seen["point"] or "quiet")])
    """)


def _tmux(*args: str, env=None) -> subprocess.CompletedProcess:
    return subprocess.run(["tmux", "-L", SOCKET, *args], capture_output=True, text=True,
                          timeout=20, env=env)


def _await(predicate, timeout: float = _DEADLINE) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(0.05)
    return predicate()


def _fork_pty(rows: int, cols: int) -> tuple[int, int]:
    """`pty.fork`, except the pty is the size it should be BEFORE the child execs.

    `pty.fork` — and `os.forkpty` under it — takes no window size, so the pty it hands
    back comes up at the kernel's default. **Measured**, on this machine and on
    `ubuntu-latest`, that default is ``80x24``, and the session below is created at
    ``100x24``. So attaching the client made tmux resize the window 100 -> 80, and a
    resize reaches every pane in it (`test_resizing_the_window_reaches_a_component_that
    _asked_for_it` is the case that says so). `acme.sized` counts resize events, so the
    ATTACH was arriving as one:

        stock pty.fork   before attach window=100x24 SIZE-0 -> after 80x24 SIZE-1
        this            before attach window=100x24 SIZE-0 -> after 100x24 SIZE-0

    That is #648. It flaked rather than failing outright because the fixture attaches
    immediately after the four splits, so the panels are normally still importing when
    the SIGWINCH goes out and never see it — on a loaded box a panel finishes booting
    first and does, which is why the same sha passed and failed in two CI runs, why it
    moved between cases run to run, and why it went away in isolation.

    Setting the size on the slave before the fork means tmux reads it at startup rather
    than being corrected by a SIGWINCH afterwards. **The correction is the event, so
    there must not be one** — a pty resized after the exec would deliver exactly the
    signal this exists to avoid.
    """
    master, slave = os.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
    pid = os.fork()
    if pid == 0:
        os.close(master)
        # setsid, controlling terminal, and dup onto 0/1/2 — the three things `pty.fork`
        # does that this must keep doing. Available since 3.11, which is `requires-python`.
        os.login_tty(slave)
        return 0, -1
    os.close(slave)
    return pid, master


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class _AFrameWithProviderPanels(PersonaIso):
    """One frame, one attached client, four provider panels.

    The fixture, shared by both classes below rather than built twice: a tmux server, a
    client on a pty and four interpreters is the most expensive setup in this suite, and
    the focus half and the pointer half are asking about the same frame.
    """

    #: What `conf_text` is asked for. `False` is charter's own default and the regime in
    #: which the pointer acts where it points without moving the keyboard, which is what
    #: the pointer cases are about.
    MOUSE = False

    #: How big the window — and therefore the pty the client is born on — is made.
    #:
    #: A class attribute rather than a literal in `setUp` because
    #: :class:`APointerPastColumnTwoTwentyThreeLandsWhereItWasAimed` needs a window WIDER
    #: than 223 columns and everything else about that fixture is identical. 100x24 is
    #: what every case here has always run at and stays the default, so no existing case
    #: changes size to make room for the new one — `_fork_pty`'s measurement is about the
    #: client agreeing with the window, not about which size they agree on.
    COLS, ROWS = 100, 24

    def setUp(self) -> None:
        super().setUp()
        v = tmuxctl.version()
        if v is None or v < tmuxctl.FLOOR:
            self.skipTest(f"the frame's floor is tmux {tmuxctl.FLOOR[0]}."
                          f"{tmuxctl.FLOOR[1]}; this machine has {v}")
        # Registered FIRST so it runs LAST: `addCleanup` is LIFO, and the client forked
        # onto a pty must be reaped before the server it is attached to goes.
        self.addCleanup(self._teardown_socket)
        (self.tmp / "charter.toml").write_text("")

        self.fid = state.frame_id("input-integ", os.getpid())
        state.frame_dir(self.fid, create=True)
        site = self._install_provider()

        existing = os.environ.get("PYTHONPATH", "")
        parts = [str(_REPO_ROOT), str(site)] + ([existing] if existing else [])
        self.env = dict(os.environ, CHARTER_ROOT=str(self.tmp),
                        PYTHONPATH=os.pathsep.join(parts))
        self.env.pop("CHARTER_HOME", None)

        started = _tmux("-f", "/dev/null", "new-session", "-d", "-s", self.fid,
                        "-x", str(self.COLS), "-y", str(self.ROWS),
                        "-P", "-F", "#{pane_id}", "cat",
                        env=self.env)
        self.assertEqual(started.returncode, 0, started.stderr)
        self.harness = started.stdout.strip()
        state.record_server(self.fid, SOCKET)
        state.record_harness_pane(self.fid, self.harness)

        # Charter's OWN frame config, sourced the way a launch sources it — never a
        # hand-written `set -g focus-events on` standing in for it. This whole file is
        # about whether that ONE line does what #559 says, so a test that re-spelled it
        # would be measuring its own copy of charter's answer (#547).
        conf = str(self.tmp / "frame.conf")
        with open(conf, "w") as fh:
            fh.write(commands_frame.conf_text(hotkey=config.FRAME["hotkey"],
                                              mouse=self.MOUSE,
                                              history_limit=100, session=self.fid))
        sourced = _tmux("source-file", conf)
        self.assertEqual(sourced.returncode, 0, sourced.stderr)

        self.pane = {}
        for cid in (FOCUS_CID, DEAF_CID, SIZE_CID, POINT_CID):
            self.pane[cid] = self._split(cid)

        # The window's size, ASKED of tmux rather than re-spelled from the `new-session`
        # above — `_rect`'s rule (#514) one method over, and here it is load-bearing: the
        # client is given exactly this size, so attaching it is not a geometry change and
        # no pane takes a SIGWINCH for it. `_fork_pty` has the measurement.
        size = self._window_size()
        # Refused here rather than carried into `_fork_pty`, where an unreadable size is
        # a `struct.error` out of `struct.pack` — a traceback about packing an unsigned
        # short, three frames from anything that would tell a reader the session had gone.
        self.assertNotEqual(size, (-1, -1),
                            "tmux would not say how big its own window is, so the client "
                            "below cannot be born the size that window already is")
        self.fd = self._attach(size)
        # And CHECKED, rather than assumed, because everything below that reads `SIZE-N`
        # is reading a count of resize events: a window that moved while the client
        # arrived would add one, and the cases would report it as a stray focus or click.
        self.assertTrue(_await(lambda: self._window_size() == size),
                        f"attaching the client resized the window from {size} to "
                        f"{self._window_size()}, so every panel took a SIGWINCH that the "
                        f"cases below would read as an event that reached it")

        # Every panel has painted its first frame before any case touches focus, so a
        # later change can only be the event.
        for cid, expect in _FIRST_PAINT:
            self.assertTrue(_await(lambda c=cid, e=expect: e in self._shown(c)),
                            f"{cid} never painted {expect}: {self._shown(cid)!r}")
        # Read a SECOND time, now that all four have painted. The loop above returns on
        # the first poll that matches, so on its own it says "this pane showed X at some
        # instant", never "X is what it shows" — a panel that painted the expected row and
        # then took an event satisfies it and is still wrong for every case below. That is
        # the shape #648 arrived as, and this is the cheap check that names it here, in
        # the fixture, instead of somewhere downstream as a stray `SIZE-1`.
        for cid, expect in _FIRST_PAINT:
            self.assertIn(expect, self._shown(cid),
                          f"{cid} painted {expect} and then left it before the first "
                          f"case ran, so something reached it that the fixture did not do")

    # -- the frame ---------------------------------------------------------- #

    def _install_provider(self) -> Path:
        """One real distribution supplying all three components.

        A real `.dist-info` with a real `entry_points.txt`, which is what an installed
        package IS as far as `importlib.metadata` is concerned — and the panel processes
        below are separate interpreters, so nothing this process could monkey-patch would
        reach them anyway.
        """
        site = self.tmp / "site"
        site.mkdir(parents=True, exist_ok=True)
        (site / "acme_focus.py").write_text(_PROVIDER, encoding="utf-8")
        info = site / "acme_focus-1.0.dist-info"
        info.mkdir(exist_ok=True)
        (info / "METADATA").write_text(
            "Metadata-Version: 2.1\nName: acme-focus\nVersion: 1.0\n", encoding="utf-8")
        (info / "entry_points.txt").write_text(
            "[charter.components]\n"
            f"{FOCUS_CID} = acme_focus:focus_component\n"
            f"{DEAF_CID} = acme_focus:deaf_component\n"
            f"{SIZE_CID} = acme_focus:sized_component\n"
            f"{POINT_CID} = acme_focus:pointed_component\n", encoding="utf-8")
        return site

    def _split(self, cid: str) -> str:
        """One panel pane running the argv charter's own launcher would run, marked the
        way charter's own launcher marks it.

        The mark is `commands_frame._panel_mark_argv`, called rather than re-spelled, for
        the reason `conf_text` is sourced above rather than hand-written (#547): this
        fixture must not carry its own copy of charter's answer. `_split_panels` is what
        issues it in production and `tests/test_frame_launcher.py` is what pins that it
        does; here it is fixture, so that the pointer cases below are asking about tmux
        rather than about whether a pane got set up.
        """
        argv = layout.panel_command(slot=cid, session=self.fid)
        r = _tmux("split-window", "-t", self.harness, "-v", "-l", "3",
                  "-P", "-F", "#{pane_id}", "--", *argv, env=self.env)
        self.assertEqual(r.returncode, 0, r.stderr)
        pane = r.stdout.strip()
        marked = subprocess.run(
            commands_frame._panel_mark_argv(socket=SOCKET, pane_id=pane),
            capture_output=True, text=True, timeout=20)
        self.assertEqual(marked.returncode, 0, marked.stderr)
        return pane

    def _attach(self, size: tuple[int, int]) -> int:
        """Attach a real client on a real pty, *size* being the size that pty is born.

        *size* is what tmux says the window ALREADY is, passed in rather than spelled
        again here, so the client cannot arrive disagreeing with the session and make
        tmux resize the window to settle it. `_fork_pty` is where that matters and why.
        """
        cols, rows = size
        refusals = []
        for term in _TERM_CANDIDATES:
            pid, fd = _fork_pty(rows=rows, cols=cols)
            if pid == 0:
                try:
                    os.environ["TERM"] = term
                    os.environ["CHARTER_ROOT"] = str(self.tmp)
                    os.execvp("tmux", ["tmux", "-L", SOCKET, "attach", "-t", self.fid])
                finally:
                    os._exit(127)
            if _await(lambda: bool(_tmux("list-clients", "-t", self.fid,
                                         "-F", "#{client_name}").stdout.strip()),
                      timeout=10.0):
                self.addCleanup(self._reap, pid, fd)
                return fd
            refusals.append(term)
            self._reap(pid, fd)
        self.skipTest("no tmux client will attach on this machine, and focus events need "
                      "one — tried TERM=" + ", ".join(refusals))

    @staticmethod
    def _reap(pid: int, fd: int) -> None:
        for call in (lambda: os.kill(pid, 9), lambda: os.waitpid(pid, 0),
                     lambda: os.close(fd)):
            try:
                call()
            except OSError:
                pass

    def _teardown_socket(self) -> None:
        """End the server and take its socket FILE with it, in that order and in ONE
        cleanup — `test_frame_palette_integration.py::_ThePalette._teardown_socket`'s
        argument verbatim: `kill-server` is a signal rather than a wait, so the listening
        socket keeps accepting for a measured fraction of a millisecond afterwards, and a
        client that connects in that window reads EOF where a reply should be.
        """
        said = _tmux("display-message", "-p", "#{socket_path}")
        was_running = said.returncode == 0
        path = said.stdout.strip()
        _tmux("kill-server")
        for candidate in {SOCKET_PATH, path if path.startswith("/") else SOCKET_PATH}:
            try:
                os.unlink(candidate)
            except OSError:
                pass
        if not was_running:
            return
        self.assertTrue(path.startswith("/"),
                        f"tmux would not say where its socket is — `#{{socket_path}}` "
                        f"gave {path!r}, so this teardown is unlinking a guess")
        self.assertFalse(os.path.exists(path),
                         f"{path} survived this test's teardown")

    # -- reading the frame back --------------------------------------------- #

    def _shown(self, cid: str) -> str:
        return _tmux("capture-pane", "-p", "-t", self.pane[cid]).stdout

    def _select(self, pane: str) -> None:
        r = _tmux("select-pane", "-t", pane)
        self.assertEqual(r.returncode, 0, r.stderr)

    def _active(self) -> str:
        return _tmux("display-message", "-p", "#{pane_id}").stdout.strip()

    def _window_size(self) -> tuple[int, int]:
        """The WINDOW's columns and rows, asked of tmux for `_rect`'s reason (#514).

        Returned as a pair so `setUp` can compare one reading with another; a request
        that fails returns a pair no window has, which fails that comparison rather than
        raising something a caller would have to unpack.
        """
        r = _tmux("display-message", "-p", "-t", self.fid,
                  "#{window_width} #{window_height}")
        said = r.stdout.split()
        if r.returncode != 0 or len(said) != 2:
            return (-1, -1)
        return int(said[0]), int(said[1])

    def _rect(self, pane: str) -> tuple[int, int, int, int]:
        """*pane*'s place in the WINDOW: left, top, width, height — asked of tmux rather
        than computed from the splits, which is #514's rule (read geometry from tmux, do
        not derive it) and also the only way this stays right when a border row shifts
        every pane down by one."""
        r = _tmux("display-message", "-p", "-t", pane,
                  "#{pane_left} #{pane_top} #{pane_width} #{pane_height}")
        self.assertEqual(r.returncode, 0, r.stderr)
        left, top, w, h = (int(n) for n in r.stdout.split())
        return left, top, w, h

    def _inject(self, payload: bytes) -> None:
        """Put bytes on the CLIENT's terminal, which is where a reporting terminal puts
        them. tmux parses and routes from there exactly as it would a real mouse."""
        os.write(self.fd, payload)

    def _point(self, pane: str, *, row: int, col: int, button: int = 0,
               release: bool = True) -> None:
        """Aim a report at *pane*'s own cell (*row*, *col*).

        The window coordinate is the pane's origin plus the cell, and SGR is 1-based —
        so what tmux hands the program back should be exactly (*row*, *col*) again, with
        its own subtraction undone. That round trip is the point of several cases below.
        """
        left, top, _w, _h = self._rect(pane)
        wcol, wrow = left + col + 1, top + row + 1
        self._inject(b"\x1b[<%d;%d;%dM" % (button, wcol, wrow))
        if release:
            self._inject(b"\x1b[<%d;%d;%dm" % (button, wcol, wrow))

    # -- the cases ---------------------------------------------------------- #


class AFocusChangeReachesTheComponentThatOwnsThePane(_AFrameWithProviderPanels):
    """#607's half: tmux tells a pane it became, or stopped being, the active one."""

    def test_selecting_a_panels_pane_reaches_its_component_and_repaints(self):
        """The whole issue, end to end: `Component.events` is a delivery now.

        Nothing here bumps the frame's version and nothing resizes anything, so the only
        thing that can have changed what this pane shows is the event.
        """
        self._select(self.harness)
        self.assertIn("FOCUS-off", self._shown(FOCUS_CID))
        self._select(self.pane[FOCUS_CID])
        self.assertTrue(_await(lambda: "FOCUS-on" in self._shown(FOCUS_CID)),
                        f"a focus event never reached it: {self._shown(FOCUS_CID)!r}")

    def test_leaving_the_pane_reaches_it_as_a_blur(self):
        """`focus` and `blur` are two names because a component receives one or the
        other, never "a focus/blur" — so the return trip has to be its own case."""
        self._select(self.pane[FOCUS_CID])
        self.assertTrue(_await(lambda: "FOCUS-on" in self._shown(FOCUS_CID)))
        self._select(self.harness)
        self.assertTrue(_await(lambda: "FOCUS-off" in self._shown(FOCUS_CID)),
                        f"a blur never reached it: {self._shown(FOCUS_CID)!r}")

    def test_a_component_that_declared_an_undelivered_kind_receives_nothing(self):
        """`key` is declared and charter does not deliver it, so this pane must say the
        same thing while it is the ACTIVE pane — and being TYPED into — as it does while
        it is not.

        The negative that matters, and it is sharper for `key` than it was for `click`:
        the harness owns the keyboard, so the only keystrokes this pane can see are ones
        the operator typed into the wrong pane. A dispatcher that delivered them would be
        acting on exactly those.
        """
        self._select(self.pane[DEAF_CID])
        _tmux("send-keys", "-t", self.pane[DEAF_CID], "hello")
        time.sleep(1.0)
        self.assertIn("DEAF-quiet", self._shown(DEAF_CID))
        self._select(self.harness)
        time.sleep(0.5)
        self.assertIn("DEAF-quiet", self._shown(DEAF_CID))

    def test_a_focus_change_reaches_only_the_pane_it_is_about(self):
        """Two panels ask for focus reporting at once — only one of them is selected."""
        self._select(self.pane[FOCUS_CID])
        self.assertTrue(_await(lambda: "FOCUS-on" in self._shown(FOCUS_CID)))
        self.assertIn("SIZE-0", self._shown(SIZE_CID))
        self.assertIn("DEAF-quiet", self._shown(DEAF_CID))

    def test_resizing_the_window_reaches_a_component_that_asked_for_it(self):
        """A resize does not bump the frame's version — `panel.py`'s SIGWINCH section is
        the same fact for charter's own renderers. It reaches every pane, focused or not,
        which is why this case never selects the pane it asserts about.

        **The count before the resize is asserted, and that is not scene-setting.** This
        case reads a running total, so "it says SIZE-1" only means "this resize reached
        it" if it said SIZE-0 first — otherwise the row it is waiting for is one some
        earlier resize already put there, and the case passes without the `resize-window`
        below having done anything. Under #648 that is exactly what happened: the client
        attach resized the window, the panel was at SIZE-1 before this case began, and
        this assertion was satisfied by that.
        """
        self._select(self.harness)
        self.assertIn("SIZE-0", self._shown(SIZE_CID),
                      "something resized this pane before the case did, so waiting for "
                      "SIZE-1 would prove nothing about the `resize-window` below")
        r = _tmux("resize-window", "-t", self.fid, "-x", "120", "-y", "26")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertTrue(_await(lambda: "SIZE-1" in self._shown(SIZE_CID)),
                        f"a resize never reached it: {self._shown(SIZE_CID)!r}")

    def test_the_paint_that_follows_is_not_sheared_by_the_mode_charter_set(self):
        """`tty.setraw` would clear `OPOST`/`ONLCR`, and a panel joins its rows with
        `\\n` — measured, that draws a staircase. A one-row panel cannot show one, so this
        asks the pane charter set the mode on to still start at column 0."""
        self._select(self.pane[FOCUS_CID])
        self.assertTrue(_await(lambda: "FOCUS-on" in self._shown(FOCUS_CID)))
        first = self._shown(FOCUS_CID).split("\n")[0]
        self.assertTrue(first.startswith("FOCUS-on"),
                        f"the row was indented by the mode: {first!r}")


class APointerReachesTheComponentItLandedOn(_AFrameWithProviderPanels):
    """The pointer half, and every case here is a claim about tmux rather than about
    charter's arithmetic — the arithmetic is
    `tests/test_a_component_event_reaches_its_handler.py`'s.

    What is being asked is the premise the whole feature rests on: that a pointer over a
    pane which is **not** the active one is routed there anyway, in that pane's own
    coordinates, and that the keyboard does not follow it.
    """

    def _pointed(self) -> str:
        return self._shown(POINT_CID)

    def test_a_click_reaches_the_component_whose_pane_it_landed_on(self):
        """The whole of this change, end to end. Nothing bumps the frame's version and
        nothing is resized, so the only thing that can have changed this pane is the
        click."""
        self._select(self.harness)
        self._point(self.pane[POINT_CID], row=0, col=2)
        self.assertTrue(_await(lambda: "POINT-click" in self._pointed()),
                        f"a click never reached it: {self._pointed()!r}")

    def test_the_keyboard_does_not_follow_the_pointer(self):
        """**The decision this feature was allowed to ship under**, pinned against the
        thing that actually decides it.

        Charter delivers where the pointer IS and never moves focus: the frame exists to
        keep the harness the thing you type into, and a click that silently moved the
        keyboard would be a second focus disagreeing with the first. With `[frame] mouse`
        off — charter's default and this fixture's — tmux does not select the pane under
        the pointer, measured on 3.7c and at the 3.2 floor. This case is what notices the
        day that stops being true.
        """
        self._select(self.harness)
        self._point(self.pane[POINT_CID], row=0, col=2)
        self.assertTrue(_await(lambda: "POINT-click" in self._pointed()),
                        "the click never arrived, so this proves nothing about focus")
        self.assertEqual(self._active(), self.harness,
                         "a click on a panel took the keyboard off the harness")

    def test_the_component_is_told_the_cell_in_its_own_coordinates(self):
        """tmux subtracts `pane_left` and `pane_top` before the bytes arrive, so a click
        aimed at this pane's own (row 0, col 3) must come back as (0, 3) — not as the
        window coordinate it was injected at, and not as that minus a second helping of
        the same offset."""
        self._select(self.harness)
        left, top, _w, _h = self._rect(self.pane[POINT_CID])
        self.assertGreater(top, 0, "the panel is at the top of the window, so this case "
                                   "could not tell a translation from an identity")
        self._point(self.pane[POINT_CID], row=0, col=3)
        self.assertTrue(_await(lambda: "POINT-click:left:0,3:" in self._pointed()),
                        f"the cell was not its own: {self._pointed()!r}")

    def test_a_click_reaches_only_the_pane_it_landed_on(self):
        """Routing is by POSITION, and three other panels are up. The `focus` panel is the
        one that matters here: it is reading its pane too, and a report routed to the
        wrong one would land in a component that never declared the kind."""
        self._select(self.harness)
        self._point(self.pane[POINT_CID], row=0, col=1)
        self.assertTrue(_await(lambda: "POINT-click" in self._pointed()))
        self.assertIn("FOCUS-off", self._shown(FOCUS_CID))
        self.assertIn("SIZE-0", self._shown(SIZE_CID))
        self.assertIn("DEAF-quiet", self._shown(DEAF_CID))

    def test_a_click_on_the_border_is_nobodys_event(self):
        """#628 gave the rule between two panes a background, which changed how it LOOKS
        and not who owns it: a border cell is a cell in neither pane, and tmux routes a
        click there to no program at all — measured on 3.7c and 3.2, with and without a
        `pane-border-style` set. So there is no border case in `frame/events.py`, and this
        is why there does not need to be.

        **A real click is landed FIRST, and that is not scene-setting.** Asserting only
        that the border click changed nothing would pass just as well if injection were
        broken, if the panel had died, or if charter delivered no clicks at all — it is a
        negative, and a negative that cannot fail is worth nothing. So the case proves the
        route works, then proves the border is not on it.
        """
        self._select(self.harness)
        left, top, _w, _h = self._rect(self.pane[POINT_CID])
        if top == 0:
            self.skipTest("this panel has no border row above it to click on")
        self._point(self.pane[POINT_CID], row=0, col=1)
        self.assertTrue(_await(lambda: "POINT-click:left:0,1:up" in self._pointed()),
                        f"the route itself is broken: {self._pointed()!r}")
        # The row directly above this pane's first row is the rule between it and whatever
        # is above; 1-based for SGR, so `top` is exactly it. A different button, so a
        # border click that DID arrive could not be mistaken for the one above.
        self._inject(b"\x1b[<2;%d;%dM\x1b[<2;%d;%dm" % (left + 2, top, left + 2, top))
        time.sleep(1.0)
        self.assertIn("POINT-click:left:0,1:up", self._pointed(),
                      "a click on the border reached a component")

    def test_the_wheel_reaches_a_component_that_declared_scroll(self):
        """A component that declares `scroll` owns the wheel over its own pane: with the
        pane requesting reports, tmux hands the wheel to the program rather than entering
        copy-mode, and the harness pane keeps its own scrollback because it is a different
        pane."""
        self._select(self.harness)
        left, top, _w, _h = self._rect(self.pane[POINT_CID])
        self._inject(b"\x1b[<64;%d;%dM" % (left + 1, top + 1))
        self.assertTrue(_await(lambda: "POINT-scroll:up:" in self._pointed()),
                        f"the wheel never reached it: {self._pointed()!r}")
        self.assertEqual(self._active(), self.harness,
                         "the wheel took the keyboard off the harness")

    def test_the_button_a_component_is_told_is_the_button_that_was_pressed(self):
        """Right- and middle-clicks measurably reach a non-active pane, so a component
        that acts on `left` must be able to tell. `_SGR_BUTTONS` is what names them and
        this is the case that says the naming survives a real tmux rather than only a
        unit test's bytes."""
        self._select(self.harness)
        self._point(self.pane[POINT_CID], row=0, col=0, button=2)
        self.assertTrue(_await(lambda: "POINT-click:right:" in self._pointed()),
                        f"a right-click was not named as one: {self._pointed()!r}")

    def test_a_button_charter_has_no_name_for_reaches_nobody(self):
        """tmux forwards the thumb buttons (8–11, encoded as 128–131) to a pane that asked
        for reporting, verbatim — measured on 3.7c and 3.2. Read as `button & 3` alone,
        button 8 is a LEFT click and a component acting on left clicks acts on it.

        A real click is landed first, for `test_a_click_on_the_border_is_nobodys_event`'s
        reason: a negative that cannot fail is worth nothing."""
        self._select(self.harness)
        self._point(self.pane[POINT_CID], row=0, col=2)
        self.assertTrue(_await(lambda: "POINT-click:left:0,2:up" in self._pointed()),
                        f"the route itself is broken: {self._pointed()!r}")
        self._point(self.pane[POINT_CID], row=0, col=5, button=128)
        time.sleep(1.0)
        self.assertIn("POINT-click:left:0,2:up", self._pointed(),
                      "a button charter has no name for was delivered as one it does")

    def test_a_press_and_a_release_are_two_events_and_say_which(self):
        """`overlay.py` keeps no press state and a release can arrive with no press, so a
        component gets both and is told which it has — the last one drawn here is the
        release, which is what a component acting once should act on."""
        self._select(self.harness)
        self._point(self.pane[POINT_CID], row=0, col=0)
        self.assertTrue(_await(lambda: "POINT-click:left:0,0:up" in self._pointed()),
                        f"the release was not the last event seen: {self._pointed()!r}")


class APointerWithTmuxsOwnMouseOn(_AFrameWithProviderPanels):
    """The same frame with `[frame] mouse = true`, which is the regime #634 is about.

    Everything above runs at charter's default, `mouse = false`, where tmux processes no
    mouse binding at all and the pointer reaching a panel is entirely the harness's doing.
    This class is the other regime — the one an operator opts into *because* they want to
    click panels — and it is where tmux's own default `MouseDown1Pane` binding used to
    select the pane under the pointer before forwarding, so every click took the keyboard
    off the harness until they pressed `F12`.

    `conf_text` now rebinds that key, conditionally on the pane under the pointer carrying
    `_PANEL_OPTION`. **The whole point of the conditional is the last two cases**: a
    blanket `bind -n MouseDown1Pane send -M` passes the first two and breaks both of
    those, measured on 3.7c and at the 3.2 floor — it takes away clicking back to any pane
    at all, the harness included, which is worse than the focus steal it fixes.

    Nothing here is stubbed and nothing is set up by the test that the launcher does not
    set up: the config is `conf_text`'s own text and the mark is `_panel_mark_argv`'s own
    argv.
    """

    #: The only difference from the fixture above, and the setting the whole class is about.
    MOUSE = True

    def setUp(self) -> None:
        super().setUp()
        # A pane the OPERATOR split, standing for what they would get by typing their own
        # prefix-split inside charter's window. Not a charter panel: no `panel_command`,
        # no mark, nothing charter would ever do to it. Two rows, so it fits beside four
        # panels in a 24-row window.
        r = _tmux("split-window", "-t", self.harness, "-v", "-l", "2",
                  "-P", "-F", "#{pane_id}", "--", "cat", env=self.env)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.operator_pane = r.stdout.strip()
        self._select(self.harness)

    def _pointed(self) -> str:
        return self._shown(POINT_CID)

    def test_tmuxs_own_mouse_really_is_on(self):
        """Without this every case below could be passing because tmux is ignoring the
        mouse entirely — which is the fixture above's regime and proves nothing about this
        one. Read off tmux rather than off `self.MOUSE`, so a `conf_text` that stopped
        writing the line fails here instead of quietly making the rest vacuous."""
        said = _tmux("show-options", "-t", self.fid, "-v", "mouse")
        self.assertEqual(said.stdout.strip(), "on", said.stderr)

    def test_a_click_still_reaches_the_component_whose_pane_it_landed_on(self):
        """Delivery first, because every focus assertion below is worth nothing if the
        click never arrived — a keyboard that did not move because nothing happened is not
        the property being claimed."""
        self._select(self.harness)
        self._point(self.pane[POINT_CID], row=0, col=2)
        self.assertTrue(_await(lambda: "POINT-click" in self._pointed()),
                        f"a click never reached it: {self._pointed()!r}")

    def test_the_keyboard_does_not_follow_the_pointer(self):
        """#634 itself. This assertion was `active pane: THE PANEL` before the rebind, on
        3.7c and at the 3.2 floor identically, and it is the reason `[frame] mouse = true`
        was documented as hostile rather than recommended."""
        self._select(self.harness)
        self._point(self.pane[POINT_CID], row=0, col=2)
        self.assertTrue(_await(lambda: "POINT-click" in self._pointed()),
                        "the click never arrived, so this proves nothing about focus")
        self.assertEqual(self._active(), self.harness,
                         "a click on a panel took the keyboard off the harness")

    def test_clicking_the_harness_still_selects_it(self):
        """Half of what the conditional buys, and the half a blanket rebind destroys. The
        keyboard starts on a PANEL here — put there by `select-pane`, not by a click —
        so the assertion is that the click moved it back, not that it never moved."""
        self._select(self.pane[POINT_CID])
        self.assertEqual(self._active(), self.pane[POINT_CID])
        self._point(self.harness, row=0, col=1)
        self.assertTrue(_await(lambda: self._active() == self.harness, 5.0),
                        "a click on the harness no longer brings the keyboard back — "
                        "which is worse than the focus steal this rebind fixes")

    def test_clicking_a_pane_the_operator_split_still_selects_it(self):
        """The other half, and the question the issue asked third: what happens to a click
        on a pane that is neither the harness nor a charter panel. It is not charter's, so
        charter changes nothing about it — tmux's documented `select-pane -t = \\;
        send-keys -M` is what runs, and the operator's own pane behaves the way every pane
        in their own tmux does."""
        self._select(self.harness)
        self._point(self.operator_pane, row=0, col=1)
        self.assertTrue(_await(lambda: self._active() == self.operator_pane, 5.0),
                        "a pane the operator split themselves stopped taking the keyboard "
                        "on a click — tmux's own behaviour, removed from a pane charter "
                        "has nothing to do with")

    def test_a_pane_split_out_of_a_PANEL_is_not_one(self):
        """The sharpest form of the same question, and it rests on a fact about tmux
        rather than about charter: a pane option is not inherited by a pane split off the
        one carrying it. So even a pane the operator carved out of a panel is theirs, and
        clicking it selects it.

        If tmux ever started copying pane options across a split, this is the case that
        notices — and the consequence would be an operator's own pane silently refusing
        the keyboard, which nothing else here would catch."""
        r = _tmux("split-window", "-t", self.pane[POINT_CID], "-v", "-l", "1",
                  "-P", "-F", "#{pane_id}", "--", "cat", env=self.env)
        if r.returncode != 0:
            self.skipTest(f"no room to split a child out of a panel: {r.stderr.strip()}")
        child = r.stdout.strip()
        said = _tmux("display-message", "-p", "-t", child, "#{@charter_panel}")
        self.assertEqual(said.stdout.strip(), "",
                         "a pane split out of a panel inherited the panel's mark")
        self._select(self.harness)
        self._point(child, row=0, col=0)
        self.assertTrue(_await(lambda: self._active() == child, 5.0),
                        "a pane the operator carved out of a panel stopped taking the "
                        "keyboard on a click")

    def test_the_wheel_still_does_not_move_the_keyboard(self):
        """The control that has always held, on both settings and both tmux versions. A
        rebind that routed the wheel through the click's branch would break it."""
        self._select(self.harness)
        left, top, _w, _h = self._rect(self.pane[POINT_CID])
        self._inject(b"\x1b[<64;%d;%dM" % (left + 1, top + 1))
        self.assertTrue(_await(lambda: "POINT-scroll" in self._pointed()),
                        f"the wheel never reached it: {self._pointed()!r}")
        self.assertEqual(self._active(), self.harness,
                         "the wheel moved the keyboard")


class APointerPastColumnTwoTwentyThreeLandsWhereItWasAimed(_AFrameWithProviderPanels):
    """A click on the 240th column of a 244-column pane is a click on the 240th column.

    **The one terminal limit found so far that could degrade to "fires wrongly".** Every
    other one degrades to *never fires*, which `component.EVENT_KINDS` asks for and which
    costs an operator a dead affordance and nothing else. This one would hand a component
    a real column that is not the one the pointer was over — and the widest single row in
    the frame is a tab bar, so on the 244-column window this was measured for it would
    switch the operator to a **real but wrong workspace**.

    The suspicion came from tmux's own CHANGES for 3.3: *"Do not report mouse positions
    (incorrectly) above the maximum of 223"*, which says in as many words that before 3.3
    tmux got this wrong — and 3.2 is `tmuxctl.FLOOR`. 223 is where the X10 mouse encoding
    runs out: it spells a coordinate as one byte at ``value + 32``, so column 224 needs
    byte 256 and wraps.

    **Measured on the real 3.2 floor binary and on 3.7c, and the suspicion is refuted for
    charter — on the leg it was aimed at.** There are two legs and only one of them is
    charter's::

        terminal -> tmux   what the outer terminal reports, in whatever mode tmux asked
                           it for.
        tmux -> pane       what tmux synthesises for the pane's program, in whatever mode
                           the PANE asked for.

    On the first leg, at a 244-column pty with ``TERM=xterm-256color``, **tmux 3.2 asks
    the outer terminal for ``?1006h``** — unconditionally, alongside ``?1000h``/``?1002h``,
    and with no ``XM``/``xm`` capability in that terminfo entry to have asked for it
    (stock macOS ships a 2015 entry that has neither). So the terminal reports in SGR and
    no wrap is possible, on either version::

        tmux 3.2   SGR col 240 in  ->  pane read b'\\x1b[<0;240;1M'
        tmux 3.7c  SGR col 240 in  ->  pane read b'\\x1b[<0;240;1M'

    The second leg is where the CHANGES entry lives, and it is reached only by a pane that
    asks for ``1000`` **without** ``1006``. Measured with exactly such a pane::

        tmux 3.2   SGR col 240 in  ->  pane read b'\\x1b[M \\x10!'   <- WRAPPED, col 16-ish
        tmux 3.2   SGR col 244 in  ->  pane read b'\\x1b[M \\x14!'   <- WRAPPED
        tmux 3.7c  SGR col 240 in  ->  pane read b'\\x1b[M \\xff!'   <- clamped at 223
        tmux 3.7c  SGR col 244 in  ->  pane read b'\\x1b[M \\xff!'   <- clamped at 223

    So the wrap is real at the floor, and `overlay.MOUSE_ON` asking for **1006 before
    1000** is the single line that keeps charter off it. That is what
    :meth:`test_the_request_is_what_keeps_a_wide_column_honest` pins, and why it is
    spelled as a literal: the constant looks like belt-and-braces and is load-bearing on
    the floor tmux charter declares.

    **And the fallback is safe rather than merely unreached.** A terminal too old to speak
    SGR sends X10 into tmux, tmux forwards that form verbatim (`overlay.decode`'s own
    measurement), and `decode` reads the payload bytes as stray single-byte KEYS — which
    `events.DELIVERED` never delivers. Never fires, not fires wrongly.

    No refusal was written into charter for any of this, and that is the finding: a guard
    refusing clicks past column 223 on 3.2 would have refused clicks that measurably
    arrive correctly.
    """

    #: Wide enough that the interesting columns are past where X10 runs out, and the width
    #: the operator this was measured for actually runs.
    COLS, ROWS = 244, 24

    def _pointed(self) -> str:
        return self._shown(POINT_CID)

    def test_the_request_is_what_keeps_a_wide_column_honest(self):
        """`overlay.MOUSE_ON` asks for SGR, and asks for it FIRST.

        A literal, and both halves matter. Dropping ``?1006h`` puts every panel on the
        X10 leg above, where the 3.2 floor wraps a column past 223 onto a real, wrong one.
        Asking for it *after* ``?1000h`` is the same hazard with a race in front of it.
        """
        self.assertEqual(overlay.MOUSE_ON, "\x1b[?1006h\x1b[?1000h")
        self.assertLess(overlay.MOUSE_ON.index("1006"), overlay.MOUSE_ON.index("1000"),
                        "the pane asks for press/release before it asks for SGR, so tmux "
                        "may synthesise an X10 report for it — which wraps past column "
                        "223 at the 3.2 floor")

    def test_an_x10_report_reaches_no_component_at_all(self):
        """The other end of the same measurement, as an assertion rather than a paragraph.

        These are the exact bytes tmux 3.2 forwarded for a wrapped column. `decode` must
        produce no `click` from them — a `key` is fine, because `events.DELIVERED` carries
        none, and the whole point is that the wrap can only ever be silence.
        """
        for payload in (b"\x1b[M \x105", b"\x1b[M \x145", b"\x1b[M \xff!"):
            with self.subTest(payload=payload):
                evs, rest = overlay.decode(payload, final=True)
                self.assertEqual(rest, b"")
                self.assertEqual([e for e in evs if e.kind != overlay.KEY], [],
                                 f"a wrapped X10 report became a pointer event: {evs!r}")

    def test_a_click_past_two_hundred_and_twenty_three_is_the_column_it_was_aimed_at(self):
        """The whole finding, end to end, through a real tmux on a real 244-column pty.

        Aimed at four columns rather than one, because the failure this is about is an
        arithmetic one and a single sample cannot tell "the column survived" from "the
        column happened to be right". 100 is the control below the boundary; 223 is the
        last column X10 can spell; 224 is the first it cannot; and the last column of the
        pane is where the operator's `+N` sits on a full-width tab bar.
        """
        self._select(self.harness)
        left, _top, width, _h = self._rect(self.pane[POINT_CID])
        self.assertEqual(left, 0, "this pane does not start at the window's left edge, so "
                                  "the columns below are not the ones being asked about")
        self.assertGreater(width, 223,
                           f"this pane is {width} columns wide, so nothing here is past "
                           f"the boundary the case is about")
        for col in (100, 222, 223, width - 1):
            with self.subTest(col=col):
                self._point(self.pane[POINT_CID], row=0, col=col)
                want = "POINT-click:left:0,%d:up" % col
                self.assertTrue(_await(lambda w=want: w in self._pointed()),
                                f"a click aimed at column {col} of a {width}-column pane "
                                f"arrived somewhere else: {self._pointed()!r}")


if __name__ == "__main__":
    unittest.main()
