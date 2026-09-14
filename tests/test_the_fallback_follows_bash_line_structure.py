r"""The guard's multi-line reading follows bash's line and quote structure (#1082, #1086 classes 1-2).

Three defects shared one cause: the guard's handling of newlines, quotes and comments across
physical lines diverged from bash, and each divergence let a `cat <vault>` on one line be read as
data (or folded into the segment before it) and go unrefused. Each was measured against real
bash/zsh with a fabricated vault; here the guard verdict is pinned.

* **#1082** — `_segment_argv_parsed`'s fallback split the command with `str.split()`, which erases
  newlines. One unbalanced quote anywhere (a trailing `echo "`) made the whole command
  unparseable, so every following line folded into the segment before it and a `cat x.json` after
  a `cd .charter/vaults` was read as an argument of `cd`. The fallback now cuts on the newlines
  bash makes a boundary and lexes each line on its own.
* **class 1** — a `<<` inside a quoted string that SPANS lines was read as a heredoc opener, so the
  lines after it were stripped as a body bash actually runs as commands. The heredoc layout now
  carries an open quote across the newline, so a `<<` quoted by a string begun on an earlier line
  opens nothing.
* **class 2** — a COMMENT line ending in `\` was folded into the next line, ending a heredoc body a
  line early and stripping the real read after the terminator. Bash does not splice a comment.

Two over-corrections the #1083 review warned against must stay closed, and are pinned here: not
splitting a newline that sits INSIDE a quote (a reader keeps its operand), and never dropping the
`$( … )` splice on the unparseable path.
"""

from __future__ import annotations

import unittest

from charter import hooks

V = ".charter/vaults/x.json"
R = f"cat {V}"
PLANE = "/x"


def denies(cmd: str) -> bool:
    return hooks._leak_reason(cmd, PLANE) is not None


class TheFallbackKeepsLineStructure(unittest.TestCase):
    """#1082: a broken quote no longer folds the next line's read into the segment before it."""

    def test_a_read_after_a_cd_on_its_own_line_is_denied(self):
        # `cd` then `cat x.json` then a stray `echo "`: bash runs the read, main allowed it.
        self.assertTrue(denies(f"cd .charter/vaults\ncat x.json\necho \""))

    def test_a_backslash_newline_splice_before_a_broken_line_is_denied(self):
        # the `\`\n` splices, so `cat $(echo . )charter/vaults/x.json` is one read.
        self.assertTrue(denies(f"cat $(echo . )\\\ncharter/vaults/x.json\necho \""))

    def test_a_well_formed_line_beside_a_broken_one_still_lexes(self):
        # three commands, only the last unbalanced; the middle read is still seen.
        self.assertTrue(denies(f"echo start\n{R}\necho \""))


class AQuoteThatSpansLinesIsNotAHeredoc(unittest.TestCase):
    """#1086 class 1: a `<<` inside a multi-line quote opens no heredoc, so the read after it
    is a command, not stripped body."""

    def test_double_quote_spanning_lines(self):
        self.assertTrue(denies(f"echo \"\ncat <<'EOF'\n\"; {R}\nEOF"))

    def test_single_quote_spanning_lines(self):
        self.assertTrue(denies(f"echo '\ncat <<\"EOF\"\n'; {R}\nEOF"))

    def test_a_real_multiline_quoted_heredoc_still_reads_as_data(self):
        """The fix must not deny a genuine `$(cat <<'EOF' … )` message: the substitution is not
        quoted, so its heredoc is real and its quoted body is data."""
        self.assertFalse(denies("git commit -m \"$(cat <<'EOF'\ndescribes .charter/vaults\nEOF\n)\""))


class ACommentIsNotSpliced(unittest.TestCase):
    """#1086 class 2: a comment line ending in `\\` ends the line; bash does not continue it."""

    def test_a_comment_after_a_heredoc_opener_ends_the_line(self):
        self.assertTrue(denies(f"cat <<'EOF' # note\\\nEOF\n{R}\nEOF"))

    def test_comment_index_finds_a_word_start_hash_only(self):
        """A `#` is a comment at a word boundary, not mid-word or inside quotes."""
        self.assertEqual(0, hooks._comment_index("# c"))
        self.assertEqual(6, hooks._comment_index("cat x # note"))
        self.assertEqual(-1, hooks._comment_index("cat a#b"))          # mid-word
        self.assertEqual(-1, hooks._comment_index("grep '#' f"))       # quoted

    def test_a_genuine_continuation_still_splices_on_the_unparseable_path(self):
        """A real (non-comment) backslash-newline is still spliced in the fallback, so a read
        it assembles across the break is refused even when a later line breaks lexing."""
        self.assertTrue(denies(f"cat \\\n{V}\necho \""))


class TheReviewWarningsStayClosed(unittest.TestCase):
    """The two over-corrections the #1083 review measured as bypasses must not reopen."""

    def test_a_newline_inside_a_quote_does_not_strand_a_readers_operand(self):
        # `grep -e "a\nb" x.json` after a cd: the quoted newline is not a boundary, so
        # x.json stays grep's operand and the read is refused (naive newline-splitting allowed it).
        self.assertTrue(denies("cd .charter/vaults; grep -e \"a\nb\" x.json\necho '"))

    def test_the_substitution_splice_is_kept_on_the_unparseable_path(self):
        # `cat $(echo . )charter/vaults/x.json` then a broken line: the splice still joins
        # `.charter` back together and refuses (not splicing here allowed it).
        self.assertTrue(denies(f"cat $(echo . )charter/vaults/x.json\necho \""))


class WhatBashRunsStaysRefusedAcrossLines(unittest.TestCase):
    """Positive controls: the same reads in their parseable form are already refused, so these
    tests measure the multi-line reading, not the vault predicate."""

    def test_the_parseable_cd_read_is_refused(self):
        self.assertTrue(denies("cd .charter/vaults && cat x.json"))

    def test_the_plain_read_is_refused(self):
        self.assertTrue(denies(R))


if __name__ == "__main__":
    unittest.main()
