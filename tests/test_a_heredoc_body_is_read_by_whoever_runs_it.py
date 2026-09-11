"""A heredoc body is stripped from the leak guard's view only when it is stdin *data*.

`_strip_reader_heredocs` decided that by the LINE's first token (#973): whenever a line
*started* with a reader (`cat`, `head`, …), the body of whatever `<<` that line opened was
dropped — even when the command actually reading that heredoc was a chained or piped shell.
So `cat x && bash <<'EOF'` / `… | bash` / `… || bash` / `… & bash`, and `cat x; bash`, each
fed a real vault read to a shell that ran it, and the guard never saw the body.

The fix attributes each heredoc to the command that opens it — the program of the SEGMENT
containing that `<<`, and the PIPELINE that segment belongs to — and strips a body only when
it is a quoted (non-expanding) heredoc fed to a reader with no executor anywhere downstream
in its pipeline. Everything else stays visible, so the guard's own newline-segmentation
reads each body line as the command it is.

Measured against real `bash`, `zsh` (the harness shell) and `dash`: a heredoc body always
follows its `<<` operator in physical order across the whole line, whichever command opens
it, and a shell/`xargs` anywhere in the opener's pipeline executes it — in `zsh` (`MULTIOS`)
even a body a pipe would seem to discard. Every one of those must reach the guard.

The whole matrix runs through the FULL `hooks.pretooluse`, not `_leak_reason` alone, because
that is the surface a Bash tool call actually hits. Two direct `_leak_reason` families pin
the #258 preservation and the terminator direction, which are unit facts about the pre-pass.
"""

from __future__ import annotations

import unittest

from charter import hooks
from tests._isolation import PlaneIso, run_hook

VAULT = ".charter/vaults/dev.json"
READ = f"cat {VAULT}"          # a real read a SHELL would execute
MENTION = f"see {VAULT} for the layout"   # prose that merely names the path

#: Every separator that ends one command and starts another, in the two spellings a `<<`
#: can straddle: a control operator and a bare newline.
SEPARATORS = [";", "&&", "||", "|", "&", "\n"]
#: The first word of the line — the token the old pre-pass (`_reader_of`, removed by #973)
#: judged the whole line by.
FIRST_WORDS = ["cat x", "head x", "true", "echo hi"]
#: Programs that RUN the body they receive.
SHELLS = ["bash", "sh"]


def _deny(payload_cmd: str, cwd: str) -> bool:
    r = run_hook(hooks.pretooluse,
                 {"tool_input": {"command": payload_cmd}, "cwd": cwd, "session_id": "s"})
    return (r or {}).get("hookSpecificOutput", {}).get("permissionDecision") == "deny"


class TheGuardSeesEveryBodyAShellRuns(PlaneIso):
    """The FULL `pretooluse` matrix: separator × first word × the shell that runs the body."""

    def denies(self, cmd: str) -> bool:
        return _deny(cmd, str(self.tmp))

    def test_the_matrix_is_answered_both_ways(self):
        """Guard against a vacuous suite: the matrix below must contain denials AND allows,
        or an assertEqual that always holds would prove nothing."""
        self.assertTrue(self.denies(f"bash <<'EOF'\n{READ}\nEOF"))
        self.assertFalse(self.denies(f"cat <<'EOF'\n{READ}\nEOF"))

    def test_a_shell_after_any_separator_after_any_first_word_is_denied(self):
        for first in FIRST_WORDS:
            for sep in SEPARATORS:
                for shell in SHELLS:
                    joint = f"{first}{sep}" if sep == "\n" else f"{first} {sep} "
                    cmd = f"{joint}{shell} <<'EOF'\n{READ}\nEOF"
                    with self.subTest(first=first, sep=repr(sep), shell=shell):
                        self.assertTrue(self.denies(cmd), cmd)

    def test_an_unquoted_shell_heredoc_is_denied_too(self):
        """The delimiter's quoting does not change who runs the body."""
        for shell in SHELLS:
            cmd = f"cat x && {shell} <<EOF\n{READ}\nEOF"
            with self.subTest(shell=shell):
                self.assertTrue(self.denies(cmd), cmd)

    def test_a_reader_after_any_separator_keeps_its_body_as_data(self):
        """The other direction, and the #258 preservation at full volume: a quoted heredoc
        fed to a reader is stdin data even when its text looks like a command, because the
        reader never runs it. Denying these is the false positive #258 removed."""
        for first in FIRST_WORDS:
            for sep in SEPARATORS:
                joint = f"{first}{sep}" if sep == "\n" else f"{first} {sep} "
                cmd = f"{joint}cat <<'EOF'\n{READ}\nEOF"
                with self.subTest(first=first, sep=repr(sep)):
                    self.assertFalse(self.denies(cmd), cmd)


