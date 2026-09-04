"""`some-workspace (5)` — the chat count on a workspace tab, and the two constraints
that decide its shape (#880).

**Both constraints are measurements, and both are about a cell an operator is about to
press.**

*The field is fixed-width and always reserved.* Tab widths decide where `slots._cuts`
falls, so a suffix that appeared when a chat opened and vanished when one closed would move
every tab to its right — and the cell the operator was aiming at would hold another
workspace's name a moment later. charter refused exactly this shape once already, for the
tab spinner: *"It takes the mark's cell rather than adding one… A spinner drawn beside a
name would re-cut the strip the moment a sibling started thinking."* The spinner had a cell
to take; a count has none, so it buys one on every workspace tab and pays for it whether or
not it has a number to put there. `TheFieldIsReservedWhetherOrNotItIsDrawn` is that,
asserted as the property it protects rather than as the width it happens to be.

*One grouped pass.* `chats.of_workspace` is an `os.scandir` plus two small reads per chat
and is uncached by design; asked once per workspace it is roughly 15 x 30 x 2 = 900 reads
for one repaint of one row. `chats.counts_by_workspace` is the same walk done once, and
`OneScanAnswersForEveryWorkspace` counts the scans rather than trusting the docstring.

**And it does not put the strip on a clock.** `slots.BAR_ANIMATED` is the chats strip and
only the chats strip — that one draws a spinner, which changes with nothing but time. A
count changes when a chat opens or closes, which is a thing that happens to the plane, so
this rides the repaint the strip already had.
"""

from __future__ import annotations

import os
import unittest
from unittest import mock

from charter import config, tui
from charter.frame import chats, slots, state

from tests._isolation import PersonaIso
from tests.test_frame_chat_switch import _plant

#: Three workspaces, so a strip has a middle tab whose columns a leading tab's field can
#: push — which is what the reserve exists to stop.
NAMES = ["alpha", "beta", "gamma"]


def _plain(rows):
    """The strip's first row with no SGR, or ``""`` — `tui.strip_ansi`, which is what
    `panel._write` runs for a plane under `NO_COLOR`."""
    return tui.strip_ansi(rows[0]) if rows else ""


class TheCountIsDrawnBesideTheName(unittest.TestCase):
    """`slots._bar` with a counts map and no plane under it."""

    def setUp(self):
        slots.TABS.forget()
        self.addCleanup(slots.TABS.forget)

    def _row(self, counts, here="beta", width=120, names=None):
        return _plain(slots._bar(list(names or NAMES), here, width, counts=counts))

    def test_a_workspace_with_chats_says_how_many(self):
        self.assertIn("alpha (5)", self._row({"alpha": 5}))

    def test_a_workspace_with_none_draws_a_blank_and_never_a_zero(self):
        """Zero renders blank, so every count on screen means something. A `(0)` beside
        twelve of fifteen names is a column of noise saying only that the feature is on."""
        row = self._row({"alpha": 5})
        self.assertNotIn("(0)", row)
        self.assertNotIn("beta (", row)

    def test_a_name_the_map_does_not_carry_counts_zero(self):
        """`chats.counts_by_workspace` answers about the chats on disk and the strip draws
        the workspaces on disk, and those are not the same list — a workspace directory
        with no chat in it is in the second and not the first. The strip reads the map with
        a default rather than trusting its keys."""
        self.assertNotIn("(", self._row({}))

    def test_a_count_past_the_ceiling_stops_being_a_number(self):
        """`(99+)` rather than `(100)`, and it is the same width — which is the point:
        a field that grew a digit at a hundred chats would move every tab right of it."""
        row = self._row({"alpha": slots.TAB_COUNT_MAX + 1})
        self.assertIn(f"alpha ({slots.TAB_COUNT_MAX}+)", row)

    def test_the_ceiling_itself_is_still_drawn_as_a_number(self):
        """`n <= TAB_COUNT_MAX` and not `n <`: the ceiling is the last count drawn in full,
        so `(99)` is a number and `(100)` is the overflow. One cell apart on screen and one
        boundary apart in the code, which is exactly the shift `tools/sweep.py` asks about
        and which the case above — asked at `MAX + 1` — cannot see."""
        row = self._row({"alpha": slots.TAB_COUNT_MAX})
        self.assertIn(f"alpha ({slots.TAB_COUNT_MAX})", row)
        self.assertNotIn("+", row, "the ceiling itself was drawn as an overflow")

    def test_the_chats_strip_reserves_nothing_at_all(self):
        """``None`` and ``{}`` are different instructions: no field, against a field with
        nothing in it. A chat is the leaf and has nothing to count, so a strip of chats
        that reserved six columns a name would be the widest thing on the row spent on
        nothing."""
        without = self._row(None)
        blank = self._row({})
        self.assertNotEqual(tui.width(without), tui.width(blank))
        self.assertEqual(tui.width(blank) - tui.width(without),
                         len(NAMES) * slots.TAB_COUNT_W)


