"""A release without notes does not publish.

The obligation is one sentence — every published version ships an entry — and it is
caught twice, at deliberately different times, because the two catches fail in different
places at different costs:

* here, against ``charter.__version__``, so a bump PR goes red **before any tag exists**;
* in ``release.yml``'s ``guard`` job, against the tag, so a tag that skipped the PR fails
  **before the irreversible PyPI upload**.

Between them sits ``charter news stamp <version>``: the bump's one mechanical step, which
moves every staged entry onto the version that is about to ship. It is a tested command
rather than a fifth thing for a release engineer to remember, for the same reason
``hooks.json``'s ``--plugin-version`` count is — "never work from a remembered count".
"""

from __future__ import annotations

import io
import re
import shutil
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import __version__, cli, commands, news

REPO_ROOT = Path(__file__).resolve().parent.parent

STAGED = """\
---
version: unreleased
headline: A persona says when it should hand work away
check: persona lint --only delegate-when
---

Body text with a `---` rule below it, and trailing structure that must survive.

1. one
2. two
"""


class TestEveryPublishedVersionHasNotes(unittest.TestCase):
    def test_an_entry_names_the_version_this_tree_would_publish(self):
        """The catch that fires in the bump PR.

        `TestVersionsMoveInLockstep` pins the four files that carry a version *number*;
        "a release has notes" is a different obligation and gets its own test rather than
        being bolted onto one that means something else.
        """
        versions = {e.version for e in news.all()}
        self.assertIn(
            __version__, versions,
            f"no news entry names {__version__}. Every published version needs one, "
            f"including a patch — write docs/news/{__version__}-<slug>.md. "
            f'"No entry" and "forgot the entry" are indistinguishable from CI.')


class TestNoTestPinsAStagedEntryByFilename(unittest.TestCase):
    """A staged entry's *filename* is a release-time casualty. Its slug is not.

    `news stamp` renames every `unreleased-<slug>.md` to `<version>-<slug>.md` as the
    first step of a bump, so a test that opens one by its staged path goes red **in the
    middle of the release** — on the one branch that has to be green before a tag, for a
    document whose text never changed. 0.52.0 lost four cases exactly that way, in
    `test_docs_claims_carry_their_residual.py`, which now resolves entries by slug.

    Only a reference to a file that **exists** is flagged. A fabricated tree inside a
    fixture may spell any path it likes (`test_plugin_freshness.py` writes an
    `unreleased-x.md` into a temporary directory); what breaks a release is the pair
    *real staged entry + a test that opens it by that name*, and the pair cannot be
    assembled without this failing on the pull request that assembles it.
    """

    _REF = re.compile(r"docs/news/(unreleased-[A-Za-z0-9._-]+\.md)")

    def test_no_test_opens_an_entry_by_its_staged_name(self):
        offenders = []
        for path in sorted((REPO_ROOT / "tests").glob("*.py")):
            if path.name == Path(__file__).name:
                continue          # the pattern above is a pattern, not a path
            for name in set(self._REF.findall(path.read_text())):
                if (REPO_ROOT / "docs" / "news" / name).is_file():
                    offenders.append(f"{path.name} → docs/news/{name}")
        self.assertEqual(
            offenders, [],
            "these tests name a staged news entry by a filename that `charter news "
            "stamp` will rename during the next bump: " + "; ".join(offenders) +
            ". Resolve the entry by its slug instead — everything after the first `-`, "
            "which survives the stamp — the way `entries()` in "
            "test_docs_claims_carry_their_residual.py does.")


