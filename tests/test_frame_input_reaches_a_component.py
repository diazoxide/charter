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
  `scroll` at all, and the one this file exists to keep true.

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
    mouse ON
      click over the panel                -> panel receives it            active: PANEL

and, for the tty mode, one pane per mode painting three ``\\n``-joined rows::

    cooked  ['AAA','BBB','CCC']   raw  ['AAA','   BBB','      CCC']   cbreak  ['AAA','BBB','CCC']

**The pointer cases INJECT reports into the client's pty rather than moving a mouse**, and
that is a deliberate separation rather than a shortcut: it makes these cases about where
tmux ROUTES a report, which is what charter depends on, and leaves *whether the terminal
sends one at all* — the `[frame] mouse` question, which charter does not control — to the
documentation and to `frame/events.py`'s own measurements. A case that needed a physical
mouse could assert neither.

`tests/test_a_component_event_reaches_its_handler.py` is the unit half; it can say what
the dispatcher does with a byte, and cannot say whether the byte ever arrives.
"""

from __future__ import annotations

import os
import pty
import shutil
import subprocess
import sys
import textwrap
import time
import unittest
from pathlib import Path

from charter import commands_frame, config
from charter.frame import layout, state, tmuxctl

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
                        "-x", "100", "-y", "24", "-P", "-F", "#{pane_id}", "cat",
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
        self.fd = self._attach()
        # Every panel has painted its first frame before any case touches focus, so a
        # later change can only be the event.
        for cid, expect in ((FOCUS_CID, "FOCUS-off"), (DEAF_CID, "DEAF-quiet"),
                            (SIZE_CID, "SIZE-0"), (POINT_CID, "POINT-quiet")):
            self.assertTrue(_await(lambda c=cid, e=expect: e in self._shown(c)),
                            f"{cid} never painted {expect}: {self._shown(cid)!r}")

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
        """One panel pane running the argv charter's own launcher would run."""
        argv = layout.panel_command(slot=cid, session=self.fid)
        r = _tmux("split-window", "-t", self.harness, "-v", "-l", "3",
                  "-P", "-F", "#{pane_id}", "--", *argv, env=self.env)
        self.assertEqual(r.returncode, 0, r.stderr)
        return r.stdout.strip()

    def _attach(self) -> int:
        refusals = []
        for term in _TERM_CANDIDATES:
            pid, fd = pty.fork()
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
                self._drain(fd)
                return fd
            refusals.append(term)
            self._reap(pid, fd)
        self.skipTest("no tmux client will attach on this machine, and focus events need "
                      "one — tried TERM=" + ", ".join(refusals))

    @staticmethod
    def _drain(fd: int) -> None:
        """Read and discard everything the attached client paints, for as long as it runs.

        **This file is the first here to use a pty in BOTH directions, and only one of them
        had a reader.** tmux writes the client's whole screen to the master and `_inject`
        writes mouse reports back down it. Nothing consumed the screen: the focus cases
        never had to, because they read the frame through `capture-pane`, which asks the
        SERVER and not the client. Four panels repainting for the life of a case put far
        more through that direction than a pty buffers, and a tmux client blocked writing
        its screen is a tmux client not reading its input — so the reports `_inject` sends
        would sit in a queue nobody empties, and the case would fail having proved nothing
        about routing.

        Stated as prevention rather than as a fix: it has not been seen, here or on CI,
        because a case lives about a second and the buffer does not fill that fast. It is
        a race whose losing side is a flake on a slower or busier machine than this one,
        and one draining thread is a great deal cheaper than diagnosing it there.

        A daemon thread, so a case that fails before its cleanup cannot leave one holding
        the process open.
        """
        import threading

        def pump():
            while True:
                try:
                    if not os.read(fd, 65536):
                        return
                except OSError:
                    return

        threading.Thread(target=pump, daemon=True).start()

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
        which is why this case never selects the pane it asserts about."""
        self._select(self.harness)
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


if __name__ == "__main__":
    unittest.main()
