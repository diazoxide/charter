"""A handoff waits for the operator's yes (chat handoff, task 4).

A handoff's brief becomes a new chat's first message, and a first message runs with the
operator's authority. So the consent is the harness's own permission prompt — an `ask` rule
for `charter handoff`, which `charter init` writes — and charter's hook refuses what that
prompt cannot cover: a sub-agent's call, an unattended run, a spelling the rule does not
match, and a stdin the prompt cannot show.

**What the prompt covers is measured, not assumed** (`docs/handoff.md` records it). On Claude
Code 2.1.268, with `Bash(charter handoff *)` in the session's own settings, a quoted heredoc,
an unquoted one, a body holding `$(x)` and backticks, a `FOO=1` prefix, an `env` wrapper and
a segment after `cd … &&` all ask — in `manual`, `acceptEdits`, `auto` and `bypassPermissions`
alike — while `python3 -m charter handoff` and a path to charter run with no prompt at all.
That second half is why a spelling other than `charter handoff …` is refused here.
"""

from __future__ import annotations

import io
import json
import os
import shutil
import stat
import tempfile
import unittest
from contextlib import redirect_stderr
from pathlib import Path
from unittest import mock

from charter import commands, config, doctor, hooks, workspace
from charter.harness import registry
from tests._isolation import PersonaIso, PlaneIso, run_hook
from tests.test_doctor_answers_for_the_session_root import SessionRootCase
from tests.test_init import InitIso

OK, WARN = doctor.OK, doctor.WARN

HEREDOC = "charter handoff beta <<'BRIEF'\nFix the widget.\nBRIEF"

#: Hand-spelled rather than read from `commands.HANDOFF_ASK_PATTERN`: a test comparing a
#: file against the constant that wrote it passes over any constant at all. This is the
#: rule G1 measured.
RULE = "Bash(charter handoff *)"


def _reason(r) -> str | None:
    out = (r or {}).get("hookSpecificOutput") or {}
    return out.get("permissionDecisionReason") if out.get("permissionDecision") == "deny" else None


