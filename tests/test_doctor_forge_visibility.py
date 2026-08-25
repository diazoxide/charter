"""FINDING 3 — a guard that silently degrades to LESS coverage is worse than no guard,
because it still LOOKS present. `registry.known_forges` now keeps the hosts it DID
resolve when a sibling `[[forge]]` block is broken (see test_forge_registry.py); this
file pins the other half — that the partial failure becomes VISIBLE, not silent.
`doctor` already has a `charter.toml` check (`check_control_plane_config`); this is
where a broken `[[forge]]` block must surface, distinct from a whole-file parse error.

Also covers the un-actionable hint fix: `check_ssh` used to always say `charter
git-policy --apply` — which deliberately no-ops for a repo whose forge is UNMANAGED
(unrecognised host), a permanently un-actionable warning for exactly that repo.
"""
from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from charter import config, doctor
from tests import _envguard
from tests._isolation import pin_update_channel

_ENV = {"GIT_CONFIG_GLOBAL": "/dev/null", "GIT_CONFIG_SYSTEM": "/dev/null"}


class TestDoctorSurfacesPartialForgeFailure(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="edm-doctor-forge-"))
        self.addCleanup(lambda: shutil.rmtree(self.root, ignore_errors=True))
        self._orig = (config.CONFIG_ERROR, config.HAS_CONTROL_PLANE, config.ROOT)
        self.addCleanup(self._restore)

    def _restore(self):
        config.CONFIG_ERROR, config.HAS_CONTROL_PLANE, config.ROOT = self._orig

    def _set(self, has_control_plane=True):
        config.CONFIG_ERROR = None
        config.HAS_CONTROL_PLANE = has_control_plane
        config.ROOT = self.root

    def test_a_bad_forge_block_next_to_a_good_one_warns_and_names_it(self):
        (self.root / "charter.toml").write_text(
            '[[forge]]\nkind = "gitlab"\nhost = "git.internal"\ngroup = "acme"\n\n'
            '[[forge]]\nkind = "bitbucket-typo"\nhost = "bad.example.com"\nowner = "x"\n')
        self._set()
        result = doctor.check_control_plane_config()
        self.assertEqual(result.status, doctor.WARN)
        self.assertIn("forge", result.detail.lower())
        self.assertIn("bitbucket-typo", result.hint)

    def test_all_good_forge_blocks_still_reports_ok(self):
        (self.root / "charter.toml").write_text(
            '[[forge]]\nkind = "gitlab"\nhost = "git.internal"\ngroup = "acme"\n')
        self._set()
        result = doctor.check_control_plane_config()
        self.assertEqual(result.status, doctor.OK)

    def test_no_charter_toml_at_all_is_a_warning_not_a_green_check(self):
        """Overturns `test_no_charter_toml_at_all_still_reports_ok`, which pinned the
        opposite.

        Every other check reports green against a plane that does not exist — `personas:
        none defined`, `vaults: none configured`, `schema: no control plane found`, all
        OK — because each is truthfully describing nothing. So a session with no personas,
        no vault, and memory being written into a scratch directory rendered as a
        completely healthy plane. This row is the only one positioned to say otherwise,
        and it was saying `ok` too.

        WARN, not FAIL, deliberately: `cmd_doctor` exits 1 only on FAIL, and doctor runs
        from the SessionStart hook — a FAIL here would print a blocker banner in every
        session opened outside a plane, which is a normal thing to do.
        """
        self._set(has_control_plane=False)
        result = doctor.check_control_plane_config()
        self.assertEqual(result.status, doctor.WARN)
        self.assertIn("no control plane", result.detail)
        self.assertTrue(result.hint, "a warning the user cannot act on is worse than none")

    def test_a_healthy_plane_names_which_plane_is_bound(self):
        """Nothing printed WHICH plane was bound, so a stale $CHARTER_ROOT, a nested
        plane and a rootless cwd were indistinguishable from a correct setup."""
        self._set()
        result = doctor.check_control_plane_config()
        self.assertEqual(result.status, doctor.OK)
        self.assertIn(str(config.ROOT), result.detail)

    def test_whole_file_parse_failure_still_wins_as_fail_not_the_new_warn_path(self):
        """A truly malformed charter.toml (CONFIG_ERROR set at import time) must still
        report FAIL with the recorded parse error — the new per-block WARN path is a
        DIFFERENT, narrower failure mode (the file parses; one block's content doesn't)."""
        self._set()
        config.CONFIG_ERROR = "/tmp/x/charter.toml is not valid TOML: boom"
        result = doctor.check_control_plane_config()
        self.assertEqual(result.status, doctor.FAIL)
        self.assertIn("charter.toml", result.detail)