class WhichCommandOpensTheHeredoc(PlaneIso):
    """Attribution facts: the body follows the `<<` in physical order, and a pipeline shares
    one execution fate."""

    def denies(self, cmd: str) -> bool:
        return _deny(cmd, str(self.tmp))

    def test_two_heredocs_on_one_line_go_to_different_programs(self):
        """`cat <<A && bash <<B`: A's body is data (dropped), B's body is a script (kept).
        Bodies follow the operators in order, so B's read must survive A's strip."""
        cmd = f"cat <<'A' && bash <<'B'\n{MENTION}\nA\n{READ}\nB"
        self.assertTrue(self.denies(cmd), cmd)

    def test_the_same_two_with_the_shell_first(self):
        """`bash <<B && cat <<A`: bodies are B then A. The read is B's, ahead of A's data."""
        cmd = f"bash <<'B' && cat <<'A'\n{READ}\nB\n{MENTION}\nA"
        self.assertTrue(self.denies(cmd), cmd)

    def test_a_reader_heredoc_piped_into_a_shell_is_not_data(self):
        """`cat <<X | bash`: the opener is a reader, but its output pipes to a shell that
        runs the body. An executor anywhere in the pipeline keeps the body visible."""
        self.assertTrue(self.denies(f"cat <<'EOF' | bash\n{READ}\nEOF"))
        self.assertTrue(self.denies(f"cat <<'EOF' | tee out | bash\n{READ}\nEOF"))

    def test_a_versioned_interpreter_downstream_is_an_executor_too(self):
        """`python3.12` is `python3` with a suffix. A name list that knew only the bare
        spelling would call this pipeline executor-free and strip a body an interpreter
        runs."""
        self.assertTrue(self.denies(f"cat <<'EOF' | python3.12\n{READ}\nEOF"))

    def test_a_wrapper_in_front_of_a_reader_is_not_an_executor(self):
        """`env cat <<'X'` is a reader behind a wrapper; `env` runs `cat`, not the body. Its
        quoted body is data, as #258 requires."""
        self.assertFalse(self.denies(f"env cat <<'EOF'\n{READ}\nEOF"))

    def test_a_heredoc_in_the_second_of_three_segments(self):
        cmd = f"cat x; bash <<'EOF' && echo done\n{READ}\nEOF"
        self.assertTrue(self.denies(cmd), cmd)

    def test_a_reader_piped_into_a_reader_stays_data(self):
        """No executor anywhere: `head <<X | grep foo` is a data stream both ends. Stripped,
        so a documented mention is not read."""
        cmd = f"head <<'EOF' | grep foo\n{MENTION}\nEOF"
        self.assertFalse(self.denies(cmd), cmd)


