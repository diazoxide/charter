"""A `$CHARTER_WORKSPACE` holding only whitespace names no workspace, so the rungs below it
decide (#1055).

The same defect #1048 fixed for `$CHARTER_PERSONA`, one module over. `workspace.chosen` took
the variable as its top rung whenever it was set and stripped it only after deciding it had a
name, and `workspace.source`, `commands_workspace._warn_env_override`, the SessionStart
confirm nudge and the launch picker's gate each tested the raw value for truth. Measured
before, on a plane where this session had selected `alpha`: with `CHARTER_WORKSPACE=" "`,
`chosen` answered `""`, so `resolve` fell to `default` past the session pointer;
`workspace current` printed `default` / `resolved via $CHARTER_WORKSPACE`; `workspace use
alpha` warned that `' '` takes precedence; and a new session was not asked to confirm a
workspace, because the variable counted as a hard pin.

**Empty was already unset, and that is the rule being extended.**
`commands_frame._frame_identity_env` launches every chat with `CHARTER_WORKSPACE=` when the
launch pinned nothing, and `frame.state.workspace_for` and `frame.slots` already stripped the
value before asking it anything. A name inside whitespace (`" alpha "`) is that name.
"""
from __future__ import annotations

import unittest

from charter import config, workspace
from tests.test_a_stale_persona_pointer_is_named import StaleIso, _run

#: Values a shell leaves in the variable that are set and name nothing.
BLANKS = (" ", "\t", "\n", " \t\n ")

#: What `workspace current` says when it ignored one.
IGNORED = ("$CHARTER_WORKSPACE is set but holds only whitespace, so charter ignored it and "
           "the rungs below it decided. Unset it, or set it to the workspace you meant.")


class WorkspaceIso(StaleIso):
    """`StaleIso`'s plane, holding the workspace `alpha`, which session `s1` selected."""

    SID = "s1"

    def setUp(self) -> None:
        super().setUp()
        config.WORKSPACES_DIR.mkdir(parents=True, exist_ok=True)
        workspace.ensure("alpha")
        with self.env(CHARTER_SESSION_ID=self.SID):
            workspace.set_active("alpha")


class TestABlankVariableLeavesTheRungsBelowDeciding(WorkspaceIso):

    def test_a_space_leaves_the_session_pointer_deciding(self):
        with self.env(CHARTER_WORKSPACE=" ", CHARTER_SESSION_ID=self.SID):
            self.assertEqual(workspace.chosen(), "alpha")
            self.assertEqual(workspace.resolve(), "alpha")
            self.assertEqual(workspace.source(), "session")

    def test_a_tab_or_a_newline_is_blank_too(self):
        for blank in BLANKS:
            with self.subTest(blank=blank), \
                    self.env(CHARTER_WORKSPACE=blank, CHARTER_SESSION_ID=self.SID):
                self.assertEqual((workspace.resolve(), workspace.source()),
                                 ("alpha", "session"))

    def test_a_name_inside_whitespace_is_that_name(self):
        """Pinned rather than changed: the variable has always been stripped, and it still
        outranks every rung below it."""
        workspace.ensure("beta")
        with self.env(CHARTER_WORKSPACE=" beta\n", CHARTER_SESSION_ID=self.SID):
            self.assertEqual(workspace.resolve(), "beta")
            self.assertEqual(workspace.source(), "$CHARTER_WORKSPACE")


