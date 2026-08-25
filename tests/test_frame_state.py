"""The frame's own state: who it is, when it last changed, how it ended.

Per frame rather than global, because two frames may run at once (one session each, named
by workspace and pid) and a shared version file would make each frame's panels redraw for
the other's activity.
"""

from __future__ import annotations

import os
import subprocess
import sys
import unittest
from unittest import mock

from tests._isolation import PersonaIso
from charter.frame import state


def _a_dead_pid() -> int:
    """A real pid that has exited and been reaped. Preferred over a large made-up
    number, which is a guess about the machine rather than a fact about it — and since
    #383 the number at the end of a frame id is asked about rather than ignored, a made-up
    one could name somebody else's live process and quietly change what a test proves."""
    p = subprocess.Popen([sys.executable, "-c", "pass"])
    p.wait()
    return p.pid


class FrameId(unittest.TestCase):
    def test_the_id_carries_the_workspace_and_the_pid(self):
        fid = state.frame_id("harness-wrapper", 4242)
        self.assertIn("harness-wrapper", fid)
        self.assertIn("4242", fid)

    def test_a_hostile_workspace_name_cannot_escape_the_state_directory(self):
        """The id becomes a directory name. `contain.py` exists because a name read out
        of a file used to be joined onto a path with nothing in between."""
        fid = state.frame_id("../../etc", 1)
        self.assertNotIn("/", fid)
        self.assertNotIn("..", fid)


class Version(PersonaIso, unittest.TestCase):
    def test_a_fresh_frame_has_a_version(self):
        self.assertTrue(state.version("f-1"))

    def test_bumping_changes_it(self):
        before = state.version("f-1")
        state.bump("f-1")
        self.assertNotEqual(before, state.version("f-1"))

    def test_reading_an_unknown_frames_version_creates_nothing_on_disk(self):
        """A probe reads; it does not act (charter/news.py). `version()` on a frame that
        was never bumped must not create the directory it is only trying to look at —
        that is the mistake correction 1 exists to rule out."""
        self.assertEqual(state.version("never-bumped"), "0")
        self.assertFalse(state.frame_dir("never-bumped").exists())

    def test_a_non_utf8_version_file_degrades_to_the_sentinel_rather_than_raising(self):
        """Fix round 2, item 1: `read_text()` on bytes that are not valid UTF-8 raises
        `UnicodeDecodeError` — a `ValueError` subclass, never caught by an `except
        OSError` alone. `panel._tick` reads this function directly with nothing of its
        own guarding the call, so an uncaught decode error here used to reach a real
        panel's run loop and kill the pane — exactly the failure this module's own
        docstring already promised could not happen ("nothing here raises... a missing
        frame answers with the sentinel"). A corrupt file is treated the same as a
        missing one: the sentinel, not an exception."""
        d = state.frame_dir("f-1", create=True)
        (d / "version").write_bytes(b"\xff\xfe not valid utf-8 \x80\x81")
        self.assertEqual(state.version("f-1"), "0")

    def test_reap_then_version_does_not_resurrect_the_directory(self):
        """If `version()` ever called `bump()` on a miss, this would fight `reap()`
        forever: reap deletes, the next poll recreates, reap deletes again.

        The pid in the name is load-bearing since #383 — `reap` refuses to remove a
        directory whose launcher is still running, so the frame this test needs reaped
        has to be named after a pid that is genuinely over. `gone-1` used to read as a
        throwaway label; pid 1 is `launchd`/`init`, and reap would now (rightly) keep it.
        """
        gone = f"gone-{_a_dead_pid()}"
        state.bump(gone)
        state.reap(set(), server="charter")
        self.assertEqual(state.version(gone), "0")
        self.assertFalse(state.frame_dir(gone).exists())


