"""`doctor` sees clones that are behind, in EVERY workspace (#156).

`doctor` reported everything healthy while a clone in a non-active workspace was seven
commits behind `origin/main`. `sync` defaults to the active workspace, which was empty, so
it reported success having done nothing. Both commands were locally truthful and jointly
misleading, because neither was scoped to the thing that was actually out of date.

A stale clone is not inert: it is what a session reads if it happens to be working in that
workspace, so the cost is silently building against a week-old tree.

**No fetch, no network.** This runs from the SessionStart hook, so behind-ness is read from
remote-tracking refs a previous `sync`/`fetch` already wrote — exactly as `check_plane_root`
reads the root's own drift. The consequence is stated rather than hidden: it can
UNDER-report, never fabricate. That is the acceptable direction, and the same discipline
ADR 0009 applies to error text.
"""

from __future__ import annotations

import subprocess
import unittest
from pathlib import Path

from charter import config, doctor, workspace
from tests._isolation import PersonaIso

OK, WARN = doctor.OK, doctor.WARN


def git(where, *args):
    return subprocess.run(["git", "-C", str(where), *args], check=True,
                          capture_output=True, text=True)


class StaleCloneCase(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        (self.tmp / "charter.toml").write_text("schema = 1\n")
        config.HAS_CONTROL_PLANE = True
        self.origin = self.tmp / "origin.git"
        # `-b main` on the BARE repo, not just the working ones. Without it the bare repo's
        # HEAD comes from the machine's `init.defaultBranch` — `main` on one dev box,
        # `master` on the CI runner — and a clone whose remote HEAD names a branch that was
        # never pushed lands on an unborn branch with NO upstream. Every "behind" assertion
        # then reads as the check being wrong rather than the fixture depending on a global
        # git setting.
        subprocess.run(["git", "init", "-q", "--bare", "-b", "main", str(self.origin)],
                       check=True, capture_output=True)

    def seed_origin(self) -> None:
        seed = self.tmp / "seed"
        subprocess.run(["git", "init", "-q", "-b", "main", str(seed)],
                       check=True, capture_output=True)
        (seed / "f").write_text("1\n")
        git(seed, "add", "-A")
        git(seed, "-c", "commit.gpgsign=false", "-c", "user.email=t@e",
            "-c", "user.name=t", "commit", "-qm", "one")
        git(seed, "remote", "add", "origin", str(self.origin))
        git(seed, "push", "-q", "-u", "origin", "main")
        self.seed = seed

    def clone_into(self, ws: str, name: str = "svc") -> Path:
        workspace.ensure(ws)
        workspace.scaffold(ws)
        dest = workspace.workspace_dir(ws) / name
        subprocess.run(["git", "clone", "-q", str(self.origin), str(dest)],
                       check=True, capture_output=True)
        # The fixture asserts its own precondition. Without this, a clone whose upstream
        # was never wired fails later as "the check says OK", which reads as a bug in the
        # check rather than in the setup — and that is a diagnosis this suite has already
        # paid for once across platforms.
        up = subprocess.run(["git", "-C", str(dest), "rev-parse", "--abbrev-ref", "@{upstream}"],
                            capture_output=True, text=True)
        self.assertEqual(up.returncode, 0,
                         f"fixture: clone has no upstream ({up.stderr.strip()})")
        return dest

    def behind_count(self, clone: Path) -> int:
        r = subprocess.run(["git", "-C", str(clone), "rev-list", "--count", "HEAD..@{upstream}"],
                           capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, f"fixture: rev-list failed ({r.stderr.strip()})")
        return int(r.stdout.strip() or 0)

    def fetch(self, clone: Path, expect_behind: int) -> None:
        """Fetch, then assert the remote-tracking ref actually moved. The check under test
        reads exactly this, so a fixture that silently fetched nothing would make the check
        look wrong."""
        subprocess.run(["git", "-C", str(clone), "fetch", "-q", "origin"],
                       check=True, capture_output=True)
        self.assertEqual(self.behind_count(clone), expect_behind,
                         "fixture: fetch did not move the remote-tracking ref")

    def advance_origin(self, n: int = 2) -> None:
        """Move origin forward, then fetch in the clone WITHOUT merging — which is the real
        shape of the bug: the remote-tracking ref knows, the working tree does not."""
        # A running counter, not the loop index: two separate `advance_origin(1)` calls
        # would otherwise write identical content and git would refuse the empty commit.
        for _ in range(n):
            self._rev = getattr(self, "_rev", 1) + 1
            (self.seed / "f").write_text(f"{self._rev}\n")
            git(self.seed, "add", "-A")
            git(self.seed, "-c", "commit.gpgsign=false", "-c", "user.email=t@e",
                "-c", "user.name=t", "commit", "-qm", f"more {self._rev}")
        git(self.seed, "push", "-q", "origin", "main")

    def check(self):
        return doctor.check_workspace_clones()


class TestItSeesEveryWorkspace(StaleCloneCase):
    def test_a_stale_clone_in_a_NON_active_workspace_is_reported(self):
        """The reported case exactly: the active workspace was empty, so `sync` had nothing
        to do and said so, while the staleness sat one workspace over."""
        self.seed_origin()
        clone = self.clone_into("other")
        self.advance_origin(2)
        self.fetch(clone, 2)
        r = self.check()
        self.assertEqual(r.status, WARN)
        self.assertIn("other", f"{r.detail} {r.hint or ''}")

    def test_the_count_is_reported(self):
        self.seed_origin()
        clone = self.clone_into("other")
        self.advance_origin(3)
        self.fetch(clone, 3)
        r = self.check()
        self.assertIn("3", f"{r.detail} {r.hint or ''}")

    def test_the_hint_names_the_command_that_fixes_every_workspace(self):
        """`charter sync` alone is what produced the false all-clear, so the hint has to be
        the `--all` form or it recreates the bug it reports."""
        self.seed_origin()
        clone = self.clone_into("other")
        self.advance_origin(1)
        self.fetch(clone, 1)
        self.assertIn("sync --all", f"{self.check().hint or ''}")

    def test_several_stale_clones_are_counted_together(self):
        self.seed_origin()
        for ws in ("one", "two"):
            c = self.clone_into(ws)
            self.advance_origin(1)
            self.fetch(c, 1)
        r = self.check()
        self.assertEqual(r.status, WARN)
        for ws in ("one", "two"):
            self.assertIn(ws, f"{r.detail} {r.hint or ''}", ws)


class TestItStaysQuietWhenItShould(StaleCloneCase):
    def test_an_up_to_date_clone_is_ok(self):
        self.seed_origin()
        self.clone_into("other")
        self.assertEqual(self.check().status, OK)

    def test_no_workspaces_is_ok(self):
        self.assertEqual(self.check().status, OK)

    def test_a_clone_with_no_upstream_is_not_stale(self):
        """A branch that tracks nothing cannot be behind anything. Reporting it would put
        the row permanently yellow on any plane with a local-only branch, which costs the
        findings that do matter."""
        d = workspace.workspace_dir("solo")
        workspace.ensure("solo")
        workspace.scaffold("solo")
        repo = d / "svc"
        subprocess.run(["git", "init", "-q", "-b", "main", str(repo)],
                       check=True, capture_output=True)
        (repo / "f").write_text("x\n")
        git(repo, "add", "-A")
        git(repo, "-c", "commit.gpgsign=false", "-c", "user.email=t@e",
            "-c", "user.name=t", "commit", "-qm", "one")
        self.assertEqual(self.check().status, OK)

    def test_a_clone_AHEAD_of_its_upstream_is_not_stale(self):
        """Unpushed work is a different condition with a different remedy, and `sync` would
        not fix it. Reporting it here would send the reader to the wrong command."""
        self.seed_origin()
        clone = self.clone_into("other")
        (clone / "f").write_text("local\n")
        git(clone, "add", "-A")
        git(clone, "-c", "commit.gpgsign=false", "-c", "user.email=t@e",
            "-c", "user.name=t", "commit", "-qm", "mine")
        self.assertEqual(self.check().status, OK)


class TestItNeverReachesTheNetwork(StaleCloneCase):
    def test_it_reports_only_what_a_previous_fetch_recorded(self):
        """The honest limit, pinned. Without a fetch the remote-tracking ref is unchanged,
        so origin moving is invisible here — the check UNDER-reports rather than fabricating,
        and must never fetch to close the gap: this runs in the SessionStart hook."""
        self.seed_origin()
        self.clone_into("other")
        self.advance_origin(4)          # origin moves, nobody fetches
        self.assertEqual(self.check().status, OK)

    def test_it_runs_no_fetch_subprocess(self):
        self.seed_origin()
        clone = self.clone_into("other")
        self.advance_origin(1)
        self.fetch(clone, 1)
        calls = []
        real = subprocess.run

        def spy(cmd, *a, **kw):
            calls.append(cmd)
            return real(cmd, *a, **kw)

        subprocess.run = spy
        self.addCleanup(setattr, subprocess, "run", real)
        self.check()
        for c in calls:
            self.assertNotIn("fetch", c, c)
            self.assertNotIn("ls-remote", c, c)


class TestItIsWiredIn(StaleCloneCase):
    def test_doctor_runs_it(self):
        """A check nothing calls is a check that does not run — and this whole issue is
        `doctor` being green about something it never looked at."""
        names = {r.name for r in doctor.run_all()}
        self.assertIn("workspace clones", names)


if __name__ == "__main__":
    unittest.main()
