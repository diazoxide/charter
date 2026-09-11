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
import time
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

#: Two real commit messages from this branch, as `git log` prints them. Both name the command
#: in prose and in backticks, which is the shape review round 3 found refused: a commit message
#: is nobody's script, and charter's own repository writes them by heredoc.
PHASE1_MESSAGE = """A handoff waits for your yes, bypassPermissions included, and a sub-agent or an unattended run cannot propose one (chat handoff, task 4, phase 1)

A handoff's brief becomes a new chat's first message and runs with the
operator's authority, and nothing on main stood in front of one.

- `charter init` writes the `ask` rule for `charter handoff *` through
  `commands.ensure_handoff_gate` (`_guard_apply` with the pattern fixed);
  `charter guard handoff` is the same rule for a news `adopt:` line, which
  cannot carry quotes.
- A7 in `hooks.pretooluse` (plane-gated): refuses a handoff from a sub-agent
  (`agent_id`, Claude Code and Codex), an unattended run, any spelling but
  `charter handoff …`, and a stdin other than one quoted heredoc.
  `hooks._is_handoff` is introduced here.
- The leak guard skips the body of a heredoc on a handoff's own segment.

Part of #956."""

ROUND1_MESSAGE = """A handoff spelled with a quote, an escape or odd spacing is refused, judged on the source (chat handoff, task 4, review round 1)

A7 compared the unquoted token texts, so `charter 'handoff'`, `\\charter handoff`
and `charter  handoff` passed as the exact form. Measured on Claude Code 2.1.268,
`Bash(charter handoff *)` does not match a quoted or split second word:
`charter 'handoff'`, `charter "handoff"` and `charter h""andoff` ran a handoff
with no prompt.

- A7 refuses unless the handoff segment's first two tokens are bare, read
  `charter` and `handoff`, and stand one ASCII space apart in the raw line.
- `_handoff_line` finds the handoff with bash's backslash-newline removed and
  every whitespace read as a space, so a continuation or a U+00A0 between the
  words is found and refused.
- The refusal names its fix: spell it exactly
  `charter handoff <workspace> <<'BRIEF'`.

Part of #956."""


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
        self.assertIn("a shell runs", _reason(r) or "")

    def test_a_handoff_inside_bash_dash_c_is_refused(self):
        r = self._decide("bash -c 'charter handoff b <<BRIEF\nx y\nBRIEF'")
        self.assertIn("a shell runs", _reason(r) or "")

    def test_a_handoff_inside_a_login_shells_dash_c_is_refused(self):
        """`-lc` is `-l` and `-c` in one cluster, and it is the spelling agents reach for."""
        r = self._decide("bash -lc 'charter handoff b <<BRIEF\nx y\nBRIEF'")
        self.assertIn("a shell runs", _reason(r) or "")

    def test_a_shell_string_that_only_searches_for_the_word_is_not_refused(self):
        self.assertIsNone(_reason(self._decide("bash -c 'grep handoff x'")))

    # Review round 3 (A2): the gap after `handoff`, read off the source rather than the tokens.

    def test_a_line_continuation_right_after_handoff_is_refused(self):
        """bash removes the backslash-newline and runs `charter handoff beta`. The tokenizer
        folds that pair into the next word, so the gap between the tokens reads as one space;
        only the source shows what stood after `handoff`."""
        r = self._decide("charter handoff \\\nbeta <<'BRIEF'\nFix the widget.\nBRIEF")
        self.assertIn("spelled exactly", _reason(r) or "")

    def test_a_line_continuation_later_in_the_command_is_still_the_exact_spelling(self):
        self.assertIsNone(_reason(self._decide(
            "charter handoff beta \\\n<<'BRIEF'\nFix the widget.\nBRIEF")))

    # Review round 3 (A3): one pin per mutation that survived round 2's sweep. Each of these
    # goes red when the line it names is deleted or narrowed.

    def _refused_in_shell(self, shell: str) -> None:
        r = self._decide(f"{shell} -c 'charter handoff b'")
        self.assertIn("a shell runs", _reason(r) or "", shell)

    def test_a_longer_word_beginning_handoff_is_not_a_spelling_of_it(self):
        """Without the "does this word carry a shell character at all" precheck, `handoffs`
        reads as `handoff` with something dropped, and an ordinary longer word — a typo, a
        plural, a subcommand charter may grow — is refused as a disguise."""
        self.assertIsNone(_reason(self._decide(
            "charter handoffs beta <<'BRIEF'\nFix the widget.\nBRIEF")))

    def test_a_disguised_handoff_inside_a_shell_string_is_refused(self):
        """The string is read by BOTH readers: the plain one and the disguise reader."""
        r = self._decide("bash -c 'charter {handoff,} b'")
        self.assertIn("a shell runs", _reason(r) or "")

    def test_a_handoff_inside_sh_dash_c_is_refused(self):
        self._refused_in_shell("sh")

    def test_a_handoff_inside_zsh_dash_c_is_refused(self):
        self._refused_in_shell("zsh")

    def test_a_handoff_inside_dash_dash_c_is_refused(self):
        self._refused_in_shell("dash")

    def test_a_handoff_inside_ksh_dash_c_is_refused(self):
        self._refused_in_shell("ksh")

    def test_a_disguised_word_at_the_end_of_the_call_is_read_whole(self):
        """A token that ends at the end of the input ends AT the stream, not one character
        before it: `hando?f` read as `hando?` is a glob that matches nothing."""
        self.assertIn("spelled exactly", _reason(self._decide("charter hando?f")) or "")

    def test_a_continuation_inside_a_shell_string_is_read_as_the_shell_reads_it(self):
        """bash removes the backslash-newline inside the string before running it, so the
        string has to be read that way too — `char\\<newline>ter handoff b` is a handoff."""
        r = self._decide("bash -c 'char\\\nter handoff b'")
        self.assertIn("a shell runs", _reason(r) or "")

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


