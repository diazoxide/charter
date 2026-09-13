"""A selection naming a persona that does not exist is said, and still selects nothing (#1045).

`charter persona remove forge` deletes the definition and leaves every session and terminal
pointer that named it. `persona._resolved` returns the first rung that names anything, so
those sessions go on resolving `forge` through their pointer, and every rung below it, the
plane's declared default included, is hidden. Measured before: SessionStart injected no role
and said nothing, `persona current` printed `forge` / `resolved via session`, and `persona
create forge` a week later made every one of those stale pointers select the new persona
without a word.

**What resolves does not change, and the first class pins that.** A pointer records a choice
somebody made. Falling through to the plane default in its place would hand a session a
persona, with its tool grants and its vault, that nobody chose for it; staying on a name that
defines nothing fails toward fewer grants. What changes is that every surface reporting the
selection now says the name defines nothing, which rung holds it, and what drops it.
"""
from __future__ import annotations

import unittest

from charter import config, persona
from tests.test_persona_selection_scope import CHAT, InAChatIso, SelectionScopeIso


def _declare(name: str) -> None:
    (config.ROOT / "charter.toml").write_text(
        f'schema = 1\n\n[persona]\ndefault = "{name}"\n')


class StaleIso(SelectionScopeIso):
    """A plane whose declared front door is `steward`, where `forge` was selected and then
    removed through the real command."""

    def setUp(self) -> None:
        super().setUp()
        self.persona_("steward")
        _declare("steward")

    def _remove(self, name: str) -> None:
        import io
        from contextlib import redirect_stderr
        from types import SimpleNamespace
        from charter import commands_persona
        with redirect_stderr(io.StringIO()):
            rc = commands_persona.cmd_persona_remove(SimpleNamespace(name=name, force=False))
        self.assertEqual(rc, 0, f"precondition: `persona remove {name}` failed")

    def select_then_remove(self, **ids) -> None:
        self.persona_("forge")
        with self.env(**ids):
            persona.set_active("forge")
        self._remove("forge")


class TestAStalePointerStillSelectsNothing(StaleIso):
    """Pinned so nobody "fixes" the silence by falling through to the default."""

    def test_a_session_pointer_to_a_removed_persona_does_not_fall_through_to_the_default(self):
        self.select_then_remove(CHARTER_SESSION_ID="s1")
        with self.env(CHARTER_SESSION_ID="s1"):
            sel = persona.selection()
            active = persona.resolve_active()
        self.assertEqual(tuple(sel), ("forge", "session", False))
        self.assertEqual(active, "forge",
                         "a stale pointer fell through to a lower rung nobody chose")

    def test_a_terminal_pointer_is_named_as_the_terminal_s(self):
        self.select_then_remove(CHARTER_SESSION_ID="s1", TMUX_PANE="%7")
        with self.env(CHARTER_SESSION_ID="s2", TMUX_PANE="%7"):
            self.assertEqual(tuple(persona.selection()), ("forge", "terminal", False))

    def test_the_plane_wide_file_is_named_as_the_plane_wide_file(self):
        """`remove` clears the plane-wide file only when that file resolved the removed
        persona for the process running `remove`, so a file a bare shell wrote stays when
        `remove` runs in a session that selected something else."""
        self.persona_("forge")
        with self.env():
            persona.set_active("forge")
        with self.env(CHARTER_SESSION_ID="s1"):
            persona.set_active("steward")
            self._remove("forge")
        self.assertTrue(config.ACTIVE_PERSONA_FILE.exists(), "precondition: file was cleared")
        with self.env(CHARTER_SESSION_ID="s2"):
            self.assertEqual(tuple(persona.selection()), ("forge", "active-file", False))

    def test_charter_persona_naming_nothing_is_named_as_the_environment_s(self):
        with self.env(CHARTER_PERSONA="ghost", CHARTER_SESSION_ID="s1"):
            self.assertEqual(tuple(persona.selection()), ("ghost", "$CHARTER_PERSONA", False))

    def test_a_persona_that_exists_is_said_to(self):
        self.persona_("forge")
        with self.env(CHARTER_SESSION_ID="s1"):
            persona.set_active("forge")
            self.assertEqual(tuple(persona.selection()), ("forge", "session", True))

    def test_the_declared_default_exists_by_construction(self):
        with self.env(CHARTER_SESSION_ID="s1"):
            self.assertEqual(tuple(persona.selection()), ("steward", "charter.toml", True))

    def test_nothing_selected_is_not_a_missing_persona(self):
        (config.ROOT / "charter.toml").write_text("schema = 1\n")
        with self.env(CHARTER_SESSION_ID="s1"):
            self.assertEqual(tuple(persona.selection()), (None, "none", False))

    def test_a_pointer_holding_a_path_is_not_said_to_exist(self):
        """The pointer is a file under `.charter/`, so its content is whatever was written
        there. A value that is not a persona name must not pass for one because a file
        happens to sit at the path it spells."""
        self.persona_("forge")
        with self.env(CHARTER_SESSION_ID="s1"):
            persona.set_active("forge")
            (config.SESSIONS_DIR / "s1.persona").write_text("../personas/forge\n")
            self.assertFalse(persona.selection().exists)


