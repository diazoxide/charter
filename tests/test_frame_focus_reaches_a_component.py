"""A focus change in a REAL tmux pane reaches a REAL provider component (#607).

Everything here is real: a real tmux server, a real attached client on a real pty, a real
installed distribution on `sys.path`, and three real `charter panel <component-id>`
processes each holding their own pane. Nothing is stubbed, because every claim this branch
makes that could be wrong is a claim about tmux:

* that tmux delivers ``\\x1b[I``/``\\x1b[O`` to a pane's PROGRAM at all;
* that it delivers them to a pane that asked and to no other;
* that a panel can read them off the descriptor `frame/pane.py` claims (fd 1), with the
  pane's `stdin` never consulted;
* that the mode `frame/events.py` sets does not shear the paint that follows;
* that a `resize-window` reaches every pane, focused or not.

The measurements this file is the standing form of, taken against **tmux 3.7c** before a
line of the dispatcher was written::

    focus-events on   pane that asked      -> READ b'\\x1b[I' on select, b'\\x1b[O' on leave
                      pane that did not    -> nothing, ever
    focus-events off  pane that asked      -> nothing, ever
    resize-window     every pane           -> SIGWINCH, focused or not

and, for the tty mode, one pane per mode painting three ``\\n``-joined rows::

    cooked  ['AAA','BBB','CCC']   raw  ['AAA','   BBB','      CCC']   cbreak  ['AAA','BBB','CCC']

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

#: The three components, one per pane. Each draws ONE line whose text is a fact about what
#: it has been handed — so `capture-pane` can tell "the event fired" from "the event did
#: not" without the test reading any charter state.
#:
#: `acme.deaf` is the negative and it is the whole reason there are three: it declares
#: `click`, which charter does not deliver, so its pane must keep saying `DEAF-quiet` for
#: the entire test — including while it is the ACTIVE pane, which is when a dispatcher that
#: ignored the declaration would light it up.
FOCUS_CID, DEAF_CID, SIZE_CID = "acme.focus", "acme.deaf", "acme.sized"

_PROVIDER = textwrap.dedent("""\
    from charter.frame import component

    API_VERSION = 1

    seen = {"focus": False, "click": 0, "resize": 0}


    def _focus(ev):
        seen["focus"] = ev.kind == "focus"
        return True


    def focus_component():
        return component.Component(
            id="acme.focus", title="Focus", edge="bottom",
            size=component.Fixed(1), events=("focus", "blur"), on_event=_focus,
            render=lambda ctx: ["FOCUS-" + ("on" if seen["focus"] else "off")])


    def _click(ev):
        seen["click"] += 1
        return True


    def deaf_component():
        return component.Component(
            id="acme.deaf", title="Deaf", edge="bottom",
            size=component.Fixed(1), events=("click",), on_event=_click,
            render=lambda ctx: ["DEAF-" + ("hit" if seen["click"] else "quiet")])


    def _resize(ev):
        seen["resize"] += 1
        return True


    def sized_component():
        return component.Component(
            id="acme.sized", title="Sized", edge="bottom",
            size=component.Fixed(1), events=("resize",), on_event=_resize,
            render=lambda ctx: ["SIZE-%d" % seen["resize"]])
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
class AFocusChangeReachesTheComponentThatOwnsThePane(PersonaIso):
    """One frame, one attached client, three provider panels."""

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

        self.fid = state.frame_id("focus-integ", os.getpid())
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
            fh.write(commands_frame.conf_text(hotkey=config.FRAME["hotkey"], mouse=False,
                                              history_limit=100, session=self.fid))
        sourced = _tmux("source-file", conf)
        self.assertEqual(sourced.returncode, 0, sourced.stderr)

        self.pane = {}
        for cid in (FOCUS_CID, DEAF_CID, SIZE_CID):
            self.pane[cid] = self._split(cid)
        self.fd = self._attach()
        # Every panel has painted its first frame before any case touches focus, so a
        # later change can only be the event.
        for cid, expect in ((FOCUS_CID, "FOCUS-off"), (DEAF_CID, "DEAF-quiet"),
                            (SIZE_CID, "SIZE-0")):
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
            f"{SIZE_CID} = acme_focus:sized_component\n", encoding="utf-8")
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

    # -- the cases ---------------------------------------------------------- #

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
        """`click` is declared and charter does not deliver it, so this pane must say the
        same thing while it is the ACTIVE pane as it does while it is not.

        The negative that matters: a dispatcher that opened the input path for any
        declaration at all would light this up on the focus report tmux sends the moment
        it is selected — which is a `click` firing on something that was not a click.
        """
        self._select(self.pane[DEAF_CID])
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


if __name__ == "__main__":
    unittest.main()
