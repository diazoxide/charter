"""Phase 5 Stage 5b, Task 7: the two bars — what they draw, how they give way, and why
neither is on every operator's frame.

**The bar is a READOUT and never the mechanism** (§3.6), and every decision here follows
from that one sentence. The palette reaches every chat and every workspace in two
keystrokes at every width — including the widths where neither bar can be drawn at all —
so a bar is allowed to degrade to a count and then to nothing, and `layout._DROP_ORDER`
gives both up before `top`.

**Neither is placed by default, and that is a decision with a measurement behind it
rather than an omission.** `frame/builtins.build` carries the argument; the short of it is
that a plane with one chat is the ordinary, permanent state (the same fact that keeps
`changes` a section rather than a pane), and each placed pane is ~7 of a switch's 41 tmux
invocations — measured at ~360 ms on tmux 3.7c and ~395 ms at the 3.2 floor. So the bars
ship as components a `[[frame.component]]` table can place, and
:class:`ABarIsPlaceableByConfig` is what says that route actually works end to end rather
than leaving two components nothing could ever ask for.
"""

from __future__ import annotations

import os
import unittest
from unittest import mock

from charter import config, contain, instance, tui
from charter.frame.component import Fixed
from charter.frame import builtins, layout, slots, state
from tests._isolation import PersonaIso
from tests.test_frame_chat_switch import _plant


