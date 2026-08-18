"""A report body has a route that never touches the shell.

The body was a single positional argument, so anything substantial had to be quoted inline
on a command line — and a report worth filing is exactly the text that does not survive
there. Observed while filing #239:

* every backticked term was command-substituted **away**. "when `open` returns" was stored
  as "when  returns" — a word silently deleted from a sentence still grammatical enough to
  skim past;
* `$(…)` inside the intended code sample executed, and `$P open https://app.example/` ran
  macOS's `/usr/bin/open`.

The draft saved in that state. What is published is irreversible, on a public tracker,
under the reporter's own identity, and the material that gets mangled is precisely the code
sample that made the report worth reading.

`secret set` already solves this shape with `--stdin`/`--from-file`; there the reason is
disclosure, here it is corruption, and the mechanism and the fix are identical. So these
are the same two flags, spelled the same way.
"""

from __future__ import annotations

import io
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace

from charter import commands_report
from tests._isolation import ReportIso

#: Every construct that dies on argv, in one string.
HOSTILE = ("A body with `backticks`, a $(echo SUBSTITUTION), a $VAR and a fence:\n"
           "\n```bash\n$P open https://app.example/\n```\n")


class BodyCase(ReportIso):
    def args(self, **kw):
        base = {"text": None, "from_file": None, "stdin": False}
        return SimpleNamespace(**{**base, **kw})

    def with_stdin(self, text: str):
        real = sys.stdin
        sys.stdin = io.StringIO(text)
        self.addCleanup(setattr, sys, "stdin", real)


class TestTheBodyCanAvoidTheShell(BodyCase):
    def test_from_file_is_verbatim(self):
        p = Path(tempfile.mkdtemp()) / "body.md"
        p.write_text(HOSTILE)
        self.assertEqual(commands_report._body(self.args(from_file=str(p))), HOSTILE)

    def test_stdin_flag_is_verbatim(self):
        self.with_stdin(HOSTILE)
        self.assertEqual(commands_report._body(self.args(stdin=True)), HOSTILE)

    def test_a_bare_dash_means_stdin(self):
        """The conventional spelling, and free to honour."""
        self.with_stdin(HOSTILE)
        self.assertEqual(commands_report._body(self.args(text="-")), HOSTILE)

    def test_a_pipe_with_no_flag_is_read(self):
        """The ordinary shape, and the one `secret set` records as having been broken once
        by demanding an explicit flag for every pipeline."""
        self.with_stdin(HOSTILE)
        self.assertEqual(commands_report._body(self.args()), HOSTILE)

    def test_an_inline_body_still_works(self):
        """Short reports keep the positional — the point is that a long one has a route,
        not that the easy one is taken away."""
        self.assertEqual(commands_report._body(self.args(text="it broke")), "it broke")

    def test_inline_text_wins_over_a_non_tty_stdin(self):
        """An agent's Bash tool presents a non-tty stdin with nothing behind it. Reading it
        in preference to the argument the caller actually passed would substitute silence
        for their report."""
        self.with_stdin("")
        self.assertEqual(commands_report._body(self.args(text="it broke")), "it broke")

    def test_an_unreadable_file_is_reported_not_swallowed(self):
        """None, not "" — an empty body and a missing file are different failures, and
        drafting an empty report because a path was wrong is the silent one."""
        self.assertIsNone(commands_report._body(self.args(from_file="/nope/missing.md")))


class TestTheEmptyCaseNamesTheRoutes(BodyCase):
    def test_it_names_the_file_route(self):
        import contextlib
        buf = io.StringIO()
        with contextlib.redirect_stderr(buf), contextlib.redirect_stdout(io.StringIO()):
            rc = commands_report._draft("bug", "")
        self.assertEqual(rc, 1)
        said = buf.getvalue()
        self.assertIn("--from-file", said)
        self.assertIn("does not survive the shell", said)


if __name__ == "__main__":
    unittest.main()
