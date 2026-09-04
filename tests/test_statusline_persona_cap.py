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

That is a statement about the CAP and not about the column, and #882 is what split the
two. The column reads in `persona.by_use` order — the plane's declared default, then
most-dispatched first — and no row's position depends on which persona the session is on;
the survival rule above still lifts the active persona, because "which rows fit" is a
question about identity that "what order do they read in" is not.
`tests/test_the_persona_switcher_sorts_by_use.py` owns the ordering half.
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


class TestTheFlatChipIsItsOwnPartsJoined(ColumnCase):
    """#516 split a chip into `PersonaChip(name, head, badges)` so `frame/slots.py` can
    give the badges a column of their own. The split is only safe while the two shapes
    are ONE builder: the moment `_persona_chips` composes anything of its own, a fix to a
    vault dot or a memory badge can land on one surface and not the other — which is the
    drift `_right`'s docstring says it delegates in order to avoid.

    Asserted as an identity over real personas rather than by reading the source, because
    the property is what the two functions RETURN, not how they are written.
    """

    def test_every_chip_is_exactly_its_head_and_its_badges(self):
        self.make_many(6)
        persona.set_active("p03")
        cells = statusline._persona_chip_cells(None)
        self.assertEqual(self.chips(), [c.head + c.badges for c in cells])
        self.assertTrue(cells, "the roster produced no cells to compare")

    def test_the_split_survives_the_truncation_notice(self):
        """The `…(+N more)` row is not a persona: it names none and carries no badges,
        and a caller drawing columns has to be able to tell. Its hidden COUNT rides with
        it as data, so a heading can add it back without parsing the sentence."""
        self.make_many(30)
        cells = statusline._persona_chip_cells(None)
        note = cells[-1]
        self.assertIsNone(note.name)
        self.assertEqual(note.badges, "")
        self.assertGreater(note.hidden, 0)
        named = sum(1 for c in cells if c.name is not None)
        self.assertEqual(named + note.hidden, 30)

    def test_the_vault_dot_travels_with_the_name_and_not_with_the_badges(self):
        """It is absent on almost every row, so in a badge column its width would be
        paid by every persona for a fact about one of them."""
        self.make_persona("vaulted", role="R", vault="nowhere-registered")
        cells = statusline._persona_chip_cells(None)
        cell = next(c for c in cells if c.name == "vaulted")
        dot = statusline._vault_dot("nowhere-registered").strip()
        self.assertTrue(dot, "the fixture produced no vault dot to place")
        self.assertIn(dot, cell.head)
        self.assertNotIn(dot, cell.badges)


class TestItNeverBreaksTheRender(ColumnCase):
    def test_a_broken_roster_still_returns_a_list(self):
        """`_persona_chips` runs on every turn and its own contract is to degrade to an
        empty column rather than take the footer down."""
        real = persona.list_personas
        persona.list_personas = lambda: (_ for _ in ()).throw(OSError("nope"))
        self.addCleanup(setattr, persona, "list_personas", real)
        self.assertEqual(self.chips(), [])

    def test_the_parts_degrade_the_same_way_the_flat_chips_do(self):
        """Both shapes answer empty, because both are the same function — a `_right`
        that got `None` here would need a guard its docstring says it does not have."""
        real = persona.list_personas
        persona.list_personas = lambda: (_ for _ in ()).throw(OSError("nope"))
        self.addCleanup(setattr, persona, "list_personas", real)
        self.assertEqual(statusline._persona_chip_cells(None), [])


if __name__ == "__main__":
    unittest.main()
