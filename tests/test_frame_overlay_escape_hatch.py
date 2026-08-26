"""The escape hatch, pressed on a real client against a DELIBERATELY HUNG overlay.

**This is the safety property of the whole command surface, and it cannot be tested
against a cooperative overlay.** An overlay that reads its tty and exits politely proves
nothing at all: the question is whether the key still works when charter's own loop is
stuck, which is precisely when nothing charter wrote gets to run. So the pane here runs a
program that puts its tty in **raw mode**, ignores `SIGWINCH`, and then sleeps — it never
reads a byte and never will.

**Why a real tmux and a real attached client.** `tmux send-keys` feeds a PANE's own input
queue and never touches the key table, so it cannot exercise a `bind -n` at all — the
same fact `tests/test_frame_tmux_integration.py` records for `display-menu`. The hatch's
entire mechanism is that tmux matches the key in its ROOT table *before* the bytes reach
the pane, so the only honest test is a client on a pty with the key written to it as a
terminal would.

**The negative half is not a control against a different mechanism — it is the proof the
overlay is really wedged.** Ordinary keys are written first and asserted to change
nothing: not the active pane, not the overlay's own screen. Without that, "F12 moved the
focus" would be equally satisfied by an overlay that had simply exited on its own.

Skipped, never failed, where the machine cannot supply what this needs: no tmux, a tmux
too old for `run-shell -C` (3.2), or a tmux that will not attach a client (a headless CI
step's `TERM=dumb`). Each skip names what was missing, and every test that DOES run
asserts exactly what it always did.
"""

from __future__ import annotations

import os
import pty
import shutil
import subprocess
import time
import unittest

from charter import commands_frame
from charter.frame import overlay, tmuxctl

_HAS_TMUX = shutil.which("tmux") is not None

#: This module's own server, unique per test PROCESS so an interrupted earlier run's
#: socket can never be mistaken for this one's.
SOCKET = f"charter-overlay-hatch-{os.getpid()}"

#: Where tmux puts :data:`SOCKET`'s FILE, computed the way tmux computes it — and only
#: the FALLBACK for it. `TheHatch._teardown_socket` asks the live server for
#: `#{socket_path}` first, because tmux is the authority on its own path and this
#: expression is only a COPY of a rule that lives in tmux's source (`$TMUX_TMPDIR` or
#: `/tmp`, then `tmux-<uid>/<name>`) — a copy that can silently drift out of date, which
#: is why nothing is ever *asserted* about it. It is spent on the one teardown that has
#: no server left to ask, and every teardown that does have one makes its claim against
#: what tmux said instead.
SOCKET_PATH = os.path.join(os.environ.get("TMUX_TMPDIR") or "/tmp",
                           f"tmux-{os.getuid()}", SOCKET)

#: How long a tmux state change gets before this gives up on it. Generous, and spent only
#: on the way to a failure: every wait below returns the instant the state is right.
_DEADLINE = 20.0

#: The overlay that will never answer. Raw mode so the tty is genuinely captured, a
#: `SIGWINCH` handler that does nothing so a resize cannot shake it loose, one marker
#: written so `capture-pane` has something to compare against, and then a sleep. The
#: closest thing to a third-party component that "captured input badly" (§4c) that can be
#: written on purpose.
_WEDGED = (
    "python3 -c \""
    "import sys,tty,signal,time;"
    "sys.stdout.write('WEDGED-OVERLAY');sys.stdout.flush();"
    "tty.setraw(sys.stdin.fileno());"
    "signal.signal(signal.SIGWINCH, signal.SIG_IGN);"
    "time.sleep(9999)\"")

#: What a terminal sends for :data:`overlay.HATCH_KEY`. `\\x1b[24~` is the standard CSI
#: form for F12 and is in tmux's own built-in key table (`tty-keys.c`) rather than coming
#: from terminfo, so it does not depend on which `TERM` the client attached with. Asserted
#: against `HATCH_KEY` by :meth:`TheHatch.test_the_key_this_module_presses_is_the_key_charter_binds`
#: so the two cannot drift into a test that presses something charter does not bind.
_F12 = b"\x1b[24~"

