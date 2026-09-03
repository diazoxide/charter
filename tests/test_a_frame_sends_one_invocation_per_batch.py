"""#780: what charter says to tmux is unchanged; how many times it says it is not.

The operator's report was *"very slow rendering on switching — it seems re-rendering all
and I see jumping texts ~0.2 sec, then it's ready"*, and both halves of it are one defect:
charter issued every tmux command on its own, and a round trip is ~5 ms on the machine
this was written on and ~13.4 ms on the one that filed #728. Measured with a real client at
200x50 and four panels, before this change and after it:

===================  ==================  ====================  ====================
path and tmux        invocations         wall clock            terminal repaints
===================  ==================  ====================  ====================
switch, 3.7c         58 -> 23            314 ms -> 142 ms      45 -> 14
switch, 3.2          50 -> 21            237 ms -> 114 ms      41 -> 12
launch, 3.7c         46 -> 17            227 ms -> 91 ms       (no client yet)
launch, 3.2          42 -> 16            174 ms -> 69 ms       (no client yet)
===================  ==================  ====================  ====================

The repaint column is the *"jumping"*: tmux redraws once per command LIST rather than once
per command, so four splits sent one at a time are four screen updates ~5 ms apart and
four sent as one list are one. `docs/frame.md` carries the same table for a reader.

**Three kinds of test, because the claim has three parts.**

1. **What tmux does with a command list** (:class:`WhatARealTmuxDoesWithACommandList`) —
   the two facts every batch here rests on, asked of a real server rather than believed:
   a chain answers for every command in it, and a refused command ABORTS the rest. Both
   were measured by hand on tmux 3.7c and at the 3.2 floor first; this is what makes them
   fail loudly if a later tmux changes its mind. It needs a tmux SERVER and no client, so
   it runs in CI.
2. **What charter now spends** (:class:`ALaunchAndASwitchSpendWhatTheyMeasured`) — the
   invocation counts, against the suite's own fakes, on a machine with no tmux at all.
   A count is the only assertion that catches the regression this issue is about: every
   other test here reads `fake.calls`, which holds one entry per tmux COMMAND and is
   deliberately unchanged by batching (`tests/_tmuxchain.py`).
3. **What a failure still costs** (:class:`AFailedBatchIsEveryCallItUsedToBe`) — the
   fallback, which is the whole reason this is `tmuxctl.write_all` and not `chain`.
   Charter's callers are written around each write failing on its own, with its own
   sentence and its own consequence; a batch that lost those would be a worse frame than
   the round trips bought.
"""

from __future__ import annotations

import fcntl
import os
import pty
import shutil
import signal
import struct
import subprocess
import termios
import threading
import time
import unittest
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame, config
from charter.frame import state, tmuxctl

from tests import _tmuxchain, _tmuxreap
from tests._isolation import PersonaIso
# At module scope, and that is load-bearing rather than tidy: `test_frame_launcher`
# captures the developer's real `config.STATE_DIR` at IMPORT time and refuses to run a
# real `cmd_launch` against it. Imported from inside a test method — after `PersonaIso`
# has already repointed the plane — it would capture the ISOLATED directory as "the real
# one" and refuse every launch here for a plane nobody owns.
from tests.test_frame_chat_switch import _FakeServer, _plant
from tests.test_frame_launcher import _FakeTmux, _launch

_HAS_TMUX = shutil.which("tmux") is not None

#: One socket for this module, carrying this process's pid so an interrupted run's server
#: is reaped by the next one rather than collided with (`tests/_tmuxreap.py`).
SOCKET = _tmuxreap.name("one-invocation")

