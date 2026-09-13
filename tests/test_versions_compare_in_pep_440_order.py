"""#1050: a pre-release is older than its release, and every version comparison says so.

`update._parse` read ``0.60.0rc1`` as ``(0, 60, 1)``: it kept the digits of each dot-separated
part and dropped the rest, so the ``1`` of ``rc1`` became a patch number and a release
candidate compared NEWER than the release it is a candidate for. ``a``, ``b`` and ``.dev``
went the same way, and ``.post1`` became a fourth component. PEP 440 orders all of them
around the release, and so do PyPI and the installer charter hands the result to.

It was latent, because charter has published no pre-release. It would have gone live the
day one was, in every caller at once: whether `charter update` moves a machine and whether
that move is backwards (#1017's refusal), whether `charter version bump` pins the team to
it, the status line's newer-charter arrow, `charter version`'s `latest` line, the order
`charter news` prints, and whether SessionStart installs a pin or refuses it as a downgrade.
So there is one key, :func:`charter.update.version_key`, and this module pins its order.
Each caller has its own case beside its siblings, so one of them cannot drift back onto a
comparator of its own.

**The expected order is PEP 440's, written out by hand**, not derived from anything charter
computes. It is the "Summary of permitted suffixes and relative ordering" in the PEP, laid
onto charter's own version numbers.

**Two things are charter's and not the PEP's**, and each is pinned below on its own:

* A **local label** (``+dev``, ``+local``) does not move a version. PEP 440 sorts
  ``0.61.0+dev`` above ``0.61.0``, but `channel.build_label` adds that label to the SAME
  wheel's number to say where it was built, and `update._parse` has always read it as the
  release it labels. A git build is not a later release than the one it carries the number of.
* A string that is **not a version** sorts below every version, and equal to every other
  string that is not one. `_parse` promised "sorts low" and only kept that promise for
  strings with no digits in them: ``build7`` read as ``(7,)``, newer than any 0.x.
"""

from __future__ import annotations

import itertools
import unittest

from charter import update

#: Strictly increasing, each against every other. Every suffix PEP 440 has, on both sides of
#: a release, with the neighbours that make each boundary a boundary: ``.dev`` of a release
#: below its first alpha, ``.dev`` of a pre-release or post-release below that release, and a
#: number compared as a number (``0.61.10`` above ``0.61.9``).
PEP_440_ORDER = (
    "0.59.0",
    "0.60.0.dev0",
    "0.60.0.dev1",
    "0.60.0a0.dev0",
    "0.60.0a0",
    "0.60.0a1.dev0",
    "0.60.0a1",
    "0.60.0a1.post0.dev0",
    "0.60.0a1.post0",
    "0.60.0a1.post1",
    "0.60.0a2",
    "0.60.0b1",
    "0.60.0rc1",
    "0.60.0rc2",
    "0.60.0rc10",
    "0.60.0",
    "0.60.0.post0.dev0",
    "0.60.0.post0",
    "0.60.0.post1",
    "0.60.1",
    "0.61.9",
    "0.61.10",
    "1.0.0",
    "1!0.0.1",
)

#: Each group is one version in every spelling PEP 440 accepts for it, so each must key
#: equal to the first. The first of each group is also in the order above, which ties the
#: spellings to a position rather than only to each other.
SAME_VERSION = (
    ("0.60.0", "0.60", "0.60.0.0", "v0.60.0", "V0.60.0", "0!0.60.0",
     " 0.60.0\n", "\t0.60.0\r\n"),
    ("0.60.0rc1", "0.60.0RC1", "0.60.0-rc1", "0.60.0_rc1", "0.60.0.rc.1", "0.60.0rc-1",
     "0.60.0c1", "0.60.0pre1", "0.60.0preview1"),
    ("0.60.0a0", "0.60.0a", "0.60.0alpha", "0.60.0alpha0", "0.60.0.a0"),
    ("0.60.0b1", "0.60.0beta1", "0.60.0-b1"),
    ("0.60.0.post1", "0.60.0post1", "0.60.0-post1", "0.60.0-1", "0.60.0rev1",
     "0.60.0.r1"),
    ("0.60.0.post0", "0.60.0.post", "0.60.0post"),
    ("0.60.0.dev0", "0.60.0.dev", "0.60.0dev", "0.60.0-dev0", "0.60.0_dev0"),
)

#: The first two are what `channel.build_label` appends to a build's own number, and
#: `update._parse` answered the release they label for both. The other two are local labels
#: charter does not write, where `_parse` read the label's digits as more version
#: (``(0, 61, 0, 1)`` and ``(0, 61, 1234)``): one rule for every label, not one per spelling.
LOCAL_LABELS = ("0.61.0+dev", "0.61.0+local", "0.61.0+dev.1", "0.61.0+abc1234")

