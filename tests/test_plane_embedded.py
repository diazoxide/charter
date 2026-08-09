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
