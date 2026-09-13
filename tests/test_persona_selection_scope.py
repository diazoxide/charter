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

class TestCreateUseReportsItTheSameWay(SelectionScopeIso):
    def test_create_use_says_how_far_the_selection_reaches(self):
        """`create --use` and `use` select the same way, so they must report the same way —
        two commands describing one act differently is how a reader learns to distrust both."""
        import io
        from contextlib import redirect_stderr
        from types import SimpleNamespace
        from charter import commands_persona
        args = SimpleNamespace(name="fresh", role="Fresh", vault=None, extends=None,
                               with_vault=False, use=True, force=False,
                               delegate_when="fresh work")
        buf = io.StringIO()
        with self.env(CHARTER_SESSION_ID="s1"), redirect_stderr(buf):
            commands_persona.cmd_persona_create(args)
        self.assertIn("this session only", buf.getvalue().lower())


#: The chat, as the launcher names it, and what `$CHARTER_SESSION_ID` holds inside it.
CHAT = "north.1"


def _plant_chat(fid: str, *, ws: str = "north") -> None:
    """Make *fid* a chat charter launched into *ws*, through the production writers.

    `tests/test_a_chat_is_locked_to_the_workspace_it_was_launched_for._plant`'s shape, and
    for its reason: a hand-written frame directory would measure the fixture, not the record
    `workspace.launch_lock` reads to decide that this process is a chat.
    """
    from charter.frame import state
    state.frame_dir(fid, create=True)
    state.record_workspace(fid, ws)
    state.record_identity(fid, {"CHARTER_HARNESS": "Claude Code",
                                "CHARTER_WORKSPACE": "", "CHARTER_PERSONA": ""})


def _terminal_pointers() -> list[str]:
    return (sorted(p.name for p in config.TERMINALS_DIR.glob("*.persona"))
            if config.TERMINALS_DIR.is_dir() else [])


class InAChatIso(SelectionScopeIso):
    """A plane holding `ops` and the chat `north.1`, and a way to run a command and read what
    it said. `TestUseInsideAChat` and `TestClearInsideAChat` stand in the same chat."""

    #: What a harness inherits when `charter claude` is typed in Terminal.app or iTerm2 without
    #: tmux: `commands_frame._frame_env` strips `TMUX` and `TMUX_PANE` and nothing else of this
    #: kind, and `session.terminal` ranks this variable first.
    LAUNCHER = "w0t0p0:0A1B2C3D-0953"

    def setUp(self) -> None:
        super().setUp()
        self.persona_("ops")
        _plant_chat(CHAT)

    def _run(self, fn, **kw) -> tuple[int, str]:
        import io
        from contextlib import redirect_stderr
        from types import SimpleNamespace
        buf = io.StringIO()
        with redirect_stderr(buf):
            rc = fn(SimpleNamespace(**kw))
        return rc, buf.getvalue()

    def _use(self, name: str) -> tuple[int, str]:
        from charter import commands_persona
        return self._run(commands_persona.cmd_persona_use, name=name)


