"""The embedded shape's consequences beyond the status line: where worktrees live, what
`init` decides, and what happens when one plane ends up inside another.

The worktree relocation is the load-bearing one. `worktree.py` puts worktrees at
``workspaces/<ws>/.worktrees/`` and says why: OUTSIDE every clone, "so nx/jest/maven never
recurse into them". That reasoning assumes the clone and the plane are different
directories. In an embedded plane they are the same one, so the identical path lands a
full second copy of the codebase inside the tree every root-level glob walks — measured in
charter's own checkout at the time this was written, 214 discoverable test files of which
142 were duplicates. ``.gitignore`` hides that from git and from nothing else.
"""
from __future__ import annotations

import io
import os
import re
import subprocess
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

from charter import commands, config, doctor, instance, statusline, worktree
from tests._isolation import PersonaIso
from tests.test_statusline_monorepo import MonorepoIso, git, init_repo


class WorktreesLeaveTheCodebase(MonorepoIso):
    """An embedded plane's worktrees default to a SIBLING of the repo."""

    def test_default_root_is_beside_the_repo_not_inside_it(self):
        root = config.WORKTREES_ROOT
        self.assertIsNotNone(root, "embedded plane kept the in-tree default")
        self.assertEqual(root.name, f"{self.tmp.name}.worktrees")
        with self.assertRaises(ValueError):
            root.relative_to(self.tmp.resolve())

    def test_worktrees_are_created_outside_the_repo(self):
        wt = self.add_worktree("piece-a")
        with self.assertRaises(ValueError):
            wt.resolve().relative_to(self.tmp.resolve())

    def test_nothing_is_added_to_a_root_level_glob_of_the_repo(self):
        """The failure this exists to prevent, stated the way a build tool sees it."""
        (self.tmp / "test_thing.py").write_text("x = 1\n")
        git(self.tmp, "add", "test_thing.py")
        git(self.tmp, "-c", "commit.gpgsign=false", "commit", "-qm", "add test")
        before = list(self.tmp.rglob("test_*.py"))
        self.add_worktree("piece-a")
        self.assertEqual(list(self.tmp.rglob("test_*.py")), before)

    def test_the_status_line_still_finds_them(self):
        """Relocating them must not cost the rows — `dirs_for` follows the same root."""
        self.add_worktree("piece-a")
        self.assertTrue([ln for ln in self.render() if re.search(r"─ piece-a\b", ln)])

    def test_a_declared_path_wins_and_is_relative_to_root(self):
        cfg = {"plane": {"shape": "embedded", "worktrees": "../elsewhere"}}
        self.assertEqual(
            config.worktrees_root_for(self.tmp, "embedded", cfg),
            (self.tmp.parent / "elsewhere").resolve())

    def test_an_absolute_declared_path_is_taken_verbatim(self):
        cfg = {"plane": {"worktrees": str(self.tmp / "here")}}
        self.assertEqual(config.worktrees_root_for(self.tmp, "embedded", cfg),
                         (self.tmp / "here").resolve())


class FleetKeepsTheOriginalLayout(PersonaIso):
    """A fleet plane's worktrees were never inside a clone, so nothing moves."""

    def test_no_relocation_root(self):
        self.assertIsNone(config.worktrees_root_for(self.tmp, "fleet", {}))

    def test_worktrees_stay_under_the_workspace(self):
        config.WORKTREES_ROOT = None
        self.assertEqual(worktree.root("demo"),
                         config.WORKSPACES_DIR / "demo" / worktree.DIR_NAME)

    def test_a_fleet_plane_may_still_declare_a_path(self):
        cfg = {"plane": {"worktrees": "../shared-worktrees"}}
        self.assertEqual(config.worktrees_root_for(self.tmp, "fleet", cfg),
                         (self.tmp.parent / "shared-worktrees").resolve())


