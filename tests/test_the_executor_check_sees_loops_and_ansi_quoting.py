r"""The heredoc-body executor check sees loops and ANSI-C quoting (#1086 classes 3 and 4).

A quoted heredoc fed to a reader is data ONLY when no executor stands in its pipeline; an executor
means the body is a script and must stay visible to the guard. Two ways the executor check missed
one, each letting a `cat <vault>` in the body be stripped as data and go unrefused. Both were
measured against real bash and zsh with a fabricated vault.

* **class 3** — `_line_pipelines` split the pipeline on the `;`/`&&` inside a `while … do … done`
  loop (and `for`/`until`/`if`/`case`), so an `eval "$l"` / `sh -c "$l"` in the loop BODY fell
  into a different "pipeline" than the heredoc it consumes. The body read as data. It now bails to
  the same "keep every body visible" answer a `{ … }` group already gets, when a compound holds an
  executor in command position.
* **class 4** — `$'…'` ANSI-C quoting desynced the lexer: shlex reads the `'` in `$'\''` as
  opening a plain quote that swallows a following `| sh -s`, so the executor was never seen. The
  construct is now rewritten to an ordinary shell-quoted token before lexing.

The behaviour is calibrated against main: a pipeline with a plain `| sh` is already refused when
its body names a vault (a conservative rule — any executor downstream keeps the body visible), and
these fixes only bring the loop and ANSI-C spellings in line with it. A loop whose body runs NO
executor (`while read l; do echo "$l"; done`) keeps the body as data and stays allowed.
"""

from __future__ import annotations

import shlex
import unittest

from charter import hooks

V = ".charter/vaults/x.json"
R = f"cat {V}"


def denies(cmd: str) -> bool:
    return hooks._leak_reason(cmd, "/x") is not None


def reason(cmd: str):
    return hooks._leak_reason(cmd, "/x")


class ALoopBodyThatRunsThePipedHeredocIsRefused(unittest.TestCase):
    """#1086 class 3: the executor inside the loop is seen, so the body is a script."""

    def test_while_read_eval(self):
        self.assertTrue(denies(f"cat <<'EOF' | while read l; do eval \"$l\"; done\n{R}\nEOF"))

    def test_while_read_sh_c(self):
        self.assertTrue(denies(f"cat <<'EOF' | while read l; do sh -c \"$l\"; done\n{R}\nEOF"))

    def test_while_read_bash_c(self):
        self.assertTrue(denies(f"cat <<'EOF' | while read l; do bash -c \"$l\"; done\n{R}\nEOF"))

    def test_for_loop_body_executor(self):
        self.assertTrue(denies(f"cat <<'EOF' | for l in x; do eval \"$(cat)\"; done\n{R}\nEOF"))

    def test_the_loop_spelling_matches_the_plain_executor_spelling(self):
        """`| sh` is already refused; `| while … eval` now is too — the point of the fix."""
        plain = f"cat <<'EOF' | sh\n{R}\nEOF"
        loop = f"cat <<'EOF' | while read l; do eval \"$l\"; done\n{R}\nEOF"
        self.assertTrue(denies(plain))
        self.assertEqual(denies(plain), denies(loop))


class ANSICQuotingDoesNotHideAnExecutor(unittest.TestCase):
    """#1086 class 4: `$'\\''` no longer desyncs the lexer past a following `| sh`."""

    def test_ansi_c_quote_before_a_pipe_to_sh(self):
        self.assertTrue(denies(f"cat - <<'EOF' $'\\'' | sh -s \\'\n{R}\nEOF"))

    def test_desugar_keeps_the_token_boundary_and_the_value(self):
        # `$'\''` is one apostrophe; the `| sh` after it stays a separate token.
        out = hooks._desugar_ansi_c("cat - <<'EOF' $'\\'' | sh -s \\'")
        toks = [t.text for t in hooks._lex(out)]
        self.assertIn("|", toks)
        self.assertIn("sh", toks)
        # a decoded program word is preserved, not replaced by a placeholder.
        self.assertEqual([t.text for t in hooks._lex(hooks._desugar_ansi_c("$'sh' x"))],
                         ["sh", "x"])

    def test_an_ansi_c_string_inside_quotes_is_left_alone(self):
        """Inside `'…'` the `$` is literal, so it is not an ANSI-C string and is not rewritten."""
        self.assertEqual("echo '$'\"'\"'x'\"'\"''",
                         hooks._desugar_ansi_c("echo '$'\"'\"'x'\"'\"''"))


class WhatIsStillDataStaysAllowed(unittest.TestCase):
    """The fixes fire only on an executor; a loop that just echoes keeps the body as data."""

    def test_a_loop_with_no_executor_is_data(self):
        self.assertFalse(denies(f"cat <<'EOF' | while read l; do echo \"$l\"; done\n{R}\nEOF"))

    def test_eval_as_an_argument_is_not_an_executor(self):
        """`grep eval` names `eval` as an argument, not a command; the body stays data."""
        self.assertFalse(denies(f"cat <<'EOF' | while read l; do grep eval f; done\n{R}\nEOF"))