class TheLadderGivesUpWholeThings(unittest.TestCase):
    """`slots._bar` alone, against a list of names and a width — no plane underneath it.

    Split out for `slots._fit_fields`' reason: the arithmetic is the thing that has to be
    right at three widths, and a test that needed a frame to ask it would be measuring the
    fixture as much as the ladder.
    """

    NAMES = ["api.1", "api.2", "api.3"]

    def _row(self, width, names=None, here="api.2", **kw):
        rows = slots._bar("chats", list(names or self.NAMES), here, width, **kw)
        self.assertLessEqual(len(rows), 1, "a bar is one row or it is none")
        return rows[0] if rows else ""

    def test_a_wide_row_shows_every_name_with_the_one_you_are_in_marked(self):
        row = self._row(200)
        for name in self.NAMES:
            self.assertIn(name, row)
        self.assertIn(f"{slots._BAR_MARK[0]}api.2", row)
        self.assertNotIn(f"{slots._BAR_MARK[0]}api.1", row)

    def test_the_two_marks_are_the_same_width_so_the_names_do_not_shift(self):
        """A mark that moved the names beside it would make the row jump every time the
        operator switched — `overlay._MARK`'s own rule, and why both entries are ASCII."""
        self.assertEqual(tui.width(slots._BAR_MARK[0]),
                         tui.width(slots._BAR_MARK[1]))
        widths = {tui.width(self._row(200, here=n)) for n in self.NAMES}
        self.assertEqual(len(widths), 1, "the row changed width when the mark moved")

    def test_a_row_with_no_room_for_every_name_draws_the_page_yours_is_on(self):
        """The names that FIT, not the one you are on and a count of everything else.

        The rung this replaces drew `*api.2  +2`, whose only tab is the one the operator
        is already standing on — correct, and inert. What the row can hold is drawn
        instead, and the rest is counted at whichever end it fell off.
        """
        row = self._row(30)
        self.assertIn("*api.2", row)
        self.assertIn("api.1", row, "the row had room for a neighbour and drew none")
        self.assertNotIn("api.3", row)
        self.assertIn("+1", row, "the name left off the row was not counted")

    def test_both_ends_are_counted_so_a_page_says_where_in_the_list_it_sits(self):
        """A single trailing `+N` beside a page that starts in the middle of the list
        claims the names on the row are the FIRST N — which a windowed strip makes false.

        Measured on a list long enough to have a page with names on both sides of it.
        """
        names = [f"workspace-{i:02d}" for i in range(15)]
        row = self._row(80, names=names, here="workspace-07")
        self.assertIn("*workspace-07", row)
        counts = [f.strip() for f in row.split(" " * slots._BAR_GAP)
                  if f.strip().startswith("+")]
        self.assertEqual(len(counts), 2, f"one end went uncounted: {row!r}")
        left, right = counts
        drawn = [n for n in names if n in row]
        self.assertEqual(int(left[1:]) + len(drawn) + int(right[1:]), len(names),
                         f"the two counts and the drawn names are not the list: {row!r}")
        self.assertEqual(names[int(left[1:])], drawn[0],
                         f"the leading count does not name where the page starts: {row!r}")

    def test_the_page_is_the_same_page_for_every_name_on_it(self):
        """**Why it is a page and not a window centred on the marked name.** The cut is a
        function of the names and the width alone, so switching to a tab that is ON the row
        redraws the identical row with only the `*` moved — which is what makes the column
        the operator pressed still mean the same thing an instant later.

        A centred window does not have this property, and the difference is not
        theoretical: measured on this project's own fifteen workspaces at 160 columns, six
        of the nine tabs a centred strip draws answer a second press at the identical
        column with a second, different workspace. `_Tabs.switch_to` cannot refuse that —
        the name at that column really did change — so a double-click would switch twice.
        """
        names = [f"workspace-{i:02d}" for i in range(15)]
        for width in (60, 80, 120, 160, 200):
            row = self._row(width, names=names, here="workspace-07")
            drawn = [n for n in names if n in row]
            self.assertIn("workspace-07", drawn, f"{width}: nothing was drawn")
            for name in drawn:
                again = self._row(width, names=names, here=name)
                self.assertEqual([n for n in names if n in again], drawn,
                                 f"{width}: switching to {name} moved the page:\n"
                                 f"  {row!r}\n  {again!r}")

    def test_a_row_with_no_room_for_a_name_says_where_you_are_and_how_many(self):
        """§3.6's "marks only", and a count IS the mark: `2/3` says where you are, which
        three dots do not."""
        self.assertEqual(self._row(16).strip(), "chats  2/3")

    def test_a_row_with_no_room_for_the_count_draws_nothing_at_all(self):
        """Rather than a fragment of one. A bar that could not say anything true says
        nothing — nothing is lost but the reminder (§3.6)."""
        self.assertEqual(self._row(11), "")

    def test_no_name_is_ever_shown_in_part_at_any_width(self):
        """**The property §3.6 asks `slots._NAME_MIN_W` to guarantee, asserted directly.**

        That constant is a floor against TRUNCATION, and this ladder never truncates —
        every rung drops whole names — so the guarantee is unconditional here rather than
        conditional on a width, and writing the floor as an `if` would be a line no input
        could reach. Asked at every width from 0 to 200 so there is no gap between the
        three widths the spec names.

        Asserted on the row's FIELDS rather than by substring search: every name shares a
        prefix with the heading and with each other, so `name[:3] in row` answers yes for
        rows that are perfectly correct. Splitting the row on its own gap and asking what
        each field IS is the property; anything else is a coincidence about spelling.
        """
        names = ["api-staging.1", "api-standby.2", "api.3"]
        counts = {f"+{n}" for n in range(1, len(names) + 1)}
        positions = {f"{i}/{len(names)}" for i in range(1, len(names) + 1)}
        for width in range(0, 201):
            row = slots._bar("chats", list(names), "api-standby.2", width)
            text = row[0] if row else ""
            self.assertLessEqual(tui.width(text), width, repr(text))
            if not text:
                continue
            head, _, body = text.strip().partition(" ")
            self.assertEqual(head, "chats", repr(text))
            for field in body.split(" " * slots._BAR_GAP):
                field = field.strip()
                if not field:
                    continue
                bare = field[len(slots._BAR_MARK[0]):] \
                    if field.startswith(slots._BAR_MARK[0]) else field
                self.assertIn(bare, {*names, *counts, *positions},
                              f"{width}: {field!r} is not a whole name, a count or a "
                              f"position — {text!r}")

    def test_one_chat_carries_the_add_affordance_and_two_do_not(self):
        row = self._row(120, names=["api.1"], here="api.1", note=slots.ADD_CHAT)
        self.assertIn(slots.ADD_CHAT, row)
        self.assertNotIn(slots.ADD_CHAT, self._row(120))

    def test_the_affordance_is_dropped_before_any_name_is(self):
        """It is a reminder and the names are the readout, so it goes first."""
        row = self._row(28, names=["api.1"], here="api.1", note=slots.ADD_CHAT)
        self.assertIn("api.1", row)
        self.assertNotIn(slots.ADD_CHAT, row)

    def test_no_names_at_all_is_no_row(self):
        self.assertEqual(slots._bar("chats", [], "api.1", 200), [])

    def test_a_row_you_are_not_on_still_lists_and_still_counts(self):
        """`here` naming nothing in the list is a real state — the workspace bar draws it
        for a frame whose recorded workspace has been deleted — and it must not become an
        unmarked row that silently claims you are somewhere. Nothing is marked, and the
        narrow rungs say how many there are without claiming a position."""
        wide = self._row(200, here="nowhere")
        self.assertNotIn(slots._BAR_MARK[0], wide)
        for name in self.NAMES:
            self.assertIn(name, wide)
        self.assertEqual(self._row(16, here="nowhere").strip(), "chats  3")

    def test_the_only_chat_is_never_counted_as_plus_zero(self):
        """The count is of the OTHERS, so one name has none. `+0` would be a field that
        is always false and always drawn."""
        row = self._row(20, names=["averylongchatname.1"], here="averylongchatname.1")
        self.assertNotIn("+0", row)

    def _rung(self, width, names=None, here="api.2", note=""):
        rows = slots._bar("chats", list(names or self.NAMES), here, width, note=note)
        return rows[0] if rows else ""

    def _fits_at(self, names, here, note=""):
        """Each rung's own row, and the exact width at which it stops fitting.

        Measured off the row rather than written down, so the assertions below follow the
        ladder if the inset or the gap ever moves.
        """
        seen, out = None, []
        for width in range(200, 0, -1):
            row = self._rung(width, names, here, note)
            if row != seen:
                if seen:
                    out.append((seen, width + 1))
                seen = row
                if not row:
                    break
        return out

    def test_every_rung_is_drawn_at_its_own_width_and_gone_one_cell_narrower(self):
        """**Each `<=` asserted on both sides of its own boundary**, which is the only way
        a comparison is pinned rather than its direction.

        The deletion sweep found eight `shift-boundary` survivors in this function — every
        `<=` → `<` and every `>=` → `>` — and each was a test measuring at a round width
        instead of at the width where the rung actually changes. A row that fits in
        exactly N cells must be drawn at N and must be gone at N-1.
        """
        for names, here, note in ((self.NAMES, "api.2", ""),
                                  (self.NAMES, "api.1", ""),
                                  (self.NAMES, "api.2", slots.ADD_CHAT),
                                  (["only.1"], "only.1", slots.ADD_CHAT)):
            rungs = self._fits_at(list(names), here, note)
            self.assertGreaterEqual(len(rungs), 2,
                                    f"{names} at {here!r} never changed rung")
            for row, width in rungs:
                with self.subTest(row=row, width=width):
                    self.assertEqual(tui.width(row), width,
                                     "the row does not fill the width it needs, so this "
                                     "measures no boundary")
                    self.assertEqual(self._rung(width, names, here, note), row)
                    self.assertNotEqual(self._rung(width - 1, names, here, note), row,
                                        "the row was still drawn one cell too narrow")

    def test_the_chat_you_are_in_being_FIRST_still_reaches_every_rung(self):
        """`at >= 0`, twice. The first name has index 0, so `at > 0` skips rung 2 entirely
        and turns `1/3` into a bare `3` — a row that has stopped saying where you are.
        Every other test here marks the middle name, which is exactly why the sweep found
        both comparisons unpinned."""
        rung2 = [r for r, _ in self._fits_at(self.NAMES, "api.1")
                 if "+2" in r]
        self.assertTrue(rung2, "rung 2 was never reached with the first name marked")
        self.assertIn(f"{slots._BAR_MARK[0]}api.1", rung2[0])
        counted = [r for r, _ in self._fits_at(self.NAMES, "api.1") if "/" in r]
        self.assertEqual([r.strip() for r in counted], ["chats  1/3"])

    def test_a_row_with_no_note_spends_no_columns_pretending_to_have_one(self):
        """`note and …` — without it an empty note still buys its own gap, and the row
        gains a trailing two cells that belong to the names."""
        row = self._rung(200, ["a.1"], "a.1", note="")
        self.assertEqual(row, row.rstrip(),
                         "an absent note still spent its separator")

    def test_the_mark_follows_the_raw_name_and_not_the_repaired_one(self):
        """`contain.one_line` is a REPAIR, so two names differing only in what it repairs
        are one string after it — and a mark matched on the drawn text would follow the
        repair rather than the identity. Neither caller can produce such a pair today
        (`chats.ID_RE` and `workspace.valid_name` both refuse those characters), which is
        why the index is taken before containment rather than left to be found later."""
        names = ["api x", "apix"]
        row = slots._bar("chats", list(names), "apix", 200)[0]
        marks = [f for f in row.split(" " * slots._BAR_GAP)
                 if f.strip().startswith(slots._BAR_MARK[0])]
        self.assertEqual(len(marks), 1,
                         f"two names were repaired into one marked row: {row!r}")
        self.assertTrue(row.rstrip().endswith(f"{slots._BAR_MARK[0]}apix"),
                        f"the mark landed on the repaired name: {row!r}")

    def test_a_hostile_name_is_contained_before_the_width_arithmetic(self):
        """#472, at the position it was filed about: a row that sized itself from a raw
        name. `tui.width` — never `len` — measures what `contain.one_line` already made
        one line of."""
        hostile = "z" * 20 + " " + "y" * 20
        for width in (200, 80, 40):
            row = slots._bar("chats", ["api.1", hostile], "api.1", width)
            text = row[0] if row else ""
            self.assertEqual(text, "".join(text.splitlines()), repr(text))
            self.assertLessEqual(tui.width(tui.strip_ansi(text)), width, repr(text))