#: What drops a pointer, everywhere but in a chat holding someone else's pointer.
USE_OR_CLEAR = ("`charter persona use <persona>` selects one that exists, or `charter "
                "persona clear` drops the selection")

#: In a chat, for a pointer that is not the chat's own: `clear` there drops only the chat's
#: session pointer (#1022), so offering it would send the reader to a command that does
#: nothing about this one.
USE_IN_A_CHAT = ("`charter persona use <persona>` selects one for this chat; `charter persona "
                 "clear` here drops only this chat's own selection, and this is not it")

#: `$CHARTER_PERSONA` outranks both pointers, so neither command moves it.
UNSET_THE_VARIABLE = ("unset `$CHARTER_PERSONA`, or set it to a persona that exists; it "
                      "outranks `charter persona use` and `charter persona clear`, so neither "
                      "moves it")


def _briefing(sid: str) -> list[str]:
    from charter import hooks
    return hooks._context_parts({"session_id": sid}, "PIECE-NOTE", live=False)


class TestSessionStartNamesIt(StaleIso):
    """Measured before: a session whose pointer named the removed `forge` was briefed with no
    persona block at all and no word about why, beside a declared default it never got."""

    def _note(self, parts: list[str]) -> str:
        hits = [p for p in parts if "no persona by that name exists" in p]
        self.assertEqual(len(hits), 1, parts)
        return hits[0]

    def test_a_session_pointer_is_named_with_both_ways_out(self):
        self.select_then_remove(CHARTER_SESSION_ID="s1")
        with self.env(CHARTER_SESSION_ID="s1"):
            parts = _briefing("s1")
        note = self._note(parts)
        self.assertIn("charter selected `forge` (via session)", note)
        self.assertIn(f"Ways out: {USE_OR_CLEAR}.", note)
        self.assertNotIn("steward", "\n\n".join(parts),
                         "a stale pointer's session was briefed as the declared default")

    def test_a_terminal_pointer_is_named_as_the_terminal_s(self):
        self.select_then_remove(CHARTER_SESSION_ID="s1", TMUX_PANE="%7")
        with self.env(CHARTER_SESSION_ID="s2", TMUX_PANE="%7"):
            note = self._note(_briefing("s2"))
        self.assertIn("charter selected `forge` (via terminal)", note)
        self.assertIn(f"Ways out: {USE_OR_CLEAR}.", note)

    def test_the_environment_is_named_and_the_pointer_commands_are_not_offered(self):
        with self.env(CHARTER_PERSONA="ghost", CHARTER_SESSION_ID="s1"):
            note = self._note(_briefing("s1"))
        self.assertIn("charter selected `ghost` (via $CHARTER_PERSONA)", note)
        self.assertIn(f"Ways out: {UNSET_THE_VARIABLE}.", note)
        self.assertNotIn(USE_OR_CLEAR, note)

    def test_a_persona_that_exists_gets_its_role_and_no_note(self):
        self.persona_("forge")
        with self.env(CHARTER_SESSION_ID="s1"):
            persona.set_active("forge")
            parts = _briefing("s1")
        self.assertTrue(any("You are the `forge` persona" in p for p in parts), parts)
        self.assertFalse(any("no persona by that name exists" in p for p in parts), parts)

    def test_nothing_selected_is_not_called_a_missing_persona(self):
        (config.ROOT / "charter.toml").write_text("schema = 1\n")
        with self.env(CHARTER_SESSION_ID="s1"):
            parts = _briefing("s1")
        self.assertFalse(any("no persona by that name exists" in p for p in parts), parts)

    def test_a_note_that_cannot_be_written_costs_only_itself(self):
        """`sessionstart` drops the whole briefing when `_context_parts` raises, so a failure
        composing this one line must not take the workspace gate and the piece with it."""
        from unittest import mock
        self.select_then_remove(CHARTER_SESSION_ID="s1")
        with self.env(CHARTER_SESSION_ID="s1"), \
                mock.patch.object(persona, "ways_out", side_effect=OSError("unreadable")):
            parts = _briefing("s1")
        self.assertIn("PIECE-NOTE", parts)
        self.assertFalse(any("no persona by that name exists" in p for p in parts), parts)
        self.assertNotIn("", parts, "an empty part became a blank block in the briefing")