class CompoundHoldsExecutorUnit(unittest.TestCase):
    """`_compound_holds_executor` requires the executor in command position inside a compound."""

    def toks(self, s):
        return hooks._split_punctuation(hooks._lex(s))

    def test_executor_in_loop_body(self):
        self.assertTrue(hooks._compound_holds_executor(self.toks("while read l; do eval x; done")))

    def test_no_compound(self):
        self.assertFalse(hooks._compound_holds_executor(self.toks("eval x")))

    def test_executor_as_argument_in_compound(self):
        self.assertFalse(hooks._compound_holds_executor(self.toks("for x in a; do grep eval f; done")))

    # --- #1091 deletion-sweep pins: each conjunct of the if/elif told from its absence ---

    def test_a_quoted_compound_word_opens_no_loop(self):
        """972 `t.bare`: a QUOTED `"while"` is a plain word, not the loop keyword, so it holds
        no executor.  Dropping `t.bare` from the `if` counts the quoted word as a loop and,
        with the `eval` behind it, returns True."""
        self.assertFalse(
            hooks._compound_holds_executor(self.toks('"while" read l; do eval x; done')))
        self.assertTrue(
            hooks._compound_holds_executor(self.toks("while read l; do eval x; done")))

    def test_a_compound_word_as_an_argument_arms_no_loop(self):
        """972 `prev in _COMMAND_POSITION_AFTER`: `case` here is echo's argument, not a command
        word, so it opens no compound.  Dropping the command-position guard lets the argument
        arm `seen_compound`, and the later `eval` then reads as a loop body."""
        self.assertFalse(hooks._compound_holds_executor(self.toks("echo case; eval z")))
        self.assertTrue(hooks._compound_holds_executor(self.toks("while read l; do eval x; done")))

    def test_a_quoted_executor_is_not_in_command_position(self):
        """974 `t.bare`: command position is read off `bare`, so a QUOTED `'eval'` — deliberate
        obfuscation, out of scope per `_leak_reason`'s docstring — does not count as the loop's
        executor.  Dropping `t.bare` from the `elif` flips this to True."""
        self.assertFalse(
            hooks._compound_holds_executor(self.toks("while read l; do 'eval' x; done")))
        self.assertTrue(
            hooks._compound_holds_executor(self.toks("while read l; do eval x; done")))


class EachAnsiCEscapeByteIsDecoded(unittest.TestCase):
    r"""#1091 lines 916-917: every entry of :data:`_ANSI_C_ESCAPES` is told from its absence.

    `$'\L'` decodes to a control byte; spelled just before the tail of an executor name it
    breaks the name (`$'\a'sh` is `<BEL>sh`, not a program that runs the piped body), so the
    quoted heredoc stays DATA and its `cat <vault>` line is stripped — ALLOW. Retune (or drop)
    that one dict entry and `$'\L'` falls through to the bare letter, completing a real executor
    (`ash`), which keeps the body visible and refuses the read — DENY. The bare-letter twin is
    the DENY control. One case per escape letter, the byte deciding the verdict."""

    def _assert(self, real: str, disguised: str):
        # the real executor keeps the piped body visible -> the vault read is refused.
        self.assertEqual(hooks._READ_REASON, reason(f"cat <<'EOF' | {real}\n{R}\nEOF"))
        # the escape decodes to a control byte, so the name is not that executor -> body is
        # data and the read is stripped.  The retune/drop mutant decodes the letter -> DENY.
        self.assertIsNone(reason(f"cat <<'EOF' | {disguised}\n{R}\nEOF"))

    def test_bell_a(self):       self._assert("ash",  "$'\\a'sh")
    def test_tab_t(self):        self._assert("tsx",  "$'\\t'sx")
    def test_newline_n(self):    self._assert("node", "$'\\n'ode")
    def test_formfeed_f(self):   self._assert("fish", "$'\\f'ish")
    def test_backspace_b(self):  self._assert("bash", "$'\\b'ash")
    def test_return_r(self):     self._assert("ruby", "$'\\r'uby")
    def test_vtab_v(self):       self._assert("eval", "e$'\\v'al")
    def test_escape_e(self):     self._assert("eval", "$'\\e'val")


class DesugarReadsARawLineWithinItsBounds(unittest.TestCase):
    r"""#1091 lines 934-952: :func:`_desugar_ansi_c` scans a raw, possibly unbalanced line, so
    each index into it is bounded and each unknown escape has a fallback.  Pinned here on the
    function because a trailing backslash never survives the folded guard path (it splices
    first), while the function must still not read past the end when handed one directly."""

    def test_an_unterminated_ansi_c_string_stops_at_the_end(self):
        # 934 line-916/944 `j < n`: `$'ab` with no closing quote walks j to n.  `j <= n`, or
        # dropping the bound, reads cmd[n] and raises.  Through the guard it is a real command.
        self.assertEqual("ab", hooks._desugar_ansi_c("$'ab"))
        self.assertEqual(hooks._READ_REASON, reason(f"cat <<'EOF' | sh x $'ab\n{R}\nEOF"))

    def test_a_trailing_backslash_is_literal_not_an_index_past_the_end(self):
        # 945 `j + 1 < n`: a `\` as the last character has no escapee; without the bound
        # cmd[j+1] reads past the end.  Bash keeps the lone backslash literal.
        self.assertEqual(shlex.quote("x\\"), hooks._desugar_ansi_c("$'x\\"))

    def test_an_unknown_escape_passes_through_as_the_following_character(self):
        # 946 `.get(cmd[j+1], cmd[j+1])`: `\s` is not a defined escape, so it decodes to `s`;
        # dropping the fallback returns None and "".join raises.  `$'\s'h` -> `sh`, an executor
        # that keeps the piped body visible, so the read is refused.
        self.assertEqual("s", hooks._desugar_ansi_c("$'\\s'"))
        self.assertEqual(hooks._READ_REASON, reason(f"cat <<'EOF' | $'\\s'h\n{R}\nEOF"))


if __name__ == "__main__":
    unittest.main()
