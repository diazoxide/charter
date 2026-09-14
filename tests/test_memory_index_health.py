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

import contextlib
import errno
import os
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from charter import config, doctor, memstore, persona
from tests import _envguard
from tests._isolation import PersonaIso, pin_update_channel
from tests.test_a_workspace_listing_names_what_it_cannot_look_at import (
    BOTH_INTERPRETERS, unsearchable)


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

    def test_ok_result_states_how_many_bases_were_checked(self):
        """'ok' with no detail would hide a check that examined nothing."""
        r = doctor.check_memory_indexes()
        if r.status == doctor.OK and "not checked" not in (r.detail or ""):
            self.assertRegex(r.detail or "", r"\d+ base\(s\)")


class DoctorRunsTheCheck(PersonaIso):
    """On a throwaway plane: `run_all` reaches `charter.local.toml` through the `harness
    profiles` row, and the real plane's file is the operator's own — `tests/_planeguard`
    refuses the read."""

    def test_check_is_wired_into_run_all(self):
        self.assertIn("memory indexes", [r.name for r in doctor.run_all()])


class OneBaseItCannotRead(PersonaIso):
    """A base inside a workspace charter cannot search is named, and every other base is still
    checked (#1014, ADR 0009).

    `Path.exists` gave the row two different wrong answers for that base. On 3.11–3.13 it
    raised, and #1012 turned that into `not checked` for the whole row, so every persona and
    workspace base went unchecked with it. On 3.14 it answers False, the base was skipped as
    absent, and the row counted it among the bases it called consistent."""

    def setUp(self) -> None:
        super().setUp()
        self.alpha = config.WORKSPACES_DIR / "alpha"
        (self.alpha / "memory").mkdir(parents=True)
        self.beta = config.WORKSPACES_DIR / "beta" / "memory"
        self.beta.mkdir(parents=True)

    def unsearchable(self, exists_answers):
        """`alpha` refusing every path inside it, the way a directory at mode 000 does — `stat`
        of `alpha` itself still answers — with `Path.exists` answering for those paths the way
        one interpreter does: *exists_answers* is `"raises"` (3.11–3.13) or `False` (3.14). Both
        are stated rather than left to whichever interpreter runs the suite."""
        alpha, real_stat, real_lstat, real_exists = self.alpha, os.stat, os.lstat, Path.exists

        def inside(p) -> bool:
            return isinstance(p, (str, os.PathLike)) and alpha in Path(p).parents

        def refused(p):
            return PermissionError(errno.EACCES, os.strerror(errno.EACCES), str(p))

        def stat(p, *args, **kwargs):
            if inside(p):
                raise refused(p)
            return real_stat(p, *args, **kwargs)

        def lstat(p, *args, **kwargs):
            if inside(p):
                raise refused(p)
            return real_lstat(p, *args, **kwargs)

        def exists(self, *args, **kwargs):
            if inside(self):
                if exists_answers == "raises":
                    raise refused(self)
                return exists_answers
            return real_exists(self, *args, **kwargs)

        stack = contextlib.ExitStack()
        stack.enter_context(mock.patch.object(os, "stat", stat))
        stack.enter_context(mock.patch.object(os, "lstat", lstat))
        stack.enter_context(mock.patch.object(Path, "exists", exists))
        return stack

    def test_with_every_base_readable_nothing_is_named(self):
        """The other side of the line: the clause is for a base that was not read, and a row
        that grew it over a plane with none would be yellow on every plane."""
        r = doctor.check_memory_indexes()
        self.assertEqual((r.status, r.detail, r.hint), (doctor.OK, "3 base(s) consistent", ""))

    def test_it_is_named_on_either_interpreter_and_never_counted_consistent(self):
        """The 3.14 half was a green row over a base nobody read."""
        for answers in ("raises", False):
            with self.subTest(exists=answers):
                with self.unsearchable(answers):
                    r = doctor.check_memory_indexes()
                self.assertEqual(r.status, doctor.WARN)
                self.assertEqual(r.detail,
                                 "2 base(s) consistent; workspaces/alpha/memory cannot be checked")
                self.assertEqual(r.hint, "workspaces/alpha/memory cannot be checked — restoring "
                                         "read access to it clears this.")

    def test_the_bases_it_can_read_still_report_their_drift(self):
        """Not checking one base is no reason to stop reporting what another says."""
        (self.beta / "orphan.md").write_text("# orphan\n\nx\n")
        (self.beta / "MEMORY.md").write_text("# Memory Index\n\n")
        for answers in ("raises", False):
            with self.subTest(exists=answers):
                with self.unsearchable(answers):
                    r = doctor.check_memory_indexes()
                self.assertEqual(r.status, doctor.WARN)
                self.assertEqual(r.detail, "0 dangling, 1 unindexed; "
                                           "workspaces/alpha/memory cannot be checked")
                self.assertTrue(r.hint.startswith("ws:beta (0 dangling, 1 unindexed)"), r.hint)
                self.assertTrue(r.hint.endswith("workspaces/alpha/memory cannot be checked — "
                                                "restoring read access to it clears this."), r.hint)

    def test_a_base_that_is_a_symlink_loop_is_told_to_fix_the_loop(self):
        """`Path.exists` answers False for a loop on every interpreter, so this base was skipped
        as absent and counted consistent too. No permission bit clears a loop."""
        (self.alpha / "memory").rmdir()
        loop = self.alpha / "memory"
        loop.symlink_to(loop)
        r = doctor.check_memory_indexes()
        self.assertEqual(r.status, doctor.WARN)
        self.assertEqual(r.detail,
                         "2 base(s) consistent; workspaces/alpha/memory cannot be checked")
        self.assertEqual(r.hint, f"workspaces/alpha/memory cannot be checked — fix the symlink "
                                 f"loop at {loop}.")

    @unittest.skipIf(os.geteuid() == 0, "root reads a directory whatever its mode")
    def test_a_real_workspace_at_mode_000_beside_a_readable_one(self):
        """The state #1014 was measured in, on whichever interpreter runs this."""
        self.alpha.chmod(0o000)
        self.addCleanup(self.alpha.chmod, 0o755)
        r = doctor.check_memory_indexes()
        self.assertEqual(r.status, doctor.WARN)
        self.assertEqual(r.detail,
                         "2 base(s) consistent; workspaces/alpha/memory cannot be checked")
        self.assertEqual(r.hint, "workspaces/alpha/memory cannot be checked — restoring read "
                                 "access to it clears this.")

    def test_a_memory_link_to_nothing_outside_the_plane_is_an_index_it_will_not_touch(self):
        """#1014 pinned this base as skipped, so that changing it would be a decision; #1043 is
        that decision. `stat` answers "not there" through the link, and the row took that for an
        absent base, so `index_refusal` — whose docstring names this exact case, "a dangling link
        that escapes, which is absent and hostile at the same time" — was never asked. A link is
        something there, so the base is read through to the refusal."""
        (self.alpha / "memory").rmdir()
        outside = Path(tempfile.mkdtemp(prefix="edm-outside-"))
        self.addCleanup(shutil.rmtree, outside, ignore_errors=True)
        (self.alpha / "memory").symlink_to(outside / "nothing")
        r = doctor.check_memory_indexes()
        self.assertEqual((r.status, r.detail), (doctor.WARN, "1 index(es) charter will not touch"))
        self.assertTrue(r.hint.startswith(f"ws:alpha: '{self.alpha / 'memory'}' resolves to "),
                        r.hint)
        self.assertIn("outside the directories a control plane keeps its data in", r.hint)

    def test_a_memory_link_to_nothing_inside_the_plane_is_an_absent_base(self):
        """The other side of that line: a link to nothing that stays in the plane is refused by
        nothing, and a base that is not there has nothing to disagree with."""
        (self.alpha / "memory").rmdir()
        (self.alpha / "memory").symlink_to(self.alpha / "nothing")
        r = doctor.check_memory_indexes()
        self.assertEqual((r.status, r.detail, r.hint), (doctor.OK, "3 base(s) consistent", ""))

    def refusing_to_list(self, directory: Path):
        """*directory* refusing to be LISTED, at both calls `Path.iterdir` and `Path.glob` make on
        3.11–3.14, while `stat` of it and of the paths in it still answers — mode 333's shape."""
        real_listdir, real_scandir = os.listdir, os.scandir

        def refusing(real):
            def call(p=".", *args, **kwargs):
                if isinstance(p, (str, os.PathLike)) and Path(p) == directory:
                    raise PermissionError(errno.EACCES, os.strerror(errno.EACCES), str(p))
                return real(p, *args, **kwargs)
            return call

        stack = contextlib.ExitStack()
        stack.enter_context(mock.patch.object(os, "listdir", refusing(real_listdir)))
        stack.enter_context(mock.patch.object(os, "scandir", refusing(real_scandir)))
        return stack

    def test_a_memory_directory_it_cannot_list_is_named_not_counted_consistent(self):
        """`Path.glob` answers an empty list for a directory it may not read, on every interpreter,
        so `memstore.files` said "no memories" and the row counted a base nobody listed."""
        (self.alpha / "memory" / "kept.md").write_text("# kept\n\nx\n")
        (self.alpha / "memory" / "MEMORY.md").write_text("# Memory Index\n\n- [kept](kept.md)\n")
        with self.refusing_to_list(self.alpha / "memory"):
            r = doctor.check_memory_indexes()
        self.assertEqual(r.status, doctor.WARN)
        self.assertEqual(r.detail,
                         "2 base(s) consistent; workspaces/alpha/memory cannot be checked")
        self.assertEqual(r.hint, "workspaces/alpha/memory cannot be checked — restoring read "
                                 "access to it clears this.")

    @unittest.skipIf(os.geteuid() == 0, "root reads a directory whatever its mode")
    def test_a_real_memory_directory_at_mode_000_or_333(self):
        """000 read as an index charter will not touch, sent to "replace the link" over a
        directory that is no link. 333 — searchable, not readable — read the one indexed memory
        as dangling. Both are a base charter could not list."""
        mem = self.alpha / "memory"
        (mem / "kept.md").write_text("# kept\n\nx\n")
        (mem / "MEMORY.md").write_text("# Memory Index\n\n- [kept](kept.md)\n")
        self.addCleanup(mem.chmod, 0o755)
        for mode in (0o000, 0o333):
            with self.subTest(mode=oct(mode)):
                mem.chmod(mode)
                r = doctor.check_memory_indexes()
                mem.chmod(0o755)
                self.assertEqual(r.status, doctor.WARN)
                self.assertEqual(r.detail,
                                 "2 base(s) consistent; workspaces/alpha/memory cannot be checked")
                self.assertEqual(r.hint, "workspaces/alpha/memory cannot be checked — restoring "
                                         "read access to it clears this.")


