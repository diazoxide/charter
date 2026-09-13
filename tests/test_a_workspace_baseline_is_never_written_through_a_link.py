"""`charter workspace reinit` writes nothing through a symlink where a baseline path belongs — #1037.

A workspace's tree travels with the plane, and git stores a symlink as a symlink. With
`workspaces/<ws>/refs/README.md` committed as a link to a path that does not exist,
`structure_status` called the README missing and `scaffold` wrote it — through the link, so
the file was created wherever the link pointed, with the permissions of whoever ran `reinit`.
The content was charter's own README; the PLACE was the committer's.

So every baseline path is asked with `lstat`, from the workspace directory down, and a link
there is in the way: nothing is written through it, and `reinit` names it with what clears it.
The one link that is still followed is a DIRECTORY link to a directory that is there and
inside the plane's data — the repointed `refs/` #1028's sentence already sends an operator
to. A link where a FILE belongs is never followed, wherever it points.

`memory` as a plain file was the second half: `refs` had #1028's sentence and `memory`
raised `contain.Refused` out of the command. Both come from the one check now.

Every link here points inside the case's own temp directory, and "outside the plane" means
outside the plane's data directories (`personas/`, `workspaces/`), which is the boundary
`contain` draws — never outside the temp directory.
"""

from __future__ import annotations

import errno
import os
import shutil
import stat
import threading
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands_workspace, config, workspace
from tests import _isolation


class AWorkspaceWithABaseline(_isolation.PersonaIso):
    """A real plane with one scaffolded workspace, and a directory beside the plane's data
    for a link to point out of it."""

    def setUp(self) -> None:
        super().setUp()
        _isolation.make_plane(self)
        self.ws = "north"
        workspace.ensure(self.ws)
        self.wd = workspace.workspace_dir(self.ws)
        # Inside the temp root and outside `personas/` and `workspaces/`: out of the plane's
        # data as `contain` measures it, and nowhere a failed test can leave litter.
        self.outside = self.tmp / "outside"
        self.outside.mkdir()

    def reinit(self) -> tuple[int, list[str]]:
        said: list[str] = []
        with mock.patch.object(commands_workspace.util, "ok", side_effect=said.append), \
             mock.patch.object(commands_workspace.util, "warn", side_effect=said.append), \
             mock.patch.object(commands_workspace.util, "err", side_effect=said.append), \
             mock.patch.object(commands_workspace.util, "info", side_effect=lambda m: None):
            rc = commands_workspace.cmd_workspace_reinit(SimpleNamespace(name=self.ws, all=False))
        return rc, said

    def replaced_by_link(self, p: Path, target: Path) -> None:
        if p.is_dir() and not p.is_symlink():
            shutil.rmtree(p)
        else:
            p.unlink()
        p.symlink_to(target)

    def a_link_row(self, rel: str, path: Path, real: str = "file") -> str:
        return (f"'{self.ws}': {rel} cannot be created — {path} is a symlink, and charter writes "
                f"nothing through one; replacing it with a real {real} clears this.")