class LocatingAWorktree(MonorepoIso):
    """`_current` marks the row you are standing in. For worktrees it never did, in
    either layout: an in-plane worktree sits at `workspaces/<ws>/.worktrees/<repo>/<piece>`
    and the plain workspace arithmetic read its repo as the literal `.worktrees`."""

    def test_locate_resolves_a_relocated_worktree(self):
        wt = self.add_worktree("piece-a")
        self.assertEqual(worktree.locate(wt), ("default", self.tmp.name, "piece-a"))

    def test_locate_resolves_a_path_deep_inside_one(self):
        wt = self.add_worktree("piece-a")
        deep = wt / "charter" / "sub"
        deep.mkdir(parents=True)
        self.assertEqual(worktree.locate(deep), ("default", self.tmp.name, "piece-a"))

    def test_locate_still_handles_the_in_plane_layout(self):
        """Both roots stay live: a plane that has just declared `[plane] worktrees` still
        has yesterday's worktrees under `workspaces/`."""
        legacy = config.WORKSPACES_DIR / "demo" / worktree.DIR_NAME / "svc" / "piece-b"
        legacy.mkdir(parents=True)
        self.assertEqual(worktree.locate(legacy), ("demo", "svc", "piece-b"))

    def test_locate_says_no_for_an_unrelated_path(self):
        self.assertIsNone(worktree.locate(self.tmp))
        self.assertIsNone(worktree.locate(self.tmp.parent))

    def test_the_status_line_marks_the_worktree_you_are_in(self):
        wt = self.add_worktree("piece-a")
        self.assertEqual(statusline._current({"cwd": str(wt)}), ("default", "piece-a"))


class DoctorFlagsStrandedWorktrees(MonorepoIso):
    """Existing worktrees are NOT moved automatically — relocating one means rewriting
    git's own gitdir pointer, and `git worktree move` is what does that correctly."""

    def _check(self):
        with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
            return doctor.check_embedded_worktrees()

    def test_clean_when_they_are_outside(self):
        self.add_worktree("piece-a")
        self.assertEqual(self._check().status, doctor.OK)

    def test_warns_when_one_is_inside_the_codebase(self):
        stranded = config.WORKSPACES_DIR / "default" / worktree.DIR_NAME / "app" / "piece-a"
        stranded.mkdir(parents=True)
        res = self._check()
        self.assertEqual(res.status, doctor.WARN)
        self.assertIn("piece-a", res.detail)

    def test_the_hint_names_the_command_that_actually_works(self):
        stranded = config.WORKSPACES_DIR / "default" / worktree.DIR_NAME / "app" / "piece-a"
        stranded.mkdir(parents=True)
        self.assertIn("git worktree move", self._check().hint)

    def test_silent_on_a_fleet_plane(self):
        """Where that path was never a problem — it is outside every clone by design."""
        self.declare_shape("fleet")
        stranded = config.WORKSPACES_DIR / "default" / worktree.DIR_NAME / "app" / "piece-a"
        stranded.mkdir(parents=True)
        self.assertEqual(self._check().status, doctor.OK)


class InitDecidesTheShape(PersonaIso):
    """`charter init` is the only moment the distinction is decidable — afterwards a fleet
    plane's root is a git repo too, so `.git` stops being evidence."""

    def _init(self, **kw):
        args = type("A", (), {"forge": "github", "owner": "acme", "host": None,
                              "shape": None, **kw})()
        with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
            return commands.cmd_init(args)

    def test_inside_an_existing_repo_writes_embedded(self):
        init_repo(self.tmp)
        self.assertEqual(self._init(), 0)
        self.assertEqual(instance.shape_of(instance.load(self.tmp)), "embedded")

    def test_an_empty_directory_stays_fleet(self):
        self.assertEqual(self._init(), 0)
        self.assertEqual(instance.shape_of(instance.load(self.tmp)), "fleet")

    def test_a_fleet_plane_gets_no_plane_block_at_all(self):
        """The default file stays the minimal one a stranger opens — no key to learn."""
        self._init()
        self.assertNotIn("[plane]", (self.tmp / "charter.toml").read_text())

    def test_the_flag_overrides_detection(self):
        init_repo(self.tmp)
        self.assertEqual(self._init(shape="fleet"), 0)
        self.assertEqual(instance.shape_of(instance.load(self.tmp)), "fleet")

    def test_an_unknown_shape_is_refused_rather_than_guessed(self):
        self.assertEqual(self._init(shape="embeded"), 1)
        self.assertFalse((self.tmp / "charter.toml").exists())

    def test_an_embedded_plane_gets_no_inventory_directory(self):
        """`discover` enumerates clone targets and an embedded plane clones nothing."""
        init_repo(self.tmp)
        self._init()
        self.assertFalse((self.tmp / "inventory").exists())
        self.assertTrue((self.tmp / "personas").is_dir())
        self.assertTrue((self.tmp / "workspaces").is_dir())

    def test_a_fleet_plane_still_gets_one(self):
        self._init()
        self.assertTrue((self.tmp / "inventory").is_dir())

    def test_the_absent_inventory_is_not_reported_as_drift(self):
        """Otherwise doctor reports drift forever and reinit declines to fix it."""
        init_repo(self.tmp)
        self._init()
        self.assertEqual(instance.drift(self.tmp), [])

    def test_reinit_agrees_with_drift_about_an_embedded_plane(self):
        """The detect and heal halves must read the shape from the same place — when
        reinit used the import-time global instead of the file, a plane whose ROOT
        differed from the process's got drift that reinit refused to act on."""
        init_repo(self.tmp)
        self._init()
        config.HAS_CONTROL_PLANE = True
        with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
            commands.cmd_reinit(type("A", (), {})())
        self.assertFalse((self.tmp / "inventory").exists())
        self.assertEqual(instance.drift(self.tmp), [])


