"""`persona lint --only <key>` — one finding, one exit code.

A news entry's `check:` has to answer ONE question: has this plane adopted THIS feature?
Bare `persona lint` cannot: its exit code covers dangling `uses:`, unloadable personas and
stale agents all at once, so a plane failing it for an unrelated reason would be told to
adopt something it already has. That is the "sloppy probe" the design refuses — an entry
with no narrow probe ships with none rather than with a misleading one.

So the narrowing is a real flag with real tests, not a convention in a markdown file.
"""

from __future__ import annotations

import io
import unittest
from contextlib import redirect_stderr, redirect_stdout
from types import SimpleNamespace

from charter import commands_persona
from tests._isolation import PersonaIso


class LintOnly(PersonaIso):
    def lint(self, only=None) -> tuple[int, str]:
        err = io.StringIO()
        args = SimpleNamespace(name=None, only=only)
        with redirect_stderr(err), redirect_stdout(io.StringIO()):
            code = commands_persona.cmd_persona_lint(args)
        return code, err.getvalue()

    def test_a_persona_missing_the_key_makes_the_probe_fail(self):
        self.make_persona("scribe", role="Scribe", vault="none")
        code, out = self.lint(only="delegate-when")
        self.assertEqual(code, 1)
        self.assertIn("scribe", out)

    def test_a_persona_carrying_the_key_makes_the_probe_pass(self):
        self.make_persona("scribe", role="Scribe", vault="none",
                          **{"delegate-when": "writing things down"})
        code, _ = self.lint(only="delegate-when")
        self.assertEqual(code, 0)

    def test_only_ignores_findings_that_are_not_its_key(self):
        """The whole point. `uses: nobody` is a real lint error, and it must not make the
        delegate-when probe claim this plane has work to do about delegate-when."""
        self.make_persona("scribe", role="Scribe", vault="none",
                          **{"delegate-when": "writing things down", "uses": "nobody"})
        code, _ = self.lint(only="delegate-when")
        self.assertEqual(code, 0)

    def test_without_only_the_command_is_unchanged(self):
        self.make_persona("scribe", role="Scribe", vault="none",
                          **{"delegate-when": "writing things down", "uses": "nobody"})
        code, _ = self.lint()
        self.assertEqual(code, 1)


if __name__ == "__main__":
    unittest.main()
