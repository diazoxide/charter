"""`clean on main` is true and reads as "up to date".

`check_plane_root` answers one question — is anyone working in the plane root — and answers
it well. Its clean line says `clean on main`, which is a statement about the working tree.
A reader takes it as a statement about the plane, and the plane root drifts behind `origin`
constantly *because* nobody works in it: every change arrives through a workspace clone and
a PR, and nothing pulls the root.

Observed three times in one session, twice misleading the reader into acting on a stale
checkout — once into re-registering a vault that the newer code would have placed
differently.

Being behind is not a fault and must not warn: it is the normal resting state of a
directory nobody edits. So the status stays OK and only the *detail* gains the count.

The count comes from the already-fetched remote ref, never a network call — `doctor` runs
from the SessionStart hook. That makes it a reading of the last fetch rather than of the
remote, and the wording says so, because a number presented as current when it is cached is
the failure ADR 0013 names.
"""

from __future__ import annotations

import subprocess
import unittest
from pathlib import Path

from charter import config, doctor
from tests._isolation import PersonaIso


#: Pinned on every invocation rather than inherited. `init.defaultBranch` decides what a
#: bare repo's HEAD points at, `git fetch` copies that into `refs/remotes/origin/HEAD`, and
#: that ref is the FIRST thing `_plane_default_branch` trusts. A machine that sets it to
#: `main` and a CI runner that leaves it unset therefore disagree about what this fixture
#: even is — which is how these tests passed locally and failed on `main`.
#: Signing is pinned off for the reason charter has a whole rule about: a commit that stops
#: to ask for a passphrase hangs a suite with nobody there to answer.
_PINS = ["-c", "init.defaultBranch=main", "-c", "commit.gpgsign=false",
         "-c", "tag.gpgsign=false"]


def _git(cwd, *args):
    return subprocess.run(["git", *_PINS, "-C", str(cwd), *args],
                          capture_output=True, text=True, check=False)


class PlaneRootBehindBase(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        root = Path(config.ROOT)
        (root / "charter.toml").write_text("schema = 1\n")
        # Derived when the tmp plane was created, i.e. before the marker above existed.
        config.HAS_CONTROL_PLANE = True
        self.addCleanup(lambda: setattr(config, "HAS_CONTROL_PLANE", False))
        _git(root, "init", "-q", "-b", "main")
        _git(root, "config", "user.email", "t@example.test")
        _git(root, "config", "user.name", "T")
        (root / "seed.txt").write_text("seed\n")
        _git(root, "add", "-A")
        _git(root, "commit", "-qm", "seed")

        # A real upstream, so `@{upstream}` resolves the way it does on a live plane.
        bare = self.tmp.parent / f"{self.tmp.name}-remote.git"
        subprocess.run(["git", *_PINS, "init", "-q", "-b", "main", "--bare", str(bare)],
                       check=False, capture_output=True)
        self.addCleanup(lambda: __import__("shutil").rmtree(bare, ignore_errors=True))
        _git(root, "remote", "add", "origin", str(bare))
        _git(root, "push", "-q", "-u", "origin", "main")
        self.root, self.bare = root, bare

    def _advance_remote(self, n=3):
        """Move origin/main ahead of the root, the way a merged PR does."""
        clone = self.tmp.parent / f"{self.tmp.name}-clone"
        subprocess.run(["git", *_PINS, "clone", "-q", str(self.bare), str(clone)],
                       check=False, capture_output=True)
        self.addCleanup(lambda: __import__("shutil").rmtree(clone, ignore_errors=True))
        _git(clone, "config", "user.email", "t@example.test")
        _git(clone, "config", "user.name", "T")
        for i in range(n):
            (clone / f"f{i}.txt").write_text("x\n")
            _git(clone, "add", "-A")
            _git(clone, "commit", "-qm", f"c{i}")
        _git(clone, "push", "-q")
        _git(self.root, "fetch", "-q", "origin")   # the root now KNOWS, without pulling


class TestTheCleanLineSaysWhetherItIsCurrent(PlaneRootBehindBase):
    def test_up_to_date_says_nothing_extra(self):
        """A count of zero on every clean preflight would be furniture."""
        r = doctor.check_plane_root()
        self.assertEqual(r.status, doctor.OK)
        self.assertNotIn("behind", r.detail)

    def test_behind_is_counted_in_the_detail(self):
        self._advance_remote(3)
        r = doctor.check_plane_root()
        self.assertIn("3 behind", r.detail)

    def test_being_behind_is_not_a_warning(self):
        """It is the normal resting state of a directory nobody works in."""
        self._advance_remote(2)
        self.assertEqual(doctor.check_plane_root().status, doctor.OK)

    def test_it_says_the_number_is_from_the_last_fetch(self):
        """It is read from an already-fetched ref, never a live query — doctor runs from
        the SessionStart hook. Presenting a cached number as current is the ADR 0013
        failure this check exists to avoid repeating."""
        self._advance_remote(1)
        self.assertIn("fetch", doctor.check_plane_root().detail.lower())

    def test_it_still_reports_the_branch(self):
        self._advance_remote(1)
        self.assertIn("main", doctor.check_plane_root().detail)

    def test_no_upstream_is_not_an_error(self):
        """A plane whose root was `git init`-ed by hand has no tracking branch."""
        _git(self.root, "branch", "--unset-upstream")
        r = doctor.check_plane_root()
        self.assertEqual(r.status, doctor.OK)
        self.assertNotIn("behind", r.detail)

    def test_a_dirty_root_still_reports_dirt_first(self):
        """The existing finding is the one that needs acting on; this must not bury it."""
        self._advance_remote(1)
        (self.root / "seed.txt").write_text("edited\n")
        r = doctor.check_plane_root()
        self.assertIn("uncommitted", r.detail)

    def test_dirt_does_not_suppress_the_drift(self):
        """The first version reported drift only on the clean path, so a root that was
        BOTH dirty and behind said nothing about being behind — and a root nobody works in
        is dirty for exactly the reason it is also stale: memory files accumulate while no
        one pulls. Observed: a plane three commits behind, holding the fix for the very
        thing being debugged, reporting only the uncommitted file."""
        self._advance_remote(2)
        (self.root / "seed.txt").write_text("edited\n")
        r = doctor.check_plane_root()
        self.assertIn("uncommitted", r.detail)
        self.assertIn("2 behind", r.detail)


if __name__ == "__main__":
    unittest.main()