class StampDir(unittest.TestCase):
    """A throwaway checkout, so a stamp test never renames a file that really ships."""

    def setUp(self):
        self.repo = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.repo, ignore_errors=True)
        (self.repo / "pyproject.toml").write_text("[project]\nname = 'x'\n")
        self.dir = self.repo / "docs" / "news"
        self.dir.mkdir(parents=True)
        # The packaged copy is pointed at nothing: these tests are about the repo's own
        # directory, and a real `charter/_news` would otherwise leak shipped entries in.
        for attr, value in (("_CHECKOUT", self.dir), ("_PACKAGED", self.repo / "absent")):
            patch = mock.patch.object(news, attr, value)
            patch.start()
            self.addCleanup(patch.stop)

    def write(self, name: str, text: str = STAGED) -> Path:
        p = self.dir / name
        p.write_text(text)
        return p


class TestStamp(StampDir):
    def test_a_staged_entry_takes_the_version_in_its_name_and_in_its_frontmatter(self):
        self.write("unreleased-delegate-when.md")
        renamed, blocked = news.stamp("0.45.0")
        self.assertEqual(blocked, [])
        self.assertEqual([dst.name for _, dst in renamed], ["0.45.0-delegate-when.md"])
        self.assertFalse((self.dir / "unreleased-delegate-when.md").exists())
        entry, = news.all()
        self.assertEqual(entry.version, "0.45.0")
        self.assertEqual(entry.slug, "delegate-when")

    def test_only_the_version_line_is_rewritten(self):
        """The rest of the file is the author's. Round-tripping through `persona.parse`
        would reorder the frontmatter and drop whatever the flat parser does not keep."""
        self.write("unreleased-x.md")
        news.stamp("0.45.0")
        after = (self.dir / "0.45.0-x.md").read_text()
        self.assertEqual(after, STAGED.replace("version: unreleased", "version: 0.45.0"))

    def test_a_target_name_already_taken_stamps_nothing_at_all(self):
        """All or nothing. A partial stamp is worse than a failed one: the guard only
        asks whether SOME entry names the version, so a run that stamped one of two
        entries publishes with the other silently missing from the notes."""
        self.write("unreleased-taken.md")
        self.write("unreleased-fine.md")
        self.write("0.45.0-taken.md", STAGED.replace("unreleased", "0.45.0"))
        renamed, blocked = news.stamp("0.45.0")
        self.assertEqual(renamed, [])
        self.assertTrue(any("taken" in why for why in blocked), blocked)
        self.assertTrue((self.dir / "unreleased-fine.md").exists(),
                        "an unblocked entry was renamed even though the run was blocked")

    def test_an_already_stamped_entry_is_left_where_it_is(self):
        self.write("0.44.1-shipped.md", STAGED.replace("unreleased", "0.44.1"))
        renamed, blocked = news.stamp("0.45.0")
        self.assertEqual((renamed, blocked), ([], []))
        self.assertTrue((self.dir / "0.44.1-shipped.md").exists())

    def test_a_tag_name_is_refused_because_the_frontmatter_carries_no_leading_v(self):
        """`v0.45.0` is the tag; `0.45.0` is the version. Stamping the tag name produces
        an entry that can never equal `__version__`, so both catches pass and `charter
        news` still shows the user nothing."""
        self.write("unreleased-x.md")
        renamed, blocked = news.stamp("v0.45.0")
        self.assertEqual(renamed, [])
        self.assertTrue(blocked)
        self.assertTrue((self.dir / "unreleased-x.md").exists())

    def test_stamping_outside_a_checkout_refuses_rather_than_editing_site_packages(self):
        """The packaged copy is rebuilt from `docs/news` on every build, so a stamp
        applied there is thrown away — silently, and after the release engineer was told
        it worked."""
        with mock.patch.object(news, "_CHECKOUT", self.repo / "docs" / "elsewhere"):
            renamed, blocked = news.stamp("0.45.0")
        self.assertEqual(renamed, [])
        self.assertTrue(blocked)


