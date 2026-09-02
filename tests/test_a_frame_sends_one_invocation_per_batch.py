"""#780: what charter says to tmux is unchanged; how many times it says it is not.

The operator's report was *"very slow rendering on switching — it seems re-rendering all
and I see jumping texts ~0.2 sec, then it's ready"*, and both halves of it are one defect:
charter issued every tmux command on its own, and a round trip is ~5 ms on the machine
this was written on and ~13.4 ms on the one that filed #728. Measured with a real client at
200x50 and four panels, before this change and after it:

===================  ==================  ====================  ====================
path and tmux        invocations         wall clock            terminal repaints
===================  ==================  ====================  ====================
switch, 3.7c         58 -> 26            329 ms -> 162 ms      45 -> 17
switch, 3.2          50 -> 24            243 ms -> 127 ms      41 -> 15
launch, 3.7c         46 -> 20            245 ms -> 114 ms      (no client yet)
launch, 3.2          42 -> 19            184 ms -> 83 ms       (no client yet)
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

import shutil
import subprocess
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
    """

    def setUp(self):
        self.addCleanup(lambda: subprocess.run(
            ["tmux", "-L", SOCKET, "kill-server"], capture_output=True))
        started = self._tmux("new-session", "-d", "-s", "s", "-x", "80", "-y", "24",
                             "-P", "-F", "#{pane_id}", "cat")
        self.assertEqual(started.returncode, 0, started.stderr)
        self.pane = started.stdout.strip()
        self.assertTrue(self.pane.startswith("%"), started.stdout)

    def _tmux(self, *args: str) -> subprocess.CompletedProcess:
        return subprocess.run(["tmux", "-L", SOCKET, *args],
                              capture_output=True, text=True, timeout=15)

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


class ALaunchAndASwitchSpendWhatTheyMeasured(PersonaIso, unittest.TestCase):
    """The counts, on a machine with no tmux — the assertion that catches a batch quietly
    coming apart again.

    **A number rather than a bound**, and deliberately: "no more than N" would go on
    passing through every regression that added a round trip back one at a time, which is
    exactly how 42 became 43 and 41 became 58 between #780 being filed and being fixed.
    A count that has to be edited is a count somebody has to look at.
    """

    def test_a_four_panel_launch_sends_eighteen_invocations_up_to_the_attach(self):
        """The whole private-server launch, up to and including `attach`. It was 44.

        Batched: the ten writes that tell the window and session what they are (one), the
        window's own dressing (one), the four splits (one), each pane's two options (one
        PER PANE — a pane's `%N` is not known until its own split has answered), and the
        four respawn hooks (one).

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
        self.assertEqual(attach + 1, 18,
                         [" ".join(c[3:])[:70] for c in fake.invocations[:attach + 1]])
        # And every command is still issued — batching moved them, it did not drop them.
        self.assertEqual(len([c for c in fake.calls if "split-window" in c]), 4)
        self.assertEqual(sorted(state.panes(_the_chat(fake))),
                         ["bottom", "repos", "right", "top"])

    def test_a_four_panel_switch_sends_twenty_six_invocations(self):
        """The `charter frame-chat` path: tear four panels down, select the window, split
        four fresh ones in.

        The teardown's eight commands (a disarm and a kill per panel) are one invocation,
        each end's window dressing is one, the four splits are one, and the four respawn
        hooks are one.
        """
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
        self.assertEqual(len(fake.invocations), 26,
                         [" ".join(c[3:])[:70] for c in fake.invocations])
        self.assertEqual(len([c for c in fake.calls if "kill-pane" in c]), 4)
        self.assertEqual(len([c for c in fake.calls if "split-window" in c]), 4)


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
