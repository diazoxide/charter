"""The per-session workspace lock: once a session confirms a workspace, it can't be
switched mid-session (unless forced), and SessionStart nudges the agent to confirm one.

The lock is keyed by the Claude session id and stored at ``.charter/sessions/<sid>.lock``.
These paths are derived from ``STATE_DIR``, so the fixture repoints the whole plane at a tmp
dir through ``config.use`` and pins ``$CLAUDE_CODE_SESSION_ID`` so ``set_active`` sees a
stable session.
"""

from __future__ import annotations

import io
import json
import os
import shutil
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace

from charter import config, hooks, workspace


class WorkspaceLockBase(unittest.TestCase):
    SID = "sess-lock-test"

    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp(prefix="edm-wslock-"))
        # `config.use`, not a hand-picked list. This fixture named five attributes and
        # `set_active` writes through a sixth — `PERSONA_STATE_DIR`, where the trace lives —
        # so every case below appended `workspace-use`/`workspace-refused` rows to the
        # DEVELOPER's own plane, twenty a run, in the session bucket `charter trace
        # --summary` counts (#372). `config.DERIVED` is the single definition of what
        # follows from a root precisely so a fixture cannot miss one; #227 moved
        # `test_plugin` onto the same seam after the same failure.
        #
        # `charter.toml` first: `use` re-derives HAS_CONTROL_PLANE, and these cases drive
        # handlers that are gated on it. The hand-rolled patch left the real plane's value
        # standing, so dropping the marker would silently change what is under test.
        (self.tmp / "charter.toml").write_text("schema = 1\n")
        self._orig = config.use(self.tmp)
        config.WORKSPACES_DIR.mkdir(parents=True, exist_ok=True)

        self._orig_env = {k: os.environ.get(k) for k in ("CLAUDE_CODE_SESSION_ID", "CHARTER_WORKSPACE")}
        os.environ["CLAUDE_CODE_SESSION_ID"] = self.SID
        os.environ.pop("CHARTER_WORKSPACE", None)
        self.addCleanup(self._restore)

    def _restore(self) -> None:
        config.restore(self._orig)
        for k, v in self._orig_env.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v
        shutil.rmtree(self.tmp, ignore_errors=True)


class TestSessionLock(WorkspaceLockBase):
    def test_first_confirm_locks_the_session(self):
        self.assertIsNone(workspace.is_locked())
        scope = workspace.set_active("alpha")
        self.assertNotEqual(scope, "locked")
        self.assertEqual(workspace.is_locked(), "alpha")
        self.assertEqual(workspace.resolve(), "alpha")

    def test_switch_mid_session_is_refused(self):
        workspace.set_active("alpha")
        scope = workspace.set_active("beta")
        self.assertEqual(scope, "locked")
        # nothing moved: still locked + resolving to alpha
        self.assertEqual(workspace.is_locked(), "alpha")
        self.assertEqual(workspace.resolve(), "alpha")

    def test_reaffirming_same_workspace_is_allowed(self):
        workspace.set_active("alpha")
        scope = workspace.set_active("alpha")
        self.assertNotEqual(scope, "locked")
        self.assertEqual(workspace.is_locked(), "alpha")

    def test_force_overrides_and_relocks(self):
        workspace.set_active("alpha")
        scope = workspace.set_active("beta", force=True)
        self.assertNotEqual(scope, "locked")
        self.assertEqual(workspace.is_locked(), "beta")
        self.assertEqual(workspace.resolve(), "beta")

    def test_unlock_then_switch(self):
        workspace.set_active("alpha")
        self.assertTrue(workspace.unlock())
        self.assertIsNone(workspace.is_locked())
        scope = workspace.set_active("beta")
        self.assertNotEqual(scope, "locked")
        self.assertEqual(workspace.is_locked(), "beta")

    def test_unlock_when_unlocked_is_noop(self):
        self.assertFalse(workspace.unlock())

    def test_env_pin_still_wins_over_lock(self):
        # $CHARTER_WORKSPACE is a hard launch pin: it dominates resolution regardless of lock.
        workspace.set_active("alpha")
        os.environ["CHARTER_WORKSPACE"] = "pinned"
        self.assertEqual(workspace.resolve(), "pinned")

    def test_a_different_session_starts_unlocked(self):
        workspace.set_active("alpha")               # locks THIS session
        self.assertIsNone(workspace.is_locked("other-session"))

    def test_reconcile_seeds_but_does_not_lock(self):
        # Simulate a terminal pane that already had a workspace, then a fresh session.
        tid = workspace._terminal_id("myterm")
        config.TERMINALS_DIR.mkdir(parents=True, exist_ok=True)
        (config.TERMINALS_DIR / f"{tid}.workspace").write_text("gamma\n")
        seeded = workspace.reconcile(session_id="fresh-sess", terminal_id="myterm")
        self.assertEqual(seeded, "gamma")
        # the session pointer is seeded (continuity) but the session is NOT locked
        self.assertEqual(workspace.resolve(session_id="fresh-sess"), "gamma")
        self.assertIsNone(workspace.is_locked("fresh-sess"))


