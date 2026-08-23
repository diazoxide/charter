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
        state.reap(set())
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
        removed = state.reap({live})
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
        self.assertEqual(state.reap({old}), [])

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
        state.reap({"some-other-frames-session"})
        self.assertEqual(state.exit_code(fid), 42,
                         "reap deleted a live launcher's frame directory, and with it "
                         "the exit code that launcher had not read yet")

    def test_a_frame_whose_launcher_has_exited_is_still_removed(self):
        """The other half of #383, and the one that keeps the fix from being a no-op
        dressed as a fix: once the pid in the name is gone there is nobody left to read
        the `exit` file, so the directory is `reap`'s to remove exactly as before."""
        fid = state.frame_id("finished", _a_dead_pid())
        state.bump(fid)
        self.assertEqual(state.reap(set()), [fid])
        self.assertFalse(state.frame_dir(fid).exists())

    def test_a_directory_that_names_no_pid_is_still_removed(self):
        """Not everything under the frame root was minted by `frame_id`: debris, a
        hand-made directory, a name from an older charter. With no pid to ask about,
        the live-session test is the only evidence there is — and it is the one `reap`
        already had, so an unparseable name must not become undeletable."""
        state.bump("debris")
        self.assertEqual(state.reap(set()), ["debris"])

    def test_a_bare_number_is_not_read_as_a_pid(self):
        """`frame_id` always emits `<workspace>-<pid>` with a non-empty workspace (its
        `or "frame"` fallback guarantees one), so a directory that is nothing but digits
        did not come from it and those digits are not a claim about any process. Named
        after THIS process's pid — as live as a pid gets — so reading it as one would
        make the directory undeletable for as long as the suite runs."""
        name = str(os.getpid())
        state.bump(name)
        self.assertEqual(state.reap(set()), [name])

    def test_a_trailing_zero_is_not_read_as_a_pid(self):
        """`kill(2)` reads 0 as "every process in my group", not as a process, so
        `os.kill(0, 0)` SUCCEEDS and a frame named `ws-0` would look alive forever.
        `frame_id` can only ever have written a real `os.getpid()` there, and that is
        never 0 — so the number is debris and the directory stays reapable."""
        state.bump("ws-0")
        self.assertEqual(state.reap(set()), ["ws-0"])

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
            self.assertEqual(state.reap(set()), [])

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
        self.assertEqual(state.reap(set()), [name])


if __name__ == "__main__":
    unittest.main()
