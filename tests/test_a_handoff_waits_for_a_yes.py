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
import subprocess
import tempfile
import time
import unittest
from contextlib import redirect_stderr
from pathlib import Path
from types import SimpleNamespace
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

    def test_the_pipeline_starts_after_the_command_before_it(self):
        """`_pipeline_slice` finds where the opener's pipeline BEGINS, and the rows below are
        what that boundary decides. Each has something ahead of the pipe on the same line — a
        backtick command, a closed substitution, an ANSI-C word, another command in the group —
        and the body still reaches `bash`, so it is still a script. Stop advancing the start and
        all four go quiet."""
        for name, cmd in (
            ("after a backtick command", "`true`; cat <<'A' | bash"),
            ("after a closed substitution", "x=$(cat <<'Q'\nhi\nQ\n); cat <<'A' | bash"),
            ("after an ANSI-C word", "echo $'don\\'t' && cat <<'A' | bash"),
            ("after another command in the group", "( echo hi; cat <<'A' | bash )"),
        ):
            with self.subTest(preceded_by=name):
                self.assertIn("a shell runs", _reason(self._decide(
                    f"{cmd}\n{HEREDOC}\nA")) or "")

    def test_a_later_reader_in_a_group_keeps_its_own_body(self):
        """The other direction of that boundary: `bash` runs FIRST here and `cat` writes the
        second body to a file, so the handoff in `cat`'s body is data. Reading the pipeline from
        the start of the line instead would hand `bash` both."""
        self.assertIsNone(_reason(self._decide(
            f"( bash <<'A'; cat <<'B' > n.md )\necho hi\nA\n{HEREDOC}\nB")))

    def test_a_heredoc_operator_with_no_delimiter_is_still_not_a_quoted_brief(self):
        """`charter handoff beta <<` opens a heredoc with no delimiter word at all. The slice
        that asks whether the delimiter is bare is EMPTY, and empty is where `all` and `any`
        part company — `all` says "treat it as unquoted" and refuses, `any` says nothing is
        there and lets the call through. Malformed input still has to get an answer."""
        r = self._decide("charter handoff beta <<")
        self.assertIn("QUOTED heredoc", _reason(r) or "")

    def test_a_punctuation_token_knows_where_it_ends(self):
        """The lexer records where each token ENDS, and a punctuation run ends by the same rule
        as a word — the spelling check reads the gap between tokens off those offsets. Take
        punctuation out of that rule and its end comes back as -1, which reads as "one before
        the start" and silently mismeasures every gap after it."""
        for line, expected in (("a;b", ("a", 0, 1)), ("a;b", (";", 1, 2)), ("a;b", ("b", 2, 3))):
            with self.subTest(token=expected[0]):
                toks = hooks._split_punctuation(hooks._lex(line))
                self.assertIn(expected, [(t.text, t.start, t.end) for t in toks])

    def test_a_backtick_substitution_is_a_group_in_the_opener_cut_too(self):
        """Review round 8, finding 1. The cut tracked `$( … )`, `( … )` and `{ … }` but not a
        backtick, so a separator INSIDE the backticks moved the command start past the real
        program: ``( bash `d; cat ` <<'EOF' )`` read its opener as `cat`. The `$( … )` spelling
        of the same command refused it, so this was an arbitrary gap, not a limit.

        Not every row here RUNS the handoff, and the difference is worth keeping straight. With
        the stubs present, three do in both bash 3.2.57 and zsh 5.9 — the `;`, the `&&` and the
        group row. `ssh` hands the body to a shell on another machine, so nothing runs locally;
        the `echo a; true` and quoted-backtick rows exit 127, and the interpreter row exits 1.
        The reason to refuse them all is the same either way: the opener is a shell, and which
        of them happens to be installed is not what the guard decides on."""
        for name, cmd in (
            ("a `;` inside backticks", "( bash `d; cat ` <<'EOF' )"),
            ("an `&&` inside backticks", "( bash `d && tee ` <<'EOF' )"),
            ("ssh, not a local shell", "( ssh h `d; cat ` <<'EOF' )"),
            ("inside a group", "{ bash `d; cat ` <<'EOF' ; }"),
            ("a whole command inside", "( bash `echo a; true ` <<'EOF' )"),
            ("backticks inside quotes", '( bash "`d; cat `" <<\'EOF\' )'),
            ("an interpreter", "( python3 - `d; cat ` <<'EOF' )"),
        ):
            with self.subTest(shape=name):
                self.assertIn("a shell runs", _reason(self._decide(
                    f"{cmd}\n{HEREDOC}\nEOF")) or "")

    def test_the_quote_map_opens_a_backtick_substitution_inside_double_quotes(self):
        """`_quote_map`'s backtick arm, which survived the whole A7 file unpinned. A backtick
        opens a substitution exactly as `$(` does, so what follows it is NOT quoted even inside
        `"…"` — and the `<<` there is a real opener. Pinned on the contract, because the verdict
        alone cannot see it: another guard refuses this line either way, which is precisely how
        the arm went unnoticed."""
        line = 'x="`bash <<EOF`"'
        self.assertFalse(hooks._inside_quotes(line, line.index("<<")))
        self.assertEqual(1, len(hooks._heredoc_openers(line)))

    def test_a_group_opener_saves_the_command_start(self):
        """The `c in "({"` push, also unpinned. `(` and `{` begin a command, so the opener words
        start after them; stop pushing and the words are read from before the group, where the
        program is something else entirely."""
        self.assertEqual(["bash"], hooks._heredoc_opener_words(
            "x=1; ( bash <<'A' )", "x=1; ( bash <<'A' )".index("<<")))
        self.assertEqual(["cat"], hooks._heredoc_opener_words(
            "bash -c x; { cat <<'A' ; }", "bash -c x; { cat <<'A' ; }".index("<<")))

    def test_a_backtick_pair_inside_double_quotes_is_seen_whole(self):
        """Review round 9, finding 1 — round 8's backtick tracking biting back. `_quote_map`
        marks the OPENING backtick of a pair inside `"…"` as quoted and its closing partner as
        not, which is right for that map and half a pair here: the cut saw one end, toggled the
        wrong way, and returned `['"']` for the opener words. Three of these four are the
        backtick spellings of rows round 7 restored, and no shell runs anything in any of them.
        """
        prose = "charter handoff beta now asks first."
        for name, cmd in (("tee", '( tee "`date`" <<\'EOF\' )'),
                          ("cat with a redirect", '( cat > "`date +%F`.md" <<\'EOF\' )'),
                          ("tee with mktemp", '( tee "`mktemp`" <<\'EOF\' )'),
                          ("mail with a subject", '( mail -s "`subj`" me <<\'EOF\' )')):
            with self.subTest(opener=name):
                self.assertIsNone(_reason(self._decide(f"{cmd}\n{prose}\nEOF")))
        with self.subTest(direction="a shell inside the quoted backticks still refuses"):
            self.assertIn("a shell runs", _reason(self._decide(
                f'( bash "`d; cat `" <<\'EOF\' )\n{HEREDOC}\nEOF')) or "")

    def test_an_escaped_backtick_is_not_a_substitution(self):
        """The other half of the same fix. Escaped backticks are literal, so `( bash a\\`d; cat
        \\`b <<'EOF' )` really is two commands and the opener is `cat` — while the unescaped
        spelling is one command whose opener is `bash`. The cut has to tell them apart."""
        esc = "( bash a\\`d; cat \\`b <<'EOF' )"
        self.assertEqual(["cat", "\\`b"], hooks._heredoc_opener_words(esc, esc.index("<<")))
        unesc = "( bash a`d; cat `b <<'EOF' )"
        self.assertEqual("bash", hooks._opener_program(
            hooks._heredoc_opener_words(unesc, unesc.index("<<"))))

    def test_a_dangling_quote_after_the_heredoc_does_not_reach_back(self):
        """Review round 9. An unbalanced quote AFTER the `<<` leaves the tail of the line quoted,
        and the opener cut walks only up to the `<<` — so the group it is inside must still be
        seen. Miss it and the words come back with the `(` attached, which the round-8 pin below
        did NOT catch: it used a line with no dangling quote, so it did not pin what it claimed.
        """
        for line in ("( tee n.md <<'EOF' ) '", '( tee n.md <<\'EOF\' ) "'):
            with self.subTest(tail=line[-1]):
                self.assertEqual(["tee", "n.md"],
                                 hooks._heredoc_opener_words(line, line.index("<<")))

    def test_an_unterminated_substitution_after_the_heredoc_is_not_the_opener(self):
        """A trailing `$(` that never closes. The cut looks for where the command STARTS, which
        is behind the `<<`; reading the line from the other end instead loses the program
        entirely and every one of these becomes "cannot be named", which searches the body."""
        for line, words, prog in (("( tee n.md <<'EOF' ) $(", ["tee", "n.md"], "tee"),
                                  ("( bash <<'EOF' ) $(", ["bash"], "bash"),
                                  ("cat <<'A' | bash $(", ["cat"], "cat")):
            with self.subTest(line=line):
                got = hooks._heredoc_opener_words(line, line.index("<<"))
                self.assertEqual(words, got)
                self.assertEqual(prog, hooks._opener_program(got))

    def test_a_closed_group_earlier_on_the_line_does_not_end_the_substitution_scan(self):
        """The depth counter in `_crowded_substitutions`. An unquoted `)` that closes something
        else — a subshell, a `case` arm, a function definition — must not be mistaken for the
        end of the substitution being counted. Mistake it and the conservative rule stops firing
        after any earlier `)`, and both shells run the handoff in the body that follows."""
        line = "( true ); x=$( cat <<'A' > n.md; bash <<'B' )"
        self.assertEqual({0, 1}, hooks._crowded_substitutions(line))
        self.assertIn("a shell runs", _reason(self._decide(
            f"{line}\n{HEREDOC}\nA\necho hi\nB")) or "")

    def test_an_unnameable_opener_among_crowded_heredocs_does_not_crash_the_hook(self):
        """`_crowded_substitutions` asks whether any opener in the substitution is an executor,
        and an opener that cannot be NAMED answers `None`. Feed that to the name check without a
        fallback and the hook raises `TypeError` — a guard that raises is a guard that has
        stopped guarding, on an ordinary shape."""
        for line in ("x=$( ${RUNNER} <<'A'; cat <<'B' )", "x=$(<<'A'; bash <<'B')"):
            with self.subTest(line=line):
                hooks._crowded_substitutions(line)      # must not raise
                self._decide(f"{line}\ncharter handoff beta now asks first.\nA\nx\nB")

    def test_a_comment_after_a_brief_opener_adds_no_brief(self):
        """`_brief_heredocs` counts the heredocs the LEXER sees against the ones the regex
        finds, and bails when they disagree. `# <<'B'` is a comment the regex counts and the
        lexer does not, so the line is not one this pass can take apart — and claiming the first
        heredoc is a brief there would drop a body the leak guard should still read."""
        self.assertEqual(set(), hooks._brief_heredocs("charter handoff b <<'A' # <<'B'"))

    def test_only_a_BARE_charter_handoff_opens_a_brief(self):
        """`_brief_heredocs` asks that both words be bare — unquoted and unescaped. A quoted or
        escaped spelling is one A7 refuses outright, so treating its body as a brief would drop
        text the leak guard should still be reading. Neither spelling moves a verdict, because
        the gate refuses them first; the contract is what holds them apart."""
        for line in ("charter 'handoff' <<'BRIEF'", "\\charter handoff <<'BRIEF'"):
            with self.subTest(spelling=line):
                self.assertEqual(set(), hooks._brief_heredocs(line))
        self.assertEqual({0}, hooks._brief_heredocs("charter handoff b <<'BRIEF'"))

    def test_a_heredoc_with_nothing_before_it_names_no_program(self):
        """`_heredoc_opener_words` returns None, not an empty list, when there is no command in
        front of the `<<` — `_opener_program` reads None as "cannot be named", which is what
        sends the body to be searched. An empty list would read as a program list that simply
        has no entries."""
        self.assertIsNone(hooks._heredoc_opener_words("<<'BRIEF'", 0))

    def test_one_heredoc_in_a_substitution_is_not_a_crowd(self):
        """The conservative substitution rule needs TWO or more heredocs before it fires. With
        one, ordinary attribution decides — `x=$( bash <<'EOF' )` is already searched because
        `bash` opened it, and forcing the whole substitution would be a different reason for the
        same answer, hiding the case where the rule really is needed."""
        self.assertEqual(set(), hooks._crowded_substitutions("x=$( bash <<'EOF' )"))
        self.assertEqual({0, 1}, hooks._crowded_substitutions("x=$( cat <<'A'; bash <<'B' )"))

    def test_the_opener_words_are_the_commands_own_words(self):
        """A unit pin, because the decision alone cannot see this: `_split_env` happens to
        filter a stray leading `(`, so a cut that keeps the bracket reaches the same verdict by
        luck. The contract is the command's OWN words, and resting a guard on another
        function's tolerance is how a defect hides until that function changes."""
        for line in ("( cat <<'A' > n.md )", "{ cat <<'A' > n.md; }", "( ( cat <<'A' ) )"):
            with self.subTest(line=line):
                self.assertEqual(["cat"], hooks._heredoc_opener_words(line, line.index("<<")))

    def test_a_substitution_that_closed_before_the_opener_is_behind_it(self):
        """`x=$(cat <<'Q' … Q)` finishes before `bash <<'EOF'` starts, so the opener of the
        second heredoc is `bash` and its body is a script. Lose the guard that knows the
        substitution is behind the `<<` and the opener is read out of the wrong command."""
        self.assertIn("a shell runs", _reason(self._decide(
            f"x=$(cat <<'Q'\nhi\nQ\n); bash <<'EOF'\n{HEREDOC}\nEOF")) or "")

    def test_two_separate_substitutions_are_not_one_crowded_one(self):
        """The conservative substitution rule counts heredocs PER substitution. Here each `$( )`
        holds one, so `cat`'s body stays data even though a shell opens the other — count them
        across the line instead and this ordinary pair is refused."""
        self.assertIsNone(_reason(self._decide(
            f"x=$( cat <<'A' > n.md ); y=$( bash <<'B' )\n{HEREDOC}\nA\necho hi\nB")))

    def test_a_heredoc_with_no_program_in_front_of_it_is_still_decided(self):
        """`( <<'EOF' )` opens a heredoc with nothing before it, so the opener words come back
        empty and no program can be named. The guard has to answer anyway — drop the empty
        check and it raises `TypeError` inside the hook instead of deciding, which is a guard
        that has stopped guarding."""
        self.assertIn("a shell runs", _reason(self._decide(
            f"( <<'EOF' )\n{HEREDOC}\nEOF")) or "")

    def test_a_command_with_no_heredoc_never_builds_a_layout(self):
        """The fast path in `_lines_a_command_could_run`, pinned by the work it avoids rather
        than by a clock: every Bash call in the plane reaches this, and almost none of them open
        a heredoc. Removing it sent a 52 KB ordinary command from 0.1 ms to 34 ms — measured,
        and the reason the path is there."""
        with mock.patch.object(hooks, "_heredoc_layout") as layout:
            rows = hooks._lines_a_command_could_run("echo hi && git status")
        layout.assert_not_called()
        self.assertEqual([("echo hi && git status", False)], rows)

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

    #: The two hints this row can give, asserted WHOLE. A fragment (`"reinit" in hint`) passes
    #: while the sentence around it says something false — which is what it did: the hint told
    #: the operator to add a rule and then that they already had it, in one breath.
    ADD = ("A handoff's brief becomes a new chat's first message and runs with your authority; "
           "this rule is the prompt that asks you first, and on Claude Code it asks under "
           "bypassPermissions too. Add it: charter guard ask 'charter handoff *'. Removing it "
           "is your choice — this row says so, and charter does not put it back.")
    _LEAD = ("A handoff's brief becomes a new chat's first message and runs with your "
             "authority; this rule is the prompt that asks you first, and on Claude Code it "
             "asks under bypassPermissions too. ")
    _TAIL = " Removing it is your choice — this row says so, and charter does not put it back."
    _WHERE = ("charter writes these settings at the plane root and at a workspace's or "
              "checkout's own root, and a chat reads them from the directory it starts in")
    #: Unwired directory, plane HOLDS the rule: no command, and no guarantee it cannot make
    #: from here — a workspace whose layer is behind is not gated, which is the state this row
    #: exists to surface.
    NOWHERE = (_LEAD + "It cannot be put in force for a chat rooted in this directory: " +
               _WHERE + ". A chat started in the plane root is gated; charter gates a "
               "workspace or a checkout once it has written the rule there, and this row says "
               "so in any whose layer is behind." + _TAIL)
    #: Unwired directory, NOTHING holds the rule: `guard ask` is not inert — run from here it
    #: writes the plane's settings — so it is named, with what it cannot do said plainly.
    NOWHERE_ADD = (_LEAD + "Add it: charter guard ask 'charter handoff *' — that gates a chat "
                   "started in the plane root, and reaches workspaces and checkouts by "
                   "mirroring into them. It cannot gate a chat rooted in this directory: " +
                   _WHERE + "." + _TAIL)
    REFRESH = ("A handoff's brief becomes a new chat's first message and runs with your "
               "authority; this rule is the prompt that asks you first, and on Claude Code it "
               "asks under bypassPermissions too. The plane already holds it for claude-code, "
               "so this directory's layer is behind — nothing to add, only to refresh: charter "
               "workspace reinit fleet. Removing it is your choice — this row says so, and "
               "charter does not put it back.")

    def test_a_session_rooted_in_a_stale_workspace_is_sent_to_reinit(self):
        """The plane HOLDS the rule and this directory does not, which is a different problem
        from never having written one: nothing needs adding, the layer here is behind. So the
        hint names `charter workspace reinit` INSTEAD of `charter guard ask` — asserted whole,
        because printed together the two halves contradict each other."""
        self._claude_rule(config.ROOT)
        self._opencode_rule()
        self.rooted_at(self.workspace)
        r = doctor.check_handoff_gate()
        self.assertEqual(r.status, WARN, r)
        self.assertEqual(self.REFRESH, r.hint)

    def test_a_plane_that_never_had_the_rule_is_told_to_add_it(self):
        """The other side, and the reason the clause is conditional: with no rule in the plane
        either, `reinit` would copy nothing."""
        self._opencode_rule()
        self.rooted_at(self.workspace)
        r = doctor.check_handoff_gate()
        self.assertEqual(r.status, WARN, r)
        self.assertEqual(self.ADD, r.hint)

    def test_a_directory_charter_does_not_wire_is_given_no_command(self):
        """Review round 2. Charter writes these settings at the plane root and at a workspace's
        or checkout's own root; Claude Code reads them from the session's EXACT directory
        (#855). So for a chat rooted anywhere else there is NO command that puts the rule in
        force — `charter guard ask` is as inert there as `reinit` was before round 1 stopped
        naming it, and all three of these rows were measured unchanged after running it.

        The row still warns, because the rule really is not in force for that chat. What it
        must not do is name a command that cannot work: a hint with no command beats a hint
        with a command that does nothing."""
        self._claude_rule(config.ROOT)
        self._opencode_rule()
        deep = self.workspace / "repo" / "src" / "deep"
        for name, where in (("docs/", config.ROOT / "docs"),
                            ("a persona directory", config.ROOT / "personas" / "x"),
                            ("a deep directory inside a checkout", deep)):
            with self.subTest(rooted_in=name):
                where.mkdir(parents=True, exist_ok=True)
                self.rooted_at(where.resolve())
                r = doctor.check_handoff_gate()
                self.assertEqual(WARN, r.status, r)
                self.assertEqual(self.NOWHERE, r.hint)

    def test_an_unwired_directory_with_no_rule_anywhere_still_names_guard_ask(self):
        """The state round 2 got wrong: with NOTHING holding the rule, `charter guard ask` is
        not inert here — run from `docs/` it writes the plane's own settings, and the plane root
        and every workspace go clean afterwards. Round 2 suppressed a useful command along with
        a useless one, because all three states it measured happened to hold the rule already.

        The cross-directory walk is the evidence: run the command where the hint appears, then
        stand in the directory the hint talks about and read the row there."""
        docs = config.ROOT / "docs"
        docs.mkdir(parents=True, exist_ok=True)
        self.rooted_at(docs.resolve())
        r = doctor.check_handoff_gate()
        self.assertEqual(WARN, r.status, r)
        self.assertEqual(self.NOWHERE_ADD, r.hint)
        commands.cmd_guard_ask(SimpleNamespace(pattern="charter handoff *", local=False,
                                               dry_run=False, yes=True))
        with self.subTest(then_standing_in="the plane root"):
            self.rooted_at(config.ROOT.resolve())
            self.assertEqual(OK, doctor.check_handoff_gate().status)
        with self.subTest(then_standing_in="a workspace"):
            self.rooted_at(self.workspace)
            self.assertEqual(OK, doctor.check_handoff_gate().status)

    def test_the_no_command_sentence_is_true_with_a_workspace_whose_layer_is_behind(self):
        """State A's sentence claims something about OTHER directories, so it must stay true in
        the worst of them: a workspace whose layer is behind is NOT gated, and that is the very
        state this row exists to surface. So the sentence promises the plane root — measured —
        and says of a workspace only that charter gates it once it has written there, and that
        this row says so in any that is behind. Standing in `docs/`, charter cannot see which
        workspaces are current, so it must not enumerate a guarantee."""
        self._claude_rule(config.ROOT)        # the plane holds it
        self._opencode_rule()                 # the workspace layer does NOT
        docs = config.ROOT / "docs"
        docs.mkdir(parents=True, exist_ok=True)
        self.rooted_at(docs.resolve())
        self.assertEqual(self.NOWHERE, doctor.check_handoff_gate().hint)
        with self.subTest(walked_to="the plane root — the one thing the sentence promises"):
            self.rooted_at(config.ROOT.resolve())
            self.assertEqual(OK, doctor.check_handoff_gate().status)
        with self.subTest(walked_to="the stale workspace — which the sentence does NOT promise"):
            self.rooted_at(self.workspace)
            self.assertEqual(WARN, doctor.check_handoff_gate().status)

    def test_the_plane_root_is_still_told_to_add_it(self):
        """The plane root IS a directory charter writes, so `guard ask` works there and the row
        keeps naming it — the no-command sentence is about directories charter does not wire,
        not about everywhere that is not a workspace."""
        self._opencode_rule()
        self.rooted_at(config.ROOT.resolve())
        r = doctor.check_handoff_gate()
        self.assertEqual(WARN, r.status, r)
        self.assertEqual(self.ADD, r.hint)

    def test_the_harness_that_is_missing_the_rule_is_the_one_named(self):
        """With only opencode lacking the rule, `reinit` copies nothing — opencode generates no
        workspace files at all. Blaming a stale Claude Code layer there names a command that
        cannot help, however true the row's WARN is."""
        self._claude_rule(config.ROOT)
        self._claude_rule(self.workspace)
        self.rooted_at(self.workspace)
        r = doctor.check_handoff_gate()
        self.assertEqual(r.status, WARN, r)
        self.assertIn("under opencode", r.detail)
        self.assertEqual(self.ADD, r.hint)

    def _workspaces_through_a_symlink(self) -> None:
        """Point charter's `workspaces/` at a SYMLINK to itself, so every path it builds from
        that directory needs normalising before it can be compared with a session's cwd.

        Built here rather than relied upon: `SessionRootCase` documents that macOS reaches its
        temp directory through `/var` → `/private/var`, and that is exactly the accident these
        two `.resolve()` calls were leaning on. On Linux there is no such symlink, so dropping
        either `.resolve()` left CI's whole suite green while the same mutation died on a
        developer's machine — a guard pinned by the platform and not by a test (#982's sweep,
        `drop-normalise` at `_reinit_target`). A symlink the fixture makes itself pins them
        everywhere.
        """
        real = (config.ROOT / "workspaces").resolve()
        real.mkdir(parents=True, exist_ok=True)
        link = config.ROOT / "workspaces-by-another-name"
        if not link.is_symlink():
            link.symlink_to(real, target_is_directory=True)
        self.enterContext(mock.patch.object(config, "WORKSPACES_DIR", link))

    def test_a_workspace_reached_through_a_symlink_is_still_this_workspace(self):
        """The session stands at the workspace's REAL path; charter builds the same directory's
        path through the symlink. They are one directory, and only normalising says so."""
        self._workspaces_through_a_symlink()
        self._claude_rule(config.ROOT)
        self._opencode_rule()
        self.rooted_at(self.workspace)
        r = doctor.check_handoff_gate()
        self.assertEqual(WARN, r.status, r)
        self.assertEqual(self.REFRESH, r.hint)

    def test_a_guest_checkout_reached_through_a_symlink_is_still_that_checkout(self):
        """The same for the guest tree, whose path charter builds by walking the symlinked
        workspace directory — a second `.resolve()`, and the sweep reads the pair together
        because neither was pinned alone."""
        self._workspaces_through_a_symlink()
        repo = self.workspace / "repo"
        repo.mkdir(parents=True, exist_ok=True)
        # env=None: the suite has already redirected git's config (#641).
        subprocess.run(["git", "init", "-q", str(repo)], check=True)
        self._claude_rule(config.ROOT)
        self._opencode_rule()
        self.rooted_at(repo.resolve())
        r = doctor.check_handoff_gate()
        self.assertEqual(WARN, r.status, r)
        self.assertEqual(self.REFRESH, r.hint)

    def test_one_harness_holding_the_rule_is_enough_to_call_the_layer_behind(self):
        """`any`, not `all`, over the harnesses that are missing the rule. With two missing and
        only one of them held by the plane, the two answers part company: `any` says the layer
        here is behind — which it is, for Claude Code — while `all` says nothing is held and
        offers `guard ask`. Every earlier test had a single missing harness, where the two
        quantifiers agree, which is how this survived (#982's sweep, `swap-synonym`)."""
        self._claude_rule(config.ROOT)          # the plane holds it for claude-code
        # …and NOT for opencode, so both harnesses are missing here but only one is held.
        docs = config.ROOT / "docs"
        docs.mkdir(parents=True, exist_ok=True)
        self.rooted_at(docs.resolve())
        r = doctor.check_handoff_gate()
        self.assertEqual(WARN, r.status, r)
        self.assertIn("opencode", r.detail)
        self.assertEqual(self.NOWHERE, r.hint)

    def test_a_chat_in_a_guest_checkout_is_sent_to_reinit(self):
        """A checkout inside a workspace is a directory `reinit` writes, so the clause belongs
        there too — and nothing pinned it. Deleting the guest-checkout match left the suite
        green, because `charter guard ask` mirrors into guest checkouts as a side effect and
        the row would still clear; only the HINT would be wrong, saying "Add it" where the
        plane already holds the rule. That is the exact contradiction this PR fixed, restored
        with every test passing (#982 review round 2), so this uses a real checkout."""
        repo = self.workspace / "repo"
        repo.mkdir(parents=True, exist_ok=True)
        # env=None: the suite has already redirected git's config, and building one by hand is
        # what `tests/_planeguard.py` refuses (#641).
        subprocess.run(["git", "init", "-q", str(repo)], check=True)
        self._claude_rule(config.ROOT)
        self._opencode_rule()
        self.assertIn(repo.resolve(), [t.resolve() for t in workspace.guest_trees("fleet")])
        self.rooted_at(repo.resolve())
        r = doctor.check_handoff_gate()
        self.assertEqual(WARN, r.status, r)
        self.assertEqual(self.REFRESH, r.hint)

    def test_an_unreadable_workspaces_directory_cannot_decide_the_plane_root(self):
        """`charter guard ask` writes at the plane root, so the advice there cannot depend on
        whether some *other* directory is readable. Asked after the stale clause it did: the
        clause reads `workspaces/` to find a reinit target, and an unreadable one reached the
        answer — emitting "cannot be put in force" where `guard ask` genuinely works, a FALSE
        sentence rather than an inert one (#982 review round 2b).

        Worse than the wrong sentence, and why this is ordered rather than merely defaulted:
        `list_workspaces` raising escaped `_reinit_target` entirely — its own `try` is inside
        the loop — and took `charter doctor` down with it, the whole command, not this row."""
        self._opencode_rule()
        self.rooted_at(config.ROOT.resolve())
        with mock.patch.object(workspace, "list_workspaces", side_effect=OSError("EACCES")):
            r = doctor.check_handoff_gate()
        self.assertEqual(WARN, r.status, r)
        self.assertEqual(self.ADD, r.hint)

    def test_when_charter_cannot_tell_where_it_stands_it_gives_the_useless_answer(self):
        """The default when even the plane-root comparison raises. CONSTRUCTED — nothing makes
        `Path.resolve` fail on a real plane — so read it as a guard, not as coverage.

        It answers `guard ask` rather than the no-command sentence because the two fail
        differently: `guard ask` is right at the plane root and merely inert elsewhere, while
        "it cannot be put in force" is FALSE at the plane root, where `guard ask` works. An
        inert hint costs a minute; a false one is believed."""
        self._opencode_rule()
        docs = config.ROOT / "docs"
        docs.mkdir(parents=True, exist_ok=True)
        self.rooted_at(docs.resolve())
        with mock.patch.object(type(config.ROOT), "resolve", side_effect=OSError("EIO")):
            r = doctor.check_handoff_gate()
        self.assertEqual(WARN, r.status, r)
        self.assertEqual(self.ADD, r.hint)

    def test_an_unreadable_workspaces_directory_still_answers_elsewhere(self):
        """And away from the plane root it answers rather than raising: charter cannot tell
        whether this directory is one it wires, so it says what is true of the rule itself."""
        self._opencode_rule()
        docs = config.ROOT / "docs"
        docs.mkdir(parents=True, exist_ok=True)
        self.rooted_at(docs.resolve())
        with mock.patch.object(workspace, "list_workspaces", side_effect=OSError("EACCES")):
            r = doctor.check_handoff_gate()
        self.assertEqual(WARN, r.status, r)
        self.assertEqual(self.NOWHERE_ADD, r.hint)

    def test_one_unreadable_workspace_does_not_hide_the_rest(self):
        """`_reinit_target`'s `except OSError: continue`. No plane builds a workspace whose path
        raises, so this is a CONSTRUCTED shape — like the harness fail-safe above, read it as a
        guard, not as coverage. It is kept rather than deleted because the alternative is an
        `OSError` escaping into the preflight from a directory charter only wanted to compare
        against, which would take the whole row down instead of skipping one workspace."""
        self._claude_rule(config.ROOT)
        self._opencode_rule()
        self.rooted_at(self.workspace)
        real = workspace.workspace_dir

        def one_raises(name):
            if name == "boom":
                raise OSError("stat: I/O error")
            return real(name)

        (config.ROOT / "workspaces" / "boom").mkdir(parents=True, exist_ok=True)
        with mock.patch.object(workspace, "workspace_dir", side_effect=one_raises):
            r = doctor.check_handoff_gate()
        self.assertEqual(WARN, r.status, r)
        self.assertEqual(self.REFRESH, r.hint)

    def test_a_harness_reinit_writes_nothing_for_is_not_sent_to_reinit(self):
        """`reinit` can only fix a harness whose layer it carries into this directory.

        **This test reaches a branch no real plane reaches**, and says so rather than reading
        as coverage. Every harness but Claude Code is judged at the plane, so "missing here"
        already implies "missing at the plane" and the clause has bowed out before this line
        matters — which is why it had no red test. The shape below is CONSTRUCTED: Claude Code
        with its `workspace_files` patched empty, which is a harness that is judged at the
        session root and generates nothing there. Kept because it is the instruction in code
        ("if nothing fixes it, say nothing rather than naming a command that will not work"),
        and the day another harness generates workspace files its absence would be a defect
        nobody could trace (#982 review round 1)."""
        self._claude_rule(config.ROOT)
        self._opencode_rule()
        self.rooted_at(self.workspace)
        h = registry.get(registry.CLAUDE_CODE)
        with mock.patch.object(type(h), "workspace_files", return_value={}):
            r = doctor.check_handoff_gate()
        self.assertEqual(r.status, WARN, r)
        self.assertEqual(self.ADD, r.hint)

    def test_a_plane_that_denies_the_handoff_holds_no_ask_rule(self):
        """One notion of "the plane holds this rule", not two. The predicate is the row's own
        `apply_ask_rule` reading one root over; a substring test over `ask` + `deny` flattened
        said yes to a plane that DENIES `charter handoff`, which holds no ask rule at all."""
        p = config.ROOT / ".claude" / "settings.json"
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(json.dumps({"permissions": {"deny": [RULE]}}))
        self._opencode_rule()
        self.rooted_at(self.workspace)
        r = doctor.check_handoff_gate()
        self.assertEqual(r.status, WARN, r)
        self.assertEqual(self.ADD, r.hint)


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