class TheGuardRefusesWhatThePromptCannotCover(PlaneIso):
    def setUp(self) -> None:
        super().setUp()
        self.enterContext(mock.patch.dict(os.environ, {"CHARTER_HARNESS": "claude-code"},
                                          clear=True))
        workspace.ensure("alpha")

    def _decide(self, command, **payload):
        return run_hook(hooks.pretooluse, {"tool_input": {"command": command}, "session_id": "s",
                                           "cwd": str(workspace.workspace_dir("alpha")),
                                           **payload})

    # -- who may propose one ---------------------------------------------------------------

    def test_a_sub_agents_handoff_is_refused_by_name(self):
        self.assertIn("from inside a sub-agent", _reason(self._decide(HEREDOC, agent_id="a1")) or "")

    def test_the_chat_the_operator_is_talking_to_is_not_refused(self):
        self.assertIsNone(_reason(self._decide(HEREDOC)))

    def test_a_handed_off_chat_may_hand_off_again(self):
        """No depth limit: every hop needs a yes (spec: Consent). Task 2 re-adds the variant
        with a recorded brief; the chat id alone is what this task can state."""
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "claude-code",
                                          "CHARTER_SESSION_ID": "beta.1"}, clear=True):
            self.assertIsNone(_reason(self._decide(HEREDOC)))

    def test_an_unattended_handoff_is_refused_by_name(self):
        r = self._decide(HEREDOC, permission_mode="bypassPermissions")
        self.assertIn("unattended run", _reason(r) or "")

    def test_auto_mode_is_attended(self):
        self.assertIsNone(_reason(self._decide(HEREDOC, permission_mode="auto")))

    def test_a_codex_sub_agents_handoff_is_refused_by_name(self):
        """G3, measured on codex-cli 0.147.0 (`codex exec --enable multi_agent_v2`): no
        main-conversation PreToolUse payload carried `agent_id`, and the sub-agent's Bash call
        carried one — so on Codex it means a sub-agent too, and the refusal reaches it."""
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "codex"}, clear=True):
            self.assertIn("from inside a sub-agent",
                          _reason(self._decide(HEREDOC, agent_id="a1")) or "")

    def test_an_agent_id_from_a_harness_nobody_measured_is_not_read_as_a_sub_agent(self):
        """opencode's plugin builds its payload with no `agent_id`, so one arriving there has
        no measured meaning — and a refusal keyed on a guess is a refusal nobody can explain."""
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "opencode"}, clear=True):
            self.assertIsNone(_reason(self._decide(HEREDOC, agent_id="a1")))

    # -- the spelling the rule matches -----------------------------------------------------

    def test_the_module_spelling_is_refused(self):
        r = self._decide("python3 -m charter handoff beta <<'BRIEF'\nx y\nBRIEF")
        self.assertIn("spelled exactly", _reason(r) or "")

    def test_an_environment_prefix_is_refused(self):
        self.assertIn("spelled exactly", _reason(self._decide("FOO=1 " + HEREDOC)) or "")

    def test_a_path_to_charter_is_refused(self):
        self.assertIn("spelled exactly", _reason(self._decide("/usr/local/bin/" + HEREDOC)) or "")

    # Review round 1: the two words are compared as the SOURCE spells them, not as the shell
    # would unquote them. Each of these runs `charter handoff` in bash, and whether the host
    # rule prompts for it is a measured question (docs/handoff.md) — refused either way.

    def _refused_as_a_spelling(self, spelling: str) -> None:
        r = self._decide(spelling + " beta <<'BRIEF'\nFix the widget.\nBRIEF")
        self.assertIn("spelled exactly", _reason(r) or "", spelling)

    def test_a_backslash_before_charter_is_refused(self):
        self._refused_as_a_spelling("\\charter handoff")

    def test_a_single_quoted_charter_is_refused(self):
        self._refused_as_a_spelling("'charter' handoff")

    def test_a_double_quoted_charter_is_refused(self):
        self._refused_as_a_spelling('"charter" handoff')

    def test_charter_split_by_empty_quotes_is_refused(self):
        self._refused_as_a_spelling('char""ter handoff')

    def test_an_escape_inside_charter_is_refused(self):
        self._refused_as_a_spelling("ch\\arter handoff")

    def test_a_single_quoted_handoff_is_refused(self):
        self._refused_as_a_spelling("charter 'handoff'")

    def test_a_double_quoted_handoff_is_refused(self):
        self._refused_as_a_spelling('charter "handoff"')

    def test_handoff_split_by_empty_quotes_is_refused(self):
        self._refused_as_a_spelling('charter h""andoff')

    def test_two_spaces_between_the_words_are_refused(self):
        self._refused_as_a_spelling("charter  handoff")

    def test_a_tab_between_the_words_is_refused(self):
        self._refused_as_a_spelling("charter\thandoff")

    def test_a_non_breaking_space_between_the_words_is_refused(self):
        """No shell reads U+00A0 as a separator, so this is one word to the tokenizer — the
        guard has to see the handoff before it can refuse its spelling."""
        self._refused_as_a_spelling("charter\u00a0handoff")

    def test_a_line_continuation_between_the_words_is_refused(self):
        """bash removes a backslash-newline and runs `charter handoff`; the source is still
        not the two words one space apart."""
        self._refused_as_a_spelling("charter \\\nhandoff")

    def test_an_escaped_backslash_at_a_line_end_is_not_a_continuation(self):
        """Two backslashes are a literal one, so the next line is its own command — and here
        that command is a handoff with an unquoted heredoc, which must still be judged."""
        r = self._decide("echo done \\\\\ncharter handoff beta <<BRIEF\nx y\nBRIEF")
        self.assertIn("an unquoted heredoc", _reason(r) or "")

    def test_a_backslash_ending_the_call_is_refused_rather_than_a_crash(self):
        r = self._decide("charter handoff beta \\")
        self.assertIn("left open on its line", _reason(r) or "")

    # Review round 2 (R2b): the gap AFTER `handoff` is one ASCII space too, or the command ends.

    def test_a_heredoc_glued_to_handoff_is_refused(self):
        r = self._decide("charter handoff<<'BRIEF' beta\nFix the widget.\nBRIEF")
        self.assertIn("spelled exactly", _reason(r) or "")

    def test_two_spaces_after_handoff_are_refused(self):
        r = self._decide("charter handoff  beta <<'BRIEF'\nFix the widget.\nBRIEF")
        self.assertIn("spelled exactly", _reason(r) or "")

    def test_a_tab_after_handoff_is_refused(self):
        r = self._decide("charter handoff\tbeta <<'BRIEF'\nFix the widget.\nBRIEF")
        self.assertIn("spelled exactly", _reason(r) or "")

    def test_nothing_after_handoff_is_the_end_of_the_command_not_a_spelling(self):
        self.assertIn("no heredoc at all", _reason(self._decide("charter handoff")) or "")

    # Review round 2 (R2c): a shell expansion in either word. `_charter_words` reads none of
    # these as charter at all, and each ran a handoff with no prompt on Claude Code 2.1.268.

    def test_an_ansi_c_quoted_charter_is_refused(self):
        self._refused_as_a_spelling("$'charter' handoff")

    def test_an_ansi_c_quoted_handoff_is_refused(self):
        self._refused_as_a_spelling("charter $'handoff'")

    def test_an_empty_ansi_c_string_inside_handoff_is_refused(self):
        self._refused_as_a_spelling("charter ha$''ndoff")

    def test_a_brace_expansion_of_handoff_is_refused(self):
        self._refused_as_a_spelling("charter {handoff,}")

    def test_a_parameter_default_of_handoff_is_refused(self):
        self._refused_as_a_spelling("charter ${x:-handoff}")

    def test_a_glob_that_matches_handoff_is_refused(self):
        self._refused_as_a_spelling("charter hando?f")

    def test_a_locale_quoted_handoff_is_refused(self):
        self._refused_as_a_spelling('charter $"handoff"')

    def test_a_disguised_handoff_beside_an_exact_one_is_still_refused(self):
        r = self._decide("charter handoff beta <<'BRIEF' && charter $'handoff' gamma\n"
                         "Fix the widget.\nBRIEF")
        self.assertIn("spelled exactly", _reason(r) or "")

    def test_a_quoted_workspace_is_still_the_exact_spelling(self):
        self.assertIsNone(_reason(self._decide(
            "charter handoff 'beta' <<'BRIEF'\nFix the widget.\nBRIEF")))

    def test_a_search_for_the_words_is_not_a_handoff(self):
        """charter's own repository greps for these words all day; only the first two words
        of a command are read, so a pattern naming them is not a spelling of one."""
        self.assertIsNone(_reason(self._decide(
            "grep -rn 'charter handoff' docs && rg 'hando?f' charter && echo $'charter' handoff")))

    # Review round 2 (R2d): one level into a string a shell runs.

    def test_a_handoff_inside_eval_is_refused(self):
        r = self._decide("eval \"charter handoff b <<'BRIEF'\nFix it.\nBRIEF\"")
        self.assertIn("inside a string a shell runs", _reason(r) or "")

    def test_a_handoff_inside_bash_dash_c_is_refused(self):
        r = self._decide("bash -c 'charter handoff b <<BRIEF\nx y\nBRIEF'")
        self.assertIn("inside a string a shell runs", _reason(r) or "")

    def test_a_handoff_inside_a_login_shells_dash_c_is_refused(self):
        """`-lc` is `-l` and `-c` in one cluster, and it is the spelling agents reach for."""
        r = self._decide("bash -lc 'charter handoff b <<BRIEF\nx y\nBRIEF'")
        self.assertIn("inside a string a shell runs", _reason(r) or "")

    def test_a_shell_string_that_only_searches_for_the_word_is_not_refused(self):
        self.assertIsNone(_reason(self._decide("bash -c 'grep handoff x'")))

    def test_the_exact_spelling_after_indentation_and_another_command_is_allowed(self):
        self.assertIsNone(_reason(self._decide(
            "cd x;  charter handoff beta <<'BRIEF'\nFix the widget.\nBRIEF")))

    def test_a_handoff_inside_a_substitution_is_not_at_the_start_of_its_command(self):
        r = self._decide("echo $(charter handoff beta <<'BRIEF'\nx y\nBRIEF\n)")
        self.assertIn("spelled exactly", _reason(r) or "")

    def test_a_handoff_after_cd_is_its_own_segment_and_allowed(self):
        self.assertIsNone(_reason(self._decide("cd x && " + HEREDOC)))

    # -- where the brief comes from --------------------------------------------------------

    def test_an_unquoted_heredoc_is_refused(self):
        r = self._decide("charter handoff beta <<BRIEF\nFix the widget.\nBRIEF")
        self.assertIn("an unquoted heredoc", _reason(r) or "")

    def test_a_pipe_into_handoff_is_refused(self):
        self.assertIn("a pipe", _reason(self._decide("printf 'x y' | charter handoff beta")) or "")

    def test_a_pipe_out_of_a_handoff_is_not_a_pipe_into_it(self):
        self.assertIsNone(_reason(self._decide(
            "charter handoff beta <<'BRIEF' | tail -1\nFix the widget.\nBRIEF")))

    def test_a_file_on_stdin_is_refused(self):
        self.assertIn("a file (<)", _reason(self._decide("charter handoff beta < brief.md")) or "")

    def test_a_here_string_is_refused(self):
        r = self._decide("charter handoff beta <<< 'fix it now'")
        self.assertIn("a here-string", _reason(r) or "")

    def test_a_handoff_with_no_heredoc_is_refused(self):
        self.assertIn("no heredoc at all", _reason(self._decide("charter handoff beta")) or "")

    def test_a_heredoc_another_command_is_fed_is_not_the_handoffs(self):
        r = self._decide("charter handoff beta && cat <<'BRIEF'\nx y\nBRIEF")
        self.assertIn("no heredoc at all", _reason(r) or "")

    def test_two_heredocs_are_refused(self):
        """bash reads both bodies and hands the command only the LAST one — measured with
        GNU bash 3.2.57: `cat <<'A' <<'B'` prints B's body. A prompt showing two bodies is not
        showing the brief."""
        r = self._decide("charter handoff beta <<'A' <<'B'\nfirst\nA\nsecond\nB")
        self.assertIn("more than one heredoc", _reason(r) or "")

    def test_a_quote_left_open_on_the_handoff_line_is_refused(self):
        r = self._decide("charter handoff beta --persona \"forge <<'BRIEF'\nx y\nBRIEF")
        self.assertIn("left open on its line", _reason(r) or "")

    def test_a_live_substitution_in_the_vision_is_refused(self):
        r = self._decide("charter handoff gamma --create --vision \"$(cat v)\" <<'BRIEF'\nx y\nBRIEF")
        self.assertIn("a live command substitution", _reason(r) or "")

    def test_a_quoted_heredoc_may_carry_dollar_signs_and_backticks(self):
        self.assertIsNone(_reason(self._decide(
            "charter handoff beta <<'BRIEF'\ncost $(x) and `y`\nBRIEF")))

    def test_a_handoff_named_inside_a_document_is_not_a_handoff(self):
        """The first handoff INVOCATION line is the one judged, so a document being written
        that shows the command is prose — its body is skipped the way a reader's is."""
        self.assertIsNone(_reason(self._decide(
            "cat > notes.md <<'DOC'\ncharter handoff beta\nDOC")))

    # -- the brief is data to the leak guard -----------------------------------------------

    def test_a_brief_that_names_a_vault_path_in_prose_is_not_a_read(self):
        self.assertIsNone(_reason(self._decide(
            "charter handoff beta <<'BRIEF'\nThe token lives in .charter/vaults/dev.json; "
            "never print it.\nBRIEF")))

    def test_a_brief_whose_prose_holds_an_apostrophe_and_a_vault_path_is_not_a_read(self):
        """The shape that really trips the leak guard: an apostrophe leaves the call
        unparseable, and an unparseable call is scanned as raw text for a vault path."""
        self.assertIsNone(_reason(self._decide(
            "charter handoff beta <<'BRIEF'\nThe token lives in .charter/vaults/dev.json; "
            "don't print it.\nBRIEF")))

    def test_a_brief_line_that_begins_with_a_reader_is_not_a_read(self):
        self.assertIsNone(_reason(self._decide(
            "charter handoff beta <<'BRIEF'\ncat .charter/vaults/dev.json would print it, so "
            "never run that.\nBRIEF")))

    def test_a_vault_read_after_the_heredoc_is_still_denied(self):
        r = self._decide(HEREDOC + "\ncat .charter/vaults/dev.json")
        self.assertIn(hooks._READ_REASON, _reason(r) or "")

    def test_a_script_heredoc_on_a_line_that_also_hands_off_is_still_read(self):
        """Only a body fed to `charter handoff` itself is data. Here the first body belongs to
        `bash` and runs; stripping it because the same line also names a handoff would hide
        a real read from the guard."""
        r = self._decide("bash <<'EOF' && " + "charter handoff beta <<'BRIEF'\n"
                         "cat .charter/vaults/dev.json\nEOF\nFix it.\nBRIEF")
        self.assertIn(hooks._READ_REASON, _reason(r) or "")

    def test_a_script_heredoc_after_a_handoff_on_the_same_line_is_still_read(self):
        """The mirror of the case above: the line BEGINS with a handoff, and the heredoc is
        still `bash`'s. A brief is only the body of a heredoc on the handoff's own segment."""
        r = self._decide("charter handoff beta && bash <<'EOF'\ncat .charter/vaults/dev.json\nEOF")
        self.assertIn(hooks._READ_REASON, _reason(r) or "")

    # -- what a refusal leaves behind ------------------------------------------------------

    def test_a_refused_handoff_leaves_the_routing_mark(self):
        """Task 2 clears `routing: require`'s mark for a handoff AFTER this guard, so a
        refused handoff — which opened nothing — still owes the turn its routing answer."""
        hooks._route_mark_set("s", ["forge"])
        self._decide(HEREDOC, permission_mode="bypassPermissions")
        self.assertEqual(hooks._route_mark_take("s"), ["forge"])

    def test_a_refusal_traces_its_reason_and_never_the_command(self):
        with mock.patch("charter.hooks._trace") as trace:
            self._decide(HEREDOC, permission_mode="bypassPermissions")
        ours = [c for c in trace.call_args_list if c.kwargs.get("reason") == "handoff-unattended"]
        self.assertEqual(len(ours), 1, trace.call_args_list)
        self.assertEqual(ours[0].args, ("deny", "s"))
        self.assertNotIn("cmd", ours[0].kwargs)
        self.assertNotIn("Fix the widget", repr(trace.call_args_list))

    def test_each_refusal_says_a_different_thing(self):
        reasons = [
            _reason(self._decide(HEREDOC, agent_id="a1")),
            _reason(self._decide(HEREDOC, permission_mode="bypassPermissions")),
            _reason(self._decide("FOO=1 " + HEREDOC)),
            _reason(self._decide("charter handoff beta <<BRIEF\nx y\nBRIEF")),
            _reason(self._decide("printf 'x y' | charter handoff beta")),
            _reason(self._decide("charter handoff beta < brief.md")),
            _reason(self._decide("charter handoff beta <<< 'fix it now'")),
            _reason(self._decide("charter handoff beta")),
            _reason(self._decide("charter handoff beta <<'A' <<'B'\nfirst\nA\nsecond\nB")),
            _reason(self._decide("charter handoff beta --persona \"forge <<'BRIEF'\nx y\nBRIEF")),
            _reason(self._decide("charter handoff g --create --vision \"$(cat v)\" <<'BRIEF'\nx\nBRIEF")),
            _reason(self._decide("bash -c 'charter handoff b <<< x'")),
        ]
        self.assertNotIn(None, reasons)
        self.assertEqual(len(set(reasons)), len(reasons), reasons)


