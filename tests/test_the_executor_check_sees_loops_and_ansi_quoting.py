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

import unittest

from charter import hooks

V = ".charter/vaults/x.json"
R = f"cat {V}"


def denies(cmd: str) -> bool:
    return hooks._leak_reason(cmd, "/x") is not None


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


if __name__ == "__main__":
    unittest.main()