class AClickResolvesAgainstWhatWasDrawn(unittest.TestCase):
    """`slots.TABS` — the column map every rung of `slots._bar` publishes.

    **The map is measured off the ROW, never off the map.** Each case below finds the
    columns it clicks by looking for the field in the string `_bar` returned, which is the
    thing the operator points at; asking `TABS` where it put something and then asking
    `TABS` what is there would agree with itself whatever the ladder did. The fixtures are
    ASCII on purpose, so a character index into that string IS a terminal column — the one
    case that is about a name needing repair says so and measures with `tui.width`.

    **A bar is horizontal, so the map is per column, and the ladder is why it has to exist
    at all**: three of the four rungs draw a different set of names and the fourth draws
    none, so "which name is at column 40" cannot be recomputed from the names and the
    width without re-walking the ladder — a second answer that disagrees with the first
    the moment a repaint lands between the paint and the click.
    """

    NAMES = ["api.1", "api.2", "api.3"]

    def setUp(self):
        # The strip is one object at module scope (`slots._Tabs`), so a case that did not
        # clear it would be reading the previous case's paint — and one that left a map
        # behind would hand it to whatever renders next.
        slots.TABS.forget()
        self.addCleanup(slots.TABS.forget)

    def _draw(self, width, names=None, here="api.2", note=""):
        rows = slots._bar("chats", list(names or self.NAMES), here, width, note=note)
        return rows[0] if rows else ""

    def _cells(self, row, field):
        """Every column *field* occupies in *row*, taken from the drawn row itself."""
        at = row.index(field)
        self.assertNotIn(field, row[at + 1:], f"{field!r} is not unique in {row!r}")
        return range(at, at + len(field))

    def test_every_tab_on_a_wide_row_resolves_to_its_own_name(self):
        """The whole feature, at the rung that draws every name: point at a tab, get that
        tab. `here` is a name that is NOT in the list so no tab is the current one and
        every one of them has an answer — the current-tab rule has its own case below."""
        row = self._draw(200, here="nowhere")
        for name in self.NAMES:
            for col in self._cells(row, name):
                self.assertEqual(slots.TABS.switch_to(col), name,
                                 f"column {col} of {row!r}")

    def test_the_mark_belongs_to_the_tab_it_marks(self):
        """A tab is its mark and its name together — the operator sees one field, and
        clicking either half of it means the same thing.

        Asserted on BOTH marks, because they are different characters in the same cell:
        the inactive `_BAR_MARK[1]` is the blank in front of a name you can switch to, and
        the active `_BAR_MARK[0]` is the `*`. The second is measured by drawing the same
        names with `here` somewhere else, which moves no column (the two marks are the
        same width — `test_the_two_marks_are_the_same_width…`), so the star's own cell can
        be asked about while it belongs to a tab that is not the current one.
        """
        row = self._draw(200, here="api.2")
        blank = self._cells(row, " api.3").start
        self.assertEqual(slots.TABS.switch_to(blank), "api.3",
                         "the blank mark in front of an inactive tab is not part of it")
        star = self._cells(row, "*api.2").start
        self.assertIsNone(slots.TABS.switch_to(star), "the tab you are on switched")
        self._draw(200, here="nowhere")
        self.assertEqual(slots.TABS.switch_to(star), "api.2",
                         "the marked tab's own mark column belongs to no tab")

    def test_the_gap_between_two_tabs_resolves_to_nothing(self):
        """Separator cells the operator can see are empty. Picking the nearer name for
        them would be the clamp `events.Dispatcher._on_canvas` refuses one rectangle out —
        a click on a cell nothing was drawn into is not a click on a neighbour."""
        row = self._draw(200, here="nowhere")
        end = self._cells(row, "api.1").stop
        for col in range(end, end + slots._BAR_GAP):
            self.assertIsNone(slots.TABS.switch_to(col), f"column {col} of {row!r}")

    def test_the_heading_resolves_to_nothing(self):
        """`  chats  ` is about no chat, exactly as the repo table's heading row is about
        no repo (`slots._repos` publishes `None` for it)."""
        row = self._draw(200, here="nowhere")
        # Up to the first tab's own MARK column, not to its name: the mark belongs to the
        # tab (`test_the_mark_belongs_to_the_tab_it_marks`), so a heading measured to the
        # first letter would assert the opposite of that case one cell over.
        for col in range(self._cells(row, " api.1").start):
            self.assertIsNone(slots.TABS.switch_to(col), f"column {col} of {row!r}")

    def test_the_empty_space_past_the_last_tab_resolves_to_nothing(self):
        """Most of a wide bar is blank. A click out there has to answer falsy rather than
        land on the last name, which is what a map running to the pane's width — or an
        index counting from the end — would do."""
        row = self._draw(200, here="nowhere")
        for col in range(len(row), 200):
            self.assertIsNone(slots.TABS.switch_to(col), f"column {col} of {row!r}")

    def test_the_tab_you_are_already_on_resolves_to_nothing(self):
        """Re-selecting what is already selected is not news — `_repos_events`' sentence,
        and here it is worth more: a re-switch is a real teardown and split (~360 ms for a
        chat) arriving exactly where the operator already was. It is also what keeps a
        double-click from switching twice."""
        row = self._draw(200, here="api.2")
        for col in self._cells(row, "api.2"):
            self.assertIsNone(slots.TABS.switch_to(col), f"column {col} of {row!r}")

    def test_neither_overflow_count_resolves_to_anything(self):
        """A `+N` stands for names that are NOT on the row, so there is nothing there to
        switch to — `…(+N more)`'s rule one axis over. **Both ends**, and the LEADING one
        is the new half: it sits between the heading and the first tab, so a map measured
        from the lead rather than from where the tabs actually start would hand its cells
        to the first name on the page — a click landing one tab off where the operator
        pressed."""
        names = [f"workspace-{i:02d}" for i in range(15)]
        row = self._draw(80, names=names, here="workspace-07")
        counts = [f.strip() for f in row.split(" " * slots._BAR_GAP)
                  if f.strip().startswith("+")]
        self.assertEqual(len(counts), 2, f"this width is not a page in the middle: {row!r}")
        for count in counts:
            for col in self._cells(row, count):
                self.assertIsNone(slots.TABS.switch_to(col), f"column {col} of {row!r}")

    def test_a_page_that_starts_in_the_middle_still_maps_its_tabs_where_they_are(self):
        """The leading count moves every tab right of where a bar without one puts them.

        **This is the case a map measured from the heading gets wrong**, and it gets it
        wrong quietly: every tab answers with its LEFT neighbour, which is a real name and
        a plausible switch. Asked on the drawn row, one column at a time, so the answer
        follows the paint rather than a second walk of the same arithmetic.

        The marked name is the one tab with no answer of its own
        (`test_the_tab_you_are_already_on_resolves_to_nothing`), and it is excluded here
        rather than the page being drawn for a `here` that names nothing — the windowed
        rung is only reached when the marked name is IN the list, which is the whole point
        of it drawing the page that name falls on.
        """
        names = [f"workspace-{i:02d}" for i in range(15)]
        here = "workspace-07"
        row = self._draw(80, names=names, here=here)
        self.assertTrue(row.lstrip().split("  ")[1].startswith("+"),
                        f"this row has no leading count to displace anything: {row!r}")
        drawn = [n for n in names if n in row and n != here]
        self.assertTrue(drawn, f"no switchable tab on the page: {row!r}")
        for name in drawn:
            for col in self._cells(row, name):
                self.assertEqual(slots.TABS.switch_to(col), name,
                                 f"column {col} of {row!r}")

    def test_the_rung_that_says_only_where_you_are_has_no_tabs_at_all(self):
        """`2/3` is a position, not a name, and there is no name on the row to click."""
        row = self._draw(16, here="api.2")
        self.assertEqual(row.strip(), "chats  2/3", repr(row))
        for col in range(0, 200):
            self.assertIsNone(slots.TABS.switch_to(col), f"column {col} of {row!r}")

    def test_a_narrowed_bar_forgets_the_tabs_it_is_no_longer_drawing(self):
        """**The `_Viewport.blank` half, one axis over.** A pane that drew every name and
        then narrowed to a count — or to nothing at all — must not go on answering for the
        tabs that were there: a click would switch to a name the operator can see is not on
        screen. Every rung publishes, including the two that draw no tab."""
        wide = self._draw(200, here="nowhere")
        col = self._cells(wide, "api.3").start
        self.assertEqual(slots.TABS.switch_to(col), "api.3")
        for width in (30, 16, 11):
            self._draw(width, here="api.2")
            self.assertIsNone(slots.TABS.switch_to(col),
                              f"width {width} kept a stale tab")

    def test_a_bar_with_no_names_at_all_publishes_no_tabs(self):
        """`chats.roster` answers with the chat asking and nothing else for a frame root it
        could not scan, and `_bar` answers `[]` for an empty list — so this is the
        unreadable-plane path, and it must leave nothing clickable behind either."""
        self._draw(200, here="nowhere")
        self.assertEqual(slots._bar("chats", [], "api.1", 200), [])
        for col in range(0, 200):
            self.assertIsNone(slots.TABS.switch_to(col), f"column {col}")

    def test_the_add_chat_affordance_is_not_a_tab(self):
        """It is a reminder naming a command, not a name to switch to."""
        row = self._draw(120, names=["api.1"], here="nowhere", note=slots.ADD_CHAT)
        self.assertIn(slots.ADD_CHAT, row)
        for col in self._cells(row, slots.ADD_CHAT):
            self.assertIsNone(slots.TABS.switch_to(col), f"column {col} of {row!r}")

    def test_a_column_nothing_drew_answers_nothing_however_far_out(self):
        """A mapping has no answer to give for a column outside it — which is where
        `_Viewport.repo_at`'s bounds check went. `-1` is the one that matters: a tuple
        would have answered with the LAST tab, hiding a wrong reading behind a plausible
        name, and the row's last column IS a tab here so that wrong answer is available."""
        row = self._draw(200, here="nowhere")
        self.assertEqual(slots.TABS.switch_to(len(row) - 1), "api.3",
                         "the last drawn column is not a tab, so -1 measures nothing")
        for col in (-1, -99, 10_000):
            self.assertIsNone(slots.TABS.switch_to(col), f"column {col} of {row!r}")

    def test_the_map_carries_the_name_on_disk_and_not_the_repaired_one(self):
        """`contain.one_line` is a REPAIR for drawing (#472), and what a click has to hand
        `charter frame-chat` or `switch.to_workspace` is the name in the directory. The two
        are one string for every name either caller can produce today, which is exactly why
        this asks with a name that needs repairing rather than waiting for a caller to stop
        refusing one."""
        raw = "api\t1"
        row = slots._bar("chats", [raw, "api.2"], "api.2", 200)[0]
        drawn = contain.one_line(raw)
        self.assertNotEqual(drawn, raw, "this name needs no repair, so it measures none")
        at = row.index(drawn)
        for col in range(at, at + tui.width(drawn)):
            self.assertEqual(slots.TABS.switch_to(col), raw,
                             f"column {col} of {row!r} carries the drawn name")

    def test_a_second_press_on_the_column_you_just_switched_from_does_nothing(self):
        """**The double-click property, asserted across the repaint that a switch causes**
        rather than against one paint.

        `test_the_tab_you_are_already_on_resolves_to_nothing` asks whether the marked tab
        is inert on the row in front of it. This asks the thing an operator can actually
        do wrong: press a tab, let the frame switch and repaint, press the same cell again.
        The answer has to be nothing — and it is only nothing because the page did not
        move. A window centred on the marked name would put a DIFFERENT workspace under
        that cell, and the second press would switch to it.

        Every drawn tab, at five widths, including the widths where the page has names on
        both sides of it.
        """
        names = [f"workspace-{i:02d}" for i in range(15)]
        for width in (60, 80, 120, 160, 200):
            row = self._draw(width, names=names, here="workspace-07")
            for name in [n for n in names if n in row and n != "workspace-07"]:
                for col in self._cells(row, name):
                    # Redrawn each time round: the strip is one object, so the paint the
                    # first press resolves against has to be the one in front of it.
                    self._draw(width, names=names, here="workspace-07")
                    self.assertEqual(slots.TABS.switch_to(col), name)
                    after = self._draw(width, names=names, here=name)
                    self.assertIsNone(
                        slots.TABS.switch_to(col),
                        f"{width}: pressing column {col} twice switched twice — "
                        f"{name} then {slots.TABS.switch_to(col)}\n"
                        f"  before {row!r}\n  after  {after!r}")