class TheQuotingJudgementIsTheSubstitutionGuards(PlaneIso):
    """"Quoted" means what `_heredoc_header` says it means — any quoting anywhere in the
    delimiter makes the body literal, verified against bash where that function lives. The
    guard reads it off the delimiter token instead; this holds the two to one answer."""

    SPELLINGS = (("<<'B'", "B"), ('<<"B"', "B"), ("<<\\B", "B"), ("<<B'B'", "BB"),
                 ("<<-'B'", "B"), ("<<B", "B"), ("<<-B", "B"))

    def setUp(self) -> None:
        super().setUp()
        self.enterContext(mock.patch.dict(os.environ, {"CHARTER_HARNESS": "claude-code"},
                                          clear=True))

    def test_the_guard_and_the_header_reader_agree_on_every_spelling(self):
        for spelling, delim in self.SPELLINGS:
            with self.subTest(spelling=spelling):
                line = f"charter handoff beta {spelling}"
                expands = hooks._heredoc_header(line, line.index("<<"))[1]
                r = run_hook(hooks.pretooluse, {
                    "tool_input": {"command": f"{line}\nx y\n{delim}"}, "session_id": "s",
                    "cwd": str(config.ROOT)})
                self.assertEqual("an unquoted heredoc" in (_reason(r) or ""), expands, _reason(r))