#: Strings that are not versions. Each one is here for what `_parse` made of it:
#: ``build7`` → ``(7,)``, newer than every 0.x; ``0.60.0-CANARY`` → ``(0, 60, 0)``, the
#: release itself; the escape → ``(0, 60, 31)``; the whole ``--version`` label, commit and
#: all → ``(0, 61, 1234)``, because the digits of the commit became the patch number.
NOT_VERSIONS = (
    "",
    "not-a-version",
    "junk.junk",
    "unreleased",
    "latest",
    "build7",
    "0.*",
    ">=0.60.0",
    "0.60.0-CANARY",
    "0.60.0 rc1",
    "0.60.0\x1b[31m",
    "0.60.0\u2028",          # not the ASCII whitespace PEP 440 strips
    "\uff10.60.0",           # a digit, but not one of 0-9
    "0.60.0prev\u0131ew1",   # matches `preview` only under a Unicode case fold
    "0.61.0+\u212a",         # KELVIN SIGN, which folds to `k`
    "0.60.0+",
    "0.60.0+dev (main @ abc1234)",
    "0.61.0+dev (abc1234)",
    "0..60",
    ".60.0",
    "0.60.0.",
    "0.60.0rc1rc2",
    "0.60.0\n0.61.0",
)


class ThePep440Order(unittest.TestCase):
    def test_every_pair_orders_the_way_pep_440_does_in_both_directions(self):
        for lower, higher in itertools.combinations(PEP_440_ORDER, 2):
            with self.subTest(lower=lower, higher=higher):
                self.assertLess(update.version_key(lower), update.version_key(higher))
                self.assertGreater(update.version_key(higher), update.version_key(lower))
                self.assertNotEqual(update.version_key(lower), update.version_key(higher))

    def test_the_report_a_release_candidate_is_older_than_its_release(self):
        """The issue's line, on its own so the failure names it."""
        self.assertLess(update.version_key("0.60.0rc1"), update.version_key("0.60.0"))

    def test_every_spelling_of_one_version_is_that_version(self):
        for spellings in SAME_VERSION:
            canonical = spellings[0]
            self.assertIn(canonical, PEP_440_ORDER)
            for spelled in spellings[1:]:
                with self.subTest(canonical=canonical, spelled=spelled):
                    self.assertEqual(update.version_key(spelled),
                                     update.version_key(canonical))


class ALocalLabelDoesNotMoveAVersion(unittest.TestCase):
    """What `charter --version` prints on a git or checkout build, measured before #1050."""

    def test_a_labelled_build_is_the_release_it_carries_the_number_of(self):
        for labelled in LOCAL_LABELS:
            with self.subTest(labelled=labelled):
                self.assertEqual(update.version_key(labelled),
                                 update.version_key("0.61.0"))

    def test_so_it_is_still_older_than_the_next_release_and_newer_than_its_candidate(self):
        for labelled in LOCAL_LABELS:
            with self.subTest(labelled=labelled):
                self.assertLess(update.version_key(labelled), update.version_key("0.61.1"))
                self.assertLess(update.version_key(labelled),
                                update.version_key("0.61.0.post1"))
                self.assertGreater(update.version_key(labelled),
                                   update.version_key("0.61.0rc1"))


class AStringThatIsNotAVersionSortsBelowEveryVersion(unittest.TestCase):
    def test_it_is_older_than_the_oldest_version_there_is(self):
        """``0.dev0`` is the lowest version PEP 440 can spell. Below it is below all."""
        floor = update.version_key("0.dev0")
        for text in NOT_VERSIONS:
            with self.subTest(text=text):
                self.assertLess(update.version_key(text), floor)

    def test_and_below_every_version_in_the_order(self):
        for text in NOT_VERSIONS:
            for version in PEP_440_ORDER:
                with self.subTest(text=text, version=version):
                    self.assertLess(update.version_key(text), update.version_key(version))

    def test_two_of_them_are_not_ordered_against_each_other(self):
        """Equal, so no caller can find a direction between two strings that have none:
        `newer_than` claims nothing, and a sort keeps the filename order it had."""
        for a, b in itertools.combinations(NOT_VERSIONS, 2):
            with self.subTest(a=a, b=b):
                self.assertEqual(update.version_key(a), update.version_key(b))

    def test_none_is_not_a_version_either(self):
        """A cache or frontmatter field can be absent, and no caller should have to check."""
        self.assertEqual(update.version_key(None), update.version_key(""))
        self.assertLess(update.version_key(None), update.version_key("0.dev0"))


if __name__ == "__main__":
    unittest.main()
