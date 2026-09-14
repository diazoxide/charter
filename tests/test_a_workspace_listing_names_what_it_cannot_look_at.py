"""Which workspaces a plane has is one answer on every interpreter, and what could not be looked
at is named beside it rather than dropped or raised (#1043, ADR 0009).

`workspace.list_workspaces` asked `Path.is_dir` of each entry under `workspaces/`. For an entry
charter cannot `stat` — `workspaces/` at mode 666, readable and not searchable, is the measured
case — that call raised on 3.11–3.13 and answered False on 3.14. So on 3.11 every doctor row that
lists workspaces read `not checked`, and on 3.14 each such workspace vanished from all of them
while the rows stayed green.

The fixtures refuse the `stat` itself and state `Path.is_dir` / `Path.exists` both ways, raising
and answering False, so the two interpreter behaviours are pinned on whichever one runs.
"""

from __future__ import annotations

import contextlib
import errno
import os
import unittest
from pathlib import Path
from unittest import mock

from charter import config, workspace
from tests._isolation import PersonaIso

#: How `Path.is_dir` / `Path.exists` answer for a path whose `stat` is refused: raising is
#: 3.11–3.13, False is 3.14.
BOTH_INTERPRETERS = ("raises", False)


def refused(p) -> PermissionError:
    return PermissionError(errno.EACCES, os.strerror(errno.EACCES), str(p))


def unsearchable(directory: Path, answers):
    """*directory* listable and not searchable, the way mode 666 leaves it: its names can be read
    and nothing under it can be `stat`-ed. `Path.is_dir` and `Path.exists` answer for those paths
    the way one interpreter does — *answers* is `"raises"` or `False`."""
    real_stat, real_lstat = os.stat, os.lstat
    real_is_dir, real_exists = Path.is_dir, Path.exists

    def inside(p) -> bool:
        return isinstance(p, (str, os.PathLike)) and directory in Path(p).parents

    def stat(p, *args, **kwargs):
        if inside(p):
            raise refused(p)
        return real_stat(p, *args, **kwargs)

    def lstat(p, *args, **kwargs):
        if inside(p):
            raise refused(p)
        return real_lstat(p, *args, **kwargs)

    def answering(real):
        def ask(self, *args, **kwargs):
            if inside(self):
                if answers == "raises":
                    raise refused(self)
                return answers
            return real(self, *args, **kwargs)
        return ask

    stack = contextlib.ExitStack()
    stack.enter_context(mock.patch.object(os, "stat", stat))
    stack.enter_context(mock.patch.object(os, "lstat", lstat))
    stack.enter_context(mock.patch.object(Path, "is_dir", answering(real_is_dir)))
    stack.enter_context(mock.patch.object(Path, "exists", answering(real_exists)))
    return stack