class TheHandoffGuardNeedsAPlane(PersonaIso):
    def test_outside_a_plane_the_handoff_guard_says_nothing(self):
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "claude-code"}, clear=True):
            r = run_hook(hooks.pretooluse, {"tool_input": {"command": HEREDOC}, "session_id": "s",
                                            "cwd": str(self.tmp),
                                            "permission_mode": "bypassPermissions"})
        self.assertIsNone(r)


class AHandoffIsRecognisedInEverySpelling(unittest.TestCase):
    """`hooks._is_handoff` — the shape the refusals above and Task 2's routing-mark clear
    share. It answers "does this call run `charter handoff` at all", in every spelling;
    whether that spelling is one the prompt covers is the guard's question, not this one's."""

    def test_the_plain_spelling_is_a_handoff(self):
        self.assertTrue(hooks._is_handoff("charter handoff beta"))

    def test_the_module_spelling_is_a_handoff(self):
        self.assertTrue(hooks._is_handoff("python3 -m charter handoff beta"))

    def test_a_prefix_and_a_path_are_still_a_handoff(self):
        self.assertTrue(hooks._is_handoff("FOO=1 charter handoff beta"))
        self.assertTrue(hooks._is_handoff("/usr/local/bin/charter handoff beta"))

    def test_a_handoff_after_another_command_is_a_handoff(self):
        self.assertTrue(hooks._is_handoff("cd x && charter handoff beta"))

    def test_another_charter_command_is_not(self):
        self.assertFalse(hooks._is_handoff("charter workspace list"))

    def test_a_command_that_only_says_the_words_is_not(self):
        self.assertFalse(hooks._is_handoff("echo charter handoff beta"))
        self.assertFalse(hooks._is_handoff("git commit -m 'charter handoff beta'"))

    def test_a_bare_charter_is_not(self):
        self.assertFalse(hooks._is_handoff("charter"))

    def test_no_command_is_not_a_handoff(self):
        self.assertFalse(hooks._is_handoff(None))
        self.assertFalse(hooks._is_handoff(""))

    def test_a_heredoc_line_that_begins_charter_handoff_counts(self):
        """Newlines separate segments, so a body line spelling the command counts too. Stated
        because Task 2 clears a routing mark on this answer, where over-matching costs one
        mark; the guard skips such bodies before it judges anything."""
        self.assertTrue(hooks._is_handoff("cat <<'DOC'\ncharter handoff beta\nDOC"))


