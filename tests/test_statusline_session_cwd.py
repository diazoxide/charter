"""The status line resolves the active workspace from the SESSION's cwd, not its own.

Claude Code hands the status line the session's working directory in the payload
(`workspace.current_dir`) precisely because the hook process's own cwd is not guaranteed
to be it. The renderer already trusts that field for one job — `_current` uses it to mark
which repo or worktree row you are standing in — and used to ignore it for the other,
resolving the active workspace from `os.getcwd()` instead.

Two notions of "where the session is" in a single render, and when they disagreed the
result was the confusing half-right state: the row for a repo in workspace A marked as
current, under a header naming workspace B. `resolve`'s own docstring is the argument
against it — the cwd rung outranks every pointer because "being inside one is not a hint
about which workspace is active, it is the fact" — and a fact read from the wrong process
is not that fact.
"""

from __future__ import annotations

import os
import re
import unittest
from pathlib import Path

from charter import config, statusline, workspace
from tests._isolation import PersonaIso


def _plain(s: str) -> str:
    return re.sub(r"\x1b\[[0-9;]*m", "", s)


class SessionCwdBase(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        # Two workspaces, each with a repo, so "which one" has a wrong answer available.
        self.alpha = config.WORKSPACES_DIR / "alpha" / "billing-api"
        self.beta = config.WORKSPACES_DIR / "beta" / "checkout-ui"
        for d in (self.alpha, self.beta):
            d.mkdir(parents=True, exist_ok=True)

        # The process sits at the plane root — inside no workspace at all, which is the
        # ordinary case for a hook and the one that exposes the bug.
        self._cwd = os.getcwd()
        os.chdir(self.tmp)
        self.addCleanup(lambda: os.chdir(self._cwd))

        self._env = {k: os.environ.get(k) for k in ("CHARTER_WORKSPACE", "COLUMNS")}
        os.environ.pop("CHARTER_WORKSPACE", None)
        self.addCleanup(self._restore_env)

    def _restore_env(self) -> None:
        for k, v in self._env.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v


class TestResolveTakesACwd(SessionCwdBase):
    def test_resolve_reads_the_cwd_it_is_given(self):
        self.assertEqual(workspace.resolve(cwd=str(self.alpha)), "alpha")
        self.assertEqual(workspace.resolve(cwd=str(self.beta)), "beta")

    def test_source_agrees_with_resolve(self):
        """A header that names the workspace and the reason must not disagree about it."""
        self.assertEqual(workspace.source(cwd=str(self.alpha)), "cwd")

    def test_a_cwd_outside_any_workspace_falls_through(self):
        """Standing in the plane root is not a claim about which workspace is active."""
        self.assertEqual(workspace.source(cwd=str(self.tmp)), "default")

    def test_an_explicit_workspace_still_outranks_the_cwd(self):
        self.assertEqual(workspace.resolve("beta", cwd=str(self.alpha)), "beta")

    def test_the_env_var_still_outranks_the_cwd(self):
        os.environ["CHARTER_WORKSPACE"] = "beta"
        self.assertEqual(workspace.resolve(cwd=str(self.alpha)), "beta")


class TestStatusLineUsesTheSessionCwd(SessionCwdBase):
    def _header(self, payload: dict) -> str:
        os.environ["COLUMNS"] = "200"
        return _plain(statusline.render(payload).split("\n")[1])

    def test_active_follows_the_payload(self):
        ws, src = statusline._active(session_id="no-such-session", cwd=str(self.alpha))
        self.assertEqual(ws, "alpha")
        self.assertEqual(src, "cwd")

    def test_the_rendered_header_names_the_sessions_workspace(self):
        """The end-to-end shape of the bug: the process is in neither workspace, the
        session is in `alpha`, and the header used to say `default`."""
        header = self._header({"workspace": {"current_dir": str(self.alpha)},
                               "session_id": "no-such-session"})
        self.assertIn("alpha", header)
        self.assertNotIn("default", header)

    def test_a_payload_without_a_cwd_is_not_an_error(self):
        """Payloads come from another program; the field is not guaranteed."""
        for payload in ({}, {"workspace": {}}, {"workspace": {"current_dir": ""}}):
            with self.subTest(payload=payload):
                self.assertIn("default", self._header(payload))

    def test_a_cwd_in_some_other_plane_does_not_name_a_workspace_here(self):
        """A path outside this plane's `workspaces/` says nothing about this plane, and
        must fall through to the pointers rather than inventing a name from its parts."""
        outside = Path(self.tmp).parent / "somewhere-else" / "workspaces" / "ghost" / "r"
        ws, src = statusline._active(session_id="no-such-session", cwd=str(outside))
        self.assertEqual(ws, config.DEFAULT_WORKSPACE)
        self.assertEqual(src, "default")


if __name__ == "__main__":
    unittest.main()
