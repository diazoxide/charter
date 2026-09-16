"""**A harness that ends keeps its tab — against a real tmux, with a real dead pane.**

Everything in `tests/test_an_ended_harness_keeps_its_tab.py` is charter's own reading of a
listing it was handed. This module is the half no fake can answer: tmux really fires
`pane-died[1]`, the ended step really runs as a `run-shell -b` child of the server, and the
pane it respawns or splits beside is really dead.

**`test_a_signal_death_ends_the_tab` is a pre-merge gate on this task** (the controller's
ruling on open question 4, `docs/superpowers/plans/2026-09-15-one-exit-gate.md:914-921`):

* it runs **unskipped** — there is no version gate and no opt-out on it beyond tmux being
  installed at all, which the whole module needs;
* its stand-in harness **enters raw mode** (`tty.setraw`) before it exits or is killed,
  because that is the state the question is about: a cancelled selector pane on a Linux CI
  runner came back dead with BOTH `#{pane_dead_status}` and `#{pane_dead_signal}` empty
  (`launcher._close_the_cancelled_chat`'s docstring, and ruling 42's own measurement), and a
  stand-in that never owned the terminal would be measuring something else;
* it covers **exit 0 and SIGKILL**, which are the two endings that must be told apart — a
  clean exit respawns the pane into the selector, and a death charter cannot name a code for
  leaves the pane alone and opens the drawer.

If the hook does not fire on Linux for a pane tmux can name neither a status nor a signal
for, this case is what says so, with the listing in the failure. It is not skipped there and
it is not softened: the answer to open question 4 is whatever this reports on CI's runner.

**No shim reaches inside these panes.** The plan's task 2 named a `PYTHONPATH` shim standing
in `selector.pick` and `ended.draw`, citing `TheSelectorOnARealServer`; that shim does not
exist there or anywhere in this suite — that class patches nothing inside the pane and drives
it with real keystrokes. So this module asserts on what a real server can be asked: the pane's
own `#{pane_dead}`, the window still being listed, the chat's recorded state, and the
`@charter_drawer` marker one listing proves.
"""

from __future__ import annotations

import os
import shutil
import signal
import sys
import unittest

from charter.frame import state

from tests._isolation import wired_as_today
from tests.test_a_chat_pane_starts_as_charter_and_becomes_the_harness import (
    _ARealChatOnARealServer, _eventually)

_HAS_TMUX = shutil.which("tmux") is not None

_WIRED = None


def setUpModule():
    global _WIRED
    _WIRED = wired_as_today()
    _WIRED.start()


