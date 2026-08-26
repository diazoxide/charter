"""The **plane repo** — the forge repo the plane root is a checkout of — is always a
clone target, without `charter discover`.

Not to be confused with the **root tree**, which is the same code seen from the other end:
the local directory you must not work in (docs/adr/0008). One is a remote you clone *from*,
the other a checkout you leave alone.

This exists because removing the embedded plane shape (docs/adr/0007) took away the way a
solo user reached their own code. Before, the plane root *was* the working tree. Now work
happens in a workspace clone, and the only route charter offered to one was an inventory
built by enumerating an entire forge owner — 63 repos queried to surface the one that
mattered, written to a file that is not gitignored. The root's own `origin` already knows
the answer, so `discover` becomes optional rather than a prerequisite.
"""
from __future__ import annotations

import json
import subprocess
import unittest
from pathlib import Path

from charter import config, inventory
from tests._isolation import PersonaIso


def _git(path: Path, *args: str):
    return subprocess.run(["git", "-C", str(path), *args],
                          check=True, capture_output=True, text=True)


class PlaneRepoCase(PersonaIso):
    def make_root_repo(self, origin: str = "https://github.com/acme/api.git") -> None:
        """The plane root as a real git repo with an origin — the ordinary case."""
        subprocess.run(["git", "init", "-q", str(config.ROOT)], check=True,
                       capture_output=True)
        # Set per-repo, not inherited: a CI runner has no global git identity, so a
        # fixture that commits fails there with exit 128 while passing on any developer
        # machine. `tests/test_statusline_worktree_rows.py::init_repo` does the same.
        _git(config.ROOT, "config", "user.email", "t@example.com")
        _git(config.ROOT, "config", "user.name", "t")
        if origin:
            _git(config.ROOT, "remote", "add", "origin", origin)
        (config.ROOT / "charter.toml").write_text(
            'schema = 1\n\n[[forge]]\nkind = "github"\nowner = "acme"\n')
        config.use(config.ROOT)


class TestDerivingThePlaneRepo(PlaneRepoCase):
    def test_it_is_named_from_the_origin_not_the_directory(self):
        """A plane in ~/work/api-checkout whose origin is …/acme/api must produce `api`,
        or a later `charter clone api` lands beside it instead of on it."""
        self.make_root_repo("https://github.com/acme/api.git")
        self.assertEqual(inventory.plane_repo()["name"], "api")

    def test_it_carries_the_owner_path(self):
        self.make_root_repo("https://github.com/acme/api.git")
        self.assertEqual(inventory.plane_repo()["path_with_namespace"], "acme/api")

    def test_it_is_clonable_over_https(self):
        """The exact URL, not merely that it looks https-ish. Asserting the prefix alone
        let `…/api.git.git` through — `_https_url` appends `.git` to a `web_url`, and an
        origin passed through verbatim already ends in one."""
        from charter.commands import _https_url
        self.make_root_repo("https://github.com/acme/api.git")
        self.assertEqual(_https_url(inventory.plane_repo()),
                         "https://github.com/acme/api.git")

    def test_an_origin_without_a_git_suffix_still_yields_one(self):
        from charter.commands import _https_url
        self.make_root_repo("https://github.com/acme/api")
        self.assertEqual(_https_url(inventory.plane_repo()),
                         "https://github.com/acme/api.git")

    def test_an_ssh_origin_still_yields_an_https_clone_url(self):
        """Golden rule: one credential, HTTPS token, never SSH."""
        from charter.commands import _https_url
        self.make_root_repo("git@github.com:acme/api.git")
        self.assertEqual(_https_url(inventory.plane_repo()),
                         "https://github.com/acme/api.git")

    def test_it_is_stamped_with_the_forge_it_belongs_to(self):
        self.make_root_repo("https://github.com/acme/api.git")
        self.assertEqual(inventory.plane_repo()["forge"], "github")

    def test_it_is_marked_as_not_coming_from_discover(self):
        self.make_root_repo()
        self.assertEqual(inventory.plane_repo()["source"], "plane")

    def test_it_carries_the_default_branch(self):
        """Unlike stack, this IS knowable from the root — and `cmd_clone` reads it to
        announce what it is cloning."""
        self.make_root_repo()
        _git(config.ROOT, "symbolic-ref", "refs/remotes/origin/HEAD", "refs/remotes/origin/trunk")
        self.assertEqual(inventory.plane_repo()["default_branch"], "trunk")

    def test_the_default_branch_falls_back_to_the_checked_out_branch(self):
        """A plane `git init`-ed and given a remote by hand never gets origin/HEAD."""
        self.make_root_repo()
        (config.ROOT / "f.txt").write_text("x")
        _git(config.ROOT, "add", "f.txt")
        _git(config.ROOT, "-c", "commit.gpgsign=false", "commit", "-qm", "c")
        self.assertTrue(inventory.plane_repo()["default_branch"])

    def test_it_claims_no_stack_it_cannot_know(self):
        """Faking a stack would put a wrong answer in the STACK column; absent renders
        as `?`, which is true."""
        self.make_root_repo()
        self.assertNotIn("stack", inventory.plane_repo())