#: The four edges a full frame draws, in the order a launch splits them.
_FOUR = ["right", "top", "bottom", "repos"]


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class WhatARealTmuxDoesWithACommandList(unittest.TestCase):
    """The two properties `tmuxctl.write_all` and `commands_frame._split_all` rest on.

    **A server and no client**, which is what makes this runnable in CI: nothing here
    attaches anything or draws anything, so `TERM=dumb` and no controlling terminal are
    no obstacle. What it needs is a tmux that will start a detached session, which CI has.

    **One server for the whole class, HELD open, and #713 is why that is a construction
    rather than tidiness** (`test_frame_tmux_integration._hold_the_server` measures it).
    `exit-empty` is on by default: the moment a socket's last session goes, the server
    retires — and it does not unlink its socket file, so the next `new-session` is a
    client that finds a socket to connect to rather than a path to build a server at. If
    it lands while the retiring server still holds its listening fd, the connect succeeds,
    the command is never run, and tmux answers **`server exited unexpectedly`**, rc 1.
    A server per test on one socket is three chances at that per run, and this class took
    one on its first CI run. The keeper session is never a target and never dies, so the
    server is born once and no client ever rebuilds it.
    """

    @classmethod
    def setUpClass(cls):
        cls._tmux_cmd("kill-server")
        held = cls._tmux_cmd("new-session", "-d", "-s", "keep", "-x", "80", "-y", "24",
                             "--", "cat")
        if held.returncode != 0:
            # Raised rather than `assert`ed: an `assert` in a fixture is stripped by
            # `python -O`, and this suite has a test that says so. Not a skip either —
            # `_HAS_TMUX` has already established there is a tmux, so a server that will
            # not start is a fact about this machine worth failing loudly over rather
            # than a capability to shrug at.
            raise AssertionError(
                f"tmux would not hold a session open on {SOCKET}: {held.stderr!r}")

    @classmethod
    def tearDownClass(cls):
        cls._tmux_cmd("kill-server")
        # `kill-server` ends the server and leaves its socket FILE — measured, and the
        # reason `tests/_tmuxreap.py` exists at all. Removed here so a run leaks nothing.
        (_tmuxreap.socket_dir() / SOCKET).unlink(missing_ok=True)

    @classmethod
    def _tmux_cmd(cls, *args: str) -> subprocess.CompletedProcess:
        return subprocess.run(["tmux", "-L", SOCKET, *args],
                              capture_output=True, text=True, timeout=15)

    def setUp(self):
        # A session of this test's own on the held server. Not killed afterwards: the
        # server must never become empty (see the class docstring), and `tearDownClass`
        # takes every session down at once.
        name = f"s{len(self.id())}{abs(hash(self.id())) % 100000}"
        started = self._tmux("new-session", "-d", "-s", name, "-x", "80", "-y", "24",
                             "-P", "-F", "#{pane_id}", "--", "cat")
        self.assertEqual(started.returncode, 0, started.stderr)
        self.pane = started.stdout.strip()
        self.assertTrue(self.pane.startswith("%"), started.stdout)

    def _tmux(self, *args: str) -> subprocess.CompletedProcess:
        return self._tmux_cmd(*args)

    def _option(self, name: str) -> str:
        """One window option's value, or ``""`` where this tmux has none set.

        `show-options -v` answers an unset user option with an error at 3.7c and with an
        empty line at the 3.2 floor, so BOTH are read as "not set" — the fact under test
        is that the value is not the one the aborted command carried, and a version's
        choice of how to say "nothing here" must not decide whether this passes.
        """
        got = self._tmux("show-options", "-w", "-t", self.pane, "-v", name)
        return got.stdout.strip() if got.returncode == 0 else ""

    def test_a_chain_answers_for_every_command_in_it(self):
        """What `_split_all` reads: four `split-window -P -F` in ONE invocation print four
        pane ids, one per line, in the order they were given.

        Without this the four splits could not be batched at all — each one's answer is
        the `-t` of the pane options that follow it, and a batch that could only report
        the last would be a frame with three unmarked panels.
        """
        argv = tmuxctl.chain([
            tmuxctl.server_argv(SOCKET, "split-window", "-t", self.pane, "-v", "-l", "1",
                                "-P", "-F", "#{pane_id}", "--", "cat")
            for _ in range(4)])
        out = subprocess.run(argv, capture_output=True, text=True, timeout=15)
        self.assertEqual(out.returncode, 0, out.stderr)
        ids = out.stdout.split()
        self.assertEqual(len(ids), 4, out.stdout)
        self.assertEqual(len(set(ids)), 4, "four splits reported one pane four times")
        for pane in ids:
            self.assertRegex(pane, r"^%\d+$")
        listed = self._tmux("list-panes", "-t", self.pane, "-F", "#{pane_id}")
        self.assertEqual(sorted(listed.stdout.split()), sorted([self.pane, *ids]),
                         "the window does not hold the panes the chain reported")

    def test_a_refused_command_abandons_the_rest_of_the_list(self):
        """The measurement `tmuxctl.write_all`'s whole fallback exists for.

        Three writes, the middle one naming an option no tmux has: the first takes, the
        third **does not run at all**, and the invocation returns non-zero. So a chain is
        one command as far as failure is concerned, while charter's callers are written
        around each write failing on its own — which is why a non-zero chain is re-issued
        one write at a time rather than merely reported.
        """
        good, bad = "@charter_batch_probe_a", "@charter_batch_probe_b"
        argv = tmuxctl.chain([
            tmuxctl.server_argv(SOCKET, "set-option", "-w", "-t", self.pane, good, "1"),
            tmuxctl.server_argv(SOCKET, "set-option", "-w", "-t", self.pane,
                                "charter-is-not-a-tmux-option", "1"),
            tmuxctl.server_argv(SOCKET, "set-option", "-w", "-t", self.pane, bad, "1")])
        out = subprocess.run(argv, capture_output=True, text=True, timeout=15)
        self.assertNotEqual(out.returncode, 0, out.stdout)
        self.assertEqual(self._option(good), "1",
                         "the command BEFORE the refusal did not take either — this "
                         "measurement is about where a list stops, and it has to have "
                         "started")
        self.assertNotEqual(self._option(bad), "1",
                            "the command after the refusal ran, so a chain no longer "
                            "abandons its tail — `tmuxctl.write_all`'s fallback is "
                            "written for a tmux that does")

    def test_unsetting_a_hook_nothing_set_is_not_a_failure(self):
        """What lets `_reconcile_panels` batch each disarm beside its own `kill-pane`.

        If `set-hook -p -u pane-died` failed on a pane with no such hook — a panel charter
        declined to arm, or one from a charter that predates #382 — the chain would abort
        there and the `kill-pane` after it would never run, so a density change would
        leave the panel the operator just dropped on screen.
        """
        out = self._tmux("set-hook", "-p", "-u", "-t", self.pane, "pane-died")
        self.assertEqual(out.returncode, 0, out.stderr)


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class WhatACommandListCostsTheOperatorsScreen(unittest.TestCase):
    """**#844's premise, asked of a real tmux with a real attached client rather than
    believed** — and it is the reason a list is the unit worth counting at all.

    #780 established that tmux redraws once per command LIST. #844 asked the next
    question, because the answer decides whether merging two lists is worth anything: does
    a list whose writes change NOTHING still redraw? The issue behind this assumed the
    opposite — that a `resize-pane` to a size a pane already has is free because it
    delivers no SIGWINCH to the pane. It delivers none; measured here, it costs the client
    a full repaint anyway.

    Measured by hand on tmux 3.7c and at the 3.2 floor, 200x50, three panes, before this
    class existed, and identically on both versions:

    ==========================================  =============  =============
    one invocation carrying                     3.7c           3.2
    ==========================================  =============  =============
    `resize-pane -x 22` on a 22-wide pane       1672 bytes     1811 bytes
    a real `resize-pane -x 30`                  1648 bytes     1787 bytes
    three no-op `resize-pane`s in ONE list      1672 bytes     1811 bytes
    `display-message -p` (a pure read)          0              0
    ==========================================  =============  =============

    So the count that decides how much of the operator's window appears to re-render is
    the number of write-carrying LISTS — not the number of commands in them, not whether
    any of them moved a boundary. That is what makes `_reassert_sizes`' harness row worth
    merging into the row pass, and it is what a future tmux would have to change for that
    merge to stop being worth anything.

    **Needs a client, which is what separates it from
    :class:`WhatARealTmuxDoesWithACommandList`** — there is no screen to repaint without
    one. It skips rather than fails where tmux will not attach one, naming what it could
    not get: a machine that cannot fork a pty client has measured nothing here, and a
    trial that measured nothing is not a result to assert on.
    """

    SOCKET = _tmuxreap.name("redraw-cost")

    def setUp(self):
        self._tmux("kill-server")
        made = self._tmux("new-session", "-d", "-s", "s", "-x", "200", "-y", "50",
                          "-P", "-F", "#{pane_id}", "--", "cat")
        self.assertEqual(made.returncode, 0, made.stderr)
        self.harness = made.stdout.strip()
        split = self._tmux("split-window", "-t", self.harness, "-h", "-l", "22",
                           "-P", "-F", "#{pane_id}", "--", "cat")
        self.assertEqual(split.returncode, 0, split.stderr)
        self.side = split.stdout.strip()
        self.addCleanup(self._reap)
        self._attach()

    def _reap(self):
        self._tmux("kill-server")
        (_tmuxreap.socket_dir() / self.SOCKET).unlink(missing_ok=True)

    def _tmux(self, *args: str) -> subprocess.CompletedProcess:
        return subprocess.run(["tmux", "-L", self.SOCKET, *args],
                              capture_output=True, text=True, timeout=20)

    def _attach(self) -> None:
        """A real pty client at 200x50, and a thread recording every byte tmux draws on
        it. Skips where no terminal type will attach — see the class docstring."""
        for term in ("xterm-256color", "screen", "vt100"):
            pid, fd = pty.fork()
            if pid == 0:                                      # pragma: no cover - child
                try:
                    os.environ["TERM"] = term
                    os.execvp("tmux", ["tmux", "-L", self.SOCKET, "attach", "-t", "s"])
                finally:
                    os._exit(127)
            fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 50, 200, 0, 0))
            self.addCleanup(self._close, pid, fd)
            for _ in range(200):
                seen = self._tmux("list-clients", "-t", "s", "-F", "#{client_name}")
                if seen.returncode == 0 and seen.stdout.strip():
                    self.drawn: list[int] = []
                    self._reading = True
                    threading.Thread(target=self._read, args=(fd,),
                                     daemon=True).start()
                    time.sleep(1.0)
                    return
                time.sleep(0.02)
        self.skipTest("no tmux client would attach on this machine, and there is no "
                      "screen to measure a repaint on without one")

    def _close(self, pid: int, fd: int) -> None:
        self._reading = False
        try:
            os.kill(pid, signal.SIGKILL)
            os.waitpid(pid, 0)
        except OSError:
            pass
        try:
            os.close(fd)
        except OSError:
            pass

    def _read(self, fd: int) -> None:                     # pragma: no cover - thread
        while self._reading:
            try:
                data = os.read(fd, 65536)
            except OSError:
                return
            if not data:
                return
            self.drawn.append(len(data))

    def _bytes_drawn(self, *invocations: list[str]) -> int:
        """How many bytes tmux draws on the client for *invocations*, run in order.

        Settled before and after: the panes run `cat` and write nothing of their own, so
        everything counted here is tmux redrawing, and the sleeps are what keep one
        probe's tail out of the next one's total.
        """
        time.sleep(0.5)
        self.drawn.clear()
        for argv in invocations:
            out = self._tmux(*argv)
            self.assertEqual(out.returncode, 0, out.stderr)
        time.sleep(0.5)
        return sum(self.drawn)

    def test_a_write_that_changes_nothing_still_repaints_the_whole_client(self):
        """The measurement #844 turns on. `resize-pane -x 22` on a pane that is already 22
        columns wide moves no boundary and delivers no SIGWINCH — and costs the client the
        same repaint a real resize does."""
        real = self._bytes_drawn(["resize-pane", "-t", self.side, "-x", "30"])
        if not real:
            self.skipTest("this client redrew nothing for a REAL resize, so it has "
                          "measured nothing about a no-op one")
        back = self._bytes_drawn(["resize-pane", "-t", self.side, "-x", "22"])
        noop = self._bytes_drawn(["resize-pane", "-t", self.side, "-x", "22"])
        self.assertGreater(noop, 0,
                           "a no-op `resize-pane` drew nothing on the client — the merge "
                           "in `_reassert_sizes` was made because it draws a full "
                           "repaint, and this tmux no longer agrees")
        self.assertGreater(noop, back // 2,
                           f"a no-op resize ({noop} B) cost far less than a real one "
                           f"({back} B), so a list of them is not the repaint #844 "
                           "merged two lists to avoid")

    def test_one_list_of_no_op_resizes_costs_one_repaint_where_three_cost_three(self):
        """The half that makes the merge worth making: the repaint is per LIST.

        Three `resize-pane`s that change nothing, sent one at a time, cost three repaints;
        the same three in one invocation cost one. `_reassert_sizes` sends a frame's rows
        and then the harness's own row, and since #844 that is one list rather than two.
        """
        one = ["resize-pane", "-t", self.side, "-x", "22"]
        two = ["resize-pane", "-t", self.harness, "-y", "50"]
        separately = self._bytes_drawn(one, two, one)
        together = self._bytes_drawn(one + [";"] + two + [";"] + one)
        self.assertGreater(separately, 0, "nothing was drawn at all — nothing measured")
        self.assertLess(together, separately / 2,
                        f"three writes as one list drew {together} B where three "
                        f"invocations drew {separately} B — tmux no longer redraws once "
                        "per command list, which is what every batch here is built on")

    def test_a_pure_read_costs_the_client_nothing(self):
        """The other side of the same rule, and the reason #510's `display-message`
        between the two size passes is not what `_reassert_sizes` had to collapse: a list
        that only READS draws nothing at all, so leaving it where the measurement needs it
        costs the operator no repaint."""
        drew = self._bytes_drawn(["display-message", "-p", "-t", self.side,
                                  "#{pane_width}"])
        self.assertEqual(drew, 0,
                         "a pure `display-message -p` repainted the client, so a read is "
                         "no longer free and #510's read between the size passes is now "
                         "costing what a write costs")


