"""A `check:` may name only a command a probe is allowed to run (#317).

`news.py` claimed that dispatching in-process was enough to keep an entry from becoming an
arbitrary-command primitive. It was not: `_tokens` refused shell syntax and an unregistered
first token, and `secret exec` takes the rest of the line as a pass-through argv — so the
tail of a `check:` reached `subprocess.run`, with a vault's credential in the child's
environment. `charter doctor` runs every released entry's `check:` unprompted on a
SessionStart hook, so the reach was every machine that upgraded, not only one that ran a
command.

The restraint that actually holds is a property of the command, not of the string:
**a probe reads, and its argv is charter's rather than the entry's.** Neither half is
derivable from the parser — argparse knows nothing about whether a command writes — so the
rule is a list of command paths a human has confirmed, and the parser is used to pin the
half it *can* answer (see `TheListIsPinnedToTheParser`).

The spawn is observed rather than inferred. A returned status is what the bug would have
lied about, so the assertion is on a file that only a real child process could have
created.
"""

from __future__ import annotations

import shutil
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from charter import cli, commands_persona, commands_secrets, commands_update, news


class _StubProvider:
    """A vault that resolves, so nothing but the guard under test can stop the spawn.

    Without this `_provider` raises and `cmd_secret_exec` returns 1 before reaching
    `subprocess.run` — which is a refusal this test did not ask for and would look exactly
    like the guard working.
    """

    def get(self, key: str) -> str:
        return "not-a-real-secret"


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
        # The entry has to be REALLY released, or `probe` is being handed something no
        # caller would reach and every assertion below is vacuous.
        self.assertEqual(len(entries), 1, "the planted entry is not released")
        return entries[0]


class AProbeSpawnsNothing(NewsDir):
    """The #317 reproduction: a `check:` whose tail is an argv for another program."""

    def setUp(self) -> None:
        super().setUp()
        self.sentinel = self.dir / "a-child-ran"
        self.script = self.dir / "child.py"
        self.script.write_text(f"open({str(self.sentinel)!r}, 'w').close()\n")

        p = mock.patch.object(commands_secrets, "_provider", lambda _name: _StubProvider())
        p.start()
        self.addCleanup(p.stop)
        # `persona secret exec` is the same function behind a proxy that resolves the
        # persona's vault first. Resolving it here for the same reason as the provider.
        p = mock.patch.object(commands_persona, "_resolve_vault", lambda _args: "probe-vault")
        p.start()
        self.addCleanup(p.stop)

    #: `secret exec` names the vault; `persona secret exec` takes it from the persona, so
    #: its whole tail is the command. Spelled out per test rather than shared, because a
    #: vault token left in front of the persona form is read as the program to run — which
    #: fails as "command not found" and looks exactly like a guard that works.
    def _check(self, *lead: str) -> str:
        argv = " ".join([*lead, sys.executable, str(self.script)])
        # Two ways this test could pass while proving nothing, both checked rather than
        # assumed: a path containing a character `_tokens` reads as shell syntax, and a
        # command line the live parser does not accept. Either would refuse the entry for
        # a reason that has nothing to do with #317.
        self.assertFalse(set(argv) & news._SHELLISH, f"{argv} was refused as shell syntax")
        self.assertTrue(news.resolves(cli.build_parser(), argv),
                        f"`charter {argv}` does not parse, so nothing would have run")
        return argv

    def test_a_check_naming_secret_exec_starts_no_process(self):
        entry = self.write(self._check("secret", "exec", "probe-vault"))
        news.probe(entry)
        self.assertFalse(self.sentinel.exists(),
                         "a news probe started a process of the entry's choosing")

    def test_a_check_naming_persona_secret_exec_starts_no_process(self):
        entry = self.write(self._check("persona", "secret", "exec"))
        news.probe(entry)
        self.assertFalse(self.sentinel.exists(),
                         "a news probe started a process of the entry's choosing")

    def test_the_entry_is_unchecked_rather_than_adopted(self):
        """Refusing is half of it. `cmd_secret_exec` returns the child's exit code, and a
        refusal that let the outer call read an exit code would report the entry ADOPTED —
        bounded is not correct (ADR 0013)."""
        entry = self.write(self._check("secret", "exec", "probe-vault"))
        self.assertEqual(news.probe(entry)[0], news.UNKNOWN)

    def test_the_reason_names_the_entrys_own_check(self):
        """During a sweep the reader is looking at a list. A reason that does not name the
        command is a reason nobody can act on."""
        entry = self.write(self._check("secret", "exec", "probe-vault"))
        _, why = news.probe(entry)
        self.assertIn("secret exec", why)
        self.assertIn(str(self.script), why)


