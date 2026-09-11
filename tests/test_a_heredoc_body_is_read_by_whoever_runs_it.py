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


class APipelineContinuesAcrossPhysicalLines(PlaneIso):
    """A trailing pipe continues the pipeline onto the next command line — the line AFTER the
    heredoc bodies. The executor is on that continuation line, so a per-physical-line plan
    would miss it and strip a body a shell runs. Measured on bash, zsh AND dash: every
    `SECRET RAN` shape below runs a planted secret; the `&&`/`||` shapes do not, because there
    the heredoc stays with its own segment's program and is not piped downstream.
    """

    def denies(self, cmd: str) -> bool:
        return _deny(cmd, str(self.tmp))

    def test_trailing_pipe_then_a_shell_on_the_next_line(self):
        """The reviewer's shape: `cat <<'EOF' |` \\n body \\n EOF \\n bash — bash runs the
        body. Allowed on base; must deny."""
        for tail in ("bash", "sh"):
            cmd = f"cat <<'EOF' |\n{READ}\nEOF\n{tail}"
            with self.subTest(tail=tail):
                self.assertTrue(self.denies(cmd), cmd)

    def test_trailing_pipe_with_a_reader_opener(self):
        cmd = f"head <<'EOF' |\n{READ}\nEOF\nbash"
        self.assertTrue(self.denies(cmd), cmd)

    def test_a_writer_opener_piped_onward_to_a_shell(self):
        """`tee x <<'EOF' |` \\n … \\n bash — tee reproduces the body to the pipe, bash runs
        it."""
        cmd = f"tee x <<'EOF' |\n{READ}\nEOF\nbash"
        self.assertTrue(self.denies(cmd), cmd)

    def test_a_three_stage_pipeline_across_lines(self):
        cmd = f"cat <<'EOF' |\n{READ}\nEOF\ntee y |\nbash"
        self.assertTrue(self.denies(cmd), cmd)

    def test_a_backslash_newline_continuation_into_a_pipe(self):
        r"""`cat <<'EOF' \` \\n `| bash` folds to `cat <<'EOF' | bash`, and the body follows."""
        cmd = f"cat <<'EOF' \\\n| bash\n{READ}\nEOF"
        self.assertTrue(self.denies(cmd), cmd)

    def test_and_continuation_leaves_the_heredoc_with_its_own_segment(self):
        """`cat <<'EOF' &&` \\n body \\n EOF \\n bash — measured on bash/zsh/dash: the body is
        NOT piped to bash (that is a separate command), so a body that only mentions a path is
        data and stays allowed."""
        cmd = f"cat <<'EOF' &&\n{MENTION}\nEOF\nbash"
        self.assertFalse(self.denies(cmd), cmd)

    def test_or_continuation_the_same(self):
        cmd = f"cat <<'EOF' ||\n{MENTION}\nEOF\nbash"
        self.assertFalse(self.denies(cmd), cmd)

    def test_a_trailing_ampersand_is_not_a_pipeline_continuation(self):
        """`cat <<'EOF' &` backgrounds cat; `bash` on the next line is a separate command that
        never receives the body. Measured: the secret does not run."""
        cmd = f"cat <<'EOF' &\n{MENTION}\nEOF\nbash\nwait"
        self.assertFalse(self.denies(cmd), cmd)