class PlaneIdentityFollowsTheMainTree(MonorepoIso):
    """`charter.toml` is a TRACKED file, so an embedded plane checks a copy of it into
    every worktree — and each one then looked like its own control plane.

    Standing in one, charter resolved the plane to the worktree: the status line went
    blank (`root_tree` requires `.git` to be a DIRECTORY and a linked worktree's is a
    file), and personas, the vault and every written memory resolved into a directory
    `git worktree remove` deletes. Worktrees are how you are meant to work in an embedded
    plane, so this was the main path, not an edge.
    """

    def setUp(self) -> None:
        super().setUp()
        from charter import root as _root
        self._root_mod = _root
        # COMMIT charter.toml before branching off. That it is a tracked file is the whole
        # premise: an untracked one is simply absent from the worktree and there is no bug
        # to reproduce. `MonorepoIso` writes it after its initial commit, which is fine for
        # every other fixture and exactly wrong for this one.
        git(self.tmp, "add", "charter.toml")
        git(self.tmp, "-c", "commit.gpgsign=false", "commit", "-qm", "charter.toml")
        self.wt = self.add_worktree("piece-a")

    def test_the_fixture_really_is_a_linked_worktree(self):
        """Everything below is meaningless if `.git` here is a directory."""
        self.assertFalse((self.wt / ".git").is_dir())
        self.assertTrue((self.wt / "charter.toml").is_file(),
                        "charter.toml was not checked out into the worktree")

    def test_the_plane_resolves_to_the_trunk_not_the_worktree(self):
        self.assertEqual(self._root_mod.find_root(self.wt), self.tmp.resolve())

    def test_a_path_deep_inside_the_worktree_resolves_the_same(self):
        deep = self.wt / "a" / "b"
        deep.mkdir(parents=True)
        self.assertEqual(self._root_mod.find_root(deep), self.tmp.resolve())

    def test_the_trunk_still_resolves_to_itself(self):
        self.assertEqual(self._root_mod.find_root(self.tmp), self.tmp.resolve())

    def test_main_worktree_of_reads_the_gitdir_pointer(self):
        self.assertEqual(self._root_mod.main_worktree_of(self.wt), self.tmp.resolve())

    def test_main_worktree_of_says_none_for_a_main_tree(self):
        self.assertIsNone(self._root_mod.main_worktree_of(self.tmp))

    def test_main_worktree_of_says_none_for_a_non_repo(self):
        plain = self.tmp / "not-a-repo"
        plain.mkdir()
        self.assertIsNone(self._root_mod.main_worktree_of(plain))

    def test_a_worktree_whose_trunk_has_no_marker_keeps_its_own(self):
        """Then this is not one plane seen from two directories — the found marker is the
        only evidence there is, and redirecting would point at something that is not a
        plane at all."""
        (self.tmp / "charter.toml").unlink()
        self.assertEqual(self._root_mod.find_root(self.wt), self.wt.resolve())

    def test_an_explicit_charter_root_is_never_redirected(self):
        """The escape hatch for anyone who genuinely wants a per-worktree plane."""
        old = os.environ.get("CHARTER_ROOT")
        os.environ["CHARTER_ROOT"] = str(self.wt)
        try:
            self.assertEqual(self._root_mod.find_root(self.tmp), self.wt.resolve())
        finally:
            if old is None:
                os.environ.pop("CHARTER_ROOT", None)
            else:
                os.environ["CHARTER_ROOT"] = old


