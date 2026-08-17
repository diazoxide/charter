"""A workspace choice that survives a session boundary (#193, unparking #124).

`workspace.resolve()` runs `--workspace` → `$CHARTER_WORKSPACE` → the tree you are standing
in → per-session pointer → per-terminal pointer → `default`. On a terminal that reports no
pane id, the per-terminal rung — the one that exists precisely to cover "new session, same
terminal" — can never fire. So a shell at the plane root lands on `default` every time, and
the operator asks "why are you in default workspace again?"

That is #124's own unpark trigger: *"a terminal in common use turns out to supply no pane
id"*. It fired.

**This is not the fix #124 rejected.** That was an IMPLICIT last-active pointer — written by
every `workspace use`, changing under sessions that never asked, which is the failure
`_terminal_id` was hardened against ("an id that is wrong in the sharing direction is worse
than no id"). This is EXPLICIT: set once by a human, stable, read only when every other rung
has missed. `charter persona default` is exactly this shape and already ships.

And what it replaces was never a considered answer either — it was a literal `default`
workspace nobody chose. Slotting a nominated one there does not make workspaces less
per-task; it makes the fallback something a human picked, and lets `default` mean "nobody
ever chose" again.
"""

from __future__ import annotations

import io
import os
import unittest
from contextlib import redirect_stderr, redirect_stdout
from types import SimpleNamespace

from charter import config, workspace
from charter import commands_workspace as cw
from tests._isolation import PersonaIso


class DefaultCase(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        os.environ.pop("CHARTER_WORKSPACE", None)
        for n in ("alpha", "beta"):
            workspace.ensure(n)
            workspace.scaffold(n)

    def run_default(self, name=None, clear=False):
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = cw.cmd_workspace_default(SimpleNamespace(name=name, clear=clear))
        return rc, out.getvalue() + err.getvalue()

    def resolve(self, **kw):
        # A cwd outside any workspace tree: the plane root, which is the normal place to
        # stand and the case the report is about.
        return workspace.resolve(cwd=str(config.ROOT), **kw)


class TestTheDeclaredDefaultAnswers(DefaultCase):
    def test_without_one_a_rootless_session_lands_on_the_builtin(self):
        self.assertEqual(self.resolve(), config.DEFAULT_WORKSPACE)

    def test_with_one_it_lands_there_instead(self):
        self.run_default("alpha")
        self.assertEqual(self.resolve(), "alpha")

    def test_it_survives_a_session_boundary(self):
        """The whole point: a new session has a new id and no pointer, and on this terminal
        no pane id either."""
        self.run_default("alpha")
        self.assertEqual(self.resolve(session_id="a-fresh-session-id"), "alpha")

    def test_it_is_committed_under_workspaces(self):
        """Committed, so it travels with the plane — the same standing `personas/.default`
        has."""
        self.run_default("alpha")
        self.assertTrue((config.WORKSPACES_DIR / ".default").exists())


class TestItIsReadLast(DefaultCase):
    def test_an_explicit_flag_still_wins(self):
        self.run_default("alpha")
        self.assertEqual(self.resolve(explicit="beta"), "beta")

    def test_the_env_var_still_wins(self):
        self.run_default("alpha")
        os.environ["CHARTER_WORKSPACE"] = "beta"
        self.addCleanup(os.environ.pop, "CHARTER_WORKSPACE", None)
        self.assertEqual(self.resolve(), "beta")

    def test_the_tree_you_stand_in_still_wins(self):
        """The cwd rung cannot be wrong — being inside a workspace's tree IS the fact, not a
        hint — so a nominated default must never override it."""
        self.run_default("alpha")
        inside = workspace.workspace_dir("beta") / "repo"
        inside.mkdir(parents=True, exist_ok=True)
        self.assertEqual(workspace.resolve(cwd=str(inside)), "beta")

    def test_a_session_pointer_still_wins(self):
        self.run_default("alpha")
        workspace.set_active("beta", session_id="s1")
        self.assertEqual(self.resolve(session_id="s1"), "beta")


class TestTheCommand(DefaultCase):
    def test_it_shows_the_current_one(self):
        self.run_default("alpha")
        _, out = self.run_default()
        self.assertIn("alpha", out)

    def test_it_says_there_is_none_and_how_to_set_one(self):
        _, out = self.run_default()
        self.assertIn("charter workspace default", out)

    def test_an_unknown_workspace_is_refused(self):
        rc, _ = self.run_default("ghost")
        self.assertEqual(rc, 1)
        self.assertIsNone(workspace.declared_default())

    def test_clear_removes_it(self):
        self.run_default("alpha")
        self.run_default("alpha", clear=True)
        self.assertIsNone(workspace.declared_default())
        self.assertEqual(self.resolve(), config.DEFAULT_WORKSPACE)


class TestTheSurfaceSaysWhichRungAnswered(DefaultCase):
    def test_the_declared_default_is_named_as_the_source(self):
        self.run_default("alpha")
        self.assertIn("declared default", workspace.source(cwd=str(config.ROOT)))

    def test_falling_through_says_WHY_not_just_that_it_did(self):
        """The operator's actual complaint: a surface asserting `default` with no reason,
        twice, where reconstructing it meant reading `resolve`. ADR 0013's second rule aimed
        at the line people read every turn."""
        src = workspace.source(cwd=str(config.ROOT))
        self.assertIn("default", src)
        self.assertNotEqual(src, "default")

    def test_it_names_the_missing_pane_id_when_that_is_the_cause(self):
        """`workspace use` already admits this; the surface that shows the RESULT did not."""
        real = workspace._terminal_id
        workspace._terminal_id = lambda: None
        self.addCleanup(setattr, workspace, "_terminal_id", real)
        self.assertIn("pane id", workspace.source(cwd=str(config.ROOT)))


if __name__ == "__main__":
    unittest.main()
