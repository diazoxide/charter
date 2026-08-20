"""`--local` promises a rule that stays on this machine. The plane must actually ignore it.

`guard --local` writes `.claude/settings.local.json` on the strength of one sentence — that
`charter init` gitignores it. It did not. charter's OWN repo does (a hand-maintained line
nobody generated), and checking there and generalising is how the claim got made.

So in a fresh plane the file was committable, and `charter guard allow --local 'gh pr merge
*'` — the exact command the feature exists for — would land in the next commit and become
the team-wide rule it was chosen to avoid. A promise that holds only in the repo where it
was tested is worse than no promise, because the flag reads as a guarantee.

Asserted on the SHIPPED artifacts (the baseline text and a real `init`), not on a constant,
because the bug was that the constant and the plane disagreed.
"""

from __future__ import annotations

import io
import json
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace

from charter import commands, config
from tests._isolation import PersonaIso

IGNORE_LINE = "/.claude/settings.local.json"


class IgnoreCase(PersonaIso):
    def init_plane(self):
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            commands.cmd_init(SimpleNamespace(forge="github", owner="x", shape=None,
                                              here=False, path=None))
        return (Path(config.ROOT) / ".gitignore").read_text()

    def allow_local(self, pattern):
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            commands.cmd_guard_allow(SimpleNamespace(pattern=pattern, local=True))


class TestAFreshPlaneIgnoresIt(IgnoreCase):
    def test_the_baseline_carries_the_rule(self):
        self.assertIn(IGNORE_LINE, commands._GITIGNORE_BASELINE)

    def test_a_real_init_writes_it(self):
        """The baseline is a constant; this asserts the file a plane actually gets."""
        self.assertIn(IGNORE_LINE, self.init_plane())

    def test_the_committed_sibling_is_NOT_ignored(self):
        """`.claude/settings.json` is charter's shared, committed file — ignoring it would
        break the status line and the guard wiring for every teammate."""
        body = self.init_plane()
        self.assertNotIn("/.claude/settings.json\n", body)


class TestTheRuleIsEnsuredWhereItMatters(IgnoreCase):
    """A plane created before this shipped still has the old `.gitignore`, and that is
    exactly where `--local` would betray its promise. The command guarantees it rather
    than assuming a fresh plane."""

    def test_writing_a_local_rule_ensures_the_ignore(self):
        p = Path(config.ROOT) / ".gitignore"
        p.write_text("# a plane from before this shipped\n/.charter/\n")
        self.allow_local("gh pr merge *")
        self.assertIn(IGNORE_LINE, p.read_text())

    def test_it_is_appended_not_rewritten(self):
        p = Path(config.ROOT) / ".gitignore"
        p.write_text("# a plane from before this shipped\n/.charter/\n/my/own/rule\n")
        self.allow_local("gh pr merge *")
        body = p.read_text()
        self.assertIn("/my/own/rule", body)
        self.assertIn("/.charter/", body)

    def test_it_is_not_added_twice(self):
        p = Path(config.ROOT) / ".gitignore"
        self.allow_local("gh pr merge *")
        self.allow_local("git status *")
        self.assertEqual(1, p.read_text().count(IGNORE_LINE))

    def test_a_shared_rule_does_not_touch_the_gitignore(self):
        """Only `--local` needs the guarantee; the committed file is meant to be committed."""
        p = Path(config.ROOT) / ".gitignore"
        p.write_text("/.charter/\n")
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            commands.cmd_guard_allow(SimpleNamespace(pattern="git status *", local=False))
        self.assertNotIn(IGNORE_LINE, p.read_text())


class TestTheFileIsStillWritten(IgnoreCase):
    def test_the_rule_lands_and_the_plane_ignores_it(self):
        self.allow_local("gh pr merge *")
        local = Path(config.ROOT) / ".claude" / "settings.local.json"
        self.assertIn("Bash(gh pr merge *)",
                      json.loads(local.read_text())["permissions"]["allow"])
        self.assertIn(IGNORE_LINE, (Path(config.ROOT) / ".gitignore").read_text())


if __name__ == "__main__":
    unittest.main()
