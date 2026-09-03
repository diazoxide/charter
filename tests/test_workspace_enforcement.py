"""Workspace enforcement hooks: the clone-edit → memo nudge (PostToolUse) and the
debounced auto-save (Stop hook) gating."""

from __future__ import annotations

import unittest
from types import SimpleNamespace
from unittest import mock

from charter import config, hooks, workspace
from charter import commands_workspace as cw
from tests._isolation import PersonaIso, PlaneIso, run_hook


def _ctx(r):
    return (r or {}).get("hookSpecificOutput", {}).get("additionalContext", "") if r else ""


class TestCloneEditNudge(PlaneIso):
    def _edit(self, path, session="s1"):
        return run_hook(hooks.posttooluse,
                        {"tool_name": "Write", "tool_input": {"file_path": path}, "session_id": session})

    def test_live_workspace_clone_edit_nudges(self):
        workspace.set_live("myws", True)  # only LIVE workspaces nudge for a shared memo
        ctx = _ctx(self._edit("workspaces/myws/iam-service/src/main.py"))
        self.assertIn("workspace memo", ctx)
        self.assertIn("myws", ctx)
        self.assertIn("iam-service", ctx)

    def test_local_workspace_clone_edit_is_silent(self):
        # LOCAL (default) → private → no "record a shared memo" nudge
        self.assertEqual(self._edit("workspaces/localws/iam-service/a.py"), None)

    def test_nudge_is_once_per_session_per_workspace(self):
        workspace.set_live("myws", True)
        self.assertTrue(_ctx(self._edit("workspaces/myws/iam-service/a.py")))   # first → nudge
        self.assertEqual(self._edit("workspaces/myws/iam-service/b.py"), None)  # second → silent
        self.assertEqual(self._edit("workspaces/myws/account-service/c.py"), None)  # same ws → silent

    def test_a_different_live_workspace_nudges_again(self):
        workspace.set_live("ws1", True)
        workspace.set_live("ws2", True)
        self.assertTrue(_ctx(self._edit("workspaces/ws1/repo/a.py")))
        self.assertTrue(_ctx(self._edit("workspaces/ws2/repo/a.py")))  # different ws → nudges

    def test_editing_workspace_memory_is_not_a_clone_nudge(self):
        workspace.set_live("myws", True)
        self.assertEqual(self._edit("workspaces/myws/memory/notes.md"), None)


class TestAutosaveGating(unittest.TestCase):
    """The Stop-hook autosave: no-op when nothing pending or within debounce; commits +
    background-pushes when pending + debounce elapsed AND the control plane's declared
    `memory.share` posture allows it. Git + push are mocked.

    This checkout has no `charter.toml`, so `config.MEMORY_SHARE` defaults to `local` —
    every scenario below sets it explicitly so the reactive-vs-posture wiring is tested
    deliberately rather than accidentally passing on the safe default."""

    def setUp(self):
        import tempfile
        from pathlib import Path
        self.tmp = Path(tempfile.mkdtemp(prefix="edm-autosave-"))
        self._orig = {k: getattr(config, k) for k in
                      ("ROOT", "STATE_DIR", "WORKSPACES_DIR", "MEMORY_SHARE")}
        config.ROOT = self.tmp
        config.STATE_DIR = self.tmp / ".charter"
        config.WORKSPACES_DIR = self.tmp / "workspaces"
        # a workspace with a memory file so _ws_meta_paths is non-empty
        (config.WORKSPACES_DIR / "default" / "memory").mkdir(parents=True)
        (config.WORKSPACES_DIR / "default" / "memory" / "notes.md").write_text("x")
        self.addCleanup(self._restore)

    def _restore(self):
        import shutil
        for k, v in self._orig.items():
            setattr(config, k, v)
        shutil.rmtree(self.tmp, ignore_errors=True)

    def _run(self, pending: bool, live: bool = True, share: str = "push"):
        config.MEMORY_SHARE = share
        status = SimpleNamespace(stdout=(" M workspaces/default/memory/notes.md\n" if pending else ""))
        with mock.patch.object(cw, "_git", return_value=status), \
             mock.patch.object(cw, "commit_push", return_value=0) as cp, \
             mock.patch.object(cw.subprocess, "Popen") as popen, \
             mock.patch.object(cw.workspace, "is_live", return_value=live), \
             mock.patch.object(cw.workspace, "resolve", return_value="default"):
            cw.cmd_workspace_autosave(SimpleNamespace())
        return cp, popen

    def test_noop_when_nothing_pending(self):
        cp, popen = self._run(pending=False)
        cp.assert_not_called()
        popen.assert_not_called()

    def test_commits_and_bg_pushes_when_pending(self):
        cp, popen = self._run(pending=True)
        cp.assert_called_once()             # secret-scanned commit (no_push)
        popen.assert_called_once()          # detached background push

    def test_skips_local_workspace(self):
        cp, popen = self._run(pending=True, live=False)  # LOCAL → never auto-committed
        cp.assert_not_called()
        popen.assert_not_called()

    def test_debounce_skips_recent(self):
        marker = config.STATE_DIR / "ws-autosave" / "default"
        marker.parent.mkdir(parents=True, exist_ok=True)
        marker.write_text("now")            # fresh marker → within debounce window
        cp, popen = self._run(pending=True)
        cp.assert_not_called()
        popen.assert_not_called()

    def test_local_posture_skips_even_when_live_and_pending(self):
        """The safe default: a workspace memo never even reaches git, regardless of the
        workspace's own LIVE/LOCAL setting."""
        cp, popen = self._run(pending=True, live=True, share="local")
        cp.assert_not_called()
        popen.assert_not_called()

    def test_commit_posture_commits_but_never_pushes(self):
        cp, popen = self._run(pending=True, share="commit")
        cp.assert_called_once()
        self.assertTrue(cp.call_args.kwargs.get("no_push"))
        popen.assert_not_called()           # `commit` records but never publishes

    def test_unrecognised_posture_skips_even_when_live_and_pending(self):
        """`config.MEMORY_SHARE` is always pre-clamped through `instance.share_of` — but
        this autosave path must not itself depend on that. A value outside the three known
        modes fell past the `local` guard and unconditionally committed (only the
        background PUSH was gated on `share == "push"`) — a workspace memo landing in git
        on a typo. Must behave exactly like `local`: no commit, no push."""
        cp, popen = self._run(pending=True, live=True, share="puhs")
        cp.assert_not_called()
        popen.assert_not_called()


if __name__ == "__main__":
    unittest.main()
