"""A release body GitHub will refuse is a release that publishes and then loses its notes.

`release.yml`'s `announce` job ends with

    python -m charter news --for "$version" > "$RUNNER_TEMP/notes.md"
    gh release create "$tag" --title "$tag" --notes-file "$RUNNER_TEMP/notes.md"

and GitHub's create-release API refuses a body over **125,000 characters** with
``body is too long (maximum is 125000 characters)``. `gh` does not truncate to fit; it
forwards the refusal. So a version whose entries render past that limit did not get
shorter notes — it got **no Release at all**.

**Why that was a release-stopper and not a cosmetic bug.** `announce` is `needs: publish`,
so the order is: guard, test, build, *upload to PyPI*, then create the Release. The upload
is the irreversible step and it has already happened by the time GitHub refuses. Worse,
the documented retry — `gh workflow run release.yml -f version=<X.Y.Z>` — cannot repair
it: the retry re-enters `publish`, `pypa/gh-action-pypi-publish` is called without
`skip-existing`, and PyPI rejects a version it already has, so `announce` is never reached
on any subsequent run. The only way out is the one the release charter forbids — writing
the Release body by hand, which forks the published notes from the shipped entry that is
supposed to be their single source.

**The existing guard could not see this, and was not wrong.** `guard` already refuses a
version whose notes cannot be *rendered*, and does it by asking `charter news --for` — the
same call `announce` makes — precisely so "the guard passed" and "the notes render" cannot
disagree. That reasoning is sound and nothing here weakens it: rendering was never the
failing step. `charter news --for` exits 0 on a 300,000-character body, because producing
the string is exactly what it was asked to do. The claim nobody was making is that the
string **fits where it is about to be sent**, and this module is that claim.

**How close it already was.** 0.52.0 published at 111,723 characters — 3,277 short of the
refusal — and nothing in the repository noticed, because nothing was looking. Entries
accumulate one pull request at a time, each author sees their own and no other, and the
total crosses on some ordinary afternoon with no pull request to blame. The 86 entries that
became 0.54.0 render to 423,196 whole — 3.4× the refusal, and the release that turned the
bound below from insurance into the ordinary path.

## What this suite holds, and why it is not one assertion

Three groups, and they fail for different reasons at different times.

**It fits.** `news.render_body` bounds what it returns, so every version — shipped or
staged — renders a body `gh release create` will accept. Asserted against the *shipped
tree* rather than a fixture, because a fixture proves `render_body` can count, which
nothing doubted; the question is a fact about the entries this repository is carrying right
now and it is only answerable by reading them.

**And nothing was dropped to make it fit.** That is the dangerous half. A body cut at
125,000 characters is the "convincing empty" this codebase refuses everywhere: a release
that ends mid-sentence with a dozen notes simply absent reads exactly like a release that
shipped a dozen fewer things, and no reader can tell. So the bound is a rendering, not a
truncation — the notes that fit are whole, the rest are a headline and a link, and a
heading between them says so in the body itself. Every one of those properties is asserted
below, including the one that would be easiest to lose by accident: a security note is
never demoted to a link while an ordinary note keeps its body.

**And a body that cannot be bounded stops the release at the cheap end.** Even with every
note reduced to a headline there is a length that will not fit, and the honest answer to it
is a refusal rather than a cut. `charter news --for` refuses, which puts the refusal in
`guard` — before `test`, `build` and `publish` — instead of after the upload.
"""

from __future__ import annotations

import io
import os
import re
import shutil
import subprocess
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands, news
from tests.test_workflows import _release

#: GitHub's documented maximum for a release body. The refusal reads `body is too long
#: (maximum is 125000 characters)`, and whether the validator behind it counts code points
#: or bytes is not something charter can run and find out — so `news._sent_length` measures
#: the larger of the two and this file compares against it in that measure.
#:
#: **Written out here rather than imported from `news`**, and that duplication is the
#: point: this is GitHub's number, and a test that read charter's copy of it would agree
#: with a typo in charter's copy of it. The two are compared below, in the one direction
#: that matters — charter may be more conservative, never less.
GITHUB_RELEASE_BODY_MAX = 125_000

#: What charter keeps back from that limit rather than spending on notes — **and the one
#: number both guards on `_BODY_BUDGET` read**, from opposite sides.
#:
#: A flat count and not a fraction, and the replacement of `HEADROOM = 0.85`, which is what
#: #878 was. A fraction says how much is left over without ever saying what it is *for*, so
#: nothing about it can be checked and it drifts against whatever else is measuring the same
#: hazard: 0.85 reserved a fifth of the allowance against a chars-vs-bytes gap measured, on
#: the tree, at under 1.3%. What is actually left to reserve for is one thing — something
#: added to the body that charter did not render, a preamble or a footer or a wrapper on the
#: `announce` step, landing on a body already at the budget and after the PyPI upload. The
#: largest block charter itself adds to a body no entry wrote is `news._elision`'s notice,
#: at 443 bytes. This is seven of those.
#:
#: The gap between the string charter measures and the string GitHub counts is deliberately
#: NOT in here: `news._sent_length` closes it on the string, and the newline `print` adds is
#: counted by `commands.cmd_news` where it happens. A reserve holding hazards that are
#: measured elsewhere is a reserve nobody can size.
RESERVE = 3_000

#: The most `_BODY_BUDGET` may be — and, read from the other end, the size above which a
#: release may be reshaped at all.
#:
#: **One number for both guards, which is the structural half of #878.** The ceiling case
#: holds `_BODY_BUDGET` at or below this. The floor case asks whether charter reshaped any
#: release whose whole body fits *under this same line* — so the largest thing the floor can
#: ever demand is this number, and the two cannot cross however much history accumulates.
#: The old pair could: the floor counted every release GitHub would have accepted whole
#: across all of history and its demand grew monotonically, while the ceiling was pinned to
#: a fixed fraction and never moved. 0.56.0 was the release that landed between them, and no
#: value of `_BODY_BUDGET` was green.
CEILING = GITHUB_RELEASE_BODY_MAX - RESERVE

_V = "0.60.0"