class TheBriefIsDataWhereverBashEndsIt(PlaneIso):
    """Review round 3, part B. The brief is dropped from the leak guard's view through #974's
    heredoc plan, so its body ends where BASH ends it — and a body nobody executes is not read
    as commands at all."""

    def setUp(self) -> None:
        super().setUp()
        self.enterContext(mock.patch.dict(os.environ, {"CHARTER_HARNESS": "claude-code"},
                                          clear=True))
        workspace.ensure("alpha")

    def _decide(self, command, **payload):
        return run_hook(hooks.pretooluse, {"tool_input": {"command": command}, "session_id": "s",
                                           "cwd": str(workspace.workspace_dir("alpha")),
                                           **payload})

    def _read_refused(self, cmd: str) -> None:
        self.assertIn(hooks._READ_REASON, _reason(self._decide(cmd)) or "", cmd)

    def test_a_brief_delimiter_split_by_quotes_does_not_hide_a_later_read(self):
        """bash ends this brief at `BRIEFX`. A regex reads the delimiter as `BRIEF`, never finds
        it, and drops to the end of the input — taking the `bash` body, and the read in it."""
        self._read_refused("charter handoff b <<BRIEF'X' && bash <<'A'\n"
                           "Fix the widget.\nBRIEFX\ncat .charter/vaults/dev.json\nA")

    def test_a_brief_delimiter_with_a_quoted_head_does_not_hide_a_later_read(self):
        self._read_refused('charter handoff b <<"BRIEF"X && bash <<\'A\'\n'
                           "Fix the widget.\nBRIEFX\ncat .charter/vaults/dev.json\nA")

    def test_a_quoted_heredoc_operator_in_the_handoffs_own_argument_hides_nothing(self):
        """`--vision '<<X'` is an argument, not a heredoc. Counting it as one shifts every
        delimiter after it, and the brief then ends on the wrong line."""
        self._read_refused("charter handoff b --vision '<<X' <<'BRIEF'; bash <<'A'\n"
                           "Fix the widget.\nBRIEF\ncat .charter/vaults/dev.json\nX\nA")

    def test_a_brief_whose_terminator_never_arrives_is_not_dropped(self):
        """bash reads an unterminated body to the end of the input, so dropping it would take
        every command after it with it. Nothing is dropped and the read stays visible."""
        self._read_refused("charter handoff b <<'BRIEF'\ncat .charter/vaults/dev.json")

    def test_the_exact_brief_is_still_data(self):
        """The case E has always been about: prose in a brief is not a read (#258's shape)."""
        self.assertIsNone(_reason(self._decide(
            "charter handoff beta <<'BRIEF'\nThe token lives in .charter/vaults/dev.json; "
            "don't print it.\nBRIEF")))

    def test_a_commit_message_that_names_a_handoff_is_not_one(self):
        """`git` runs no body, so a commit message is data however many handoffs it describes.
        Both shapes are how this branch's own commits were written."""
        for name, msg in (("phase 1", PHASE1_MESSAGE), ("round 1", ROUND1_MESSAGE)):
            for how, cmd in (("-F -", f"git commit -F - <<'EOF'\n{msg}\nEOF"),
                             ("-m $(cat …)",
                              f'git commit -m "$(cat <<\'EOF\'\n{msg}\nEOF\n)"')):
                with self.subTest(message=name, how=how):
                    self.assertIsNone(_reason(self._decide(cmd)))

    def test_a_handoff_inside_a_heredoc_a_shell_runs_is_refused(self):
        """The host sees `bash`, so no prompt stands in front of the handoff in its body."""
        r = self._decide("bash <<'EOF'\ncharter handoff beta <<'BRIEF'\nFix it.\nBRIEF\nEOF")
        self.assertIn("a shell runs", _reason(r) or "")

    def test_a_read_the_brief_swallows_is_data_and_the_same_read_in_bashs_body_is_not(self):
        """Where a brief ENDS decides who runs the line after it, so each pair below differs by
        one line. In the first of each, `bash` never reaches the read: ` BRIEF` is not a
        terminator (`<<` wants the delimiter alone on the line) and a trailing backslash in an
        UNQUOTED body continues the line, so `BRIEF` is eaten by the continuation. Both were
        measured under bash 3.2.57 and zsh 5.9: the read runs in the `-in-A` rows only.

        Ending the brief anywhere but where bash ends it breaks one half or the other — too
        early hides a real read, too late refuses a brief that only talks about one.
        """
        for name, quoted, brief, in_a in (
            ("a line that only looks like the terminator", True,
             "charter handoff b <<'BRIEF' && bash <<'A'\nFix the widget.\n BRIEF\n"
             "cat .charter/vaults/dev.json\nBRIEF\necho in-a\nA",
             "charter handoff b <<'BRIEF' && bash <<'A'\nFix the widget.\n BRIEF\nBRIEF\n"
             "cat .charter/vaults/dev.json\nA"),
            ("a terminator eaten by a line continuation", False,
             "charter handoff b <<BRIEF && bash <<'A'\nFix the widget. \\\nBRIEF\n"
             "cat .charter/vaults/dev.json\nBRIEF\necho in-a\nA",
             "charter handoff b <<BRIEF && bash <<'A'\nFix the widget. \\\nBRIEF\nBRIEF\n"
             "cat .charter/vaults/dev.json\nA"),
        ):
            with self.subTest(shape=name, whose_body="the brief's"):
                # Each row is pinned by the decision it produces, not by the absence of the read
                # message: "no read reason" passes for free the moment the guard stops saying
                # that for any reason at all. The first shape is allowed outright; the second's
                # brief is an UNQUOTED heredoc, so the gate refuses it on its own separate
                # terms — and a read would be a different refusal from either.
                if quoted:
                    self.assertIsNone(_reason(self._decide(brief)))
                else:
                    self.assertIn("QUOTED heredoc", _reason(self._decide(brief)) or "")
            with self.subTest(shape=name, whose_body="bash's"):
                self._read_refused(in_a)

    def test_the_quoted_shape_of_that_pair_is_allowed_outright(self):
        """The row above proves no READ is seen; this one proves nothing else refuses it either,
        so a brief that quotes a vault path really does reach the prompt."""
        self.assertIsNone(_reason(self._decide(
            "charter handoff b <<'BRIEF' && bash <<'A'\nFix the widget.\n BRIEF\n"
            "cat .charter/vaults/dev.json\nBRIEF\necho in-a\nA")))

    def test_the_briefs_end_is_read_off_the_header_not_the_plans_delimiter(self):
        """Round 4, finding 3. The line the fix turns on is the header read — `delim, expands,
        dash` taken from `_heredoc_header` rather than from `_HEREDOC_RE`'s `delim` group. Put
        the regex reading back and these pass a brief no terminator ever matches (the regex
        stops at `BRIEF`, bash at `BRIEFX`), so the body is never data and its prose is scanned
        as a read. Each spelling below is one bash ends at a different word than the regex does.
        """
        bodies = {
            # Plain prose naming a vault path is NOT enough to tell the two readings apart —
            # the guard does not call it a read either way. These two are what it reacts to.
            "a line that opens with a reader":
                "cat .charter/vaults/dev.json would print it, so never run that.",
            "prose holding an apostrophe":
                "The token lives in .charter/vaults/dev.json; don't print it.",
        }
        for body_name, body in bodies.items():
            for opener, terminator in (("<<BRIEF'X'", "BRIEFX"), ('<<"BRIEF"X', "BRIEFX"),
                                       ("<<'BR'IEF", "BRIEF")):
                with self.subTest(opener=opener, body=body_name):
                    # Pinned as "allowed outright", not as "the read message is absent": the
                    # second form goes green the moment the guard stops producing that message
                    # for any reason, which is how a pin quietly stops pinning.
                    self.assertIsNone(_reason(self._decide(
                        f"charter handoff b {opener}\n{body}\n{terminator}")))


