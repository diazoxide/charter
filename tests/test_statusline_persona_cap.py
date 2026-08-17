"""The persona column is bounded, like the repo column already was.

Claude Code's status line has no scroll and no max height — it is stdout, one row per line
printed, and the docs say only "keep output short". So height is charter's to bound, and
charter bounded half of it: `_MAX_REPO_LINES` caps the repo tree, and nothing capped the
personas.

Because the layout pads the shorter column to match the longer, that made personas alone
drive the whole box. Measured before this change:

     personas  repos  lines
            3      2      8
           16      2     21
           30      2     35        <- whatever the repo count
            3     20     19        <- 20 repos still fits: the repo cap works

A status line taller than the conversation is not a status line.

**Which chips survive matters more than how many.** Keeping the first N alphabetically would
drop exactly the personas worth seeing, so the order is what a truncated column is FOR: the
active persona, then anything carrying a health mark — `_health_mark` speaks only when
something is wrong — then the rest.
"""

from __future__ import annotations

import unittest

from charter import config, persona, statusline
from tests._isolation import PersonaIso


class ColumnCase(PersonaIso):
    def make_many(self, n: int, draft: str | None = None) -> None:
        for i in range(n):
            name = f"p{i:02d}"
            self.make_persona(name, role=f"Role {i}", vault="none")
        if draft:
            p = persona.def_path(draft)
            p.write_text(p.read_text().replace("---\n", "---\ndraft: yes\n", 1))

    def chips(self):
        return statusline._persona_chips(None)


class TestTheColumnIsCapped(ColumnCase):
    def test_a_small_roster_is_untouched(self):
        """The cap must not change what a normal plane shows — most planes are small, and a
        truncation notice on them would be noise."""
        self.make_many(4)
        self.assertEqual(len(self.chips()), 4)
        self.assertFalse(any("more" in c for c in self.chips()))

    def test_a_large_roster_is_bounded(self):
        self.make_many(30)
        self.assertLessEqual(len(self.chips()), statusline._MAX_PERSONA_LINES)

    def test_it_matches_the_repo_budget(self):
        """One number for one question — how tall may a column get — rather than two that
        drift apart."""
        self.assertEqual(statusline._MAX_PERSONA_LINES, statusline._MAX_REPO_LINES)


class TestItSaysWhatItHid(ColumnCase):
    def test_the_hidden_count_is_shown(self):
        """The contract `_repo_rows` already keeps with its own "(+N more)": a column that
        silently drops rows reads as a complete roster."""
        self.make_many(30)
        tail = self.chips()[-1]
        self.assertIn("more", tail)

    def test_the_count_is_correct(self):
        self.make_many(30)
        chips = self.chips()
        shown = len(chips) - 1                       # the notice row is not a persona
        self.assertIn(f"+{30 - shown} more", chips[-1])

    def test_it_names_where_to_see_the_rest(self):
        self.make_many(30)
        self.assertIn("charter persona list", self.chips()[-1])


class TestTheRightChipsSurvive(ColumnCase):
    def test_the_active_persona_is_kept(self):
        self.make_many(30)
        persona.set_active("p29")                    # last alphabetically
        self.assertTrue(any("p29" in c for c in self.chips()))

    def test_an_unhealthy_persona_is_kept_over_a_healthy_one(self):
        """A truncated column exists to show what is wrong. `p29` would fall off the end of
        an alphabetical cut, and it is precisely the one worth seeing."""
        self.make_many(30, draft="p29")
        self.assertTrue(any("p29" in c for c in self.chips()))

    def test_a_healthy_persona_may_be_dropped(self):
        """The other half of the same statement — something has to go."""
        self.make_many(30)
        chips = self.chips()
        names = {f"p{i:02d}" for i in range(30)}
        shown = {n for n in names if any(n in c for c in chips)}
        self.assertLess(len(shown), len(names))


class TestItNeverBreaksTheRender(ColumnCase):
    def test_a_broken_roster_still_returns_a_list(self):
        """`_persona_chips` runs on every turn and its own contract is to degrade to an
        empty column rather than take the footer down."""
        real = persona.list_personas
        persona.list_personas = lambda: (_ for _ in ()).throw(OSError("nope"))
        self.addCleanup(setattr, persona, "list_personas", real)
        self.assertEqual(self.chips(), [])


if __name__ == "__main__":
    unittest.main()
