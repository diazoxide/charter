"""The README has to say what each harness cannot do, and what to do about it.

Twice in two releases the code shipped and the front door did not. 0.40.0 registered three
harnesses while the README still called charter a tool for Claude Code agents; the next
change gave every ceiling a remedy and the README went on saying "no status bar" with no
hint that anything answered it — which is precisely the "nothing can be done" reading that
change existed to prevent.

Both misses have the same shape: the ADR gets updated because that is where the reasoning
happens, and the README does not because by then nobody is thinking about the reader. This
is `test_asset_freshness`'s trick applied to prose — the repo already fails a build when a
screenshot drifts from what charter prints, and a capability the front door never mentions
is the same drift in a cheaper medium.

**Deliberately only `README.md`.** It is the file PyPI renders and the first thing anyone
evaluating charter reads. Extending this to every doc would make it a chore that gets
suppressed rather than a check that gets heeded.
"""

from __future__ import annotations

import unittest
from pathlib import Path

from charter.harness import registry

README = Path(__file__).resolve().parents[1] / "README.md"


def undocumented(prose: str, required: dict[str, str]) -> list[str]:
    """Which of *required* (``label -> text``) is absent from *prose*.

    A plain substring test, on purpose: anything cleverer (fuzzy matching, stripping
    punctuation) would start passing on prose that merely resembles the claim, and a check
    that can be satisfied by accident is worse than none.
    """
    return [f"{label}: {text!r}" for label, text in required.items() if text not in prose]


class TheCheckerCatchesWhatItIsFor(unittest.TestCase):
    """The checker is tested before it is trusted — a green assertion that cannot go red
    proves nothing, which is the lesson the opencode shim taught this session."""

    def test_a_missing_claim_is_reported(self):
        missing = undocumented("charter runs anywhere.",
                               {"remedy": "charter statusline --watch"})
        self.assertEqual(len(missing), 1)
        self.assertIn("charter statusline --watch", missing[0])

    def test_a_present_claim_is_not(self):
        self.assertEqual(
            undocumented("Run `charter statusline --watch` in a spare terminal.",
                         {"remedy": "charter statusline --watch"}), [])


class TheReadmeSaysIt(unittest.TestCase):
    def setUp(self) -> None:
        self.prose = README.read_text()

    def test_every_registered_harness_is_named(self):
        """0.40.0's miss. A harness charter supports and the README never mentions is one
        nobody discovers."""
        missing = undocumented(self.prose,
                               {h.name: h.name for h in registry.all()})
        self.assertEqual(missing, [], f"README does not name: {missing}")

    def test_every_remedy_charter_offers_is_documented(self):
        """The second miss. A ceiling with an answer that only `doctor` knows about leaves
        the reader believing the limit is absolute."""
        required = {f"{h.name}/{d.key}": d.remedy
                    for h in registry.all() for d in h.deficits if d.remedy}
        self.assertTrue(required, "no remedies exist — this test would pass vacuously")
        missing = undocumented(self.prose, required)
        self.assertEqual(missing, [],
                         "README states a ceiling without its remedy — add it to the "
                         f"harness table's last column: {missing}")


if __name__ == "__main__":
    unittest.main()