class ALinkWhereABaselineFileBelongs(AWorkspaceWithABaseline):

    def test_a_dangling_refs_readme_does_not_create_what_it_points_at(self):
        """The issue's own shape. `write_text` follows a link to nowhere and creates its target,
        so a committed link at a name charter writes is a write to a path the committer chose."""
        readme = workspace.refs_dir(self.ws) / "README.md"
        target = self.outside / "planted.md"
        self.replaced_by_link(readme, target)
        rc, said = self.reinit()
        self.assertEqual(rc, 0, said)
        self.assertFalse(os.path.lexists(target), "reinit created the file the link names")
        self.assertEqual(os.readlink(readme), str(target), "charter touched the link")
        self.assertIn(self.a_link_row("refs/README.md", readme), said)
        for claim in ("Up to date", "added refs/README.md"):
            self.assertFalse(any(claim in s for s in said), (claim, said))

    def test_a_link_to_a_file_outside_the_plane_is_named_and_left_as_it_is(self):
        """A live link out of the plane at the memory index answered "there" through the link,
        and `scaffold`'s index write then raised `contain.Refused` out of `reinit`. It is in the
        way like the dangling one: named, and the file it points at is not written."""
        idx = workspace.memory_index(self.ws)
        target = self.outside / "somebodys.md"
        target.write_text("somebody's notes\n")
        self.replaced_by_link(idx, target)
        rc, said = self.reinit()
        self.assertEqual(rc, 0, said)
        self.assertEqual(target.read_text(), "somebody's notes\n", "charter wrote through the link")
        self.assertEqual(os.readlink(idx), str(target), "charter touched the link")
        self.assertIn(self.a_link_row("memory/MEMORY.md", idx), said)

    def test_a_dangling_link_inside_the_plane_does_not_create_its_target_either(self):
        """`contain.writable` follows a link that lands inside the plane's data, dangling or not,
        so the containment check alone let `scaffold_charter` create `workspace.md`'s target. A
        link where a baseline FILE belongs is never followed, wherever it points."""
        charter = workspace.charter_file(self.ws)
        target = self.wd / "elsewhere.md"
        self.replaced_by_link(charter, target)
        rc, said = self.reinit()
        self.assertEqual(rc, 0, said)
        self.assertFalse(os.path.lexists(target), "scaffold created the file the link names")
        self.assertIn(self.a_link_row("workspace.md", charter), said)

    def test_a_charter_linked_out_of_the_plane_does_not_take_reinit_down(self):
        """`scaffold_charter` asks `contain.writable` first, which RAISES for a link out of the
        plane — so a committed `workspace.md` link ended `reinit` in `contain.Refused` before the
        row that names it could print."""
        charter = workspace.charter_file(self.ws)
        target = self.outside / "their-charter.md"
        target.write_text("theirs\n")
        self.replaced_by_link(charter, target)
        rc, said = self.reinit()
        self.assertEqual(rc, 0, said)
        self.assertEqual(target.read_text(), "theirs\n")
        self.assertIn(self.a_link_row("workspace.md", charter), said)

    def test_a_live_link_inside_the_plane_is_named_too(self):
        """Not only the links that would be written through today: a link where a baseline file
        belongs is named wherever it points, so a committed link is seen before the day its target
        goes away and a write would follow it."""
        readme = workspace.refs_dir(self.ws) / "README.md"
        target = self.wd / "real-readme.md"
        target.write_text("kept\n")
        self.replaced_by_link(readme, target)
        rc, said = self.reinit()
        self.assertEqual(rc, 0, said)
        self.assertEqual(target.read_text(), "kept\n")
        self.assertIn(self.a_link_row("refs/README.md", readme), said)

    def test_a_manifest_that_is_a_link_is_named_as_one_and_not_called_unwritten(self):
        """The manifest's own after-the-fact check said "could not be written — something is in
        the way" for a link, and counted the workspace as a repair that failed. It is the same
        link as every other baseline name, and gets the same sentence, once."""
        manifest = workspace.manifest_path(self.ws)
        # Inside the plane's data: a target outside it is refused by `contain` on the write, so
        # only this one tells "never tried" from "tried and was refused".
        target = self.wd / "nowhere.json"
        self.replaced_by_link(manifest, target)
        rc, said = self.reinit()
        self.assertEqual(rc, 0, said)
        self.assertFalse(os.path.lexists(target))
        self.assertEqual(os.readlink(manifest), str(target), "charter replaced the link")
        self.assertIn(self.a_link_row("workspace.json", manifest), said)
        for claim in ("could not be written", "Up to date"):
            self.assertFalse(any(claim in s for s in said), (claim, said))

    def test_a_link_where_a_file_belongs_is_listed_in_the_way_and_does_not_flag_the_workspace(self):
        """Not `missing`, for #1028's reason: a status line that says "needs reinit" sends the
        operator to a command that will not add the file."""
        readme = workspace.refs_dir(self.ws) / "README.md"
        self.replaced_by_link(readme, self.outside / "planted.md")
        status = workspace.structure_status(self.ws)
        self.assertEqual(status["missing"], [])
        self.assertEqual(status["in_the_way"], [("refs/README.md", readme, errno.ELOOP)])
        self.assertFalse(workspace.needs_reinit(self.ws))

    def test_a_baseline_file_that_is_a_symlink_loop_is_named_as_the_loop_alone(self):
        """A link, and one the filesystem will not answer for: #980's row names it with the loop to
        fix, and the link row must not name the same path a second time with another repair."""
        readme = workspace.refs_dir(self.ws) / "README.md"
        self.replaced_by_link(readme, readme)
        status = workspace.structure_status(self.ws)
        self.assertEqual(status["in_the_way"], [])
        self.assertEqual([rel for rel, _path, _code in status["unreadable"]], ["refs/README.md"])