def _run(fn, **kw) -> tuple[int, str, str]:
    import io
    from contextlib import redirect_stderr, redirect_stdout
    from types import SimpleNamespace
    out, err = io.StringIO(), io.StringIO()
    with redirect_stdout(out), redirect_stderr(err):
        rc = fn(SimpleNamespace(**kw))
    return rc, out.getvalue(), err.getvalue()


class TestCurrentAndListSayItIsMissing(StaleIso):
    """Measured before: `persona current` printed `forge` and `resolved via session`, and
    `persona list` headed its table `Active persona: forge  (via session)` above a table with
    no `forge` row — a bare name in both, for a persona that defines nothing."""

    def test_current_keeps_the_name_on_stdout_and_says_it_is_missing(self):
        """stdout stays the name the ladder resolved, which is what a script reading it has
        always been handed; what changed is that the reader is told it defines nothing."""
        from charter import commands_persona
        self.select_then_remove(CHARTER_SESSION_ID="s1")
        with self.env(CHARTER_SESSION_ID="s1"):
            rc, out, err = _run(commands_persona.cmd_persona_current)
        self.assertEqual(rc, 0, err)
        self.assertEqual(out, "forge\n")
        self.assertIn("resolved via session — no persona by that name exists, so no persona "
                      "is active, and the plane's default does not stand in for it.", err)
        self.assertIn(f"Ways out: {USE_OR_CLEAR}.", err)

    def test_current_for_a_persona_that_exists_is_unchanged(self):
        from charter import commands_persona
        self.persona_("forge")
        with self.env(CHARTER_SESSION_ID="s1"):
            persona.set_active("forge")
            rc, out, err = _run(commands_persona.cmd_persona_current)
        self.assertEqual((rc, out), (0, "forge\n"), err)
        self.assertIn("resolved via session\n", err)
        self.assertNotIn("Ways out", err)

    def test_list_heads_its_table_with_the_missing_name_and_the_ways_out(self):
        from charter import commands_persona
        self.select_then_remove(CHARTER_SESSION_ID="s1")
        with self.env(CHARTER_SESSION_ID="s1"):
            rc, out, err = _run(commands_persona.cmd_persona_list)
        self.assertEqual(rc, 0, err)
        self.assertIn("Active persona: forge  (via session — no persona by that name exists, "
                      "so no persona is active)\n", out)
        self.assertIn(f"Ways out: {USE_OR_CLEAR}.", out)

    def test_list_for_a_persona_that_exists_is_unchanged(self):
        from charter import commands_persona
        self.persona_("forge")
        with self.env(CHARTER_SESSION_ID="s1"):
            persona.set_active("forge")
            rc, out, err = _run(commands_persona.cmd_persona_list)
        self.assertIn("Active persona: forge  (via session)\n", out)
        self.assertNotIn("Ways out", out + err)