class TestUseInsideAChat(InAChatIso):
    """`charter persona use`, typed in a chat's shell or run by a sub-agent inside it (#953).

    A chat speaks for no terminal, which `workspace use` settled for the same pointer one
    noun over (#936) and `frame/switch.py` settled for this very pointer (#411). Every id
    `session.terminal` could key on inside a frame names something other than this chat: a
    tmux pane number the next server hands to an unrelated chat, or the `$TERM_SESSION_ID`
    of the terminal that launched the frame, inherited by every pane on its server.
    """

    def test_a_chat_on_a_pane_writes_no_pointer_the_next_server_s_pane_reads(self):
        """Measured before: chat `north.1` on pane `%9` wrote `terminals/-9.persona` = `ops`,
        and a chat on a later server that drew pane `%9` again resolved `ops` through the
        terminal rung. tmux numbers panes per server, and charter's private server exits with
        its last frame, so that second chat is ordinary."""
        with self.env(CHARTER_SESSION_ID=CHAT, TMUX_PANE="%9"):
            rc, err = self._use("ops")
            here = persona.resolve_active()
        self.assertEqual(rc, 0, err)
        self.assertEqual(here, "ops", "the chat's own commands stopped following the choice")
        self.assertEqual(_terminal_pointers(), [], "a chat wrote a terminal pointer")
        _plant_chat("south.1", ws="south")
        with self.env(CHARTER_SESSION_ID="south.1", TMUX_PANE="%9"):
            self.assertIsNone(persona.resolve_active(),
                              "a chat that never chose a persona resolved another chat's")

    def test_the_sentence_says_the_choice_is_this_chat_s_alone(self):
        """Measured before: "for this terminal (kept across closing/reopening Claude)", beside
        a pointer no new chat reads. Writing no terminal pointer alone turned it into "this
        terminal reports no pane id, so a new session starts with no persona", which is false
        in a chat too: the pane has an id, and that is not why a new chat does not have the
        choice. `workspace use` says the same thing about the same write (#936)."""
        self.persona_("steward")
        (config.ROOT / "charter.toml").write_text(
            'schema = 1\n\n[persona]\ndefault = "steward"\n')
        with self.env(CHARTER_SESSION_ID=CHAT, TMUX_PANE="%9"):
            rc, err = self._use("ops")
        self.assertEqual(rc, 0, err)
        self.assertIn("Active persona set to 'ops' for this chat only — a new chat does not "
                      "inherit it.", err)
        self.assertNotIn("kept across closing/reopening", err)
        self.assertNotIn("reports no pane id", err)
        # Not "a new chat starts as 'steward'": a chat opened with `--persona`, or by a
        # handoff that names one, starts as that, so the plane's default is not a promise.
        self.assertNotIn("steward", err)

    def test_a_chat_leaves_the_terminal_that_launched_it_alone(self):
        """Measured before: with the launcher's `$TERM_SESSION_ID` inherited, `use ops` here
        wrote `terminals/<launcher>.persona`, and both another chat on the same server and the
        launching shell itself then resolved `ops`. That shell had chosen `forge` for itself."""
        self.persona_("forge")
        with self.env(TERM_SESSION_ID=self.LAUNCHER):
            self.assertEqual(persona.set_active("forge"), "terminal",
                             "the launcher's own pointer was not planted")
        with self.env(CHARTER_SESSION_ID=CHAT, TERM_SESSION_ID=self.LAUNCHER, TMUX_PANE="%9"):
            rc, err = self._use("ops")
        self.assertEqual(rc, 0, err)
        with self.env(TERM_SESSION_ID=self.LAUNCHER):
            self.assertEqual(persona.resolve_active(), "forge",
                             "a chat moved the choice of the terminal that launched it")
        _plant_chat("south.1", ws="south")
        with self.env(CHARTER_SESSION_ID="south.1", TERM_SESSION_ID=self.LAUNCHER,
                      TMUX_PANE="%12"):
            self.assertNotEqual(persona.resolve_active(), "ops",
                                "another chat on the same server adopted this chat's choice")

    def test_create_use_in_a_chat_selects_the_same_way(self):
        """`create --use` goes through the same `set_active`, so it had the same leak and the
        same sentence; two commands describing one act differently is how a reader learns to
        distrust both."""
        from charter import commands_persona
        with self.env(CHARTER_SESSION_ID=CHAT, TMUX_PANE="%9"):
            rc, err = self._run(commands_persona.cmd_persona_create,
                                name="fresh", role="Fresh", vault=None, extends=None,
                                with_vault=False, use=True, force=False,
                                delegate_when="fresh work")
            here = persona.resolve_active()
        self.assertEqual(rc, 0, err)
        self.assertEqual(here, "fresh")
        self.assertEqual(_terminal_pointers(), [], "a chat wrote a terminal pointer")
        self.assertIn("Active persona set to 'fresh' for this chat only — a new chat does not "
                      "inherit it.", err)


class TestUseOutsideAChatIsUnchanged(SelectionScopeIso):
    """The other side of #953's line. A session id that names no chat is a harness in an
    ordinary pane, and its pane pointer is the whole point of #255: it is what survives
    closing and reopening Claude there. `test_a_pane_selection_says_it_survives_a_restart`
    asserts only the word "terminal", which the no-pane-id sentence also contains, so a fix
    that wrote no terminal pointer for anyone would have passed it."""

    def test_a_session_in_an_ordinary_pane_still_writes_its_terminal_pointer(self):
        import io
        from contextlib import redirect_stderr
        from types import SimpleNamespace
        from charter import commands_persona
        self.persona_("forge")
        buf = io.StringIO()
        with self.env(CHARTER_SESSION_ID="s1", TMUX_PANE="%7"), redirect_stderr(buf):
            rc = commands_persona.cmd_persona_use(SimpleNamespace(name="forge"))
        self.assertEqual(rc, 0, buf.getvalue())
        self.assertEqual(_terminal_pointers(), ["-7.persona"])
        self.assertIn("Active persona set to 'forge' for this terminal (kept across "
                      "closing/reopening Claude).", buf.getvalue())
        with self.env(CHARTER_SESSION_ID="s2", TMUX_PANE="%7"):
            self.assertEqual(persona.resolve_active(), "forge")


