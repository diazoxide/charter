"""The handoff decisions are written down where a reader without the plan can find them.

A chat handoff was settled in a grill and built over six tasks, and the reasons lived in a
workspace file that ships with nobody. This is the tripwire for the three places that carry
them into the repo: `CONTEXT.md` for the words, `docs/adr/0021-*` for the consent decision,
and `docs/handoff.md` for the page `charter docs show handoff` serves.

The ADR is pinned **by number**, which the harness-profile plan left free by taking 0022
(`docs/superpowers/plans/2026-09-11-harness-profiles.md`, *Controller rulings* 2, and
`tests/test_the_profile_words_are_written_down.py`, which pins its own by slug so the two
could merge in either order).

These read files off the tree, never through `config`: a test that resolved the plane would
be answering about whatever plane it ran in.
"""

from __future__ import annotations

import unittest
from pathlib import Path

from charter import dispatch

ROOT = Path(__file__).resolve().parents[1]
ADR = ROOT / "docs" / "adr"


def _entry(text: str, term: str) -> str:
    """The body of one `**Term**:` entry in `CONTEXT.md`, up to the next bold term.

    Sliced rather than line-counted because an entry's definition runs to as many lines as
    it needs, and `_Avoid_` is the last of them.
    """
    start = text.index(f"**{term}**:")
    rest = text[start + len(term) + 5:]
    end = rest.find("\n**")
    return rest if end < 0 else rest[:end]


class TestTheWordsAreDefined(unittest.TestCase):
    def test_context_defines_chat_handoff_and_brief(self):
        """The spec's Language section names three words and three things to stop calling
        them. A glossary that carries the terms without the refusals leaves `spawn` and
        `sub-session` available to the next writer, which is how they got in."""
        text = (ROOT / "CONTEXT.md").read_text()
        for term in ("Chat", "Handoff", "Brief"):
            self.assertIn(f"**{term}**:", text, f"CONTEXT.md defines no {term!r}")
            body = _entry(text, term)
            self.assertRegex(body, r"(?m)^_Avoid_:",
                             f"{term!r} names nothing it is not")

    def test_the_glossary_refuses_the_words_the_spec_ruled_out(self):
        """`sub-session` is the one all three share: a handed-off chat is not a child of
        anything, and a word that implies it is a word that invites a report-back channel
        this feature deliberately does not have."""
        text = (ROOT / "CONTEXT.md").read_text()
        for term, words in (("Chat", ("session", "spawn", "sub-session")),
                            ("Handoff", ("spawn", "delegation", "sub-session")),
                            ("Brief", ("prompt", "context", "instructions"))):
            body = _entry(text, term).lower()
            avoid = body[body.index("_avoid_:"):]
            for word in words:
                with self.subTest(term=term, word=word):
                    self.assertIn(word, avoid, f"{term!r} still permits {word!r}")


class TestTheDecisionsAreRecorded(unittest.TestCase):
    def test_the_consent_adr_takes_the_next_number(self):
        found = sorted(ADR.glob("0021-*.md"))
        self.assertEqual(1, len(found),
                         f"expected one ADR 0021, found {[p.name for p in found]}")
        self.assertTrue(found[0].read_text().startswith("# "),
                        f"{found[0].name} does not open with its claim as a title")

    def _adr(self) -> str:
        return next(ADR.glob("0021-*.md")).read_text()

    def test_the_consent_adr_says_the_prompt_is_not_a_boundary(self):
        """The load-bearing sentence of the whole gate. A reader who takes the ask rule for
        a boundary will delete A7 as redundant, and the spellings it refuses ran with no
        prompt at all on the version this was measured on."""
        text = self._adr()
        self.assertIn("security boundary", text)
        self.assertIn("2.1.268", text, "the boundary claim names no measured version")

    def test_the_consent_adr_states_what_each_decision_cost(self):
        """A decision record without its costs is a list of what was built, which the code
        already says. These are the four that were paid knowingly."""
        text = self._adr()
        for claim in ("python3 -m charter handoff",   # the spelling CONTRIBUTING uses
                      "${EDITOR}",                    # an unnameable opener errs toward refusing
                      "os.urandom",                   # the fence marker is not reproducible
                      "0.750"):                       # the boilerplate scored as agreement
            with self.subTest(claim=claim):
                self.assertIn(claim, text, f"ADR 0021 states no cost about {claim!r}")

    def test_the_consent_adr_names_the_harnesses_it_cannot_gate(self):
        """Codex has no prompt in front of a handoff and opencode cannot refuse a sub-agent's
        one. Both are ceilings charter reports rather than closes, so both belong in the
        record that says what the gate is."""
        text = self._adr()
        for harness in ("Codex", "opencode"):
            self.assertIn(harness, text, f"ADR 0021 says nothing about {harness}")


class TestThePageUsesChartersWords(unittest.TestCase):
    def test_the_handoff_page_uses_charters_words(self):
        """`spawn` and `sub-session` are what the glossary was written to stop. The page a
        chat is pointed at is the one most likely to teach them back."""
        text = (ROOT / "docs" / "handoff.md").read_text().lower()
        for word in ("spawn", "sub-session"):
            self.assertNotIn(word, text, f"docs/handoff.md still says {word!r}")

    def test_the_handoff_page_points_at_the_decision_record(self):
        """The page says what happens; the ADR says why it cannot be undone cheaply. A page
        that never names it is a page whose reasons decay on their own."""
        text = (ROOT / "docs" / "handoff.md").read_text()
        self.assertIn("adr/0021-", text,
                      "docs/handoff.md links no consent ADR")


class TestTheAdvicePairIsNamedForRouting(unittest.TestCase):
    def test_the_advice_pair_is_named_for_routing(self):
        """`handoffs_since_first_advice` counted DISPATCHES, and "handoff" now means opening
        a chat. Both halves are asserted: a rename that leaves the old name behind leaves the
        contradiction it was for."""
        self.assertTrue(hasattr(dispatch, "routed_since_first_advice"))
        self.assertFalse(hasattr(dispatch, "handoffs_since_first_advice"),
                         "the old name is still readable, so the word still means two things")


if __name__ == "__main__":
    unittest.main()