class WorkspacesItCannotStat(PersonaIso):
    """A `workspaces/` charter can list and not search — mode 666 — hid every workspace base from
    the row (#1043). `list_workspaces` asked `Path.is_dir`, which raised on 3.11–3.13, so the row
    read `not checked` for every base, and answered False on 3.14, so the row was OK over the
    one base left. Each is named instead, and the bases it can read are still checked."""

    def setUp(self) -> None:
        super().setUp()
        for ws in ("alpha", "beta"):
            (config.WORKSPACES_DIR / ws / "memory").mkdir(parents=True)

    DETAIL = "1 base(s) consistent; workspaces/alpha, workspaces/beta cannot be checked"
    HINT = ("workspaces/alpha cannot be checked — restoring read access to it clears this; "
            "workspaces/beta cannot be checked — restoring read access to it clears this.")

    def test_on_either_interpreter(self):
        for answers in BOTH_INTERPRETERS:
            with self.subTest(is_dir=answers):
                with unsearchable(config.WORKSPACES_DIR, answers):
                    r = doctor.check_memory_indexes()
                self.assertEqual((r.status, r.detail, r.hint), (doctor.WARN, self.DETAIL, self.HINT))

    @unittest.skipIf(os.geteuid() == 0, "root searches a directory whatever its mode")
    def test_a_real_workspaces_directory_at_mode_666(self):
        config.WORKSPACES_DIR.chmod(0o666)
        self.addCleanup(config.WORKSPACES_DIR.chmod, 0o755)
        r = doctor.check_memory_indexes()
        self.assertEqual((r.status, r.detail, r.hint), (doctor.WARN, self.DETAIL, self.HINT))