class TheTerminatorMatchesBashExactly(PlaneIso):
    """A heredoc ends where bash ends it, not at the first `.strip()`-equal line. A lenient
    match ends a KEPT (shell) body EARLY, and the real body lines that spill past it are then
    read as the NEXT heredoc's body — which, if that one is a reader's, gets stripped, hiding
    the read. Measured on bash, zsh and dash: the secret runs; denied on main; the lenient
    pre-pass had regressed it to allowed."""

    def denies(self, cmd: str) -> bool:
        return _deny(cmd, str(self.tmp))

    def test_a_lenient_terminator_does_not_end_a_shell_body_early(self):
        """`bash <<'A'` whose body contains a line ` A` (leading space) — not the terminator
        to bash — followed by the real read, then the exact `A`. The read is inside bash's
        body, and a following `cat <<'B'` must not swallow and strip it."""
        cmd = f"bash <<'A' && cat <<'B'\necho hi\n A\n{READ}\nB\nA\ndata\nB"
        self.assertTrue(self.denies(cmd), cmd)

    def test_an_unquoted_body_backslash_splices_over_a_terminator(self):
        """An UNQUOTED heredoc body splices a trailing backslash with the next line, so a
        line that looks like the terminator is eaten and the body runs on. Measured on
        bash/zsh/dash: `bash <<A` … `echo a\\` … `A` … read … `A` runs the read. The kept
        shell body must extend past the spliced `A`, not hand the read to a later reader
        heredoc to strip."""
        cmd = f"bash <<A && cat <<'B'\necho a\\\nA\n{READ}\nA\ndata\nB"
        self.assertTrue(self.denies(cmd), cmd)

    def test_a_quoted_body_does_not_splice_a_trailing_backslash(self):
        """The other half of the splice rule, and a leak-hider without it. A QUOTED body is
        literal, so a line ending in `\\` splices nothing and the next line still terminates
        it. Splicing regardless of quoting would run `cat`'s body on past its `A`, swallow the
        read meant for `bash <<'B'` into the dropped reader body, and allow a command that
        leaks in bash, zsh and dash."""
        cmd = f"cat <<'A' && bash <<'B'\ndata\\\nA\n{READ}\nB"
        self.assertTrue(self.denies(cmd), cmd)

    def test_a_dash_terminator_still_strips_leading_tabs(self):
        """`<<-` strips leading tabs from body and terminator alike, so a tab-indented
        terminator still ends the body. A quoted reader body naming a path stays data (#258),
        proving the tab-aware match did not run the body off the end."""
        cmd = "cat > notes.md <<-'DOC'\n\t.charter/vaults/db.json\n\tDOC"
        self.assertIsNone(hooks._leak_reason(cmd))


class ThePlanCarriesBashsOwnDelimiter(PlaneIso):
    """Each plan entry carries `_heredoc_header`'s answer as DATA, so a second caller has the
    bash-accurate delimiter without re-parsing — and decides nothing here.

    Where quoting splits the delimiter word, the two readings differ: for `<<EO'F'` the
    pre-pass's regex reads `EO` while bash reads `EOF` (measured against bash, zsh and dash).
    Acting on the shorter reading is the safe direction *here* — a terminator that is never
    found leaves the body visible — so the verdicts stay keyed on it. A caller that would DROP
    a body (a `charter handoff` brief, say) must use the carried header instead, or it would
    drop to end of input and swallow the commands after the heredoc.
    """

    SPLIT_QUOTED = ["EO'F'", "'EO'F", '"EO"F']

    def test_every_split_quote_spelling_exposes_bashs_delimiter(self):
        for spell in self.SPLIT_QUOTED:
            header = "cat <<" + spell
            with self.subTest(spelling=spell):
                plan = hooks._heredoc_strip_plan(header)
                self.assertEqual(1, len(plan), header)
                self.assertEqual("EOF", plan[0][2][0], "carried delimiter is bash's")
                self.assertEqual("EO", plan[0][0], "the pre-pass still matches on its own")

    def test_the_carried_header_decides_nothing(self):
        """The verdicts of those spellings are exactly what they were before the facts were
        carried — the data is inert."""
        for spell, denied in (("EO'F'", True), ("'EO'F", False), ('"EO"F', False),
                              ("\\EOF", True)):
            cmd = f"cat <<{spell}\nbody\nEOF\n{READ}"
            with self.subTest(spelling=spell):
                self.assertEqual(denied, _deny(cmd, str(self.tmp)), cmd)

    def test_a_backslash_delimiter_has_no_entry_to_carry(self):
        """`<<\\EOF` is not matched by the pre-pass's header regex at all, so there is no
        entry — the bash-accurate fact is still one `_heredoc_header` call away, and nothing
        is stripped, which is why the verdict above is a denial."""
        header = "cat <<\\EOF"
        self.assertEqual([], hooks._heredoc_strip_plan(header))
        self.assertEqual("EOF", hooks._heredoc_header(header, header.index("<<"))[0])