class FrameDirContainment(PersonaIso, unittest.TestCase):
    def test_a_hostile_fid_cannot_resolve_outside_the_frame_root(self):
        """`fid` is not always minted by this module — a later caller reads it out of
        $CHARTER_SESSION_ID, so a traversal, an absolute path, or an embedded separator
        must be refused rather than silently rewritten into a safe-looking name."""
        root = state._root()
        for hostile in ("../../etc/passwd", "/etc/passwd", "a/b", "..", "."):
            with self.subTest(hostile=hostile):
                self.assertIsNone(state.frame_dir(hostile))
                self.assertIsNone(state.frame_dir(hostile, create=True))

    def test_no_directory_is_created_for_a_hostile_fid(self):
        state.frame_dir("../../escaped", create=True)
        self.assertFalse(state._root().exists())

    def test_bump_on_a_hostile_fid_does_not_raise(self):
        """bump() runs from charter's hooks, where an exception costs a session its
        turn — a malformed $CHARTER_SESSION_ID must be a no-op, not a crash."""
        state.bump("../../escaped")  # must not raise
        self.assertFalse(state._root().exists())


class OverlongFid(PersonaIso, unittest.TestCase):
    """`contain.child` bounds shape, not length — a 5000-character fid passes it and
    then hits `mkdir`'s own ENAMETOOLONG, which is reachable from a real
    `$CHARTER_SESSION_ID` (`session.py`'s id-safety regex strips characters, never
    bounds length). `bump`/`record_exit` run from hooks, where that has to degrade to a
    no-op rather than propagate."""

    def test_bump_on_an_overlong_fid_does_not_raise_or_create(self):
        fid = "x" * 5000
        state.bump(fid)  # must not raise
        self.assertFalse(state._root().exists())
        self.assertEqual(state.version(fid), "0")

    def test_record_exit_on_an_overlong_fid_does_not_raise_or_create(self):
        fid = "x" * 5000
        state.record_exit(fid, 7)  # must not raise
        self.assertFalse(state._root().exists())
        self.assertIsNone(state.exit_code(fid))


class WriteFailureIsNotFatal(PersonaIso, unittest.TestCase):
    """The over-long-fid case above fails at `mkdir`, before any write is attempted.
    This covers the other half: the directory exists, but the write into it fails
    anyway (a filesystem that fills up between `mkdir` and `os.replace`, say) — still a
    hook's-eye no-op, not a raise."""

    def test_bump_survives_a_failing_replace(self):
        with mock.patch("charter.frame.state.os.replace", side_effect=OSError("disk full")):
            state.bump("f-1")  # must not raise
        self.assertEqual(state.version("f-1"), "0")

    def test_record_exit_survives_a_failing_replace(self):
        with mock.patch("charter.frame.state.os.replace", side_effect=OSError("disk full")):
            state.record_exit("f-1", 9)  # must not raise
        self.assertIsNone(state.exit_code("f-1"))

    def test_a_failed_bump_leaves_the_previous_version_intact(self):
        """This is the property the tmp-file + os.replace shape in `bump()` exists for,
        and Finding 1 made it load-bearing: a failed write no longer raises, so the only
        thing standing between it and a reader seeing a corrupted value is that
        os.replace never touches the target file unless it fully succeeds. A bump that
        fails must leave the version a reader already saw exactly as it was — not "0",
        not empty, not some partial write of the new value."""
        state.bump("f-1")
        before = state.version("f-1")
        with mock.patch("charter.frame.state.os.replace", side_effect=OSError("disk full")):
            state.bump("f-1")  # the write fails silently (Finding 1)
        self.assertEqual(state.version("f-1"), before)


class ExitCode(PersonaIso, unittest.TestCase):
    def test_an_unfinished_frame_has_no_exit_code(self):
        self.assertIsNone(state.exit_code("f-1"))

    def test_the_recorded_code_comes_back(self):
        state.record_exit("f-1", 42)
        self.assertEqual(state.exit_code("f-1"), 42)


