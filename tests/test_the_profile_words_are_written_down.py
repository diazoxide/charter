"""The profile decisions are written down where a reader without the plan can find them.

Harness profiles were settled in a grill and then built over seven tasks, and the reasons
lived in a workspace file that ships with nobody. This is the tripwire for the three places
that carry them into the repo: `CONTEXT.md` for the words, `docs/adr/` for the decisions,
and the phase-5 spec's own credentials sentence, which profiles made false.

The ADR is pinned **by slug, not by number**. The chat-handoff plan claims 0021 and pins
that number in its own test, so whichever of the two merges first must not break the other
(`docs/superpowers/plans/2026-09-11-harness-profiles.md`, *Controller rulings* 2).

These read files off the tree, never through `config`: a test that resolved the plane would
be answering about whatever plane it ran in.
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

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
    def test_context_defines_the_three_terms(self):
        """A word charter refuses on (`account`, `alias`) has to be in the glossary that
        says why, or the next writer reaches for it again."""
        text = (ROOT / "CONTEXT.md").read_text()
        for term in ("Harness profile", "Kind", "Profile selector"):
            self.assertIn(f"**{term}**:", text, f"CONTEXT.md defines no {term!r}")
            body = _entry(text, term)
            self.assertRegex(body, r"(?m)^_Avoid_:",
                             f"{term!r} names nothing it is not")

    def test_the_profile_glossary_refuses_alias_and_account(self):
        """The two words the spec's Language section rules out by name — a shell alias
        charter never sees, and an account a profile need not change."""
        body = _entry((ROOT / "CONTEXT.md").read_text(), "Harness profile").lower()
        avoid = body[body.index("_avoid_:"):]
        for word in ("alias", "account"):
            self.assertIn(word, avoid, f"'harness profile' still permits {word!r}")


class TestTheDecisionsAreRecorded(unittest.TestCase):
    def test_the_profile_adr_exists_once(self):
        """Pinned by slug. Two plans were numbering ADRs at the same time, and the number
        is whichever one merged first."""
        found = sorted(ADR.glob("*-a-harness-profile-belongs-to-one-machine.md"))
        self.assertEqual(1, len(found),
                         f"expected one profile ADR, found {[p.name for p in found]}")
        self.assertTrue(found[0].read_text().startswith("# "),
                        f"{found[0].name} does not open with its claim as a title")

    def test_the_profile_adr_states_its_limits(self):
        """ADR 0022's costs are the half a decision record is usually missing. Three of
        them are the reason the guard rails are guard rails: the launch record and the
        wiring cache sit under `.charter/`, a wrapper script can still carry a key, and
        the proof was one account in two folders."""
        text = next(ADR.glob("*-a-harness-profile-belongs-to-one-machine.md")).read_text()
        for claim in (".charter/", "wrapper script", "guard rail"):
            self.assertIn(claim, text, f"the profile ADR states no limit about {claim!r}")

    def test_adr_0017_is_amended_for_the_local_file(self):
        """0017 ignores a path charter creates that carries credentials. This one carries
        none and is ignored anyway, so the amendment has to say why."""
        text = (ADR / "0017-charter-ignores-what-carries-credentials.md").read_text()
        self.assertIn("charter.local.toml", text)

    def test_adr_0018_is_amended_for_a_pane_before_its_harness(self):
        """Written in Task 2's own pull request, not here (ruling 44): the launcher that
        holds a refusal in a chat's pane shipped with it. This pins the sentence the whole
        amendment turns on, which the profile selector rests on too."""
        text = (ADR / "0018-charter-may-run-the-harness-but-never-draws-it.md").read_text()
        self.assertIn("no harness has ever run", text)

    def test_adr_0018_bounds_are_the_ones_the_launcher_keeps(self):
        """The records task read that amendment against `charter/frame/launcher.py` and
        found its bounds narrower than the code in two places and looser in two more. The
        code was right and the record was corrected, so these pin the corrections:

        * `framed_chat()` prints a line that is **not** a refusal and lets the launch go on;
        * the wait needs a terminal as well as an attended open (`sys.stdin.isatty()`);
        * `state.record_launch` runs on every refusal — only the wait is conditional.

        Asked of the words rather than of the code because the code already behaves this
        way; what drifted, and what would drift again, is the sentence describing it.
        """
        text = (ADR / "0018-charter-may-run-the-harness-but-never-draws-it.md").read_text()
        for claim in ("unproven", "not a refusal", "isatty", "state.record_launch"):
            self.assertIn(claim, text,
                          f"0018's amendment no longer accounts for {claim!r}")

    def test_the_phase5_credentials_line_carries_its_supersession(self):
        """That spec is the record of what was decided in August, so the sentence stays
        and is marked rather than rewritten."""
        spec = (ROOT / "docs" / "superpowers" / "specs"
                / "2026-08-28-phase5-workspace-and-chat-tabs.md").read_text()
        self.assertIn("Superseded 2026-09-11 by harness profiles", spec)
        line = next(l for l in spec.splitlines()
                    if "Superseded 2026-09-11 by harness profiles" in l)
        self.assertIn("2026-09-11-harness-profiles.md", line + spec,
                      "the supersession names no spec to read instead")


class TestTheNewsEntryKeepsItsShape(unittest.TestCase):
    """CONTRIBUTING's frontmatter rules, asked of the one entry this feature ships.

    `test_news_gate` already asks them of every entry in the tree; this asks them by
    *slug*, because five tasks each append to this file and the release stamp renames it.
    """

    #: `test_news_gate.test_no_test_opens_an_entry_by_its_staged_name` refuses a test that
    #: hard-codes `unreleased-<slug>.md`, so the entry is found by what it is ABOUT instead.
    #:
    #: **By the term and not by the headline**, which is the correction Task 3 made: this
    #: looked for "harness profiles from charter.local.toml", a phrase out of the headline
    #: Task 1 wrote — and the plan has Task 3 replace that headline, so the locator went
    #: looking for words its own feature had moved on from and the class failed as "no news
    #: entry announces harness profiles". The term this entry exists to introduce outlives
    #: every task's rewrite of the sentence around it.
    def _entry_text(self) -> str:
        for path in sorted((ROOT / "docs" / "news").glob("*.md")):
            text = path.read_text()
            if "harness profile" in text:
                return text
        self.fail("no news entry announces harness profiles")

    def test_the_frontmatter_is_flat_and_unquoted(self):
        body = self._entry_text()
        block = re.match(r"---\n(.*?)\n---\n", body, re.S)
        self.assertIsNotNone(block, "the entry opens with no frontmatter block")
        keys = {}
        for line in block.group(1).splitlines():
            key, _, value = line.partition(":")
            keys[key] = value.strip()
            self.assertIn(":", line, f"frontmatter line is not key: value — {line!r}")
            self.assertNotRegex(value.strip(), r"""^['"].*['"]$""",
                                f"{key}: is quoted, and charter unquotes nothing")
        self.assertLessEqual(set(keys), {"version", "headline", "check", "adopt",
                                         "lead", "security"},
                             "a key charter does not read renders as nothing at all")
        self.assertEqual("unreleased", keys.get("version"))
        self.assertEqual("reinit", keys.get("adopt"),
                         "the ignore line is what a plane adopts")


if __name__ == "__main__":
    unittest.main()