def tearDownModule():
    if _WIRED is not None:
        _WIRED.stop()


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class AnExitOnARealServer(_ARealChatOnARealServer, unittest.TestCase):
    """The two presentations, driven by a real harness really ending in a real pane."""

    SLUG = "ended-real"

    def _raw_mode_harness(self, *, exits: int | None) -> None:
        """Rewrite the fixture's recorder so it OWNS THE TERMINAL before it ends.

        The base fixture's stand-in records and exits without touching the tty. Open
        question 4 is about a pane whose status and signal both come back empty, and the
        measurement charter already has (ruling 42) puts that state after a process leaves
        RAW MODE — so a stand-in that never entered it would answer a different question.

        *exits* is the code to exit with, or ``None`` to hold until something kills it.
        """
        binary = self.tmp / "bin" / "claude"
        binary.write_text(
            f"#!{sys.executable}\n"
            "import json, os, sys, time, tty\n"
            # The wiring probe first (ruling 10): the launcher asks this binary
            # `plugin list --json` before it execs it, and a recorder that recorded THAT
            # call would leave the launch refusing and overwrite the measurement.
            "if sys.argv[1:3] == ['plugin', 'list']:\n"
            "    json.dump([{'id': 'charter@charter', 'scope': 'user', 'enabled': True,\n"
            "                'installedAt': '2026-09-12T00:00:00Z'}], sys.stdout)\n"
            "    sys.exit(0)\n"
            "out = os.path.join(os.environ['RECORD_DIR'], 'harness.json')\n"
            "with open(out + '.tmp', 'w') as f:\n"
            "    json.dump({'argv': sys.argv[1:], 'pid': os.getpid()}, f)\n"
            "os.replace(out + '.tmp', out)\n"
            # **Raw mode, and the failure is swallowed deliberately.** A pane with no tty
            # would otherwise take the whole case down before it measured anything; what
            # matters is that the harness owned the terminal wherever it could, which is
            # every real pane this runs in.
            "try:\n"
            "    tty.setraw(sys.stdin.fileno())\n"
            "except Exception:\n"
            "    pass\n"
            + ("time.sleep(300)\n" if exits is None else f"sys.exit({exits})\n"))
        binary.chmod(0o755)

    def _start(self, *, exits: int | None) -> str:
        """Launch one chat whose harness ends the way *exits* says. Answers its id.

        **It proves the harness RAN before anything is measured**, and that is not
        belt-and-braces: every assertion in this module is about what happens after a
        harness ends, so a launch where none ever started would satisfy all of them
        vacuously and report a gate as met. `_harness()` waits for the record the stand-in
        writes in the pane and fails with the pane's own text if it never arrives — the
        difference between "the harness exited" and "the harness was never there".
        """
        (self.records / "harness.json").unlink(missing_ok=True)
        self._raw_mode_harness(exits=exits)
        before = set(self._tmux("list-windows", "-a", "-F",
                                "#{window_name}").stdout.split())
        self.assertEqual(self._launch(), 0)
        after = set(self._tmux("list-windows", "-a", "-F",
                               "#{window_name}").stdout.split())
        new = sorted(after - before)
        self.assertEqual(len(new), 1, f"expected one new window, got {new!r}")
        fid = new[0]
        ran = self._harness()
        self.assertTrue(ran.get("pid"), f"the stand-in harness never ran in {fid}'s pane")
        if exits is not None:
            # And the exit-status hook really fired, which is what makes the ended step
            # reachable at all: `pane-died[0]` writes the code that decides which
            # presentation the tab gets.
            #
            # **Only for a harness that ends ON ITS OWN.** The held one is still running
            # here — it is killed by the caller, after this returns — so waiting for its
            # code would be waiting for something nothing has caused yet.
            self.assertTrue(
                _eventually(lambda: state.exit_code(fid) is not None, timeout=10.0),
                f"no exit code was recorded for {fid}:"
                f"{self._why(fid, state.harness_pane(fid) or '')}")
        return fid

    def _dead(self, pane: str) -> str:
        return self._tmux("display-message", "-p", "-t", pane,
                          "#{pane_dead}").stdout.strip()

    def _drawers(self) -> list[str]:
        """Every pane one listing proves is a drawer, by the marker charter wrote.

        `|` rather than a TAB: this is a raw `tmux` without `-u`, and #984 measured a client
        whose environment names no UTF-8 locale getting a literal tab back as `_`.
        """
        return [row for row in self._tmux("list-panes", "-a", "-F",
                                          "#{pane_id}|#{@charter_drawer}"
                                          ).stdout.splitlines()
                if row.rsplit("|", 1)[-1]]

    def _why(self, fid: str, pane: str) -> str:
        return (f"\n  chat={fid} pane={pane} dead={self._dead(pane)!r}"
                f"\n  ended={state.is_ended(fid)!r} exit={state.exit_code(fid)!r}"
                f" drawn={state.was_drawn(fid)!r}"
                f"\n  windows={self._tmux('list-windows', '-a', '-F', '#{window_name}').stdout.split()}"
                f"\n  panes=\n    " + self._tmux(
                    "list-panes", "-a", "-F",
                    "#{pane_id}|#{pane_dead}|#{@charter_chat}|#{@charter_plane}"
                    "|#{@charter_drawer}").stdout.strip().replace("\n", "\n    "))

    def test_a_signal_death_ends_the_tab(self):
        """**Open question 4's gate.** Unskipped, raw-mode stand-in, exit 0 and SIGKILL.

        A clean exit must put the selector back in the same pane; a `SIGKILL` — which is the
        ending tmux may be unable to name a status OR a signal for — must leave the dead pane
        exactly as it is and open the drawer beside it. Either way the tab survives, which is
        the whole of decision 4.
        """
        for ending, exits in (("exit 0", 0), ("SIGKILL", None)):
            with self.subTest(ending=ending):
                fid = self._start(exits=exits)
                pane = state.harness_pane(fid)
                self.assertTrue(pane, f"no harness pane recorded for {fid}")
                if exits is None:
                    os.kill(self._harness()["pid"], signal.SIGKILL)

                self.assertTrue(
                    _eventually(lambda: state.is_ended(fid), timeout=5.0),
                    f"the ended step never claimed this exit:{self._why(fid, pane)}")
                self.assertIn(fid, self._tmux("list-windows", "-a", "-F",
                                              "#{window_name}").stdout.split(),
                              f"the tab did not survive:{self._why(fid, pane)}")

                if exits == 0:
                    self.assertTrue(
                        _eventually(lambda: self._dead(pane) == "0", timeout=5.0),
                        f"a clean exit did not respawn the pane into the selector:"
                        f"{self._why(fid, pane)}")
                else:
                    self.assertEqual(self._dead(pane), "1",
                                     f"a killed harness's pane was not left alone:"
                                     f"{self._why(fid, pane)}")
                    self.assertTrue(
                        _eventually(lambda: any(r.rsplit("|", 1)[-1] == fid
                                                for r in self._drawers()), timeout=5.0),
                        f"no drawer opened for the killed chat:{self._why(fid, pane)}")

    def test_the_last_chat_ending_keeps_the_session(self):
        """**The one the IDE spec left open** (§5): keeping the last chat of a workspace as
        an ended tab is what `attach` was feared to block on. It does not — the window is
        still there, so the session is too, and `charter claude` returns when the tab is
        closed rather than when the harness exits (reading E4)."""
        fid = self._start(exits=0)

        self.assertTrue(_eventually(lambda: state.is_ended(fid), timeout=5.0))

        self.assertIn("beta", self._tmux("list-sessions", "-F",
                                         "#{session_name}").stdout.split(),
                      "the workspace's session went with its last chat's harness")


if __name__ == "__main__":
    unittest.main()