class ThisPlaneIsWhyTheRungIsWindowed(unittest.TestCase):
    """The fifteen workspaces this repository's own plane has, at the widths it is run at.

    **The measurement #725 shipped with, kept as a test.** That change made both bars
    clickable and the `workspaces` bar stayed inert on the plane it was reported from:
    those names need 274 columns for rung 1, so every real width fell to a rung whose only
    drawn tab was the one the operator was already on. "Clickable" and "reaches nothing"
    are the same screen.

    Written against the real names rather than a fixture of even-width ones, because the
    greedy cut is about the widths names actually have — `relations-and-delegations` is 25
    cells and `todos` is 5, and a page of uniform names would measure neither.
    """

    NAMES = sorted([
        "authority-audit", "autonomy", "charter-update-skill", "default", "fleet",
        "harness-wrapper", "news-dispatch-guard", "opencode-integration", "plane-shape",
        "relations-and-delegations", "showcase", "statusline-improvements", "todos",
        "tracking-github-issues", "user-reporting",
    ])
    HERE = "harness-wrapper"

    def setUp(self):
        slots.TABS.forget()
        self.addCleanup(slots.TABS.forget)

    def _switchable(self, width):
        rows = slots._bar("workspaces", list(self.NAMES), self.HERE, width)
        row = rows[0] if rows else ""
        self.assertLessEqual(tui.width(row), width, repr(row))
        return row, {slots.TABS.switch_to(col) for col in range(width)} - {None}

    def test_rung_one_still_needs_two_hundred_and_seventy_four_columns(self):
        """The number the whole change is about. Measured off the ladder, so it follows
        the inset and the gap if either moves."""
        widest = next(w for w in range(400)
                      if len(slots._bar("workspaces", list(self.NAMES), self.HERE, w)) == 1
                      and all(n in slots._bar("workspaces", list(self.NAMES),
                                              self.HERE, w)[0] for n in self.NAMES))
        self.assertEqual(widest, 274)

    def test_every_real_width_now_reaches_workspaces_the_operator_is_not_on(self):
        """120, 160 and 200 columns. Before this change each of them drew
        `*harness-wrapper  +14` and switched to nothing at all."""
        for width, least in ((120, 5), (160, 7), (200, 9)):
            row, switchable = self._switchable(width)
            self.assertNotIn(self.HERE, switchable,
                             "the tab you are on is not somewhere to switch to")
            self.assertGreaterEqual(
                len(switchable), least,
                f"{width} columns reaches {len(switchable)} workspaces: {row!r}")

    def test_the_narrowest_frame_that_draws_a_name_still_says_where_in_the_list_it_is(self):
        """A page of one is inert — there is nothing on it but the name you are on — and
        that is the rung this replaced, arrived at honestly. It still carries both counts,
        so it says strictly more than the `*harness-wrapper  +14` it replaces: which of
        the fifteen this is."""
        row, switchable = self._switchable(60)
        self.assertIn(f"{slots._BAR_MARK[0]}{self.HERE}", row)
        counts = [f.strip() for f in row.split(" " * slots._BAR_GAP)
                  if f.strip().startswith("+")]
        self.assertEqual(sum(int(c[1:]) for c in counts),
                         len(self.NAMES) - len([n for n in self.NAMES if n in row]),
                         f"the counts do not add up to what is off the row: {row!r}")