class ListingCase(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        self.root = config.WORKSPACES_DIR
        for ws in ("alpha", "beta"):
            (self.root / ws / "memory").mkdir(parents=True)


class AnEntryItCannotStatIsNamedNotDroppedOrRaised(ListingCase):
    def test_on_either_interpreter_every_entry_is_named_with_its_errno(self):
        for answers in BOTH_INTERPRETERS:
            with self.subTest(is_dir=answers):
                with unsearchable(self.root, answers):
                    names, unread = workspace.read_workspaces()
                self.assertEqual(names, [])
                self.assertEqual(unread, [(self.root / "alpha", errno.EACCES),
                                          (self.root / "beta", errno.EACCES)])

    def test_the_names_are_the_same_answer_on_either_interpreter(self):
        """`list_workspaces` is the first half of that answer: what can be opened. It neither
        raises (3.11–3.13) nor differs from it (3.14)."""
        for answers in BOTH_INTERPRETERS:
            with self.subTest(is_dir=answers):
                with unsearchable(self.root, answers):
                    self.assertEqual(workspace.list_workspaces(), [])
                    self.assertEqual(workspace.uncheckable_workspaces(),
                                     [("alpha", errno.EACCES), ("beta", errno.EACCES)])

    @unittest.skipIf(os.geteuid() == 0, "root searches a directory whatever its mode")
    def test_a_real_workspaces_directory_at_mode_666(self):
        """The state #1043 was measured in."""
        self.root.chmod(0o666)
        self.addCleanup(self.root.chmod, 0o755)
        names, unread = workspace.read_workspaces()
        self.assertEqual((names, unread), ([], [(self.root / "alpha", errno.EACCES),
                                                 (self.root / "beta", errno.EACCES)]))

    def test_a_workspace_that_is_a_symlink_loop_is_named_with_eloop(self):
        loop = self.root / "gamma"
        loop.symlink_to(loop)
        self.assertEqual(workspace.read_workspaces(),
                         (["alpha", "beta"], [(loop, errno.ELOOP)]))


class TheLegacyCloneScanAsksTheSameWay(ListingCase):
    """`legacy_flat_clones` asked `Path.is_dir` of every entry under `workspaces/` too, so
    `charter status` ended in a traceback on 3.11–3.13 over a workspace it cannot `stat`, after
    printing the rows it could (#1043). The workspace is named by the listing; this scan is only
    looking for stray clones, and one it cannot tell about is none it can report."""

    def test_on_either_interpreter_it_answers_and_does_not_raise(self):
        for answers in BOTH_INTERPRETERS:
            with self.subTest(is_dir=answers):
                with unsearchable(self.root, answers):
                    self.assertEqual(workspace.legacy_flat_clones(), [])

    def test_a_stray_clone_among_the_workspaces_is_still_found(self):
        stray = self.root / "stray"
        (stray / ".git").mkdir(parents=True)
        (self.root / "notes.txt").write_text("")
        self.assertEqual(workspace.legacy_flat_clones(), [stray])


class WhatIsNotAWorkspaceIsNeitherListedNorNamed(ListingCase):
    """The other side of the line: only an entry whose kind cannot be told is unread."""

    def test_a_readable_plane_names_nothing(self):
        self.assertEqual(workspace.read_workspaces(), (["alpha", "beta"], []))

    def test_a_file_a_dot_entry_a_stray_clone_and_a_link_to_nothing(self):
        (self.root / ".DS_Store").write_text("")
        (self.root / "notes.txt").write_text("")
        (self.root / ".worktrees").mkdir()
        (self.root / "stray" / ".git").mkdir(parents=True)
        (self.root / "gone").symlink_to(self.root / "nowhere")
        self.assertEqual(workspace.read_workspaces(), (["alpha", "beta"], []))

    def test_no_workspaces_directory_is_no_workspaces(self):
        for ws in ("alpha", "beta"):
            (self.root / ws / "memory").rmdir()
            (self.root / ws).rmdir()
        self.root.rmdir()
        self.assertEqual(workspace.read_workspaces(), ([], []))

    @unittest.skipIf(os.geteuid() == 0, "root lists a directory whatever its mode")
    def test_a_workspaces_directory_it_cannot_list_still_raises(self):
        """Nothing under it can be counted, so the rows that list workspaces keep reading that as
        `not checked` for the whole row — the contract #1012 and #1014 left it with."""
        self.root.chmod(0o000)
        self.addCleanup(self.root.chmod, 0o755)
        with self.assertRaises(PermissionError):
            workspace.read_workspaces()
        with self.assertRaises(PermissionError):
            workspace.list_workspaces()


class EveryDoctorRowThatListsWorkspacesNamesThem(ListingCase):
    """`workspace layer` and `changes` list workspaces too, and read the same two wrong answers
    as the memory and clones rows: `not checked` for the whole row on 3.11–3.13, OK over a plane
    with no workspace read on 3.14. Each says beside its verdict what it could not check, and is
    not OK while it stands."""

    DETAIL = "; workspaces/alpha, workspaces/beta cannot be checked"
    HINT = ("workspaces/alpha cannot be checked — restoring read access to it clears this; "
            "workspaces/beta cannot be checked — restoring read access to it clears this.")

    def setUp(self) -> None:
        super().setUp()
        (self.tmp / "charter.toml").write_text("schema = 1\n")
        config.HAS_CONTROL_PLANE = True

    def rows(self):
        from charter import doctor
        return {"workspace layer": doctor.check_workspace_harness,
                "changes": doctor.check_changes}

    def test_with_every_workspace_readable_nothing_is_named(self):
        from charter import doctor
        for name, check in self.rows().items():
            with self.subTest(row=name):
                r = check()
                self.assertEqual(r.status, doctor.OK, r)
                self.assertNotIn("cannot be checked", r.detail)

    def test_on_either_interpreter_each_row_names_what_it_could_not_check(self):
        from charter import doctor
        for name, check in self.rows().items():
            for answers in BOTH_INTERPRETERS:
                with self.subTest(row=name, is_dir=answers):
                    with unsearchable(self.root, answers):
                        r = check()
                    self.assertEqual(r.status, doctor.WARN, r)
                    self.assertTrue(r.detail.endswith(self.DETAIL), r.detail)
                    self.assertEqual(r.hint, self.HINT)

    def test_a_failing_row_stays_failed_beside_what_it_could_not_check(self):
        """The clause lifts OK to WARN and lowers nothing: a record nobody can read is still a
        FAIL when a workspace one over could not be looked at."""
        from charter import change, doctor
        store = change.changes_dir("alpha")
        store.mkdir(parents=True)
        (store / "broken.json").write_text("{not json")
        loop = self.root / "gamma"
        loop.symlink_to(loop)
        r = doctor.check_changes()
        self.assertEqual(r.status, doctor.FAIL, r)
        self.assertTrue(r.detail.startswith("unreadable record(s): alpha/broken"), r.detail)
        self.assertTrue(r.detail.endswith("; workspaces/gamma cannot be checked"), r.detail)
        self.assertTrue(r.hint.endswith(f"   workspaces/gamma cannot be checked — fix the symlink "
                                        f"loop at {loop}."), r.hint)

    @unittest.skipIf(os.geteuid() == 0, "root searches a directory whatever its mode")
    def test_a_real_workspaces_directory_at_mode_666(self):
        from charter import doctor
        self.root.chmod(0o666)
        self.addCleanup(self.root.chmod, 0o755)
        for name, check in self.rows().items():
            with self.subTest(row=name):
                r = check()
                self.assertEqual(r.status, doctor.WARN, r)
                self.assertTrue(r.detail.endswith(self.DETAIL), r.detail)


if __name__ == "__main__":
    unittest.main()
