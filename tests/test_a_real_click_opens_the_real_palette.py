"""A real click on `F2 palette`, and on a real persona row, in a REAL tmux — #742, #751.

The reports these answer are *"I clicked the one thing the frame advertises and nothing
happened"* and *"I clicked a persona and the active persona did not change"*, and nothing
about either could have been settled by a unit test:
`tests/test_the_strips_and_the_roster_answer_a_click.py` says which column is a door and
which row is which persona; only this can say that a report injected into a real client's
terminal becomes a palette pane and a moved `▸` at all.

Everything here is real: a real tmux server, a real attached client on a real pty, real
`charter panel` processes each holding their own pane, the frame's own private config
sourced from `commands_frame.conf_text`, and — for the palette case — a real detached
`charter frame-palette` that carves a real pane off the harness. What that buys over the
unit half, link by link:

* that `charter panel bottom` and `charter panel right` build a dispatcher at all, which
  they did not before this change: the two strips and the sidebar declared no events, so
  `panel._run` opened no input path, wrote no `overlay.MOUSE_ON`, and left the tty alone;
* that the pane really asks its terminal to report (a process nobody in this test can
  monkey-patch writing to fd 1);
* that tmux routes the report to the pane under the pointer, **in that pane's own cells**,
  and leaves the keyboard on the harness — the pane a click on a DOOR then moves it to is
  the palette's, which is what `F2` does and is the whole of what the hint promises;
* that the persona switch reaches the OTHER pane: the sidebar is one process and the
  switch is another, and `▸` moving is the only proof the two are talking through
  `persona.set_active` and `state.bump` rather than through an object one interpreter has.

**`mouse = true`, because that is the regime this feature is for.** With charter's flag
off, tmux asks the terminal to report from the ACTIVE pane's own request alone, and the
active pane here is a `cat` — so nothing would be sent, nothing would arrive, and a green
test would be measuring an empty room. `docs/frame.md` states that half to the operator.

**A socket per TEST**, which is
`tests/test_a_real_click_on_a_real_tab_bar_switches.py`'s measurement and matters more
here: a click starts a DETACHED charter that goes on making tmux calls after the case has
finished asserting, and on a shared socket the next case's panes inherit ids the previous
case's child is still aiming at.
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
from charter.frame import layout, state, tmuxctl

from tests import _tmuxreap
from tests._isolation import PersonaIso, make_plane

_HAS_TMUX = shutil.which("tmux") is not None

_REPO_ROOT = Path(__file__).resolve().parents[1]

_SERVERS = itertools.count()

_DEADLINE = 30.0

_TERM_CANDIDATES = tuple(dict.fromkeys(
    ([os.environ["TERM"]] if os.environ.get("TERM", "dumb") != "dumb" else [])
    + ["xterm-256color", "screen", "vt100"]))

#: Wide enough for the attention row to keep every field it has, so the `hotkey` field —
#: the LAST one `slots._fit_fields` keeps, and therefore the first a narrow row drops — is
#: actually drawn. A case run below that width would be measuring the absence.
COLS = 120

#: Rows the sidebar pane is split with: the heading and three persona rows, with room to
#: spare so `_cap_personas` draws no `…(+N more)` — that row is the unit half's case.
SIDEBAR_ROWS = 8


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
    resize the window, which is a SIGWINCH in every pane — a repaint no case here asked
    for, which is exactly the noise these cases have to be free of.
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


class _ARealFrameWithStrips(PersonaIso):
    """One real frame with charter's OWN `attention` and `sidebar` panels and a pointer.

    **`make_plane` and not merely `PersonaIso`'s root**: both gestures under test start a
    CHILD charter that resolves its own plane, and `PersonaIso` writes no `charter.toml`,
    so such a child would go looking for a plane of its own and find the operator's live
    one (#527).
    """

    #: The persona the frame starts on and the one it is clicked onto. Literals, so a case
    #: cannot be satisfied by whatever the plane happens to contain.
    HERE = "steward"
    THERE = "docs"

    def setUp(self) -> None:
        super().setUp()
        v = tmuxctl.version()
        if v is None or v < tmuxctl.FLOOR:
            self.skipTest(f"the frame's floor is tmux {tmuxctl.FLOOR[0]}."
                          f"{tmuxctl.FLOOR[1]}; this machine has {v}")
        self.socket = _tmuxreap.name(f"doors{next(_SERVERS)}")
        self.socket_path = os.path.join(os.environ.get("TMUX_TMPDIR") or "/tmp",
                                        f"tmux-{os.getuid()}", self.socket)
        self.addCleanup(self._teardown_socket)
        self.plane = make_plane(self, "schema = 1\n")
        for name in (self.HERE, self.THERE):
            self.make_persona(name)
        # **One persona with a real badge on it, because #753's half of this file needs a
        # glyph to click.** `draft: true` is what `charter persona create` stamps and what
        # `persona.is_draft` reads, so `⚑` here is the badge an ordinary un-finished
        # persona carries rather than a string this fixture painted — which is the whole
        # point: `make_persona` alone produces a HEALTHY persona and no badge column at
        # all, so a version of this file without this line was measuring a pane that had
        # nothing to explain.
        self.make_persona("forge", draft="true")
        (config.WORKSPACES_DIR / "alpha").mkdir(parents=True, exist_ok=True)

        self.fid = state.frame_id("doors", os.getpid())
        state.frame_dir(self.fid, create=True)

        existing = os.environ.get("PYTHONPATH", "")
        parts = [str(_REPO_ROOT)] + ([existing] if existing else [])
        # **`$CHARTER_SESSION_ID` is in the environment the SERVER is started with**, which
        # is how every pane tmux later spawns inherits it — the same rung
        # `commands_frame._session_id_env_argv` writes with `set-environment`, and the one
        # `persona.resolve_active` reads to answer "who is this frame being" (ADR 0019).
        # Without it the sidebar child resolves a persona for no session and draws every
        # row idle, so the marker this file measures would never be anywhere to move from.
        self.env = dict(os.environ, CHARTER_ROOT=str(self.plane),
                        CHARTER_SESSION_ID=self.fid,
                        PYTHONPATH=os.pathsep.join(parts))
        self.env.pop("CHARTER_HOME", None)
        # **`$CHARTER_PERSONA` is deliberately absent.** It is a PIN: `switch.to_persona`
        # refuses outright when the launch carried one, and that value would sit in every
        # panel pane's environment — so a fixture that set it would make the persona case
        # below measure the refusal instead of the switch.
        self.env.pop("CHARTER_PERSONA", None)
        self.env.pop("CHARTER_WORKSPACE", None)

        self.harness = self._start_session(self.fid)
        self._tmux("set-environment", "-t", self.fid, "CHARTER_SESSION_ID", self.fid)
        state.record_server(self.fid, self.socket)
        state.record_harness_pane(self.fid, self.harness)
        self._source_conf(self.fid)

    # -- the frame ---------------------------------------------------------- #

    def _tmux(self, *args: str, env=None) -> subprocess.CompletedProcess:
        return subprocess.run(["tmux", "-L", self.socket, *args], capture_output=True,
                              text=True, timeout=20, env=env)

    def _start_session(self, fid: str) -> str:
        started = self._tmux("-f", "/dev/null", "new-session", "-d", "-s", fid,
                             "-x", str(COLS), "-y", "24", "-P", "-F", "#{pane_id}", "cat",
                             env=self.env)
        self.assertEqual(started.returncode, 0, started.stderr)
        return started.stdout.strip()

    def _source_conf(self, fid: str) -> None:
        """charter's OWN frame config, `mouse = true`.

        Asking `conf_text` for the text rather than writing `set mouse on` by hand is what
        keeps the `MouseDown1Pane` rebind (#634) under test — a hand-written config would
        leave tmux's default binding in place and "the keyboard stays on the harness"
        would be measuring a config this file invented.
        """
        conf = str(self.tmp / f"{fid}.conf")
        with open(conf, "w") as fh:
            fh.write(commands_frame.conf_text(hotkey=config.FRAME["hotkey"], mouse=True,
                                              history_limit=100, session=fid))
        sourced = self._tmux("source-file", conf)
        self.assertEqual(sourced.returncode, 0, sourced.stderr)

    def _split_panel(self, slot: str, *, flag: str, size: int) -> str:
        """One real `charter panel <slot>` pane, marked the way its launcher marks it.

        `layout.panel_command` and `commands_frame._panel_mark_argv` are called rather
        than re-spelled: that argv is what the launcher really sends, and that mark is what
        the `MouseDown1Pane` bind asks about before it decides whether a click may move the
        keyboard.
        """
        argv = layout.panel_command(slot=slot, session=self.fid)
        r = self._tmux("split-window", "-t", self.harness, flag, "-l", str(size),
                       "-P", "-F", "#{pane_id}", "--", *argv, env=self.env)
        self.assertEqual(r.returncode, 0, r.stderr)
        pane = r.stdout.strip()
        marked = subprocess.run(
            commands_frame._panel_mark_argv(socket=self.socket, pane_id=pane),
            capture_output=True, text=True, timeout=20)
        self.assertEqual(marked.returncode, 0, marked.stderr)
        return pane

    def _bring_up(self, *panels) -> None:
        """Split the named panels, put the harness back in front, and attach a client.

        **The harness is put back in front, and that is the regime rather than tidiness.**
        `split-window` selects the pane it made, and a frame whose strip was the ACTIVE
        pane would be reporting the mouse because THAT pane asked — the `mouse = false`
        route, which would make every case below pass with charter's flag off.
        `commands_frame._split_panels` ends the same way for the same reason.
        """
        for slot, flag, size in panels:
            setattr(self, slot, self._split_panel(slot, flag=flag, size=size))
        selected = self._tmux("select-pane", "-t", self.harness)
        self.assertEqual(selected.returncode, 0, selected.stderr)
        size = self._window_size()
        self.assertNotEqual(size, (-1, -1), "tmux would not say how big its window is")
        self.fd = self._attach(size)
        self.assertEqual(self._active(), self.harness,
                         "the harness is not the active pane, so a report reaching a "
                         "panel would prove nothing about `[frame] mouse`")

    def _attach(self, size) -> int:
        cols, rows = size
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
            if _await(lambda: bool(self._tmux("list-clients", "-t", self.fid,
                                              "-F", "#{client_name}").stdout.strip()),
                      timeout=10.0):
                self.addCleanup(self._reap, pid, fd)
                self.assertTrue(
                    _await(lambda: self._window_size() == size),
                    "attaching the client resized the window, so every panel took a "
                    "SIGWINCH the cases below would read as an event")
                return fd
            refusals.append(term)
            self._reap(pid, fd)
        self.skipTest("no tmux client will attach on this machine, and a pointer needs "
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

    def _shown(self, pane: str) -> str:
        return self._tmux("capture-pane", "-p", "-t", pane).stdout

    def _row(self, pane: str, n: int = 0) -> str:
        rows = self._shown(pane).split("\n")
        return rows[n].rstrip() if n < len(rows) else ""

    def _window_size(self) -> tuple[int, int]:
        r = self._tmux("display-message", "-p", "-t", self.fid,
                       "#{window_width} #{window_height}")
        said = r.stdout.split()
        if r.returncode != 0 or len(said) != 2:
            return (-1, -1)
        return int(said[0]), int(said[1])

    def _rect(self, pane: str) -> tuple[int, int]:
        r = self._tmux("display-message", "-p", "-t", pane, "#{pane_left} #{pane_top}")
        self.assertEqual(r.returncode, 0, r.stderr)
        left, top = (int(n) for n in r.stdout.split())
        return left, top

    def _active(self) -> str:
        return self._tmux("display-message", "-p", "#{pane_id}").stdout.strip()

    def _panes(self) -> list[str]:
        return self._tmux("list-panes", "-t", self.fid,
                          "-F", "#{pane_id}").stdout.split()

    def _click(self, pane: str, *, col: int, row: int = 0, button: int = 0) -> None:
        """One left press and its release, at the pane's own cell, through the client.

        Both halves are sent because that is what a terminal sends. Only the press is
        acted on — every one of charter's handlers reads §4i the same way — and sending
        only the press would leave this measuring a gesture no mouse makes.
        """
        left, top = self._rect(pane)
        os.write(self.fd, b"\x1b[<%d;%d;%dM" % (button, left + col + 1, top + row + 1))
        os.write(self.fd, b"\x1b[<%d;%d;%dm" % (button, left + col + 1, top + row + 1))

    def _column_of(self, pane: str, field: str, row: int = 0) -> int:
        """Which column of *pane* the drawn *field* starts in — read off the PANE.

        Off the pane and never off `slots.DOORS`: that object lives in the panel's own
        process and not in this one, and asking charter where it put something and then
        clicking there would be a test that agrees with itself. This is the column the
        operator's eye lands on.
        """
        drawn = self._row(pane, row)
        at = drawn.index(field)
        self.assertNotIn(field, drawn[at + 1:], f"{field!r} is not unique in {drawn!r}")
        return at

    def _row_of(self, pane: str, name: str) -> int:
        """Which row of *pane* draws persona *name* — again read off the pane."""
        rows = self._shown(pane).split("\n")
        hits = [i for i, r in enumerate(rows) if f" {name}" in r]
        self.assertEqual(len(hits), 1, f"{name!r} is not on exactly one row of {rows!r}")
        return hits[0]


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class ARealClickOnTheHotkeyHintOpensTheRealPalette(_ARealFrameWithStrips,
                                                   unittest.TestCase):
    """`F2 palette` is the only affordance charter advertises, and it is a button now.

    **Three processes and none of them is this one.** The row is painted by a `charter
    panel bottom` child, the click is decoded there, and the palette is carved by a
    detached `charter frame-palette` that child started. What this asserts is what the
    operator sees: a pane appears, it is the one their keyboard is now in, and it is the
    palette.
    """

    def setUp(self) -> None:
        super().setUp()
        self._bring_up(("bottom", "-v", 1))
        self.assertTrue(
            _await(lambda: "palette" in self._row(self.bottom)),
            f"the attention row never drew the hint: {self._row(self.bottom)!r}")

    def test_clicking_the_hint_opens_a_palette_pane_and_the_keyboard_follows_it(self):
        """What pressing `F2` does, reached with the pointer instead.

        The keyboard moving is not the pointer moving it — `overlay.modal_argvs` makes the
        palette's pane the active one deliberately, because a modal surface you cannot
        type into is not one. `docs/frame.md`'s promise that a click on a PANEL leaves the
        keyboard on the harness is kept and is asserted separately below: what moved the
        keyboard here is the palette opening, exactly as it does from the key.
        """
        before = self._panes()
        self.assertEqual(self._active(), self.harness)
        self._click(self.bottom, col=self._column_of(self.bottom, "palette"))
        self.assertTrue(
            _await(lambda: len(self._panes()) > len(before)),
            "no pane was carved off the harness, so the palette never opened")
        opened = [p for p in self._panes() if p not in before]
        self.assertEqual(len(opened), 1, f"expected one new pane, got {opened}")
        self.assertTrue(_await(lambda: self._active() == opened[0]),
                        "the palette opened and the keyboard did not follow it")
        self.assertTrue(
            _await(lambda: "workspace" in self._shown(opened[0])
                   or "persona" in self._shown(opened[0])),
            f"the new pane is not the palette: {self._shown(opened[0])!r}")

    def test_clicking_the_todo_count_beside_it_opens_nothing(self):
        """The control for the case above, and the half a feature like this gets wrong: a
        row that answers EVERY click is as wrong as one that answers none. The todo count
        is a readout — the frame reporting, not offering.
        """
        before = self._panes()
        self._click(self.bottom, col=self._column_of(self.bottom, "todo"))
        time.sleep(1.5)
        self.assertEqual(self._panes(), before,
                         "a click on the todo count opened something")
        self.assertEqual(self._active(), self.harness,
                         "and it moved the keyboard off the harness")

    def test_a_click_on_the_separator_before_the_hint_opens_nothing(self):
        """The ` · ` belongs to neither field, and charter will not pick the nearer one —
        `slots._Tabs` refuses its own `_BAR_GAP` for the identical reason.

        The column is the one immediately left of the hint rather than the first ` · ` on
        the row: this plane draws an alert, so the row has several, and a case that clicked
        whichever came first would be clicking a different cell on a plane that has none.
        """
        before = self._panes()
        at = self._column_of(self.bottom, "F2 palette")
        self.assertEqual(self._row(self.bottom)[at - 3:at], " · ")
        self._click(self.bottom, col=at - 2)
        time.sleep(1.5)
        self.assertEqual(self._panes(), before)

    def test_a_release_with_no_press_opens_nothing(self):
        """§4i, measured end to end: `frame/overlay.py` recorded a drag begun on a pane
        border delivering exactly one release, and every charter handler acts on the press
        for that reason. This sends only the unpaired half a real drag delivers.
        """
        before = self._panes()
        left, top = self._rect(self.bottom)
        col = self._column_of(self.bottom, "palette")
        os.write(self.fd, b"\x1b[<0;%d;%dm" % (left + col + 1, top + 1))
        time.sleep(1.5)
        self.assertEqual(self._panes(), before,
                         "a release with no press opened the palette")


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class ARealClickOnARealPersonaRowSwitchesTheFrame(_ARealFrameWithStrips,
                                                  unittest.TestCase):
    """The sidebar's roster: click a persona, the frame is being that persona.

    The proof is `▸` moving on the pane, which is a fact about THREE processes — the
    sidebar that drew it, the detached `charter frame-switch --persona` the click started,
    and `persona.set_active` writing where both can see it. An in-process assertion would
    prove none of that.
    """

    #: Split alongside the sidebar so the legend has somewhere to land. `bottom` is a
    #: THIRD process: the click is decoded in the sidebar's, `state.say` writes the frame's
    #: notice file and bumps the version, and this pane draws it on its own next poll —
    #: which is the only thing that can show the two are talking through the frame's state
    #: rather than through an object one interpreter has.
    PANELS = (("right", "-h", 22), ("bottom", "-v", 1))

    def setUp(self) -> None:
        super().setUp()
        self._bring_up(*self.PANELS)
        self.assertTrue(
            _await(lambda: "▪ personas" in self._shown(self.right)),
            f"the sidebar never painted: {self._shown(self.right)!r}")
        # **The frame is put on `HERE` as a FIXTURE, in this process, not by running the
        # thing under test.** `persona.set_active` keyed on the frame's own id is exactly
        # what `switch.to_persona` calls, and `state.bump` is what it ends with — so the
        # sidebar child sees it through the same two writes the click will use, and the
        # case below is still measuring the click rather than a second run of itself.
        #
        # Stated rather than assumed, because the case below asserts the marker MOVES: it
        # has to know which row carries `▸` before the click, and "some persona" is not a
        # starting point a `▸ {THERE}` assertion can be read against. It is no longer a
        # statement about the column's ORDER — `_persona_chip_cells` used to lift the
        # active persona to the top and does not since #882 (`persona.by_use`), which is
        # also why `_row_of` looks the row up by name instead of assuming an index.
        from charter import persona as p_mod
        p_mod.set_active(self.HERE, session_id=self.fid, terminal_id="")
        state.bump(self.fid)
        self.assertTrue(
            _await(lambda: f"▸ {self.HERE}" in self._shown(self.right)),
            f"the frame did not start on {self.HERE}: {self._shown(self.right)!r}")

    def test_clicking_an_idle_persona_moves_the_marker_to_it(self):
        at = self._row_of(self.right, self.THERE)
        self._click(self.right, col=3, row=at)
        self.assertTrue(
            _await(lambda: f"▸ {self.THERE}" in self._shown(self.right)),
            f"the frame never adopted {self.THERE}: {self._shown(self.right)!r}")
        self.assertNotIn(f"▸ {self.HERE}", self._shown(self.right),
                         "two personas are marked active")

    def test_the_keyboard_stays_on_the_harness(self):
        """`docs/frame.md`'s promise, and the one #634 had to rebind `MouseDown1Pane` to
        keep once `mouse = true`: a click on a PANEL acts where you pointed and leaves your
        typing where it was. A persona switch is not a surface, so nothing about it should
        move the keyboard the way opening the palette deliberately does.
        """
        at = self._row_of(self.right, self.THERE)
        self._click(self.right, col=3, row=at)
        self.assertTrue(_await(lambda: f"▸ {self.THERE}" in self._shown(self.right)))
        self.assertEqual(self._active(), self.harness)

    def test_clicking_the_badge_column_puts_the_legend_on_the_attention_row(self):
        """#753, end to end and across two panes: click the `⚑` on a persona row and the
        frame itself says what the glyph means.

        The column is found by looking for the glyph ON THE PANE rather than by asking
        `slots.CHIPS` where it put the badges — that object lives in the sidebar's process
        and not in this one, and a test that asked charter where it drew something and then
        clicked there would be a test that agrees with itself.
        """
        rows = self._shown(self.right).split("\n")
        hits = [(i, r.rindex("⚑")) for i, r in enumerate(rows) if "⚑" in r]
        self.assertTrue(hits, f"no persona carries a badge to click: {rows!r}")
        row, col = hits[0]
        self.assertNotIn("no usable vault", self._shown(self.bottom))
        self._click(self.right, col=col, row=row)
        self.assertTrue(
            _await(lambda: "no usable vault" in self._shown(self.bottom)),
            f"the legend never reached the attention row: "
            f"{self._shown(self.bottom)!r}")
        self.assertEqual(self._active(), self.harness,
                         "explaining a glyph moved the keyboard")

    def test_clicking_the_badge_column_switches_no_persona(self):
        """The other half of the two-cell rule, on a real pane: the row that was clicked
        must still be the row it was, and the marker must not have moved."""
        rows = self._shown(self.right).split("\n")
        hits = [(i, r.rindex("⚑")) for i, r in enumerate(rows) if "⚑" in r]
        self.assertTrue(hits)
        row, col = hits[0]
        self._click(self.right, col=col, row=row)
        self.assertTrue(_await(lambda: "no usable vault" in self._shown(self.bottom)))
        self.assertIn(f"▸ {self.HERE}", self._shown(self.right),
                      "a click on the badges adopted a persona")

    def test_clicking_the_heading_switches_nothing(self):
        """`▪ personas 3` names no persona, so it is absent from the map rather than
        guarded against — `slots._Chips`' structural bounds check."""
        self._click(self.right, col=3, row=0)
        time.sleep(1.5)
        self.assertIn(f"▸ {self.HERE}", self._shown(self.right),
                      "a click on the heading moved the marker")

    def test_clicking_the_persona_you_already_are_switches_nothing(self):
        """`slots._Chips.switch_to`'s rule, end to end: re-adopting is not news, and this
        is also what stops a double-click switching twice."""
        at = self._row_of(self.right, self.HERE)
        self._click(self.right, col=3, row=at)
        time.sleep(1.5)
        self.assertIn(f"▸ {self.HERE}", self._shown(self.right))


if __name__ == "__main__":
    unittest.main()
