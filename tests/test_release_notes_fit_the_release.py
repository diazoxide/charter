"""A release body GitHub will refuse is a release that publishes and then loses its notes.

`release.yml`'s `announce` job ends with

    python -m charter news --for "$version" > "$RUNNER_TEMP/notes.md"
    gh release create "$tag" --title "$tag" --notes-file "$RUNNER_TEMP/notes.md"

and GitHub's create-release API refuses a body over **125,000 characters** with
``body is too long (maximum is 125000 characters)``. `gh` does not truncate to fit; it
forwards the refusal. So a version whose entries render past that limit does not get
shorter notes — it gets **no Release at all**.

**Why that is a release-stopper and not a cosmetic bug.** `announce` is `needs: publish`,
so the order is: guard, test, build, *upload to PyPI*, then create the Release. The
upload is the irreversible step and it has already happened by the time GitHub refuses.
Worse, the documented retry — `gh workflow run release.yml -f version=<X.Y.Z>` — cannot
repair it: the retry re-enters `publish`, `pypa/gh-action-pypi-publish` is called without
`skip-existing`, and PyPI rejects a version it already has, so `announce` is never reached
on any subsequent run. The only way out is the one the release charter forbids — writing
the Release body by hand, which forks the published notes from the shipped entry that is
supposed to be their single source.

**The existing guard cannot see this.** `guard` already refuses a version whose notes
cannot be *rendered*, and does it by asking `charter news --for` — the same call
`announce` makes — precisely so "the guard passed" and "the notes render" cannot disagree.
That reasoning is sound and this does not weaken it: rendering is not the failing step.
`charter news --for` exits 0 on a 300,000-character body, because producing the string is
exactly what it was asked to do. The claim nobody was making is that the string **fits
where it is about to be sent**.

**Why this asserts on the shipped tree rather than a fixture.** A fixture proves
`render_body` can count, which nothing doubted. The question here is a fact about the
entries this repository is actually carrying right now, and it is only answerable by
reading them: entries accumulate one PR at a time, each author sees their own and no
other, and the total crosses the limit on some ordinary afternoon with no PR to blame.
0.52.0 published at 111,349 characters — 3,651 short of the refusal — and nothing in the
repository noticed how close that was, because nothing was looking.

The two cases below are deliberately separate. `test_published_versions_fit` is the
permanent regression guard and passes. `test_the_staged_release_fits` is about the release
being cut next, and it is the one that goes red while the staged set is too big — at
`unittest` time, on a branch, which is the cheap end. The expensive end is a PyPI upload
that cannot be undone and a Release that cannot be created.
"""

from __future__ import annotations

import unittest

from charter import news

#: GitHub's documented maximum for a release body, in characters — not bytes. The API
#: counts characters, and charter's entries are not ASCII (they carry `—`, `✗`, `⬢`), so
#: measuring `len(body.encode())` would report a larger number than the one GitHub
#: applies and would fail a release that would in fact have been accepted.
GITHUB_RELEASE_BODY_MAX = 125_000

#: How close to the ceiling a version may come before this suite says so. A release that
#: fits with 200 characters to spare is not a release that is safe — the next entry lands
#: on main without anyone re-rendering the body, and the version after it is the one that
#: cannot publish. 0.52.0 sat at 89% of the limit and read as fine.
HEADROOM = 0.85


def _versions() -> set[str]:
    return {e.version for e in news.all() if e.version != news.UNRELEASED}


class ReleaseBodyFits(unittest.TestCase):
    def test_published_versions_fit(self) -> None:
        """Every stamped version still renders a body GitHub would accept.

        This is the regression half: it is about entries that already shipped, so it must
        stay green forever. It would go red if someone lengthened an old entry — which is
        allowed, and which is exactly when you want to be told.
        """
        for version in sorted(_versions()):
            with self.subTest(version=version):
                size = len(news.render_body(version))
                self.assertLessEqual(
                    size, GITHUB_RELEASE_BODY_MAX,
                    f"the rendered notes for {version} are {size:,} characters and "
                    f"GitHub refuses a release body over {GITHUB_RELEASE_BODY_MAX:,} — "
                    f"`gh release create` would fail with `body is too long`",
                )

    def test_the_staged_release_fits(self) -> None:
        """The entries staged for the next release render a body GitHub would accept.

        `render_body(UNRELEASED)` is the real rendering path, reached through the real
        `news.all()` sort, so this measures the string `announce` will actually send —
        not an approximation of it. Stamping renames files and rewrites one frontmatter
        line; it does not change a body's length.
        """
        staged = news.for_version(news.UNRELEASED)
        if not staged:
            self.skipTest("no entries staged for the next release")
        size = len(news.render_body(news.UNRELEASED))
        self.assertLessEqual(
            size, GITHUB_RELEASE_BODY_MAX,
            f"{len(staged)} entries are staged for the next release and they render to "
            f"{size:,} characters. GitHub refuses a release body over "
            f"{GITHUB_RELEASE_BODY_MAX:,}, so `announce` would fail with `body is too "
            f"long` AFTER the PyPI upload has already happened, and the documented "
            f"workflow_dispatch retry cannot reach `announce` again because `publish` "
            f"fails on a version PyPI already has. Bound the body before tagging.",
        )

    def test_the_staged_release_has_headroom(self) -> None:
        """And it is not merely under the limit, but under it with room to spare.

        Separate from the case above because the two say different things. "Would this
        release publish?" is a fact about today. "Is the next one going to?" is the one
        0.52.0 could have answered and was never asked.
        """
        staged = news.for_version(news.UNRELEASED)
        if not staged:
            self.skipTest("no entries staged for the next release")
        size = len(news.render_body(news.UNRELEASED))
        ceiling = int(GITHUB_RELEASE_BODY_MAX * HEADROOM)
        self.assertLessEqual(
            size, ceiling,
            f"the staged notes are {size:,} characters, which is "
            f"{size / GITHUB_RELEASE_BODY_MAX:.0%} of GitHub's "
            f"{GITHUB_RELEASE_BODY_MAX:,}-character limit. Under the limit is not the "
            f"same as safe: the next entry to land is not re-measured by anything.",
        )


if __name__ == "__main__":
    unittest.main()