def _versions() -> set[str]:
    return {e.version for e in news.all() if e.version != news.UNRELEASED}


def _whole(version: str) -> str:
    """*version*'s entries rendered whole — the body before the bound has any say in it."""
    return "\n\n".join(news._part(e) for e in news.for_version(version))


def _within_the_reserve() -> dict[str, int]:
    """Every version charter could publish whole without spending :data:`RESERVE`, and how
    long its whole body is.

    **Stamped versions and the staged one together**, because the question they are asked
    below is the same question and the answer must not change when `charter news stamp`
    renames the files. The old floor read only stamped versions and the old ceiling read
    only the staged ones, so on a release commit — where the staged entries have just become
    a stamped version — one of them changed its mind and the other went silent.

    Filtered by :data:`CEILING` rather than by GitHub's raw limit, which is what stops the
    demand ratcheting past what the ceiling allows. A release whose notes come to more than
    charter will spend is one charter is *supposed* to reshape; asking it to render whole
    would be asking for the reserve back.
    """
    sizes = {v: news._sent_length(_whole(v))
             for v in [*sorted(_versions()), news.UNRELEASED] if news.for_version(v)}
    return {v: n for v, n in sizes.items() if n <= CEILING}


class ReleaseBodyFits(unittest.TestCase):
    """The shipped tree, measured. Every case here reads real entries."""

    def test_charter_never_spends_more_of_the_limit_than_github_allows(self):
        """The two constants, checked against each other in the direction with teeth.

        Charter's ceiling being *below* GitHub's is a policy and this suite has no opinion
        on how far below. Charter's ceiling being *above* GitHub's is a release that
        renders happily and is then refused, which is the whole finding — so that is the
        comparison, rather than an equality that would forbid the headroom on purpose.
        """
        self.assertLessEqual(
            news.RELEASE_BODY_MAX, GITHUB_RELEASE_BODY_MAX,
            f"charter believes GitHub accepts {news.RELEASE_BODY_MAX:,} characters and "
            f"GitHub accepts {GITHUB_RELEASE_BODY_MAX:,}")
        self.assertLessEqual(
            news._BODY_BUDGET, news.RELEASE_BODY_MAX,
            "render_body is allowed to spend more than the limit it is bounding against")

    def test_published_versions_fit(self) -> None:
        """Every stamped version renders a body GitHub would accept.

        This is the regression half: it is about entries that already shipped, so it must
        stay green forever. It would go red if someone lengthened an old entry past what
        the bound can absorb — which is allowed, and which is exactly when you want to be
        told.
        """
        for version in sorted(_versions()):
            with self.subTest(version=version):
                size = news._sent_length(news.render_body(version)) + 1
                self.assertLessEqual(
                    size, GITHUB_RELEASE_BODY_MAX,
                    f"the notes file announce would write for {version} is {size:,} "
                    f"long and GitHub refuses a release body over "
                    f"{GITHUB_RELEASE_BODY_MAX:,} — `gh release create` would fail with "
                    f"`body is too long`",
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
        size = news._sent_length(news.render_body(news.UNRELEASED)) + 1
        self.assertLessEqual(
            size, GITHUB_RELEASE_BODY_MAX,
            f"{len(staged)} entries are staged for the next release and they render to "
            f"{size:,} characters. GitHub refuses a release body over "
            f"{GITHUB_RELEASE_BODY_MAX:,}, so `announce` would fail with `body is too "
            f"long` AFTER the PyPI upload has already happened, and the documented "
            f"workflow_dispatch retry cannot reach `announce` again because `publish` "
            f"fails on a version PyPI already has.",
        )

    def test_the_budget_keeps_the_reserve_it_declares(self) -> None:
        """`_BODY_BUDGET`'s value, pinned from above — the half that objects to raising it.

        **It reads no entries, and that is the fix rather than a weakening.** The case this
        replaces measured `render_body(UNRELEASED)` and skipped when nothing was staged,
        which is exactly the tree `charter news stamp` leaves behind — so on every release
        commit, the one place this number decides a body about to be uploaded, the objection
        did not run and a skipped case reported success. It could not have had teeth anyway:
        `render_body` bounds its own output at `_BODY_BUDGET`, so no rendered body can ever
        exceed it and no measurement of one can constrain it. The constant is the only thing
        there is to hold, so this holds the constant, in the open, against a limit written
        out independently a few lines above.

        Two assertions because there are two different failures, and only the first of them
        is the one this suite was founded on:

        **The hard edge.** A body at the budget, plus the newline `print` adds, must fit
        GitHub's limit. Past that line a release is *refused* — after the PyPI upload, out
        of reach of the documented retry.

        **And the declared reserve.** Between the hard edge and :data:`CEILING` a release
        still publishes, so this half is policy rather than catastrophe: it is the promise
        that :data:`RESERVE` is actually left over for something charter does not render.
        Spending it is allowed and takes an edit to `RESERVE` here, in a file charter does
        not own, next to the sentence saying what it was being kept for.
        """
        self.assertLessEqual(
            news._BODY_BUDGET + 1, GITHUB_RELEASE_BODY_MAX,
            f"render_body will spend up to {news._BODY_BUDGET:,} and `print` adds a "
            f"newline, so the file announce writes can reach "
            f"{news._BODY_BUDGET + 1:,} — past GitHub's {GITHUB_RELEASE_BODY_MAX:,}. "
            f"`gh release create` fails with `body is too long`, in `announce`, after "
            f"the PyPI upload that cannot be taken back.")
        self.assertLessEqual(
            news._BODY_BUDGET, CEILING,
            f"_BODY_BUDGET is {news._BODY_BUDGET:,}, which leaves "
            f"{GITHUB_RELEASE_BODY_MAX - news._BODY_BUDGET:,} of GitHub's "
            f"{GITHUB_RELEASE_BODY_MAX:,} for anything charter did not render — a "
            f"preamble, a footer, a wrapper on the announce step. RESERVE says that "
            f"should be {RESERVE:,}. Lower RESERVE here if the reserve is what you mean "
            f"to spend; do not raise the budget past it silently.")

    def test_the_floor_can_never_ask_for_more_than_the_ceiling_allows(self) -> None:
        """#878 itself, asserted rather than argued — the two guards cannot cross.

        The floor below requires every version in :func:`_within_the_reserve` to render
        whole, so the largest budget it can ever demand is the largest whole body in that
        set. The set is filtered by :data:`CEILING`, so that demand is bounded by the
        ceiling by construction and the window between them is never empty.

        The version of this suite that reported #878 had no such bound. Its floor counted
        every release GitHub would have accepted whole, across all history, and required all
        but one of them to render whole — a demand that only ever grows as releases land in
        the band below the limit — while its ceiling was a fixed fraction of that limit and
        never moved. They crossed on 0.56.0, and `_BODY_BUDGET` had no legal value.

        Written as a case rather than left as a property of the code because it is a
        property of the code that one deleted filter removes: drop the `<= CEILING` from
        `_within_the_reserve` and this reddens immediately, on 0.54.0's 426,799.
        """
        demand = max(_within_the_reserve().values(), default=0)
        self.assertLessEqual(
            demand, CEILING,
            f"the floor demands a budget of at least {demand:,} and the ceiling forbids "
            f"anything over {CEILING:,}, so no value of _BODY_BUDGET is green. The two "
            f"guards are reading different windows again, which is #878.")

    def test_the_bound_is_an_exception_rather_than_the_normal_path(self):
        """`_BODY_BUDGET`'s value, pinned from below, and the half with something to
        measure. Too high is a constant compared against a limit; too low is a fact about
        the notes this repository is carrying, and only reading them answers it.

        **Too low and every release becomes a table of contents** — every length assertion
        in this file still passes, and the notes stop being notes. Nothing else in the suite
        would notice: a smaller budget is *safer* in the only direction the other cases
        measure.

        **The set is every version charter could have published whole with the reserve
        intact — stamped and staged — and none of them may be reshaped.** Two corrections
        are folded into that sentence, and they are the two halves of #878.

        *It used to be every elided release*, which charged the budget for outcomes the
        budget cannot change: 0.54.0's 86 notes come to 423,196 characters whole, 3.4× the
        125,000 the API takes, and no value of `_BODY_BUDGET` publishes that body. Its
        elision is the bound working.

        *And then it was every release GitHub would have accepted whole*, which is the
        ratchet #878 reported: a demand growing with history, against a ceiling that never
        moved. Filtering by :data:`CEILING` instead asks the question the reserve makes
        sense of — **is charter shortening notes it did not have to shorten, given what it
        said it would keep back?** — and bounds the demand by the ceiling in the same
        breath.

        *And it now reads the staged release too.* The staged entries are the next stamped
        version; leaving them out meant this case gave one answer on a branch and another on
        the release commit cut from it, which is the tree where the answer is published.

        Teeth, measured on the shipped tree rather than by reading a literal from `news.py`
        back at itself: at 122,000 nothing in the set is reshaped. At 115,000 the staged
        release is. At 100,000 — the value #878 was filed against — 0.52.0 joins it, four of
        its 24 notes reduced to links though the Release GitHub actually holds for v0.52.0
        carries all 24 whole. At 90,000, 0.53.0 joins them.
        """
        reshaped = sorted(v for v in _within_the_reserve()
                          if news.render_body(v) != _whole(v))
        self.assertFalse(
            reshaped,
            f"charter's own budget reshapes {len(reshaped)} release(s) it had the room to "
            f"publish whole ({', '.join(reshaped)}) — so _BODY_BUDGET is too small: it is "
            f"eliding notes to buy headroom RESERVE has already accounted for. Raising it "
            f"is the fix and `test_the_budget_keeps_the_reserve_it_declares` says how far, "
            f"on this tree and every other — it reads no entries, so it does not go quiet "
            f"on a stamped release branch the way its predecessor did.")

    def test_the_number_charter_bounds_by_is_the_size_of_the_file_announce_writes(self):
        """What `news._sent_length` claims to be, checked against the thing itself.

        `announce` redirects `charter news --for` into a file and hands GitHub the file, so
        the length that decides an elision and a refusal has to be the length of that file —
        the body, encoded, plus the newline `print` writes. Asserted as an equality against
        the encoded string, which is the one construction of it that does not go through
        `_sent_length`.

        **And the two measures really do differ here**, or the equality above would hold for
        `len` as well and this would pin nothing. The second assertion says so on real
        entries: charter's notes carry `—`, `✗` and `⬢`, and the encoded body of every
        version charter has cut runs between 0.25% and 1.22% longer than its character count.
        """
        differ = 0
        for version in [*sorted(_versions()), news.UNRELEASED]:
            if not news.for_version(version):
                continue
            body = news.render_body(version)
            with self.subTest(version=version):
                self.assertEqual(
                    news._sent_length(body) + 1, len((body + "\n").encode()),
                    "the number charter bounds and refuses on is not the size of the "
                    "file announce writes")
            differ += news._sent_length(body) > len(body)
        self.assertTrue(
            differ, "no version's encoded body is longer than its character count, so "
                    "the case above cannot tell the two measures apart")

    def test_every_note_the_staged_release_carries_is_in_the_body(self):
        """The half that makes the case above worth passing.

        A bound that fits by dropping notes passes every length assertion in this class
        and is the exact failure they exist to prevent — the release publishes, the body
        is accepted, and a dozen things that shipped are not in it. So the shipped tree is
        asked the other question too: is every staged note still *in* the body it renders?
        """
        staged = news.for_version(news.UNRELEASED)
        if not staged:
            self.skipTest("no entries staged for the next release")
        body = news.render_body(news.UNRELEASED)
        for e in staged:
            with self.subTest(slug=e.slug):
                self.assertIn(f"### {news.marker(e)}{e.headline}", body,
                              f"{e.slug} is staged for the next release and its heading "
                              f"is not in the body that release would publish")

    def test_and_every_note_it_could_not_carry_whole_is_reachable(self):
        """Elided is not dropped, and the difference has to be checkable rather than
        promised. Each note the body could not carry whole is followed by a link, and the
        link resolves to a path that exists in this checkout."""
        staged = news.for_version(news.UNRELEASED)
        if not staged:
            self.skipTest("no entries staged for the next release")
        body = news.render_body(news.UNRELEASED)
        if "listed by headline only" not in body:
            self.skipTest("the staged notes fit whole, so nothing is elided to reach")
        root = Path(__file__).resolve().parent.parent
        linked = re.findall(r"Full note: \[`(docs/news/[^`]+)`\]\((https://[^)]+)\)", body)
        self.assertTrue(linked, "the body says it elided notes and links to none of them")
        for path, url in linked:
            with self.subTest(path=path):
                self.assertTrue((root / path).is_file(),
                                f"{path} is linked from the release body and is not a "
                                f"file in this repository")
                self.assertTrue(url.endswith(Path(path).name),
                                f"{url} does not point at {path}")


def _entry(version: str, headline: str, body: str = "body text", **fields) -> str:
    lines = [f"version: {version}", f"headline: {headline}"]
    lines += [f"{k}: {v}" for k, v in fields.items()]
    return "---\n" + "\n".join(lines) + f"\n---\n\n{body}\n"


class NewsDir(unittest.TestCase):
    """Entries read from a throwaway directory, so a test never depends on what shipped —
    the same isolation `tests/test_news.py` and `tests/test_news_ordering.py` establish,
    for the same reason."""

    def setUp(self):
        self.dir = Path(tempfile.mkdtemp())
        # Removed rather than left behind: these fixtures are hundreds of kilobytes each,
        # which is the whole point of them.
        self.addCleanup(shutil.rmtree, self.dir, ignore_errors=True)
        patch = mock.patch.object(news, "_PACKAGED", self.dir)
        patch.start()
        self.addCleanup(patch.stop)

    def write(self, name: str, text: str) -> None:
        (self.dir / name).write_text(text)

    def filler(self, token: str, size: int = 20_000) -> str:
        """A body long enough to matter, carrying a token nothing else can produce."""
        return f"{token} " + "x" * size

    def oversized(self, ordinary: int = 10, secure: int = 2) -> None:
        """One version whose notes cannot all be rendered whole.

        **Filename order and declared order disagree, deliberately.** The security notes
        are named `z-…` and the ordinary ones `a-…`, so a bound that elided in filename
        order would keep every ordinary body and drop every security one — and a fixture
        where the right answer is also the alphabetical one is a fixture that cannot fail.
        """
        for i in range(ordinary):
            slug = f"a-ordinary-{i}"
            self.write(f"{_V}-{slug}.md",
                       _entry(_V, f"ordinary {i}", self.filler(f"BODY-{slug}")))
        for i in range(secure):
            slug = f"z-security-{i}"
            self.write(f"{_V}-{slug}.md",
                       _entry(_V, f"security {i}", self.filler(f"BODY-{slug}"),
                              security="true"))

    def headings(self, body: str) -> list[str]:
        return [ln[4:] for ln in body.splitlines() if ln.startswith("### ")]


class ABodyThatFitsIsUntouched(NewsDir):
    """The bound is a ceiling, not a style. Every release but two has rendered whole and
    must keep rendering whole, byte for byte — a change that reshaped the ordinary case
    would be this fix rewriting fifteen published releases to solve a problem two have."""

    def test_a_short_version_renders_every_body_in_full(self):
        for slug in ("a-one", "b-two", "c-three"):
            self.write(f"{_V}-{slug}.md", _entry(_V, slug, f"BODY-{slug}"))
        body = news.render_body(_V)
        for slug in ("a-one", "b-two", "c-three"):
            self.assertIn(f"BODY-{slug}", body)
        self.assertNotIn("listed by headline only", body)
        self.assertNotIn("Full note:", body)

    def test_a_body_of_exactly_the_budget_is_still_rendered_whole(self):
        """The early return's boundary, not just its direction.

        `<` instead of `<=` would send a release that fits exactly through the elision
        path — every note but the last rendered whole, the last one traded for a link, and
        a heading explaining an elision that was not needed. One character, and the only
        release it could ever happen to is one nobody would think to test.

        The dial moves rather than the fixture, for the reason its sibling in
        `TheGateRefusesABodyItCannotBound` does: `_BODY_BUDGET` is charter's own policy
        number and the claim under test is what the comparison means by it, not what the
        number is.

        Measured with `news._sent_length` and not `len`, because that is what the budget is
        denominated in — an ASCII fixture makes the two agree, and a case that only holds
        while the fixture stays ASCII is one a `—` in a headline would silently retire.
        """
        for slug in ("a-one", "b-two", "c-three"):
            self.write(f"{_V}-{slug}.md", _entry(_V, slug, self.filler(f"BODY-{slug}")))
        whole = "\n\n".join(news._part(e) for e in news.for_version(_V))
        with mock.patch.object(news, "_BODY_BUDGET", news._sent_length(whole)):
            self.assertEqual(
                news.render_body(_V), whole,
                "a body of exactly the budget went through the elision path, so the "
                "budget is being read as exclusive")
        with mock.patch.object(news, "_BODY_BUDGET", news._sent_length(whole) - 1):
            self.assertIn(
                "listed by headline only", news.render_body(_V),
                "one character under the budget changed nothing, so the case above is "
                "not measuring the boundary it claims to")

    def test_an_entry_with_no_body_does_not_open_a_gap_in_the_notes(self):
        """The one shape `_part`'s `rstrip` is for, and the only one it can change.

        `persona.parse` hands back a stripped body, so for every entry that HAS one the
        strip is a no-op — which makes an entry with a headline and nothing under it the
        whole of what that call does. Without it the heading keeps the two newlines meant
        to separate it from a body that is not there, and the notes carry a blank gap that
        reads like a note whose text failed to render.

        Asserted as an equality for the reason the case below is: the claim is about the
        exact separation between entries, and `assertIn` cannot see a doubled blank line.
        """
        self.write(f"{_V}-a-one.md", _entry(_V, "one", body=""))
        self.write(f"{_V}-b-two.md", _entry(_V, "two", body="BODY-b-two"))
        self.assertEqual(news.render_body(_V), "### one\n\n### two\n\nBODY-b-two")

    def test_and_is_exactly_what_joining_the_entries_gives(self):
        """Stated as an equality rather than as three `assertIn`s, because "unchanged" is
        the claim: no preamble, no footer, no separator that a reader of an older release
        would not recognise."""
        for slug in ("a-one", "b-two"):
            self.write(f"{_V}-{slug}.md", _entry(_V, slug, f"BODY-{slug}"))
        self.assertEqual(
            news.render_body(_V),
            "### a-one\n\nBODY-a-one\n\n### b-two\n\nBODY-b-two")


class NothingIsDroppedToMakeItFit(NewsDir):
    """The property that separates a bound from a truncation.

    Cutting the string at the limit satisfies every length assertion in this file. It is
    also indistinguishable, to a reader, from a release that shipped fewer things — which
    is why "it renders under the limit" is not the assertion this finding needs.
    """

    def test_the_fixture_is_actually_over_the_bound(self):
        """Otherwise every case below is asserting about a body that was never elided."""
        self.oversized()
        whole = sum(len(f"### {e.headline}\n\n{e.body}".rstrip())
                    for e in news.for_version(_V))
        self.assertGreater(whole, news._BODY_BUDGET,
                           "this fixture fits, so it exercises none of the bound")
        self.assertIn("listed by headline only", news.render_body(_V))

    def test_the_bound_holds(self):
        self.oversized()
        self.assertLessEqual(news._sent_length(news.render_body(_V)), news._BODY_BUDGET)

    def test_every_entry_still_has_its_own_heading(self):
        """In its own place in the order. Twelve entries in, twelve headings out."""
        self.oversized()
        self.assertEqual(
            self.headings(news.render_body(_V)),
            [f"{news.marker(e)}{e.headline}" for e in news.for_version(_V)])

    def test_no_body_is_cut_short(self):
        """Every note is whole or absent — never half. An excerpt is the shape with no
        mark on it saying where it stopped, which is the reading failure this refuses."""
        self.oversized()
        body = news.render_body(_V)
        for e in news.for_version(_V):
            token = f"BODY-{e.slug}"
            with self.subTest(slug=e.slug):
                if token in body:
                    self.assertIn(e.body.strip(), body,
                                  f"{e.slug} is in the release body with only part of "
                                  f"its text")

    def test_every_elided_entry_names_the_note_that_holds_its_text(self):
        self.oversized()
        body = news.render_body(_V)
        for e in news.for_version(_V):
            if f"BODY-{e.slug}" in body:
                continue
            with self.subTest(slug=e.slug):
                self.assertIn(f"docs/news/{_V}-{e.slug}.md", body,
                              f"{e.slug} lost its body and the release notes do not say "
                              f"where the body is")

    def test_at_least_one_entry_was_elided_and_at_least_one_was_not(self):
        """The bound has two failure modes that both pass a length check: eliding nothing
        (and overflowing) and eliding everything (and publishing a table of contents where
        the notes were). This is the case that has an opinion about the middle."""
        self.oversized()
        body = news.render_body(_V)
        kept = [e.slug for e in news.for_version(_V) if f"BODY-{e.slug}" in body]
        gone = [e.slug for e in news.for_version(_V) if f"BODY-{e.slug}" not in body]
        self.assertTrue(kept, "every note was reduced to a link")
        self.assertTrue(gone, "nothing was elided, so the body is over the bound")


class TheBodySaysSoItself(NewsDir):
    """A body that quietly links half its notes is still a body a reader trusts whole.

    The elision is therefore announced in the document, at the cut, as a heading — so it
    lands in GitHub's own outline rather than in a paragraph a reader scrolls past.
    """

    #: How the notice is found, here and on the shipped tree. Anchored on its own sentence
    #: rather than on `## `, because entry bodies write their own `##` headings — 122 of
    #: them across `docs/news` today — so the heading level alone identifies nothing.
    NOTICE = re.compile(r"(?m)^## (\d+) of these (\d+) notes are listed by headline only$")

    def test_the_notice_is_a_heading_in_the_body(self):
        self.oversized()
        self.assertRegex(news.render_body(_V), self.NOTICE)

    def test_it_is_written_once(self):
        """Once, because it is one fact. Two copies of an arithmetic sentence is two
        answers to `how many were elided?`, and nothing renders them together."""
        self.oversized()
        self.assertEqual(len(self.NOTICE.findall(news.render_body(_V))), 1)

    def test_it_counts_both_halves_and_names_the_limit(self):
        """"Some notes are linked" is a sentence a reader cannot check. These they can."""
        self.oversized()
        body = news.render_body(_V)
        entries = news.for_version(_V)
        kept = sum(1 for e in entries if f"BODY-{e.slug}" in body)
        self.assertIn(f"## {len(entries) - kept} of these {len(entries)} notes", body)
        self.assertIn(f"{kept} are above in full", body)
        self.assertIn(f"{news.RELEASE_BODY_MAX:,}", body)

    def test_it_sits_between_the_whole_notes_and_the_linked_ones(self):
        """At the cut, not at the top or the bottom. A notice above the notes describes
        something the reader has not reached; one below it describes something they have
        already read past without knowing."""
        self.oversized()
        body = news.render_body(_V)
        cut = self.NOTICE.search(body).start()
        self.assertNotIn("Full note:", body[:cut],
                         "a linked note appears above the notice that announces linking")
        self.assertNotIn("BODY-", body[cut:],
                         "a whole note appears below the notice")

    def test_the_notice_is_ruled_off_from_the_notes_above_it(self):
        """The other half of that conditional. A heading immediately after the last whole
        note reads as that note's own subheading — entry bodies write `##` headings of
        their own, 122 of them across `docs/news` — so the rule is what makes the break a
        break. Asserting only the case where it is absent would leave "always absent"
        passing, which is the same document with the seam rubbed out.
        """
        self.oversized()
        body = news.render_body(_V)
        cut = self.NOTICE.search(body).start()
        self.assertTrue(body[:cut].endswith("---\n\n"),
                        f"the notice follows the last whole note with no rule between "
                        f"them: {body[cut - 30:cut]!r}")

    def test_a_version_where_no_note_fits_whole_does_not_open_with_a_rule(self):
        """Not a hypothetical shape: one note longer than the budget produces it. The rule
        exists to separate the whole notes from the linked ones, so with no whole notes it
        separates nothing — and a body that opens with `---` is one some renderers read as
        frontmatter rather than as a horizontal line."""
        self.write(f"{_V}-a-huge.md",
                   _entry(_V, "the only one", self.filler("BODY-a-huge", 150_000)))
        body = news.render_body(_V)
        self.assertFalse(body.startswith("---"), body[:40])
        self.assertRegex(body, self.NOTICE)
        self.assertIn("0 are above in full", body)
        self.assertNotIn("BODY-a-huge", body)
        self.assertIn("docs/news/0.60.0-a-huge.md", body)

    def test_it_says_nothing_was_dropped_and_that_is_true(self):
        self.oversized()
        body = news.render_body(_V)
        self.assertIn("Every note this version shipped is in this list.", body)
        self.assertEqual(len(self.headings(body)), len(news.for_version(_V)))


class OrderDecidesWhatKeepsItsBody(NewsDir):
    """The demotion that must never happen, and the reason it cannot.

    There is no rule inside the bound that protects security notes. There is one order —
    `news.all()`'s — and the bound reads it, so the notes that lead are the notes that keep
    their bodies. A second rule would be #486 exactly: a claim honoured in one view and
    quietly not in the other.

    Which makes the consequence worth asserting rather than assuming, because it is a
    property of two functions agreeing and either of them can move.
    """

    def test_a_security_note_keeps_its_body_while_ordinary_ones_lose_theirs(self):
        self.oversized()
        body = news.render_body(_V)
        for e in news.for_version(_V):
            if e.security:
                with self.subTest(slug=e.slug):
                    self.assertIn(f"BODY-{e.slug}", body,
                                  f"{e.slug} declares `security: true` and was reduced to "
                                  f"a link")
        self.assertTrue(any(f"BODY-{e.slug}" not in body
                            for e in news.for_version(_V) if not e.security),
                        "no ordinary note was elided, so this proves nothing about which "
                        "note the bound reaches for first")

    def test_no_note_keeps_its_body_while_a_note_above_it_loses_one(self):
        """The general form, and the one that survives a change to what `security:` means.
        The cut is a single point in the order: read the body top to bottom and the whole
        notes never resume after the linked ones start."""
        self.oversized()
        body = news.render_body(_V)
        kept = [f"BODY-{e.slug}" in body for e in news.for_version(_V)]
        self.assertNotIn((False, True), list(zip(kept, kept[1:])),
                         "a note kept its body below one that lost its own — the bound is "
                         "picking notes rather than cutting once")

    def test_a_lead_note_is_not_elided_either(self):
        """`lead:` is a position rather than a class, and the position it names is first —
        so the note a release chose to open with cannot be the one reduced to a link."""
        self.oversized()
        self.write(f"{_V}-a-leading.md",
                   _entry(_V, "the lead", self.filler("BODY-a-leading"), lead="true"))
        body = news.render_body(_V)
        self.assertIn("BODY-a-leading", body)
        self.assertEqual(self.headings(body)[0], "the lead")


class AnElidedNoteIsReachable(NewsDir):
    """A link is only "nothing was dropped" if it goes somewhere."""

    def test_a_stamped_version_links_its_own_tag(self):
        """Not `main`. A published release describes what shipped, and the note behind
        each headline has to be the note **as that release shipped it** — main is free to
        rewrite it afterwards, and a tag is not."""
        self.oversized()
        body = news.render_body(_V)
        self.assertIn(f"/blob/v{_V}/docs/news/", body)
        self.assertNotIn("/blob/main/", body)

    def test_a_staged_version_links_the_branch_it_lives_on(self):
        """`unreleased` has no tag to point at, and its render is a preview nobody
        publishes — so the link goes to the branch where the file actually is."""
        for i in range(12):
            slug = f"s-{i}"
            self.write(f"{news.UNRELEASED}-{slug}.md",
                       _entry(news.UNRELEASED, f"staged {i}", self.filler(f"BODY-{slug}")))
        body = news.render_body(news.UNRELEASED)
        self.assertIn("/blob/main/docs/news/unreleased-", body)
        self.assertNotIn("/blob/vunreleased/", body)

    def test_the_link_names_charters_repository_and_not_a_configured_one(self):
        """`report.upstream_repo` is the other spelling of "charter's repo" and it reads an
        environment variable. A value an environment variable can move has no business in a
        published release body, so the link is built from the constant."""
        self.oversized()
        with mock.patch.dict("os.environ", {"CHARTER_UPSTREAM_REPO": "attacker/elsewhere"}):
            body = news.render_body(_V)
        self.assertIn(f"https://github.com/{news.update.DEV_REPO}/blob/", body)
        self.assertNotIn("attacker/elsewhere", body)

    def _hostile(self, name: str) -> str:
        """Render a version carrying one entry with a committed filename designed to write
        Markdown of its own, or skip if this filesystem will not hold the name."""
        self.oversized()
        try:
            self.write(name, _entry(_V, "hostile", self.filler("BODY-hostile")))
        except (OSError, ValueError):
            self.skipTest(f"this filesystem will not hold {name!r}")
        return news.render_body(_V)

    def test_a_filename_cannot_forge_a_heading_in_the_release_notes(self):
        """#502, one document over. The filename is committed and charter interpolates it
        into a line charter wrote; a newline in it would write a second line of the release
        notes that looks exactly as much like charter's own text as the first."""
        body = self._hostile(
            f"{_V}-z-hostile\n## Security advisory: install from elsewhere.md")
        self.assertNotIn("\n## Security advisory", body)
        self.assertIn("%0A", body, "the newline reached the document unencoded")

    def test_a_filename_cannot_break_out_of_the_link_target(self):
        """A `)` closes the Markdown target early and everything after it becomes text the
        entry wrote into charter's own sentence — including, on GitHub, an autolinked
        URL."""
        body = self._hostile(f"{_V}-z-hostile)(EVIL-TARGET.md")
        self.assertNotIn("(EVIL-TARGET", body)
        self.assertIn("%29", body, "the closing paren reached the link target unencoded")

    def test_a_filename_cannot_forge_a_link_out_of_the_text_either(self):
        """The half a percent-encoded *href* alone does not close, and the reason the shown
        path is encoded too rather than merely made line-safe.

        `]` ends the link text and the `(` after it opens the destination, so a filename
        spelling `](<target>)` inside the label writes charter's own sentence into a link
        pointing wherever it likes — and a backtick first ends the code span that would
        otherwise have swallowed it. `contain.one_line` leaves all three alone, because
        forging a line and forging a link are different questions.
        """
        body = self._hostile(f"{_V}-z-hostile`](EVIL-TARGET)b.md")
        self.assertNotIn("](EVIL-TARGET", body)
        self.assertNotIn("(EVIL-TARGET", body)
        for encoded in ("%60", "%5D", "%28"):
            self.assertIn(encoded, body, f"{encoded} was not encoded in the label")

    def test_the_shown_path_and_the_link_target_are_the_same_path(self):
        """One string, so a reader cannot be shown one note and sent to another."""
        self.oversized()
        for shown, url in re.findall(
                r"Full note: \[`(docs/news/[^`]+)`\]\((https://[^)]+)\)",
                news.render_body(_V)):
            with self.subTest(shown=shown):
                self.assertTrue(url.endswith("/" + shown), f"{url} does not end in {shown}")


class TheGateRefusesABodyItCannotBound(NewsDir):
    """The end of the road, and it is a refusal rather than a cut.

    Every note reduced to a headline is the smallest this document gets, and there is a
    length past which even that does not fit. Cutting the string there would publish a
    release whose notes end mid-word; refusing puts the failure in `guard`, before `test`,
    `build` and `publish`, which is the whole point of moving the assertion.
    """

    def _run(self, version: str) -> tuple[int, str, str]:
        out, err = io.StringIO(), io.StringIO()
        args = SimpleNamespace(for_version=version, pending=False, since=None, until=None)
        with redirect_stdout(out), redirect_stderr(err):
            code = commands.cmd_news(args)
        return code, out.getvalue(), err.getvalue()

    def test_headlines_alone_over_the_limit_are_refused_rather_than_printed(self):
        for i in range(6):
            self.write(f"{_V}-h-{i}.md",
                       _entry(_V, "h" * 30_000 + f" {i}", f"BODY-{i}"))
        code, out, err = self._run(_V)
        self.assertEqual(code, 1, "a body GitHub would refuse was printed for announce "
                                  "to publish")
        self.assertEqual(out, "", "part of an unpublishable body reached stdout, which is "
                                 "the file announce redirects into")
        self.assertIn("too long", err)
        self.assertIn(f"{news.RELEASE_BODY_MAX:,}", err)

    def test_the_refusal_says_why_linking_more_of_them_will_not_help(self):
        """The reader's obvious next move is the one that cannot work: `render_body` has
        already linked every note it can. Left unsaid, the operator retries the fix that
        made no difference, in the middle of a release."""
        for i in range(6):
            self.write(f"{_V}-h-{i}.md", _entry(_V, "h" * 30_000 + f" {i}", f"BODY-{i}"))
        _code, _out, err = self._run(_V)
        self.assertIn("headlines alone do not fit", err)

    def test_a_body_at_exactly_the_limit_is_refused_for_the_newline_print_adds(self):
        """The off-by-one that only ever shows up at the ceiling.

        `cmd_news` prints the body, so the file `announce` redirects into is one character
        longer than the string measured here — and that character is one GitHub counts. A
        gate measuring the string rather than the file would pass a body that arrives at
        125,001 characters, at the one point in the release where there is no second try.

        Rendering is stubbed because the property under test is the gate's arithmetic, not
        the renderer's: the only way to reach exactly the limit through real entries is to
        build them to the character, which would assert about the fixture instead.
        """
        self.write(f"{_V}-a.md", _entry(_V, "a"))
        with mock.patch.object(news, "render_body",
                               return_value="x" * news.RELEASE_BODY_MAX):
            code, out, err = self._run(_V)
        self.assertEqual(code, 1, f"a body of exactly {news.RELEASE_BODY_MAX:,} characters "
                                  f"was printed, and `print` makes the notes file one "
                                  f"character longer than that")
        self.assertEqual(out, "")
        self.assertIn("too long", err)

    def test_a_body_under_the_limit_in_characters_and_over_it_encoded_is_refused(self):
        """The other off-by-one at the ceiling, and the one that needed a decision.

        GitHub's refusal says `characters`. Whether the validator behind it counts code
        points or bytes charter cannot run and find out, and the only bodies where the
        answer changes anything are the ones within a percent of the limit — which is every
        body this gate exists for. So the gate takes the larger of the two measures and the
        question stops mattering.

        The fixture is the shape that separates them: 124,999 characters, one of which is an
        em dash, so the file `announce` writes is 125,002 bytes. Counted as characters it is
        exactly at the limit once `print`'s newline is added and would have been published.
        """
        self.write(f"{_V}-a.md", _entry(_V, "a"))
        body = "x" * (news.RELEASE_BODY_MAX - 2) + "—"
        self.assertEqual(len(body) + 1, news.RELEASE_BODY_MAX,
                         "the fixture is not at the limit by the character measure, so it "
                         "would be refused whichever measure the gate uses")
        with mock.patch.object(news, "render_body", return_value=body):
            code, out, err = self._run(_V)
        self.assertEqual(code, 1, "a body GitHub refuses on any byte-counting reading of "
                                  "its own limit was printed for announce to upload")
        self.assertEqual(out, "")
        self.assertIn("too long", err)

    def test_and_one_character_under_it_is_printed(self):
        """The other direction, and it is not decoration: a gate that refused everything
        would pass every case above and take the release path with it."""
        self.write(f"{_V}-a.md", _entry(_V, "a"))
        with mock.patch.object(news, "render_body",
                               return_value="x" * (news.RELEASE_BODY_MAX - 1)):
            code, out, err = self._run(_V)
        self.assertEqual(code, 0, err)
        self.assertEqual(len(out), news.RELEASE_BODY_MAX)

    def test_a_body_of_exactly_the_budget_is_spent_rather_than_cut_further(self):
        """The boundary, not just the direction. `<` instead of `<=` would elide one more
        note than the budget calls for — a note's whole text traded for a link to buy a
        character nobody needed.

        Reached by moving the budget to the length of a real candidate rather than by
        sizing a fixture to the character: the number is charter's own policy dial and the
        claim under test is what the comparison means by it, so the dial is what moves.

        `news._sent_length` and not `len`, and here the two genuinely differ: the fixture is
        ASCII but `news._elision`'s notice — which only an elided body carries — has an em
        dash in it. A budget set from the character count would be two under the byte count
        the render is comparing, and this case would be measuring an off-by-two instead of
        the boundary it names.
        """
        self.oversized()
        body = news.render_body(_V)
        exact = news._sent_length(body)
        with mock.patch.object(news, "_BODY_BUDGET", exact):
            self.assertEqual(
                news.render_body(_V), body,
                "a body of exactly the budget was cut one note further, so the budget is "
                "being read as exclusive")
        with mock.patch.object(news, "_BODY_BUDGET", exact - 1):
            self.assertLess(
                news._sent_length(news.render_body(_V)), exact,
                "one character under the budget changed nothing, so the case above is "
                "not measuring the boundary it claims to")

    def test_a_release_whose_notes_are_merely_long_still_publishes(self):
        """The refusal is for what cannot be bounded, not for what is big. 69 entries is a
        release, not a defect, and a gate that refused them would have moved the outage
        rather than removed it."""
        self.oversized(ordinary=40, secure=4)
        code, out, err = self._run(_V)
        self.assertEqual(code, 0, err)
        self.assertLessEqual(len(out), news.RELEASE_BODY_MAX)
        self.assertEqual(len(self.headings(out)), 44)


class ARefusedRenderStopsTheRelease(unittest.TestCase):
    """The other half of moving the assertion: a refusal only counts if it stops the step.

    `guard` runs `charter news --for` for its exit code, and that half is asserted where
    the guard's other refusals are. `announce` runs the same command for its *stdout*, and
    then hands the file it wrote to `gh release create`. If a refused render did not abort
    that step, `announce` would create a Release from whatever ended up in the file — an
    empty one, on the path where the PyPI upload has already happened. Which is a worse
    outcome than the failure this whole change is about: no notes, and no error either.

    Actions runs `run:` blocks under `bash -e`, so this held before the step said so. That
    is exactly why it is written out and asserted: a property that holds because of a
    platform default is a property nobody in the file states and nobody notices losing.
    """

    def _announce(self) -> str:
        job = _release()["jobs"]["announce"]
        steps = [s for s in job["steps"] if isinstance(s, dict) and "run" in s]
        self.assertEqual(len(steps), 1, "announce no longer has exactly one script step")
        return steps[0]["run"]

    def _execute(self, news_exit: int, errexit: bool = True) -> tuple[int, list[str]]:
        """Run the announce script with `python` and `gh` stubbed, and report what `gh`
        was asked to do.

        *errexit* is how the shell was started, and both values are exercised below.
        ``True`` is Actions' own invocation (`bash -e {0}`); ``False`` is the shell the
        script's own `set -eu` exists for, and is the only setting under which deleting
        that line changes anything.
        """
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(os.path.realpath(raw))
            (tmp / "pyproject.toml").write_text(
                '[project]\nname = "charter-cp"\nversion = "0.54.0"\n')
            (tmp / "bin").mkdir()
            log = tmp / "gh.log"
            (tmp / "bin" / "python").write_text(
                "#!/bin/sh\n"
                'case "$1" in\n'
                "  -c) echo 0.54.0 ;;\n"                       # the pyproject read
                f'  -m) echo "NOTES BODY"; exit {news_exit} ;;\n'   # charter news --for
                "esac\n")
            (tmp / "bin" / "gh").write_text(
                "#!/bin/sh\n"
                f'echo "$*" >> {log}\n'
                # No release exists yet, so the script goes on to render one.
                'if [ "$1 $2" = "release view" ]; then exit 1; fi\n'
                "exit 0\n")
            for name in ("python", "gh"):
                (tmp / "bin" / name).chmod(0o755)
            env = {"PATH": f"{tmp / 'bin'}:/usr/bin:/bin", "RUNNER_TEMP": str(tmp),
                   "GH_TOKEN": "stub"}
            argv = ["bash"] + (["-e"] if errexit else []) + ["-c", self._announce()]
            done = subprocess.run(argv, cwd=tmp, env=env, capture_output=True, text=True)
            asked = log.read_text().splitlines() if log.exists() else []
            return done.returncode, asked

    def test_a_render_that_refuses_leaves_no_release_behind(self):
        code, asked = self._execute(news_exit=1)
        self.assertNotEqual(code, 0, "the step succeeded after the render refused")
        self.assertFalse([c for c in asked if c.startswith("release create")],
                         f"a Release was created from a body charter refused: {asked}")

    def test_and_it_does_not_hold_only_because_of_the_runners_shell_flags(self):
        """The case the script's own `set -eu` is for, and the reason the case above is not
        enough on its own: Actions starts `run:` as `bash -e {0}`, so the property holds
        there with the line deleted — a test that ran only that shape would pin nothing.
        Run the same script under a shell that was handed no flags and it still refuses to
        create a Release from a body charter would not print."""
        code, asked = self._execute(news_exit=1, errexit=False)
        self.assertNotEqual(code, 0, "the script relies on the runner's `-e` and says so "
                                     "nowhere")
        self.assertFalse([c for c in asked if c.startswith("release create")],
                         f"a Release was created from a body charter refused: {asked}")

    def test_and_a_render_that_succeeds_still_creates_one(self):
        """The other direction, and it is not decoration: a step that failed unconditionally
        would pass every case above and never publish a Release again."""
        for errexit in (True, False):
            with self.subTest(errexit=errexit):
                code, asked = self._execute(news_exit=0, errexit=errexit)
                self.assertEqual(code, 0)
                self.assertTrue([c for c in asked if c.startswith("release create")], asked)


if __name__ == "__main__":
    unittest.main()
