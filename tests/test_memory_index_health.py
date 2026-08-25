"""MEMORY.md must agree with the files beside it — `memstore.index_drift` + `doctor`.

Both failure shapes were found in a real control plane, and neither needed a
concurrency bug. MEMORY.md is append-heavy and edited by many agents and humans
at once:

* **dangling** — a hand-written commit added two index lines but created only one
  of the two files, so `charter recall` could surface a hit nobody can read;
* **unindexed** — a merge conflict on MEMORY.md was resolved by taking one side
  wholesale, dropping the other side's line while its file survived.

`doctor` therefore WARNs rather than FAILs: drift is hygiene, and the check runs
from the SessionStart hook, which must never block a session.
"""

from __future__ import annotations

import unittest

from charter import doctor, memstore, persona
from tests import _envguard
from tests._isolation import PersonaIso, pin_update_channel


class IndexDrift(PersonaIso):
    def setUp(self) -> None:
        # A memory base is a directory inside a plane; since #336 `memstore.files`
        # refuses one that resolves outside the plane's data, so the fixture is a real
        # base rather than a bare temp dir (see the header of `tests/test_memstore.py`).
        super().setUp()
        self.d = persona.memory_dir("indexhealth")
        self.d.mkdir(parents=True, exist_ok=True)

    def _mem(self, name: str, body: str = "a fact") -> Path:
        p = self.d / name
        p.write_text(f"# {name[:-3]}\n\n_2026-08-08 · persistent_\n\n{body}\n")
        return p

    def _index(self, *links: str) -> None:
        body = "# Memory Index\n\n" + "".join(f"- [T]({l})\n" for l in links)
        (self.d / "MEMORY.md").write_text(body)

    def test_clean_base_has_no_drift(self):
        self._mem("a.md"); self._index("a.md")
        self.assertEqual(memstore.index_drift(self.d), {"dangling": [], "unindexed": []})

    def test_dangling_link_is_reported(self):
        self._mem("a.md"); self._index("a.md", "never-written.md")
        self.assertEqual(memstore.index_drift(self.d)["dangling"], ["never-written.md"])

    def test_unindexed_file_is_reported(self):
        self._mem("a.md"); self._mem("b.md"); self._index("a.md")
        self.assertEqual(memstore.index_drift(self.d)["unindexed"], ["b.md"])

    def test_both_at_once(self):
        self._mem("a.md"); self._index("gone.md")
        self.assertEqual(memstore.index_drift(self.d),
                         {"dangling": ["gone.md"], "unindexed": ["a.md"]})

    def test_memory_index_itself_is_never_counted(self):
        self._mem("a.md"); self._index("a.md")
        self.assertNotIn("MEMORY.md", memstore.index_drift(self.d)["unindexed"])

    def test_a_url_ish_title_is_not_mistaken_for_a_link(self):
        """Titles carry API paths; only a bare slug is a filename.

        Without this, `- [GET /v1/x.md](a.md)` would register a phantom link and
        report drift that isn't there.
        """
        self._mem("a.md")
        (self.d / "MEMORY.md").write_text(
            "# Memory Index\n\n- [GET /api/v1/thing.md returns 404](a.md)\n")
        self.assertEqual(memstore.index_drift(self.d), {"dangling": [], "unindexed": []})

    def test_missing_index_reports_every_file_as_unindexed(self):
        self._mem("a.md"); self._mem("b.md")
        self.assertEqual(memstore.index_drift(self.d)["unindexed"], ["a.md", "b.md"])

    def test_absent_directory_is_not_an_error(self):
        self.assertEqual(memstore.index_drift(self.d / "nope"),
                         {"dangling": [], "unindexed": []})


class DoctorCheck(unittest.TestCase):
    """The check must be able to FAIL — one that only ever reports OK is worthless.

    It shipped that way for a moment: a broad `except Exception` swallowed a
    NameError and returned OK, so it silently checked nothing.
    """

    def setUp(self) -> None:
        # Outside a frame, with no session id and no pinned workspace: stated here
        # rather than inherited from the shell the suite was launched from
        # (#519, #521, #528).
        _envguard.unset_all()

        # `run_all` reaches `check_plugin_freshness`, and so the channel (#459).
        pin_update_channel(self)

    def test_check_returns_a_named_result(self):
        r = doctor.check_memory_indexes()
        self.assertEqual(r.name, "memory indexes")

    def test_check_never_fails_a_session(self):
        """Runs from SessionStart — WARN at worst, never FAIL."""
        self.assertIn(doctor.check_memory_indexes().status, (doctor.OK, doctor.WARN))

    def test_check_is_wired_into_run_all(self):
        self.assertIn("memory indexes", [r.name for r in doctor.run_all()])

    def test_ok_result_states_how_many_bases_were_checked(self):
        """'ok' with no detail would hide a check that examined nothing."""
        r = doctor.check_memory_indexes()
        if r.status == doctor.OK and "not checked" not in (r.detail or ""):
            self.assertRegex(r.detail or "", r"\d+ base\(s\)")



class IndexGrowthSignal(PersonaIso):
    """#2: an index only ever appends, and nothing said when it got long.

    Not a truncation guard — charter injects a bounded digest at SessionStart, so a
    long index costs nothing there. This is the nudge toward `persona optimize` that
    otherwise required you to already suspect you needed it.
    """

    def setUp(self) -> None:
        # A memory base is a directory inside a plane; since #336 `memstore.files`
        # refuses one that resolves outside the plane's data, so the fixture is a real
        # base rather than a bare temp dir (see the header of `tests/test_memstore.py`).
        super().setUp()
        self.d = persona.memory_dir("indexhealth")
        self.d.mkdir(parents=True, exist_ok=True)

    def test_index_size_counts_memories_not_index_lines(self):
        """Files are the truth: a base mid-drift must not report a number that
        disagrees with index_drift()."""
        for i in range(3):
            (self.d / f"m{i}.md").write_text(f"# m{i}\n\nx\n")
        (self.d / "MEMORY.md").write_text("# Memory Index\n\n- [a](m0.md)\n")  # 1 of 3 listed
        self.assertEqual(memstore.index_size(self.d), 3)

    def test_index_size_is_zero_for_an_absent_base(self):
        self.assertEqual(memstore.index_size(self.d / "nope"), 0)

    def test_threshold_is_a_nudge_not_a_cap(self):
        """If this ever becomes a hard limit, the docstring above is wrong."""
        self.assertIsInstance(doctor._INDEX_LINES_WARN, int)
        self.assertGreater(doctor._INDEX_LINES_WARN, 0)

if __name__ == "__main__":
    unittest.main()
