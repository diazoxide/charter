"""A `$CHARTER_PERSONA` holding only whitespace names no persona, so the rungs below it decide
(#1048).

`persona._resolved` took the variable as the top rung whenever it was set, and stripped it
only after deciding it had a name. `export CHARTER_PERSONA=$(…)` over a command that printed
only a space or a tab, or a stray space in an rc file, is set and names nothing (a
substitution drops its trailing newlines, so a command printing only a newline leaves the
variable empty, which was already unset). Measured before,
on a plane whose `charter.toml` declares `steward`: the session resolved to `""` via
`$CHARTER_PERSONA`, which hid the session pointer, the terminal pointer, the plane-wide file
and the declared default. SessionStart gave it no role, the status line drew `◆ persona
none`, `persona use` warned that `' '` takes precedence, and `persona current` printed
`(none)` / `resolved via $CHARTER_PERSONA` without saying the variable was blank.

**Empty was already unset, and that is the rule being extended, not a new one.**
`commands_frame._frame_identity_env` launches every chat with `CHARTER_PERSONA=` when the
launch pinned nothing, and every reader tests the value for truth. `session.current` strips
`$CHARTER_SESSION_ID` and treats what is left empty as no id, and `frame.switch._pin` already
treated a whitespace-only launch pin as no pin — so before this, the frame's switcher said a
blank-pinned frame could switch while the panel it repainted went on resolving `""`.

A name inside whitespace (`" forge "`) is that name, as a pointer file's content is: every
reader of a persona name in charter strips it.
"""
from __future__ import annotations

import unittest

from charter import config, persona
from tests.test_a_stale_persona_pointer_is_named import StaleIso, _briefing, _run

#: Values a shell leaves in the variable that are set and name nothing.
BLANKS = (" ", "\t", "\n", " \t\n ")

#: What `persona current` says when it ignored one.
IGNORED = ("$CHARTER_PERSONA is set but holds only whitespace, so charter ignored it and the "
           "rungs below it decided. Unset it, or set it to the persona you meant.")


class TestABlankVariableLeavesTheRungsBelowDeciding(StaleIso):
    """`StaleIso` declares `steward` in `charter.toml`."""

    def test_a_space_leaves_the_declared_default_deciding(self):
        with self.env(CHARTER_PERSONA=" ", CHARTER_SESSION_ID="s1"):
            self.assertEqual(tuple(persona.selection()), ("steward", "charter.toml", True))
            self.assertEqual(persona.resolve_active(), "steward")
            self.assertEqual(persona.source(), "charter.toml")

    def test_a_tab_or_a_newline_is_blank_too(self):
        for blank in BLANKS:
            with self.subTest(blank=blank), \
                    self.env(CHARTER_PERSONA=blank, CHARTER_SESSION_ID="s1"):
                self.assertEqual(tuple(persona.selection()),
                                 ("steward", "charter.toml", True))

    def test_a_blank_variable_does_not_hide_a_session_pointer(self):
        self.persona_("forge")
        with self.env(CHARTER_SESSION_ID="s1"):
            persona.set_active("forge")
        with self.env(CHARTER_PERSONA="\n", CHARTER_SESSION_ID="s1"):
            self.assertEqual(tuple(persona.selection()), ("forge", "session", True))

    def test_a_name_inside_whitespace_is_that_name(self):
        """Pinned rather than changed: the variable has always been stripped, like a pointer
        file's content, and it still outranks every rung below it."""
        self.persona_("forge")
        with self.env(CHARTER_PERSONA=" forge\n", CHARTER_SESSION_ID="s1"):
            self.assertEqual(tuple(persona.selection()), ("forge", "$CHARTER_PERSONA", True))


