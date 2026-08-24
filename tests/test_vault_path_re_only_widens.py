"""`_VAULT_PATH_RE` must never deny less than the version before it.

This branch edits a security regex, which is the change most able to make a guard weaker
while looking like it makes it stronger — one badly placed alternation, one group that
turns a required trailing slash into an optional one, and paths that used to be denied
stop being denied, silently, with no test in the suite complaining because every test
asserts what the NEW pattern catches.

So the property is differential:

    for a corpus of operands, everything the previous pattern denied is still denied

`BASELINE` is the pattern as it stood on `origin/main` before the `fingerprint`
alternative was added, kept here as a fixture rather than read from git — a test that
shells out to `git show` is a test that stops running in a tarball, in a shallow clone,
and in CI on a detached tree. It is frozen deliberately: it is a record of a past
guarantee, not a copy of the current source, and it must NOT be updated when the live
pattern changes. Updating it is exactly how a differential test becomes a tautology.

**The corpus is the weak part, and saying so is the point.** A differential test can only
compare the two patterns on inputs somebody wrote down, so it proves "not weaker on these"
and never "not weaker". What it does bound is the regression that actually happens:
someone edits the pattern for one new case and quietly loses an old one. The unbounded
question — which spellings neither pattern catches — is the tool gate's job, which asks
the filesystem rather than the string; `test_plane_reads_are_contained.py` carries that
half, and `hooks.py` names the limit above the pattern.
"""

from __future__ import annotations

import re
import unittest

from charter.hooks import _VAULT_PATH_RE

#: `origin/main`'s pattern at the point this branch forked. Frozen; never update it.
BASELINE = re.compile(r"\.(?:charter|edm)(?:/(?:vaults/|browser|active-)|/?$)")

#: Operands to compare the two patterns on. Everything the baseline matched here must
#: still match; the new alternative adds matches and is allowed to.
CORPUS = (
    ".charter/vaults/devops.json",
    ".charter/vaults/team/prod.json",
    "./.charter/vaults/devops.json",
    "/Users/x/proj/.charter/vaults/devops.json",
    ".edm/vaults/devops.json",
    ".charter/browser",
    ".charter/browser/profiles",
    ".edm/browser",
    ".charter/active-persona",
    ".edm/active-persona",
    ".charter",
    ".charter/",
    ".edm",
    ".edm/",
    "~/.charter",
    "/Users/x/proj/.charter/",
    # Not matched by either, and listed so a change that starts matching them shows up as
    # a deliberate widening rather than as noise: the registry and the state subtree are
    # ordinary reads, and denying them was a real regression once (#443's other half).
    ".charter/vaults.json",
    ".charter/state/session.json",
    "charter/secrets/base.py",
    "docs/secrets.md",
    # The new alternative. Denied now, not before — the one direction that is allowed.
    ".charter/fingerprint.key",
    ".edm/fingerprint.key",
)


class TheGuardOnlyWidens(unittest.TestCase):

    def test_nothing_the_previous_pattern_denied_is_allowed_now(self) -> None:
        lost = [s for s in CORPUS if BASELINE.search(s) and not _VAULT_PATH_RE.search(s)]
        self.assertEqual(
            lost, [],
            "this branch's pattern denies LESS than origin/main's on these operands. A "
            "security branch that is weaker than the branch it merges into must not merge.")

    def test_the_baseline_is_not_a_copy_of_the_live_pattern(self) -> None:
        """Otherwise the comparison above is `x == x` and holds no matter what happens.

        The two patterns are supposed to differ by exactly the alternative this branch
        added. If they ever become identical, either the branch's change was reverted or
        somebody 'fixed' this fixture by pasting the current source into it.
        """
        self.assertNotEqual(BASELINE.pattern, _VAULT_PATH_RE.pattern)
        gained = [s for s in CORPUS if _VAULT_PATH_RE.search(s) and not BASELINE.search(s)]
        self.assertIn(".charter/fingerprint.key", gained,
                      "the fingerprint key is the whole point of the change under test")

    def test_the_corpus_exercises_both_answers(self) -> None:
        """A corpus every pattern matches, or none matches, compares nothing."""
        self.assertTrue(any(BASELINE.search(s) for s in CORPUS))
        self.assertTrue(any(not _VAULT_PATH_RE.search(s) for s in CORPUS),
                        "no operand in the corpus is allowed, so a pattern that denied "
                        "literally everything would pass every assertion in this file")


if __name__ == "__main__":       # pragma: no cover
    unittest.main()
