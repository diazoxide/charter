"""News: what a version brought, and whether THIS plane has taken it up.

Two properties carry the design. First, an entry is *checked*, never assumed — a `check:`
that cannot run is `unknown`, which is neither "you have it" nor "you need it" (ADR 0013,
and `doctor._NOT_CHECKED_HINT`: absence of information is not evidence of health). Second,
`check:`/`adopt:` are charter subcommands dispatched IN-PROCESS: there is no shell and no
argv to escape into, so a news entry cannot become an arbitrary-command primitive — the
restraint `docsrc._TOPIC` keeps for `docs show`, applied to something that executes.
"""

from __future__ import annotations

import io
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from types import SimpleNamespace
from pathlib import Path
from unittest import mock

from charter import cli, news

ENTRY = """\
---
version: 0.44.0
headline: A persona says when it should hand work away
check: persona lint
adopt: persona sync-agents
---

Every persona now declares `delegate-when:`.
"""


class NewsDir(unittest.TestCase):
    """Entries read from a throwaway directory, so a test never depends on what shipped."""

    def setUp(self):
        self.dir = Path(tempfile.mkdtemp())
        patch = mock.patch.object(news, "_PACKAGED", self.dir)
        patch.start()
        self.addCleanup(patch.stop)

    def write(self, name: str, text: str) -> None:
        (self.dir / name).write_text(text)


class Parsing(NewsDir):
    def test_an_entry_carries_its_version_headline_and_actions(self):
        self.write("0.44.0-delegate-when.md", ENTRY)
        e, = news.all()
        self.assertEqual(e.version, "0.44.0")
        self.assertEqual(e.slug, "delegate-when")
        self.assertEqual(e.headline, "A persona says when it should hand work away")
        self.assertEqual(e.check, "persona lint")
        self.assertEqual(e.adopt, "persona sync-agents")
        self.assertIn("delegate-when", e.body)

    def test_an_entry_with_no_probe_is_informational_not_broken(self):
        self.write("0.44.1-patch.md", "---\nversion: 0.44.1\nheadline: Fixes a crash\n---\n")
        e, = news.all()
        self.assertEqual(e.check, "")
        self.assertEqual(news.probe(e)[0], "informational")


class Unreleased(NewsDir):
    def test_a_staged_entry_never_surfaces_to_a_user(self):
        """A feature PR writes `unreleased-<slug>.md`; the bump stamps it. Until then it
        names no version that was ever true, so it must not appear in any user-facing
        view — the whole reason staging exists rather than guessing a version number."""
        self.write("unreleased-thing.md",
                   "---\nversion: unreleased\nheadline: Not out yet\n---\n")
        self.write("0.44.0-out.md", ENTRY)
        self.assertEqual([e.slug for e in news.released()], ["out"])
        self.assertEqual([e.slug for e in news.between("0.43.0", "0.99.0")], ["out"])


class Ordering(NewsDir):
    def test_versions_sort_numerically_not_lexically(self):
        """0.10.0 is newer than 0.2.0. `update._parse` already knows this; a second
        comparator here would be a second answer to one question."""
        for v in ("0.2.0", "0.10.0"):
            self.write(f"{v}-x.md", f"---\nversion: {v}\nheadline: v{v}\n---\n")
        self.assertEqual([e.version for e in news.released()], ["0.2.0", "0.10.0"])

    def test_between_is_exclusive_of_where_you_were_and_inclusive_of_where_you_land(self):
        for v in ("0.43.0", "0.44.0", "0.45.0"):
            self.write(f"{v}-x.md", f"---\nversion: {v}\nheadline: v{v}\n---\n")
        self.assertEqual([e.version for e in news.between("0.43.0", "0.45.0")],
                         ["0.44.0", "0.45.0"])


class Probing(NewsDir):
    def _entry(self, check: str):
        self.write("0.44.0-x.md",
                   f"---\nversion: 0.44.0\nheadline: h\ncheck: {check}\n---\n")
        return news.all()[0]

    def test_exit_zero_means_this_plane_already_has_it(self):
        with mock.patch.object(news, "_dispatch", return_value=0):
            self.assertEqual(news.probe(self._entry("persona lint"))[0], "adopted")

    def test_a_non_zero_exit_means_pending(self):
        with mock.patch.object(news, "_dispatch", return_value=1):
            self.assertEqual(news.probe(self._entry("persona lint"))[0], "pending")

    def test_a_probe_that_cannot_run_is_unknown_never_pending(self):
        """A `check:` naming a subcommand this CLI does not have exits non-zero for a
        reason that has nothing to do with adoption. Reporting that as "you need this"
        invents work; reporting it as "you have it" hides the entry forever."""
        status, why = news.probe(self._entry("persona no-such-subcommand"))
        self.assertEqual(status, "unknown")
        self.assertTrue(why.strip())

    def test_a_probe_is_never_handed_to_a_shell(self):
        status, _ = news.probe(self._entry("persona lint; rm -rf /"))
        self.assertEqual(status, "unknown")

    def test_a_probe_cannot_name_a_non_charter_command(self):
        status, _ = news.probe(self._entry("/bin/sh -c whoami"))
        self.assertEqual(status, "unknown")


