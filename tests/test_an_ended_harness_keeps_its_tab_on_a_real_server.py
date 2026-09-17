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

**And that last sentence is why the runner's tmux is pinned.** `ubuntu-latest` ships tmux
3.4, whose server can lose a SIGCHLD and then destroy the pane without firing `pane-died`
at all — so the reading this module takes off CI was, about one run in eight, not a reading
about charter. A gate whose answer is "whatever the runner says" has to be run on a runner
that can answer. `tests/test_ci_runs_a_tmux_that_reaps_its_children` holds the floor and
carries the mechanism and the measurements; `#1116` is the issue.

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

#: How long a case here waits for a `pane-died` hook's own `run-shell` child to land.
#:
#: **Sized for a CONTENDED runner rather than for a quiet laptop**, and the number is a
#: measurement rather than a guess. The whole of `AnExitOnARealServer` completes in 4.5 s on
#: a developer machine; on CI `test (3.12)` still exceeded the old 10 s while 3.11, 3.13 and
#: 3.14 passed on the same runner image. This matrix runs beside up to fourteen sweep shards
#: against an account-wide ceiling of twenty concurrent jobs, so the tmux server's
#: `run-shell` children are competing for a machine that is already full.
#:
#: **A failure that depends on the PYTHON version is contention, never a tmux capability**,
#: and that is what rules out reading it as open question 4. `_pane_died_write_hook_argv`'s
#: `${v:-N}` guarantees the file it writes is a parseable integer even for the signal death
#: that question is about — `_UNKNOWN_DEATH_CODE`, never an empty line — so `exit=None` is a
#: hook that has not run yet, not one that ran and wrote nothing. The failing run agreed:
#: `ended` was False too, so neither `pane-died[0]` nor `[1]` had left any trace at all.
#:
#: Raising this weakens no assertion. The same predicate must still come true, and the same
#: diagnostic is printed if it does not — what changes is only how long a starved runner is
#: given to get there.
#:
#: **The paragraph above is kept because it is the record, and the reasoning in it has since
#: been measured wrong.** The failure was NOT contention, and the Python-version argument
#: does not hold: `ubuntu-latest` ships tmux 3.4, whose server can lose a SIGCHLD and then
#: destroy the pane WITHOUT firing `pane-died` at all — `#{pane_dead}` `1` with
#: `#{pane_dead_status}` and `#{pane_dead_signal}` both empty, permanently. Which Python
#: happened to be red is scheduling noise on a rate, not evidence about a capability:
#: stripped of charter entirely the miss rate is 8 in 60 on tmux 3.4 and 0 in 40 on 3.5, and
#: this module's own gate case is 5 in 30 against 0 in 30. `_dying` and `_why` below now
#: print the two fields that would have said so at the time, and CI installs a tmux above
#: the floor (`tests/test_ci_runs_a_tmux_that_reaps_its_children`, #1116).
#:
#: **So this number is now belt and braces rather than the fix, and it is deliberately not
#: lowered here.** Nothing in the measurements above says what the right bound is on a
#: contended runner — they say the event either arrives promptly or never arrives — and
#: shrinking a bound on the strength of a finding about a different variable is how 45
#: came to be written on the strength of a finding about this one. Lower it when somebody
#: has the distribution of *arrival times* on the pinned tmux, and quote it here.
_HOOK_SECONDS = 45.0

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
                _eventually(lambda: state.exit_code(fid) is not None,
                            timeout=_HOOK_SECONDS),
                f"no exit code was recorded for {fid}:"
                f"{self._why(fid, state.harness_pane(fid) or '')}")
        return fid

    #: The three fields that together say HOW a pane died, asked in one round trip.
    #:
    #: **`#{pane_dead}` alone cannot tell the two failures apart, and that is #1116's whole
    #: history.** `server_destroy_pane` closes `wp->fd` — which is the entirety of
    #: `#{pane_dead}` — and only afterwards decides whether to `notify_pane("pane-died")`.
    #: On tmux 3.4 it can return before that notify (`server-fn.c:329`,
    #: `remain_on_exit != 0 && (~wp->flags & PANE_STATUSREADY)`), because `PANE_STATUSREADY`
    #: is set only by `server_child_exited` and that runs off a SIGCHLD the server can lose.
    #: So a pane reads `dead=1` with BOTH the status and the signal EMPTY and no hook has
    #: run or ever will.
    #:
    #: Read `dead=1` on its own and that looks exactly like "the hook fired and charter
    #: mishandled it" — which is the reading #1116 recorded, and it sent the investigation
    #: at a wall-clock bound that was never the variable. The two empty fields beside it are
    #: what name the real thing.
    _DYING = "#{pane_dead}|#{pane_dead_status}|#{pane_dead_signal}"

    def _dying(self, pane: str) -> tuple[str, str, str]:
        """``(dead, status, signal)`` for *pane*, each exactly as tmux spelled it.

        Empty strings are answers here, not missing data: an empty `#{pane_dead_status}`
        is tmux saying it has no status to report, which is a different fact from `0`.
        """
        said = self._tmux("display-message", "-p", "-t", pane, self._DYING).stdout.strip()
        fields = (said.split("|") + ["", "", ""])[:3]
        return fields[0], fields[1], fields[2]

    def _dead(self, pane: str) -> str:
        return self._dying(pane)[0]

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
        """Everything the next failure of this shape needs in order to name itself.

        **`dead=` used to be the only death field here, and the omission cost a whole
        investigation.** #1116 reads a real failure — `dead='1' ended=False exit=None` —
        as contention, on the grounds that the `exit 0` arm "certainly has a status", and
        raises a wall-clock bound accordingly. The status was the one thing the listing did
        not print. On tmux 3.4 it is empty, along with the signal, and the pane-died hook
        never ran: see `_DYING`. A diagnostic that prints `dead=1` and stops is a
        diagnostic that can only be read as charter's fault.

        The server's own version goes in for the same reason — it is the discriminator
        between "charter is wrong" and "this tmux cannot say", and it is asked of the
        socket running these panes rather than of `tmux -V` on `$PATH`, so a runner with
        two tmuxes reports the one that actually owns the pane.
        """
        dead, status, sig = self._dying(pane)
        return (f"\n  chat={fid} pane={pane} dead={dead!r}"
                f" pane_dead_status={status!r} pane_dead_signal={sig!r}"
                f"\n  (legend: dead=1 with BOTH of those EMPTY means tmux destroyed the"
                f" pane without ever firing pane-died — a SIGCHLD the server lost, not"
                f" charter. Below tmux 3.5 that is about 1 run in 8. See #1116.)"
                f"\n  server tmux={self._tmux('display-message', '-p', '#{version}').stdout.strip()!r}"
                f"\n  ended={state.is_ended(fid)!r} exit={state.exit_code(fid)!r}"
                f" drawn={state.was_drawn(fid)!r}"
                f"\n  windows={self._tmux('list-windows', '-a', '-F', '#{window_name}').stdout.split()}"
                f"\n  panes (id|dead|status|signal|chat|plane|drawer)=\n    " + self._tmux(
                    "list-panes", "-a", "-F",
                    "#{pane_id}|#{pane_dead}|#{pane_dead_status}|#{pane_dead_signal}"
                    "|#{@charter_chat}|#{@charter_plane}"
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
                    _eventually(lambda: state.is_ended(fid), timeout=_HOOK_SECONDS),
                    f"the ended step never claimed this exit:{self._why(fid, pane)}")
                self.assertIn(fid, self._tmux("list-windows", "-a", "-F",
                                              "#{window_name}").stdout.split(),
                              f"the tab did not survive:{self._why(fid, pane)}")

                if exits == 0:
                    self.assertTrue(
                        _eventually(lambda: self._dead(pane) == "0",
                                    timeout=_HOOK_SECONDS),
                        f"a clean exit did not respawn the pane into the selector:"
                        f"{self._why(fid, pane)}")
                else:
                    self.assertEqual(self._dead(pane), "1",
                                     f"a killed harness's pane was not left alone:"
                                     f"{self._why(fid, pane)}")
                    self.assertTrue(
                        _eventually(lambda: any(r.rsplit("|", 1)[-1] == fid
                                                for r in self._drawers()),
                                    timeout=_HOOK_SECONDS),
                        f"no drawer opened for the killed chat:{self._why(fid, pane)}")

    def test_the_last_chat_ending_keeps_the_session(self):
        """**The one the IDE spec left open** (§5): keeping the last chat of a workspace as
        an ended tab is what `attach` was feared to block on. It does not — the window is
        still there, so the session is too, and `charter claude` returns when the tab is
        closed rather than when the harness exits (reading E4)."""
        fid = self._start(exits=0)

        self.assertTrue(_eventually(lambda: state.is_ended(fid),
                                    timeout=_HOOK_SECONDS))

        self.assertIn("beta", self._tmux("list-sessions", "-F",
                                         "#{session_name}").stdout.split(),
                      "the workspace's session went with its last chat's harness")


if __name__ == "__main__":
    unittest.main()
