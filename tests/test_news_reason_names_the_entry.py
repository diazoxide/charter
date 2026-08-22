"""A refused `check:` blames whatever is actually at fault — the entry, or this machine.

`news.py` keeps five reasons for having no answer, and the split between two of them is
the whole of #321. Its own comment states it:

    "Did not run here" points the reader at their own machine, which is right for a check
    this CLI could not resolve and wrong for an entry whose `check:` can never run
    anywhere — folding those together hides the second behind the first.

A `check:` containing shell syntax is the second kind. It cannot run on any machine, in
any version, ever; the entry is broken and somebody has to go and fix it. It was reported
as the first kind, because `_tokens` refuses shell syntax and an unregistered first token
in one breath and both arrive at `_dispatch` as a bare ``None`` — so the maintainer who
wrote the bad entry was told to go and look at their own laptop.

The pair that must stay apart is the pair this file tests, and it is not shell-syntax vs
unlisted — both of those are the entry's defect and say so. It is **entry-side** vs
**machine-side**: a command this CLI does not have is genuinely "did not run here" (an
entry written against a charter this one is not), and folding *that* into the entry-side
reason would be the same bug pointing the other way. Both directions are asserted here.

Nothing is dispatched by any test below: every `check:` used is refused before a command
function is reached, which is asserted rather than assumed.
"""

from __future__ import annotations

import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from charter import cli, news

#: A `check:` that can never run anywhere: `&` is shell syntax, so `_tokens` refuses it.
#: Its first token is a real, listed subcommand on purpose — that is what makes the
#: refusal attributable to the shell syntax and to nothing else.
SHELLISH_CHECK = "doctor && echo adopted"

#: A `check:` naming a command that exists and is registered, but that a probe may not
#: run (`_PROBEABLE`). The entry-side reason charter already gets right.
UNLISTED_CHECK = "secret exec devops -- true"

#: A `check:` this CLI cannot resolve at all. Machine-side: an entry written against a
#: charter that has the command, read by one that does not.
UNRESOLVED_CHECK = "frobnicate --hard"


class NewsDir(unittest.TestCase):
    """Entries read from a throwaway directory, so no test depends on what shipped."""

    def setUp(self) -> None:
        self.dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.dir, ignore_errors=True)
        patch = mock.patch.object(news, "_PACKAGED", self.dir)
        patch.start()
        self.addCleanup(patch.stop)

    def write(self, check: str, *, slug: str = "x") -> news.Entry:
        (self.dir / f"0.44.0-{slug}.md").write_text(
            f"---\nversion: 0.44.0\nheadline: h\ncheck: {check}\nadopt: version\n---\nbody\n")
        entries = [e for e in news.released() if e.slug == slug]
        # Really released, or `probe` is being handed something no caller would reach and
        # every assertion below is vacuous.
        self.assertEqual(len(entries), 1, "the planted entry is not released")
        self.assertEqual(entries[0].check, check, "the frontmatter did not survive parsing")
        return entries[0]

    def why(self, check: str) -> str:
        status, why = news.probe(self.write(check))
        self.assertEqual(status, news.UNKNOWN, f"`{check}` was not refused at all")
        return why