class ClearExit(PersonaIso, unittest.TestCase):
    """A new frame adopting a recycled pid inherits the directory of the frame that had
    that pid before it (#383 keeps a directory while its pid is live, and on a launch
    that pid is the launcher's own). `clear_exit` is what stops it inheriting the dead
    frame's exit code along with the directory."""

    def test_a_recorded_code_is_gone_afterwards(self):
        state.record_exit("f-1", 99)
        state.clear_exit("f-1")
        self.assertIsNone(state.exit_code("f-1"))

    def test_the_version_a_panel_polls_is_left_alone(self):
        """Only `exit` is stale on a relaunch. `version` is a monotonic counter panels
        compare against their last reading, and moving it backwards — or removing it —
        is `bump`'s business, not this function's."""
        state.bump("f-1")
        before = state.version("f-1")
        state.clear_exit("f-1")
        self.assertEqual(state.version("f-1"), before)

    def test_clearing_a_frame_that_was_never_recorded_creates_nothing(self):
        """It runs on the launch path against an id that usually has no directory at
        all — the ordinary first launch for a workspace. A read must not mint one, the
        same rule `version()` follows."""
        state.clear_exit("never-existed")
        self.assertFalse(state.frame_dir("never-existed").exists())

    def test_clearing_a_hostile_fid_does_not_raise(self):
        state.clear_exit("../../escaped")  # must not raise
        self.assertFalse(state._root().exists())

    def test_clearing_survives_a_failing_unlink(self):
        """Nothing in this module raises: a launch is not worth failing over a file
        that could not be deleted."""
        state.record_exit("f-1", 99)
        with mock.patch("charter.frame.state.Path.unlink", side_effect=OSError("read-only")):
            state.clear_exit("f-1")  # must not raise


class RespawnAttempts(PersonaIso, unittest.TestCase):
    """The counter that stops a broken panel respawning forever.

    A panel pane's `pane-died` hook survives the respawn it triggers (verified against
    real tmux 3.7c: `show-hooks -p` reads the hook back unchanged after `respawn-pane`),
    so a panel that dies instantly on every start would respawn in a hot loop with
    nothing anywhere counting. tmux cannot count; this is where the count lives.
    """

    def setUp(self) -> None:
        super().setUp()
        # `respawn_attempt` never creates a frame's directory (see its own docstring),
        # so these tests stand up the state a live frame always already has: `cmd_launch`
        # creates and bumps it before a single pane is split.
        for fid in ("f-1", "f-2"):
            state.bump(fid)

    def test_the_first_attempt_is_one_and_each_call_claims_the_next(self):
        self.assertEqual(state.respawn_attempt("f-1", "top"), 1)
        self.assertEqual(state.respawn_attempt("f-1", "top"), 2)
        self.assertEqual(state.respawn_attempt("f-1", "top"), 3)

    def test_each_slot_counts_on_its_own(self):
        """One broken panel must not spend another's attempts — a `left` renderer that
        crashes on every start would otherwise use up `bottom`'s budget and stop a
        perfectly healthy panel being brought back after an unrelated death."""
        state.respawn_attempt("f-1", "top")
        state.respawn_attempt("f-1", "top")
        self.assertEqual(state.respawn_attempt("f-1", "bottom"), 1)

    def test_each_frame_counts_on_its_own(self):
        state.respawn_attempt("f-1", "top")
        self.assertEqual(state.respawn_attempt("f-2", "top"), 1)

    def test_a_frame_already_reaped_cannot_count_and_is_not_recreated(self):
        """Counting must not resurrect a directory `reap` has deleted — the hazard this
        module's own docstring records for `version()`, reached here through a write
        path instead of a read.

        Reached from a real frame's teardown, though less often since #383: the panels
        all die when the session is killed, so every panel's `pane-died` hook fires on
        the way out, and `reap` now KEEPS a directory whose launcher pid is still live —
        which the launcher's own closing `reap()` always is. So the directory usually
        survives that moment and is taken by a later launch instead; this is the case
        after that, where the count arrives at a name nothing owns any more. `f-gone`
        carries no pid at all (`_launcher_pid` needs `<name>-<digits>`), so `reap`
        removes it exactly as it did before #383."""
        state.bump("f-gone")
        state.reap(set(), server="charter")
        self.assertIsNone(state.respawn_attempt("f-gone", "top"))
        self.assertFalse(state.frame_dir("f-gone").exists(),
                         "counting a respawn recreated a frame directory reap removed")

    def test_a_frame_id_the_directory_layer_refuses_cannot_count(self):
        """`None`, never a number — and the caller reads that as "give up", not as
        "attempt zero". A count that cannot be recorded is exactly the state in which
        respawning is unbounded, so the safe degrade is to stop, leaving the dead pane
        and its own message visible."""
        self.assertIsNone(state.respawn_attempt("../../etc", "top"))

    def test_a_slot_name_with_a_separator_is_refused_rather_than_joined(self):
        """The slot is part of a FILE name. `commands_frame.cmd_respawn` already refuses
        a slot with no renderer before reaching here, but this module's own rule is that
        a name handed to it is resolved through `contain.child` rather than trusted by
        whoever called it (see `frame_dir`'s own docstring).

        `../y`, not the obvious `../../../etc/passwd`, and the difference IS the test.
        The obvious one lands under directories that do not exist, so a version with no
        containment check at all still answers `None` — from the failed write, not from
        any refusal — and the test passes green over a deleted guard (confirmed by
        mutation twice: the first shape of both this test and the code under it was
        green with `contain.child` replaced by a bare join). `../y` climbs exactly one
        level, out of the per-slot directory and back into the frame's own, where the
        write really would succeed — so only a refusal can produce `None`, and the file
        it would have left behind is checked for directly.
        """
        d = state.frame_dir("f-1", create=True)
        self.assertIsNone(state.respawn_attempt("f-1", "../y"))
        self.assertFalse((d / "y").exists(),
                         "the slot name was joined onto the path instead of refused")
        self.assertIsNone(state.respawn_attempt("f-1", "../../../etc/passwd"))

    def test_a_write_that_fails_cannot_count_rather_than_raising(self):
        """Same must-not-raise promise as `bump`: this runs from a tmux hook."""
        state.respawn_attempt("f-1", "top")
        with mock.patch("pathlib.Path.write_text", side_effect=OSError("full")):
            self.assertIsNone(state.respawn_attempt("f-1", "top"))