class ALinkOrAFileWhereABaselineDirectoryBelongs(AWorkspaceWithABaseline):

    def test_a_memory_that_is_a_file_is_named_to_move_out_of_the_way(self):
        """#1037's second half: #1028 gave `refs` this sentence, and `memory` as a file still took
        `reinit` down with `contain.Refused` from the index write. One check, every directory."""
        memory = workspace.memory_dir(self.ws)
        shutil.rmtree(memory)
        memory.write_text("somebody's notes\n")
        rc, said = self.reinit()
        self.assertEqual(rc, 0, said)
        self.assertIn(f"'{self.ws}': memory/MEMORY.md cannot be created — {memory} is not a "
                      f"directory, and charter never moves existing content; moving it out of the "
                      f"way clears this.", said)
        self.assertEqual(memory.read_text(), "somebody's notes\n", "charter touched the file")
        self.assertFalse(any("Up to date" in s for s in said), said)

    def test_a_memory_linked_to_a_directory_outside_the_plane_gets_nothing_written_into_it(self):
        memory = workspace.memory_dir(self.ws)
        self.replaced_by_link(memory, self.outside)
        rc, said = self.reinit()
        self.assertEqual(rc, 0, said)
        self.assertEqual(list(self.outside.iterdir()), [], "charter wrote out of the plane")
        self.assertIn(self.a_link_row("memory/MEMORY.md", memory, real="directory"), said)

    def test_a_refs_linked_to_a_directory_outside_the_plane_gets_no_readme_written_into_it(self):
        """The README write had no containment check at all, so a `refs` link to any directory
        that exists put charter's README in it."""
        refs = workspace.refs_dir(self.ws)
        self.replaced_by_link(refs, self.outside)
        rc, said = self.reinit()
        self.assertEqual(rc, 0, said)
        self.assertEqual(list(self.outside.iterdir()), [], "charter wrote out of the plane")
        self.assertIn(self.a_link_row("refs/README.md", refs, real="directory"), said)

    def test_a_refs_linked_to_a_file_is_named_as_the_link_it_is(self):
        refs = workspace.refs_dir(self.ws)
        target = self.outside / "a-file"
        target.write_text("kept\n")
        self.replaced_by_link(refs, target)
        rc, said = self.reinit()
        self.assertEqual(rc, 0, said)
        self.assertEqual(target.read_text(), "kept\n")
        self.assertIn(self.a_link_row("refs/README.md", refs, real="directory"), said)

    def test_a_refs_linked_to_a_directory_inside_the_plane_is_still_followed(self):
        """The link that stays followed, and the reason directories and files are told apart: a
        `refs` repointed at a directory inside the plane is what #1028's sentence tells an operator
        to make, and `contain` has always followed a link that lands inside the plane's data."""
        refs = workspace.refs_dir(self.ws)
        shared = self.wd / "shared-refs"
        shared.mkdir()
        self.replaced_by_link(refs, shared)
        self.assertEqual(workspace.structure_status(self.ws)["in_the_way"], [])
        rc, said = self.reinit()
        self.assertEqual(rc, 0, said)
        self.assertTrue((shared / "README.md").is_file(), "the README was not written through")
        self.assertFalse(any("cannot be created" in s for s in said), said)

    def test_a_refs_linked_to_plane_data_outside_workspaces_is_still_followed(self):
        """The other arm of the one conditional: `workspaces/` is the WORKSPACE DIRECTORY's
        bound (#1062), and a directory beneath it answers to `contain`'s plane-data rule. A
        `refs` shared with a persona's references lands in `personas/`, inside the plane's data
        and outside `workspaces/`, so only this input tells the two rules apart for `refs`."""
        refs = workspace.refs_dir(self.ws)
        shared = config.PERSONAS_DIR / "steward" / "refs"
        shared.mkdir(parents=True)
        self.replaced_by_link(refs, shared)
        self.assertEqual(workspace.structure_status(self.ws)["in_the_way"], [])
        rc, said = self.reinit()
        self.assertEqual(rc, 0, said)
        self.assertTrue((shared / "README.md").is_file(), "the README was not written through")

    def test_a_memory_linked_to_nothing_is_named_with_the_link_to_fix(self):
        """`FileNotFoundError` from the `stat` of a directory whose `lstat` answered: a link to a
        target that is gone. #1028's sentence, for `memory` from the same check as `refs`."""
        memory = workspace.memory_dir(self.ws)
        self.replaced_by_link(memory, self.outside / "gone")
        rc, said = self.reinit()
        self.assertEqual(rc, 0, said)
        self.assertIn(f"'{self.ws}': memory/MEMORY.md cannot be created — {memory} is a symlink "
                      f"whose target is not there, and charter writes nothing through it; "
                      f"removing or repointing that link clears this.", said)
        self.assertFalse(os.path.lexists(self.outside / "gone"))

    def test_a_refs_linked_beneath_a_file_is_named_as_a_link_to_nothing(self):
        """`NotADirectoryError` from that same `stat`: the link's target runs through a file, so
        the name is there and resolves to nothing — the dangling link's answer, not a crash out
        of the `mkdir` and not "added"."""
        refs = workspace.refs_dir(self.ws)
        (self.outside / "a-file").write_text("kept\n")
        self.replaced_by_link(refs, self.outside / "a-file" / "sub")
        with self.assertRaises(NotADirectoryError, msg="fixture: the target must raise ENOTDIR"):
            os.stat(refs)
        rc, said = self.reinit()
        self.assertEqual(rc, 0, said)
        self.assertIn(f"'{self.ws}': refs/README.md cannot be created — {refs} is a symlink whose "
                      f"target is not there, and charter writes nothing through it; removing or "
                      f"repointing that link clears this.", said)
        self.assertEqual((self.outside / "a-file").read_text(), "kept\n")

    def test_a_path_beneath_a_directory_it_may_not_search_is_never_called_in_the_way(self):
        """The `lstat` that is refused rather than answered. `structure_status` asks this only of a
        rel that answered, so no command reaches it but a race; asked of the function directly,
        as #1028 asks its loop, because a refusal must come back as "not mine to name", never
        raise out of `reinit`."""
        if os.geteuid() == 0:
            self.skipTest("root ignores the mode, so this says nothing about the clause")
        refs = workspace.refs_dir(self.ws)
        refs.chmod(0o000)
        self.addCleanup(refs.chmod, 0o755)
        with self.assertRaises(PermissionError, msg="fixture: the lstat must be refused"):
            os.lstat(refs / "README.md")
        self.assertIsNone(workspace._in_the_way(refs / "README.md", self.wd))

    def test_a_directory_link_that_loops_is_never_called_in_the_way(self):
        """The `stat` that is refused after its `lstat` answered: a `memory` link that loops raises
        ELOOP, which is neither "resolves to nothing" nor "no directory". It is the uncheckable
        kind, `_stopped_at`'s to name with "fix the symlink loop", so here it is `None`."""
        memory = workspace.memory_dir(self.ws)
        self.replaced_by_link(memory, memory)
        with self.assertRaises(OSError, msg="fixture: the stat must loop") as caught:
            os.stat(memory)
        self.assertEqual(caught.exception.errno, errno.ELOOP)
        self.assertIsNone(workspace._in_the_way(workspace.memory_index(self.ws), self.wd))

    def test_a_memory_linked_to_a_directory_inside_the_plane_is_still_followed(self):
        memory = workspace.memory_dir(self.ws)
        shared = self.wd / "shared-memory"
        shared.mkdir()
        self.replaced_by_link(memory, shared)
        rc, said = self.reinit()
        self.assertEqual(rc, 0, said)
        self.assertTrue((shared / "MEMORY.md").is_file(), "the index was not written through")
        self.assertFalse(any("cannot be created" in s for s in said), said)

    def test_a_workspace_directory_linked_out_of_the_plane_gets_no_baseline_written_into_it(self):
        """The workspace directory is a directory of the layout too, and `workspaces/<ws>` is as
        committable a name as any below it. The harness layer is not this check's, so only the
        baseline paths are asserted."""
        wd = self.wd
        self.replaced_by_link(wd, self.outside)
        rc, said = self.reinit()
        self.assertEqual(rc, 0, said)
        for rel in ("workspace.md", "workspace.json", "memory", "refs"):
            self.assertFalse(os.path.lexists(self.outside / rel), f"{rel} written out of the plane")
        for rel in ("workspace.md", "workspace.json", "memory/MEMORY.md", "refs/README.md"):
            self.assertIn(self.a_link_row(rel, wd, real="directory"), said)


    def test_a_workspace_directory_linked_elsewhere_in_the_plane_is_named_as_scaffold_refuses_it(self):
        """One rule for the workspace directory, not two: `scaffold` and the harness layer refuse
        one that does not resolve inside `workspaces/` (#1062), so `reinit` must name it by that
        test too. A directory under `personas/` is plane data, which `contain` would follow, and
        "added workspace.md" over a scaffold that wrote nothing is the report ADR 0013 forbids."""
        elsewhere = config.PERSONAS_DIR / "not-a-workspace"
        elsewhere.mkdir(parents=True)
        self.replaced_by_link(self.wd, elsewhere)
        rc, said = self.reinit()
        self.assertEqual(rc, 0, said)
        self.assertEqual(sorted(p.name for p in elsewhere.iterdir()), [])
        self.assertIn(self.a_link_row("workspace.md", self.wd, real="directory"), said)
        self.assertFalse(any("added workspace.md" in s for s in said), said)