class TheRuleIsWrittenForANewPlane(PersonaIso):
    def _settings(self) -> dict:
        return json.loads((config.ROOT / ".claude" / "settings.json").read_text())

    def test_the_pattern_is_the_rule_that_was_measured(self):
        self.assertEqual(commands._as_rule(commands.HANDOFF_ASK_PATTERN), RULE)

    def test_it_writes_claude_codes_bash_rule(self):
        commands.ensure_handoff_gate(config.ROOT)
        self.assertIn(RULE, self._settings()["permissions"]["ask"])

    def test_it_writes_opencodes_bash_permission(self):
        commands.ensure_handoff_gate(config.ROOT)
        doc = json.loads((config.ROOT / "opencode.json").read_text())
        self.assertEqual(doc["permission"]["bash"]["charter handoff *"], "ask")

    def test_codex_is_named_as_unable_rather_than_skipped(self):
        results, _blocked = commands.ensure_handoff_gate(config.ROOT)
        self.assertIn(("codex", "unsupported"), [(h.name, s) for h, s, _d in results])

    def test_writing_it_twice_is_not_an_edit(self):
        commands.ensure_handoff_gate(config.ROOT)
        before = (config.ROOT / ".claude" / "settings.json").read_bytes()
        results, blocked = commands.ensure_handoff_gate(config.ROOT)
        self.assertFalse(blocked)
        self.assertEqual({(h.name, s) for h, s, _d in results if s != "unsupported"},
                         {("claude-code", "present"), ("opencode", "present")})
        self.assertEqual((config.ROOT / ".claude" / "settings.json").read_bytes(), before)

    def test_a_plane_with_no_settings_file_gets_an_indented_one(self):
        """The format a writer echoes is the file's own; with no file there is nothing to echo,
        and the rule still lands in a file a person can read — two spaces, as before."""
        commands.ensure_handoff_gate(config.ROOT)
        text = (config.ROOT / ".claude" / "settings.json").read_text()
        self.assertIn('\n  "permissions": {\n    "ask": [\n', text)

    def test_a_malformed_settings_file_blocks_every_harness_and_is_left_untouched(self):
        p = config.ROOT / ".claude" / "settings.json"
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text("{nope")
        _results, blocked = commands.ensure_handoff_gate(config.ROOT)
        self.assertTrue(blocked)
        self.assertEqual(p.read_text(), "{nope")
        self.assertFalse((config.ROOT / "opencode.json").exists())