class OnlyAListedCommandIsProbeable(unittest.TestCase):
    """What the list says, said directly — `probeable` is the whole rule."""

    def test_a_pass_through_command_is_refused(self):
        self.assertFalse(news.probeable("secret exec v ls"))
        self.assertFalse(news.probeable("persona secret exec v ls"))

    def test_the_shipped_probes_are_allowed(self):
        self.assertTrue(news.probeable("persona lint"))
        self.assertTrue(news.probeable("persona lint --only delegate-when"))

    def test_frame_probe_is_allowed_but_frame_dash_dash_probe_is_not(self):
        """`charter frame-probe` (this task) is a TOP-LEVEL command specifically because
        `frame` itself can never be added here: every parser `cli._wire` builds — `frame`
        included — carries a pass-through `rest` (#317's shape), `--probe` on it or not.
        Both halves are pinned together so a future edit that lists `("frame",)` instead
        fails HERE, not only in `TheListIsPinnedToTheParser` below (which would report
        THAT failure as "frame takes a pass-through argv", true but not why this specific
        command exists)."""
        self.assertTrue(news.probeable("frame-probe"))
        self.assertFalse(news.probeable("frame --probe"))

    def test_a_command_that_probes_news_is_still_allowed_here(self):
        """`news --pending` re-enters, and #311's guard is what answers that — a separate
        question, answered later and with its own reason. Folding the two together would
        report the wrong fact for whichever one this rule reached first."""
        self.assertTrue(news.probeable("news --pending"))
        self.assertTrue(news.probeable("doctor"))

    def test_a_subcommand_below_a_listed_one_needs_its_own_listing(self):
        """`news` is probeable; `news stamp` renames files. Allowing a prefix would allow
        every command beneath it, which is how a read-only list acquires a writer."""
        self.assertFalse(news.probeable("news stamp 9.9.9"))

    def test_a_flag_cannot_hide_the_subcommand_that_follows_it(self):
        """argparse reads past `--pending` and finds `stamp`. A walk that stopped at the
        first flag would score this as plain `news` and let the rename through."""
        self.assertEqual(cli.build_parser().parse_args(
            ["news", "--pending", "stamp", "9.9.9"]).func.__name__, "cmd_news_stamp")
        self.assertFalse(news.probeable("news --pending stamp 9.9.9"))

    def test_an_unregistered_command_is_refused(self):
        self.assertFalse(news.probeable("not-a-charter-command"))
        self.assertFalse(news.probeable(""))


class TheListIsPinnedToTheParser(unittest.TestCase):
    """The half the parser CAN answer, asserted over the whole list.

    A list alone would drift: a listed command that later grows a pass-through positional
    reopens #317 with nothing failing. Deriving the refusal at runtime instead would only
    move the same check somewhere more expensive and less complete — entries, list and
    parser ship in one wheel, so a test over the three is a proof rather than a sample.
    """

    def test_every_listed_path_is_a_command_the_parser_has(self):
        """A rename that left the list behind would turn a probe into permanent `unknown`
        in the field, and say nothing about why."""
        for path in sorted(news._PROBEABLE):
            with self.subTest(path=" ".join(path)):
                self.assertIsNotNone(news._parser_at(path),
                                     f"`charter {' '.join(path)}` is listed but not a command")

    def test_no_listed_command_takes_a_pass_through_argv(self):
        """#317's shape, refused structurally so it cannot come back under a new name."""
        for path in sorted(news._PROBEABLE):
            with self.subTest(path=" ".join(path)):
                self.assertEqual(
                    news._pass_through(news._parser_at(path)), [],
                    f"`charter {' '.join(path)}` takes an argv from the entry")

    def test_the_commands_that_carry_a_pass_through_today_are_not_listed(self):
        """Read the parser, not a memory of it: whatever takes a pass-through argv now is
        exactly what may not be probeable, however the list is later edited."""
        offenders = [p for p in news._all_command_paths()
                     if news._pass_through(news._parser_at(p))]
        self.assertTrue(offenders, "no pass-through command found — the walk is broken")
        for path in offenders:
            with self.subTest(path=" ".join(path)):
                self.assertNotIn(path, news._PROBEABLE)


class TheShippedEntriesStillProbe(unittest.TestCase):
    """A rule that quietly refused everything would pass every test above.

    Held against what actually ships, on the real parser, running the real commands.
    """

    def setUp(self) -> None:
        self.entries = [e for e in news.released() if e.check]
        self.assertTrue(self.entries, "no shipped entry carries a `check:` — nothing pinned")

    def test_every_shipped_check_is_probeable(self):
        for e in self.entries:
            with self.subTest(entry=e.slug):
                self.assertTrue(news.probeable(e.check),
                                f"{e.path.name}: `charter {e.check}` may no longer be probed")

    def test_every_shipped_check_still_reaches_an_answer(self):
        """Not `unknown`: the probe has to RUN. Adopted or pending are both fine — which
        one depends on the plane the suite happens to be running on."""
        for e in self.entries:
            with self.subTest(entry=e.slug):
                status, why = news.probe(e)
                self.assertIn(status, (news.ADOPTED, news.PENDING),
                              f"{e.path.name}: `charter {e.check}` stopped probing — {why}")


class TheMutationGuardIsBehindThisOne(NewsDir):
    """#318's `refuse_mutation` and this rule answer different questions; both stay.

    `update` is not probeable, so a `check:` naming it is refused here and never reaches
    the installer. That is the outer layer, asserted below. The inner one — `cmd_update`
    declining on its own account because a probe is in flight — is exercised by
    `test_news_cross_process.py`, which opens this list deliberately to reach it.
    """

    def test_a_check_naming_update_is_refused_before_the_installer(self):
        entry = self.write("update --to 9.9.9")
        with mock.patch.object(commands_update, "cmd_update") as run:
            status, _ = news.probe(entry)
        run.assert_not_called()
        self.assertEqual(status, news.UNKNOWN)


if __name__ == "__main__":
    unittest.main()