class APlaneReachedThroughALinkIsNotInItsOwnWay(_isolation.PersonaIso):
    """Only the workspace directory and what is beneath it are names a commit to the plane can
    plant. The plane root, and anything above it, may be a link for reasons of the operator's
    own — and on macOS every temp path runs through ``/var``, which is one, so this held there
    for free and would not have on a Linux runner unless a case built the link."""

    def test_a_workspace_under_a_linked_plane_root_gets_its_baseline(self):
        real = self.tmp / "real-plane"
        real.mkdir()
        (real / "charter.toml").write_text("schema = 1\n")
        link = self.tmp / "link-plane"
        link.symlink_to("real-plane")
        prev = config.use(link)
        self.addCleanup(config.restore, prev)
        workspace.ensure("alpha")
        self.assertNotEqual(workspace.workspace_dir("alpha"),
                            workspace.workspace_dir("alpha").resolve(),
                            "fixture normalised for free — this case would assert nothing")
        self.assertEqual(workspace.structure_status("alpha")["in_the_way"], [])
        self.assertTrue((workspace.refs_dir("alpha") / "README.md").is_file())
        self.assertTrue(workspace.memory_index("alpha").is_file())

class TheCreateItselfNeverFollowsALink(AWorkspaceWithABaseline):
    """The check above is `lstat` then write, and a link planted between the two is followed by
    a plain `open(…, "w")`. Each baseline create is exclusive (``O_CREAT | O_EXCL``), which POSIX
    requires to fail at a symlink "regardless of the contents of the symbolic link", so the final
    component is closed by the kernel as well — measured here by standing the check down, which is
    what losing that race amounts to. Every target is inside the plane's data, where `contain`
    follows a link and so is no second line.

    Measured on macOS 26 / Python 3.14: a plain `open(link, "w")` created a dangling link's
    target; `open(link, "x")` and `os.open(link, O_CREAT | O_EXCL | O_NOFOLLOW)` both raised
    EEXIST and created nothing. A DIRECTORY link planted in the window is still followed: the
    flag judges the last component only, and a directory link into the plane is one charter
    follows on purpose."""

    def raced(self, p: Path) -> Path:
        target = self.wd / f"raced-{p.name}"
        self.replaced_by_link(p, target)
        with mock.patch.object(workspace, "_in_the_way", return_value=None):
            workspace.scaffold(self.ws)
        return target

    def test_the_refs_readme_is_not_created_through_a_link_that_won_the_race(self):
        target = self.raced(workspace.refs_dir(self.ws) / "README.md")
        self.assertFalse(os.path.lexists(target), "the README was created through the link")

    def test_the_workspace_charter_is_not_created_through_a_link_that_won_the_race(self):
        target = self.raced(workspace.charter_file(self.ws))
        self.assertFalse(os.path.lexists(target), "workspace.md was created through the link")

    def test_the_memory_index_is_not_created_through_a_link_that_won_the_race(self):
        target = self.raced(workspace.memory_index(self.ws))
        self.assertFalse(os.path.lexists(target), "MEMORY.md was created through the link")

    def test_the_manifest_is_not_created_through_a_link_that_won_the_race(self):
        """No flag needed, and measured rather than assumed: the manifest is published with
        `os.replace`, which renames over the link itself and never opens what it names."""
        target = self.raced(workspace.manifest_path(self.ws))
        self.assertFalse(os.path.lexists(target), "workspace.json was created through the link")

    def test_a_create_that_finds_the_file_already_there_leaves_it_alone(self):
        """The exclusive create's other answer. Two launches scaffolding one workspace at once is
        ordinary (#893), and the loser must neither raise nor write over the winner's file."""
        readme = workspace.refs_dir(self.ws) / "README.md"
        readme.unlink()
        real_exists = workspace._exists

        def arrives_meanwhile(p, follow=False):
            answer = real_exists(p, follow)
            if Path(p) == readme and answer is False:
                readme.write_text("the other launch's\n")
            return answer

        with mock.patch.object(workspace, "_exists", side_effect=arrives_meanwhile):
            workspace.scaffold(self.ws)
        self.assertEqual(readme.read_text(), "the other launch's\n")