class TestClearOutsideAChatReadsBack(SelectionScopeIso):
    """`clear` outside a chat, part 2 of #1045.

    Measured before: with `charter.toml` declaring `steward` and a legacy `personas/.default`
    naming `forge`, `clear` printed "Resolves to the committed default 'forge' now
    (personas/.default)." while the shell resolved `steward` through `charter.toml`, which
    outranks the dotfile. And with `$CHARTER_PERSONA=forge` it printed "Active persona
    cleared." while `forge` went on deciding in that shell. It described a rung it had not
    read; now it reads back what the shell resolves to (ADR 0013), as a chat's clear does."""

    def setUp(self) -> None:
        super().setUp()
        self.persona_("steward")
        self.persona_("forge")
        self.persona_("ops")

    def _clear(self) -> tuple[int, str]:
        from charter import commands_persona
        rc, _out, err = _run(commands_persona.cmd_persona_clear)
        return rc, err

    def test_it_names_the_declared_default_that_outranks_the_legacy_dotfile(self):
        _declare("steward")
        (config.PERSONAS_DIR / ".default").write_text("forge\n")
        with self.env(CHARTER_SESSION_ID="s1", TMUX_PANE="%7"):
            persona.set_active("ops")
            rc, err = self._clear()
        self.assertEqual(rc, 0, err)
        self.assertIn("Active persona cleared.", err)
        self.assertIn("This shell now resolves to 'steward' (via charter.toml).", err)
        self.assertNotIn("forge", err, "clear named a rung the shell does not resolve by")

    def test_the_legacy_dotfile_is_named_as_itself_when_it_decides(self):
        (config.PERSONAS_DIR / ".default").write_text("forge\n")
        with self.env(CHARTER_SESSION_ID="s1"):
            persona.set_active("ops")
            rc, err = self._clear()
        self.assertIn("This shell now resolves to 'forge' (via committed-default).", err)

    def test_nothing_left_is_said_to_be_nothing(self):
        with self.env(CHARTER_SESSION_ID="s1"):
            persona.set_active("ops")
            rc, err = self._clear()
        self.assertEqual(rc, 0, err)
        self.assertIn("This shell now resolves to no persona.", err)

    def test_charter_persona_still_deciding_is_not_called_cleared(self):
        with self.env(CHARTER_PERSONA="forge", CHARTER_SESSION_ID="s1", TMUX_PANE="%7"):
            persona.set_active("ops")
            rc, err = self._clear()
        self.assertEqual(rc, 0, err)
        self.assertNotIn("Active persona cleared.", err)
        self.assertIn("Persona selection cleared, but $CHARTER_PERSONA outranks every "
                      "selection and still decides in this shell.", err)
        self.assertIn("This shell now resolves to 'forge' (via $CHARTER_PERSONA).", err)
        with self.env(CHARTER_SESSION_ID="s1", TMUX_PANE="%7"):
            self.assertIsNone(persona.resolve_active(), "the pointers were not cleared")

    def test_charter_persona_naming_nothing_is_read_back_as_missing(self):
        with self.env(CHARTER_PERSONA="ghost", CHARTER_SESSION_ID="s1"):
            rc, err = self._clear()
        self.assertIn("This shell now resolves to 'ghost' (via $CHARTER_PERSONA), where no "
                      "persona by that name exists, so no persona is active.", err)
        self.assertIn(f"Ways out: {UNSET_THE_VARIABLE}.", err)


class TestClearInAChatReadsBackAMissingPersona(InAChatIso):
    def test_the_launcher_s_pointer_to_a_removed_persona_is_read_back_as_missing(self):
        """#1044's readback, one case further: the chat's clear leaves the launcher's pointer,
        and when that pointer names a removed persona, "resolves to 'forge' (via terminal)"
        is a bare name for a persona that defines nothing."""
        from charter import commands_persona
        self.persona_("forge")
        with self.env(TERM_SESSION_ID=self.LAUNCHER):
            persona.set_active("forge")
        StaleIso._remove(self, "forge")
        with self.env(CHARTER_SESSION_ID=CHAT, TERM_SESSION_ID=self.LAUNCHER, TMUX_PANE="%9"):
            rc, _out, err = _run(commands_persona.cmd_persona_clear)
        self.assertEqual(rc, 0, err)
        self.assertIn("This chat now resolves to 'forge' (via terminal), where no persona by "
                      "that name exists, so no persona is active.", err)
        self.assertIn(f"Ways out: {USE_IN_A_CHAT}.", err)