class AnExistingPlaneAdoptsTheGateWithOneCommand(PersonaIso):
    """`charter guard handoff` — `guard ask 'charter handoff *'` with the pattern fixed.

    It exists because the news entry's `adopt:` line is how a plane `init` did not make takes
    the rule up, and that line is never a shell string: `news._tokens` refuses any action
    holding a character a shell reads as syntax — the quotes a pattern with spaces needs — and
    splits the rest on whitespace, so `guard ask 'charter handoff *'` can never parse there."""

    def test_the_word_reaches_the_command(self):
        from charter import cli
        args = cli.build_parser().parse_args(["guard", "handoff"])
        self.assertIs(args.func, commands.cmd_guard_handoff)

    def test_the_news_entry_adopts_it_with_a_line_charter_can_run(self):
        from charter import cli, news
        self.assertTrue(news.resolves(cli.build_parser(), "guard handoff"))
        self.assertFalse(news.resolves(cli.build_parser(), "guard ask 'charter handoff *'"))

    def test_it_writes_the_rule_every_harness_that_can_hold_one_holds(self):
        self.assertEqual(commands.cmd_guard_handoff(mock.Mock(spec=[])), 0)
        s = json.loads((config.ROOT / ".claude" / "settings.json").read_text())
        self.assertIn(RULE, s["permissions"]["ask"])
        doc = json.loads((config.ROOT / "opencode.json").read_text())
        self.assertEqual(doc["permission"]["bash"]["charter handoff *"], "ask")

    def test_a_malformed_settings_file_is_refused_like_guard_ask_refuses_it(self):
        p = config.ROOT / ".claude" / "settings.json"
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text("{nope")
        self.assertNotEqual(commands.cmd_guard_handoff(mock.Mock(spec=[])), 0)
        self.assertEqual(p.read_text(), "{nope")