class TheStructureStampNeverFollowsALink(AWorkspaceWithABaseline):
    """`.charter-structure` is a file of the layout like the four baseline files, so it gets
    their rule rather than a second one: never written through a link, wherever it points.
    #1062 stamped it past `contain.write_refusal`, which refuses a link out of the plane and
    follows one that stays inside — so a dangling link to a name in the plane still had its
    target created. The open itself refuses now (``O_NOFOLLOW | O_NONBLOCK``)."""

    def test_a_dangling_marker_link_inside_the_plane_does_not_create_its_target(self):
        marker = self.wd / ".charter-structure"
        target = self.wd / "raced-structure"
        self.replaced_by_link(marker, target)
        workspace.scaffold(self.ws)
        self.assertFalse(os.path.lexists(target), "the stamp created the file the link names")
        self.assertEqual(os.readlink(marker), str(target), "charter touched the link")

    def test_a_marker_linked_to_a_file_inside_the_plane_is_not_overwritten(self):
        marker = self.wd / ".charter-structure"
        target = self.wd / "somebodys-file"
        target.write_text("kept\n")
        self.replaced_by_link(marker, target)
        workspace.scaffold(self.ws)
        self.assertEqual(target.read_text(), "kept\n", "the stamp wrote through the link")

    def test_the_stamp_is_created_readable_and_not_executable_under_the_umask(self):
        """The stamp is created by `os.open`, so its mode is the one spelled there: ``0o666`` less
        the umask, as `write_text` created it. Pinned under two fixed umasks, so a mode that is
        executable fails the first and one narrower than `write_text`'s — which would decide the
        operator's umask for them — fails the second."""
        marker = self.wd / ".charter-structure"
        marker.unlink()
        old = os.umask(0o022)
        try:
            workspace.scaffold(self.ws)
        finally:
            os.umask(old)
        self.assertEqual(stat.S_IMODE(os.lstat(marker).st_mode), 0o644)
        self.assertEqual(marker.read_text(), f"{workspace.STRUCTURE_VERSION}\n")
        old = os.umask(0o000)
        try:
            marker.unlink()
            workspace.scaffold(self.ws)
        finally:
            os.umask(old)
        self.assertEqual(stat.S_IMODE(os.lstat(marker).st_mode), 0o666)

    def test_a_marker_that_is_a_fifo_does_not_hang_the_scaffold(self):
        """`scaffold` runs from `ensure` on a launch. `write_refusal` refused a FIFO by its mode;
        the open answers ENXIO for one nobody reads, and `scaffold` reads no marker before it —
        the baseline classification it asks is apart from `structure_status`'s marker read."""
        marker = self.wd / ".charter-structure"
        marker.unlink()
        os.mkfifo(marker)
        done = threading.Event()

        def run():
            workspace.scaffold(self.ws)
            done.set()

        threading.Thread(target=run, daemon=True).start()
        finished = done.wait(5)
        if not finished:
            # Release the blocked open so the thread does not outlive the case.
            os.close(os.open(marker, os.O_RDONLY | os.O_NONBLOCK))
        self.assertTrue(finished, "scaffold blocked on a FIFO at .charter-structure")
        self.assertTrue(stat.S_ISFIFO(os.lstat(marker).st_mode))

