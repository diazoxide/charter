"""Tests for charter.glstate: the status-line's background GitLab refresh.

F2: the refresh used to shell out to ``bin/edm`` — a script that lived in the
old monorepo the engine was extracted from. No control plane ships it, so the
spawn raised FileNotFoundError every time, silently swallowed by a bare
``except Exception``. These tests pin the fixed behavior: the command targets
the installed package via ``-m charter``, and a spawn failure must not start
the retry cooldown (only a successful spawn should).
"""

from __future__ import annotations

import sys
import unittest
from pathlib import Path
from unittest import mock

from charter import glstate

from tests._isolation import PersonaIso


class MaybeSpawnCommandTests(PersonaIso):
    def _dirs(self):
        d = self.tmp / "somerepo"
        d.mkdir(parents=True, exist_ok=True)
        return [d]

    def test_spawns_the_installed_package_not_a_repo_path(self):
        captured = {}

        def fake_popen(cmd, **kwargs):
            captured["cmd"] = cmd
            return mock.MagicMock()

        with mock.patch.object(glstate.subprocess, "Popen", side_effect=fake_popen):
            glstate.maybe_spawn(self._dirs())

        self.assertIn("cmd", captured, "Popen was never called — nothing was stale?")
        cmd = captured["cmd"]
        self.assertEqual(cmd[0], sys.executable)
        # -m charter, not a script path inside the control plane (no bin/edm here).
        # -P (#390): without it, spawned with this render path's own cwd, `-m` would
        # import a `charter/` package sitting under that cwd instead of the installed one.
        self.assertEqual(cmd[1:5], ["-P", "-m", "charter", "gl-refresh"])
        joined = " ".join(str(c) for c in cmd)
        self.assertNotIn("bin/edm", joined)
        self.assertNotIn(str(glstate.config.ROOT), joined)

    def test_workspace_is_appended_when_given(self):
        captured = {}

        def fake_popen(cmd, **kwargs):
            captured["cmd"] = cmd
            return mock.MagicMock()

        with mock.patch.object(glstate.subprocess, "Popen", side_effect=fake_popen):
            glstate.maybe_spawn(self._dirs(), workspace="demo")

        self.assertEqual(captured["cmd"][-2:], ["--workspace", "demo"])

    def test_spawn_failure_does_not_start_the_cooldown(self):
        """A genuine FileNotFoundError/etc must not suppress the next attempt —
        only a spawn that actually launched should arm the cooldown lock."""
        with mock.patch.object(
            glstate.subprocess, "Popen", side_effect=FileNotFoundError("no such file")
        ):
            glstate.maybe_spawn(self._dirs())  # must not raise

        self.assertFalse(glstate._lock_file().exists(), "lock must not be armed on spawn failure")

    def test_successful_spawn_arms_the_cooldown(self):
        with mock.patch.object(glstate.subprocess, "Popen", return_value=mock.MagicMock()):
            glstate.maybe_spawn(self._dirs())

        self.assertTrue(glstate._lock_file().exists(), "lock should be armed once the refresh launched")


class TestForgeAwareState(unittest.TestCase):
    """The status line renders `!42` for a GitLab MR and `#42` for a GitHub PR — each
    audience sees its own forge's notation, never the other's.

    Resolves via `registry.resolve_host`, a host-component matcher that also consults
    the active control plane's own declared forges, so a self-hosted host resolves too —
    `state_for_repo` must go through that resolver.
    """

    def test_open_change_and_ci_come_from_the_repos_forge(self):
        calls = {}

        class FakeForge:
            kind, host, cli, change_sigil = "github", "github.com", "gh", "#"
            def open_change(self, path, branch):
                calls["change"] = (path, branch); return 42
            def ci_status(self, path, branch):
                calls["ci"] = (path, branch); return "success"

        with mock.patch("charter.forge.registry.resolve_host", return_value=FakeForge()), \
             mock.patch("charter.glstate._remote_url", return_value="https://github.com/acme/api.git"), \
             mock.patch("charter.glstate._remote_path", return_value="acme/api"):
            got = glstate.state_for_repo(Path("/tmp/acme-api"), "main")
        self.assertEqual(got["change"], 42)
        self.assertEqual(got["ci"], "success")
        self.assertEqual(got["sigil"], "#")
        self.assertEqual(calls["change"], ("acme/api", "main"))

    def test_an_unknown_host_yields_empty_state_not_a_crash(self):
        with mock.patch("charter.forge.registry.resolve_host", return_value=None), \
             mock.patch("charter.glstate._remote_url", return_value="https://example.com/a/b.git"), \
             mock.patch("charter.glstate._remote_path", return_value="a/b"):
            got = glstate.state_for_repo(Path("/tmp/x"), "main")
        self.assertEqual(got, {"change": None, "ci": None, "sigil": ""})

    def test_a_clone_with_no_remote_yields_empty_state(self):
        with mock.patch("charter.glstate._remote_url", return_value=None), \
             mock.patch("charter.glstate._remote_path", return_value=None):
            got = glstate.state_for_repo(Path("/tmp/x"), "main")
        self.assertEqual(got, {"change": None, "ci": None, "sigil": ""})

    def test_a_forge_call_that_raises_yields_empty_state_not_a_crash(self):
        """Best-effort by contract: an API failure inside the forge (network error,
        missing CLI, ...) must degrade to empty state — the status line must never
        raise no matter what the forge implementation does."""
        class BoomForge:
            kind, host, cli, change_sigil = "gitlab", "gitlab.com", "glab", "!"
            def open_change(self, path, branch):
                raise RuntimeError("glab: not authenticated")
            def ci_status(self, path, branch):
                return None

        with mock.patch("charter.forge.registry.resolve_host", return_value=BoomForge()), \
             mock.patch("charter.glstate._remote_url", return_value="https://gitlab.com/acme/api.git"), \
             mock.patch("charter.glstate._remote_path", return_value="acme/api"):
            got = glstate.state_for_repo(Path("/tmp/acme-api"), "main")
        self.assertEqual(got, {"change": None, "ci": None, "sigil": ""})


class TestReadForBackwardCompat(PersonaIso):
    """`read_for` must read a cache entry written by an OLDER charter (the pre-forge-
    protocol shape: `mr` instead of `change`, no `sigil` at all) without raising — a
    stale on-disk cache degrades to the `!` display default, never a KeyError."""

    def _write_cache(self, entry: dict) -> Path:
        d = self.tmp / "old-repo"
        d.mkdir(parents=True, exist_ok=True)
        cache_file = glstate._cache_file()
        cache_file.parent.mkdir(parents=True, exist_ok=True)
        import json, time
        cache_file.write_text(json.dumps({str(d): {"ts": time.time(), **entry}}))
        return d

    def test_old_shape_entry_with_mr_key_still_renders(self):
        d = self._write_cache({"branch": "main", "mr": 7, "ci": "success"})
        got = glstate.read_for([d], {d: "main"})
        self.assertEqual(got[d]["change"], 7)
        self.assertEqual(got[d]["ci"], "success")
        self.assertEqual(got[d]["sigil"], "")  # render path defaults this to "!"

    def test_new_shape_entry_is_read_as_is(self):
        d = self._write_cache({"branch": "main", "change": 9, "ci": "running", "sigil": "#"})
        got = glstate.read_for([d], {d: "main"})
        self.assertEqual(got[d], {"change": 9, "ci": "running", "sigil": "#"})


if __name__ == "__main__":
    unittest.main()