class ClearRespawn(PersonaIso, unittest.TestCase):
    """The other half of `clear_exit`'s bill, for the counter rather than the exit code.

    Same cause: since #383 `reap` keeps a directory while the pid in its name is live,
    so a launcher landing on a recycled pid for the SAME workspace mints the same id and
    adopts the previous frame's whole directory. `respawn_attempt` never resets, so an
    adopted count is a budget already spent on deaths that happened to another frame.
    """

    def test_the_next_attempt_starts_from_one_again(self):
        state.bump("f-1")
        state.respawn_attempt("f-1", "top")
        state.respawn_attempt("f-1", "top")
        state.clear_respawn("f-1")
        self.assertEqual(state.respawn_attempt("f-1", "top"), 1)

    def test_every_slot_is_cleared_not_only_the_one_that_died(self):
        """A frame's panels each keep their own file, and all of them are the previous
        frame's. Clearing one slot would leave the rest of the new frame's panels with a
        budget spent by a frame they were never part of."""
        state.bump("f-1")
        for slot in ("top", "bottom", "left"):
            state.respawn_attempt("f-1", slot)
        state.clear_respawn("f-1")
        for slot in ("top", "bottom", "left"):
            self.assertEqual(state.respawn_attempt("f-1", slot), 1, slot)

    def test_only_this_frames_counts_go(self):
        state.bump("f-1")
        state.bump("f-2")
        state.respawn_attempt("f-2", "top")
        state.clear_respawn("f-1")
        self.assertEqual(state.respawn_attempt("f-2", "top"), 2)

    def test_the_version_a_panel_polls_is_left_alone(self):
        """Same rule `clear_exit` follows: moving the counter panels compare against is
        `bump`'s business, and `cmd_launch` calls it one line later anyway."""
        state.bump("f-1")
        before = state.version("f-1")
        state.clear_respawn("f-1")
        self.assertEqual(state.version("f-1"), before)

    def test_clearing_a_frame_that_never_counted_creates_nothing(self):
        """The ordinary first launch for a workspace has no directory here at all, and
        a launch must not mint one just to empty it."""
        state.clear_respawn("never-existed")
        self.assertFalse(state.frame_dir("never-existed").exists())

    def test_clearing_a_hostile_fid_does_not_raise(self):
        state.clear_respawn("../../escaped")  # must not raise
        self.assertFalse(state._root().exists())