class TheChatBarReadsThePlane(PersonaIso, unittest.TestCase):
    """`slots.chats_bar` over a real frame directory."""

    def setUp(self):
        super().setUp()
        self._env = mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "api"},
                                    clear=False)
        self._env.start()
        self.addCleanup(self._env.stop)

    def test_the_bar_hides_the_second_name_when_there_is_one_chat_and_says_how(self):
        """Stage 5b's exit criterion, first half: "the chat bar is absent with one chat".
        Absent means it stops being a list — it still says which chat you are in and how
        to get a second, because a row that vanished would leave an operator with no way
        to learn the feature exists."""
        _plant("api.1", workspace="api")
        row = slots.chats_bar("api.1", 200)[0]
        self.assertIn("api.1", row)
        self.assertIn(slots.ADD_CHAT, row)
        self.assertIn("charter <harness>", row,
                      "the affordance must name something that works today")

    def test_the_bar_lists_both_chats_when_there_are_two(self):
        """The other half: "present with two"."""
        _plant("api.1", workspace="api")
        _plant("api.2", workspace="api")
        row = slots.chats_bar("api.2", 200)[0]
        self.assertIn("api.1", row)
        self.assertIn("*api.2", row)
        self.assertNotIn(slots.ADD_CHAT, row)

    def test_only_this_workspaces_chats_are_on_the_bar(self):
        _plant("api.1", workspace="api")
        _plant("web.1", workspace="web")
        row = slots.chats_bar("api.1", 200)[0]
        self.assertNotIn("web.1", row)

    def test_a_plane_it_cannot_read_draws_no_row_rather_than_raising(self):
        with mock.patch("os.scandir", side_effect=OSError("nope")):
            self.assertEqual(slots.chats_bar("", 200), [])