class TestTheCommand(StampDir):
    def _run(self, version: str) -> tuple[int, str]:
        err = io.StringIO()
        with redirect_stdout(io.StringIO()), redirect_stderr(err):
            code = commands.cmd_news_stamp(SimpleNamespace(version=version))
        return code, err.getvalue()

    def test_stamping_reports_each_rename_and_succeeds(self):
        self.write("unreleased-delegate-when.md")
        code, out = self._run("0.45.0")
        self.assertEqual(code, 0)
        self.assertIn("0.45.0-delegate-when.md", out)

    def test_a_version_left_with_no_entry_is_named_not_assumed_fine(self):
        """Read back what was written. Exiting 0 here would hand the release engineer a
        clean run and let the failure surface at the tag instead, which is the expensive
        end of ADR 0013's point about success being checked."""
        code, out = self._run("0.45.0")
        self.assertEqual(code, 1)
        self.assertIn("0.45.0", out)

    def test_the_existing_news_flags_still_parse_now_that_news_has_a_subcommand(self):
        parser = cli.build_parser()
        for argv in (["news"], ["news", "--pending"], ["news", "--since", "0.44.0"],
                     ["news", "--for", "0.44.1"]):
            with self.subTest(argv=argv):
                args = parser.parse_args(argv)
                self.assertIs(args.func, commands.cmd_news)
        stamp = parser.parse_args(["news", "stamp", "0.45.0"])
        self.assertIs(stamp.func, commands.cmd_news_stamp)
        self.assertEqual(stamp.version, "0.45.0")


def _jobs(text: str) -> dict[str, str]:
    """`release.yml`'s job blocks by name. Flat text, because charter has no YAML."""
    body = text.split("\njobs:\n", 1)[1]
    starts = [(m.start(), m.group(1)) for m in re.finditer(r"^  ([\w-]+):$", body, re.M)]
    out = {}
    for i, (pos, name) in enumerate(starts):
        end = starts[i + 1][0] if i + 1 < len(starts) else len(body)
        out[name] = body[pos:end]
    return out


class TestTheReleaseWorkflow(unittest.TestCase):
    """The half of decision 7 that lives in CI, pinned here because a workflow is never
    exercised by the suite that ships beside it — it is exercised once, irreversibly."""

    def setUp(self):
        self.text = (REPO_ROOT / ".github" / "workflows" / "release.yml").read_text()
        self.jobs = _jobs(self.text)

    def test_the_guard_asks_charter_whether_the_version_has_notes(self):
        self.assertIn("news --for", self.jobs["guard"])

    def test_the_guard_runs_before_anything_is_published(self):
        self.assertIn("needs: guard", self.jobs["test"])
        self.assertIn("needs: build", self.jobs["publish"])

    def test_one_job_creates_the_github_release_after_publish_succeeds(self):
        after = [name for name, block in self.jobs.items() if "needs: publish" in block]
        self.assertEqual(len(after), 1, f"expected exactly one post-publish job: {after}")
        block = self.jobs[after[0]]
        self.assertIn("gh release create", block)

    def test_the_release_body_is_generated_from_the_shipped_entry(self):
        """Decision 7's whole point: one text is the offline suggestion and the public
        notes, so the two cannot drift. A hand-written `--notes` would fork them."""
        block = next(b for b in self.jobs.values() if "gh release create" in b)
        self.assertIn("news --for", block)
        self.assertIn("--notes-file", block)
        self.assertNotIn('--notes "', block)

    def test_the_release_job_carries_write_alone(self):
        """`contents: write` on the one job that needs it. Widening the workflow's
        top-level grant would hand it to `test` and `build` as well, which run the
        repository's own code."""
        self.assertIn("permissions:\n  contents: read\n", self.text)
        block = next(b for b in self.jobs.values() if "gh release create" in b)
        self.assertIn("contents: write", block)
        for name in ("guard", "test", "build"):
            self.assertNotIn("contents: write", self.jobs[name])

    def test_the_retry_path_does_not_fail_on_a_release_that_already_exists(self):
        """`workflow_dispatch` re-runs a failed release without a new tag, and the
        publish may have been the step that succeeded."""
        block = next(b for b in self.jobs.values() if "gh release create" in b)
        self.assertIn("gh release view", block)


if __name__ == "__main__":
    unittest.main()