class Reap(PersonaIso, unittest.TestCase):
    """Every fixture in here is named after a pid that has genuinely exited, the KEPT
    ones as deliberately as the removed ones.

    Up to #383 the trailing number in these names was decoration (`dead-1`, `live-1`,
    `old-1`); `reap` now reads it as the launcher's pid, and **pid 1 is `launchd`/`init`,
    which never exits**. On the remove side that is loud — a fixture reap refuses to
    remove fails its own assertion. On the KEEP side it is silent and worse: `live-1`
    and `old-1` were kept by the pid rule, the `live` argument stopped deciding anything,
    and both tests passed with the live-session check deleted outright. `_a_dead_pid()`
    on both sides puts membership in `live` back to being the only thing that can keep
    a directory here.
    """

    def test_a_directory_whose_session_is_gone_is_removed(self):
        dead = f"dead-{_a_dead_pid()}"
        live = f"live-{_a_dead_pid()}"
        state.bump(dead)
        state.bump(live)
        removed = state.reap({live}, server="charter")
        self.assertEqual(removed, [dead])
        self.assertFalse(state.frame_dir(dead).exists())
        self.assertTrue(state.frame_dir(live).exists(),
                        "the live session's directory was reaped — and since its "
                        "launcher is dead, `live` is the only thing that could have "
                        "saved it")

    def test_a_live_frame_is_never_reaped_by_age(self):
        """A long-lived frame is exactly what an age heuristic would eat.

        The pid is a dead one on purpose (see the class docstring): this is the test
        that pins "reap never deletes by age", so the only reason its fixture may
        survive is the session being live."""
        old = f"old-{_a_dead_pid()}"
        state.bump(old)
        self.assertEqual(state.reap({old}, server="charter"), [])

    def test_a_sibling_exit_code_survives_a_reap_that_beats_its_own_launcher(self):
        """#383. `reap` runs at EVERY frame launch, and the set it is handed names the
        tmux sessions live at that instant. A sibling frame whose session has just ended
        is therefore absent from it while its own launcher is still inside `cmd_launch`,
        one line short of reading the `exit` file it just recorded. Removing the
        directory there does not merely lose bookkeeping: `exit_code` answers `None`,
        `cmd_launch` turns that into a returned 0, and a harness that actually failed is
        reported as a success to whatever `&&` chain or CI step called charter.

        This test process's own pid stands in for that launcher. It is not a charter
        launcher, which is the point — `reap` cannot tell the difference and must not
        try: all it may ask is whether the process named at the end of the directory is
        still there to come back for its answer."""
        fid = state.frame_id("sibling", os.getpid())
        state.record_exit(fid, 42)
        state.reap({"some-other-frames-session"}, server="charter")
        self.assertEqual(state.exit_code(fid), 42,
                         "reap deleted a live launcher's frame directory, and with it "
                         "the exit code that launcher had not read yet")

    def test_a_frame_whose_launcher_has_exited_is_still_removed(self):
        """The other half of #383, and the one that keeps the fix from being a no-op
        dressed as a fix: once the pid in the name is gone there is nobody left to read
        the `exit` file, so the directory is `reap`'s to remove exactly as before."""
        fid = state.frame_id("finished", _a_dead_pid())
        state.bump(fid)
        self.assertEqual(state.reap(set(), server="charter"), [fid])
        self.assertFalse(state.frame_dir(fid).exists())

    def test_a_directory_that_names_no_pid_is_still_removed(self):
        """Not everything under the frame root was minted by `frame_id`: debris, a
        hand-made directory, a name from an older charter. With no pid to ask about,
        the live-session test is the only evidence there is — and it is the one `reap`
        already had, so an unparseable name must not become undeletable."""
        state.bump("debris")
        self.assertEqual(state.reap(set(), server="charter"), ["debris"])

    def test_a_bare_number_is_not_read_as_a_pid(self):
        """`frame_id` always emits `<workspace>-<pid>` with a non-empty workspace (its
        `or "frame"` fallback guarantees one), so a directory that is nothing but digits
        did not come from it and those digits are not a claim about any process. Named
        after THIS process's pid — as live as a pid gets — so reading it as one would
        make the directory undeletable for as long as the suite runs."""
        name = str(os.getpid())
        state.bump(name)
        self.assertEqual(state.reap(set(), server="charter"), [name])

    def test_a_trailing_zero_is_not_read_as_a_pid(self):
        """`kill(2)` reads 0 as "every process in my group", not as a process, so
        `os.kill(0, 0)` SUCCEEDS and a frame named `ws-0` would look alive forever.
        `frame_id` can only ever have written a real `os.getpid()` there, and that is
        never 0 — so the number is debris and the directory stays reapable."""
        state.bump("ws-0")
        self.assertEqual(state.reap(set(), server="charter"), ["ws-0"])

    def test_a_launcher_this_user_may_not_signal_still_counts_as_alive(self):
        """EPERM is an ANSWER, and the opposite of what it looks like. `os.kill(pid, 0)`
        raises `PermissionError` for a process that exists and belongs to somebody else —
        another operator's frame on a shared machine, or one launched under `sudo`.
        Read as "gone", it would make every such frame reapable while its harness was
        still running, which is #383 again with a different cast.

        Forced rather than found: pid 1 answers this way for an unprivileged run and
        answers plain success for a root CI container, so asking the real machine would
        pin the branch on a laptop and quietly stop pinning it in CI."""
        state.bump("another-users-frame-4242")
        with mock.patch("charter.frame.state.os.kill",
                        side_effect=PermissionError(1, "Operation not permitted")):
            self.assertEqual(state.reap(set(), server="charter"), [])

    def test_liveness_is_never_asked_off_posix(self):
        """`os.kill(pid, 0)` is a question on POSIX and an ANSWER on Windows, where it
        maps to `TerminateProcess` — asking it there would kill whatever process the
        number in a directory name happened to land on. `news._outer_probe` documents
        the same trap and this file's own suite pins it there; this pins it here, where
        the number comes off a filesystem name rather than an environment variable.

        Asserted on the helper rather than through `reap`, and not for tidiness:
        `mock.patch` of `os.name` is global for its duration, and `_root()` builds a
        `Path` — under a patched `os.name` pathlib refuses to instantiate at all
        ("cannot instantiate 'WindowsPath' on your system"), so a test driving the whole
        of `reap` would fail on the wrong line and prove nothing about `os.kill`. The
        pid handed over is real and live (ours), so a helper that asked anyway would get
        a truthful "alive" back — the assertion has to be that the question was never
        PUT, not that the answer came out a particular way."""
        with mock.patch("charter.frame.state.os.name", "nt"), \
             mock.patch("charter.frame.state.os.kill") as kill:
            self.assertTrue(state._launcher_is_alive(os.getpid()))
        kill.assert_not_called()

    def test_a_number_too_large_to_be_a_pid_neither_raises_nor_survives(self):
        """`reap` runs on the launch path, where this module's own docstring promises
        nothing raises. A trailing number beyond what a `pid_t` can hold parses as an
        int perfectly well and then makes `os.kill` raise `OverflowError` — which is
        NOT an `OSError`, so guarding only that would let it escape into a launch. It
        also names no process, so the directory stays reapable."""
        name = "ws-99999999999999999999"
        state.bump(name)
        self.assertEqual(state.reap(set(), server="charter"), [name])