class InitWritesTheGate(InitIso):
    def _init_quietly(self) -> tuple[int, str]:
        buf = io.StringIO()
        with redirect_stderr(buf):
            rc = self._init()
        return rc, buf.getvalue()

    def test_init_writes_the_gate(self):
        with mock.patch("charter.commands.ensure_handoff_gate", return_value=([], False)) as m:
            self._init_quietly()
        m.assert_called_once_with(self.root)

    def test_a_fresh_plane_asks_before_a_handoff(self):
        rc, out = self._init_quietly()
        self.assertEqual(rc, 0, out)
        s = json.loads((self.root / ".claude" / "settings.json").read_text())
        self.assertIn(RULE, s["permissions"]["ask"])
        # Folded into the one mention of the file `TestSummaryStaysReadable` pins.
        self.assertEqual(out.count(".claude/settings.json"), 1, out)
        self.assertIn("ask: charter handoff", out)

    def test_a_second_init_lists_the_rule_as_present(self):
        self._init_quietly()
        _rc, out = self._init_quietly()
        present = next(line for line in out.splitlines() if "already present" in line)
        self.assertIn("ask: charter handoff", present)

    def test_a_gate_init_could_not_write_names_the_file_and_the_command(self):
        row = (registry.get("claude-code"), "malformed", "/plane/.claude/settings.json")
        with mock.patch("charter.commands.ensure_handoff_gate", return_value=([row], True)):
            _rc, out = self._init_quietly()
        self.assertIn("/plane/.claude/settings.json is not valid", out)
        self.assertIn("charter guard ask 'charter handoff *'", out)

    def test_a_rule_that_passed_the_check_and_could_not_be_written_is_said(self):
        row = (registry.get("claude-code"), "unwritable",
               "/plane/.claude/settings.json (Permission denied)")
        with mock.patch("charter.commands.ensure_handoff_gate", return_value=([row], False)):
            _rc, out = self._init_quietly()
        self.assertIn("could not write /plane/.claude/settings.json (Permission denied)", out)