class WhatTheRowSaysOfTheBasesItRead(PersonaIso):
    """The verdicts the unread clause is added beside, each pinned at the shape it prints."""

    def refused_index(self, ws: str) -> None:
        """A base whose `MEMORY.md` is a link out of the plane — an index charter will not
        write through (#349)."""
        mem = config.WORKSPACES_DIR / ws / "memory"
        mem.mkdir(parents=True)
        outside = Path(tempfile.mkdtemp(prefix="edm-outside-"))
        self.addCleanup(shutil.rmtree, outside, ignore_errors=True)
        (outside / "MEMORY.md").write_text("# Memory Index\n")
        (mem / "MEMORY.md").symlink_to(outside / "MEMORY.md")

    def test_two_refused_indexes_are_both_named_with_nothing_elided(self):
        for ws in ("one", "two"):
            self.refused_index(ws)
        r = doctor.check_memory_indexes()
        self.assertEqual((r.status, r.detail), (doctor.WARN, "2 index(es) charter will not touch"))
        self.assertTrue(r.hint.startswith("ws:one: "), r.hint)
        self.assertIn("; ws:two: ", r.hint)
        self.assertNotIn("…", r.hint)
        self.assertTrue(r.hint.endswith("  → this is a defect in a committed file: replace the "
                                        "link with a real MEMORY.md"), r.hint)

    def test_a_third_refused_index_is_elided(self):
        """Two are shown; the rest are said to exist rather than dropped without a mark."""
        for ws in ("one", "two", "three"):
            self.refused_index(ws)
        r = doctor.check_memory_indexes()
        self.assertEqual((r.status, r.detail), (doctor.WARN, "3 index(es) charter will not touch"))
        self.assertTrue(r.hint.endswith(", …  → this is a defect in a committed file: replace "
                                        "the link with a real MEMORY.md"), r.hint)

    def test_a_plane_whose_only_finding_is_a_large_index(self):
        """Growth with no drift is its own verdict — a count of large indexes, not "0 dangling,
        0 unindexed", which reads as a drift report that found nothing."""
        mem = config.WORKSPACES_DIR / "big" / "memory"
        mem.mkdir(parents=True)
        names = [f"m{i:03}.md" for i in range(doctor._INDEX_LINES_WARN)]
        for n in names:
            (mem / n).write_text(f"# {n[:-3]}\n\nx\n")
        (mem / "MEMORY.md").write_text(
            "# Memory Index\n\n" + "".join(f"- [T]({n})\n" for n in names))
        r = doctor.check_memory_indexes()
        self.assertEqual((r.status, r.detail), (doctor.WARN, "1 large index(es)"))
        self.assertTrue(r.hint.startswith(f"large: ws:big ({doctor._INDEX_LINES_WARN} entries)"),
                        r.hint)



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