class TheFakesSplitExactlyWhatCharterJoined(unittest.TestCase):
    """`tests/_tmuxchain.commands` is the inverse of `tmuxctl.chain`, and this is what
    says so — because every other test in the suite now believes it.

    A fake that split too eagerly would report commands charter never sent; one that split
    too little would answer a batch of eight with the last one's arguments and let seven
    assertions quietly stop meaning anything. The case that decides it is an argument
    CONTAINING a semicolon — `tmuxctl.SEPARATOR`'s own measurement is that tmux reads only
    a standalone `;` as one, and charter really sends both shapes: the exit-status hook's
    action is a shell line with a `;` in the middle of it, and the value
    `frame/overlay.close_argvs` writes back into `@charter_hatch` is a tmux command line
    with a spaced ` ; ` in it. Either would be torn in half by a character-wise split.
    """

    def test_a_chain_splits_back_into_the_commands_it_was_built_from(self):
        hook = commands_frame._pane_died_write_hook_argv(socket="s", harness_pane="%1")
        self.assertTrue(any(";" in part for part in hook),
                        "the exit-status hook's action no longer carries a `;` inside "
                        "one argument, so this test is no longer about the case it names")
        spaced = tmuxctl.server_argv("s", "set-option", "-w", "-t", "%1",
                                     "@charter_hatch",
                                     "select-pane -t %1 ; kill-pane -t %2")
        argvs = [tmuxctl.server_argv("s", "set-option", "-w", "-t", "%1",
                                     "remain-on-exit", "on"),
                 hook, spaced,
                 tmuxctl.server_argv("s", "kill-pane", "-t", "%2")]
        self.assertEqual(_tmuxchain.commands(tmuxctl.chain(argvs)), argvs)

    def test_one_command_is_handed_back_whole(self):
        argv = tmuxctl.server_argv("s", "kill-pane", "-t", "%2")
        self.assertEqual(_tmuxchain.commands(argv), [argv])