class TheFieldIsReservedWhetherOrNotItIsDrawn(unittest.TestCase):
    """**The constraint the whole design is built around**, asserted as the property it
    buys rather than as the constant it is spelled with.

    A chat opening or closing must move NO column of the strip: not the names, not the
    counts, not the cut, and not the click map. That is `slots.TAB_SPINNER`'s rule said
    about a field that has no cell of its own — it takes six, on every tab, always.
    """

    #: Every count a caller can reach, including both sides of the ceiling and a plane
    #: whose map is empty. The width is one number for all of them or the reserve is not a
    #: reserve.
    MAPS = ({}, {"alpha": 1}, {"alpha": 9}, {"alpha": 10}, {"alpha": 99},
            {"alpha": 100}, {"alpha": 4000},
            {"alpha": 3, "beta": 12, "gamma": 99})

    def setUp(self):
        slots.TABS.forget()
        self.addCleanup(slots.TABS.forget)

    def test_no_map_at_any_width_moves_a_single_column_of_the_row(self):
        """Every width from 0 to 200, every map above, and the whole click map compared —
        not just the rendered text. A strip that drew the same row and published a
        different map would switch to the neighbour of the tab that was pressed."""
        for width in range(0, 201):
            shape = None
            for counts in self.MAPS:
                slots.TABS.forget()
                rows = slots._bar(list(NAMES), "beta", width, counts=counts)
                drawn = [tui.width(tui.strip_ansi(r)) for r in rows]
                cells = {(r, c): slots.TABS.switch_to(r, c)
                         for r in range(len(rows) or 1) for c in range(width)}
                if shape is None:
                    shape = (drawn, cells)
                    continue
                self.assertEqual(
                    (drawn, cells), shape,
                    f"{width} columns: {counts} draws a different strip from "
                    f"{self.MAPS[0]} — a chat opening re-cut the row")

    def test_a_count_belongs_to_the_tab_it_is_the_count_of(self):
        """It is drawn against that name, there is nothing else it could be a click on,
        and `slots._tab_columns` gives it those cells because the field it walks IS the
        name plus the count."""
        rows = slots._bar(list(NAMES), "beta", 120, counts={"alpha": 5})
        row = tui.strip_ansi(rows[0])
        at = row.index("(5)")
        for col in range(at, at + len("(5)")):
            self.assertEqual(slots.TABS.switch_to(0, col), "alpha",
                             f"column {col} of {row!r}")

    def test_the_reserve_is_six_columns_a_tab_and_that_is_the_price(self):
        """**Spelled, because it is what this feature costs and the cost is the argument
        against it.** Six columns on every workspace tab is why these fifteen names need
        352 columns for one row rather than 262, and every other case here reads
        `TAB_COUNT_W` on both sides — so a wider ceiling would spend more of the row with
        nothing going red. A change to `TAB_COUNT_MAX` has to come back to this line and
        restate the price, which is `layout.BAR_MAX_ROWS`' own discipline one constant
        over."""
        self.assertEqual(slots.TAB_COUNT_W, 6)
        self.assertEqual(slots.TAB_COUNT_MAX, 99)

    def test_the_reserve_is_derived_from_the_widest_thing_it_can_hold(self):
        """A `6` written down beside a ceiling of 99 is two numbers that can drift apart.
        Asked of the function rather than of the constant, at both ends and past them."""
        for n in (0, 1, 9, 10, 99, 100, 10_000):
            self.assertEqual(tui.width(slots._tab_count(n)), slots.TAB_COUNT_W, n)


