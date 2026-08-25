"""charter's hook manifest must not need a feature only one harness has.

Codex loads charter's plugin and runs its hooks — then prints, twice, every session:

    warning: skipping async hook in …/hooks/hooks.json: async hooks are not supported yet

So `charter persona _gc` (prunes ended sessions' scratch) and `charter gl-refresh` (the
status line's forge state) never run there, silently after the warning scrolls past. That
is the whole initiative in miniature: charter asked the host for something one host does
not have, and the answer is to stop asking rather than to wait.

`--detach` returns immediately and leaves the work to a process that outlives the hook,
which is what `async` bought — and it is charter's own code, so it works everywhere.
"""

from __future__ import annotations

import json
import unittest
from pathlib import Path
from unittest import mock

from charter import util
from tests import _envguard

MANIFEST = Path(__file__).resolve().parents[1] / "hooks" / "hooks.json"


def _entries(doc: dict):
    for event, groups in (doc.get("hooks") or {}).items():
        for group in groups:
            for entry in group.get("hooks") or []:
                yield event, entry


class TheManifestAsksForNothingExotic(unittest.TestCase):
    def setUp(self) -> None:
        self.doc = json.loads(MANIFEST.read_text())

    def test_no_entry_declares_async(self):
        offenders = [f"{ev}: {e.get('command')}" for ev, e in _entries(self.doc)
                     if "async" in e]
        self.assertEqual(offenders, [],
                         "Codex skips async hooks entirely — detach inside the command "
                         f"instead: {offenders}")

    def test_the_work_async_used_to_cover_is_still_deferred(self):
        """Dropping `async` without detaching would trade a skipped hook for a session
        that blocks on a network refresh — a worse bug, and a quieter one."""
        cmds = [e.get("command", "") for _ev, e in _entries(self.doc)]
        for needle in ("persona _gc", "gl-refresh"):
            with self.subTest(command=needle):
                matching = [c for c in cmds if needle in c]
                self.assertTrue(matching, f"{needle} is no longer wired at all")
                self.assertTrue(all("--detach" in c for c in matching),
                                f"{needle} would now block the session: {matching}")


class Detaching(unittest.TestCase):
    def test_it_respawns_the_same_command_without_the_flag(self):
        with mock.patch.object(util.subprocess, "Popen") as popen:
            self.assertTrue(util.detach_self(["persona", "_gc"]))
        argv = popen.call_args.args[0]
        # -P (#390): `-m` prepends the cwd to sys.path, so a hook run from a charter
        # checkout would otherwise respawn that tree instead of the installed package.
        self.assertEqual(argv[1:], ["-P", "-m", "charter", "persona", "_gc"])
        self.assertNotIn("--detach", argv)

    def test_the_child_outlives_the_hook(self):
        """`start_new_session` is the point: a hook's process group is torn down when the
        turn ends, and a refresh killed halfway is worse than one that never started."""
        with mock.patch.object(util.subprocess, "Popen") as popen:
            util.detach_self(["gl-refresh"])
        self.assertTrue(popen.call_args.kwargs.get("start_new_session"))

    def test_a_spawn_that_fails_is_reported_not_raised(self):
        with mock.patch.object(util.subprocess, "Popen", side_effect=OSError("nope")):
            self.assertFalse(util.detach_self(["gl-refresh"]))


if __name__ == "__main__":
    unittest.main()


class TheCommandsHonourTheFlag(unittest.TestCase):
    """The manifest passing `--detach` proves nothing on its own.

    An earlier attempt at this change inserted the guard into the wrong function — the
    manifest test still passed, because it only reads JSON. What the hook actually calls
    has to be driven.
    """
    def setUp(self) -> None:
        # Outside a frame, with no session id and no pinned workspace: stated here
        # rather than inherited from the shell the suite was launched from
        # (#519, #521, #528).
        _envguard.unset_all()

    def _drive(self, module_name: str, func_name: str, argv: list[str]):
        import importlib
        from types import SimpleNamespace

        mod = importlib.import_module(module_name)
        with mock.patch.object(util, "detach_self", return_value=True) as spawn:
            rc = getattr(mod, func_name)(SimpleNamespace(detach=True, workspace=None))
        self.assertEqual(rc, 0)
        spawn.assert_called_once_with(argv)

    def test_gl_refresh_detaches_instead_of_refreshing(self):
        self._drive("charter.commands", "cmd_gl_refresh", ["gl-refresh"])

    def test_persona_gc_detaches_instead_of_collecting(self):
        self._drive("charter.commands_persona", "cmd_persona_gc", ["persona", "_gc"])

    def test_without_the_flag_the_work_still_happens_here(self):
        """The detach path must not become the only path — `charter gl-refresh` typed by
        hand should refresh, not fork and vanish."""
        from types import SimpleNamespace

        from charter import commands
        with mock.patch.object(util, "detach_self") as spawn, \
                mock.patch.object(commands.workspace, "repo_trees", return_value=[]):
            commands.cmd_gl_refresh(SimpleNamespace(detach=False, workspace=None))
        spawn.assert_not_called()
