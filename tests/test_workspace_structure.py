"""Workspace structure versioning: a marker (.charter-structure) anchors the current layout,
so a workspace from an older umbrella (missing files or an old marker) is detected as stale
and healed idempotently by `charter workspace reinit`. This is the durable upgrade path for any
future structural addition — bump STRUCTURE_VERSION + create it in scaffold()."""

from __future__ import annotations

import unittest
from types import SimpleNamespace

from charter import workspace
from charter import commands_workspace as cw
from tests._isolation import PersonaIso


class StructureCase(PersonaIso):
    def _fresh(self, name):
        workspace.ensure(name)
        workspace.scaffold(name)

    # --- the LIVE "commit the restored files" hint ---
    def test_reinit_does_not_advise_save_when_only_local_files_were_restored(self):
        """refs/ is gitignored for a workspace, so `save` would print "Nothing to save".

        Reported from a real session: reinit healed a LIVE workspace by restoring
        refs/README.md and told the user to run `charter workspace save`, which
        then had nothing to do.
        """
        import io
        from contextlib import redirect_stderr
        self._fresh("w")
        workspace.set_live("w", True)
        (workspace.refs_dir("w") / "README.md").unlink()
        buf = io.StringIO()
        with redirect_stderr(buf):
            cw.cmd_workspace_reinit(SimpleNamespace(name="w", all=False))
        self.assertNotIn("charter workspace save", buf.getvalue())

    def test_reinit_does_advise_save_when_a_shared_file_was_restored(self):
        """workspace.md IS in the LIVE share set — that one is worth committing."""
        import io
        from contextlib import redirect_stderr
        self._fresh("w")
        workspace.set_live("w", True)
        workspace.charter_file("w").unlink()
        buf = io.StringIO()
        with redirect_stderr(buf):
            cw.cmd_workspace_reinit(SimpleNamespace(name="w", all=False))
        self.assertIn("charter workspace save", buf.getvalue())

    # --- pre-rename marker migration ---
    def test_legacy_marker_is_migrated_not_treated_as_v0(self):
        """The marker was renamed .edm-structure -> .charter-structure.

        Reading the new name only would reset every existing workspace to v0:
        an up-to-date workspace reads as stale, gets flagged for reinit, and on
        a LIVE workspace that manufactures a commit. Found in the wild — nine
        workspaces all reported stale after the rename.
        """
        self._fresh("w")
        d = workspace.workspace_dir("w")
        (d / workspace._STRUCTURE_MARKER).rename(d / workspace._LEGACY_STRUCTURE_MARKER)

        self.assertEqual(workspace.structure_version("w"), workspace.STRUCTURE_VERSION)
        self.assertFalse(workspace.needs_reinit("w"))
        self.assertTrue((d / workspace._STRUCTURE_MARKER).exists())
        self.assertFalse((d / workspace._LEGACY_STRUCTURE_MARKER).exists(),
                         "the legacy marker should be renamed, not left as litter")

    def test_legacy_marker_keeps_its_own_version(self):
        """Migrate by rename, never by re-stamping — a genuinely old workspace
        must stay old so reinit still heals it."""
        self._fresh("w")
        d = workspace.workspace_dir("w")
        (d / workspace._STRUCTURE_MARKER).unlink()
        (d / workspace._LEGACY_STRUCTURE_MARKER).write_text("1\n")

        self.assertEqual(workspace.structure_version("w"), 1)
        self.assertTrue(workspace.needs_reinit("w"))

    def test_both_markers_present_keeps_the_new_one_and_drops_the_old(self):
        self._fresh("w")
        d = workspace.workspace_dir("w")
        (d / workspace._LEGACY_STRUCTURE_MARKER).write_text("0\n")

        self.assertEqual(workspace.structure_version("w"), workspace.STRUCTURE_VERSION)
        self.assertFalse((d / workspace._LEGACY_STRUCTURE_MARKER).exists())

    # --- detection ---
    def test_fresh_workspace_is_up_to_date(self):
        self._fresh("w")
        s = workspace.structure_status("w")
        self.assertTrue(s["ok"])
        self.assertEqual(s["missing"], [])
        self.assertEqual(s["version"], workspace.STRUCTURE_VERSION)
        self.assertFalse(workspace.needs_reinit("w"))

    def test_missing_marker_flags_stale(self):
        self._fresh("w")
        workspace._structure_marker("w").unlink()
        self.assertEqual(workspace.structure_version("w"), 0)
        self.assertTrue(workspace.needs_reinit("w"))

    def test_missing_baseline_file_flags_stale(self):
        self._fresh("w")
        workspace.charter_file("w").unlink()
        s = workspace.structure_status("w")
        self.assertIn("workspace.md", s["missing"])
        self.assertFalse(s["ok"])

    def test_version_drift_flags_even_with_all_files(self):
        # The future-feature case: all *current* files exist, but the marker predates a
        # later STRUCTURE_VERSION → still flagged (nothing missing, just an old version).
        self._fresh("w")
        workspace._structure_marker("w").write_text("0\n")
        s = workspace.structure_status("w")
        self.assertEqual(s["missing"], [])
        self.assertFalse(s["ok"])
        self.assertTrue(workspace.needs_reinit("w"))

    def test_nonexistent_workspace_not_flagged(self):
        self.assertFalse(workspace.needs_reinit("ghost"))

    # --- reinit heals ---
    def test_reinit_heals_and_stamps(self):
        self._fresh("w")
        workspace._structure_marker("w").unlink()
        workspace.charter_file("w").unlink()
        workspace.memory_index("w").unlink()
        before = workspace.reinit("w")
        self.assertFalse(before["ok"])
        self.assertIn("workspace.md", before["missing"])
        self.assertIn("memory/MEMORY.md", before["missing"])
        self.assertFalse(workspace.needs_reinit("w"))           # healed
        self.assertEqual(workspace.structure_version("w"), workspace.STRUCTURE_VERSION)

    def test_reinit_is_additive_preserves_content(self):
        self._fresh("w")
        workspace.set_vision("w", "keep me")
        workspace.remember("w", "keep this memory")
        workspace._structure_marker("w").unlink()               # only the marker is stale
        workspace.reinit("w")
        self.assertEqual(workspace.read_vision("w"), "keep me")  # charter untouched
        mem = "\n".join(p.read_text() for p in workspace.memories("w"))
        self.assertIn("keep this memory", mem)                   # memories untouched

    # --- command ---
    def test_reinit_command_single(self):
        self._fresh("w")
        workspace._structure_marker("w").unlink()
        rc = cw.cmd_workspace_reinit(SimpleNamespace(name="w", all=False))
        self.assertEqual(rc, 0)
        self.assertFalse(workspace.needs_reinit("w"))

    def test_reinit_command_all(self):
        for n in ("a", "b"):
            self._fresh(n)
            workspace._structure_marker(n).unlink()
        rc = cw.cmd_workspace_reinit(SimpleNamespace(name=None, all=True))
        self.assertEqual(rc, 0)
        self.assertFalse(workspace.needs_reinit("a"))
        self.assertFalse(workspace.needs_reinit("b"))

    def test_reinit_command_missing_workspace(self):
        self.assertEqual(cw.cmd_workspace_reinit(SimpleNamespace(name="ghost", all=False)), 1)