class TheTrapsBashParsesOneWay(PlaneIso):
    """Constructs whose `;`/`&&`/`<<` a naive splitter would read wrong."""

    def denies(self, cmd: str) -> bool:
        return _deny(cmd, str(self.tmp))

    def test_a_separator_inside_quotes_is_not_a_separator(self):
        """`echo 'a; b' && bash <<'EOF'` — the quoted `;` is one word; the real boundary is
        the `&&`, and bash runs the body."""
        cmd = f"echo 'a; b' && bash <<'EOF'\n{READ}\nEOF"
        self.assertTrue(self.denies(cmd), cmd)

    def test_a_separator_inside_a_substitution_is_not_a_boundary(self):
        cmd = f'echo "$(echo a; echo b)" && bash <<\'EOF\'\n{READ}\nEOF'
        self.assertTrue(self.denies(cmd), cmd)

    def test_an_escaped_semicolon_keeps_one_command(self):
        r"""`cat x \; bash <<'EOF'` is ONE `cat` reading files `x`, `;`, `bash`; the heredoc
        is cat's stdin *data*, so the body is not executed and stays stripped."""
        cmd = f"cat x \\; bash <<'EOF'\n{READ}\nEOF"
        self.assertFalse(self.denies(cmd), cmd)

    def test_a_dash_heredoc_with_tab_indented_body(self):
        cmd = f"cat x && bash <<-'EOF'\n\t{READ}\n\tEOF"
        self.assertTrue(self.denies(cmd), cmd)

    def test_a_delimiter_that_looks_like_an_operator(self):
        """`bash <<'&&'` — the quoted delimiter is a word, not a boundary; its body runs."""
        cmd = f"cat x && bash <<'&&'\n{READ}\n&&"
        self.assertTrue(self.denies(cmd), cmd)

    def test_a_here_string_is_not_a_heredoc(self):
        """`cat <<<"x"` opens no heredoc, so the next line is its own command and is read.
        The old pre-pass mistook `<<<x` for a heredoc `<<x` and stripped the read away."""
        cmd = f'cat <<<"x"\n{READ}\nx'
        self.assertTrue(self.denies(cmd), cmd)

    def test_a_heredoc_marker_inside_a_comment_opens_nothing(self):
        cmd = f"cat x # <<'EOF'\n{READ}\nEOF"
        self.assertTrue(self.denies(cmd), cmd)

    def test_a_quoted_heredoc_marker_opens_nothing(self):
        cmd = f'cat "<<EOF"\n{READ}\nEOF'
        self.assertTrue(self.denies(cmd), cmd)

    def test_a_grouped_reader_piped_into_a_shell(self):
        """`{ cat <<X; } | bash` routes the group's output to a shell. The attribution bails
        on a group rather than guess, which keeps the body visible — the safe direction."""
        cmd = f"{{ cat <<'EOF'\n{READ}\nEOF\n}} | bash"
        self.assertTrue(self.denies(cmd), cmd)


class TheStripPrePassAsAUnit(PlaneIso):
    """`_strip_reader_heredocs` facts that are simplest asserted on the pre-pass itself."""

    def test_258_quoted_reader_body_is_dropped(self):
        """The reported #258 case, and the terminator direction: the body is removed so a
        document naming charter's layout is not read as a read of it."""
        cmd = f"cat > workspaces/ws/doc.md <<'DOC'\n{READ}\nDOC"
        self.assertIsNone(hooks._leak_reason(cmd))

    def test_258_unquoted_prose_body_is_allowed(self):
        cmd = "cat > notes.md <<DOC\nsee .charter/vaults/ for the layout\nDOC"
        self.assertIsNone(hooks._leak_reason(cmd))

    def test_an_unquoted_reader_body_runs_its_substitution(self):
        """An unquoted body is expanded by the shell before the reader sees it, so a command
        substitution in it runs. It must NOT be stripped, unlike a quoted body."""
        cmd = f"cat <<EOF\n$(cat {VAULT})\nEOF"
        self.assertIsNotNone(hooks._leak_reason(cmd))

    def test_a_kept_body_only_ends_where_bash_ends_it_or_earlier(self):
        """A terminator with trailing spaces is not the terminator to bash; the pre-pass may
        end a body earlier than bash (showing the guard more), never later."""
        cmd = f"bash <<'EOF'\n{READ}\nEOF"
        self.assertIsNotNone(hooks._leak_reason(cmd))


if __name__ == "__main__":
    unittest.main()