class ALaunchAndASwitchSpendWhatTheyMeasured(PersonaIso, unittest.TestCase):
    """The counts, on a machine with no tmux — the assertion that catches a batch quietly
    coming apart again.

    **A number rather than a bound**, and deliberately: "no more than N" would go on
    passing through every regression that added a round trip back one at a time, which is
    exactly how 42 became 43 and 41 became 58 between #780 being filed and being fixed.
    A count that has to be edited is a count somebody has to look at.
    """

    def test_a_four_panel_launch_sends_fifteen_invocations_up_to_the_attach(self):
        """The whole private-server launch, up to and including `attach`. It was 44.

        Against THIS MODULE'S fake, which is what makes the number assertable at all; a
        real launch on a real server makes two more reads than the fake does (46 -> 17
        measured), and the shape of the saving is the same one.

        Five batches carry 33 of the 47 commands: the ten writes that tell the window and
        session what they are, the window's own dressing, the four splits, every pane's
        own options, and the four respawn hooks.

        Unbatched, and named here so a reader can see what is left rather than guess: two
        reaping reads, `new-session`, the eager death check, `list-panes`, the resize
        hook, the window-seat read, `select-window`, `select-pane`, `attach`.
        """
        fake = _FakeTmux(panel_pane_ids={"top": "%11", "bottom": "%12",
                                         "right": "%13", "repos": "%14"},
                         still_live=True)
        with mock.patch.dict(config.FRAME, {"slots": list(_FOUR)}):
            self.assertEqual(_launch(fake, cols=200, rows=50), 0)
        attach = next(i for i, c in enumerate(fake.invocations) if "attach" in c)
        self.assertEqual(attach + 1, 15,
                         [" ".join(c[3:])[:70] for c in fake.invocations[:attach + 1]])
        # And every command is still issued — batching moved them, it did not drop them.
        self.assertEqual(len([c for c in fake.calls if "split-window" in c]), 4)
        self.assertEqual(sorted(state.panes(_the_chat(fake))),
                         ["bottom", "repos", "right", "top"])

    def test_a_four_panel_switch_sends_twenty_two_invocations(self):
        """The `charter frame-chat` path: select the window, split four fresh panels in,
        then tear the four the operator left down. It was 58, then 23, and is 22.

        Each end's window dressing is one invocation, the four splits are one, every new
        pane's options are one, the four respawn hooks are one, the teardown's eight
        commands (a disarm and a kill per panel) are one, and — since #844 — a frame's
        rows and the harness's own row are one rather than two.
        """
        fake = self._switch()
        self.assertEqual(len(fake.invocations), 22,
                         [" ".join(c[3:])[:70] for c in fake.invocations])
        self.assertEqual(len([c for c in fake.calls if "kill-pane" in c]), 4)
        self.assertEqual(len([c for c in fake.calls if "split-window" in c]), 4)

    def test_a_frames_rows_and_its_harnesss_row_are_one_command_list(self):
        """#844's collapse, and the one thing a count alone would not say: which list.

        `_reassert_sizes` sends the columns, reads the variable pane's width (#510's
        measurement, which must stay a read between the passes), and then sends the rows.
        The harness's own height is the last write of that second pass and used to be an
        invocation of its own — so a frame's rows landed as two command lists and cost the
        operator two whole-client repaints. Measured on tmux 3.7c and at the 3.2 floor
        with a real attached client at 200x50: **every list carrying a write redraws the
        client**, and a `resize-pane` to a size the pane already has is no exception (1672
        bytes on 3.7c, 1811 at the floor — what a real resize costs), while a list of
        three no-op resizes costs exactly one of those.

        The ORDER inside the list is asserted too, and it is the half that had to survive
        the merge: `resize-pane -y` moves one boundary (#515), so the harness is asserted
        after the strips. A chain runs in the order it is given, so it still is.
        """
        fake = self._switch()
        rows = [_tmuxchain.commands(inv) for inv in fake.invocations]
        rows = [cmds for cmds in rows
                if len(cmds) > 1 and all("resize-pane" in c for c in cmds)]
        self.assertEqual(len(rows), 1,
                         "the arriving frame's rows are not one command list")
        targets = [c[c.index("-t") + 1] for c in rows[0]]
        self.assertEqual(targets[-1], "%2",
                         "the harness pane is not the last resize in the list — "
                         "`resize-pane -y` moves one boundary, so it has to be")
        self.assertTrue(all(c[-2] == "-y" for c in rows[0]), rows[0])
        # And the read #510 put between the two passes is still a read of its own, still
        # after the columns and still before these rows. Collapsing THAT away is what this
        # merge deliberately did not do.
        verbs = [_tmuxchain.commands(inv) for inv in fake.invocations]
        flat = [inv for inv in verbs]
        widths = [i for i, cmds in enumerate(flat)
                  if any(commands_frame._PANE_WIDTH_FORMAT in c for c in cmds)]
        cols = [i for i, cmds in enumerate(flat)
                if any("resize-pane" in c and "-x" in c for c in cmds)]
        self.assertTrue(cols and widths and cols[0] < widths[0]
                        < flat.index(rows[0]), (cols, widths))

    def test_the_arriving_chat_is_dressed_before_the_one_being_left_is_tidied(self):
        """#844's ordering, pinned where the invocation counts are: **every** split of the
        window the operator has just arrived at comes before **every** kill in the window
        they left.

        It ran the other way round, and `cmd_chat`'s own docstring has always said the two
        re-layouts are independent — so nothing had ever pinned the order and nothing
        needed to change for this but the two blocks swapping places. What it cost was
        measured end to end on tmux 3.7c with a real attached client and four panels at
        200x50: the client moves at invocation 3, and the arriving frame's panels used to
        appear at invocation 15 — **66 ms** during which the operator is looking at a bare
        full-screen harness pane while charter tidies a window nobody can see. They appear
        at invocation 8 now, 38 ms after the move.
        """
        fake = self._switch()
        splits = [i for i, c in enumerate(fake.calls) if "split-window" in c]
        kills = [i for i, c in enumerate(fake.calls) if "kill-pane" in c]
        self.assertEqual(len(splits), 4)
        self.assertEqual(len(kills), 4)
        self.assertLess(max(splits), min(kills),
                        [" ".join(c[3:])[:60] for c in fake.calls])

    def _switch(self) -> _FakeServer:
        """One four-panel `cmd_chat` from `api.1` to `api.2`, and the fake it spent."""
        _plant("api.1", workspace="api", pane="%1")
        _plant("api.2", workspace="api", pane="%2")
        state.record_server("api.1", commands_frame.SOCKET)
        state.record_server("api.2", commands_frame.SOCKET)
        state.record_panes("api.1", panels={"top": "%3", "bottom": "%4",
                                            "right": "%5", "repos": "%6"})
        fake = _FakeServer(size="200:50")
        with mock.patch.object(tmuxctl, "run", fake), \
             mock.patch.object(tmuxctl, "version", lambda: (3, 7)), \
             mock.patch.dict(config.FRAME, {"slots": list(_FOUR)}), \
             mock.patch.dict("os.environ", {"CHARTER_SESSION_ID": "api.1",
                                            "CHARTER_WORKSPACE": "api"}, clear=False):
            self.assertEqual(
                commands_frame.cmd_chat(SimpleNamespace(chat_id="api.2")), 0)
        return fake