class TestCreateSaysWhichSelectionsItRevives(StaleIso):
    """Measured before: after `remove forge`, `create forge` printed only that it created
    `forge`, and every session and terminal whose pointer still named it resolved to the new
    persona — its tools and its vault — without anybody choosing it again."""

    def _create(self, name: str, **extra) -> tuple[int, str]:
        from charter import commands_persona
        kw = dict(name=name, role=name.title(), vault=None, extends=None, with_vault=False,
                  use=False, force=False, delegate_when="work")
        kw.update(extra)
        rc, _out, err = _run(commands_persona.cmd_persona_create, **kw)
        return rc, err

    def test_it_counts_the_pointers_that_name_it_by_rung(self):
        self.persona_("forge")
        with self.env(CHARTER_SESSION_ID="s1", TMUX_PANE="%7"):
            persona.set_active("forge")
        with self.env(CHARTER_SESSION_ID="s2"):
            persona.set_active("forge")
        with self.env():
            persona.set_active("forge")
        with self.env(CHARTER_SESSION_ID="s3"):
            persona.set_active("steward")
            self._remove("forge")
            rc, err = self._create("forge")
        self.assertEqual(rc, 0, err)
        self.assertIn("4 selection(s) already named 'forge' before it existed, and now select "
                      "it: 2 session pointer(s), 1 terminal pointer(s), the plane-wide "
                      ".charter/active-persona. Nobody chose it again: those sessions and "
                      "terminals now resolve to this persona.", err)

    def test_a_single_rung_is_counted_alone(self):
        self.select_then_remove(CHARTER_SESSION_ID="s1")
        with self.env(CHARTER_SESSION_ID="s9"):
            rc, err = self._create("forge")
        self.assertIn("1 selection(s) already named 'forge' before it existed, and now select "
                      "it: 1 session pointer(s).", err)

    def test_a_name_no_pointer_holds_says_nothing(self):
        """Pointers naming other personas, on every rung, are not this name's."""
        self.select_then_remove(CHARTER_SESSION_ID="s1", TMUX_PANE="%7")
        with self.env():
            persona.set_active("steward")
        with self.env(CHARTER_SESSION_ID="s9"):
            rc, err = self._create("fresh")
        self.assertEqual(rc, 0, err)
        self.assertNotIn("already named", err)

    def test_overwriting_a_persona_that_exists_revives_nothing(self):
        """`--force` over a live persona: its pointers were never stale, so "before it
        existed" would be false."""
        self.persona_("forge")
        with self.env(CHARTER_SESSION_ID="s1"):
            persona.set_active("forge")
            rc, err = self._create("forge", force=True)
        self.assertEqual(rc, 0, err)
        self.assertNotIn("already named", err)


class TestSessionStartInAChat(InAChatIso):
    def test_a_pointer_the_chat_did_not_write_is_not_offered_to_clear_from_the_chat(self):
        """The chat inherited the launcher's `$TERM_SESSION_ID`, so the launcher's pointer to
        the removed `forge` is what the chat resolves. `clear` in a chat drops only the chat's
        own pointer (#1022), so telling the chat to run it would change nothing."""
        self.persona_("forge")
        with self.env(TERM_SESSION_ID=self.LAUNCHER):
            persona.set_active("forge")
        StaleIso._remove(self, "forge")
        with self.env(CHARTER_SESSION_ID=CHAT, TERM_SESSION_ID=self.LAUNCHER, TMUX_PANE="%9"):
            parts = _briefing(CHAT)
        note = next(p for p in parts if "no persona by that name exists" in p)
        self.assertIn("charter selected `forge` (via terminal)", note)
        self.assertIn(f"Ways out: {USE_IN_A_CHAT}.", note)

    def test_the_chat_s_own_pointer_is_offered_to_clear(self):
        self.persona_("forge")
        with self.env(CHARTER_SESSION_ID=CHAT, TMUX_PANE="%9"):
            self._use("forge")
        StaleIso._remove(self, "forge")
        with self.env(CHARTER_SESSION_ID=CHAT, TMUX_PANE="%9"):
            parts = _briefing(CHAT)
        note = next(p for p in parts if "no persona by that name exists" in p)
        self.assertIn(f"Ways out: {USE_OR_CLEAR}.", note)


if __name__ == "__main__":
    unittest.main()