class UnreadableDirectoriesAnswerNo(PersonaIso):
    """`is_clone` and friends answer "is there a checkout here". For a directory the
    process cannot enter, the honest answer is no — not an exception.

    `pathlib` does not count EACCES among the errors it swallows (only ENOENT, ENOTDIR,
    EBADF and ELOOP), so `(d / ".git").is_dir()` RAISES on Linux and returns False on
    macOS. That divergence took CI red once already: one unreadable directory under
    `workspaces/` took down every caller that scans it, including the status line, whose
    failure mode is a blank footer on every turn.
    """

    def _denying(self, path):
        from pathlib import Path
        real = Path.is_dir

        def strict(self_, *a, **kw):
            if self_ == path or path in self_.parents:
                raise PermissionError(13, "Permission denied", str(self_))
            return real(self_, *a, **kw)

        Path.is_dir = strict
        self.addCleanup(setattr, Path, "is_dir", real)

    def test_is_clone_answers_no_rather_than_raising(self):
        workspace.ensure("alpha")
        workspace.scaffold("alpha")
        d = workspace.workspace_dir("alpha") / "locked"
        d.mkdir(parents=True, exist_ok=True)
        self._denying(d)
        self.assertFalse(workspace.is_clone(d))

    def test_scanning_a_workspace_survives_one_unreadable_directory(self):
        """The case that actually bit: the scan, not the predicate. One directory nobody
        can enter must not make the whole workspace unlistable."""
        workspace.ensure("alpha")
        workspace.scaffold("alpha")
        d = workspace.workspace_dir("alpha") / "locked"
        d.mkdir(parents=True, exist_ok=True)
        self._denying(d)
        self.assertEqual(workspace.clones("alpha"), [])


if __name__ == "__main__":
    unittest.main()