class TestCheckSshHintIsHonest(unittest.TestCase):
    """`charter git-policy --apply` deliberately no-ops for an UNMANAGED-forge repo
    (`gitpolicy.apply` — there is no policy to apply for a host it can't identify). The
    doctor hint must not tell a developer to run a fix that provably does nothing."""

    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="edm-doctor-ssh-root-"))
        self.addCleanup(lambda: shutil.rmtree(self.root, ignore_errors=True))
        self.ws = self.root / "workspaces"
        self._orig = (config.ROOT, config.WORKSPACES_DIR)
        self.addCleanup(self._restore)

    def _restore(self):
        config.ROOT, config.WORKSPACES_DIR = self._orig

    def _repo(self, name: str, origin: str) -> Path:
        d = self.ws / "w" / name
        d.mkdir(parents=True)
        subprocess.run(["git", "init", "-q", str(d)], check=True, capture_output=True,
                       env={**os.environ, **_ENV})
        subprocess.run(["git", "-C", str(d), "remote", "add", "origin", origin],
                       check=True, capture_output=True)
        return d

    def test_only_unmanaged_repos_get_an_actionable_hint_not_apply(self):
        self._repo("unmanaged", "https://bitbucket.example.org/acme/api.git")
        config.ROOT, config.WORKSPACES_DIR = self.root, self.ws
        with mock.patch.object(config, "ROOT", self.root), \
             mock.patch.object(config, "WORKSPACES_DIR", self.ws):
            result = doctor.check_ssh()
        self.assertEqual(result.status, doctor.WARN)
        # the hint must not tell the developer that running --apply alone fixes this —
        # it deliberately no-ops for an unmanaged forge (no policy to apply). It may
        # still MENTION the command while explaining that, but the actionable step must
        # be the real fix: declaring the host.
        self.assertNotIn("Apply the single-credential policy to every clone: "
                         "charter git-policy --apply", result.hint)
        self.assertIn("no-ops", result.hint)
        self.assertIn("[[forge]]", result.hint)
        self.assertIn("charter.toml", result.hint)

    def test_drifted_but_managed_repos_still_get_the_apply_hint(self):
        self._repo("drifted", "https://gitlab.com/acme/api.git")
        with mock.patch.object(config, "ROOT", self.root), \
             mock.patch.object(config, "WORKSPACES_DIR", self.ws):
            result = doctor.check_ssh()
        self.assertEqual(result.status, doctor.WARN)
        self.assertIn("git-policy --apply", result.hint)

    def test_mixed_repos_get_a_hint_that_distinguishes_the_two(self):
        self._repo("drifted", "https://gitlab.com/acme/api.git")
        self._repo("unmanaged", "https://bitbucket.example.org/acme/api.git")
        with mock.patch.object(config, "ROOT", self.root), \
             mock.patch.object(config, "WORKSPACES_DIR", self.ws):
            result = doctor.check_ssh()
        self.assertEqual(result.status, doctor.WARN)
        self.assertIn("git-policy --apply", result.hint)
        self.assertIn("forge", result.hint.lower())


class TestDoctorChecksDeclaredForgesOnly(unittest.TestCase):
    """FINDING I3 (part 1) — `check_forge_cli`/`check_forge_auth` hardcoded
    `GitLabForge()`, so a GitHub-only control plane got a `glab` FAIL ("brew install
    glab") — a real tool, just the wrong one, and misleading for a control plane that
    never touches GitLab at all. `doctor` must check the forges THIS control plane
    actually declares, falling back to the historical single-GitLab default only when
    none are declared (a fresh `charter init`, or a legacy single-forge control plane)."""

    def setUp(self):
        # Outside a frame, with no session id and no pinned workspace: stated here
        # rather than inherited from the shell the suite was launched from
        # (#519, #521, #528).
        _envguard.unset_all()

        self.root = Path(tempfile.mkdtemp(prefix="edm-doctor-forgecli-"))
        self.addCleanup(lambda: shutil.rmtree(self.root, ignore_errors=True))
        self._orig = (config.CONFIG_ERROR, config.HAS_CONTROL_PLANE, config.ROOT)
        self.addCleanup(self._restore)
        # `doctor.run_all()` below reaches `check_plugin_freshness`, which branches on the
        # channel. Three attributes are hand-set here and `UPDATE` is not one of them, so
        # without this the forge assertions ran against the developer's own `[update]`
        # section (#459).
        pin_update_channel(self)

    def _restore(self):
        config.CONFIG_ERROR, config.HAS_CONTROL_PLANE, config.ROOT = self._orig

    def _set(self, has_control_plane=True):
        config.CONFIG_ERROR = None
        config.HAS_CONTROL_PLANE = has_control_plane
        config.ROOT = self.root

    def test_no_charter_toml_falls_back_to_a_single_gitlab_default(self):
        self._set(has_control_plane=False)
        forges = doctor.declared_or_default_forges()
        self.assertEqual([f.kind for f in forges], ["gitlab"])

    def test_a_github_only_control_plane_never_checks_glab(self):
        (self.root / "charter.toml").write_text(
            '[[forge]]\nkind = "github"\nowner = "acme"\n')
        self._set()
        forges = doctor.declared_or_default_forges()
        self.assertEqual([f.kind for f in forges], ["github"])
        results = doctor.run_all()
        names = [r.name for r in results]
        self.assertIn("gh", names)
        self.assertNotIn("glab", names)
        self.assertNotIn("glab auth", names)

    def test_a_github_only_control_plane_never_shows_the_glab_install_hint(self):
        """The exact misleading outcome named in the finding: a GitHub-only user must
        never be told to `brew install glab` at all."""
        (self.root / "charter.toml").write_text(
            '[[forge]]\nkind = "github"\nowner = "acme"\n')
        self._set()
        results = doctor.run_all()
        hints = " ".join(r.hint for r in results if r.hint)
        self.assertNotIn("brew install glab", hints)

    def test_a_gitlab_and_github_control_plane_checks_both_clis(self):
        (self.root / "charter.toml").write_text(
            '[[forge]]\nkind = "gitlab"\ngroup = "acme"\n\n'
            '[[forge]]\nkind = "github"\nowner = "acme"\n')
        self._set()
        results = doctor.run_all()
        names = [r.name for r in results]
        self.assertIn("glab", names)
        self.assertIn("gh", names)


if __name__ == "__main__":
    unittest.main()