class DoctorNamesTheGate(SessionRootCase):
    def _claude_rule(self, where: Path) -> None:
        p = where / ".claude" / "settings.json"
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(json.dumps({"permissions": {"ask": [RULE]}}))

    def _opencode_rule(self) -> None:
        (config.ROOT / "opencode.json").write_text(
            json.dumps({"permission": {"bash": {"charter handoff *": "ask"}}}))

    def test_a_missing_rule_is_a_warning_naming_the_command_that_adds_it(self):
        self.rooted_at(config.ROOT)
        r = doctor.check_handoff_gate()
        self.assertEqual(r.status, WARN)
        self.assertIn("claude-code", r.detail)
        self.assertIn("opencode", r.detail)
        self.assertIn("charter guard ask 'charter handoff *'", r.hint)
        self.assertIn("your choice", r.hint)

    def test_a_rule_in_force_is_ok(self):
        self._claude_rule(config.ROOT)
        self._opencode_rule()
        self.rooted_at(config.ROOT)
        r = doctor.check_handoff_gate()
        self.assertEqual(r.status, OK, r)
        self.assertIn("asks first under claude-code", r.detail)

    def test_a_session_in_a_workspace_the_rule_has_not_reached_is_a_warning(self):
        """Claude Code reads the session's own settings (#855): the plane's rule is not in
        force in a chat standing at `workspaces/<ws>/`, and this row says so rather than
        reading the plane's file and printing a tick."""
        self._claude_rule(config.ROOT)
        self._opencode_rule()
        self.rooted_at(self.workspace)
        r = doctor.check_handoff_gate()
        self.assertEqual(r.status, WARN, r)
        self.assertIn("under claude-code", r.detail)
        self.assertNotIn("opencode", r.detail)

    def test_a_workspace_that_carries_the_rule_is_ok(self):
        """opencode is judged by the plane's `opencode.json` wherever the session stands —
        it resolves project config at the repository root, which for a workspace directory is
        the plane."""
        self._claude_rule(self.workspace)
        self._opencode_rule()
        self.rooted_at(self.workspace)
        self.assertEqual(doctor.check_handoff_gate().status, OK)

    def test_a_malformed_settings_file_is_not_read_as_a_missing_rule(self):
        p = config.ROOT / ".claude" / "settings.json"
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text("{nope")
        self._opencode_rule()
        self.rooted_at(config.ROOT)
        r = doctor.check_handoff_gate()
        self.assertEqual(r.status, WARN)
        self.assertEqual(r.hint, doctor._NOT_CHECKED_HINT)
        self.assertNotIn("guard ask", r.hint)
        self.assertIn("is not valid", r.detail)

    def test_a_settings_file_charter_cannot_read_is_not_read_as_a_missing_rule(self):
        """Measured: `Path.exists` on a file under a mode-000 directory raises PermissionError
        on Python 3.11, 3.12 and 3.13 (and answers False on 3.14), and the settings loaders
        call it outside their own `try`. A row that let it out would take doctor down."""
        self._opencode_rule()
        self.rooted_at(config.ROOT)
        denied = PermissionError(13, "Permission denied")
        with mock.patch("charter.harness.claude_code.ClaudeCodeHarness.apply_ask_rule",
                        side_effect=denied):
            r = doctor.check_handoff_gate()
        self.assertEqual(r.status, WARN)
        self.assertEqual(r.hint, doctor._NOT_CHECKED_HINT)
        self.assertIn("could not be read", r.detail)

    def test_the_harnesses_that_cannot_refuse_are_named(self):
        self._claude_rule(config.ROOT)
        self._opencode_rule()
        self.rooted_at(config.ROOT)
        r = doctor.check_handoff_gate()
        self.assertEqual(r.status, OK, r)
        for name in ("opencode", "codex"):
            gap = next(d for d in registry.get(name).deficits if d.key == "handoff-gate")
            self.assertIn(f"↳ {name}: {gap.detail}", r.detail)

    def test_the_row_is_in_the_preflight(self):
        self.assertIn("handoff gate", doctor.check_names())


class EveryHarnessSaysHowAHandoffIsGated(unittest.TestCase):
    def test_claude_code_holds_the_rule_and_declares_no_gap(self):
        tmp = Path(tempfile.mkdtemp(prefix="charter-gate-"))
        self.addCleanup(shutil.rmtree, tmp, True)
        h = registry.get("claude-code")
        self.assertEqual(h.apply_ask_rule(tmp, commands.HANDOFF_ASK_PATTERN, dry_run=True)[0],
                         "added")
        self.assertNotIn("handoff-gate", [d.key for d in h.deficits])

    def test_opencode_and_codex_name_the_gap_and_invent_no_remedy(self):
        for name in ("opencode", "codex"):
            with self.subTest(harness=name):
                gap = [d for d in registry.get(name).deficits if d.key == "handoff-gate"]
                self.assertEqual(len(gap), 1)
                self.assertTrue(gap[0].detail)
                self.assertEqual(gap[0].remedy, "")


if __name__ == "__main__":
    unittest.main()