class TestWhenThereIsNothingToDerive(PlaneRepoCase):
    def test_a_root_that_is_not_a_git_repo_contributes_nothing(self):
        """`charter init` in an empty directory is the README's own 60-second path."""
        (config.ROOT / "charter.toml").write_text("schema = 1\n")
        self.assertIsNone(inventory.plane_repo())

    def test_a_root_with_no_origin_contributes_nothing(self):
        """A repo with no remote cannot be cloned *from* in any useful sense — a
        workspace clone of it would have no upstream to push to."""
        self.make_root_repo(origin="")
        self.assertIsNone(inventory.plane_repo())

    def test_an_origin_on_an_undeclared_forge_contributes_nothing(self):
        """Guessing the first declared forge would build a plausible, wrong clone URL —
        failing later with a confusing error instead of here with an honest silence."""
        self.make_root_repo("https://svn.example.invalid/acme/api.git")
        self.assertIsNone(inventory.plane_repo())

    def test_it_never_raises(self):
        self.assertIsNone(inventory.plane_repo())


class TestItReachesEveryConsumer(PlaneRepoCase):
    def test_repos_includes_it_with_no_inventory_on_disk(self):
        self.make_root_repo()
        self.assertEqual([r["name"] for r in inventory.repos()], ["api"])

    def test_it_is_findable_by_name(self):
        self.make_root_repo()
        self.assertIsNotNone(inventory.find(inventory.repos(), "api"))

    def test_it_is_findable_by_path(self):
        self.make_root_repo()
        self.assertIsNotNone(inventory.find(inventory.repos(), "acme/api"))

    def test_a_plane_with_nothing_derivable_still_lists_nothing(self):
        (config.ROOT / "charter.toml").write_text("schema = 1\n")
        self.assertEqual(inventory.repos(), [])


class TestAgainstARealInventory(PlaneRepoCase):
    def _write_inventory(self, repos):
        config.INVENTORY.parent.mkdir(parents=True, exist_ok=True)
        config.INVENTORY.write_text(json.dumps(
            {"group": "acme", "count": len(repos), "repos": repos}) + "\n")

    def test_the_discovered_record_wins_a_duplicate(self):
        """It carries stack, kind, description and an id — everything the synthetic one
        cannot know."""
        self.make_root_repo()
        self._write_inventory([{"name": "api", "path_with_namespace": "acme/api",
                                "forge": "github", "stack": "python", "id": 7}])
        found = inventory.find(inventory.repos(), "api")
        self.assertEqual(found.get("stack"), "python")
        self.assertNotIn("source", found)

    def test_a_duplicate_is_not_listed_twice(self):
        self.make_root_repo()
        self._write_inventory([{"name": "api", "path_with_namespace": "acme/api",
                                "forge": "github"}])
        self.assertEqual([r["name"] for r in inventory.repos()], ["api"])

    def test_deduping_is_by_path_not_bare_name(self):
        """Two repos called `api` under different owners are different repos — which is
        what path_with_namespace exists to disambiguate."""
        self.make_root_repo("https://github.com/acme/api.git")
        self._write_inventory([{"name": "api", "path_with_namespace": "other/api",
                                "forge": "github"}])
        self.assertEqual(len(inventory.repos()), 2)

    def test_other_repos_are_untouched(self):
        self.make_root_repo()
        self._write_inventory([{"name": "web", "path_with_namespace": "acme/web",
                                "forge": "github"}])
        self.assertEqual(sorted(r["name"] for r in inventory.repos()), ["api", "web"])


class TestAnExplicitExcludeWins(PlaneRepoCase):
    def test_excluding_the_plane_repo_by_name_removes_it(self):
        """A written instruction about your own plane beats charter being helpful."""
        self.make_root_repo()
        (config.ROOT / "charter.toml").write_text(
            'schema = 1\n\n[[forge]]\nkind = "github"\nowner = "acme"\n'
            'exclude = ["api"]\n')
        config.use(config.ROOT)
        self.assertEqual(inventory.repos(), [])


class TestItNeverReachesDisk(PlaneRepoCase):
    def test_save_strips_the_synthetic_entry(self):
        """inventory/repos.json says "regenerate with `charter discover`; do not
        hand-edit". A synthetic record landing there becomes a permanent fake that no
        discover removes — a hazard currently avoided by accident, pinned here by design.
        """
        self.make_root_repo()
        inventory.save(inventory.repos())
        on_disk = json.loads(config.INVENTORY.read_text())["repos"]
        self.assertEqual(on_disk, [])

    def test_save_keeps_real_records(self):
        self.make_root_repo()
        inventory.save(inventory.repos() + [{"name": "web",
                                             "path_with_namespace": "acme/web",
                                             "forge": "github"}])
        on_disk = json.loads(config.INVENTORY.read_text())["repos"]
        self.assertEqual([r["name"] for r in on_disk], ["web"])


class TestCloneToleratesAThinRecord(PlaneRepoCase):
    """`cmd_clone` read `r['default_branch']` directly, so ANY record without it took the
    whole command down with a KeyError rather than cloning. Found by cloning the plane
    repo — the first record charter ever built by hand rather than from a forge query."""

    def test_a_record_without_a_default_branch_does_not_crash_the_announcement(self):
        from charter import commands
        thin = {"name": "api", "path_with_namespace": "acme/api", "forge": "github",
                "web_url": "https://github.com/acme/api"}
        self.assertIsInstance(commands._clone_announcement(thin), str)

    def test_the_announcement_names_the_branch_when_there_is_one(self):
        from charter import commands
        rec = {"name": "api", "path_with_namespace": "acme/api", "forge": "github",
               "default_branch": "trunk"}
        self.assertIn("trunk", commands._clone_announcement(rec))


if __name__ == "__main__":
    unittest.main()
