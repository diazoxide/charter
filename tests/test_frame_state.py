"""The frame's own state: who it is, when it last changed, how it ended.

Per frame rather than global, because two frames may run at once (one session each, named
by workspace and pid) and a shared version file would make each frame's panels redraw for
the other's activity.
"""

from __future__ import annotations

import unittest
from unittest import mock

from tests._isolation import PersonaIso
from charter.frame import state


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

    def test_reap_then_version_does_not_resurrect_the_directory(self):
        """If `version()` ever called `bump()` on a miss, this would fight `reap()`
        forever: reap deletes, the next poll recreates, reap deletes again."""
        state.bump("gone-1")
        state.reap(set())
        self.assertEqual(state.version("gone-1"), "0")
        self.assertFalse(state.frame_dir("gone-1").exists())


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


class Reap(PersonaIso, unittest.TestCase):
    def test_a_directory_whose_session_is_gone_is_removed(self):
        state.bump("dead-1")
        state.bump("live-1")
        removed = state.reap({"live-1"})
        self.assertEqual(removed, ["dead-1"])
        self.assertFalse(state.frame_dir("dead-1").exists())
        self.assertTrue(state.frame_dir("live-1").exists())

    def test_a_live_frame_is_never_reaped_by_age(self):
        """A long-lived frame is exactly what an age heuristic would eat."""
        state.bump("old-1")
        self.assertEqual(state.reap({"old-1"}), [])


if __name__ == "__main__":
    unittest.main()
