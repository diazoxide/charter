"""opencode's wiring is one installed artifact, not a file in every repo.

The per-tree design existed for one reason: opencode does not search parent directories
for *project* plugins, so a shim at the plane root was inert in the clone where the session
actually starts. It does read `~/.config/opencode/plugin/` — checked by putting a probe
there and booting from an unrelated directory — which was never established before building
the alternative.

What that assumption cost: a generated file written into every clone and worktree, a
`.git/info/exclude` entry per checkout to keep them out of `git status`, a `doctor` row for
trees missing them, per-tree staleness detection, and a backfill in `reinit`. All of it
answers a question that does not need asking.

Same shape as Codex now: one artifact per harness, installed once.
"""

from __future__ import annotations

import os
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from charter.harness import opencode, registry
from tests import _envguard


class WhereItInstalls(unittest.TestCase):
    def setUp(self) -> None:
        # Outside a frame, with no session id and no pinned workspace: stated here
        # rather than inherited from the shell the suite was launched from
        # (#519, #521, #528).
        _envguard.unset_all()

        self.home = Path(tempfile.mkdtemp(prefix="charter-ocglobal-"))
        self.addCleanup(lambda: shutil.rmtree(self.home, True))
        self.env = mock.patch.dict(os.environ, {"XDG_CONFIG_HOME": str(self.home)})
        self.env.start()
        self.addCleanup(self.env.stop)

    def test_the_plugin_goes_where_opencode_reads_it_for_every_project(self):
        self.assertEqual(opencode.global_dir(), self.home / "opencode")

    def test_xdg_is_honoured_before_the_default(self):
        """A machine that moves its config dir must not get charter's plugin written to a
        path opencode never reads — the same reason `CODEX_HOME` is honoured."""
        with mock.patch.dict(os.environ, {}, clear=True):
            self.assertEqual(opencode.global_dir(),
                             Path.home() / ".config" / "opencode")

    def test_installing_writes_the_plugin_and_the_command(self):
        registry.get("opencode").wire(Path("/unused-plane-root"))
        self.assertTrue((opencode.global_dir() / "plugin" / "charter.ts").is_file())
        self.assertTrue((opencode.global_dir() / "command" / "charter.md").is_file())

    def test_nothing_is_written_into_the_plane(self):
        """The whole point. A plane is somebody's repo, and charter's own housekeeping has
        no business appearing in its `git status`."""
        plane = Path(tempfile.mkdtemp(prefix="charter-plane-"))
        self.addCleanup(lambda: shutil.rmtree(plane, True))
        registry.get("opencode").wire(plane)
        self.assertEqual(list(plane.iterdir()), [])


class NothingIsWiredPerTreeAnyMore(unittest.TestCase):
    def test_the_per_tree_hooks_are_gone(self):
        """Left behind, these would keep writing into every clone while the global plugin
        did the real work — two mechanisms for one job, and the quiet one wins arguments."""
        for gone in ("wire_tree", "refresh_tree", "wire_tree_missing", "wire_tree_stale"):
            with self.subTest(member=gone):
                self.assertFalse(hasattr(registry.get("opencode"), gone),
                                 f"{gone} survived the migration")

    def test_commands_no_longer_wires_trees(self):
        from charter import commands

        for gone in ("wire_work_tree", "refresh_work_tree", "_backfill_work_trees"):
            with self.subTest(member=gone):
                self.assertFalse(hasattr(commands, gone))

    def test_doctor_no_longer_checks_trees(self):
        from charter import doctor

        self.assertFalse(hasattr(doctor, "check_harness_trees"))


if __name__ == "__main__":
    unittest.main()