class TheHeaderCountBailIsLoadBearing(PlaneIso):
    """`_heredoc_strip_plan` bails when the header regex and the lexer disagree on how many
    heredocs a line opens. Deleting the bail raises `IndexError` in `_leak_reason` here, and
    flips a here-string classification — so it is pinned, not incidental (review M4)."""

    def denies(self, cmd: str) -> bool:
        return _deny(cmd, str(self.tmp))

    def test_a_dangling_heredoc_operator_returns_a_decision_without_raising(self):
        """`cat <<'EOF' | bash <<` — the second `<<` names no delimiter, so the lexer counts
        two `<<` and the regex one. The bail must return a clean verdict, not raise."""
        cmd = f"cat <<'EOF' | bash <<\n{READ}\nEOF"
        try:
            decided = self.denies(cmd)
        except Exception as exc:                       # noqa: BLE001 — the point of the pin
            self.fail(f"_leak_reason raised instead of deciding: {exc!r}")
        self.assertTrue(decided, cmd)                  # a shell still runs the body

    def test_a_here_string_before_a_heredoc_keeps_its_verdict(self):
        """`cat <<<x <<'EOF'` — a here-string then a heredoc. The counts disagree, so the
        pre-pass strips nothing; pin the resulting verdict so the bail cannot silently flip
        it."""
        cmd = f"cat <<<x <<'EOF'\n{READ}\nEOF"
        self.assertTrue(self.denies(cmd), cmd)


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

    def test_an_unquoted_heredoc_marker_in_a_comment_opens_nothing(self):
        """The #972-reviewer shape: `# <<EOF` is comment text, so the next line is a command
        and is read. The old regex matched the `<<EOF` and stripped to end of input, hiding
        it. Measured allowed on main `a5aa860`."""
        cmd = f"cat notes.txt # <<EOF\n{READ}"
        self.assertTrue(self.denies(cmd), cmd)

    def test_a_heredoc_marker_in_single_quotes_is_an_argument(self):
        """`grep '<<EOF' file` searches for the literal text `<<EOF`; it opens no heredoc, so
        the next line is read. Measured allowed on main `a5aa860`."""
        cmd = f"grep '<<EOF' file\n{READ}"
        self.assertTrue(self.denies(cmd), cmd)

    def test_an_escaped_heredoc_operator_opens_nothing(self):
        r"""`cat x \<<EOF` — the `\<` is not the heredoc operator, so no body is opened and
        the next line is a command. Measured allowed on main `a5aa860`."""
        cmd = f"cat x \\<<EOF\n{READ}"
        self.assertTrue(self.denies(cmd), cmd)

    def test_a_quoted_heredoc_marker_opens_nothing(self):
        cmd = f'cat "<<EOF"\n{READ}\nEOF'
        self.assertTrue(self.denies(cmd), cmd)

    def test_a_real_heredoc_after_a_comment_marker_still_works(self):
        """The comment's fake `<<EOF` must not swallow a genuine heredoc on a later line. The
        real one is a quoted reader body — stdin data — so its prose mention stays allowed
        (#258), proving the comment did not derail heredoc detection."""
        cmd = f"cat notes.txt # <<EOF\ncat <<'DOC'\n{MENTION}\nDOC"
        self.assertFalse(self.denies(cmd), cmd)

    def test_a_real_shell_heredoc_after_a_comment_marker_is_still_denied(self):
        cmd = f"echo hi # <<EOF\nbash <<'DOC'\n{READ}\nDOC"
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