class TestCurrentSaysItIgnoredTheBlank(StaleIso):

    def _current(self, **env) -> tuple[str, str]:
        from charter import commands_persona
        with self.env(CHARTER_SESSION_ID="s1", **env):
            rc, out, err = _run(commands_persona.cmd_persona_current)
        self.assertEqual(rc, 0, err)
        return out, err

    def test_a_blank_variable_is_named_beside_the_rung_that_decided(self):
        out, err = self._current(CHARTER_PERSONA=" ")
        self.assertEqual(out, "steward\n")
        self.assertIn("resolved via charter.toml\n", err)
        self.assertIn(IGNORED, err)

    def test_it_is_said_beside_a_missing_persona_too(self):
        """A blank variable and a stale pointer are two things wrong with one shell, and the
        note about the pointer must not be the only one the operator reads."""
        self.select_then_remove(CHARTER_SESSION_ID="s1")
        out, err = self._current(CHARTER_PERSONA="\t")
        self.assertEqual(out, "forge\n")
        self.assertIn("resolved via session — no persona by that name exists", err)
        self.assertIn(IGNORED, err)

    def test_an_empty_variable_is_not_reported(self):
        """Empty is what a frame launches every chat with when nothing was pinned
        (`commands_frame._frame_identity_env`), so saying it was ignored would be said in
        every chat, about an export nobody wrote."""
        out, err = self._current(CHARTER_PERSONA="")
        self.assertEqual(out, "steward\n")
        self.assertNotIn("whitespace", err)

    def test_a_name_inside_whitespace_is_not_reported(self):
        self.persona_("forge")
        out, err = self._current(CHARTER_PERSONA=" forge ")
        self.assertEqual(out, "forge\n")
        self.assertIn("resolved via $CHARTER_PERSONA\n", err)
        self.assertNotIn("whitespace", err)


class TestEveryReaderAgrees(StaleIso):
    """The surfaces that read `$CHARTER_PERSONA`, each measured before the fix."""

    def test_session_start_briefs_the_persona_the_rungs_below_chose(self):
        """Measured before: no persona block at all, for a plane that declares `steward`."""
        with self.env(CHARTER_PERSONA=" ", CHARTER_SESSION_ID="s1"):
            parts = _briefing("s1")
        joined = "\n\n".join(parts)
        self.assertIn("You are the `steward` persona for this session", joined)
        self.assertIn("(via charter.toml)", joined)

    def test_the_status_line_draws_the_persona_the_rungs_below_chose(self):
        """Measured before: `◆ persona none`, beside a roster holding `steward`."""
        from charter import statusline, tui
        with self.env(CHARTER_PERSONA="\n", CHARTER_SESSION_ID="s1"):
            line = statusline._persona_line_parts()
        self.assertIsNotNone(line)
        self.assertEqual(tui.strip_ansi(line.head), "◆ steward")

    def test_use_is_not_told_a_blank_variable_takes_precedence(self):
        """Measured before: `$CHARTER_PERSONA=' ' is set and takes precedence — commands use
        ' ', not 'forge'.`, beside a selection that did decide."""
        from charter import commands_persona
        self.persona_("forge")
        with self.env(CHARTER_PERSONA=" ", CHARTER_SESSION_ID="s1"):
            rc, _out, err = _run(commands_persona.cmd_persona_use, name="forge")
            self.assertEqual(persona.resolve_active(), "forge")
        self.assertEqual(rc, 0, err)
        self.assertNotIn("takes precedence", err)

    def test_use_under_a_real_name_still_says_the_name_that_decides(self):
        from charter import commands_persona
        self.persona_("forge")
        with self.env(CHARTER_PERSONA=" forge ", CHARTER_SESSION_ID="s1"):
            rc, _out, err = _run(commands_persona.cmd_persona_use, name="steward")
        self.assertEqual(rc, 0, err)
        self.assertIn("takes precedence — commands use 'forge', not 'steward'.", err)

    def test_the_frame_switcher_and_the_panel_it_repaints_agree_on_a_blank_pin(self):
        """`frame.switch._pin` reads the launch record, and already called a blank pin no
        pin, so it allowed the switch. The panel resolves through `persona.selection` with
        the same blank in its environment, and went on resolving `""` over the pointer the
        switch had just written."""
        from charter.frame import state, switch
        self.persona_("forge")
        fid = "f-blank"
        state.frame_dir(fid, create=True)
        state.record_identity(fid, {"CHARTER_SESSION_ID": fid, "CHARTER_PERSONA": " "})
        with self.env(CHARTER_SESSION_ID=fid, CHARTER_PERSONA=" "):
            out = switch.to_persona(fid, "forge")
            self.assertTrue(out.ok, out.message)
            self.assertEqual(switch.current_persona(fid), "forge")
            self.assertEqual(tuple(persona.selection()), ("forge", "session", True))
        self.assertTrue((config.SESSIONS_DIR / f"{fid}.persona").exists())


if __name__ == "__main__":
    unittest.main()
