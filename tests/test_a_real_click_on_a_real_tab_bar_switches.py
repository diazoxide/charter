"""A click on a REAL tab bar, in a REAL tmux, moves the frame — and leaves the keyboard.

The report this answers is *"I put both bars on my plane, clicked a tab, and nothing
happened."* Nothing about that could have been settled by a unit test: every link in it is
a claim about tmux or about a separate process.
`tests/test_a_click_on_a_tab_bar_switches.py` says what the handler does with an event and
`tests/test_frame_bars.py` says what the renderer publishes; only this can say that a
report injected into a client's terminal becomes a switch at all.

Everything here is real: a real tmux server, a real attached client on a real pty, real
`charter panel` processes holding their own panes, the frame's own private config sourced
from `commands_frame.conf_text` rather than hand-written, and — the link no fake can stand
in for — a real detached `charter frame-switch` / `charter frame-chat` child, started by
the panel process out of its own event handler. What that buys, link by link:

* that `charter panel workspaces` builds a dispatcher for a bar at all, which it did not
  before this change: both bars declared `events = ()`, so `events.wanted` answered `()`,
  `panel._run` built no dispatcher, and the pane's terminal was never even asked to report;
* that the pane really asks (`overlay.MOUSE_ON` written to fd 1 by a process nobody in
  this test can monkey-patch);
* that tmux routes the report to the pane under the pointer, in that pane's own cells, and
  **leaves the keyboard on the harness** — #634's `MouseDown1Pane` rebind, which is what
  makes a clickable bar something an operator can use rather than a trap;
* that the handler's `_spawn` really starts a charter that really performs the switch, in
  a plane the child resolves for itself — a link that is three processes long and whose
  failure mode (`No module named charter`, a plane resolved from a cwd) is silent.

**`mouse = true`, because that is the regime this feature is for.** With charter's flag
off, tmux asks the terminal to report from the ACTIVE pane's own request alone, and the
active pane here is a `cat` — so nothing would be sent, nothing would arrive, and a green
test would be measuring an empty room. `docs/frame.md` states that half to the operator.

**Verified on tmux 3.7c and at the 3.2 floor** (`tmuxctl.FLOOR`), by putting a 3.2 built
from the release tarball first on `$PATH`. Identical on both, so nothing here carries a
version gate.
"""

from __future__ import annotations

import fcntl
import itertools
import os
import shutil
import struct
import subprocess
import sys
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

#: How many servers this module has started. **A socket per TEST, not per class or per
#: run**, and the reason is what this file is about: a click starts a DETACHED charter that
#: goes on making tmux calls after the case that clicked has finished asserting — a chat
#: switch is ~20 of them. On a shared socket the next case kills that server, starts its
#: own, and the previous case's child then aims `select-pane -t %1` at a pane id that
#: exists again and belongs to something else. Measured: running both classes together on
#: one socket failed `_active()` in the second class's `setUp` with the bar selected
#: instead of the harness, and neither class fails alone. A dead socket answers a stray
#: child with an error and nothing else.
_SERVERS = itertools.count()

_DEADLINE = 30.0

_TERM_CANDIDATES = tuple(dict.fromkeys(
    ([os.environ["TERM"]] if os.environ.get("TERM", "dumb") != "dumb" else [])
    + ["xterm-256color", "screen", "vt100"]))

#: Wide enough for rung 1 — every name whole, which is the rung a click is about. The
#: narrow rungs draw no tab to click and `tests/test_frame_bars.py` is where that is said.
COLS = 120


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
    resize the window, which is a SIGWINCH in every pane. Here that would repaint the bar
    for a reason no case asked for, which is exactly the noise these cases have to be
    free of.
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