_TERM_CANDIDATES = tuple(dict.fromkeys(
    ([os.environ["TERM"]] if os.environ.get("TERM", "dumb") != "dumb" else [])
    + ["xterm-256color", "screen", "vt100"]))


def _tmux(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run(["tmux", "-L", SOCKET, *args], capture_output=True, text=True,
                          timeout=15)


def _panes() -> dict[str, str]:
    """``{pane id: "1"|"0"}`` — which pane this window's client is typing into."""
    out = _tmux("list-panes", "-t", "s", "-F", "#{pane_id} #{pane_active}").stdout
    return dict(line.split() for line in out.splitlines() if " " in line)


def _await(predicate, timeout: float = _DEADLINE) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(0.05)
    return predicate()


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class TheHatch(unittest.TestCase):
    """One frame, one wedged overlay, one keypress."""

    def setUp(self) -> None:
        v = tmuxctl.version()
        if v is None or v < tmuxctl.FLOOR:
            self.skipTest(f"`run-shell -C` first exists in tmux {tmuxctl.FLOOR[0]}."
                          f"{tmuxctl.FLOOR[1]}; this machine has {v}")
        # Registered FIRST so it runs LAST — `addCleanup` is LIFO, and every client this
        # test forks onto a pty must be reaped before the server it is attached to goes.
        self.addCleanup(self._teardown_socket)
        started = _tmux("-f", "/dev/null", "new-session", "-d", "-s", "s",
                        "-x", "100", "-y", "30", "-P", "-F", "#{pane_id}", "cat")
        self.assertEqual(started.returncode, 0, started.stderr)
        self.harness = started.stdout.strip()
        # A panel, so "the hatch returned to the HARNESS" is a real claim rather than
        # "the only other pane left". With one panel there are three panes and two wrong
        # answers.
        split = _tmux("split-window", "-t", self.harness, "-v", "-l", "3",
                      "-P", "-F", "#{pane_id}", "cat")
        self.assertEqual(split.returncode, 0, split.stderr)
        self.panel = split.stdout.strip()
        # Charter's OWN frame config, sourced the way a launch sources it — not a
        # hand-written `bind` line standing in for it. A test that re-spelled the bind
        # would be measuring its own copy of charter's answer (#547), and the quoting
        # inside that line is exactly the part worth putting through tmux's real parser.
        conf = os.path.join(
            os.environ.get("TMPDIR", "/tmp"), f"charter-hatch-{os.getpid()}.conf")
        with open(conf, "w") as fh:
            fh.write(commands_frame.conf_text(hotkey="F2", mouse=False,
                                              history_limit=100, session="s"))
        self.addCleanup(lambda: os.path.exists(conf) and os.unlink(conf))
        sourced = _tmux("source-file", conf)
        self.assertEqual(sourced.returncode, 0, sourced.stderr)
        # Read back off tmux's own key table, not off the string charter produced: this
        # is the one assertion that the quoting in that line survived tmux's parser.
        bound = [ln for ln in _tmux("list-keys", "-T", "root").stdout.splitlines()
                 if f" {overlay.HATCH_KEY} " in ln]
        self.assertEqual(len(bound), 1, bound)
        self.assertIn(overlay.HATCH_OPTION, bound[0])
        self.assertIn("run-shell -C", bound[0])

    def _teardown_socket(self) -> None:
        """End the server and take its socket FILE with it, in that order and in ONE
        cleanup — `tests/test_frame_tmux_integration.py::_TmuxServerFixture` verbatim,
        because this module had the same shared socket and was missing the second half.

        **`kill-server` is a signal, not a wait, and the socket keeps ANSWERING while
        the server is on its way out.** tmux's `cmd_kill_server_exec` is two statements —
        `kill(getpid(), SIGTERM)` and `return (CMD_RETURN_NORMAL)` — so the server
        signals ITSELF and then answers this client normally: `kill-server` returns 0
        with the listening socket still bound and still accepting. A client that connects
        in that window reads EOF where a reply should be, which is `client_dispatch`'s
        `imsg == NULL` branch setting `CLIENT_EXIT_LOST_SERVER`, whose message is the
        string `server exited unexpectedly` and whose status is 1. That is the whole of
        the CI failure at `c735efc`: every test here shares one socket, so the next
        test's `new-session` was racing the previous test's teardown, and it lost two of
        the five matrix jobs that got far enough to run.

        Measured rather than reasoned: on tmux 3.7c, connecting to the socket kept
        SUCCEEDING for a median of 0.4 ms and up to 1.3 ms after `kill-server` had
        already returned (24 of 25 trials landed at least one connect), and the socket
        file was still on disk in 25 of 25. That is why no local run sees this — a whole
        `subprocess` round trip between two tests is 10-20 ms on an unloaded machine,
        which steps clean over a sub-millisecond window — and why a loaded four-core
        runner walks straight into it: the CI log has the previous test finishing at
        `.0170` and this one failing at `.0221`, 5 ms later.

        Unlinking closes the window rather than waiting it out. A client whose socket
        path is not there does not connect to a dying server at all: it starts a fresh
        one. There is nothing left to wait FOR, which is the discipline the news entry
        slugged `a-green-tmux-run-now-means-something` states for this whole module
        family — a `sleep` long enough to cover the window would be a guess at a number
        that was measured right here, and a poll for the server's absence would still be
        polling the one call that can fail.

        ONE cleanup doing both, never two registered separately: `addCleanup` runs LIFO,
        so `addCleanup(kill-server)` followed by `addCleanup(unlink)` unlinks FIRST and
        kills SECOND — which points `kill-server` at a path with no server on it, leaves
        the real one running, and hands the next test a socket file it did not make.

        **And the path comes from tmux, which is the only reason the check below is
        worth anything.** The first version of this asserted that the paths it had
        already decided to unlink were gone — which a wrong :data:`SOCKET_PATH` satisfies
        for free, because a path that was never the socket does not exist either before
        or after. Measured: with tmux's answer suppressed and `SOCKET_PATH` pointed at a
        path that is not the socket, the module went green and left the real socket file
        standing, race and all. So the claim is made against tmux's OWN answer, and a
        running server that will not give one is a failure rather than a fallback.
        """
        said = _tmux("display-message", "-p", "#{socket_path}")
        # rc tells "no server to ask" apart from "a server that would not answer", and
        # only the second one is a defect: a teardown with no server has nothing to
        # claim, and a tmux whose `#{socket_path}` expands to nothing has hidden the one
        # fact this depends on.
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
        self.assertTrue(
            path.startswith("/"),
            f"tmux would not say where its socket is — `#{{socket_path}}` gave {path!r}, "
            f"so this teardown is unlinking a guess ({SOCKET_PATH}) and cannot tell "
            f"whether it removed the socket the next test will race")
        self.assertFalse(
            os.path.exists(path),
            f"{path} survived this test's teardown, so the next test's `new-session` "
            f"can still reach a server that is exiting")

    def _open_a_wedged_overlay(self) -> str:
        """Charter's own open path, with a program that will never answer in the pane."""
        argv = overlay.open_argv(SOCKET, harness=self.harness, command=["sh", "-c", _WEDGED])
        self.assertIsNotNone(argv)
        opened = subprocess.run(argv, capture_output=True, text=True, timeout=15)
        self.assertEqual(opened.returncode, 0, opened.stderr)
        pane = opened.stdout.strip()
        for cmd in overlay.modal_argvs(SOCKET, harness=self.harness, overlay_pane=pane):
            got = subprocess.run(cmd, capture_output=True, text=True, timeout=15)
            self.assertEqual(got.returncode, 0, got.stderr)
        self.assertTrue(_await(lambda: "WEDGED-OVERLAY" in
                               _tmux("capture-pane", "-p", "-t", pane).stdout),
                        "the overlay pane never drew anything")
        return pane

    def _attach(self) -> int:
        """A real `tmux attach` on a pty. Returns the master fd; the client is this
        process's own child to reap."""
        refusals = []
        for term in _TERM_CANDIDATES:
            pid, fd = pty.fork()
            if pid == 0:
                try:
                    os.environ["TERM"] = term
                    os.execvp("tmux", ["tmux", "-L", SOCKET, "attach", "-t", "s"])
                finally:
                    os._exit(127)
            if _await(lambda: bool(_tmux("list-clients", "-t", "s",
                                         "-F", "#{client_name}").stdout.strip()),
                      timeout=10.0):
                self.addCleanup(self._reap, pid, fd)
                return fd
            refusals.append(term)
            self._reap(pid, fd)
        self.skipTest("no tmux client will attach on this machine, and a key table entry "
                      "needs one — tried TERM=" + ", ".join(refusals))

    @staticmethod
    def _reap(pid: int, fd: int) -> None:
        for call in (lambda: os.kill(pid, 9), lambda: os.waitpid(pid, 0),
                     lambda: os.close(fd)):
            try:
                call()
            except OSError:
                pass

    def test_the_key_this_module_presses_is_the_key_charter_binds(self):
        """`\\x1b[24~` is F12. If `HATCH_KEY` ever moves, this test must move with it —
        otherwise every assertion below would be pressing a key charter does not bind and
        measuring an overlay that simply never closed."""
        self.assertEqual(overlay.HATCH_KEY, "F12")

    def test_one_key_returns_to_the_harness_from_an_overlay_that_never_answers(self):
        pane = self._open_a_wedged_overlay()
        fd = self._attach()
        self.assertTrue(_await(lambda: _panes().get(pane) == "1"),
                        "the overlay pane never became the active one")

        # The overlay really is wedged: ordinary keys reach it and change nothing.
        before = _tmux("capture-pane", "-p", "-t", pane).stdout
        os.write(fd, b"qqq\r\x1b[A\x1b[B")
        time.sleep(1.0)
        self.assertEqual(_panes().get(pane), "1",
                         "an ordinary key moved the focus — the overlay was not modal")
        self.assertEqual(_tmux("capture-pane", "-p", "-t", pane).stdout, before,
                         "the overlay answered a key — it is not the wedge this tests")

        os.write(fd, _F12)
        self.assertTrue(_await(lambda: _panes().get(self.harness) == "1"),
                        f"{overlay.HATCH_KEY} did not return the operator to the harness")
        self.assertTrue(_await(lambda: pane not in _panes()),
                        "the wedged overlay pane is still in the window")
        self.assertIn(self.panel, _panes(),
                      "the hatch took the panel with it")

    def test_the_window_the_overlay_covered_comes_back(self):
        """Zoom is what makes it modal; killing the pane is what gives the frame back."""
        pane = self._open_a_wedged_overlay()
        fd = self._attach()
        self.assertTrue(_await(lambda: _tmux(
            "display-message", "-p", "-t", "s", "#{window_zoomed_flag}").stdout.strip()
            == "1"), "the overlay never covered the window")
        os.write(fd, _F12)
        self.assertTrue(_await(lambda: _tmux(
            "display-message", "-p", "-t", "s", "#{window_zoomed_flag}").stdout.strip()
            == "0"), "the window is still zoomed after the hatch")

    def test_with_no_overlay_open_the_key_still_returns_to_the_harness(self):
        """"Unconditionally, from any state" includes the state with no overlay in it —
        and the measured trap is here: an unset option expanding into `kill-pane -t ""`
        kills the CURRENT pane, so this asserts the panel is still standing."""
        armed = overlay.arm_hatch_argv(SOCKET, harness=self.harness)
        self.assertIsNotNone(armed)
        self.assertEqual(subprocess.run(armed, capture_output=True, timeout=15)
                         .returncode, 0)
        fd = self._attach()
        got = _tmux("select-pane", "-t", self.panel)
        self.assertEqual(got.returncode, 0, got.stderr)
        self.assertTrue(_await(lambda: _panes().get(self.panel) == "1"))
        os.write(fd, _F12)
        self.assertTrue(_await(lambda: _panes().get(self.harness) == "1"),
                        f"{overlay.HATCH_KEY} did not return to the harness")
        self.assertIn(self.panel, _panes(),
                      "the hatch killed a pane when there was no overlay to kill")

    def test_the_overlay_pane_is_split_in_rows_and_tmux_accepts_the_size(self):
        """Both halves of `open_argv`'s split, measured rather than restated.

        `-l` is rows under `-v` and columns under `-h`, so the flag and the number are
        one decision: under the other flag this same `5` is a five-COLUMN pane. And
        `_SPLIT_ROWS`' own docstring names the single way the number can cost anything —
        tmux refusing the split for want of room — which is a non-zero return code from
        the very command this runs, in a window deliberately shorter than a laptop's.
        """
        short = _tmux("new-window", "-t", "s", "-P", "-F", "#{pane_id}", "cat")
        self.assertEqual(short.returncode, 0, short.stderr)
        harness = short.stdout.strip()
        self.assertEqual(_tmux("resize-window", "-t", "s", "-x", "80", "-y", "14")
                         .returncode, 0)
        argv = overlay.open_argv(SOCKET, harness=harness, command=["cat"])
        self.assertIsNotNone(argv)
        opened = subprocess.run(argv, capture_output=True, text=True, timeout=15)
        self.assertEqual(opened.returncode, 0,
                         f"tmux refused the split: {opened.stderr}")
        pane = opened.stdout.strip()
        size = _tmux("display-message", "-p", "-t", pane,
                     "#{pane_height} #{pane_width}").stdout.split()
        self.assertEqual(size[0], str(overlay._SPLIT_ROWS),
                         f"the overlay pane came out {size} — `-l` counted columns")
        self.assertEqual(size[1], "80", f"the overlay pane came out {size}")

    def test_a_close_that_cannot_name_the_overlay_kills_nothing(self):
        """The module's leading measurement, run rather than described.

        `kill-pane -t ""` is not a no-op: measured again for this call site — a plain
        CLI invocation, not the hatch's `run-shell` — `tmux -L … kill-pane -t ""`
        returns 0 and kills the session's ACTIVE pane, silently. `close_argvs` takes the
        overlay's id as a plain string with no `None` spelling, so an unset variable or
        a `#{pane_id}` capture that came back empty arrives here as `""`; what this
        asserts is that whatever it produces for one can be run against a real server
        with the harness and a panel standing and leave both of them standing.
        """
        before = set(_panes())
        self.assertGreaterEqual(len(before), 2, before)
        for bad in ("", " ", "%", "$overlay"):
            with self.subTest(overlay_pane=bad):
                for cmd in overlay.close_argvs(SOCKET, harness=self.harness,
                                               overlay_pane=bad):
                    subprocess.run(cmd, capture_output=True, timeout=15)
                self.assertEqual(set(_panes()), before,
                                 f"a close naming {bad!r} took a pane with it")

    def test_charter_own_close_path_hands_the_pane_back_too(self):
        """The ordinary exit, so the hatch is the exception rather than the mechanism."""
        pane = self._open_a_wedged_overlay()
        for cmd in overlay.close_argvs(SOCKET, harness=self.harness, overlay_pane=pane):
            got = subprocess.run(cmd, capture_output=True, text=True, timeout=15)
            self.assertEqual(got.returncode, 0, got.stderr)
        self.assertTrue(_await(lambda: pane not in _panes()))
        self.assertEqual(_panes().get(self.harness), "1")
        self.assertEqual(
            _tmux("show-options", "-w", "-t", self.harness, "-v",
                  overlay.HATCH_OPTION).stdout.strip(),
            overlay.hatch_command(harness=self.harness),
            "the hatch is still armed to kill a pane that is gone")


if __name__ == "__main__":
    unittest.main()