class AWorktreeWithoutTheMarkerStillFindsItsPlane(MonorepoIso):
    """The gap `PlaneIdentityFollowsTheMainTree` left open, and the one that actually
    fires in charter's own documented flow.

    That class redirects a worktree that HAS a checked-out `charter.toml` to its main
    tree. But `charter init` writes `charter.toml` and never stages it, so a worktree cut
    from `main` does not contain one — and with no marker anywhere above, the walk found
    nothing to redirect FROM and fell back to the cwd. Following charter's own `enter:`
    line then opened a session whose plane was the worktree: no personas, no vault, and
    memory written into a directory `git worktree remove --force` deletes. `doctor`
    reported it green.

    A worktree branched before the plane was committed has exactly the same shape, so
    committing `charter.toml` fixes today's case and not tomorrow's.
    """

    def setUp(self) -> None:
        super().setUp()
        from charter import root as _root
        self._root_mod = _root
        # charter.toml deliberately NOT committed — that is the whole premise.
        self.wt = self.add_worktree("task1")

    def test_the_fixture_has_no_marker(self):
        self.assertFalse((self.wt / "charter.toml").exists(),
                         "fixture committed the marker; there is no bug to reproduce")

    def test_the_plane_resolves_to_the_repo_the_worktree_was_cut_from(self):
        self.assertEqual(self._root_mod.find_root(self.wt), self.tmp.resolve())

    def test_a_path_deep_inside_it_resolves_too(self):
        deep = self.wt / "src" / "pkg"
        deep.mkdir(parents=True)
        self.assertEqual(self._root_mod.find_root(deep), self.tmp.resolve())

    def test_a_plain_directory_with_no_plane_anywhere_still_raises(self):
        """The fallback must not start inventing planes for ordinary directories."""
        plain = self.tmp.parent / f"{self.tmp.name}-elsewhere"
        plain.mkdir(exist_ok=True)
        self.addCleanup(lambda: plain.rmdir())
        with self.assertRaises(self._root_mod.ControlPlaneNotFound):
            self._root_mod.find_root(plain)

    def test_a_worktree_whose_repo_has_no_plane_still_raises(self):
        """Being in a worktree is not itself evidence of a plane."""
        (self.tmp / "charter.toml").unlink()
        with self.assertRaises(self._root_mod.ControlPlaneNotFound):
            self._root_mod.find_root(self.wt)

    def test_doctor_no_longer_calls_a_missing_plane_healthy(self):
        from charter import config as _config, doctor
        old = _config.HAS_CONTROL_PLANE
        _config.HAS_CONTROL_PLANE = False
        self.addCleanup(lambda: setattr(_config, "HAS_CONTROL_PLANE", old))
        self.assertEqual(doctor.check_control_plane_config().status, doctor.WARN)


class NestedPlanesAreVisible(PersonaIso):
    """`find_root` takes the innermost marker — the git/cargo/npm contract, and correct.
    But the embedded shape puts `charter.toml` into ordinary product repos, and a fleet
    plane clones product repos into its workspaces. So cd-ing into `workspaces/<ws>/<repo>`
    silently swaps personas, vault and memory destination. Nothing said so."""

    def _make_nested(self) -> tuple[Path, Path]:
        outer = self.tmp
        (outer / "charter.toml").write_text("schema = 1\n")
        inner = outer / "workspaces" / "default" / "product"
        inner.mkdir(parents=True)
        (inner / "charter.toml").write_text('schema = 1\n\n[plane]\nshape = "embedded"\n')
        return outer, inner

    def test_detects_the_outer_plane(self):
        outer, inner = self._make_nested()
        config.ROOT = inner
        self.assertEqual(statusline._nested_under(), outer.resolve())

    def test_a_standalone_plane_reports_nothing(self):
        (self.tmp / "charter.toml").write_text("schema = 1\n")
        self.assertIsNone(statusline._nested_under())

    def test_a_plain_parent_directory_is_not_a_trap(self):
        """Only a plane whose WORKSPACES holds us swaps anything. A plane that merely
        sits above us on the filesystem is the ordinary nesting `find_root` is for."""
        outer = self.tmp
        (outer / "charter.toml").write_text("schema = 1\n")
        inner = outer / "sub" / "project"
        inner.mkdir(parents=True)
        (inner / "charter.toml").write_text("schema = 1\n")
        config.ROOT = inner
        self.assertIsNone(statusline._nested_under())

    def test_the_alert_names_both_planes_and_the_fix(self):
        outer, inner = self._make_nested()
        config.ROOT = inner
        alerts = "\n".join(statusline._alerts("default"))
        alerts = re.sub(r"\x1b\[[0-9;]*m", "", alerts)
        self.assertIn("nested plane", alerts)
        self.assertIn(inner.name, alerts)
        self.assertIn(outer.name, alerts)
        self.assertIn("CHARTER_ROOT", alerts)


if __name__ == "__main__":
    unittest.main()
