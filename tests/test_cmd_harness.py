"""`charter harness` — see which harnesses charter knows, and arm the opt-in one."""

from __future__ import annotations

import io
import os
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands_harness
from tests._isolation import PersonaIso


def _run(fn, args) -> tuple[int, str]:
    out, err = io.StringIO(), io.StringIO()
    with redirect_stdout(out), redirect_stderr(err):
        rc = fn(args)
    return rc, out.getvalue() + err.getvalue()


class HarnessList(PersonaIso):
    """Against a throwaway plane, outside a frame (`PersonaIso`). The listing now starts with
    this plane's harness profiles, and those are read off `charter.local.toml` — a file that
    exists only on the machine running the suite, so `_planeguard` refuses the real one."""

    def test_it_names_every_registered_harness_and_its_ceilings(self):
        rc, text = _run(commands_harness.cmd_harness_list, SimpleNamespace())
        self.assertEqual(rc, 0)
        for name in ("claude-code", "opencode", "codex"):
            self.assertIn(name, text)
        self.assertIn("status-bar", text)

    def test_it_marks_the_harness_this_session_is_in(self):
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "opencode"}, clear=True):
            _rc, text = _run(commands_harness.cmd_harness_list, SimpleNamespace())
        self.assertRegex(text, r"[*>•→].{0,4}opencode")


class HarnessInstall(PersonaIso):
    """`PersonaIso` because the install resolves a PROFILE before a registry name (ruling 8),
    and profiles are read off `charter.local.toml` — the real plane's is refused."""

    def setUp(self) -> None:
        super().setUp()
        self.home = Path(tempfile.mkdtemp(prefix="charter-codexhome-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(self.home, True))
        self.enterContext(mock.patch.dict(os.environ, {"CODEX_HOME": str(self.home)}))

    def test_installing_codex_names_the_harness_and_points_at_the_plugin(self):
        """The hooks come from the plugin. This writes the one thing the plugin cannot —
        `$CHARTER_HARNESS` — and says where the rest comes from, so nobody adds a second
        copy by hand.

        **Non-zero now, and that is the point of the last step.** Charter writes the policy
        line and then asks Codex whether the profile is wired; the plugin install and the
        hook approval are Codex's own commands, printed here with `CODEX_HOME=` in front. A
        0 would report success over a profile that will still refuse to launch, which is the
        remedy-that-ends-the-investigation `opencode.unvouched` was written against."""
        rc, text = _run(commands_harness.cmd_harness_install,
                        SimpleNamespace(name="codex"))
        self.assertEqual(rc, 1)
        self.assertTrue((self.home / "config.toml").is_file())
        self.assertIn("plugin", text.lower())
        self.assertIn(f"CODEX_HOME={self.home}", text)

    def test_a_harness_charter_can_finish_wiring_is_wired_rather_than_pointed_at(self):
        """It used to answer "needs no opt-in — `charter init` writes its wiring", and that
        stopped being the whole truth when an unwired profile began refusing to launch
        (ruling 10): the sentence a refused `charter opencode` prints is THIS command, so
        this command has to be the one that fixes it. It writes the shim into the config
        home the profile names and then asks opencode whether it took."""
        rc, text = _run(commands_harness.cmd_harness_install,
                        SimpleNamespace(name="opencode"))
        self.assertEqual(rc, 0, text)
        self.assertIn("wired", text)

    def test_an_unknown_harness_is_refused_with_the_known_ones_named(self):
        """The names are the PROFILES now, which is a superset of the registry's: a plane
        that pinned `codex` to an older release should be told that name, not the built-in
        it replaced."""
        rc, text = _run(commands_harness.cmd_harness_install,
                        SimpleNamespace(name="gemini-cli"))
        self.assertEqual(rc, 2)
        self.assertIn("codex", text)


if __name__ == "__main__":
    unittest.main()