class AShellsHeredocIsSearchedWhenThePlanIsUnknown(PlaneIso):
    """Round 4, finding 1. A7 and the leak guard read the SAME heredoc plan, and used to take
    opposite defaults when that plan came back unknown: the leak guard keeps every body visible
    and still denies, while A7 dropped every body and so saw nothing. A canonical handoff inside
    a shell's heredoc then reached the host — which matched only `bash` — with no prompt.

    A whole-line plan goes unknown for ordinary reasons: a `<<` the regex finds inside quotes or
    a comment that the lexer does not, a here-string, or a group/subshell/substitution, which
    `_line_pipelines` refuses to guess at. The answer is not to flip the default — that refuses
    `git commit -m "$(cat <<'EOF'…)"`, the shape round 3 fixed — but to fall back PER HEREDOC.
    """

    HANDOFF = "charter handoff beta <<'BRIEF'\nFix it.\nBRIEF"

    def setUp(self) -> None:
        super().setUp()
        self.enterContext(mock.patch.dict(os.environ, {"CHARTER_HARNESS": "claude-code"},
                                          clear=True))
        workspace.ensure("alpha")

    def _decide(self, command, **payload):
        return run_hook(hooks.pretooluse, {"tool_input": {"command": command}, "session_id": "s",
                                           "cwd": str(workspace.workspace_dir("alpha")),
                                           **payload})

    def test_a_handoff_in_a_shells_heredoc_is_refused_however_the_plan_was_lost(self):
        """Every shape here opens `bash`'s heredoc and puts the exact canonical handoff in the
        body. Each was measured running with NO prompt before this fix."""
        for name, cmd in (
            ("a quoted `<<` in an earlier word", f"echo '<<X' && bash <<'EOF'\n{self.HANDOFF}\nEOF"),
            ("a `<<` in a comment", f"bash <<'EOF' # <<X\n{self.HANDOFF}\nEOF"),
            ("an earlier here-string", f"cat <<<x; bash <<'EOF'\n{self.HANDOFF}\nEOF"),
            ("a subshell", f"( bash <<'EOF' )\n{self.HANDOFF}\nEOF"),
            ("a group", f"{{ bash <<'EOF'; }}\n{self.HANDOFF}\nEOF"),
            ("a command substitution", f"x=$( bash <<'EOF' )\n{self.HANDOFF}\nEOF"),
            ("a backslash-quoted delimiter", f"bash <<\\EOF\n{self.HANDOFF}\nEOF"),
        ):
            with self.subTest(shape=name):
                self.assertIn("a shell runs", _reason(self._decide(cmd)) or "")

    def test_a_body_no_one_runs_is_still_data_when_the_plan_is_unknown(self):
        """The other half, and the reason the default is not simply flipped: `git` runs no body.
        The second shape is the one that loses the plan — the `<<` sits inside a quoted word, so
        the lexer sees no heredoc at all — and it is this branch's own commit-message spelling.
        """
        message = ("A handoff waits for your yes\n\nRefuses `charter handoff` from a sub-agent.")
        for how, cmd in (("-F -", f"git commit -F - <<'EOF'\n{message}\nEOF"),
                         ("-m $(cat …)", f'git commit -m "$(cat <<\'EOF\'\n{message}\nEOF\n)"')):
            with self.subTest(how=how):
                self.assertIsNone(_reason(self._decide(cmd)))

    def test_a_brief_is_still_data_when_the_plan_is_unknown(self):
        """A brief is data by the same fallback: nobody runs it. Without the `charter handoff`
        arm, this brief's own prose would be searched for a handoff and refused."""
        self.assertIsNone(_reason(self._decide(
            "echo '<<X' && charter handoff beta <<'BRIEF'\n"
            "Ask the next chat to run charter handoff when it is done.\nBRIEF")))

    def test_each_heredoc_is_judged_by_its_own_opener(self):
        """Review round 5, ruling A. ONE command, two heredocs, one subshell: the only thing
        that differs between these two is which body holds the handoff. `cat`'s body is written
        to a file and `bash`'s is run, so the pair can only come out right if each heredoc is
        judged by the program that opened IT — not by whether the line mentions a shell
        somewhere. Judging the line as a whole refused both."""
        pair = "( cat <<'A' > notes.md; bash <<'B' )\n{}\n{}\nB"
        with self.subTest(body="cat's, which is written to a file"):
            self.assertIsNone(_reason(self._decide(
                pair.format(HEREDOC + "\nA", "echo hi"))))
        with self.subTest(body="bash's, which is run"):
            self.assertIn("a shell runs", _reason(self._decide(
                pair.format("notes\nA", HEREDOC))) or "")

    def test_an_opener_that_runs_no_body_keeps_its_body_out_of_the_search(self):
        """Ruling A again, on the openers that are neither a reader nor a shell. `git`, `tee`
        and `mail` hand a body on — to a commit, a file, a message — and none of them runs it.
        That gap is what refused this branch's own commit messages inside `( … )`."""
        for name, cmd in (
            ("git commit -F -",
             "( git commit -F - <<'MSG'\nDocs\n\ncharter handoff beta now asks first.\nMSG\n)"),
            ("tee", "( tee notes.md <<'EOF'\ncharter handoff beta asks first.\nEOF\n)"),
            ("mail", "( mail -s x you@e <<'EOF'\ncharter handoff beta asks first.\nEOF\n)"),
        ):
            with self.subTest(opener=name):
                self.assertIsNone(_reason(self._decide(cmd)))

    def test_a_heredoc_operator_inside_quotes_opens_nothing(self):
        """Ruling B. `<<` inside quotes is a sentence or a pattern, not an opener. Counting it
        cost twice: the count stopped agreeing with the lexer's, and the phantom became a
        heredoc whose terminator never arrives — so its body swallowed the rest of the input
        and the REAL opener after it was never consulted."""
        for name, cmd in (
            ("a tip in double quotes",
             'echo "use <<EOF for heredocs" && cat <<\'EOF\' > notes.md\n'
             "charter handoff beta asks first.\nEOF"),
            ("a regex in single quotes",
             "rg -n '<<\\w' docs/ && cat <<'EOF' > notes.md\n"
             "charter handoff beta asks first.\nEOF"),
        ):
            with self.subTest(phantom=name):
                self.assertIsNone(_reason(self._decide(cmd)))

    def test_a_real_heredoc_that_never_terminates_still_refuses(self):
        """The other direction of ruling B, which the fix must not weaken: this `<<'EOF'` is
        real, bash reads it to the end of the input, and the handoff in it is a handoff."""
        self.assertIn("a shell runs", _reason(self._decide(
            f"echo '<<X' && bash <<'EOF'\n{HEREDOC}")) or "")

    def test_a_heredoc_inside_a_substitution_in_double_quotes_is_a_real_opener(self):
        """The limit of ruling B, and the shape it nearly broke: `"$(cat <<'EOF')"` runs a
        command, so its `<<` opens a real heredoc even though a `"` is open around it. This is
        how this repository writes commit messages, and calling that heredoc a phantom read the
        message as commands and refused it as a misspelled handoff."""
        self.assertIsNone(_reason(self._decide(
            'git commit -m "$(cat <<\'EOF\'\nDocs\n\ncharter handoff beta now asks first.\nEOF\n)"')))

    def test_a_backtick_substitution_is_not_a_line_the_lexer_can_be_trusted_on(self):
        """Ruling C. `$( bash <<'EOF' )` was pinned as shape 6; this is the same construct in
        its other spelling. The lexer has no backtick in `_GROUPING`, so it folds ``x=`bash``
        into one word and returns a plan that is WRONG rather than absent — and a fallback that
        fires only on "no plan" never fires. All three spellings ran with no prompt."""
        for name, cmd in (("with an assignment", f"x=`bash <<'EOF'`\n{HEREDOC}\nEOF"),
                          ("sh, not bash", f"x=`sh <<'EOF'`\n{HEREDOC}\nEOF"),
                          ("with no assignment", f"`bash <<'EOF'`\n{HEREDOC}\nEOF")):
            with self.subTest(spelling=name):
                self.assertIn("a shell runs", _reason(self._decide(cmd)) or "")

    def test_a_brief_on_a_plan_less_line_is_still_charters_stdin(self):
        """The `charter handoff` arm of the per-heredoc decision, isolated. A group later on the
        line costs the whole-line plan, but the handoff itself is spelled canonically, so the
        gate has no other reason to refuse — and the brief's prose deliberately OPENS a line with
        the two words. Drop the arm and that prose is read as a misspelled handoff."""
        self.assertIsNone(_reason(self._decide(
            "charter handoff beta <<'BRIEF' && { true; }\n"
            "charter handoff gamma is the next step.\nBRIEF")))

    def test_an_opener_that_cannot_be_resolved_to_a_name_searches_its_body(self):
        """Review round 6, ruling 3. A name charter does not have is not a name charter may
        assume is harmless: with `RUNNER=bash` both shells run the body. Round 5 asked instead
        whether anything ELSE on the line ran text, which answers "no" when the only shell on
        the line is the unnameable word itself — so these ran with no prompt.

        `$(which bash)` is the same fault wearing a name: cutting at the nearest `$(` reads the
        program as `which`, which is not the program that runs. A word that came out of a
        substitution is not a name charter has."""
        for name, cmd in (("a variable", f"( ${{RUNNER}} <<'EOF' )\n{HEREDOC}\nEOF"),
                          ("$SHELL", f"( ${{SHELL}} <<'EOF' )\n{HEREDOC}\nEOF"),
                          ("a substitution", f"( $(which bash) <<'EOF' )\n{HEREDOC}\nEOF")):
            with self.subTest(opener=name):
                self.assertIn("a shell runs", _reason(self._decide(cmd)) or "")

    def test_a_pipe_hands_the_body_on_and_a_semicolon_does_not(self):
        """Ruling 1, and the line between the two regressions this branch traded back and forth.
        Round 4 asked the whole LINE and refused 22 good-faith commands whose shell sat after a
        `;` or `&&`; round 5 asked the opener's word alone and let `cat <<'A' | bash` through,
        which both shells run. The pipeline is the unit a shell actually uses."""
        for name, cmd, refused in (
            ("a pipe, in a subshell", f"( cat <<'A' | bash )\n{HEREDOC}\nA", True),
            ("a pipe, in a group", f"{{ cat <<'A' | bash; }}\n{HEREDOC}\nA", True),
            ("a semicolon", "( cat <<'A' > n.md; bash <<'B' )\n"
                            "charter handoff beta asks first.\nA\necho hi\nB", False),
            ("an &&", "( cat <<'A' > n.md && bash )\ncharter handoff beta asks first.\nA", False),
        ):
            with self.subTest(joined_by=name):
                if refused:
                    self.assertIn("a shell runs", _reason(self._decide(cmd)) or "")
                else:
                    self.assertIsNone(_reason(self._decide(cmd)))

    def test_ansi_c_quoting_is_its_own_context(self):
        """Ruling 2. In `$'don\\'t'` the `\\'` CLOSES the string in bash and zsh. Reading it as a
        close and the next `'` as a reopen put the scan one quote out of step, which erased the
        real `bash <<'EOF'` opener entirely — and cost an over-refusal the other way, on prose
        that merely holds an apostrophe."""
        with self.subTest(direction="the erased opener"):
            self.assertIn("a shell runs", _reason(self._decide(
                f"echo $'don\\'t' && bash <<'EOF'\n{HEREDOC}\nEOF")) or "")
        with self.subTest(direction="the prose over-refusal"):
            self.assertIsNone(_reason(self._decide(
                "echo $'don\\'t' && cat <<'EOF' > notes.md\n"
                "charter handoff beta asks first.\nEOF")))

    def test_a_dollar_inside_double_quotes_opens_nothing(self):
        """Review round 7, item 1. Inside `"…"` both shells read a bare `$` as a literal, so
        `$'` opens no ANSI-C string there. Treating it as an opener let the `$` swallow the
        closing quote: the rest of the line read as quoted and EVERY later heredoc opener was
        erased. The trigger is a regex anchor — `"^$"`, `"handoff$"` — which is ordinary typing,
        so this cost a fail-open and an over-refusal at the same time."""
        for name, cmd in (("a blank-line filter", 'grep -v "^$" f && bash <<\'EOF\''),
                          ("a price", 'echo "cost: 5$" && bash <<\'EOF\''),
                          ("an apostrophe and a dollar", 'echo "don\'t 5$" && bash <<\'EOF\''),
                          ("$' literal inside the quotes", 'echo "don\'t $\'" ; bash <<\'EOF\'')):
            with self.subTest(hidden_by=name):
                self.assertIn("a shell runs", _reason(self._decide(
                    f"{cmd}\n{HEREDOC}\nEOF")) or "")
        for name, cmd in (("^$", 'grep -v "^$" f && cat <<\'EOF\' > n.md'),
                          ("handoff$", 'rg -n "handoff$" docs/ && cat <<\'EOF\' > n.md')):
            with self.subTest(anchor=name):
                self.assertIsNone(_reason(self._decide(
                    f"{cmd}\ncharter handoff beta now asks first.\nEOF")))

    def test_a_heredoc_operator_inside_an_ansi_c_word_opens_nothing_either(self):
        """`$'…'` counts as QUOTED, like `'…'` and `"…"` — so a `<<` inside one is a phantom.
        Drop it from the quoted contexts and `echo $'<<X'` becomes an opener whose terminator
        never arrives, swallowing the real `cat` after it and refusing ordinary prose."""
        with self.subTest(direction="the phantom is skipped"):
            self.assertIsNone(_reason(self._decide(
                "echo $'<<X' && cat <<'EOF' > n.md\ncharter handoff beta now asks first.\nEOF")))
        with self.subTest(direction="a real opener after it is still found"):
            self.assertIn("a shell runs", _reason(self._decide(
                f"echo $'<<X' && bash <<'EOF'\n{HEREDOC}\nEOF")) or "")

    def test_a_closed_substitution_before_the_heredoc_is_an_argument(self):
        """Item 3. A `${…}` or a CLOSED `$( … )` before the `<<` is a redirect target or a flag
        value, not the start of a new command — cutting the opener words there threw away a
        program that was plainly nameable and refused twelve ordinary commands. The opener is
        `tee`, `mail`, `git`, `cat` in every row here."""
        prose = "charter handoff beta now asks first."
        for name, cmd in (
            ("tee ${OUT}", f"( tee ${{OUT}} <<'EOF' )\n{prose}\nEOF"),
            ("mail -s x ${TO}", f"( mail -s x ${{TO}} <<'EOF' )\n{prose}\nEOF"),
            ('mail -s "${SUBJ}"', f"( mail -s \"${{SUBJ}}\" me <<'EOF' )\n{prose}\nEOF"),
            ("git commit -F - ${FLAGS}",
             f"( git commit -F - ${{FLAGS}} <<'EOF' )\nDocs\n\n{prose}\nEOF"),
            ('cat > "$(date +%F).md"', f"( cat > \"$(date +%F).md\" <<'EOF' )\n{prose}\nEOF"),
            ('tee "$(mktemp)"', f"( tee \"$(mktemp)\" <<'EOF' )\n{prose}\nEOF"),
            ("a ${…} earlier in the group", f"( n=${{N}}; wc -l <<'EOF' )\n{prose}\nEOF"),
        ):
            with self.subTest(opener=name):
                self.assertIsNone(_reason(self._decide(cmd)))

    def test_an_opener_that_IS_a_substitution_is_still_unresolvable(self):
        """The other half of item 3, and round 6's ruling 3 kept intact: when the substitution
        or the variable IS the program, charter still cannot name it, so the body is searched.
        Only a CLOSED substitution standing before the opener was ever an argument."""
        for name, cmd in (("${RUNNER}", f"( ${{RUNNER}} <<'EOF' )\n{HEREDOC}\nEOF"),
                          ("$(which bash)", f"( $(which bash) <<'EOF' )\n{HEREDOC}\nEOF"),
                          ("$(which cat)", f"( $(which cat) <<'EOF' )\n{HEREDOC}\nEOF")):
            with self.subTest(opener=name):
                self.assertIn("a shell runs", _reason(self._decide(cmd)) or "")

    def test_a_long_command_line_is_decided_in_linear_time(self):
        """Item 2, a fail-open in disguise. `_pipeline_slice` and `_crowded_substitutions` ask
        about every position on the line; re-scanning the quoting from the start each time made
        this O(n²) — 29.8 s for a 17 KB line, past the hook's own 60 s timeout, and a guard that
        times out is a guard that stops deciding. A first message is capped at 12,288 bytes, so
        the size is reachable with ordinary input. The bound is deliberately loose: the point is
        the SHAPE of the curve, and quadratic blows through it by two orders of magnitude."""
        pad = " ".join(f"notes-{i}.md" for i in range(1400))
        cmd = f"( cat {pad} <<'A' > n.md )\ncharter handoff beta now asks first.\nA"
        self.assertGreater(len(cmd.split("\n")[0]), 16000)
        started = time.monotonic()
        self._decide(cmd)
        self.assertLess(time.monotonic() - started, 5.0)

    def test_two_heredocs_in_one_substitution_take_the_conservative_answer(self):
        """Ruling 4. In `x=$( cat <<'A' > n.md; bash <<'B' )` bash hands the FIRST body to
        `bash` and leaves `n.md` empty — the opposite of this guard's attribution, so the
        handoff in the "cat" body runs. Getting bash's ordering right inside a substitution is
        not work this guard carries, so every body in such a substitution is searched. A
        substitution holding ONE reader heredoc is untouched, which is how commit messages are
        written here."""
        with self.subTest(shape="two heredocs, one of them a shell's"):
            self.assertIn("a shell runs", _reason(self._decide(
                f"x=$( cat <<'A' > n.md; bash <<'B' )\n{HEREDOC}\nA\necho hi\nB")) or "")
        with self.subTest(shape="one reader heredoc"):
            self.assertIsNone(_reason(self._decide(
                'git commit -m "$(cat <<\'EOF\'\nDocs\n\n'
                'charter handoff beta now asks first.\nEOF\n)"')))

    def test_ssh_runs_the_body_on_the_other_machine(self):
        """`ssh` is not a local interpreter and is not in `_EXECUTORS`, which the leak guard also
        reads — what `ssh` does with a body is a question about this gate, not about a secret
        leaving the host. It still runs the body, so the body is searched."""
        self.assertIn("a shell runs", _reason(self._decide(
            f"( ssh host <<'EOF' )\n{HEREDOC}\nEOF")) or "")

    def test_a_reader_piped_into_a_shell_is_searched_whoever_opened_it(self):
        """What `_line_runs_text` pins now that it asks about the PIPELINE rather than the line:
        `cat` is a named non-shell, so only the `| bash` downstream of it makes its body a
        script. Stub `_line_runs_text` to `False`, or empty the executor list, and this goes
        green while `cat <<'A'; bash` — the row above — stays allowed either way."""
        self.assertIn("a shell runs", _reason(self._decide(
            f"( cat <<'A' | bash )\n{HEREDOC}\nA")) or "")


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