class TheWorkspaceBarReadsTheFrame(PersonaIso, unittest.TestCase):
    def test_it_marks_the_workspace_the_FRAME_is_on_not_this_process(self):
        """#512: a panel is a child of a tmux server shared between every frame on the
        machine, so a bar resolving locally would mark another plane's workspace."""
        for name in ("alpha", "beta"):
            (config.WORKSPACES_DIR / name).mkdir(parents=True, exist_ok=True)
        state.frame_dir("f1", create=True)
        state.record_workspace("f1", "beta")
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": ""}, clear=False):
            row = slots.workspaces_bar("f1", 200)[0]
        self.assertIn("*beta", row)
        self.assertIn("alpha", row)
        self.assertNotIn("*alpha", row)


class ABarIsPlaceableByConfig(unittest.TestCase):
    """**The route that makes these components real rather than dead code.**

    Neither bar has a committed `[frame] slots` word — adding one would put it on every
    operator's frame, which `frame/builtins.build` measures the cost of — so the only way
    to ask for one is a `[[frame.component]]` table. Before `builtins.places` existed that
    form refused them: the branch asked `cid in SLOT_OF`, so a component charter
    registers, sizes and can draw fell through to the provider check and was refused for
    having no installed distribution behind it.
    """

    TABLES = {"frame": {"component": [
        {"use": "chats", "edge": "top", "size": 1},
        {"use": "workspaces", "edge": "top", "size": 1},
        {"use": "identity"},
        {"use": "attention"},
    ]}}

    def test_a_component_table_places_both_bars_at_the_geometry_they_declare(self):
        frame = instance.frame_of(self.TABLES)
        placed = {p["use"]: (p["edge"], p["size"]) for p in frame["components"]}
        self.assertEqual(placed["chats"], ("top", Fixed(1)))
        self.assertEqual(placed["workspaces"], ("top", Fixed(1)))

    def test_a_table_naming_an_edge_the_bar_does_not_declare_is_refused(self):
        """A built-in's edge is derived at import (`layout._derive`), so a different one
        could only be read, validated, stored and ignored — the convincing empty this form
        is written against. Refused whole, #535."""
        cfg = {"frame": {"component": [{"use": "chats", "edge": "bottom", "size": 1}]}}
        self.assertIsNone(instance.component_tables(cfg["frame"]))

    def test_a_sidebar_part_is_still_not_placeable(self):
        """`places` answers off `Registry.on_edge`, which excludes a composite's parts —
        so `changes` stays a section. A part that could be placed as well would be drawn
        twice, once in its own pane and once inside the sidebar's."""
        for part in ("personas", "todos", "changes"):
            self.assertFalse(builtins.places(part), part)
            cfg = {"frame": {"component": [{"use": part, "edge": "right", "size": 4}]}}
            self.assertIsNone(instance.component_tables(cfg["frame"]), part)

    def test_a_panel_process_can_draw_a_bar_it_was_handed_by_name(self):
        """`slots.drawable` is the one answer four callers share, and a bar has to be in
        it or `charter panel chats --session <fid>` refuses rather than painting."""
        self.assertTrue(slots.drawable("chats"))
        self.assertTrue(slots.drawable("workspaces"))
        self.assertFalse(slots.drawable("changes"))

    def test_places_refuses_anything_that_is_not_a_name(self):
        """It is asked of a value read out of a committed `charter.toml`, on the import
        path of every charter command — so a TOML array or a table reaching
        `Registry.on_edge`'s comparison must be a `False` here rather than a traceback
        that takes `charter --version` down with it."""
        for junk in (None, 3, True, ["chats"], {"use": "chats"}):
            self.assertFalse(builtins.places(junk), repr(junk))

    def test_a_provider_cannot_answer_for_a_bars_name(self):
        """`drawable`'s own rule, extended to the bars: a distribution declaring `chats`
        must not become the answer to a question about charter's own component."""
        with mock.patch.object(builtins, "supplies", return_value=False):
            self.assertTrue(slots.drawable("chats"))