def _the_chat(fake) -> str:
    """The chat id the launch under test gave its window — read off the fake rather than
    spelled, because the ordinal depends on what the plane already held."""
    assert fake.fid, "the launch never named its chat"
    return fake.fid


class AFailedBatchIsEveryCallItUsedToBe(unittest.TestCase):
    """`tmuxctl.write_all`'s fallback, which is what makes batching safe to do at all.

    Every caller batched by #780 branches on each write's own return code and says its own
    sentence about what an operator loses. Those sentences are the reason the frame is
    debuggable, and a batch that collapsed them into one phrase would trade a real thing
    for round trips. So a chain that comes back non-zero is thrown away and every write is
    re-issued on its own — which is only sound because every write in a batch means the
    same thing twice.
    """

    def _writes(self):
        return [tmuxctl.Write("arming a", tmuxctl.server_argv("s", "set-option", "@a", "1")),
                tmuxctl.Write("arming b", tmuxctl.server_argv("s", "set-option", "@b", "2")),
                tmuxctl.Write("arming c", tmuxctl.server_argv("s", "set-option", "@c", "3"),
                              report=False)]

    def _spy(self, answer):
        seen = []

        def run(action, argv, **kw):
            seen.append((action, list(argv), kw.get("report", True)))
            return answer(argv)

        return seen, run

    def test_a_batch_that_takes_is_one_invocation(self):
        seen, run = self._spy(lambda argv: subprocess.CompletedProcess(argv, 0, "", ""))
        with mock.patch.object(tmuxctl, "run", run):
            out = tmuxctl.write_all("arming the lot", self._writes())
        self.assertEqual(len(seen), 1, seen)
        self.assertEqual(seen[0][0], "arming the lot")
        self.assertEqual(seen[0][1].count(tmuxctl.SEPARATOR), 2, seen[0][1])
        self.assertEqual([p.returncode for p in out], [0, 0, 0])

    def test_a_batch_that_is_refused_is_re_issued_write_by_write(self):
        """Each write's OWN action phrase and its own *report* — the two things
        `tmuxctl.Write` carries, and the two a caller reads a failure through."""
        seen, run = self._spy(
            lambda argv: subprocess.CompletedProcess(
                argv, 0 if tmuxctl.SEPARATOR not in argv else 1, "", "nope"))
        with mock.patch.object(tmuxctl, "run", run):
            out = tmuxctl.write_all("arming the lot", self._writes())
        self.assertEqual([a for a, _, _ in seen],
                         ["arming the lot", "arming a", "arming b", "arming c"])
        self.assertEqual([r for _, _, r in seen], [False, True, True, False])
        self.assertEqual([p.returncode for p in out], [0, 0, 0])

    def test_a_wedged_server_is_reported_once_and_never_re_issued(self):
        """A timeout does not say the writes did not happen, and re-issuing on that is
        the one thing idempotence cannot cover. Reported, and handed back to every write
        so the caller degrades exactly as it does for any other non-zero return."""
        seen, run = self._spy(
            lambda argv: subprocess.CompletedProcess(argv, tmuxctl.TIMED_OUT, "", "slow"))
        said = []
        with mock.patch.object(tmuxctl, "run", run), \
             mock.patch.object(tmuxctl, "report_failure",
                               lambda *a: said.append(a[0])):
            out = tmuxctl.write_all("arming the lot", self._writes())
        self.assertEqual(len(seen), 1, seen)
        self.assertEqual(said, ["arming the lot"])
        self.assertEqual([p.returncode for p in out], [tmuxctl.TIMED_OUT] * 3)

    def test_a_batch_nothing_would_have_reported_stays_quiet_on_a_wedged_server(self):
        """`_apply_sizes` opts every `resize-pane` out of reporting, because a pane that
        died between the map being read and this running is not worth printing over the
        agent's own screen. Becoming a batch must not start printing it."""
        quiet = [tmuxctl.Write("sizing a", tmuxctl.server_argv("s", "resize-pane", "-y",
                                                               "1"), report=False),
                 tmuxctl.Write("sizing b", tmuxctl.server_argv("s", "resize-pane", "-y",
                                                               "2"), report=False)]
        _, run = self._spy(
            lambda argv: subprocess.CompletedProcess(argv, tmuxctl.TIMED_OUT, "", "slow"))
        said = []
        with mock.patch.object(tmuxctl, "run", run), \
             mock.patch.object(tmuxctl, "report_failure",
                               lambda *a: said.append(a[0])):
            tmuxctl.write_all("sizing the panels", quiet)
        self.assertEqual(said, [])

    def test_one_write_is_sent_as_itself_and_never_as_a_list_of_one(self):
        """A group of one is the ordinary case on the switch's entering end — one panel
        missing — and a `tmux … ;`-less invocation is what a reader of the failure sees."""
        seen, run = self._spy(lambda argv: subprocess.CompletedProcess(argv, 0, "", ""))
        with mock.patch.object(tmuxctl, "run", run):
            tmuxctl.write_all("arming the lot", self._writes()[:1])
        self.assertEqual([a for a, _, _ in seen], ["arming a"])
        self.assertNotIn(tmuxctl.SEPARATOR, seen[0][1])

    def test_nothing_to_write_is_no_invocation_at_all(self):
        seen, run = self._spy(lambda argv: subprocess.CompletedProcess(argv, 0, "", ""))
        with mock.patch.object(tmuxctl, "run", run):
            self.assertEqual(tmuxctl.write_all("arming the lot", []), [])
        self.assertEqual(seen, [])