class TheAskRuleReachesAWorkspaceChat(PlaneIso):
    """The gate's other half, which Phase 1 could only assert indirectly (chat handoff, task 4,
    phase 2).

    A chat launched in `workspaces/<ws>/` reads that directory's settings and nowhere else, so
    the plane's `ask` rule is in force there only if it rides into the generated document. #942
    (PR #948) built that path; these are the tests that the HANDOFF rule travels it — #948's own
    suite never names `charter handoff`, so this is coverage rather than a second opinion.

    What #948 mirrors is its business, and `deny` is asserted here nowhere: two suites pinning
    one behaviour from different angles is how a guard ends up half-pinned.
    """

    PLUGINS = {"charter@charter": True}

    def _plane(self, permissions: dict) -> dict:
        """The plane's settings, as `charter init` leaves them plus *permissions*; returns the
        generated workspace document."""
        p = config.ROOT / ".claude"
        p.mkdir(parents=True, exist_ok=True)
        p.joinpath("settings.json").write_text(
            json.dumps({"enabledPlugins": self.PLUGINS, "permissions": permissions}))
        files = registry.get(registry.CLAUDE_CODE).workspace_files()
        return json.loads(files[".claude/settings.json"]) if ".claude/settings.json" in files \
            else {}

    def test_a_workspace_settings_document_carries_the_planes_asks(self):
        """The rule reaches the chat that runs the command. Asserted as membership plus the
        `allow` invariant rather than equality on the whole `permissions` object: equality
        breaks the moment a neighbouring feature legitimately adds a key, and this test should
        fail only when the handoff gate stops travelling."""
        perms = self._plane({"ask": [RULE], "allow": ["Bash(ls *)"]})["permissions"]
        self.assertIn(RULE, perms["ask"])
        self.assertNotIn("allow", perms)

    def test_an_allow_rule_never_travels_into_a_workspace(self):
        """The security invariant, from the angle the test above cannot reach: a permissive
        rule must not appear ANYWHERE in the generated text, whichever plane file it came
        from. `ask` and `deny` restrict; `allow` widens, and widening is the plane's own
        business.

        **Both generated sets, because they read different plane files.** The local file's
        rules ride only in `checkout_files` (`.claude/settings.local.json`); iterating
        `workspace_files` alone reads the shared document twice and never looks at the local
        one — which is what the first version of this test did, while its name and this PR's
        body both claimed otherwise. #948's own `test_a_local_grant_does_not_travel_either`
        pins the local half; this one holds the handoff-shaped plane to the same line."""
        p = config.ROOT / ".claude"
        p.mkdir(parents=True, exist_ok=True)
        p.joinpath("settings.json").write_text(json.dumps(
            {"enabledPlugins": self.PLUGINS,
             "permissions": {"ask": [RULE], "allow": ["Bash(ls *)"]}}))
        p.joinpath("settings.local.json").write_text(json.dumps(
            {"permissions": {"allow": ["Bash(curl *)"], "ask": ["Bash(local *)"]}}))
        h = registry.get(registry.CLAUDE_CODE)
        generated = {**h.workspace_files(), **h.checkout_files()}
        self.assertIn(".claude/settings.local.json", generated, "the local file must be read")
        for rel, text in generated.items():
            with self.subTest(generated=rel):
                self.assertNotIn("Bash(ls *)", text)
                self.assertNotIn("Bash(curl *)", text)
                self.assertNotIn('"allow"', text)

    def test_an_ask_bucket_of_the_wrong_shape_travels_as_nothing(self):
        """A string where a list belongs is a plane that declares no rules charter can read —
        not a plane declaring one rule spelled oddly. Nothing travels, and the document is
        still written for what it does carry."""
        self.assertNotIn("permissions", self._plane({"ask": "Bash(x)"}))

    def test_a_rule_that_is_not_a_string_is_dropped(self):
        """One unusable entry costs itself, not the rules beside it: a handoff gate that
        travels only when every neighbouring rule is well-formed is a gate that stops being
        in force because of someone else's typo."""
        self.assertEqual(["Bash(a *)"],
                         self._plane({"ask": ["Bash(a *)", 3]})["permissions"]["ask"])

    def test_the_layer_row_still_judges_by_charters_own_keys(self):
        """`doctor`'s `workspace layer` row compares the keys charter authors. The rules ride
        in the same document but are the plane's, not charter's, so they must not be added to
        that list — a plane that changes an unrelated `ask` would otherwise read as charter's
        layer having gone stale."""
        from charter.harness import claude_code
        self.assertEqual(("enabledPlugins", "env"), claude_code.WORKSPACE_KEYS)

    def test_a_launch_refreshes_a_workspace_the_plane_gave_a_new_ask(self):
        """The rule has to reach workspaces that already exist. A plane that adds the gate
        after a workspace was made would otherwise leave that chat ungated until someone
        happened to run `reinit`."""
        self._plane({})
        workspace.ensure("w")
        settings = config.ROOT / "workspaces" / "w" / ".claude" / "settings.json"
        self.assertNotIn("permissions", json.loads(settings.read_text()))
        self._plane({"ask": [RULE]})
        workspace.ensure("w")
        self.assertIn(RULE, json.loads(settings.read_text())["permissions"]["ask"])


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