class TheSizerAndTheRendererMeasureOneStrip(PersonaIso, unittest.TestCase):
    """#500's shape one slot over: the launcher sizes the pane and the renderer spends it,
    so a want the renderer does not fill is blank rows off the harness.

    The count field is where that could have gone wrong quietly — a sizer that composed
    without the counts would plan a strip six cells a name narrower than the one that gets
    drawn.
    """

    def test_the_want_is_the_rows_the_strip_then_fills(self):
        for name in NAMES:
            (config.WORKSPACES_DIR / name).mkdir(parents=True, exist_ok=True)
        for i in range(1, 8):
            _plant(f"alpha.{i}", workspace="alpha")
        for cols in range(0, 200, 3):
            with self.subTest(cols=cols):
                want = slots.bar_rows_wanted("alpha.1", "workspaces",
                                             pane_cols=cols, cap=3)
                slots.TABS.forget()
                lines = slots.workspaces_bar("alpha.1", cols, want)
                if lines:
                    self.assertEqual(len(lines), want,
                                     f"{cols} columns asked for {want} rows and filled "
                                     f"{len(lines)}")

    def test_the_strip_draws_the_count_the_plane_actually_has(self):
        """End to end through the real entry, so the map the renderer reads is the one
        `chats.counts_by_workspace` answers and not a fixture's."""
        for name in NAMES:
            (config.WORKSPACES_DIR / name).mkdir(parents=True, exist_ok=True)
        for i in range(1, 4):
            _plant(f"alpha.{i}", workspace="alpha")
        _plant("gamma.1", workspace="gamma")
        row = _plain(slots.workspaces_bar("alpha.1", 200))
        self.assertIn("alpha (3)", row)
        self.assertIn("gamma (1)", row)
        self.assertNotIn("beta (", row)

    def test_a_plane_it_cannot_read_costs_the_numbers_and_not_the_pane(self):
        """`_workspace_counts` is guarded for `slots.working_chats`' reason and this
        module's own rule: a readout must never cost a pane. What is left is the strip that
        shipped before this feature, with the field still reserved."""
        with mock.patch.object(chats, "counts_by_workspace",
                               side_effect=OSError("no plane")):
            self.assertEqual(slots._workspace_counts(), {})


class OneScanAnswersForEveryWorkspace(PersonaIso, unittest.TestCase):
    """The second constraint, counted rather than argued."""

    def setUp(self):
        super().setUp()
        for i in range(1, 4):
            _plant(f"alpha.{i}", workspace="alpha")
        _plant("beta.1", workspace="beta")

    def test_every_workspaces_count_costs_one_scandir(self):
        """**Not one per name**, which is what the strip would have paid asking
        `of_workspace` fifteen times. Counted with a real spy on `os.scandir`, because the
        cost is the syscall and not the function that wraps it."""
        real = os.scandir
        calls = []

        def spy(path):
            calls.append(str(path))
            return real(path)

        with mock.patch.object(chats.os, "scandir", spy):
            counts = chats.counts_by_workspace()
        self.assertEqual(counts, {"alpha": 3, "beta": 1})
        self.assertEqual(len(calls), 1, calls)

    def test_the_counts_agree_with_the_roster_name_by_name(self):
        """One membership rule, asked once. A count that disagreed with the list a click
        on that tab reaches would be worse than no count at all."""
        for name in ("alpha", "beta", "gamma"):
            self.assertEqual(chats.counts_by_workspace().get(name, 0),
                             len(chats.of_workspace(name)), name)

    def test_a_chat_charter_cannot_place_is_counted_nowhere(self):
        """It is in no workspace's roster either, so counting it somewhere would be a
        number no tab on the strip could be pressed to see."""
        os.makedirs(state._root() / "orphan.1", exist_ok=True)
        counts = chats.counts_by_workspace()
        self.assertEqual(counts, {"alpha": 3, "beta": 1})
        self.assertTrue(all(isinstance(k, str) for k in counts))

    def test_an_unreadable_frame_root_is_no_counts_and_no_raise(self):
        with mock.patch.object(chats.os, "scandir", side_effect=OSError("gone")):
            self.assertEqual(chats.counts_by_workspace(), {})
            self.assertEqual(chats.of_workspace("alpha"), [])


if __name__ == "__main__":
    unittest.main()
