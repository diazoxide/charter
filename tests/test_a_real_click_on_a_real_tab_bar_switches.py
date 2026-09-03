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
from charter.frame import chats, layout, slots, state, tmuxctl

from tests import _tmuxreap
from tests._isolation import PersonaIso, make_plane

_HAS_TMUX = shutil.which("tmux") is not None


def _NOT_OPEN(ws: str) -> str:
    """What a click on a not-open workspace's tab puts on this fixture's attention row.

    **This used to be a flat refusal and is now as far as an OPEN gets here.** A tab for a
    workspace with no session opens it (`commands_frame._open_workspace`); what stops it in
    *this* module is the next question along — which harness to open it with. The chats
    these cases build record no `$CHARTER_HARNESS` and the isolated plane declares no
    `[harness] default`, so the open refuses by name rather than starting something nobody
    chose. That is deliberate and load-bearing for a test suite: **no case in this file may
    ever start a real harness process**, and the assertion below is what would notice if
    one became reachable.

    Spelled here rather than imported, deliberately: it is the operator-visible sentence
    and this module's whole subject is that a click PRODUCES one. A constant read out of
    the module under test would follow a reworded sentence silently, and the wording is
    the thing. It carries the name, so a click that landed on the wrong tab fails here.
    """
    return f"cannot open '{ws}': this chat records no harness"

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
        # **`$CHARTER_WORKSPACE` is deliberately absent from the pane environment.** It
        # is a PIN on which workspace this CHAT is in (`state.own_workspace`'s first rung),
        # and a fixture that set it would be answering half of every case here for
        # charter: `switch.current_workspace` would come back from the environment rather
        # than from the record the launcher wrote, so the "already in this workspace"
        # refusal would fire on a name the test never chose. It no longer refuses a switch
        # (§4b — a client moves and the chat does not), which is why this comment is about
        # the fixture rather than about a refusal.
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

    def _panes(self, session: str | None = None) -> list[str]:
        """Every pane of this frame's window, asked of tmux.

        How a case says whether the palette opened: it is a real pane carved off the
        harness by a real detached `charter frame-palette`, so a pane appearing is the
        observable — `tests/test_a_real_click_opens_the_real_palette.py` uses the same one
        for the doorway on the attention row.
        """
        return self._tmux("list-panes", "-t", session or self.fid,
                          "-F", "#{pane_id}").stdout.split()

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
class ARealClickOnTheWorkspaceBarReachesTheSwitch(_ARealFrameWithBars,
                                                  unittest.TestCase):
    """The `workspaces` bar: click a name, and `charter frame-switch --workspace` runs.

    **Three processes and none of them is this one.** The bar is painted by a `charter
    panel workspaces` child, the click is decoded there, and the switch is performed by a
    `charter frame-switch` child that panel starts. The only thing between them and this
    test is the plane on disk — so these passing is the proof that the whole chain is real
    rather than an artefact of one interpreter.

    **The class was `…MovesTheFrame` and the frame no longer moves** (§4j: a chat belongs
    to its workspace for life). What the click produces is the refusal on the frame's own
    attention row, and that is what these await — the far end of the same three processes,
    written by the same child, reached by the same click. Everything this file is really
    about — the column arithmetic, the keyboard staying on the harness, the page not
    sliding — is unchanged and is what would still fail if the chain broke.
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

    def test_clicking_a_tab_reaches_the_switch_and_the_answer_comes_back(self):
        """The report, answered end to end.

        **The observable has changed twice and the chain has not.** It was
        `state.frame_workspace(fid) == THERE` until §4j removed that write, then
        `switch.FOR_LIFE` while #789 refused every tab. Under §4b the answer names the
        workspace that was clicked: `THERE` exists on this plane and has no tmux session
        on this test's server, so the switch has nowhere to move the client and says so.
        That is a strictly stronger reading than the constant it replaces — it proves the
        NAME reached the child, which `FOR_LIFE` interpolated nothing to prove.

        `state.notice` is the other end of the same three processes — the panel decodes
        the click, spawns `charter frame-switch --workspace <name>`, and that child writes
        the outcome to the frame's attention row.

        A switch that actually lands a client in another workspace needs a second live
        workspace session and is measured in
        `tests/test_a_workspace_switch_moves_the_client.py`.
        """
        self._click(self.bar, col=self._column_of(self.bar, f" {self.THERE}"))
        self.assertTrue(
            _await(lambda: _NOT_OPEN(self.THERE) in state.notice(self.fid)),
            f"the click never reached the switch: notice is "
            f"{state.notice(self.fid)!r}")
        self.assertEqual(state.frame_workspace(self.fid), self.HERE,
                         "the switch moved the chat anyway")

    def test_the_bar_repaints_and_the_mark_stays_where_the_chat_is(self):
        """The switch ends in a `state.bump` — the version this panel's poll was already
        watching — so the bar redraws without a repaint of this test's own. What it
        redraws is the same mark: the bar marks the workspace this CHAT is in
        (`switch.current_workspace`), and no switch moves that.

        Was `test_the_bar_repaints_with_the_mark_on_the_workspace_it_moved_to`.
        """
        was = state.version(self.fid)
        self._click(self.bar, col=self._column_of(self.bar, f" {self.THERE}"))
        self.assertTrue(_await(lambda: state.version(self.fid) != was),
                        "the click never bumped the frame, so nothing repaints")
        row = self._bar_row(self.bar)
        self.assertIn(f"*{self.HERE}", row, row)
        self.assertNotIn(f"*{self.THERE}", row, row)

    def test_the_click_leaves_the_keyboard_on_the_harness(self):
        """The property `mouse = true` used to take away and #634 gave back, asserted
        against a BAR rather than the repo table. A tab you cannot click without losing
        your prompt is not a tab an operator will click twice."""
        self._click(self.bar, col=self._column_of(self.bar, f" {self.THERE}"))
        self.assertTrue(_await(
            lambda: _NOT_OPEN(self.THERE) in state.notice(self.fid)))
        self.assertEqual(self._active(), self.harness,
                         "clicking the workspace bar moved the keyboard off the harness")

    def test_clicking_the_workspace_you_are_already_on_starts_nothing(self):
        """The tab you are on spawns no switch at all — `slots._Tabs.switch_to` refuses
        before a process is started, which is a different thing from the switch refusing
        after one is. This is that rule holding through a real terminal, a real pane and a
        real handler, and the notice is what tells the two apart: an empty attention row
        means nothing ran."""
        before = self._bar_row(self.bar)
        self._click(self.bar, col=self._column_of(self.bar, f"*{self.HERE}"))
        time.sleep(2.0)
        self.assertEqual(state.frame_workspace(self.fid), self.HERE)
        self.assertEqual(state.notice(self.fid), "",
                         "the tab the frame is on started a switch")
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


class _ARealChatBarOverTwoChats(_ARealFrameWithBars):
    """One frame, one `chats` bar, two chats — the fixture both buttons are pressed on.

    Held apart from the cases so the two gestures are measured against ONE frame rather
    than two: same bar, same chats, same client, same `mouse = true` config out of
    `conf_text`. It has no test methods of its own, so it is discovered and runs nothing.
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


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class ARealClickOnTheChatBarMovesTheClient(_ARealChatBarOverTwoChats, unittest.TestCase):
    """The `chats` bar: click a name, the client is on that chat's window.

    **This is the half that moves a CLIENT rather than a file**, and it is the one #684 is
    about: membership comes from this plane's own chat directories (`state.frame_workspace`,
    a file), the target is a recorded pane ID and never a session NAME, and `cmd_chat`
    establishes where both chats actually are before it aims anything anywhere — because
    `select-window` at another session's pane returns 0 and moves that session while this
    client stays put.
    """

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
class ARealRightClickOnTheChatBarOpensARealMenu(_ARealChatBarOverTwoChats,
                                               unittest.TestCase):
    """Button 2 on a real tab, through a real client, really draws a menu — #846.

    **This is the link no unit test can stand in for, and it is the only claim in this
    feature that was ever in doubt.** That charter's handler answers a `right` event is
    `tests/test_a_right_click_on_a_tab_acts_on_that_tab.py`; what could not be settled
    there is whether button 2 ever arrives at all. tmux might route it to a binding, or
    swallow it, or hand it over as X10 — and `#{mouse_any_flag}` is what decides, on a
    pane in another window that this test's own client is not looking at.

    It arrives. Measured here on a real server, a real client on a real pty, a real
    `charter panel chats` holding the pane, and a real detached `charter frame-palette
    --tab <chat>` that splits a real overlay pane and paints a real menu into it.

    **Nothing here presses Enter, and that is deliberate rather than incidental.** The
    last row of the surface these cases draw is `chat: close`, and going through with it
    stops a harness. What is asserted is the surface — that it exists, that it is about
    the tab that was pressed, and that opening it moved nothing.

    The class inherits its whole fixture from the left-click cases above, so the two
    gestures are measured against one frame: same bar, same two chats, same client, same
    `mouse = true` config out of `conf_text`.
    """

    #: SGR's number for the right button. `overlay._SGR_BUTTONS` maps it to `right`, and
    #: this is the far end of the same trip: what the terminal writes is what tmux
    #: forwards, and 2 is what a real right-click is spelled with.
    BUTTON = 2

    def _menu_pane(self) -> str:
        """The pane the menu opened in, or `""` — whatever this window has that the frame
        did not.

        Off tmux's own listing and never off charter's record: `state.panes` names the
        PANELS a re-layout wrote, and an overlay is deliberately not one of them
        (`overlay.OVERLAY_OPTION` is a pane option, so the answer dies with the pane). A
        pane appearing is the same observable `tests/test_a_real_click_opens_the_real
        _palette.py` uses for the doorway on the attention row.
        """
        for pane in self._panes(self.WS):
            if pane not in (self.harness, self.bar):
                return pane
        return ""

    def _await_menu(self, tab: str) -> str:
        self.assertTrue(
            _await(lambda: bool(self._menu_pane())),
            "no pane was ever opened, so button 2 never reached the panel")
        pane = self._menu_pane()
        self.assertTrue(
            _await(lambda: f"chat: close {tab}" in self._shown(pane)),
            f"the pane that opened never drew {tab}'s menu: {self._shown(pane)!r}")
        return pane

    def test_a_right_press_on_another_chats_tab_opens_that_chats_menu(self):
        """The whole chain: a button-2 report written to the client's pty, decoded by a
        `charter panel chats` process, resolved through `slots._Tabs.tab_at`, spawned as
        `charter frame-palette --tab <chat>`, split into an overlay pane by a THIRD
        process, and painted."""
        self._click(self.bar, col=self._column_of(self.bar, f" {self.there}"),
                    button=self.BUTTON)
        shown = self._shown(self._await_menu(self.there))
        self.assertIn(f"chat {self.there}", shown)
        self.assertIn("chat: previous transcript", shown)

    def test_a_right_press_on_the_tab_you_are_ON_opens_its_menu(self):
        """`slots._Tabs.tab_at`'s one difference from `switch_to`, end to end. The left
        click on this same cell is asserted three cases up to move nothing at all."""
        self._click(self.bar, col=self._column_of(self.bar, f"*{self.here}"),
                    button=self.BUTTON)
        self.assertIn(f"chat {self.here}", self._shown(self._await_menu(self.here)))

    def test_opening_the_menu_switches_nothing(self):
        """A right-click is not a slow left-click. The client stays on the window it was
        on, and the chat the menu is ABOUT keeps its own harness — the tab is still there
        to be closed, or not, by the keypress that has not happened."""
        window = self._current_window(self.WS)
        self._click(self.bar, col=self._column_of(self.bar, f" {self.there}"),
                    button=self.BUTTON)
        self._await_menu(self.there)
        self.assertEqual(self._current_window(self.WS), window)
        self.assertIn(self.other_harness,
                      self._tmux("list-panes", "-a", "-F", "#{pane_id}").stdout.split())

    def test_the_menu_takes_the_keyboard_and_the_harness_survives(self):
        """**A menu is modal, and this is the one gesture on a bar that is.** A switch
        leaves the keyboard on a harness (#634); a menu must take it, or the rows it just
        drew could not be chosen from — `overlay.modal_argvs` selects the overlay pane and
        zooms it over the window. The harness is not touched: it is still a pane, and
        `overlay.HATCH_KEY` is what gives it back."""
        self._click(self.bar, col=self._column_of(self.bar, f" {self.there}"),
                    button=self.BUTTON)
        pane = self._await_menu(self.there)
        self.assertTrue(_await(lambda: self._active() == pane),
                        f"the menu drew but the keyboard stayed on {self._active()}")
        self.assertIn(self.harness, self._panes(self.WS))

    def test_escape_closes_the_menu_and_stops_nothing(self):
        """**The way out, and the assertion that the menu commits nothing by existing.**

        `overlay.Surface.run` treats Escape as a cancel and answers no row, so
        `frame/tabmenu.act` is never reached — which matters more on this surface than on
        any other, because the row under the cursor when Escape is pressed is one keypress
        from a confirmation that stops a harness. What comes back is the pane:
        `commands_frame._close_palette` selects the harness, kills the overlay and re-arms
        the hatch, as one chained tmux command.
        """
        self._click(self.bar, col=self._column_of(self.bar, f" {self.there}"),
                    button=self.BUTTON)
        pane = self._await_menu(self.there)
        os.write(self.fd, b"\x1b")
        self.assertTrue(
            _await(lambda: pane not in self._panes(self.WS)),
            "Escape left the menu's pane standing, holding a live process")
        self.assertTrue(_await(lambda: self._active() == self.harness),
                        f"the menu closed but the keyboard stayed on {self._active()}")
        self.assertIn(self.other_harness,
                      self._tmux("list-panes", "-a", "-F", "#{pane_id}").stdout.split())

    def test_a_right_press_on_the_heading_opens_nothing(self):
        """`  chats  ` is about no chat. A cell the strip drew no tab into is absent from
        `slots._Tabs`' map whatever asks about it, so this costs not even an interpreter
        start — which is what makes the empty pane list below a real assertion rather than
        a race this case happened to win.

        **What this case deliberately does NOT assert is where the keyboard ended up, and
        that is a measurement rather than an omission.** tmux's own default binding for
        this key, read back off a real 3.7c with `list-keys -T root`, is::

            MouseDown3Pane if-shell -F -t = "#{||:#{mouse_any_flag},…}"
                             { select-pane -t = ; send-keys -M }
                             { display-menu … }

        The branch that forwards the report **selects the pane first**. That is #634's
        defect, one button over and still open: with `[frame] mouse = true`, a right-click
        on any charter panel moves the keyboard to it before the byte arrives. It is
        pre-existing — it is what tmux has always done to button 3 — and it costs nothing
        on the gesture this feature is for, because a menu that opens takes the keyboard
        anyway and `commands_frame._close_palette` selects the harness on the way out. It
        costs a MISS one left click, or `overlay.HATCH_KEY`.

        Charter binds nothing here, which is `frame/builtins._MENU_BUTTON`'s own note:
        `MouseDown1Pane`'s fix works because its else-branch is tmux's own two commands
        written out, and button 3's else-branch is a `display-menu` a page long that
        differs between versions. Asserting the steal here would enshrine it; asserting
        its absence would be red. So this asserts what is contractual — nothing opened —
        and says why the third line is missing.
        """
        self._click(self.bar, col=self._column_of(self.bar, "chats"),
                    button=self.BUTTON)
        time.sleep(2.0)
        self.assertEqual(self._menu_pane(), "")
        self.assertIn(self.harness, self._panes(self.WS))


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class ARealClickOnAWINDOWEDWorkspaceBarReachesTheSwitch(_ARealFrameWithBars,
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

    def test_a_click_on_a_tab_the_old_rung_never_drew_reaches_the_switch(self):
        """The operator's report, at the operator's width. Before the windowed rung this
        row was `workspaces  *harness-wrapper  +14` and this click had nothing to land
        on.

        What comes back names the workspace that was clicked — see
        `ARealClickOnTheWorkspaceBarMovesTheFrame` for why the observable is the attention
        row rather than the frame's record. The column arithmetic this class exists for is
        untouched: a click that landed on the wrong cell, or on none, produces no notice at
        all — and one that landed on the wrong TAB now names the wrong workspace, which
        this assertion would catch and the old constant could not.
        """
        self._click(self.bar, col=self._column_of(self.bar, f" {self.THERE}"))
        self.assertTrue(
            _await(lambda: _NOT_OPEN(self.THERE) in state.notice(self.fid)),
            f"the click never reached the switch: {state.notice(self.fid)!r} — "
            f"row {self._bar_row(self.bar)!r}")

    def test_the_page_does_not_move_under_the_pointer_when_the_bar_repaints(self):
        """**The double-click property, through a real terminal.** The cut is made from the
        names and the width alone, so the row that comes back after the click holds the
        same names in the same columns — and a second press at that cell lands on the same
        tab it did the first time.

        A window centred on the marked name would have slid one step here, putting a third
        workspace under that cell. The mark does not move — it marks the workspace this
        chat is in — so what is asserted is that the page is identical and that the second
        press produces the same answer as the first rather than acting on a name that
        arrived under the pointer.
        """
        before = self._bar_row(self.bar)
        col = self._column_of(self.bar, f" {self.THERE}")
        self._click(self.bar, col=col)
        self.assertTrue(
            _await(lambda: _NOT_OPEN(self.THERE) in state.notice(self.fid)),
            "the first click never reached the switch, so there is nothing to measure")
        after = self._bar_row(self.bar)
        self.assertEqual([n for n in self.NAMES if n in after],
                         [n for n in self.NAMES if n in before],
                         f"the page moved under the pointer:\n  {before!r}\n  {after!r}")
        self.assertEqual(self._column_of(self.bar, f" {self.THERE}"), col,
                         "the tab this test clicked is at a different column now")
        self._click(self.bar, col=col)
        time.sleep(2.0)
        self.assertEqual(state.frame_workspace(self.fid), self.HERE,
                         "the second press at the same column moved the chat")

    def test_neither_overflow_count_switches_the_frame_to_a_workspace(self):
        """`+9` stands for names that are not on the row, so it must not pick one of them.

        It is no longer inert — see the case below, which is the same two clicks asked
        the other way round — and this is what keeps the two answers apart on a real
        terminal: whatever a count does, it does not switch. A handler that fell through
        from `more_at` to the switch would name whichever workspace happened to be under
        the map, which is the class of wrongness this whole file exists to catch.
        """
        row = self._bar_row(self.bar)
        counts = [f.strip() for f in row.split("  ") if f.strip().startswith("+")]
        self.assertTrue(counts, f"this row carries no count to click: {row!r}")
        for count in counts:
            self._click(self.bar, col=self._column_of(self.bar, count))
        time.sleep(2.0)
        self.assertEqual(state.frame_workspace(self.fid), self.HERE)

    def test_clicking_the_overflow_count_opens_a_real_palette(self):
        """*"when workspaces are more we are showing `+N` now in tabs — but user can't
        click and see other workspaces."*

        The operator pressed this field on their own frame and nothing happened. This is
        that press, on a real 120-column terminal, with the fifteen workspaces that make
        the count appear in the first place — and three processes, none of them this one:
        the row is painted by a `charter panel workspaces` child, the report is decoded
        there, and the palette is carved by a detached `charter frame-palette` that child
        starts.

        **The pane is the observable**, exactly as it is for a click on the attention row's
        doorway, and the keyboard following it is `overlay.modal_argvs` doing what `F2`
        does: a modal surface you cannot type into is not one. That is not the pointer
        moving the keyboard — a click on a TAB leaves it on the harness, which is
        `test_the_click_leaves_the_keyboard_on_the_harness`'s job two classes up and stays
        true.
        """
        before = self._panes()
        self.assertEqual(self._active(), self.harness)
        row = self._bar_row(self.bar)
        counts = [f.strip() for f in row.split("  ") if f.strip().startswith("+")]
        self.assertTrue(counts, f"this row carries no count to click: {row!r}")
        self._click(self.bar, col=self._column_of(self.bar, counts[0]))
        self.assertTrue(
            _await(lambda: len(self._panes()) > len(before)),
            f"no pane was carved off the harness, so the {counts[0]} is still inert — "
            f"row {self._bar_row(self.bar)!r}")
        opened = [p for p in self._panes() if p not in before]
        self.assertEqual(len(opened), 1, f"expected one new pane, got {opened}")
        self.assertTrue(
            _await(lambda: "workspace" in self._shown(opened[0])
                   or "chat" in self._shown(opened[0])),
            f"the new pane is not the palette: {self._shown(opened[0])!r}")

    def test_clicking_the_heading_still_opens_nothing(self):
        """The control the case above cannot do without: a row that answers EVERY click is
        as wrong as one that answers none. `workspaces` is a label — the frame naming what
        the row is about — and the cells it occupies were measured, not made live."""
        before = self._panes()
        self._click(self.bar, col=self._column_of(self.bar, "workspaces"))
        time.sleep(2.0)
        self.assertEqual(self._panes(), before,
                         "a click on the heading opened something")
        self.assertEqual(self._active(), self.harness)


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class ARealPressOnThePlusReachesTheCommandBehindIt(_ARealFrameWithBars,
                                                   unittest.TestCase):
    """The chat bar's `+`, pressed on a real terminal — #: *"`+` button not working for
    creating new session."*

    **Three processes and none of them is this one**, exactly as for a tab: the row is
    painted by a `charter panel chats` child, the report is decoded there, and
    `charter frame-new-chat` is started detached by that child.

    **What this can and cannot say, stated rather than left to be discovered.**
    `commands_frame.SOCKET` is a module constant — `"charter"`, the one shared server every
    frame on this machine runs on — and `cmd_launch` uses it rather than the server the
    frame recorded. So a case that let the launch RUN would build a session on the
    operator's own live charter server, from a suite. That is the one thing no test in this
    repository may do, so no test here does it: the launcher itself is asserted against a
    stand-in by `tests/test_the_chat_bars_plus_makes_a_chat.py`, and what is real here is
    everything up to it.

    Everything is a great deal: the affordance is at the column the paint put it in, tmux
    routes the report to a pane that is not active, the panel decodes it and tells a `+`
    from a tab, the child resolves which frame it belongs to out of its own environment,
    it establishes that this is charter's own server, it proves the workspace's session is
    this plane's (`_plane_session` — a real `list-panes` against a real server), and it
    reports on the frame's own attention row, which is the only surface a detached process
    with `/dev/null` for stdout has. Every one of those is a link that was not there
    before this change.

    **The stop this fixture reaches is the harness one, and it is reached honestly rather
    than arranged.** The plane declares no `[harness] default` and the chats this fixture
    plants record none, which is the migration state of every chat launched by a charter
    that predates `state.record_identity`. So the last link — "which harness does the new
    chat run" — has no answer, and the command says so instead of guessing. A case that
    passed this by faking a harness would have to let the launch happen.
    """

    WS = "alpha"

    #: A word out of :data:`commands_frame.cmd_new_chat`'s harness refusal, spelled by hand
    #: rather than imported: a constant read out of the module under test follows a
    #: reworded sentence into a green run, which is the survivor this repository's sweep
    #: reports. Short enough to survive a rewording that keeps the meaning, specific enough
    #: that no other refusal on this path says it.
    _NO_HARNESS = "records no harness this charter can launch"

    def setUp(self) -> None:
        super().setUp()
        self.here = f"{self.WS}.1"
        self.harness = self._start_session(self.here, session=self.WS)
        state.frame_dir(self.here, create=True)
        state.record_workspace(self.here, self.WS)
        state.record_server(self.here, self.socket)
        state.record_harness_pane(self.here, self.harness)
        state.record_identity(self.here, {"CHARTER_SESSION_ID": self.here,
                                          "CHARTER_ROOT": str(self.plane)})
        self._source_conf(self.WS)
        self.assertEqual(self._tmux("select-window", "-t", self.harness).returncode, 0)
        self.bar = self._split_bar(self.harness, "chats", self.here)
        self.assertEqual(self._tmux("select-pane", "-t", self.harness).returncode, 0)
        self.fd = self._attach(self.WS)
        self.assertTrue(
            _await(lambda: slots.ADD_CHAT in self._bar_row(self.bar).rstrip()),
            f"the chat bar never painted its `+`: {self._bar_row(self.bar)!r}")
        self.assertEqual(self._active(), self.harness)
        self.assertEqual(state.notice(self.here), "",
                         "this frame already carries a notice, so the cases below could "
                         "pass on something the fixture said")

    def _plus(self) -> int:
        """The column the `+` is drawn in, read off the PANE.

        Not `self._column_of`, because that helper asserts the field is unique on the row
        and a single `+` is a character an overflow count also starts with. The affordance
        is the LAST field on the row by construction, so the last `+` is it — and the row
        it is read off is the one tmux painted, which is what the operator's eye lands on.
        """
        row = self._bar_row(self.bar).rstrip()
        self.assertTrue(row.endswith(slots.ADD_CHAT),
                        f"the row does not end in the affordance: {row!r}")
        return len(row) - 1

    def test_a_press_on_the_plus_reaches_the_command_behind_it(self):
        """The whole chain, end to end, with the answer coming back on the frame's own
        row — which is where every outcome of a detached charter has to land."""
        self._click(self.bar, col=self._plus())
        self.assertTrue(
            _await(lambda: self._NO_HARNESS in state.notice(self.here)),
            f"the press never reached the command: {state.notice(self.here)!r} — "
            f"row {self._bar_row(self.bar)!r}")

    def test_the_press_leaves_the_keyboard_on_the_harness(self):
        """`docs/frame.md`'s promise, kept for the third gesture as well as the first two:
        a click on a PANEL acts where it points and does not move the keyboard. Nothing
        about making a chat changes that — the new chat's own window is what a successful
        press would select, and that is tmux moving a client, not a pointer moving focus.
        """
        self._click(self.bar, col=self._plus())
        self.assertTrue(_await(lambda: self._NO_HARNESS in state.notice(self.here)),
                        "the press never arrived, so this proves nothing about focus")
        self.assertEqual(self._active(), self.harness,
                         "a press on the `+` took the keyboard off the harness")

    def test_a_press_that_could_not_finish_created_nothing(self):
        """The negative that makes the refusal worth having: it stopped, and it stopped
        BEFORE anything existed. One window on the server and one chat on the plane, after
        a press that reached the command and declined."""
        before = self._tmux("list-windows", "-t", self.WS, "-F", "#{window_id}").stdout
        self._click(self.bar, col=self._plus())
        self.assertTrue(_await(lambda: self._NO_HARNESS in state.notice(self.here)))
        time.sleep(1.5)
        self.assertEqual(
            self._tmux("list-windows", "-t", self.WS, "-F", "#{window_id}").stdout,
            before, "a refused press still added a window")
        self.assertEqual(chats.of_workspace(self.WS), [self.here],
                         "a refused press still made a chat directory")

    def test_the_cell_beside_the_plus_reaches_nothing(self):
        """The control every affordance case needs: a row that answers EVERY click is as
        wrong as one that answers none. The gap before the `+` belongs to neither field —
        and its left-hand neighbour is a TAB, so picking the nearer one would switch
        instead of create."""
        self._click(self.bar, col=self._plus() - 1)
        time.sleep(2.0)
        self.assertEqual(state.notice(self.here), "",
                         "the gap before the `+` reached the command")

    def test_a_release_with_no_press_reaches_nothing(self):
        """§4i, measured end to end for the one gesture on this row that CREATES: a drag
        begun on a pane border delivers exactly one release, and every charter handler acts
        on the press for that reason."""
        left, top = self._rect(self.bar)
        col = self._plus()
        os.write(self.fd, b"\x1b[<0;%d;%dm" % (left + col + 1, top + 1))
        time.sleep(2.0)
        self.assertEqual(state.notice(self.here), "",
                         "an unpaired release reached the command")
