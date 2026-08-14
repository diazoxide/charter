"""`charter workspace optimize` — curation for the workspace journal (#93), and the
doctor hint that used to point at a command which could not fix it (#92).

The curation engine was already base-agnostic: `curate.report`/`apply_safe` take any
memory directory. Every caller lived in `commands_persona.py`, so the fastest-growing store
in the plane — appended every session, nudged by the memory-cadence hook — had no dedupe,
no index repair and no stale review.

The two issues are one change. Widening the hint without the command would be a lie the
other way round, and the hint is what makes the gap visible in the first place.
"""

from __future__ import annotations

import io
import unittest
from contextlib import redirect_stderr, redirect_stdout
from types import SimpleNamespace

from charter import doctor, memstore, workspace
from charter import commands_workspace as cw
from tests._isolation import PersonaIso


class OptimizeCase(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        workspace.ensure("alpha")
        workspace.scaffold("alpha")
        self.mem = workspace.memory_dir("alpha")

    def write_memory(self, slug: str, body: str, indexed: bool = True) -> None:
        """A memory file, optionally left OUT of MEMORY.md — which is the drift a merge
        resolved by taking one side produces, and what `optimize --apply` repairs."""
        self.mem.mkdir(parents=True, exist_ok=True)
        (self.mem / f"{slug}.md").write_text(
            f"---\nname: {slug}\ndescription: {slug}\nmetadata:\n  type: project\n---\n\n{body}\n")
        idx = memstore.index_path(self.mem)
        if not idx.exists():
            idx.write_text("# Memory\n\n")
        if indexed:
            idx.write_text(idx.read_text() + f"- [{slug}]({slug}.md) — {slug}\n")

    def optimize(self, name="alpha", apply=False, all=False):
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = cw.cmd_workspace_optimize(SimpleNamespace(
                name=None if all else name, all=all, apply=apply, stale_days=90))
        return rc, out.getvalue() + err.getvalue()


class TestItCuratesWorkspaceMemory(OptimizeCase):
    def test_it_reports_a_workspaces_corpus(self):
        self.write_memory("one", "a durable fact about the api")
        rc, out = self.optimize()
        self.assertEqual(rc, 0)
        self.assertIn("alpha", out)

    def test_apply_repairs_an_unindexed_file(self):
        """The drift doctor reports. Before this command there was no way to fix it for a
        workspace base — the memory stayed out of MEMORY.md, and therefore out of the
        SessionStart digest, indefinitely."""
        self.write_memory("orphan", "never linked", indexed=False)
        self.assertEqual(memstore.index_drift(self.mem)["unindexed"], ["orphan.md"])
        self.optimize(apply=True)
        self.assertEqual(memstore.index_drift(self.mem)["unindexed"], [])

    def test_read_only_names_what_apply_would_do(self):
        """A read-only run that silently rewrote the index is how "read-only" stops
        meaning anything — the persona command's own rule, kept here."""
        self.write_memory("orphan", "never linked", indexed=False)
        rc, out = self.optimize(apply=False)
        self.assertEqual(rc, 0)
        self.assertIn("orphan", out)
        self.assertEqual(memstore.index_drift(self.mem)["unindexed"], ["orphan.md"])

    def test_all_covers_every_workspace(self):
        workspace.ensure("beta")
        workspace.scaffold("beta")
        other = workspace.memory_dir("beta")
        other.mkdir(parents=True, exist_ok=True)
        (other / "b.md").write_text("---\nname: b\ndescription: b\nmetadata:\n  type: project\n---\n\nbeta fact\n")
        memstore.index_path(other).write_text("# Memory\n\n")
        self.write_memory("a", "alpha fact")
        rc, out = self.optimize(all=True)
        self.assertEqual(rc, 0)
        self.assertIn("beta", out)

    def test_an_unknown_workspace_is_an_error(self):
        rc, _ = self.optimize(name="ghost")
        self.assertEqual(rc, 1)

    def test_an_empty_corpus_is_not_an_error(self):
        rc, _ = self.optimize()
        self.assertEqual(rc, 0)


class TestTheDoctorHintNamesACommandThatWorks(OptimizeCase):
    """#92: doctor reported `ws:<name> (0 dangling, 7 unindexed)` and then suggested
    `charter persona optimize --all --apply`, whose loop never touches a workspace base.
    It exited successfully having fixed nothing, so the drift read as repaired."""

    def _check(self) -> str:
        r = doctor.check_memory_indexes()
        return f"{r.detail} {r.hint or ''}"

    def test_workspace_drift_points_at_workspace_optimize(self):
        self.write_memory("orphan", "never linked", indexed=False)
        self.assertIn("workspace optimize", self._check())

    def test_workspace_drift_does_not_point_at_persona_optimize(self):
        """The precise failure: a hint that runs cleanly and fixes nothing is worse than no
        hint, because the drift now reads as handled."""
        self.write_memory("orphan", "never linked", indexed=False)
        self.assertNotIn("persona optimize", self._check())

    def test_persona_drift_still_points_at_persona_optimize(self):
        from charter import config, persona
        # The shared base, because `check_memory_indexes` always enumerates it — a dir for
        # a persona that does not exist is never scanned, so it would prove nothing.
        pdir = persona.memory_dir(config.SHARED_PERSONA, shared=True)
        pdir.mkdir(parents=True, exist_ok=True)
        (pdir / "p.md").write_text("---\nname: p\ndescription: p\nmetadata:\n  type: project\n---\n\nx\n")
        memstore.index_path(pdir).write_text("# Memory\n\n")
        self.assertIn("persona optimize", self._check())


if __name__ == "__main__":
    unittest.main()