class TestCurrentSaysItIgnoredTheBlank(WorkspaceIso):

    def _current(self, **env) -> tuple[str, str]:
        from charter import commands_workspace
        with self.env(CHARTER_SESSION_ID=self.SID, **env):
            rc, out, err = _run(commands_workspace.cmd_workspace_current)
        self.assertEqual(rc, 0, err)
        return out, err

    def test_a_blank_variable_is_named_beside_the_rung_that_decided(self):
        """Measured before: `default` / `resolved via $CHARTER_WORKSPACE`, and no word that
        the variable held nothing."""
        out, err = self._current(CHARTER_WORKSPACE="\t")
        self.assertEqual(out, "alpha\n")
        self.assertIn("resolved via session,", err)
        self.assertIn(IGNORED, err)

    def test_an_empty_variable_is_not_reported(self):
        """Empty is what a frame launches every chat with when nothing was pinned
        (`commands_frame._frame_identity_env`), so saying it was ignored would be said in
        every chat, about an export nobody wrote."""
        out, err = self._current(CHARTER_WORKSPACE="")
        self.assertEqual(out, "alpha\n")
        self.assertNotIn("whitespace", err)

    def test_a_name_inside_whitespace_is_not_reported(self):
        workspace.ensure("beta")
        out, err = self._current(CHARTER_WORKSPACE=" beta ")
        self.assertEqual(out, "beta\n")
        self.assertIn("resolved via $CHARTER_WORKSPACE,", err)
        self.assertNotIn("whitespace", err)


class TestUseIsWarnedOnlyAboutAVariableThatDecides(WorkspaceIso):
    """`workspace use` (and `create --use`) warn that the variable outranks the selection."""

    def _use(self, name: str, **env) -> str:
        from charter import commands_workspace
        with self.env(CHARTER_SESSION_ID=self.SID, **env):
            rc, _out, err = _run(commands_workspace.cmd_workspace_use, name=name, force=False,
                                 create=False)
        self.assertEqual(rc, 0, err)
        return err

    def test_use_is_not_told_a_blank_variable_takes_precedence(self):
        """Measured before: `$CHARTER_WORKSPACE=' ' is set and takes precedence — commands in
        this session will still act on ' ', not 'alpha'.`, beside a selection that did
        decide."""
        err = self._use("alpha", CHARTER_WORKSPACE=" ")
        self.assertNotIn("takes precedence", err)

    def test_use_under_a_real_name_says_the_name_that_decides(self):
        """The name resolution ranks, stripped: before, the warning quoted the raw value, so
        `' beta '` was named as a workspace commands act on."""
        workspace.ensure("beta")
        err = self._use("alpha", CHARTER_WORKSPACE=" beta ")
        self.assertIn("$CHARTER_WORKSPACE='beta' is set and takes precedence — commands in this "
                      "session will still act on 'beta', not 'alpha'.", err)

    def test_use_of_the_name_the_variable_already_names_is_not_warned(self):
        """The variable and the selection agree, so nothing outranks anything."""
        for value in ("alpha", " alpha "):
            with self.subTest(value=value):
                self.assertNotIn("takes precedence", self._use("alpha", CHARTER_WORKSPACE=value))

    def test_the_name_is_one_line(self):
        """The value comes out of a shell, so a line separator inside it would start a line
        of charter's output that charter did not write."""
        err = self._use("alpha", CHARTER_WORKSPACE="be\nta")
        warning = [ln for ln in err.splitlines() if "takes precedence" in ln]
        self.assertEqual(len(warning), 1, err)
        self.assertIn("still act on 'be\\x0ata', not 'alpha'.", warning[0])


class TestABlankVariableIsNoHardPin(WorkspaceIso):
    """The two places that skip asking because the variable pinned the workspace."""

    def test_session_start_asks_a_new_session_to_confirm_a_workspace(self):
        """Measured before: no nudge at all, so a session with nothing chosen was never asked
        and went on in whatever the ladder fell to."""
        from charter import hooks
        with self.env(CHARTER_WORKSPACE=" ", CHARTER_SESSION_ID="s2"):
            self.assertIn("Confirm the workspace", hooks._workspace_confirm_nudge("s2"))

    def test_session_start_still_skips_a_real_pin(self):
        from charter import hooks
        with self.env(CHARTER_WORKSPACE=" alpha ", CHARTER_SESSION_ID="s2"):
            self.assertEqual(hooks._workspace_confirm_nudge("s2"), "")

    def _picker_wanted(self, **env) -> bool:
        from types import SimpleNamespace
        from unittest import mock
        from charter import commands_frame
        with self.env(**env), mock.patch("sys.stdin") as sin, mock.patch("sys.stdout") as out:
            sin.isatty.return_value = True
            out.isatty.return_value = True
            return commands_frame._picker_wanted(SimpleNamespace(workspace=None, pick=False),
                                                 None)

    def test_a_launch_nothing_chose_is_still_offered_the_picker(self):
        """Measured before: `charter claude` under a blank variable skipped the picker as if
        the operator had named a workspace, and launched into `default`."""
        self.assertTrue(self._picker_wanted(CHARTER_WORKSPACE="\t"))

    def test_a_launch_under_a_real_pin_is_not(self):
        self.assertFalse(self._picker_wanted(CHARTER_WORKSPACE=" alpha "))


