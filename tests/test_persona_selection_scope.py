"""Selecting a persona is per-session and per-terminal, exactly like a workspace.

`charter persona use` used to write ONE file for the whole plane
(`.charter/active-persona`), so choosing `forge` in one pane silently changed the persona
in every other pane and in every future session — while workspaces, whose pointers this
mirrors, have kept a per-session *and* a per-terminal pointer for precisely that reason.
Parallel work with different personas is the feature; sharing one identity behind the
user's back is the failure this closes (#255).

The legacy plane-wide file keeps resolving one rung lower, so nobody's existing selection
disappears on upgrade.
"""
from __future__ import annotations

import os
import unittest
from unittest import mock

from tests._isolation import PersonaIso
from charter import config, persona

_SCOPE_VARS = ("CHARTER_PERSONA", "CHARTER_SESSION_ID", "CLAUDE_CODE_SESSION_ID",
               "TERM_SESSION_ID", "TMUX_PANE", "STY", "SSH_TTY")


class SelectionScopeIso(PersonaIso):
    def env(self, **kw):
        """A pristine environment: no persona override, and only the scope ids given.

        `os.ttyname` is neutralised alongside them. `_terminal_id` falls back to the
        controlling tty when no pane variable is set, so a suite run from a terminal would
        find a pane id and one run in CI would not — the machine-is-not-the-runner trap
        CONTRIBUTING describes, and it would make every "no terminal pointer" assertion
        below pass locally and fail on the runner.
        """
        keep = {k: v for k, v in os.environ.items() if k not in _SCOPE_VARS}
        keep.update(kw)
        env = mock.patch.dict(os.environ, keep, clear=True)
        tty = mock.patch("os.ttyname", side_effect=OSError("no tty"))
        return _Both(env, tty)

    def persona_(self, name: str) -> str:
        return self.make_persona(name, role=name.title(), vault="none")


class _Both:
    """Two context managers as one — `with self.env(...)` reads better than a nest."""

    def __init__(self, *cms):
        self.cms = cms

    def __enter__(self):
        for c in self.cms:
            c.__enter__()
        return self

    def __exit__(self, *exc):
        for c in reversed(self.cms):
            c.__exit__(*exc)
        return False


class TestPerSessionSelection(SelectionScopeIso):
    def test_a_selection_is_scoped_to_the_session_that_made_it(self):
        self.persona_("forge")
        with self.env(CHARTER_SESSION_ID="s1"):
            persona.set_active("forge")
            self.assertEqual(persona.resolve_active(), "forge")
        with self.env(CHARTER_SESSION_ID="s2"):
            self.assertIsNone(persona.resolve_active())

    def test_the_source_names_the_session(self):
        self.persona_("forge")
        with self.env(CHARTER_SESSION_ID="s1"):
            persona.set_active("forge")
            self.assertEqual(persona.source(), "session")

    def test_clearing_only_clears_this_session(self):
        self.persona_("forge")
        self.persona_("release")
        with self.env(CHARTER_SESSION_ID="s1"):
            persona.set_active("forge")
        with self.env(CHARTER_SESSION_ID="s2"):
            persona.set_active("release")
            persona.clear_active()
            self.assertIsNone(persona.resolve_active())
        with self.env(CHARTER_SESSION_ID="s1"):
            self.assertEqual(persona.resolve_active(), "forge")


class TestPerTerminalSelection(SelectionScopeIso):
    def test_a_pane_keeps_its_persona_across_sessions(self):
        """The reason a terminal pointer exists at all: a pane survives closing Claude."""
        self.persona_("forge")
        with self.env(CHARTER_SESSION_ID="s1", TMUX_PANE="%7"):
            persona.set_active("forge")
        with self.env(CHARTER_SESSION_ID="s2", TMUX_PANE="%7"):
            self.assertEqual(persona.resolve_active(), "forge")
            self.assertEqual(persona.source(), "terminal")

    def test_another_pane_is_unaffected(self):
        self.persona_("forge")
        with self.env(CHARTER_SESSION_ID="s1", TMUX_PANE="%7"):
            persona.set_active("forge")
        with self.env(CHARTER_SESSION_ID="s2", TMUX_PANE="%9"):
            self.assertIsNone(persona.resolve_active())

    def test_the_session_pointer_outranks_the_pane(self):
        self.persona_("forge")
        self.persona_("release")
        with self.env(CHARTER_SESSION_ID="s1", TMUX_PANE="%7"):
            persona.set_active("forge")
        with self.env(CHARTER_SESSION_ID="s2", TMUX_PANE="%7"):
            persona.set_active("release")
            self.assertEqual(persona.resolve_active(), "release")

    def test_no_pane_id_writes_no_terminal_pointer(self):
        """An id that is wrong in the sharing direction is worse than no id."""
        self.persona_("forge")
        with self.env(CHARTER_SESSION_ID="s1"):
            persona.set_active("forge")
        self.assertFalse(any(config.TERMINALS_DIR.glob("*.persona"))
                         if config.TERMINALS_DIR.exists() else False)


