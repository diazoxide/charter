"""Where worktrees live: the default, and the escape hatch that overrides it.

Worktrees default to ``workspaces/<ws>/.worktrees/`` — deliberately OUTSIDE every clone,
so that (in `worktree.py`'s own words) "nx/jest/maven never recurse into them". A tree that
answers a root-level glob with several copies of itself is the failure being avoided:
measured in a layout that made that mistake, 214 test files discoverable from one root, 142
of them duplicates.

``$CHARTER_WORKTREES`` and ``[plane] worktrees`` relocate that root. These cases were
salvaged from the embedded-plane suite when that shape was removed (docs/adr/0007) — the
shape is gone, the relocation is not.
"""
from __future__ import annotations

import unittest

from charter import config, worktree
from tests._isolation import PersonaIso


class TestTheDefaultLayout(PersonaIso):
    def test_no_relocation_root_by_default(self):
        self.assertIsNone(config.worktrees_root_for(self.tmp, {}))

    def test_worktrees_stay_under_the_workspace(self):
        config.WORKTREES_ROOT = None
        self.assertEqual(worktree.root("demo"),
                         config.WORKSPACES_DIR / "demo" / worktree.DIR_NAME)


class TestTheDeclaredOverride(PersonaIso):
    def test_a_declared_path_is_relative_to_the_plane_root(self):
        cfg = {"plane": {"worktrees": "../elsewhere"}}
        self.assertEqual(config.worktrees_root_for(self.tmp, cfg),
                         (self.tmp.parent / "elsewhere").resolve())

    def test_an_absolute_declared_path_is_taken_verbatim(self):
        cfg = {"plane": {"worktrees": str(self.tmp / "here")}}
        self.assertEqual(config.worktrees_root_for(self.tmp, cfg),
                         (self.tmp / "here").resolve())

    def test_any_plane_may_declare_one(self):
        """It was introduced for a shape that no longer exists, and kept because the
        hazard can recur in any layout that puts worktrees where a build tool globs."""
        cfg = {"plane": {"worktrees": "../shared-worktrees"}}
        self.assertEqual(config.worktrees_root_for(self.tmp, cfg),
                         (self.tmp.parent / "shared-worktrees").resolve())


if __name__ == "__main__":
    unittest.main()