class TestTheSameKindOfValueOnThePersonaSide(StaleIso):
    """Two smaller cases found beside the workspace one (#1055)."""

    #: What a `--persona` holding only whitespace is told.
    NOT_A_NAME = "no persona ' ' (a persona name is never only whitespace)"

    def test_use_names_the_persona_the_variable_holds_on_one_line(self):
        """Measured before: `$CHARTER_PERSONA='fo` and `rge' is set and takes precedence …`
        on two lines, the second one looking like a line of charter's own."""
        from charter import commands_persona
        self.persona_("forge")
        with self.env(CHARTER_PERSONA="fo\nrge", CHARTER_SESSION_ID="s1"):
            rc, _out, err = _run(commands_persona.cmd_persona_use, name="forge")
        self.assertEqual(rc, 0, err)
        warning = [ln for ln in err.splitlines() if "takes precedence" in ln]
        self.assertEqual(len(warning), 1, err)
        self.assertIn("$CHARTER_PERSONA='fo\\x0arge' is set and takes precedence — commands use "
                      "'fo\\x0arge', not 'forge'.", warning[0])

    def test_a_persona_secret_flag_of_only_whitespace_is_refused(self):
        """Measured before: `persona ' ' has no vault. Add `vault:` to its file, or `charter
        vault add <v> --persona  `.` — advice about a file no name of whitespace can have."""
        from charter import commands_persona
        with self.env(CHARTER_SESSION_ID="s1"):
            rc, _out, err = _run(commands_persona.cmd_persona_secret_list, persona=" ",
                                 vault=None)
        self.assertEqual(rc, 1, err)
        self.assertIn(self.NOT_A_NAME, err)
        self.assertNotIn("has no vault", err)

    def test_a_recall_flag_of_only_whitespace_is_refused(self):
        """Measured before: exit 0 and `No memories yet across workspace, persona, shared,
        refs.`, searched under a persona named `' '` while the declared `steward` was not
        searched at all."""
        from charter import commands
        with self.env(CHARTER_SESSION_ID="s1"):
            rc, out, err = _run(commands.cmd_recall, persona="\t", query=None, scope=None,
                                ephemeral=False, since=None, limit=8, workspace=None,
                                all_workspaces=False, full=False)
        self.assertEqual(rc, 1, err)
        self.assertIn("no persona '\\x09' (a persona name is never only whitespace)", err)
        self.assertEqual(out, "")

    def test_a_named_persona_flag_is_untouched(self):
        """The refusal is about whitespace, not about the flag: `--persona steward` still
        searches `steward`, and no flag still means the active persona. So does an empty
        one, which every reader of the flag has always taken as no flag."""
        from charter import commands
        for flag in ("steward", None, ""):
            with self.subTest(flag=flag), self.env(CHARTER_SESSION_ID="s1"):
                rc, out, err = _run(commands.cmd_recall, persona=flag, query=None, scope=None,
                                    ephemeral=False, since=None, limit=8, workspace=None,
                                    all_workspaces=False, full=False)
                self.assertEqual(rc, 0, err)
                self.assertIn("refs:steward", out)


if __name__ == "__main__":
    unittest.main()