class ThePreconditionsTheseTestsRestOn(NewsDir):
    """Assert the gate under test is the gate that fires.

    Every assertion in this file is about *which* refusal happened. If the shellish check
    were refused for being an unregistered command, or the unresolved one for containing
    shell syntax, the tests below would still pass and would prove nothing.
    """

    def test_the_shellish_check_is_refused_by_the_shell_gate_and_not_the_other_one(self):
        parser = cli.build_parser()
        self.assertTrue(set(SHELLISH_CHECK) & news._SHELLISH,
                        "the check under test contains no shell syntax")
        self.assertIn(SHELLISH_CHECK.split()[0], cli._subcommand_names(parser),
                      "its first token is not a subcommand, so `_tokens` would refuse it "
                      "for the machine-side reason and this file would test nothing")
        self.assertIsNone(news._tokens(SHELLISH_CHECK, parser),
                          "`_tokens` accepted it, so no refusal is under test")

    def test_the_shell_gate_is_what_stands_between_this_check_and_a_dispatch(self):
        """Without the shell refusal this entry would REACH `doctor`: `_command_path`
        stops at `&&` and reports `("doctor",)`, which is listed. So the gate is load
        bearing, and the reason it records is the only thing #321 changes."""
        parser = cli.build_parser()
        tokens = SHELLISH_CHECK.split()
        self.assertIn(news._command_path(tokens, parser), news._PROBEABLE)

    def test_the_unresolved_check_is_refused_for_naming_no_command_here(self):
        parser = cli.build_parser()
        self.assertFalse(set(UNRESOLVED_CHECK) & news._SHELLISH,
                         "it contains shell syntax, so it is the other case")
        self.assertNotIn(UNRESOLVED_CHECK.split()[0], cli._subcommand_names(parser))

    def test_the_unlisted_check_names_a_real_command_a_probe_may_not_run(self):
        parser = cli.build_parser()
        self.assertFalse(set(UNLISTED_CHECK) & news._SHELLISH)
        self.assertIsNotNone(news._tokens(UNLISTED_CHECK, parser),
                             "it never reaches the `_PROBEABLE` gate")
        self.assertFalse(news.probeable(UNLISTED_CHECK, parser))


class AShellSyntaxCheckBlamesTheEntry(NewsDir):
    def test_it_does_not_point_the_reader_at_their_own_machine(self):
        """The sentence #321 is about. Nothing on the reader's machine is wrong, and
        every machine that installs this entry gets the same refusal."""
        self.assertNotIn("did not run here", self.why(SHELLISH_CHECK))

    def test_it_says_the_entry_is_not_something_a_check_may_name(self):
        """Same defect class as an unlisted command — the entry names something a probe
        cannot run — so it reuses that reason rather than inventing a sixth."""
        self.assertIn("is not a command a `check:` may name", self.why(SHELLISH_CHECK))

    def test_it_names_the_entrys_own_check(self):
        """A sweep prints a list. A reason that does not quote the broken `check:` is one
        nobody can act on."""
        self.assertIn(SHELLISH_CHECK, self.why(SHELLISH_CHECK))

    def test_it_reads_the_same_as_an_unlisted_command(self):
        """Both are the entry's defect, so they are the same diagnosis. This is the
        assertion that would break if a sixth string were added for shell syntax."""
        self.assertEqual(self.why(SHELLISH_CHECK).replace(SHELLISH_CHECK, ""),
                         self.why(UNLISTED_CHECK).replace(UNLISTED_CHECK, ""))


class ACheckThisCliCannotResolveStillBlamesTheMachine(NewsDir):
    """The other direction, and the reason the two gates inside `_tokens` cannot simply
    both be folded onto the entry. A `check:` naming a command this charter does not have
    is an entry written against a charter this one is not — which IS a fact about here."""

    def test_it_says_the_probe_did_not_run_here(self):
        self.assertIn("did not run here", self.why(UNRESOLVED_CHECK))

    def test_it_does_not_blame_the_entry(self):
        self.assertNotIn("is not a command a `check:` may name", self.why(UNRESOLVED_CHECK))

    def test_a_listed_command_with_a_flag_this_cli_lacks_also_did_not_run_here(self):
        """The same machine-side shape one gate further in: the command exists and is
        listed, and only the flag is gone. `doctor` is real and probeable, so this is
        refused by the parser rather than by either gate in `_tokens`."""
        check = "doctor --a-flag-no-charter-ever-had"
        self.assertTrue(news.probeable(check, cli.build_parser()),
                        "it was refused before the parser, so this tests the wrong gate")
        self.assertIn("did not run here", self.why(check))