class TestConfirmNudge(WorkspaceLockBase):
    def test_nudge_fires_when_unconfirmed(self):
        msg = hooks._workspace_confirm_nudge(self.SID)
        self.assertIn("Confirm the workspace", msg)
        self.assertIn("charter workspace use", msg)

    def test_no_nudge_once_locked(self):
        workspace.set_active("alpha")
        self.assertEqual(hooks._workspace_confirm_nudge(self.SID), "")

    def test_no_nudge_when_env_pinned(self):
        os.environ["CHARTER_WORKSPACE"] = "pinned"
        self.assertEqual(hooks._workspace_confirm_nudge(self.SID), "")

    def test_sessionstart_emits_the_nudge(self):
        old = sys.stdin
        sys.stdin = io.StringIO(json.dumps({"session_id": self.SID}))
        try:
            buf = io.StringIO()
            with redirect_stdout(buf):
                hooks.sessionstart()
        finally:
            sys.stdin = old
        out = buf.getvalue().strip()
        self.assertTrue(out, "sessionstart should emit context when unconfirmed")
        ctx = json.loads(out)["hookSpecificOutput"]["additionalContext"]
        self.assertIn("Confirm the workspace", ctx)


class TestCommandLayer(WorkspaceLockBase):
    """The `edm workspace` command handlers surface the lock as a non-zero exit + message."""

    def _run(self, fn, **kw):
        buf = io.StringIO()
        with redirect_stdout(buf):
            rc = fn(SimpleNamespace(**kw))
        return rc, buf.getvalue()

    def test_use_locks_then_refuses_switch(self):
        # `--create`: `use` no longer invents a workspace from a name. A typo used to be
        # created AND session-locked, so the correction hit "locked to 'fature-x'".
        rc, _ = self._run(commands.cmd_workspace_use, name="alpha", force=False, create=True)
        self.assertEqual(rc, 0)
        self.assertEqual(workspace.is_locked(), "alpha")
        rc, out = self._run(commands.cmd_workspace_use, name="beta", force=False, create=True)
        self.assertEqual(rc, 2)  # refused (message goes to stderr)
        self.assertEqual(workspace.is_locked(), "alpha")

    def test_use_force_switches(self):
        self._run(commands.cmd_workspace_use, name="alpha", force=False, create=True)
        rc, _ = self._run(commands.cmd_workspace_use, name="beta", force=True, create=True)
        self.assertEqual(rc, 0)
        self.assertEqual(workspace.is_locked(), "beta")

    def test_create_use_respects_lock(self):
        self._run(commands.cmd_workspace_use, name="alpha", force=False, create=True)
        rc, out = self._run(commands.cmd_workspace_create, name="beta", use=True, force=False, repos=[])
        self.assertEqual(rc, 2)
        # the workspace is still created even though the switch was refused
        self.assertTrue((config.WORKSPACES_DIR / "beta").exists())
        self.assertEqual(workspace.is_locked(), "alpha")


# imported late so the module-under-test picks up the patched config at call time
from charter import commands_workspace as commands  # noqa: E402


if __name__ == "__main__":
    unittest.main()


class UseDoesNotInventAWorkspaceFromATypo(WorkspaceLockBase):
    """`use` validated the name's SHAPE, then created it and took the session lock. So
    `charter workspace use fature-x` made `fature-x`, locked to it, and the correction hit
    `✗ Workspace is 🔒 locked to 'fature-x' for this session` — a one-way door out of a
    single mistyped character."""

    def _run(self, **kw):
        buf, err = io.StringIO(), io.StringIO()
        with redirect_stdout(buf), redirect_stderr(err):
            rc = commands.cmd_workspace_use(SimpleNamespace(force=False, create=False, **kw))
        return rc, err.getvalue()

    def test_an_unknown_name_is_refused(self):
        rc, err = self._run(name="fature-x")
        self.assertEqual(rc, 1)
        self.assertIn("no workspace named", err)

    def test_it_does_not_create_the_typo(self):
        self._run(name="fature-x")
        self.assertFalse((config.WORKSPACES_DIR / "fature-x").exists())

    def test_it_does_not_take_the_session_lock(self):
        """The half that made it unrecoverable."""
        self._run(name="fature-x")
        self.assertIsNone(workspace.is_locked())

    def test_it_suggests_the_name_you_meant(self):
        workspace.ensure("feature-x")
        _rc, err = self._run(name="fature-x")
        self.assertIn("feature-x", err)

    def test_create_is_the_deliberate_path(self):
        buf, err = io.StringIO(), io.StringIO()
        with redirect_stdout(buf), redirect_stderr(err):
            rc = commands.cmd_workspace_use(
                SimpleNamespace(name="brand-new", force=False, create=True))
        self.assertEqual(rc, 0)
        self.assertTrue((config.WORKSPACES_DIR / "brand-new").exists())


class EnsureScaffoldsSoNothingAsksForReinit(WorkspaceLockBase):
    """`scaffold` ran only from create/live/restore/fork, so a workspace born via
    `charter clone` or `workspace use` got a bare directory — and the status line then
    showed `⚠ reinit` every turn, phrased as post-upgrade drift, for a workspace charter
    had just made correctly. The README's own quickstart ended that way."""

    def test_a_freshly_ensured_workspace_does_not_need_reinit(self):
        workspace.ensure("fresh")
        self.assertFalse(workspace.needs_reinit("fresh"))

    def test_it_has_its_baseline_structure(self):
        workspace.ensure("fresh")
        self.assertTrue((config.WORKSPACES_DIR / "fresh" / "memory").is_dir())
