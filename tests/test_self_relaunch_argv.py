"""Every self-relaunch site builds its child argv through `util.self_relaunch_argv`,
never a hand-built `[sys.executable, "-m", "charter", ...]` list of its own — #390.

`python -m charter` prepends the current working directory to `sys.path` before it even
looks for the `charter` package to import (`-m`'s own documented behaviour). Every site
below sets its child's `cwd` to something outside charter's own control — a project
directory, a workspace root, wherever an operator's pane happened to start — and when
THAT directory contains its own `charter/` package (a charter checkout dogfooding
itself, the common case for anyone developing charter), the child imports that tree
instead of the installed one. `-P` (3.11+, `pyproject.toml` already requires it) is
`-m`'s own switch for "don't do that".

`tests/test_self_relaunch_shadowing.py` proves the mechanism end to end against a real
decoy package that genuinely shadows. This module is the fast half: one test per site,
asserting the argv it hands to `Popen`/`split-window` carries `-P` — a passing
end-to-end test on ONE site does not prove the other sites were changed, so each is
pinned separately here.
"""

from __future__ import annotations

import subprocess
import sys
import unittest
from pathlib import Path
from unittest import mock

from charter import glstate, planegit, update, util
from charter import commands_workspace as cw

from tests._isolation import PersonaIso


class SelfRelaunchArgvShape(unittest.TestCase):
    """The helper itself: charter/util.py."""

    def test_bare_call_is_the_interpreter_plus_dash_p_plus_the_module(self):
        self.assertEqual(util.self_relaunch_argv(),
                         [sys.executable, "-P", "-m", "charter"])

    def test_extra_args_are_appended_after_the_module(self):
        self.assertEqual(
            util.self_relaunch_argv("workspace", "_pushbg"),
            [sys.executable, "-P", "-m", "charter", "workspace", "_pushbg"],
        )


class DetachSelfUsesIt(unittest.TestCase):
    """charter/util.py:288 — `detach_self`, a session-start hook's own background
    refresh (`gl-refresh`, `persona _gc`)."""

    def test_argv_carries_dash_p(self):
        with mock.patch.object(util.subprocess, "Popen") as popen:
            self.assertTrue(util.detach_self(["gl-refresh"]))
        argv = popen.call_args.args[0]
        self.assertEqual(argv, util.self_relaunch_argv("gl-refresh"))


class GlstateMaybeSpawnUsesIt(PersonaIso):
    """charter/glstate.py:226 — `gl-refresh`, spawned from the status line's own render
    path. The quiet one: on any charter checkout this ran the wrong charter on every
    render, indefinitely, until fixed."""

    def test_argv_carries_dash_p(self):
        d = self.tmp / "somerepo"
        d.mkdir(parents=True, exist_ok=True)
        captured = {}

        def fake_popen(cmd, **kw):
            captured["cmd"] = cmd
            return mock.MagicMock()

        with mock.patch.object(glstate.subprocess, "Popen", side_effect=fake_popen):
            glstate.maybe_spawn([d])

        self.assertIn("cmd", captured, "Popen was never called — nothing was stale?")
        self.assertEqual(captured["cmd"], util.self_relaunch_argv("gl-refresh"))


class UpdateMaybeSpawnUsesIt(PersonaIso):
    """charter/update.py:185 — `_version-check`, ALSO spawned from the status line's own
    render path. Not one of the five sites #390 originally named; found by grepping the
    tree for every `sys.executable`."""

    def test_argv_carries_dash_p(self):
        captured = {}

        def fake_popen(cmd, **kw):
            captured["cmd"] = cmd
            return mock.MagicMock()

        with mock.patch.object(update.subprocess, "Popen", side_effect=fake_popen):
            update.maybe_spawn()

        self.assertIn("cmd", captured, "Popen was never called — was the cache fresh?")
        self.assertEqual(captured["cmd"], util.self_relaunch_argv("_version-check"))


class WorkspaceAutosavePushUsesIt(unittest.TestCase):
    """charter/commands_workspace.py:686 — `workspace _pushbg`, the Stop-hook autosave's
    background push under the `push` sharing posture."""

    def test_argv_carries_dash_p(self):
        with mock.patch.object(cw.subprocess, "Popen") as popen:
            cw._spawn_pushbg(Path("/some/control/plane"))
        argv = popen.call_args.args[0]
        self.assertEqual(argv, util.self_relaunch_argv("workspace", "_pushbg"))
        self.assertEqual(popen.call_args.kwargs.get("cwd"), "/some/control/plane")


class DiscoverPushUsesIt(unittest.TestCase):
    """charter/planegit.py:87 — `workspace _pushbg`, `discover`'s own background push of
    HEAD, the sibling of the autosave push above."""

    def test_argv_carries_dash_p(self):
        with mock.patch.object(planegit.subprocess, "Popen") as popen:
            planegit._spawn_bg_push(Path("/some/control/plane"))
        argv = popen.call_args.args[0]
        self.assertEqual(argv, util.self_relaunch_argv("workspace", "_pushbg"))
        self.assertEqual(popen.call_args.kwargs.get("cwd"), "/some/control/plane")


if __name__ == "__main__":
    unittest.main()