class TestLegacyPlaneWideFile(SelectionScopeIso):
    def test_it_still_resolves_when_no_pointer_exists(self):
        """Fail toward no change: an upgrade must not drop someone's current selection."""
        self.persona_("forge")
        config.STATE_DIR.mkdir(parents=True, exist_ok=True)
        config.ACTIVE_PERSONA_FILE.write_text("forge\n")
        with self.env(CHARTER_SESSION_ID="s1"):
            self.assertEqual(persona.resolve_active(), "forge")
            self.assertEqual(persona.source(), "active-file")

    def test_a_session_pointer_outranks_it(self):
        self.persona_("forge")
        self.persona_("release")
        config.STATE_DIR.mkdir(parents=True, exist_ok=True)
        config.ACTIVE_PERSONA_FILE.write_text("forge\n")
        with self.env(CHARTER_SESSION_ID="s1"):
            persona.set_active("release")
            self.assertEqual(persona.resolve_active(), "release")

    def test_it_outranks_the_declared_default(self):
        self.persona_("forge")
        self.persona_("steward")
        (config.ROOT / "charter.toml").write_text(
            'schema = 1\n\n[persona]\ndefault = "steward"\n')
        config.STATE_DIR.mkdir(parents=True, exist_ok=True)
        config.ACTIVE_PERSONA_FILE.write_text("forge\n")
        with self.env(CHARTER_SESSION_ID="s1"):
            self.assertEqual(persona.resolve_active(), "forge")

    def test_clearing_removes_it_too(self):
        """Otherwise `clear` would leave the plane-wide file quietly deciding again."""
        self.persona_("forge")
        config.STATE_DIR.mkdir(parents=True, exist_ok=True)
        config.ACTIVE_PERSONA_FILE.write_text("forge\n")
        with self.env(CHARTER_SESSION_ID="s1"):
            persona.clear_active()
            self.assertIsNone(persona.resolve_active())


class TestWithNoSessionId(SelectionScopeIso):
    def test_a_selection_with_no_session_and_no_pane_still_works(self):
        """A bare shell (no harness session id, no pane) must still be able to choose —
        it falls back to the plane-wide file, which is what that file is now for."""
        self.persona_("forge")
        with self.env():
            persona.set_active("forge")
            self.assertEqual(persona.resolve_active(), "forge")


class TestUseReportsItsReach(SelectionScopeIso):
    """`persona use` must say how long the selection lasts, because the three answers
    differ and the reader who is not told goes looking for a bug the next time the status
    line disagrees with what they chose."""

    def _use(self, name: str) -> str:
        import io
        from contextlib import redirect_stderr
        from types import SimpleNamespace
        from charter import commands_persona
        buf = io.StringIO()
        with redirect_stderr(buf):
            commands_persona.cmd_persona_use(SimpleNamespace(name=name))
        return buf.getvalue()

    def test_a_pane_selection_says_it_survives_a_restart(self):
        self.persona_("forge")
        with self.env(CHARTER_SESSION_ID="s1", TMUX_PANE="%7"):
            out = self._use("forge").lower()
        self.assertIn("terminal", out)

    def test_without_a_pane_it_says_the_selection_dies_with_the_session(self):
        self.persona_("forge")
        with self.env(CHARTER_SESSION_ID="s1"):
            out = self._use("forge").lower()
        self.assertIn("this session only", out)

    def test_without_a_pane_it_names_what_a_new_session_would_start_as(self):
        """The declared default is what the next session gets — name it, or the reader
        cannot tell whether the fallback is a persona or nothing at all."""
        self.persona_("forge")
        self.persona_("steward")
        (config.ROOT / "charter.toml").write_text(
            'schema = 1\n\n[persona]\ndefault = "steward"\n')
        with self.env(CHARTER_SESSION_ID="s1"):
            out = self._use("forge")
        self.assertIn("steward", out)

if __name__ == "__main__":
    unittest.main()