class TheCommand(NewsDir):
    """`charter news` output must be self-sufficient.

    The skill is a Claude Code artifact; opencode gets none. So an agent reading only this
    output has to be able to finish the job, which is also why there is no `--json`: a
    machine surface would immediately become the thing that is parsed, and the prose the
    thing nobody checks.
    """

    def _run(self, **kw) -> str:
        from charter import commands

        out = io.StringIO()
        args = SimpleNamespace(pending=False, since=None, until=None, for_version=None)
        for k, v in kw.items():
            setattr(args, k, v)
        with redirect_stdout(out), redirect_stderr(io.StringIO()):
            commands.cmd_news(args)
        return out.getvalue()

    def test_a_pending_entry_is_one_line_naming_how_to_adopt_it(self):
        self.write("0.44.0-delegate-when.md", ENTRY)
        with mock.patch.object(news, "_dispatch", return_value=1):
            out = self._run(pending=True)
        self.assertIn("delegate-when", out)
        self.assertIn("A persona says when it should hand work away", out)
        self.assertIn("persona sync-agents", out)

    def test_an_adopted_entry_is_not_offered_again(self):
        self.write("0.44.0-delegate-when.md", ENTRY)
        with mock.patch.object(news, "_dispatch", return_value=0):
            out = self._run(pending=True)
        self.assertNotIn("delegate-when", out)

    def test_an_entry_with_no_adopt_command_says_it_is_manual(self):
        self.write("0.44.0-x.md",
                   "---\nversion: 0.44.0\nheadline: Do it by hand\ncheck: persona lint\n---\nbody\n")
        with mock.patch.object(news, "_dispatch", return_value=1):
            out = self._run(pending=True)
        self.assertIn("manual", out)

    def test_an_informational_entry_is_not_dressed_up_as_work(self):
        """An entry with no probe and no adopt command has nothing for the reader to do —
        a patch note, usually. Printing "adopt: manual (see the entry)" under it invents a
        chore out of a line that exists to say there is none."""
        self.write("0.44.1-patch.md",
                   "---\nversion: 0.44.1\nheadline: Fixes a crash\n---\nnothing to do\n")
        out = self._run(since="0.44.0", until="0.44.1")
        self.assertIn("Fixes a crash", out)
        self.assertNotIn("adopt:", out)

    def test_a_pending_entry_in_the_range_still_says_how_to_take_it_up(self):
        self.write("0.44.0-delegate-when.md", ENTRY)
        with mock.patch.object(news, "_dispatch", return_value=1):
            out = self._run(since="0.43.0", until="0.44.0")
        self.assertIn("persona sync-agents", out)

    def test_for_a_version_prints_the_release_body(self):
        self.write("0.44.0-delegate-when.md", ENTRY)
        out = self._run(for_version="0.44.0")
        self.assertIn("A persona says when it should hand work away", out)
        self.assertIn("delegate-when", out)


class OutsideAControlPlane(NewsDir):
    """A probe asks "has THIS PLANE adopted it?" — a question with no subject here.

    Running the probes anyway would answer a question nobody asked, slowly, and report
    whatever a plane-less charter happens to exit with as though it meant something.
    """

    def _run(self, **kw) -> tuple[str, str]:
        from charter import commands, config

        out, err = io.StringIO(), io.StringIO()
        args = SimpleNamespace(pending=False, since=None, until=None, for_version=None)
        for k, v in kw.items():
            setattr(args, k, v)
        with mock.patch.object(config, "HAS_CONTROL_PLANE", False), \
             redirect_stdout(out), redirect_stderr(err):
            commands.cmd_news(args)
        return out.getvalue(), err.getvalue()

    def test_the_range_still_prints(self):
        self.write("0.44.0-delegate-when.md", ENTRY)
        out, _ = self._run(since="0.43.0", until="0.44.0")
        self.assertIn("A persona says when it should hand work away", out)

    def test_it_says_once_that_it_cannot_tell_what_you_have_adopted(self):
        self.write("0.44.0-delegate-when.md", ENTRY)
        _, err = self._run(since="0.43.0", until="0.44.0")
        self.assertIn("no control plane", err)

    def test_no_probe_is_run(self):
        self.write("0.44.0-delegate-when.md", ENTRY)
        with mock.patch.object(news, "_dispatch") as d:
            self._run(since="0.43.0", until="0.44.0")
        d.assert_not_called()

    def test_pending_says_what_it_cannot_answer_rather_than_answering_it(self):
        self.write("0.44.0-delegate-when.md", ENTRY)
        with mock.patch.object(news, "_dispatch") as d:
            out, err = self._run(pending=True)
        d.assert_not_called()
        self.assertIn("no control plane", err)


class ShippedEntriesAreValid(unittest.TestCase):
    def test_every_shipped_action_resolves_against_the_live_parser(self):
        """A `check:` referencing a removed flag degrades to permanent silence, which is
        the failure `_NOT_CHECKED_HINT` exists to name. Proving it PARSES (never running
        it) fails the suite of the PR that removes the flag, where the cost is lowest —
        the same insight as `test_docs_show.py`: the gap is invisible from a checkout."""
        parser = cli.build_parser()
        for e in news.all():
            for field in ("check", "adopt"):
                argv = getattr(e, field)
                if not argv:
                    continue
                with self.subTest(entry=e.slug, field=field):
                    self.assertTrue(news.resolves(parser, argv),
                                    f"{e.path.name}: `charter {argv}` does not parse")


if __name__ == "__main__":
    unittest.main()