class ReapAcrossServers(PersonaIso, unittest.TestCase):
    """A frame lives on ONE tmux server, and only that server can say it is dead.

    Charter now runs frames on two: its own private one (`tmux -L charter`, sessions
    named by frame id) and, when charter is started from inside a tmux the operator
    already has, THEIRS (`tmux -S <socket>`, windows named by frame id). Neither
    server's liveness list mentions the other's frames at all, so an unscoped `reap`
    deletes the other's state on sight — a running frame's panels lose the version file
    they poll, and its recorded exit code goes with it, while the frame itself is still
    on screen. The frame's own server is written down when its directory is created and
    checked here.

    Every fixture here is named after a pid that has genuinely exited, for the reason
    `Reap`'s own docstring gives: since #383 `reap` reads the trailing number as the
    launcher's pid and keeps any directory whose launcher is still running. `mine-1` and
    `theirs-1` read as throwaway labels, but pid 1 is `launchd`/`init` — the pid rule
    would have kept every one of them and these tests would have passed with the server
    check deleted outright.
    """

    THEIRS = "/private/tmp/tmux-502/default"

    def _frame_on(self, stem, server):
        fid = f"{stem}-{_a_dead_pid()}"
        state.bump(fid)
        state.record_server(fid, server)
        return fid

    def test_a_frame_on_another_server_survives_this_servers_reap(self):
        mine = self._frame_on("mine", "charter")
        theirs = self._frame_on("theirs", self.THEIRS)
        self.assertEqual(state.reap(set(), server="charter"), [mine])
        self.assertTrue(state.frame_dir(theirs).exists(),
                        "the other server's frame was reaped — and since its launcher "
                        "is dead too, the recorded server is the only thing that could "
                        "have saved it")

    def test_the_other_server_reaps_its_own(self):
        """The same test from the other side, so a `reap` that simply never removed
        anything would not pass both."""
        mine = self._frame_on("mine", "charter")
        theirs = self._frame_on("theirs", self.THEIRS)
        self.assertEqual(state.reap(set(), server=self.THEIRS), [theirs])
        self.assertTrue(state.frame_dir(mine).exists())

    def test_a_live_frame_on_this_server_is_still_kept(self):
        gone = self._frame_on("mine", "charter")
        live = self._frame_on("mine", "charter")
        self.assertEqual(state.reap({live}, server="charter"), [gone])

    def test_a_live_launcher_on_this_server_is_still_kept(self):
        """The two guards are independent and BOTH are asked (#381 + #383). This one
        matches the server exactly and is absent from `live`, so the server check has
        nothing left to say — only the pid rule can keep it, and it must, or #383's
        fix stops reaching frames that record a server (which, since #381, is all of
        them)."""
        fid = state.frame_id("sibling", os.getpid())
        state.record_server(fid, "charter")
        state.record_exit(fid, 42)
        self.assertEqual(state.reap(set(), server="charter"), [])
        self.assertEqual(state.exit_code(fid), 42)

    def test_a_frame_from_before_charter_recorded_this_is_still_reapable(self):
        """The migration case, and the one place an unknown server matches every
        server. A directory with no marker was written by a charter that only ever ran
        frames on its own private server; leaving it unreapable forever would trade a
        transient bug for a permanent leak."""
        fid = f"legacy-{_a_dead_pid()}"
        state.bump(fid)
        self.assertIsNone(state.frame_server(fid))
        self.assertEqual(state.reap(set(), server=self.THEIRS), [fid])

    def test_the_recorded_server_reads_back(self):
        state.record_server("f-1", self.THEIRS)
        self.assertEqual(state.frame_server("f-1"), self.THEIRS)

    def test_recording_a_server_for_an_id_no_directory_can_be_made_for_is_a_no_op(self):
        """`record_server` runs on the launch path, where an id `contain.child` refuses
        must degrade rather than raise — the same promise every other writer in this
        module makes."""
        state.record_server("../escape", "charter")
        self.assertIsNone(state.frame_server("../escape"))