class ARefusedSplitStillCostsOnlyItsOwnPanel(PersonaIso, unittest.TestCase):
    """`commands_frame._split_all` — the one batch that reads a value back.

    A `split-window` is not idempotent, so this cannot use `tmuxctl.write_all`'s replay:
    re-issuing one that already took is a second pane for one component, which is the
    #714 shape. What it does instead is count the ids tmux answered with — the command at
    `len(ids)` is the one that was refused, and nothing after it ran — and re-issue from
    there one at a time.
    """

    def _split(self, answer):
        seen = []

        def run(action, argv, **kw):
            seen.append((action, list(argv)))
            return answer(argv)

        cmds = [tmuxctl.server_argv("s", "split-window", "-t", "%0", "-P", "-F",
                                    "#{pane_id}", "--", "charter", "panel", slot)
                for slot in _FOUR]
        with mock.patch.object(tmuxctl, "run", run):
            made = commands_frame._split_all("s", slots=list(_FOUR), cmds=cmds, env=None)
        return seen, made

    def test_four_splits_that_take_are_one_invocation_and_four_panes(self):
        ids = iter(["%10", "%11", "%12", "%13"])
        seen, made = self._split(
            lambda argv: subprocess.CompletedProcess(
                argv, 0, "".join(f"{next(ids)}\n" for _ in range(4)), ""))
        self.assertEqual(len(seen), 1, seen)
        self.assertEqual(made, [("right", "%10"), ("top", "%11"),
                                ("bottom", "%12"), ("repos", "%13")])

    def test_a_refusal_half_way_re_issues_only_what_never_ran(self):
        """The second split is refused, so tmux ran two commands and printed one id. The
        remaining two slots are then sent on their own — the failing one included, which
        is what makes it reportable under "drawing a panel" rather than under the batch's
        joint phrase."""
        answers = iter([
            # The chain: `right` took and printed its id, `top` was refused, and the two
            # after it never ran at all.
            subprocess.CompletedProcess([], 1, "%10\n", "no space for a new pane"),
            # `top` again, on its own, and refused again — this is the call that gets to
            # say "drawing a panel" rather than the batch's joint phrase.
            subprocess.CompletedProcess([], 1, "", "no space for a new pane"),
            subprocess.CompletedProcess([], 0, "%12\n", ""),
            subprocess.CompletedProcess([], 0, "%13\n", ""),
        ])
        seen, made = self._split(lambda argv: next(answers))
        self.assertEqual([a for a, _ in seen],
                         ["drawing this frame's panels", "drawing a panel",
                          "drawing a panel", "drawing a panel"])
        self.assertEqual(made, [("right", "%10"), ("bottom", "%12"), ("repos", "%13")],
                         "every slot after the refused one must still be attempted, and "
                         "the refused one must not be counted as a pane")
        self.assertNotIn(tmuxctl.SEPARATOR, seen[1][1], "the re-issue is one command")

    def test_a_wedged_tmux_splits_nothing_twice(self):
        """A timeout does not say the split did not happen. Re-issuing it is how a frame
        ends up with two panes for one component and a record naming only the second."""
        said = []
        with mock.patch.object(tmuxctl, "report_failure",
                               lambda *a: said.append(a[0])):
            seen, made = self._split(
                lambda argv: subprocess.CompletedProcess(argv, tmuxctl.TIMED_OUT, "", ""))
        self.assertEqual(len(seen), 1, seen)
        self.assertEqual(made, [])
        self.assertEqual(said, ["drawing this frame's panels"])