class BothBarsGoWhenTheRowsRunOut(unittest.TestCase):
    """§3.6 asks that both bars "join `layout._DROP_ORDER`, above `top`".

    **Taken literally that instruction changed nothing**, and this class is what says so.
    That constant was read by nothing: `visible_slots` spelled its order out by hand as
    `s != "right"` and `s != "top"`, so a bar added to the list would have survived
    exactly the shortage that took the identity row — the wrong way round for a readout
    the palette makes redundant. `layout._ROW_DROPS` derives the row-edge half from it, so
    the list is now the mechanism and these assertions are about behaviour rather than
    about a tuple.
    """

    ALL = ["chats", "workspaces", "top", "bottom", "repos", "right"]

    def _kept(self, cols, rows):
        frame = config.FRAME
        return layout.visible_slots(list(self.ALL), cols, rows,
                                    frame["min_cols"], frame["min_rows"])

    def test_a_roomy_terminal_keeps_both_bars(self):
        self.assertEqual(self._kept(200, 50), self.ALL)

    def test_a_short_terminal_gives_up_both_bars_with_the_identity_row(self):
        kept = self._kept(200, 16)
        for gone in ("chats", "workspaces", "top", "right"):
            self.assertNotIn(gone, kept)
        self.assertIn("bottom", kept,
                      "the attention strip is the one slot that never goes")

    def test_the_drop_list_is_what_decides_and_not_a_second_copy_of_it(self):
        """The property that makes `_DROP_ORDER` a constant: every row-edge name in it is
        one a short terminal actually loses. Deleting an entry has to change this."""
        kept = self._kept(200, 16)
        for name in layout._ROW_DROPS:
            self.assertNotIn(name, kept, f"{name} is in _ROW_DROPS and survived")
        self.assertEqual(layout._ROW_DROPS, ("chats", "workspaces", "top"))