class TheFramesOwnWorkspace(PersonaIso, unittest.TestCase):
    """`record_workspace`/`frame_workspace` — #512.

    A frame is launched FOR a workspace, and no process inside it can work out which one:
    `workspace.resolve`'s deciding rungs are a `$CHARTER_WORKSPACE` the launcher usually
    does not have, a cwd that is the plane root, a per-session pointer keyed on an id that
    inside a frame names the FRAME, and a per-terminal pointer keyed on the asking pane.
    The launcher is one ordinary shell in the operator's own terminal and answers all
    three; a panel answers none of them, and falls to `default`. So the launcher writes
    the answer down.
    """

    def test_the_recorded_workspace_reads_back(self):
        state.record_workspace("f-1", "harness-wrapper")
        self.assertEqual(state.frame_workspace("f-1"), "harness-wrapper")

    def test_a_frame_nobody_recorded_one_for_says_it_does_not_know(self):
        """`None`, never a guessed name. The migration case (a frame launched by a
        charter that predates this and still running across the upgrade) and the failed
        write are the same fact — "do not take this frame's workspace from here" — and
        `slots._frame_workspace` is what decides what to do instead."""
        state.bump("f-never-recorded")
        self.assertIsNone(state.frame_workspace("f-never-recorded"))

    def test_a_relaunch_on_the_same_id_overwrites_rather_than_keeps(self):
        """The recycled-pid case #383 is about, on this file. A frame id is
        `<workspace>-<launcher pid>` and `reap` keeps a directory while that pid is live —
        which on a launch it is, because it is the launcher's own. An adopted `workspace`
        is another frame's answer, so every launch rewrites it, exactly as `record_server`
        does with its own marker."""
        state.record_workspace("f-1", "an-older-frames-workspace")
        state.record_workspace("f-1", "this-frames-workspace")
        self.assertEqual(state.frame_workspace("f-1"), "this-frames-workspace")

    def test_a_name_that_could_escape_the_workspaces_directory_is_refused_on_read(self):
        """The value is joined onto `workspaces/` by `workspace_dir()` and drawn on a
        panel's screen. #442 is what an unchecked `../../` in that position already cost
        once, through `workspace.declared_default`; this keeps the same rule
        (`workspace.valid_name`) on charter's own copy of the same kind of value.

        Written past `record_workspace` deliberately — the writer is charter's own
        launcher and never produces this, so a test that went through it would be pinning
        the writer rather than the reader that has to survive a corrupt file."""
        d = state.frame_dir("f-1", create=True)
        (d / "workspace").write_text("../../escaped\n")
        self.assertIsNone(state.frame_workspace("f-1"))

    def test_an_empty_recorded_workspace_is_not_known_either(self):
        """A truncated write is the shape that would otherwise pass the truthiness test
        one layer up and hand `workspace_dir()` the `workspaces/` directory itself."""
        d = state.frame_dir("f-1", create=True)
        (d / "workspace").write_text("\n")
        self.assertIsNone(state.frame_workspace("f-1"))

    def test_recording_for_an_id_no_directory_can_be_made_for_is_a_no_op(self):
        """The launch path's own promise, kept here too: an id `contain.child` refuses
        degrades to "charter does not know" rather than taking the launch down."""
        state.record_workspace("../escape", "demo")
        self.assertIsNone(state.frame_workspace("../escape"))

    def test_reading_never_creates_the_directory_it_looked_in(self):
        """The rule the whole module keeps and `version`'s docstring states: a read must
        not resurrect a directory `reap()` has just removed."""
        self.assertIsNone(state.frame_workspace("f-never-existed"))
        self.assertFalse(state.frame_dir("f-never-existed").exists(),
                         "a read minted the frame directory it was only looking in")


if __name__ == "__main__":
    unittest.main()