class AnExclusiveCreateIsExclusiveOnBothBranches(_isolation.PersonaIso):
    """`config.create_for` is `write_for`'s dispatch with ``O_EXCL``, and the dispatch has two
    branches: a plain `open` for a committed path, which honours ``"x"`` by itself, and
    `_private_fd` for charter's own state, which built its flags by hand and would have opened
    ``"x"`` as a create-or-truncate that follows a link."""

    def setUp(self) -> None:
        super().setUp()
        self.d = config.STATE_DIR / "creates"
        config.mkdir_for(self.d)
        self.assertTrue(config.under_state(self.d), "fixture: not the private branch")

    def test_a_state_file_is_not_created_through_a_link(self):
        link, target = self.d / "record.json", self.d / "planted.json"
        link.symlink_to(target)
        self.assertFalse(config.create_for(link, "{}\n"))
        self.assertFalse(os.path.lexists(target), "the private branch followed the link")

    def test_a_state_file_that_is_there_is_not_truncated(self):
        p = self.d / "record.json"
        p.write_text("kept\n")
        self.assertFalse(config.create_for(p, "{}\n"))
        self.assertEqual(p.read_text(), "kept\n")

    def test_a_state_file_it_creates_is_private_and_whole(self):
        p = self.d / "record.json"
        self.assertTrue(config.create_for(p, b"{}\n"))
        self.assertEqual(p.read_bytes(), b"{}\n")
        self.assertEqual(stat.S_IMODE(p.stat().st_mode) & 0o077, 0)
