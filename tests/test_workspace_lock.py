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
from unittest import mock

from charter import config, hooks, workspace
from tests import _envguard
from tests._isolation import no_background_refresh


class WorkspaceLockBase(unittest.TestCase):
    SID = "sess-lock-test"

    def setUp(self) -> None:
        # Outside a frame, with no session id and no pinned workspace: stated here
        # rather than inherited from the shell the suite was launched from
        # (#519, #521, #528).
        _envguard.unset_all()

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
    def setUp(self) -> None:
        super().setUp()
        # `test_sessionstart_emits_the_nudge` runs the real hook, which starts the
        # newer-charter check since #938; this plane has no cache or cooldown lock to stop
        # the fork. These cases read the nudge.
        no_background_refresh(self)

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

    # #936 gave a CHAT its own answers in three places, each behind `workspace.launch_lock()`:
    # the refusal, the line `create --use` adds after one, and `unlock`. The deletion sweep
    # forced each of those branches on and nothing went red, because no test said what a
    # session that is NOT a chat hears. These three do. This fixture's session has no frame
    # directory under its id, so `launch_lock` answers `None` here.

    def _said(self, fn, **kw):
        """Every refusal and hint these handlers print goes to stderr, so capture that."""
        err = io.StringIO()
        with redirect_stdout(io.StringIO()), redirect_stderr(err):
            rc = fn(SimpleNamespace(**kw))
        return rc, err.getvalue()

    def test_outside_a_chat_the_refusal_is_still_the_sessions_sentence(self):
        """A chat is told it was launched in its workspace and to open a chat elsewhere. A
        session that is not a chat has real ways out that a chat does not (`unlock`,
        `--force`, a new session), so it keeps the sentence that names them. With the chat
        branch forced on, it would be told it is locked to `'None'`, the workspace a chat
        it is not was launched in."""
        self._run(commands.cmd_workspace_use, name="alpha", force=False, create=True)
        rc, err = self._said(commands.cmd_workspace_use, name="beta", force=False, create=True)
        self.assertEqual(rc, 2)
        self.assertIn("locked to 'alpha' for this session", err)
        self.assertNotIn("launched in", err)

    def test_outside_a_chat_create_use_still_names_a_new_session(self):
        """Inside a chat the "start a new session, or --force" line is dropped, because the
        refusal above it has already named opening a chat. Outside one it is the only place
        the two real routes are named."""
        self._run(commands.cmd_workspace_use, name="alpha", force=False, create=True)
        rc, err = self._said(commands.cmd_workspace_create,
                             name="beta", use=True, force=False, repos=[])
        self.assertEqual(rc, 2)
        self.assertIn("Workspace 'beta' was created; start a new session to use it, "
                      "or re-run with --force", err)

    def test_workspace_current_says_whether_this_session_is_locked(self):
        """`ws current` exists to explain the resolution, and the lock note is half that
        answer. #936 split it three ways — unlocked, locked to what resolved, locked to
        something else — and the first two had no test at all: re-spelling either left the
        suite green."""
        rc, err = self._said(commands.cmd_workspace_current)
        self.assertEqual(rc, 0, err)
        self.assertIn(", unlocked", err)

        self._run(commands.cmd_workspace_use, name="alpha", force=False, create=True)
        rc, err = self._said(commands.cmd_workspace_current)
        self.assertEqual(rc, 0, err)
        self.assertIn("🔒 locked for this session", err)

    def test_the_first_use_of_a_session_says_it_set_and_locked_the_workspace(self):
        """The ordinary path, whose sentence nothing asserted. Both guards on that line
        were survivors: with the `locked and` conjunct dropped, a session that had never
        been locked was told "🔒 still locked to 'None'"; with the verb conditional
        collapsed, its first selection was announced as "re-locked to"."""
        rc, err = self._said(commands.cmd_workspace_use, name="alpha", force=False, create=True)
        self.assertEqual(rc, 0, err)
        self.assertIn("Active workspace set to 'alpha'", err)
        self.assertIn("🔒 locked for this session", err)
        self.assertNotIn("re-locked", err)
        self.assertNotIn("still locked", err)

    def test_a_session_with_no_id_is_told_of_no_lock_because_none_was_written(self):
        """`set_active` writes the lock under a session id, so a shell with no harness id
        gets a terminal pointer and no lock at all. Announcing "🔒 locked for this session"
        there named a lock nothing holds — the same false claim as the chat case, one branch
        over. It is also what pins the `locked and` conjunct above: without it, `None !=
        'alpha'` is true and the announcement became "🔒 still locked to 'None'"."""
        # The pane is STATED: with none, `set_active` writes nothing at all, which is a
        # different sentence (the case below). A runner has no pane id and a laptop shell
        # usually does, so leaving it to the environment made this two tests.
        with mock.patch.dict(os.environ, {}, clear=False), \
                mock.patch.object(workspace, "_terminal_id", return_value="pane-a"):
            os.environ.pop("CLAUDE_CODE_SESSION_ID", None)
            rc, err = self._said(commands.cmd_workspace_use,
                                 name="alpha", force=False, create=True)
            self.assertIsNone(workspace.is_locked(), "the fixture wrote a lock after all")
        self.assertEqual(rc, 0, err)
        self.assertIn("Active workspace set to 'alpha'", err)
        self.assertIn("unlocked", err)
        self.assertNotIn("🔒", err)
        self.assertNotIn("still locked", err)

    def test_create_use_in_a_shell_with_no_session_id_claims_no_lock_either(self):
        """The third success path, and review round 2's finding: `create --use` announced
        "🔒 locked for this session" unconditionally while `use` had stopped. Codex's shells
        are this case (`harness/codex.py`: no per-session id reaches them)."""
        with mock.patch.dict(os.environ, {}, clear=False), \
                mock.patch.object(workspace, "_terminal_id", return_value="pane-e"):
            os.environ.pop("CLAUDE_CODE_SESSION_ID", None)
            rc, err = self._said(commands.cmd_workspace_create,
                                 name="epsilon", use=True, force=False, repos=[])
            self.assertIsNone(workspace.is_locked(), "the fixture wrote a lock after all")
        self.assertEqual(rc, 0, err)
        self.assertIn("Active workspace set to 'epsilon'", err)
        self.assertIn("unlocked", err)
        self.assertNotIn("locked for this session", err)

    def test_a_process_with_no_session_and_no_pane_is_told_nothing_was_persisted(self):
        """`set_active` keys its pointers on a session id and a pane id; with neither it
        writes nothing and reports scope `none`. This answered "Active workspace set to
        'gamma'." — and the next command still resolved to `default`."""
        with mock.patch.dict(os.environ, {}, clear=False), \
                mock.patch.object(workspace, "_terminal_id", return_value=None):
            os.environ.pop("CLAUDE_CODE_SESSION_ID", None)
            rc, err = self._said(commands.cmd_workspace_use,
                                 name="gamma", force=False, create=True)
            resolved = workspace.resolve(cwd=config.ROOT)
        self.assertEqual(rc, 0, err)
        self.assertEqual(resolved, config.DEFAULT_WORKSPACE, "something was persisted after all")
        self.assertIn("Nothing was persisted", err)
        self.assertIn("no session id and no pane id", err)
        self.assertIn("--workspace gamma", err)
        self.assertNotIn("Active workspace set to", err)

    def test_outside_a_chat_a_forced_switch_still_says_it_re_locked(self):
        """The other side of the sentence #936 changed. Outside a chat `--force` really does
        move the lock, so the announcement says so and names where it moved to. Inside a
        chat it moves the commands and not the lock, and says that instead."""
        self._run(commands.cmd_workspace_use, name="alpha", force=False, create=True)
        rc, err = self._said(commands.cmd_workspace_use, name="beta", force=True, create=True)
        self.assertEqual(rc, 0, err)
        self.assertIn("Active workspace re-locked to 'beta'", err)
        self.assertIn("🔒 locked for this session", err)
        self.assertEqual(workspace.is_locked(), "beta")

    def _reconcile_with_payload(self, payload_id: str) -> None:
        """`charter workspace _reconcile` as the SessionStart hook runs it: a JSON payload
        on stdin, and whatever the environment says about this session."""
        old = sys.stdin
        sys.stdin = io.StringIO(json.dumps({"session_id": payload_id}))
        try:
            with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
                commands.cmd_workspace_reconcile(SimpleNamespace())
        finally:
            sys.stdin = old

    def _pane_holding(self, name: str) -> None:
        tid = workspace._terminal_id("myterm")
        config.TERMINALS_DIR.mkdir(parents=True, exist_ok=True)
        (config.TERMINALS_DIR / f"{tid}.workspace").write_text(name + "\n")
        os.environ["TERM_SESSION_ID"] = "myterm"
        self.addCleanup(os.environ.pop, "TERM_SESSION_ID", None)

    def test_reconcile_seeds_the_environments_id_ahead_of_the_payload(self):
        """An id in the environment outranks the payload's. This holds under the old key
        and the new one alike — `$CLAUDE_CODE_SESSION_ID` came first in both — so it pins the
        ordering and NOT the round-1 miskey; the case below is the one that measures that."""
        self._pane_holding("gamma")
        self._reconcile_with_payload("a-different-harness-uuid")
        self.assertEqual(workspace.for_session(self.SID), "gamma")
        self.assertFalse((config.SESSIONS_DIR / "a-different-harness-uuid.workspace").exists())

    def test_reconcile_keys_on_the_chat_id_when_the_chat_has_no_launch_record(self):
        """The round-1 miskey, measured on its own. Inside a chat WITH a launch record the
        early return decides first, so it never reaches the key; a frame with no record (the
        migration case) does. There `$CHARTER_SESSION_ID` is the session every other reader
        resolves by, and the old key — `$CLAUDE_CODE_SESSION_ID` first — seeded the harness's
        id instead, where nothing looks and nothing reaps."""
        self._pane_holding("gamma")
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "nolaunch.1"}):
            self.assertIsNone(workspace.launch_lock(), "the fixture has a launch record")
            self._reconcile_with_payload("payload-uuid")
        self.assertEqual(workspace.for_session("nolaunch.1"), "gamma")
        self.assertFalse((config.SESSIONS_DIR / f"{self.SID}.workspace").exists())
        self.assertFalse((config.SESSIONS_DIR / "payload-uuid.workspace").exists())

    def test_a_harness_that_exports_no_id_still_seeds_from_its_payload(self):
        """The fallback, and the reason the payload is still read: a harness that puts no
        session id in the environment has nothing else to say who it is."""
        self._pane_holding("gamma")
        with mock.patch.dict(os.environ, {}, clear=False):
            os.environ.pop("CLAUDE_CODE_SESSION_ID", None)
            self._reconcile_with_payload("payload-only-session")
        self.assertEqual(workspace.for_session("payload-only-session"), "gamma")

    def test_outside_a_chat_unlock_still_releases_the_lock(self):
        """`unlock` refuses inside a chat, whose lock is its launch record. Outside one the lock
        is a file and releasing it is this command's whole job."""
        self._run(commands.cmd_workspace_use, name="alpha", force=False, create=True)
        rc, err = self._said(commands.cmd_workspace_unlock)
        self.assertEqual(rc, 0, err)
        self.assertIsNone(workspace.is_locked())


# imported late so the module-under-test picks up the patched config at call time
from charter import commands_workspace as commands  # noqa: E402


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


if __name__ == "__main__":
    unittest.main()