class TestClearInsideAChat(InAChatIso):
    """`charter persona clear` in a chat, the reverse of #953 (#1022).

    `use` there stopped writing a terminal pointer, and `clear` kept deleting one: the pointer
    keyed on the `$TERM_SESSION_ID` the chat inherited from the terminal that launched its
    frame. So a chat erased a choice that terminal made, somewhere else, and never wrote.
    """

    def _clear(self) -> tuple[int, str]:
        from charter import commands_persona
        return self._run(commands_persona.cmd_persona_clear)

    def _launcher_chooses(self, name: str) -> None:
        self.persona_(name)
        with self.env(TERM_SESSION_ID=self.LAUNCHER):
            self.assertEqual(persona.set_active(name), "terminal",
                             "the launcher's own pointer was not planted")

    def test_a_chat_leaves_the_selection_of_the_terminal_that_launched_it(self):
        """Measured before: the launcher ran `use forge`, a chat that inherited its
        `$TERM_SESSION_ID` ran `clear`, and the launcher's terminal then resolved `None`."""
        self._launcher_chooses("forge")
        in_the_chat = dict(CHARTER_SESSION_ID=CHAT, TERM_SESSION_ID=self.LAUNCHER,
                           TMUX_PANE="%9")
        with self.env(**in_the_chat):
            self._use("ops")
            rc, err = self._clear()
            here = persona.resolve_active()
        self.assertEqual(rc, 0, err)
        with self.env(TERM_SESSION_ID=self.LAUNCHER):
            self.assertEqual(persona.resolve_active(), "forge",
                             "a chat's clear erased the choice of the terminal that launched it")
        self.assertIsNone(persona.for_session(CHAT), "the chat's own selection survived clear")
        self.assertEqual(here, "forge")

    def test_a_chat_leaves_the_plane_wide_selection_a_bare_shell_made(self):
        """`.charter/active-persona` is written only by a shell with no session id and no pane
        id, and a chat always has a session id, so that file is never the chat's selection
        either. Deleting it from a chat is the same erasure one rung lower."""
        self.persona_("forge")
        with self.env():
            self.assertEqual(persona.set_active("forge"), "plane",
                             "the bare shell's plane-wide file was not planted")
        with self.env(CHARTER_SESSION_ID=CHAT, TMUX_PANE="%9"):
            self._use("ops")
            rc, err = self._clear()
        self.assertEqual(rc, 0, err)
        with self.env():
            self.assertEqual(persona.resolve_active(), "forge",
                             "a chat's clear erased the plane-wide selection")

    def test_the_sentence_says_this_chat_s_selection_was_cleared_and_what_it_resolves_to(self):
        """"Active persona cleared." was true when clear dropped every rung. In a chat it drops
        one, and the chat goes on resolving the launcher's `forge` through the terminal rung,
        so the bare sentence is the lie `persona.clear_active` warns about, exposed by the
        next status line. Say whose selection went, and read back what is left (ADR 0013)."""
        self._launcher_chooses("forge")
        with self.env(CHARTER_SESSION_ID=CHAT, TERM_SESSION_ID=self.LAUNCHER, TMUX_PANE="%9"):
            self._use("ops")
            rc, err = self._clear()
        self.assertEqual(rc, 0, err)
        self.assertIn("Active persona cleared for this chat only — other chats and terminals "
                      "keep theirs.", err)
        self.assertIn("This chat now resolves to 'forge' (via terminal).", err)
        self.assertNotIn("Active persona cleared.", err)

    def test_a_chat_with_no_selection_of_its_own_is_told_nothing_was_cleared(self):
        """A chat that never ran `use` has no pointer for `clear` to drop, so "cleared" would
        report a write that did not happen."""
        with self.env(CHARTER_SESSION_ID=CHAT, TMUX_PANE="%9"):
            rc, err = self._clear()
        self.assertEqual(rc, 0, err)
        self.assertIn("This chat had no persona selection of its own, so nothing was cleared.",
                      err)
        self.assertNotIn("cleared for this chat", err)
        self.assertIn("This chat now resolves to no persona.", err)

    def test_removing_the_persona_the_launcher_chose_leaves_the_launcher_s_pointer(self):
        """#1022 named `persona remove` as the same path, and it is not quite: `remove` clears
        only when the removed persona resolved from the plane-wide file, so the launcher's
        pointer never reaches `clear_active` from here. Pinned so that a change routing remove
        through clear, in the name of consistency, cannot start deleting it. The pointer now
        names a persona that no longer exists, and what to do about that is not a chat's
        decision to make."""
        self._launcher_chooses("forge")
        with self.env(CHARTER_SESSION_ID=CHAT, TERM_SESSION_ID=self.LAUNCHER, TMUX_PANE="%9"):
            from charter import commands_persona
            rc, err = self._run(commands_persona.cmd_persona_remove, name="forge", force=False)
        self.assertEqual(rc, 0, err)
        self.assertEqual(len(_terminal_pointers()), 1,
                         "a chat's remove deleted the pointer of the terminal that launched it")


class TestClearOutsideAChatIsUnchanged(SelectionScopeIso):
    """The other side of #1022's line: a harness in an ordinary pane still clears every rung,
    because there the terminal pointer is its own (#255)."""

    def test_a_session_in_an_ordinary_pane_still_clears_its_terminal_pointer(self):
        import io
        from contextlib import redirect_stderr
        from types import SimpleNamespace
        from charter import commands_persona
        self.persona_("forge")
        buf = io.StringIO()
        with self.env(CHARTER_SESSION_ID="s1", TMUX_PANE="%7"):
            persona.set_active("forge")
            with redirect_stderr(buf):
                rc = commands_persona.cmd_persona_clear(SimpleNamespace())
        self.assertEqual(rc, 0, buf.getvalue())
        self.assertEqual(_terminal_pointers(), [])
        self.assertIn("Active persona cleared.", buf.getvalue())
        with self.env(CHARTER_SESSION_ID="s2", TMUX_PANE="%7"):
            self.assertIsNone(persona.resolve_active())

if __name__ == "__main__":
    unittest.main()