class _ARealFrameWithBars(PersonaIso):
    """One real frame whose panels are bars, on a plane a CHILD charter can also find.

    **`make_plane` and not merely `PersonaIso`'s root**, and the reason is the whole
    point of this file: the switch is performed by a `charter frame-switch` process the
    PANEL starts, which resolves its own plane. `PersonaIso` redirects `config` in this
    interpreter and writes no `charter.toml`, so such a child would go looking for a plane
    of its own and — on the machine this was written on — find the operator's live one
    (#527). A plane a test claims to have has to be one a subprocess can find.
    """

    #: The workspace this frame starts on, and the one it is clicked onto. Literals, so a
    #: case cannot be satisfied by whatever the plane happens to contain.
    HERE = "alpha"
    THERE = "beta"

    def setUp(self) -> None:
        super().setUp()
        v = tmuxctl.version()
        if v is None or v < tmuxctl.FLOOR:
            self.skipTest(f"the frame's floor is tmux {tmuxctl.FLOOR[0]}."
                          f"{tmuxctl.FLOOR[1]}; this machine has {v}")
        self.socket = _tmuxreap.name(f"tabbar{next(_SERVERS)}")
        self.socket_path = os.path.join(os.environ.get("TMUX_TMPDIR") or "/tmp",
                                        f"tmux-{os.getuid()}", self.socket)
        self.addCleanup(self._teardown_socket)
        self.plane = make_plane(self, self._plane_toml())
        for name in (self.HERE, self.THERE):
            (config.WORKSPACES_DIR / name).mkdir(parents=True, exist_ok=True)

        existing = os.environ.get("PYTHONPATH", "")
        parts = [str(_REPO_ROOT)] + ([existing] if existing else [])
        self.env = dict(os.environ, CHARTER_ROOT=str(self.plane),
                        PYTHONPATH=os.pathsep.join(parts))
        self.env.pop("CHARTER_HOME", None)
        # **`$CHARTER_WORKSPACE` is deliberately absent from the pane environment.** It is
        # a PIN: `switch.to_workspace` refuses outright when the launch carried one,
        # because that value sits in every panel pane's process environment and no file
        # charter writes can take it out. A fixture that set it would make every case here
        # measure the refusal.
        self.env.pop("CHARTER_WORKSPACE", None)

    def _tmux(self, *args: str, env=None) -> subprocess.CompletedProcess:
        """One tmux command against THIS case's own server."""
        return subprocess.run(["tmux", "-L", self.socket, *args], capture_output=True,
                              text=True, timeout=20, env=env)

    def _plane_toml(self) -> str:
        """The plane's `charter.toml` — what a CHILD process reads to know this frame.

        The bars are placed with a `[[frame.component]]` table, which is the only form
        that can place one (neither has a committed `[frame] slots` spelling), so this is
        also the config an operator writes to get what these cases click on.
        """
        return ("schema = 1\n"
                '[[frame.component]]\nuse = "workspaces"\nedge = "top"\nsize = 1\n'
                '[[frame.component]]\nuse = "chats"\nedge = "top"\nsize = 1\n')

    # -- the frame ---------------------------------------------------------- #

    def _start_session(self, fid: str, session: str | None = None) -> str:
        """A session whose first window's only pane is a `cat` standing in for a harness."""
        started = self._tmux("-f", "/dev/null", "new-session", "-d", "-s", session or fid,
                        "-x", str(COLS), "-y", "24", "-P", "-F", "#{pane_id}", "cat",
                        env=self.env)
        self.assertEqual(started.returncode, 0, started.stderr)
        return started.stdout.strip()

    def _source_conf(self, fid: str) -> None:
        """charter's OWN frame config, `mouse = true`.

        Asking `conf_text` for the text rather than writing `set mouse on` by hand is what
        makes the `MouseDown1Pane` rebind (#634) part of what is under test — a
        hand-written config would leave tmux's default binding in place and the "keyboard
        stays on the harness" case below would be measuring a config this file invented.
        """
        conf = str(self.tmp / f"{fid}.conf")
        with open(conf, "w") as fh:
            fh.write(commands_frame.conf_text(hotkey=config.FRAME["hotkey"], mouse=True,
                                              history_limit=100, session=fid))
        sourced = self._tmux("source-file", conf)
        self.assertEqual(sourced.returncode, 0, sourced.stderr)

    def _split_bar(self, harness: str, name: str, fid: str) -> str:
        """One real `charter panel <name>` pane, marked the way its launcher marks it.

        `layout.panel_command` and `commands_frame._panel_mark_argv` are called rather
        than re-spelled: the argv is what the launcher really sends, and that mark is what
        the `MouseDown1Pane` bind asks about.
        """
        argv = layout.panel_command(slot=name, session=fid)
        r = self._tmux("split-window", "-t", harness, "-v", "-l", "1",
                  "-P", "-F", "#{pane_id}", "--", *argv, env=self.env)
        self.assertEqual(r.returncode, 0, r.stderr)
        pane = r.stdout.strip()
        marked = subprocess.run(
            commands_frame._panel_mark_argv(socket=self.socket, pane_id=pane),
            capture_output=True, text=True, timeout=20)
        self.assertEqual(marked.returncode, 0, marked.stderr)
        return pane

    def _attach(self, session: str) -> int:
        size = self._window_size(session)
        self.assertNotEqual(size, (-1, -1), "tmux would not say how big its window is")
        cols, rows = size
        refusals = []
        for term in _TERM_CANDIDATES:
            pid, fd = _fork_pty(rows=rows, cols=cols)
            if pid == 0:
                try:
                    os.environ["TERM"] = term
                    os.environ["CHARTER_ROOT"] = str(self.plane)
                    os.execvp("tmux",
                              ["tmux", "-L", self.socket, "attach", "-t", session])
                finally:
                    os._exit(127)
            if _await(lambda: bool(self._tmux("list-clients", "-t", session,
                                              "-F", "#{client_name}").stdout.strip()),
                      timeout=10.0):
                self.addCleanup(self._reap, pid, fd)
                self.assertTrue(
                    _await(lambda: self._window_size(session) == size),
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

    def _bar_row(self, pane: str) -> str:
        return self._shown(pane).split("\n")[0].rstrip()

    def _window_size(self, session: str) -> tuple[int, int]:
        r = self._tmux("display-message", "-p", "-t", session,
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

    def _current_window(self, session: str) -> str:
        return self._tmux("display-message", "-p", "-t", f"{session}:",
                     "#{window_id}").stdout.strip()

    def _window_of(self, pane: str) -> str:
        return self._tmux("display-message", "-p", "-t", pane, "#{window_id}").stdout.strip()

    def _click(self, pane: str, *, col: int, row: int = 0, button: int = 0) -> None:
        """One left press and its release, at the pane's own cell, through the client.

        Both halves are sent because that is what a terminal sends. Only the press is
        acted on — `frame/builtins._bar_events`' reading of §4i — and sending only the
        press would leave this measuring a gesture no mouse makes.
        """
        left, top = self._rect(pane)
        wcol, wrow = left + col + 1, top + row + 1
        os.write(self.fd, b"\x1b[<%d;%d;%dM" % (button, wcol, wrow))
        os.write(self.fd, b"\x1b[<%d;%d;%dm" % (button, wcol, wrow))

    def _column_of(self, pane: str, field: str) -> int:
        """Which column of *pane* the drawn *field* starts in — read off the pane.

        Off the PANE and never off `slots.TABS`: that object lives in the panel's process
        and not in this one, and asking charter where it put something and then clicking
        there would be a test that agrees with itself. This is the column the operator's
        eye lands on.
        """
        row = self._bar_row(pane)
        at = row.index(field)
        self.assertNotIn(field, row[at + 1:], f"{field!r} is not unique in {row!r}")
        return at


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class ARealClickOnTheWorkspaceBarMovesTheFrame(_ARealFrameWithBars, unittest.TestCase):
    """The `workspaces` bar: click a name, the frame is on that workspace.

    **Three processes and none of them is this one.** The bar is painted by a `charter
    panel workspaces` child, the click is decoded there, and the switch is performed by a
    `charter frame-switch` child that panel starts. The only thing between them and this
    test is the plane on disk — so these passing is the proof that the whole chain is real
    rather than an artefact of one interpreter.
    """

    def setUp(self) -> None:
        super().setUp()
        self.fid = state.frame_id("tabbar", os.getpid())
        state.frame_dir(self.fid, create=True)
        state.record_workspace(self.fid, self.HERE)
        self.harness = self._start_session(self.fid)
        state.record_server(self.fid, self.socket)
        state.record_harness_pane(self.fid, self.harness)
        self._source_conf(self.fid)
        self.bar = self._split_bar(self.harness, "workspaces", self.fid)
        # **The harness is put back in front, and that is the regime, not tidiness.**
        # `split-window` selects the pane it made, and a frame whose bar was the ACTIVE
        # pane would be reporting the mouse because THAT pane asked — the `mouse = false`
        # route `docs/frame.md` describes, which would make every case below pass with
        # charter's flag off. `commands_frame._split_panels` ends the same way.
        self.assertEqual(self._tmux("select-pane", "-t", self.harness).returncode, 0)
        self.fd = self._attach(self.fid)
        self.assertTrue(
            _await(lambda: f"*{self.HERE}" in self._bar_row(self.bar)),
            f"the bar never painted: {self._bar_row(self.bar)!r}")
        self.assertEqual(self._active(), self.harness,
                         "the harness is not the active pane, so a report reaching the "
                         "bar would prove nothing about `[frame] mouse`")

    def test_clicking_a_tab_moves_the_frame_to_that_workspace(self):
        """The report, answered end to end. Nothing here writes the frame's workspace and
        nothing resizes anything, so a frame that moved moved because of the click."""
        self._click(self.bar, col=self._column_of(self.bar, f" {self.THERE}"))
        self.assertTrue(
            _await(lambda: state.frame_workspace(self.fid) == self.THERE),
            f"the click never reached the switch: "
            f"{state.frame_workspace(self.fid)!r}")

    def test_the_bar_repaints_with_the_mark_on_the_workspace_it_moved_to(self):
        """The switch ends in a `state.bump`, which is the version this panel's poll was
        already watching — so the mark follows with no repaint of this test's own."""
        self._click(self.bar, col=self._column_of(self.bar, f" {self.THERE}"))
        self.assertTrue(
            _await(lambda: f"*{self.THERE}" in self._bar_row(self.bar)),
            f"the bar never redrew the mark: {self._bar_row(self.bar)!r}")

    def test_the_click_leaves_the_keyboard_on_the_harness(self):
        """The property `mouse = true` used to take away and #634 gave back, asserted
        against a BAR rather than the repo table. A tab you cannot click without losing
        your prompt is not a tab an operator will click twice."""
        self._click(self.bar, col=self._column_of(self.bar, f" {self.THERE}"))
        self.assertTrue(_await(
            lambda: state.frame_workspace(self.fid) == self.THERE))
        self.assertEqual(self._active(), self.harness,
                         "clicking the workspace bar moved the keyboard off the harness")

    def test_clicking_the_workspace_you_are_already_on_moves_nothing(self):
        """Re-switching would re-gather the plane and bump the frame to arrive exactly
        where it already was. The rule is `slots._Tabs.switch_to`'s; this is it holding
        through a real terminal, a real pane and a real handler."""
        before = self._bar_row(self.bar)
        self._click(self.bar, col=self._column_of(self.bar, f"*{self.HERE}"))
        time.sleep(2.0)
        self.assertEqual(state.frame_workspace(self.fid), self.HERE)
        self.assertEqual(self._bar_row(self.bar), before)

    def test_clicking_the_heading_moves_nothing(self):
        """`  workspaces  ` is about no workspace. It is a field the operator can see and
        click, so "nothing happens" has to be what happens rather than the nearest name
        being picked for them."""
        self._click(self.bar, col=0)
        time.sleep(2.0)
        self.assertEqual(state.frame_workspace(self.fid), self.HERE)

    def test_clicking_past_the_last_tab_moves_nothing(self):
        """The empty right-hand end of the row — the `+N` overflow's neighbour, and the
        commonest miss."""
        self._click(self.bar, col=COLS - 2)
        time.sleep(2.0)
        self.assertEqual(state.frame_workspace(self.fid), self.HERE)


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class ARealClickOnTheChatBarMovesTheClient(_ARealFrameWithBars, unittest.TestCase):
    """The `chats` bar: click a name, the client is on that chat's window.

    **This is the half that moves a CLIENT rather than a file**, and it is the one #684 is
    about: membership comes from this plane's own chat directories (`state.frame_workspace`,
    a file), the target is a recorded pane ID and never a session NAME, and `cmd_chat`
    establishes where both chats actually are before it aims anything anywhere — because
    `select-window` at another session's pane returns 0 and moves that session while this
    client stays put.
    """

    WS = "alpha"

    def setUp(self) -> None:
        super().setUp()
        self.here, self.there = f"{self.WS}.1", f"{self.WS}.2"
        self.harness = self._start_session(self.here, session=self.WS)
        second = self._tmux("new-window", "-t", self.WS, "-P", "-F", "#{pane_id}",
                       "cat", env=self.env)
        self.assertEqual(second.returncode, 0, second.stderr)
        self.other_harness = second.stdout.strip()
        for chat, pane in ((self.here, self.harness),
                           (self.there, self.other_harness)):
            state.frame_dir(chat, create=True)
            state.record_workspace(chat, self.WS)
            state.record_server(chat, self.socket)
            state.record_harness_pane(chat, pane)
            state.record_identity(chat, {"CHARTER_SESSION_ID": chat,
                                         "CHARTER_ROOT": str(self.plane)})
        self._source_conf(self.WS)
        self.assertEqual(self._tmux("select-window", "-t", self.harness).returncode, 0)
        self.bar = self._split_bar(self.harness, "chats", self.here)
        # **The bar is recorded as this chat's panel, which is what makes the switch tear
        # it down** — `cmd_chat`'s step 2 is `_apply_arrangement(<chat being left>,
        # want=[])`, and `state.panes` is the record it reads to know which pane to kill.
        # Without this line the teardown is a no-op and the case below measures a switch
        # that never touched the pane its own click came from, which is the one thing
        # about this path that is not like the palette's.
        state.record_panes(self.here, panels={"chats": self.bar})
        self.assertEqual(self._tmux("select-pane", "-t", self.harness).returncode, 0)
        self.fd = self._attach(self.WS)
        self.assertTrue(
            _await(lambda: self.there in self._bar_row(self.bar)),
            f"the chat bar never painted both chats: {self._bar_row(self.bar)!r}")
        self.assertEqual(self._active(), self.harness)

    def test_clicking_the_other_chats_tab_moves_the_client_to_its_window(self):
        """`select-window` at the target chat's own harness PANE — a pane id resolves to
        that pane's window, which is why charter keeps no window-id record beside the pane
        record it already has. The client following is the whole switch."""
        self._click(self.bar, col=self._column_of(self.bar, f" {self.there}"))
        self.assertTrue(
            _await(lambda: self._current_window(self.WS)
                   == self._window_of(self.other_harness)),
            "the click never moved the client to the other chat's window")

    def test_the_click_leaves_the_keyboard_on_a_harness(self):
        """A chat switch moves the WINDOW, and the pane the operator types in afterwards
        is the new chat's harness — never the bar they clicked, and never a panel.

        **Asked once the switch has SETTLED, and the first version asked too early.** The
        client moving is step 1 of four; step 3 splits the entered chat's own panels, and
        `split-window` selects the pane it makes — `commands_frame._split_panels` re-selects
        the harness at the end, for the same reason this case exists. Between those two
        moments the active pane really is a panel, for a few hundred milliseconds. Measured
        rather than reasoned: this file was green on the machine it was written on and
        failed on CI's 3.13 runner with `'%4' != '%1'`, which is that window. So the case
        waits for `state.panes` — the record step 3 writes last — before it asks.
        """
        self._click(self.bar, col=self._column_of(self.bar, f" {self.there}"))
        self.assertTrue(
            _await(lambda: self._current_window(self.WS)
                   == self._window_of(self.other_harness)),
            "the click never moved the client to the other chat's window")
        self.assertTrue(
            _await(lambda: bool(state.panes(self.there))),
            "the switch never finished laying the entered chat out, so the pane it "
            "leaves selected is not the one this case is about")
        self.assertTrue(
            _await(lambda: self._active() == self.other_harness),
            f"clicking the chat bar left the keyboard on {self._active()} rather than "
            f"the entered chat's harness {self.other_harness}")

    def test_the_switch_runs_to_the_end_after_killing_the_pane_it_started_from(self):
        """**The whole switch, from a click on a pane the switch then destroys.**

        Step 2 of a chat switch is `_apply_arrangement(<chat being left>, want=[])`, and
        on a frame where a bar is one of the panels that means killing the very pane the
        click came from — the one whose process started the child. Nothing else in this
        file reaches past that point: the client has already moved by then. What is
        asserted here is step THREE, the chat being ENTERED getting its panels, which is
        the switch having run to its end rather than having stopped when its own parent
        went away.

        **What this does NOT pin, said rather than implied.** `_spawn`'s
        `start_new_session=True` looks like the thing that makes this work, and it was
        written up that way first; measured with `start_new_session=False` the case is
        still green, so the SIGHUP is not what this case is about and the comment claiming
        it has been deleted from `_bar_events` rather than left as a story. `_spawn` is
        used because it is the one answer to how a frame surface starts detached work, not
        because this case can tell it apart from a bare `Popen`.
        """
        self._click(self.bar, col=self._column_of(self.bar, f" {self.there}"))
        self.assertTrue(
            _await(lambda: self.bar not in self._tmux(
                "list-panes", "-a", "-F", "#{pane_id}").stdout.split()),
            "the chat that was left kept the bar pane, so the teardown this case is "
            "about did not run and nothing below is measured")
        self.assertTrue(
            _await(lambda: bool(state.panes(self.there))),
            "the switch stopped when the pane that started it was killed — its own "
            f"teardown took it with it: {state.panes(self.there)!r}")

    def test_clicking_the_chat_you_are_in_moves_nothing(self):
        """`chats.check` refuses a switch to the chat you are already in — a teardown and
        a split for no change on screen — but the click never gets that far: the bar knows
        which tab is marked, so the gesture costs nothing at all rather than costing an
        interpreter start and a refusal."""
        window = self._current_window(self.WS)
        self._click(self.bar, col=self._column_of(self.bar, f"*{self.here}"))
        time.sleep(2.0)
        self.assertEqual(self._current_window(self.WS), window)
        self.assertEqual(self._active(), self.harness)


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class ARealClickOnAWINDOWEDWorkspaceBarMovesTheFrame(_ARealFrameWithBars,
                                                     unittest.TestCase):
    """The rung an operator's plane actually draws, clicked on a real terminal.

    **Every other case in this file clicks rung 1**, where every name is on the row —
    which needs 274 columns for the fifteen workspaces this project's own plane has. So
    the cases above pass on a bar no operator can produce, and the report that produced
    them (*"seems not working?"*) survived them: the rung a 120-column terminal reaches
    drew one tab, the one the frame was already on, and clicking it is correctly nothing.

    This is that plane. Fifteen real workspace names, a 120-column window, and a click on a
    tab that is only on the row because the rung draws a PAGE rather than the marked name
    alone. Nothing here is a narrower unit of the same thing: the columns the operator's
    pointer lands in are read off the pane tmux painted, so a page whose map was published
    from the wrong starting column — the leading `+N` displaces every tab right of it —
    fails here and passes every unit test that asks `slots.TABS` where it put something.
    """

    #: The fifteen this plane has. Real names rather than even-width ones: the cut is
    #: greedy over the widths names actually have, and `relations-and-delegations` (25
    #: cells) against `todos` (5) is what makes the page boundary land where it does.
    NAMES = sorted([
        "authority-audit", "autonomy", "charter-update-skill", "default", "fleet",
        "harness-wrapper", "news-dispatch-guard", "opencode-integration", "plane-shape",
        "relations-and-delegations", "showcase", "statusline-improvements", "todos",
        "tracking-github-issues", "user-reporting",
    ])
    #: The workspace this frame is on, and one that shares its page at :data:`COLS`. Both
    #: are asserted to be on the drawn row in `setUp` rather than assumed, so a change to
    #: the cut fails loudly here instead of silently clicking empty cells.
    HERE = "harness-wrapper"
    THERE = "authority-audit"

    def setUp(self) -> None:
        super().setUp()
        for name in self.NAMES:
            (config.WORKSPACES_DIR / name).mkdir(parents=True, exist_ok=True)
        self.fid = state.frame_id("tabbarwin", os.getpid())
        state.frame_dir(self.fid, create=True)
        state.record_workspace(self.fid, self.HERE)
        self.harness = self._start_session(self.fid)
        state.record_server(self.fid, self.socket)
        state.record_harness_pane(self.fid, self.harness)
        self._source_conf(self.fid)
        self.bar = self._split_bar(self.harness, "workspaces", self.fid)
        self.assertEqual(self._tmux("select-pane", "-t", self.harness).returncode, 0)
        self.fd = self._attach(self.fid)
        self.assertTrue(
            _await(lambda: f"*{self.HERE}" in self._bar_row(self.bar)),
            f"the bar never painted: {self._bar_row(self.bar)!r}")
        row = self._bar_row(self.bar)
        self.assertNotIn(self.NAMES[-1], row,
                         f"{COLS} columns drew every name, so this measures rung 1 and "
                         f"not the windowed rung: {row!r}")
        self.assertIn(f" {self.THERE}", row,
                      f"the page does not hold the tab this case clicks: {row!r}")
        self.assertEqual(self._active(), self.harness)

    def test_a_click_on_a_tab_the_old_rung_never_drew_moves_the_frame(self):
        """The operator's report, at the operator's width. Before the windowed rung this
        row was `workspaces  *harness-wrapper  +14` and this click had nothing to land
        on."""
        self._click(self.bar, col=self._column_of(self.bar, f" {self.THERE}"))
        self.assertTrue(
            _await(lambda: state.frame_workspace(self.fid) == self.THERE),
            f"the click never reached the switch: "
            f"{state.frame_workspace(self.fid)!r} — row {self._bar_row(self.bar)!r}")

    def test_the_page_does_not_move_under_the_pointer_when_the_frame_switches(self):
        """**The double-click property, through a real terminal.** The cut is made from the
        names and the width alone, so the row that comes back after the switch holds the
        same names in the same columns with only the `*` moved — and a second press at the
        cell that just switched lands on the tab the frame has arrived at, which does
        nothing.

        A window centred on the marked name would have slid one step here, putting a third
        workspace under that cell; `slots._Tabs.switch_to` could not refuse it, because the
        name at that column really would have changed.
        """
        before = self._bar_row(self.bar)
        col = self._column_of(self.bar, f" {self.THERE}")
        self._click(self.bar, col=col)
        self.assertTrue(
            _await(lambda: state.frame_workspace(self.fid) == self.THERE),
            "the first click never switched, so there is no repaint to measure")
        self.assertTrue(
            _await(lambda: f"*{self.THERE}" in self._bar_row(self.bar)),
            f"the bar never redrew the mark: {self._bar_row(self.bar)!r}")
        after = self._bar_row(self.bar)
        self.assertEqual([n for n in self.NAMES if n in after],
                         [n for n in self.NAMES if n in before],
                         f"the page moved under the pointer:\n  {before!r}\n  {after!r}")
        self._click(self.bar, col=col)
        time.sleep(2.0)
        self.assertEqual(state.frame_workspace(self.fid), self.THERE,
                         "the second press at the same column switched a second time")

    def test_neither_overflow_count_is_a_tab(self):
        """`+9` stands for names that are not on the row. It is the field an operator is
        most likely to try, and it has to do nothing rather than pick one of them."""
        row = self._bar_row(self.bar)
        counts = [f.strip() for f in row.split("  ") if f.strip().startswith("+")]
        self.assertTrue(counts, f"this row carries no count to click: {row!r}")
        for count in counts:
            self._click(self.bar, col=self._column_of(self.bar, count))
        time.sleep(2.0)
        self.assertEqual(state.frame_workspace(self.fid), self.HERE)
