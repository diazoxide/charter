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
`changes` a section rather than a pane), and each placed pane is a share of a switch's tmux
invocations — 26 of them since #780 batched every write nothing reads back, ~162 ms on
tmux 3.7c and ~127 ms at the 3.2 floor with four panels. So the bars
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


def _plain(row: str) -> str:
    """*row* with the frame's own paint taken off.

    The bar draws the tab you are on as a reverse-video block (`chrome.block`), so the
    string a rung returns is no longer the string a terminal shows — and every question
    below that is about TEXT (is this field a whole name? is `chats  2/3` the whole row?)
    is about the second one. `tui.strip_ansi` is what `panel._write` runs for a plane
    under `NO_COLOR`, so this is also literally what such a plane is shown.

    Questions about COLUMNS do not go through here and must not: `tui.width` already
    counts no SGR, so a width taken off the painted row is the width the pane sees.
    """
    return tui.strip_ansi(row)


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

    def test_a_page_is_cut_inside_the_room_its_own_leading_count_has_left(self):
        """**The leading `+N` is paid for before a name is, and this is where that shows.**

        A page after the first is cut inside `room` MINUS that count, so the row it goes on
        to compose fits the pane. Cut inside the whole room instead and the row comes out
        one name too wide — the ladder measures it, gives the entire rung up, and a name
        with a perfectly good page is drawn as `3/15`, with nothing on the bar to click.
        The deletion sweep found that reserve unpinned and this is the case it asked for.

        Asked as the property that failure breaks, which is **widening the pane never takes
        a name away**. The reserve is what keeps the ladder monotone: without it a wider
        room cuts a page one name longer, the leading count that page has to carry was
        never budgeted for, the row overflows and the whole rung is given up. Measured on
        the mutant — with these fifteen names the row is drawn at 31 columns, gone at 42
        through 45, and back at 46. A bar that loses its names when the pane gets bigger is
        a bug an operator reports as a flicker.

        Every name of the list, at every width from 0 to 200, plus the narrowest row for
        three of them written out — one near the start, one in the middle, one at the end,
        because the leading count is two cells wide for some and three for others.
        """
        names = [f"workspace-{i:02d}" for i in range(15)]
        for here in names:
            drawn = [w for w in range(201)
                     if here in self._row(w, names=names, here=here)]
            self.assertTrue(drawn, f"{here} is drawn at no width at all")
            missing = [w for w in range(min(drawn), 201) if w not in drawn]
            self.assertEqual(missing, [],
                             f"{here} was drawn at {min(drawn)} columns and gone at "
                             f"{missing[:3]} — the pane got wider and lost a name")
        for here, expected in (("workspace-02", "chats  +2  *workspace-02  +12"),
                               ("workspace-07", "chats  +7  *workspace-07  +7"),
                               ("workspace-13", "chats  +13  *workspace-13  +1")):
            narrowest = min(w for w in range(201)
                            if here in self._row(w, names=names, here=here))
            row = self._row(narrowest, names=names, here=here)
            self.assertEqual(_plain(row).strip(), expected)
            self.assertEqual(tui.width(row), narrowest,
                             "the row does not fill the width it is first drawn at, so "
                             "this measures no boundary")

    def test_the_last_page_is_never_the_tab_you_are_on_alone(self):
        """**#767, and it is #758 coming back at a WIDER width.**

        Every page but the last is filled to the brim, so the last holds the remainder —
        and a remainder shrinks as the pages grow. With the marked name sorting last, a
        fifteen-name plane drew `+14  *workspace-14` at 228 columns: one tab, the one the
        frame is already on, which `_Tabs.switch_to` correctly refuses. The bar was
        clickable and reached nothing, at a width wider than the one that fixed it.

        The last page now takes a name back from the page before it rather than standing
        alone. Asked from 150 columns up, where the room is never the reason.
        """
        names = [f"workspace-{i:02d}" for i in range(15)]
        here = names[-1]
        for width in range(150, 281):
            row = self._row(width, names=names, here=here)
            drawn = [n for n in names if n in row]
            self.assertIn(here, drawn, f"{width}: the marked name is not on the row")
            self.assertGreaterEqual(
                len(drawn), 2,
                f"{width} columns drew only the tab the frame is on: {row!r}")

    def test_the_limit_this_cut_does_not_fix_is_ten_widths_of_exactly_one_name(self):
        """**The stated cost, asserted instead of merely described** (#767).

        `_page`'s docstring says the count is still not monotone — "10 such widths between
        60 and 280 on a fifteen-name list, down from 12, each of exactly one name" — and a
        number that lives only in a docstring is a claim nothing can falsify. It was
        written from a measurement and there was nothing to keep it true.

        Two claims, and the second is the one that matters: **no drop is ever more than a
        single name.** A cut that started losing three at a time would still be "not
        monotone" and would be a different, worse thing. The count of ten is pinned beside
        it so that a change to the cut has to come back here and restate what it costs —
        which is how the docstring stays honest rather than becoming folklore.
        """
        names = [f"workspace-{i:02d}" for i in range(15)]
        here = names[-1]
        drops = []
        previous = None
        for width in range(60, 281):
            row = self._row(width, names=names, here=here)
            count = len([n for n in names if n in row])
            if previous is not None and count < previous:
                drops.append((width, previous - count))
            previous = count
        self.assertEqual([step for _w, step in drops], [1] * len(drops),
                         f"a drop cost more than one name: {drops}")
        self.assertEqual(len(drops), 10,
                         f"the ladder's docstring says ten widths and this found "
                         f"{len(drops)}: {drops}")

    def test_rescuing_the_last_page_costs_the_pages_before_it_nothing_here(self):
        """The floor is on the LAST page, so it moves one boundary and no other. On this
        project's own fifteen workspaces — where `harness-wrapper` sorts mid-list and the
        last page is never the marked one — every width draws exactly what filling each
        page to the brim drew, which is what makes this free rather than a trade."""
        ws = ThisPlaneIsWhyTheRungIsWindowed.NAMES
        drawn = []
        for width in (100, 120, 160, 200, 240):
            rows = slots._bar("workspaces", list(ws), "harness-wrapper", width)
            row = rows[0] if rows else ""
            drawn.append(len([n for n in ws if n in row]))
        self.assertEqual(drawn, [4, 6, 8, 10, 13],
                         "the rescue took a tab off a plane it was not for")

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
            text = _plain(row[0]) if row else ""
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

    def test_the_affordance_is_drawn_only_when_a_caller_asks_for_one(self):
        """`_bar` draws no note it was not handed one for. Which BARS ask is
        `chats_bar`/`workspaces_bar`'s decision and has its own case at
        :class:`TheChatBarReadsThePlane`; this is the arithmetic half."""
        row = _plain(self._row(120, names=["api.1"], here="api.1", note=slots.ADD_CHAT))
        self.assertTrue(row.rstrip().endswith(slots.ADD_CHAT), repr(row))
        self.assertNotIn(slots.ADD_CHAT, self._row(120))

    def test_the_affordance_is_dropped_before_any_name_is(self):
        """It is one affordance and the names are the readout, so it goes first.

        A `+` is one cell where the sentence it replaced was 29, so the width that drops it
        while keeping the name is narrower than it used to be — measured off the ladder
        below rather than written as a number, for the reason `test_rung_one_still_needs
        _two_hundred_and_seventy_four_columns` gives: a constant here would be a second
        copy of the arithmetic."""
        names = ["api-staging.1", "api-standby.2"]
        widest = max(w for w in range(0, 201)
                     if slots.ADD_CHAT not in
                     _plain(self._row(w, names=names, here="api-staging.1",
                                      note=slots.ADD_CHAT))
                     and "api-standby.2" in _plain(self._row(w, names=names,
                                                             here="api-staging.1",
                                                             note=slots.ADD_CHAT)))
        row = _plain(self._row(widest, names=names, here="api-staging.1",
                               note=slots.ADD_CHAT))
        for name in names:
            self.assertIn(name, row, repr(row))
        self.assertNotIn(slots.ADD_CHAT, row, repr(row))

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

    def test_a_page_that_ends_the_list_spends_no_columns_on_a_count_it_has_not_got(self):
        """The same rule one rung down, for the TRAILING `+N` — and the deletion sweep is
        what asked for it.

        A page whose last name is the last name in the list draws no trailing count, and
        the separator that would have gone in front of it must go with it. The sweep found
        `f"{gap}{trailing}" if trailing else ""` surviving: with the `else` collapsed, such
        a page ends in the gap alone, which is two blank cells on a plane whose rules are
        hidden and ` | ` on one whose rules are visible — a seam drawn between a tab and
        nothing.

        **It is not only cosmetic**, which is why it is asserted as a width rather than as
        a strip. Those cells are measured: `_bar` refuses the whole rung when the composed
        body will not fit, so a page carrying a phantom separator is given up one to three
        columns earlier than it should be — a name lost from a pane that had room for it.

        Asked over every width and every name, because which pages end the list depends on
        both, and asked on a plane whose rules are VISIBLE as well, where the phantom is a
        glyph rather than whitespace and `rstrip` alone would not see it.
        """
        names = [f"workspace-{i:02d}" for i in range(15)]
        for rules in ("hidden", "visible"):
            with mock.patch.dict(config.FRAME, {"rules": rules}):
                for here in names:
                    for width in range(0, 201):
                        row = _plain(self._row(width, names=names, here=here))
                        if not row:
                            continue
                        self.assertEqual(
                            row, row.rstrip(),
                            f"{rules} at {width} on {here}: the row ends in the "
                            f"separator of a count it did not draw — {row!r}")
                        self.assertFalse(
                            row.rstrip().endswith(slots._BAR_RULE),
                            f"{rules} at {width} on {here}: a seam was drawn between the "
                            f"last tab and nothing — {row!r}")

    def test_the_mark_follows_the_raw_name_and_not_the_repaired_one(self):
        """`contain.one_line` is a REPAIR, so two names differing only in what it repairs
        are one string after it — and a mark matched on the drawn text would follow the
        repair rather than the identity. Neither caller can produce such a pair today
        (`chats.ID_RE` and `workspace.valid_name` both refuse those characters), which is
        why the index is taken before containment rather than left to be found later."""
        names = ["api x", "apix"]
        row = _plain(slots._bar("chats", list(names), "apix", 200)[0])
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
    ASCII on purpose, so what separates a character index from a terminal column is only
    the frame's own paint — :meth:`_cells` is where the two are reconciled, and the one
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
        """Every column *field* occupies in *row*, taken from the drawn row itself.

        **A character index is not a column, and this is where the two stopped being one
        number.** The bar paints the tab you are on (`chrome.block`), so every field right
        of it sits some escape bytes further into the string than it sits into the pane —
        and a case that clicked the character index would be clicking a column left of the
        one it read, which is precisely the off-by-a-tab this class exists to catch.
        `tui.width` of the text BEFORE the field is the column, and it counts no SGR, so
        it is right whether or not anything on the row is painted.
        """
        at = row.index(field)
        self.assertNotIn(field, row[at + 1:], f"{field!r} is not unique in {row!r}")
        start = tui.width(row[:at])
        return range(start, start + tui.width(field))

    def test_every_tab_on_a_wide_row_resolves_to_its_own_name(self):
        """The whole feature, at the rung that draws every name: point at a tab, get that
        tab. `here` is a name that is NOT in the list so no tab is the current one and
        every one of them has an answer — the current-tab rule has its own case below."""
        row = self._draw(200, here="nowhere")
        for name in self.NAMES:
            for col in self._cells(row, name):
                self.assertEqual(slots.TABS.switch_to(0, col), name,
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
        self.assertEqual(slots.TABS.switch_to(0, blank), "api.3",
                         "the blank mark in front of an inactive tab is not part of it")
        star = self._cells(row, "*api.2").start
        self.assertIsNone(slots.TABS.switch_to(0, star), "the tab you are on switched")
        self._draw(200, here="nowhere")
        self.assertEqual(slots.TABS.switch_to(0, star), "api.2",
                         "the marked tab's own mark column belongs to no tab")

    def test_the_gap_between_two_tabs_resolves_to_nothing(self):
        """Separator cells the operator can see are empty. Picking the nearer name for
        them would be the clamp `events.Dispatcher._on_canvas` refuses one rectangle out —
        a click on a cell nothing was drawn into is not a click on a neighbour."""
        row = self._draw(200, here="nowhere")
        end = self._cells(row, "api.1").stop
        for col in range(end, end + slots._BAR_GAP):
            self.assertIsNone(slots.TABS.switch_to(0, col), f"column {col} of {row!r}")

    def test_the_heading_resolves_to_nothing(self):
        """`  chats  ` is about no chat, exactly as the repo table's heading row is about
        no repo (`slots._repos` publishes `None` for it)."""
        row = self._draw(200, here="nowhere")
        # Up to the first tab's own MARK column, not to its name: the mark belongs to the
        # tab (`test_the_mark_belongs_to_the_tab_it_marks`), so a heading measured to the
        # first letter would assert the opposite of that case one cell over.
        for col in range(self._cells(row, " api.1").start):
            self.assertIsNone(slots.TABS.switch_to(0, col), f"column {col} of {row!r}")

    def test_the_empty_space_past_the_last_tab_resolves_to_nothing(self):
        """Most of a wide bar is blank. A click out there has to answer falsy rather than
        land on the last name, which is what a map running to the pane's width — or an
        index counting from the end — would do."""
        row = self._draw(200, here="nowhere")
        for col in range(len(row), 200):
            self.assertIsNone(slots.TABS.switch_to(0, col), f"column {col} of {row!r}")

    def test_the_tab_you_are_already_on_resolves_to_nothing(self):
        """Re-selecting what is already selected is not news — `_repos_events`' sentence,
        and here it is worth more: a re-switch is a real teardown and split (~162 ms for a
        four-panel chat on 3.7c) arriving exactly where the operator already was. It is also what keeps a
        double-click from switching twice."""
        row = self._draw(200, here="api.2")
        for col in self._cells(row, "api.2"):
            self.assertIsNone(slots.TABS.switch_to(0, col), f"column {col} of {row!r}")

    def test_neither_overflow_count_switches_to_anything(self):
        """A `+N` stands for names that are NOT on the row, so there is nothing there to
        switch to — `…(+N more)`'s rule one axis over. **Both ends**, and the LEADING one
        is the new half: it sits between the heading and the first tab, so a map measured
        from the lead rather than from where the tabs actually start would hand its cells
        to the first name on the page — a click landing one tab off where the operator
        pressed.

        This is still exactly true and it is no longer the whole answer: a count is a
        `more_at` cell now, which is the case below. The two are separate methods for
        `_Tabs.more_at`'s reason, so they get separate cases: what would be wrong is a
        count that started SWITCHING somewhere, and that is what this keeps out.
        """
        names = [f"workspace-{i:02d}" for i in range(15)]
        row = _plain(self._draw(80, names=names, here="workspace-07"))
        counts = [f.strip() for f in row.split(" " * slots._BAR_GAP)
                  if f.strip().startswith("+")]
        self.assertEqual(len(counts), 2, f"this width is not a page in the middle: {row!r}")
        for count in counts:
            for col in self._cells(row, count):
                self.assertIsNone(slots.TABS.switch_to(0, col), f"column {col} of {row!r}")

    def test_both_overflow_counts_are_a_cell_that_opens_the_picker(self):
        """*"when workspaces are more we are showing `+N` now in tabs — but user can't
        click and see other workspaces."*

        The operator pressed one. That is the strongest evidence available about what a
        `+9` looks like it does, and the answer was nothing at all.

        **Both ends, measured off the drawn row**, for the reason every case in this class
        gives: the leading count is the one a map built from the heading rather than from
        the composition would put in the wrong place, and it is drawn at a different width
        than the trailing one (`+13` against `+1`), so a `_span` measured with `len` would
        pass on one and fail on the other only for names that sort late.
        """
        names = [f"workspace-{i:02d}" for i in range(15)]
        row = _plain(self._draw(80, names=names, here="workspace-07"))
        counts = [f.strip() for f in row.split(" " * slots._BAR_GAP)
                  if f.strip().startswith("+")]
        self.assertEqual(len(counts), 2, f"this width is not a page in the middle: {row!r}")
        for count in counts:
            for col in self._cells(row, count):
                self.assertTrue(slots.TABS.more_at(0, col),
                                f"the {count} is still inert at column {col}: {row!r}")

    def test_nothing_but_a_count_is_a_cell_that_opens_the_picker(self):
        """The negative, and it is the one that keeps the two answers apart. A tab, the
        heading, a gap and the space past the last name must all answer `more_at` no —
        otherwise a click meant for a workspace would open a palette instead of switching,
        which is the same class of wrong as switching to the neighbour."""
        names = [f"workspace-{i:02d}" for i in range(15)]
        row = _plain(self._draw(80, names=names, here="workspace-07"))
        counts = [f.strip() for f in row.split(" " * slots._BAR_GAP)
                  if f.strip().startswith("+")]
        opening = {c for c in range(200) if slots.TABS.more_at(0, c)}
        expected = {col for count in counts for col in self._cells(row, count)}
        self.assertEqual(opening, expected,
                         f"a cell that is not a count opens the picker: {row!r}")

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
                self.assertEqual(slots.TABS.switch_to(0, col), name,
                                 f"column {col} of {row!r}")

    def test_the_rung_that_says_only_where_you_are_has_no_tabs_at_all(self):
        """`2/3` is a position, not a name, and there is no name on the row to click."""
        row = _plain(self._draw(16, here="api.2"))
        self.assertEqual(row.strip(), "chats  2/3", repr(row))
        for col in range(0, 200):
            self.assertIsNone(slots.TABS.switch_to(0, col), f"column {col} of {row!r}")

    def test_the_rung_that_says_only_where_you_are_still_opens_the_picker(self):
        """**The width where this matters most.** A frame narrow enough to fall to `2/3`
        has a strip that can be pointed at and not pressed — the whole list is off the row,
        so every rule above about clicking a tab is vacuous here. `2/3` is the same field
        the counts are, saying the same thing about all three names instead of about nine
        of fifteen, so it opens the same chooser.

        The heading and the space past it stay inert, which is what says the cells were
        measured rather than the whole row being made live."""
        row = _plain(self._draw(16, here="api.2"))
        for col in self._cells(row, "2/3"):
            self.assertTrue(slots.TABS.more_at(0, col), f"column {col} of {row!r}")
        for col in self._cells(row, "chats"):
            self.assertFalse(slots.TABS.more_at(0, col), f"the heading opened a picker")
        self.assertFalse(slots.TABS.more_at(0, tui.width(row) + 5),
                         "the space past the row opened a picker")

    def test_a_rung_that_draws_nothing_opens_nothing(self):
        """`_Viewport.blank`'s half, for the third thing `publish` writes. A bar narrowed
        past its last rung draws no row — so there is no count on screen, and a `more_at`
        that survived the narrowing would open a palette from a cell the operator can see
        is empty."""
        self._draw(80, names=[f"workspace-{i:02d}" for i in range(15)],
                   here="workspace-07")
        self.assertTrue(any(slots.TABS.more_at(0, c) for c in range(80)))
        self.assertEqual(self._draw(11), "")
        self.assertEqual([c for c in range(200) if slots.TABS.more_at(0, c)], [],
                         "a bar that drew no row kept the counts it is no longer drawing")

    def test_a_narrowed_bar_forgets_the_tabs_it_is_no_longer_drawing(self):
        """**The `_Viewport.blank` half, one axis over.** A pane that drew every name and
        then narrowed to a count — or to nothing at all — must not go on answering for the
        tabs that were there: a click would switch to a name the operator can see is not on
        screen. Every rung publishes, including the two that draw no tab."""
        wide = self._draw(200, here="nowhere")
        col = self._cells(wide, "api.3").start
        self.assertEqual(slots.TABS.switch_to(0, col), "api.3")
        for width in (30, 16, 11):
            self._draw(width, here="api.2")
            self.assertIsNone(slots.TABS.switch_to(0, col),
                              f"width {width} kept a stale tab")

    def test_a_bar_with_no_names_at_all_publishes_no_tabs(self):
        """`chats.roster` answers with the chat asking and nothing else for a frame root it
        could not scan, and `_bar` answers `[]` for an empty list — so this is the
        unreadable-plane path, and it must leave nothing clickable behind either."""
        self._draw(200, here="nowhere")
        self.assertEqual(slots._bar("chats", [], "api.1", 200), [])
        for col in range(0, 200):
            self.assertIsNone(slots.TABS.switch_to(0, col), f"column {col}")

    def test_the_add_chat_affordance_is_not_a_tab(self):
        """It makes a chat; it does not switch to one. There is no chat there yet, which
        is the whole point of pressing it."""
        row = _plain(self._draw(120, names=["api.1"], here="nowhere",
                                note=slots.ADD_CHAT))
        self.assertTrue(row.rstrip().endswith(slots.ADD_CHAT), repr(row))
        for col in self._cells(row, slots.ADD_CHAT):
            self.assertIsNone(slots.TABS.switch_to(0, col), f"column {col} of {row!r}")

    def test_the_add_chat_affordance_is_a_cell_that_makes_a_chat(self):
        """*"`+` button not working for creating new session."*

        The other half of the case above, and the reason the row has a third question:
        the cell switches to nothing and opens no picker, and it is not therefore inert.
        """
        row = _plain(self._draw(120, names=["api.1"], here="nowhere",
                                note=slots.ADD_CHAT))
        cells = list(self._cells(row, slots.ADD_CHAT))
        self.assertEqual([c for c in range(200) if slots.TABS.add_at(0, c)], cells,
                         f"the `+` is not the one cell that makes a chat: {row!r}")
        for col in cells:
            self.assertFalse(slots.TABS.more_at(0, col),
                             "the `+` was published as an overflow count as well")

    def test_a_plus_and_a_plus_n_are_never_on_the_same_row(self):
        """**Two fields that both begin with `+` and mean different things**, kept apart
        by the ladder rather than by a reader's care: the affordance is drawn only on the
        rung where every name fits and a count only on the rung where they do not.

        Asked at every width on a list long enough to overflow, because "they cannot both
        appear" is exactly the kind of claim that is true of the three widths somebody
        checked.
        """
        names = [f"api.{i}" for i in range(1, 13)]
        for width in range(0, 201):
            row = _plain(self._draw(width, names=names, here="api.6",
                                    note=slots.ADD_CHAT))
            fields = [f.strip() for f in row.split(" " * slots._BAR_GAP) if f.strip()]
            plus = [f for f in fields if f.startswith("+")]
            self.assertLessEqual(len(plus), 2, f"{width}: {row!r}")
            if slots.ADD_CHAT in plus:
                self.assertEqual(plus, [slots.ADD_CHAT],
                                 f"{width}: a `+` and a count share a row: {row!r}")

    def test_a_narrowed_bar_forgets_the_affordance_it_is_no_longer_drawing(self):
        """`_Viewport.blank`'s rule for the fourth thing `publish` writes. A bar that kept
        its `+` cell through a resize would make a chat from a column the operator can see
        holds a name."""
        self._draw(120, names=["api.1"], here="api.1", note=slots.ADD_CHAT)
        self.assertTrue(any(slots.TABS.add_at(0, c) for c in range(120)))
        self._draw(120, names=["api.1"], here="api.1")
        self.assertEqual([c for c in range(200) if slots.TABS.add_at(0, c)], [],
                         "the bar kept an affordance it stopped drawing")

    def test_no_row_at_any_width_ever_draws_or_maps_a_column_left_of_zero(self):
        """**The property a deleted guard used to protect by accident** (#767).

        `_page` walks the last boundary LEFT to keep the final page from being a lone tab.
        A `len(cuts) >= 3` conjunct kept that walk away from a single-field list, where the
        boundary would step to `-1` — and the deletion sweep found it, because on such a
        list `_bar` refuses the rung on width whatever `_page` answers, so the guard could
        not change a row. It was deleted rather than suppressed. What was NOT deleted is
        the property: a `+-1` field is unreadable and a negative column in the strip is the
        wrong-answer-hiding index `_Tabs` refuses on purpose.

        So this asserts the property where it can be seen, on the drawn row and the
        published map, rather than trusting a guard whose effect nothing could observe.
        Singletons and a name far wider than the pane are the shapes that reach the walk.
        """
        lists = (["only"], ["a"], ["x" * 60], ["a", "b"], ["x" * 40, "y"],
                 [f"workspace-{i:02d}" for i in range(15)])
        for names in lists:
            for here in names:
                for width in range(0, 120):
                    rows = slots._bar("chats", list(names), here, width)
                    row = rows[0] if rows else ""
                    self.assertNotIn("+-", row,
                                     f"{names} at {width}: a count went negative: {row!r}")
                    for col in range(-4, 0):
                        self.assertIsNone(
                            slots.TABS.switch_to(0, col),
                            f"{names} at {width}: column {col} is a tab: {row!r}")

    def test_a_column_nothing_drew_answers_nothing_however_far_out(self):
        """A mapping has no answer to give for a column outside it — which is where
        `_Viewport.repo_at`'s bounds check went. `-1` is the one that matters: a tuple
        would have answered with the LAST tab, hiding a wrong reading behind a plausible
        name, and the row's last column IS a tab here so that wrong answer is available."""
        row = self._draw(200, here="nowhere")
        self.assertEqual(slots.TABS.switch_to(0, len(row) - 1), "api.3",
                         "the last drawn column is not a tab, so -1 measures nothing")
        for col in (-1, -99, 10_000):
            self.assertIsNone(slots.TABS.switch_to(0, col), f"column {col} of {row!r}")

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
            self.assertEqual(slots.TABS.switch_to(0, col), raw,
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
                    self.assertEqual(slots.TABS.switch_to(0, col), name)
                    after = self._draw(width, names=names, here=name)
                    self.assertIsNone(
                        slots.TABS.switch_to(0, col),
                        f"{width}: pressing column {col} twice switched twice — "
                        f"{name} then {slots.TABS.switch_to(0, col)}\n"
                        f"  before {row!r}\n  after  {after!r}")


class TheTabYouAreOnIsDrawnAsOne(unittest.TestCase):
    """The strip's own paint: a block on the tab you are on, and nothing anywhere else.

    *"now we are showing plain text, no colors, no active tab/session color"* — the report
    this class is the standing form of. The bar drew a `*` and nothing else, which is a
    caption's way of saying "you are here" and not a strip's.

    **Reverse video and not a colour, which is `frame/chrome.py`'s decision and not a
    fresh one.** Reverse is the operator's own foreground and background exchanged, so it
    is right on every theme charter cannot see — including the Solarized palettes, where
    every named grey charter could have picked IS somebody's background. It also survives
    every terminal tier tmux downsamples for. And it is orthogonal to `[frame] chrome`,
    which is the shipped `off`: that key paints the pane's BACKGROUND through a tmux pane
    option and says nothing about what a renderer writes into its own row.

    **`slots._BAR_MARK` stays, and that is what makes the design read with no colour at
    all.** `panel._write` strips every escape from every row on a plane under `NO_COLOR`,
    and the Linux console can carry no highlight worth the name — so a strip whose only
    answer to "which tab am I on" was the paint would have no answer there. The `*` is a
    character. The block is on top of it, never instead of it.
    """

    NAMES = ["api.1", "api.2", "api.3"]
    #: Spelled out rather than read off `chrome._REVERSE` and `chrome._OFF`. A case built
    #: from the constants it is about agrees with any value they take, which is the
    #: survivor `commands_change.BLOCK_END` produced — so the escape a terminal actually
    #: receives is written here, by hand, a second time.
    ON, OFF = "\x1b[7m", "\x1b[0m"

    def setUp(self):
        slots.TABS.forget()
        self.addCleanup(slots.TABS.forget)

    def _row(self, width=200, names=None, here="api.2"):
        rows = slots._bar("chats", list(names or self.NAMES), here, width)
        return rows[0] if rows else ""

    def test_the_tab_you_are_on_is_a_block_and_no_other_tab_is(self):
        row = self._row()
        self.assertIn(f"{self.ON}*api.2{self.OFF}", row)
        self.assertEqual(row.count(self.ON), 1, f"more than one block: {row!r}")

    def test_the_block_covers_the_mark_and_the_name_and_stops(self):
        """**Exactly the cells `slots.TABS` gives that tab, which is the point.** A block
        one cell wide either way would highlight a column that answers for a different tab
        — or for no tab — and the operator would be pointing at what they can see is lit.
        """
        row = self._row()
        painted = row.split(self.ON, 1)[1].split(self.OFF, 1)[0]
        self.assertEqual(painted, "*api.2")
        start = tui.width(row[:row.index(self.ON)])
        for col in range(start, start + tui.width(painted)):
            self.assertIsNone(slots.TABS.switch_to(0, col),
                              f"column {col} is inside the block and is not the tab you "
                              f"are on: {row!r}")
        self.assertEqual(slots.TABS.switch_to(0, start - 1), None)
        self.assertEqual(slots.TABS.switch_to(0, start + tui.width(painted)), None)

    def test_the_paint_costs_the_row_no_cell_at_all(self):
        """Which is why it could be added to a strip that is already competing for columns.
        Asked at every width, so a rung that measured the painted body instead of the
        plain one would be caught at the boundary where it overflows rather than only at
        the wide end."""
        for width in range(0, 201):
            painted = self._row(width)
            plain = _plain(painted)
            self.assertEqual(tui.width(painted), tui.width(plain), repr(painted))
            self.assertLessEqual(tui.width(painted), width, repr(painted))

    def test_a_row_that_is_on_no_tab_paints_nothing(self):
        """`here` naming nothing in the list is a real state — the workspace bar draws it
        for a frame whose recorded workspace has been deleted. Nothing is marked, so
        nothing is lit; a block on a tab the frame is not on would be the row claiming a
        position it has just declined to claim one field over."""
        row = self._row(here="nowhere")
        self.assertNotIn(self.ON, row, f"an unmarked row painted something: {row!r}")

    def test_with_every_escape_stripped_the_row_still_says_where_you_are(self):
        """`NO_COLOR`, the Linux console, `charter panel chats --session x > /tmp/log`.
        The strip has to keep working on all three, and this is the property that says it
        does: the answer survives the paint being deleted."""
        row = _plain(self._row())
        self.assertIn(f"{slots._BAR_MARK[0]}api.2", row)
        self.assertNotIn(f"{slots._BAR_MARK[0]}api.1", row)
        self.assertNotIn(f"{slots._BAR_MARK[0]}api.3", row)

    def test_every_column_of_a_painted_row_still_answers_for_the_tab_under_it(self):
        """**The regression the paint could actually cause**, asked directly and at both
        the rung that draws every name and the rung that draws a page.

        Escapes make the string longer than the row, so a map measured off character
        positions would hand every tab right of the block to its left-hand neighbour — a
        real name, a plausible switch, and wrong. The map is built from `tui.width` of the
        fields, which counts no SGR, and this is what says so.
        """
        names = [f"workspace-{i:02d}" for i in range(15)]
        for width in (200, 120, 80, 60):
            with self.subTest(width=width):
                row = self._row(width, names=names, here="workspace-07")
                if not row:
                    continue
                plain = _plain(row)
                for name in (n for n in names if n in plain):
                    at = plain.index(name) - tui.width(slots._BAR_MARK[0])
                    want = None if name == "workspace-07" else name
                    for col in range(at, at + 1 + tui.width(name)):
                        self.assertEqual(slots.TABS.switch_to(0, col), want,
                                         f"{width}: column {col} of {plain!r}")


class TheSeamBetweenTwoTabsIsThePlanesOwnAnswer(unittest.TestCase):
    """`[frame] rules`, one scope in — the seam between two TABS rather than two panes.

    *"we don't have any separator borders."* The key that answers that question already
    exists and the operator has already been asked it: `rules = "hidden"` (shipped) means
    "I want a surface with no seams in it", `rules = "visible"` means "show me the
    structure". A second key for the tab strip would let one frame draw pane borders and
    no tab rules, which is the disagreement `instance.FRAME_RULES` exists to end.

    **The glyph is ASCII and that is the sharp end of this class.** `│` is what an IDE
    draws and it is East-Asian *Ambiguous*: `tui.width` says one cell, a terminal may draw
    two. The repo table draws box glyphs and can afford to because its click map is per
    ROW. A bar's map is per COLUMN, so a separator that comes out a cell wider than it was
    measured shifts every tab right of it — ten separators, ten columns, and the operator
    presses one workspace and lands on another. `component.EVENT_KINDS`' "fires wrongly",
    reached through a glyph.
    """

    NAMES = ["api.1", "api.2", "api.3"]

    def setUp(self):
        slots.TABS.forget()
        self.addCleanup(slots.TABS.forget)

    def _with(self, rules):
        return mock.patch.dict(config.FRAME, {"rules": rules})

    def _row(self, width=200, names=None, here="api.2", rules="hidden"):
        with self._with(rules):
            rows = slots._bar("chats", list(names or self.NAMES), here, width)
        return rows[0] if rows else ""

    def test_a_plane_whose_rules_are_hidden_draws_the_row_it_always_drew(self):
        """**The shipped default costs nothing**, which is what made this affordable on a
        row that is already short of columns. Byte for byte the old composition: two
        blanks between two tabs and no glyph anywhere."""
        row = _plain(self._row(rules="hidden"))
        self.assertEqual(row.strip(), "chats   api.1  *api.2   api.3")
        self.assertNotIn(slots._BAR_RULE, row)

    def test_a_plane_whose_rules_are_visible_puts_one_between_every_pair(self):
        row = _plain(self._row(rules="visible"))
        self.assertEqual(row.strip(), "chats   api.1 | *api.2 |  api.3")
        self.assertEqual(row.count(slots._BAR_RULE), len(self.NAMES) - 1)

    def test_the_rule_is_a_glyph_no_terminal_draws_two_cells_wide(self):
        """Asked as the PROPERTY rather than as `== "|"`: what matters is that no terminal
        may disagree with `tui.width` about it, and every character outside ASCII is a
        character some terminal might. A future edit reaching for `│` because it looks
        better fails here, which is where the reason is written down."""
        self.assertEqual(tui.width(slots._BAR_RULE), 1)
        self.assertTrue(slots._BAR_RULE.isascii(),
                        f"{slots._BAR_RULE!r} is outside ASCII, so a terminal may draw it "
                        f"a different width than this row was measured at — and every tab "
                        f"right of it then answers a click meant for its neighbour")

    def test_nothing_the_strip_draws_is_outside_ascii(self):
        """The rule above, asked of the WHOLE row rather than of one constant — the mark,
        the separator, the counts and the position. A name is not charter's to hold to
        this and is excluded by construction: these fixtures are ASCII, so anything else
        on the row came from this module."""
        names = [f"workspace-{i:02d}" for i in range(15)]
        for width in range(0, 261):
            row = _plain(self._row(width, names=names, here="workspace-07",
                                   rules="visible"))
            self.assertTrue(row.isascii(), f"{width}: {row!r}")

    def test_the_rule_belongs_to_neither_tab(self):
        """A seam is a cell the operator can see is not a name. Picking the nearer one for
        it would be the clamp `events.Dispatcher._on_canvas` refuses one rectangle out —
        and here it would be worse than for a blank gap, because a rule LOOKS like an
        edge and the tab it would be read as is the one on the far side of it."""
        row = self._row(rules="visible", here="nowhere")
        plain = _plain(row)
        self.assertIn(slots._BAR_RULE, plain)
        for col, ch in enumerate(plain):
            if ch == slots._BAR_RULE:
                self.assertIsNone(slots.TABS.switch_to(0, col),
                                  f"the seam at column {col} answered for a tab: {plain!r}")

    def test_every_column_of_a_ruled_row_answers_for_the_tab_under_it(self):
        """The whole map, on a plane that draws rules, at four widths and on a list long
        enough to reach the windowed rung. A gap read as two cells in the cut and drawn as
        three would put every tab one column left of where the map says it is."""
        names = [f"workspace-{i:02d}" for i in range(15)]
        for width in (300, 200, 120, 80):
            with self.subTest(width=width):
                row = self._row(width, names=names, here="workspace-07",
                                rules="visible")
                if not row:
                    continue
                plain = _plain(row)
                self.assertLessEqual(tui.width(plain), width, repr(plain))
                for name in (n for n in names if n in plain):
                    at = plain.index(name) - tui.width(slots._BAR_MARK[0])
                    want = None if name == "workspace-07" else name
                    for col in range(at, at + 1 + tui.width(name)):
                        self.assertEqual(slots.TABS.switch_to(0, col), want,
                                         f"{width}: column {col} of {plain!r}")

    def test_the_ladder_pays_for_the_rules_rather_than_overflowing(self):
        """**A rule costs a column and the ladder is what finds it.** At the width where
        every name fits without rules, the same row with rules on cannot hold them all —
        so it must give up a whole name, exactly as it does for a narrower pane, and never
        run past the width it was given.

        Asked at every width from 0 to 300 on this project's own fifteen workspaces, which
        is where fourteen extra cells is a name and a half.
        """
        names = [f"workspace-{i:02d}" for i in range(15)]
        for width in range(0, 301):
            with self._with("visible"):
                rows = slots._bar("workspaces", list(names), "workspace-07", width)
            text = _plain(rows[0]) if rows else ""
            self.assertLessEqual(tui.width(text), width, f"{width}: {text!r}")
        widest = next(w for w in range(500)
                      if all(n in _plain(self._row(w, names=names, here="workspace-07",
                                                   rules="visible")) for n in names))
        without = next(w for w in range(500)
                       if all(n in _plain(self._row(w, names=names, here="workspace-07",
                                                    rules="hidden")) for n in names))
        self.assertEqual(widest - without, len(names) - 1,
                         "a plane that draws rules pays exactly one cell per seam")


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
        return row, {slots.TABS.switch_to(0, col) for col in range(width)} - {None}

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

    def test_a_workspace_with_one_chat_still_says_which_and_offers_another(self):
        """Stage 5b's exit criterion, first half: "the chat bar is absent with one chat".
        Absent means it stops being a list — it still says which chat you are in and how
        to get a second, because a row that vanished would leave an operator with no way
        to learn the feature exists."""
        _plant("api.1", workspace="api")
        row = _plain(slots.chats_bar("api.1", 200)[0])
        self.assertIn("api.1", row)
        self.assertTrue(row.rstrip().endswith(slots.ADD_CHAT), repr(row))

    def test_the_bar_lists_both_chats_when_there_are_two_and_still_offers_a_third(self):
        """The other half: "present with two" — and the `+` stays.

        **It used to go**, on the argument that the affordance was a reminder for a plane
        that had not discovered chats yet. That was right about a SENTENCE and is wrong
        about a button: an operator with two chats wanting a third is exactly who presses
        it, and a `+` that appears and disappears depending on how many tabs there are is
        the one behaviour no tab strip anywhere has.
        """
        _plant("api.1", workspace="api")
        _plant("api.2", workspace="api")
        row = _plain(slots.chats_bar("api.2", 200)[0])
        self.assertIn("api.1", row)
        self.assertIn("*api.2", row)
        self.assertTrue(row.rstrip().endswith(slots.ADD_CHAT), repr(row))

    def test_the_workspace_bar_offers_no_such_thing(self):
        """**A chat is a press; a workspace is a name.** A new chat has nothing for an
        operator to type — its id is allocated and its workspace is fixed for life (§4j) —
        while a new workspace is a directory and a name #518 refuses to create on a typo.
        So the `+` is the chat bar's and the workspace bar draws none, at any width and
        whatever the plane holds."""
        for name in ("alpha", "beta"):
            (config.WORKSPACES_DIR / name).mkdir(parents=True, exist_ok=True)
        state.frame_dir("f1", create=True)
        state.record_workspace("f1", "alpha")
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": ""}):
            for width in range(0, 201):
                rows = slots.workspaces_bar("f1", width)
                row = _plain(rows[0]) if rows else ""
                self.assertFalse(row.rstrip().endswith(slots.ADD_CHAT),
                                 f"{width}: the workspace bar drew a `+`: {row!r}")
                self.assertEqual([c for c in range(width) if slots.TABS.add_at(0, c)], [],
                                 f"{width}: the workspace bar published an affordance")

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

    **`top` left `_ROW_DROPS` in #740 and the bars stayed**, which is the same argument
    carried one step further: the bars go at `min_rows` because `F2` reaches every chat
    and every workspace at every width, and `top` does not go there because nothing
    reaches the workspace and the persona it names. See `layout.visible_slots`.
    """

    ALL = ["chats", "workspaces", "top", "bottom", "repos", "right"]

    def _kept(self, cols, rows):
        frame = config.FRAME
        return layout.visible_slots(list(self.ALL), cols, rows,
                                    frame["min_cols"], frame["min_rows"])

    def test_a_roomy_terminal_keeps_both_bars(self):
        self.assertEqual(self._kept(200, 50), self.ALL)

    def test_a_short_terminal_gives_up_both_bars_and_keeps_the_identity_row(self):
        """**The identity row's half of this reversed in #740**, and the bars' half did
        not. A bar is a readout the palette reaches in two keystrokes at every width, so a
        short terminal loses the reminder and nothing else; `top`'s workspace and persona
        are reached by nothing else once the sidebar has gone, which is why ranking it
        beside them was the inversion that issue is about."""
        kept = self._kept(200, 16)
        for gone in ("chats", "workspaces", "right"):
            self.assertNotIn(gone, kept)
        self.assertIn("top", kept,
                      "the identity row is above both bars in _DROP_ORDER")
        self.assertIn("bottom", kept,
                      "the attention strip is the one slot that never goes")

    def test_the_drop_list_is_what_decides_and_not_a_second_copy_of_it(self):
        """The property that makes `_DROP_ORDER` a constant: every row-edge name in it is
        one a short terminal actually loses. Deleting an entry has to change this."""
        kept = self._kept(200, 16)
        for name in layout._ROW_DROPS:
            self.assertNotIn(name, kept, f"{name} is in _ROW_DROPS and survived")
        self.assertEqual(layout._ROW_DROPS, ("chats", "workspaces"))
        self.assertEqual(layout._DROP_ORDER[-1], "top",
                         "`top` left _ROW_DROPS and must still be last in the order it "
                         "is derived from — see layout.visible_slots (#740)")
