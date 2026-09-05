"""#829 item 2: a strip whose names will not fit on one row is given another one.

*"panes are resizable — and user can resize and it should show more tabs opened on new
resized rows."*

**A panel cannot make its own pane taller.** tmux owns pane geometry and both bars declare
`Fixed(1)` (`frame/builtins.py`), so the second row is not a renderer setting — it is a
size the LAUNCHER asks for, which is `layout.slot_sizes` and the three `commands_frame`
call sites that go through `_slot_sizes`. That is the same seam `repos` has had since #488
and the reason this file's cases are spread across three modules: what the names need
(`slots.bar_rows_wanted`), what the frame can afford (`layout._grown`), and what tmux is
then told (`layout.panel_argvs`, `commands_frame._reassert_sizes`).

**It grows only when the tabs actually overflow**, which is the refinement #829's decision
carries over the two shapes the issue named. A fixed two-row launch costs a row off the
harness on every plane that places a bar whether or not the names overflow — a cost
everyone pays for a case only some hit, and rows against the harness are what #740 settled
once already. The strip already knows when it is overflowing: that is what `+N` counts.

**And it grows only into rows the harness can spare.** The budget is `layout.harness_rows`
of the ungrown map minus `layout.HARNESS_MIN_ROWS`, so a window with nothing spare grows
nothing and says so by leaving the strip exactly as it was.

**What the 3.2 floor gets, stated rather than assumed.** The launch path has no version
gate: a strip is born at the height its names need at every tmux charter runs on, floor
included. What `tmuxctl.RESIZE_HOOK_FLOOR` gates is the RE-computation — `window-resized`
does not exist below tmux 3.3 (`set-hook -w window-resized` answers `invalid option`,
rc 1, measured), so at the floor a strip keeps the height it was born with until the frame
is relaunched. `tmuxctl.below_resize_hook_message` is the band that already tells the
operator so — and it names the recovery, which is `charter frame-resize` typed by hand.
`TheFloorKeepsTheHeightItWasBornWith` is what pins both halves.
"""

from __future__ import annotations

import unittest
from unittest import mock

from charter import commands_frame, config, instance, tui
from charter.frame import layout, slots, state, tmuxctl
from tests._isolation import PersonaIso
from tests.test_frame_chat_switch import _plant

#: The fifteen workspaces this repository's own plane has — the list #725 measured the
#: windowed rung against, kept here for the same reason `test_frame_bars` keeps it: the
#: greedy cut is about the widths names actually have, and a fixture of even-width ones
#: would measure neither `relations-and-delegations` (25 cells) nor `todos` (5).
NAMES = sorted([
    "authority-audit", "autonomy", "charter-update-skill", "default", "fleet",
    "harness-wrapper", "news-dispatch-guard", "opencode-integration", "plane-shape",
    "relations-and-delegations", "showcase", "statusline-improvements", "todos",
    "tracking-github-issues", "user-reporting",
])
HERE = "harness-wrapper"

#: A frame that draws both strips and the table, in the order `[[frame.component]]`
#: places them.
#:
#: **A bar has to be PLACED for `layout` to have any geometry for it at all**, and that is
#: not a fixture detail — it is the whole reason `max_rows` cannot be a renderer setting.
#: Neither bar is in `builtins.SLOT_OF`, so neither is in `layout.SLOT_SIZE` and
#: `layout._size_of` answers for one only out of the arrangement this plane committed
#: (#687). `slot_sizes` therefore drops a bar nothing placed, and a case that patched no
#: arrangement would be asserting about a slot that is not in the answer.
SLOTS = ["top", "chats", "workspaces", "bottom", "repos", "right"]


def _placed(*, style=None, **sizes):
    """The resolved arrangement that places both bars, built the way an operator's
    `charter.toml` reaches `config.FRAME` — through `instance.frame_of`, never by hand.

    *sizes* pins a slot to a committed height, which is the one route a bar's own
    `[[frame.component]]` table has (#687); *style* carries the pane keys (`pad`, `bg`)
    for one slot.
    """
    tables = []
    for use in ("identity", "chats", "workspaces", "attention", "repos", "sidebar"):
        table = {"use": use}
        if use in sizes:
            table.update(edge="top", size=sizes[use])
        if style and use in style:
            table.update(style[use])
        tables.append(table)
    return instance.frame_of({"frame": {"component": tables}})


def _strip(names=NAMES, here=HERE, note="", counts=None):
    """A `slots.BARS` entry that answers *names* without touching a plane.

    The seam the sizer reads through, used as one. `TheRealStripsGoThroughTheSameSeam`
    is what says charter's own two entries reach it, so this is a fixture for the
    arithmetic rather than a stand-in for the feature.

    *counts* defaults to ``None`` — no count field — which is what makes the widths below
    the ladder's own rather than the workspace strip's (#880). The reserve that strip spends
    is `slots.TAB_COUNT_W` per tab and has its own cases; every number in this module is
    about how many rows a list of names needs, and folding a fixed six cells per name into
    them would measure the field as much as the ladder.
    """
    return lambda fid: (list(names), here, note, counts)


class TheStripAsksForTheRowsItsNamesNeed(unittest.TestCase):
    """`slots.bar_rows_wanted` — the measurement the launcher sizes the pane from.

    Asked through `slots.BARS` rather than a plane, for `test_frame_bars`'
    `TheLadderGivesUpWholeThings` reason: the arithmetic is the thing that has to be right
    at every width, and a case that needed a frame to ask it would be measuring the
    fixture as much as the answer.
    """

    def _want(self, cols, cap=3, names=NAMES, here=HERE, note="", slot="probe"):
        with mock.patch.dict(slots.BARS, {slot: _strip(names, here, note)}):
            return slots.bar_rows_wanted("f-1", slot, pane_cols=cols, cap=cap)

    def test_a_strip_whose_names_all_fit_on_one_row_asks_for_one(self):
        """262 columns is where rung 1 draws all fifteen (`test_frame_bars` pins the
        number), and a strip that is not overflowing must not take a row off the harness
        — which is the whole argument against the fixed two-row launch #829 rejected."""
        self.assertEqual(self._want(262), 1)
        self.assertEqual(self._want(400), 1)

    def test_a_strip_that_overflows_asks_for_the_rows_its_pages_need(self):
        """The widths #725 measured an operator running at, and the answers are the page
        counts the same cut produces: two rows at 160 and three at 120."""
        self.assertEqual(self._want(160), 2)
        self.assertEqual(self._want(120), 3)

    def test_the_want_is_the_rows_the_renderer_then_actually_fills(self):
        """The property the whole seam exists for, and the one #500 shipped broken twice
        one pane over: the launcher sizes the pane and the renderer spends it, so a want
        the renderer does not fill is blank rows off the harness, and a want it overflows
        is names cut off with nothing saying so.

        Asked at EVERY mark as well as every width, because the run a strip draws depends
        on which page the mark is on and a want measured for one mark sizes the pane for
        all of them.
        """
        for here in NAMES:
            for cols in range(0, 320, 3):
                for cap in (layout.BAR_MAX_ROWS, 40):
                    with self.subTest(here=here, cols=cols, cap=cap):
                        want = self._want(cols, cap=cap, here=here)
                        slots.TABS.forget()
                        lines = slots._bar(list(NAMES), here, cols,
                                           rows=want)
                        if lines:
                            self.assertEqual(
                                len(lines), want,
                                f"{cols} columns asked for {want} rows and filled "
                                f"{len(lines)} — the rest come off the harness blank")

    def test_a_want_the_ceiling_did_not_stop_reaches_every_name(self):
        """The other half: where the strip was allowed all the rows it asked for, it holds
        the whole list. Below the ceiling this is what growing a row BUYS, and a want that
        stopped short of it would be rows spent for nothing."""
        for cols in range(40, 320, 7):
            want = self._want(cols, cap=40)
            if want >= 40:
                continue
            slots.TABS.forget()
            lines = slots._bar(list(NAMES), HERE, cols, rows=want)
            drawn = {slots.TABS.switch_to(r, c)
                     for r in range(want) for c in range(cols)} - {None}
            if lines and len(drawn) > 1:
                with self.subTest(cols=cols):
                    self.assertEqual(drawn | {HERE}, set(NAMES),
                                     f"{cols} columns asked for {want} rows and still "
                                     f"cannot reach every workspace")

    def test_a_strip_too_narrow_to_draw_a_name_asks_for_one_row_and_not_the_ceiling(self):
        """The `2/3` rung and the rung below it. A frame that cannot draw one name cannot
        draw one on a second row either, so spending rows there would be taking them off
        the harness for a count — the exact trade `layout.visible_slots` refuses one rung
        down when it drops both bars whole.

        20 columns is the `6/15` rung and 0 is the rung that draws nothing. Both were 3
        before the "did it fill what it was given" test went in beside the coverage one,
        which is two rows of harness spent on a count. (30 columns was the `6/15` rung
        while the strip drew a heading; without one, 30 has room for a page — #880.)
        """
        self.assertEqual(self._want(20), 1)
        self.assertEqual(self._want(0), 1)
        self.assertEqual(self._want(20, cap=8), 1)

    def test_the_ceiling_is_the_callers_and_it_bounds_the_answer(self):
        """Carried rather than read, `layout.repos_rows`' discipline for *pinned_rows*:
        applying `BAR_MAX_ROWS` here AND in `layout._grown` would be a bound no input
        could make observable. 80 columns wants five rows and gets what it is allowed."""
        self.assertEqual(self._want(80, cap=8), 5)
        self.assertEqual(self._want(80, cap=3), 3)
        self.assertEqual(self._want(80, cap=1), 1)

    def test_the_shipped_ceiling_is_three(self):
        """Spelled, because it is a number with a measurement behind it rather than a
        derived one: this plane's fifteen workspaces need 2 rows at 160 columns, 3 at 120,
        3 at 100 and 5 at 80, and three is where a strip stops being a strip. (They were
        2/3/4/6 while the strip still spent twelve columns on a heading — #880.)"""
        self.assertEqual(layout.BAR_MAX_ROWS, 3)

    def test_the_panes_own_pad_comes_off_the_width_before_the_names_are_measured(self):
        """#500's half of this, and it is the same argument `repos_rows_wanted` makes: the
        renderer composes at `slots.content_width`, so a padded pane's strip is planned for
        `pane_cols - 2 * pad` and asking the unpadded question would size the pane from a
        number the renderer never sees.

        The two-row band for these names starts at 138 columns, so a 140-column pane with a
        two-cell pad each side is a three-row strip and the same pane without one is a
        two-row strip. Asked through a real `[[frame.component]]` table, because a pad only
        exists where an operator committed one.
        """
        with mock.patch.dict(config.FRAME, _placed(style={"chats": {"pad": 2}})):
            self.assertEqual(self._want(140, slot="chats"), 3)
        with mock.patch.dict(config.FRAME, _placed()):
            self.assertEqual(self._want(140, slot="chats"), 2)

    def test_a_slot_that_draws_no_strip_is_answered_rather_than_raised_at(self):
        """`layout.slot_sizes`' filter-don't-refuse discipline: the slot list is committed,
        untrusted input, and a name that is not a strip has the height its pane already
        has."""
        self.assertEqual(slots.bar_rows_wanted("f-1", "repos", pane_cols=200, cap=3), 1)
        self.assertEqual(slots.bar_rows_wanted("f-1", "top", pane_cols=200, cap=3), 1)

    def test_measuring_a_strip_does_not_publish_a_map_for_it(self):
        """The reason `slots._compose` was split out of `slots._bar` (#829). This runs in
        the LAUNCHER's process and in the `frame-resize` child; a sizing question that
        wrote `slots.TABS` would leave a map describing a strip nobody is looking at, and
        in a test process it would describe one another case had just drawn."""
        slots.TABS.forget()
        slots._bar(["api.1", "api.2"], "api.1", 200)
        at = tui.width(slots._inset()) + 1
        before = slots.TABS.switch_to(0, at)
        self._want(120)
        self.assertEqual(slots.TABS.switch_to(0, at), before,
                         "the sizing question overwrote the paint's own map")


class EveryRowOfAGrownStripIsClickable(unittest.TestCase):
    """`slots._bar` with rows to spend, and `slots.TABS` under it.

    **The map is keyed by `(row, col)` and that is the load-bearing half.** A column-keyed
    map on a two-row strip does not degrade to answering nothing — it answers the row above
    about a click on the row below, which is `component.EVENT_KINDS`' *fires wrongly* and
    the same harm `slots._BAR_RULE` is ASCII to avoid.
    """

    def setUp(self):
        slots.TABS.forget()
        self.addCleanup(slots.TABS.forget)

    def _strip(self, width, rows, names=NAMES, here=HERE, note=""):
        return slots._bar(list(names), here, width, note=note, rows=rows)

    def test_a_second_row_draws_the_names_the_first_could_not(self):
        """The operator's own sentence, measured: 160 columns reaches eight workspaces on
        one row and every one of the fifteen on two. (Seven, until #880 gave the heading's
        twelve columns back to the names.)"""
        one = self._strip(160, 1)
        reachable_one = {slots.TABS.switch_to(0, c) for c in range(160)} - {None}
        two = self._strip(160, 2)
        reachable_two = {slots.TABS.switch_to(r, c)
                         for r in range(2) for c in range(160)} - {None}
        self.assertEqual(len(one), 1)
        self.assertEqual(len(two), 2)
        self.assertEqual(len(reachable_one), 8)
        self.assertEqual(reachable_two | {HERE}, set(NAMES))

    def test_every_drawn_tab_answers_at_its_own_row_and_at_no_other(self):
        """Walked off the drawn rows rather than off the ladder, `test_frame_bars`'
        `AClickResolvesAgainstWhatWasDrawn` discipline: the paint is what a click is
        resolved against, so the paint is what the case reads."""
        rows = self._strip(120, 3)
        self.assertEqual(len(rows), 3)
        for r, line in enumerate(rows):
            plain = tui.strip_ansi(line)
            for name in NAMES:
                at = plain.find(name)
                if at < 0 or name == HERE:
                    continue
                with self.subTest(row=r, name=name):
                    self.assertEqual(slots.TABS.switch_to(r, at), name)
                    others = [o for o in range(len(rows)) if o != r]
                    self.assertNotIn(name, [slots.TABS.switch_to(o, at) for o in others],
                                     "a column answers with a name from another row")

    def test_the_inset_every_row_starts_at_is_not_a_tab(self):
        """Every row of the strip begins at the frame's own left edge, and those cells are
        charter's: no name was drawn in them and a click there switches nothing.

        **Asked on EVERY row, which is what it took over from the case it replaces.** Until
        #880 this was the blank under the `workspaces` heading, and it could only ever be
        asked about rows two and three because row one held the word itself. With no
        heading there is one lead on every row and the same question to ask of all of
        them."""
        rows = self._strip(120, 3)
        lead = tui.width(slots._inset())
        for r in range(len(rows)):
            for c in range(lead):
                with self.subTest(row=r, col=c):
                    self.assertIsNone(slots.TABS.switch_to(r, c))
                    self.assertFalse(slots.TABS.more_at(r, c))
                    self.assertFalse(slots.TABS.add_at(r, c))

    def test_every_row_of_the_strip_starts_its_tabs_at_the_same_left_edge(self):
        """**What is left of the heading case after #880, and it is the half that was ever
        load-bearing.** This class used to assert that `workspaces` was drawn on row one
        and blanked to the same width on the rows below it — a heading repeated down the
        left of a three-row strip being the same word three times where names are competing
        for columns. The heading is gone; the property the blank under it existed for is
        not, and it is this one: every row begins its names in the SAME column, which is
        what lets every page be cut against one `room` and is what a click's stability
        rests on.

        **Nothing else here can see it**, which is why it is still its own case. Every
        column assertion in this class is taken from the row it is about, so a second row
        that started three cells to the left would map its own tabs correctly and still be
        cut for a room it does not have. Only comparing the rows to each other tells them
        apart.
        """
        for cols, rows in ((120, 3), (160, 2), (100, 3)):
            drawn = [tui.strip_ansi(r) for r in self._strip(cols, rows)]
            with self.subTest(cols=cols, rows=rows):
                self.assertGreater(len(drawn), 1, "this width drew no second row")
                lead = tui.width(slots._inset())
                indents = {tui.width(line) - tui.width(line.lstrip()) for line in drawn}
                # A row whose first field is a tab carries `_BAR_MARK`'s blank in front of
                # the name, so its text begins one cell past the lead; a row whose first
                # field is a leading `+N`, or whose first tab is the marked one, begins at
                # the lead itself. Both are the same lead — what this refuses is a row
                # composed against a different one.
                self.assertLessEqual(
                    indents, {lead, lead + tui.width(slots._BAR_MARK[1])},
                    f"a row was composed against a different lead: {drawn!r}")

    def test_a_row_the_strip_did_not_draw_answers_nothing(self):
        """A pane taller than the names need draws fewer rows than it has, and the rows
        below the last are cells nothing was drawn into. `panel._write` leaves them to
        tmux; this says a click on one reaches no tab."""
        rows = self._strip(200, 3)
        for r in range(len(rows), 6):
            for c in range(0, 200, 7):
                with self.subTest(row=r, col=c):
                    self.assertIsNone(slots.TABS.switch_to(r, c))

    def test_the_counts_land_on_the_rows_the_names_they_stand_for_are_next_to(self):
        """A leading `+N` says the run starts partway into the list, so it belongs beside
        the FIRST tab drawn; a trailing one says the list goes on past the last, so it
        belongs after it. On a grown strip those are two different rows, and each is
        clickable where it was drawn.

        120 columns cuts these fifteen into three pages. Two rows draw the first two of
        them for a mark on either, so the only count is a trailing one on the second row;
        a mark on the third page draws that page alone, with a leading count on it.
        """
        rows = self._strip(120, 2, here=NAMES[8])
        self.assertEqual(len(rows), 2)
        first, second = (tui.strip_ansi(r) for r in rows)
        self.assertNotIn("+", first, f"a leading count on a run that starts at 0: {first!r}")
        self.assertIn("+", second, f"the names past this run went uncounted: {second!r}")
        self.assertTrue(slots.TABS.more_at(1, second.rindex("+")),
                        "the trailing count is inert on the row it was drawn on")
        self.assertFalse(slots.TABS.more_at(0, second.rindex("+")),
                         "the count answers for a row it was not drawn on")

        tail = self._strip(120, 2, here=NAMES[13])
        self.assertEqual(len(tail), 1)
        only = tui.strip_ansi(tail[0])
        self.assertIn("+", only, f"a run that starts mid-list went uncounted: {only!r}")
        self.assertTrue(slots.TABS.more_at(0, only.index("+")),
                        "the leading count is inert")

    def test_a_grown_strip_that_holds_the_whole_list_still_offers_the_plus(self):
        """The affordance is drawn where the whole list is on the strip — one row's worth
        on the rung that fits them all, or a grown strip whose rows hold them. An operator
        who widened their pane and lost the `+` would have been given rows and charged a
        button for them."""
        rows = self._strip(160, 2, note=slots.ADD_CHAT)
        self.assertEqual(len(rows), 2)
        last = tui.strip_ansi(rows[-1])
        self.assertTrue(last.rstrip().endswith(slots.ADD_CHAT), repr(last))
        self.assertTrue(slots.TABS.add_at(1, last.rstrip().rindex(slots.ADD_CHAT)))

    def test_a_plus_and_a_count_are_never_on_one_strip(self):
        """`_Tabs.add_at`'s structural promise, restated for a strip that has rows: the
        two fields that both begin with `+` must never be on screen together, and a
        second ROW is on screen with the first."""
        for cols in range(20, 300, 3):
            for rows in (1, 2, 3):
                with self.subTest(cols=cols, rows=rows):
                    drawn = self._strip(cols, rows, note=slots.ADD_CHAT)
                    plain = "\n".join(tui.strip_ansi(r) for r in drawn)
                    counts = [f for f in plain.replace("\n", " ").split()
                              if f.startswith("+") and f[1:].isdigit()]
                    if slots.ADD_CHAT in plain.split():
                        self.assertEqual(counts, [],
                                         f"a `+` shares a strip with a count: {plain!r}")

    def test_a_run_whose_SECOND_row_would_overflow_gives_the_rung_up(self):
        """The ladder's refusal is measured on every row, not on the first. `slots._cuts`
        puts at least one field on a page whatever it costs, so a name wider than the whole
        row composes a page that overflows — and a run can have that page second, where a
        check on the first row alone would never look.

        The list is built so that row one fits and row two cannot: a short name and a name
        wider than the pane, in a width where the short one is a page of its own.
        """
        wide = "w" * 90
        rows = self._strip(40, 2, names=["api.1", wide], here="api.1")
        for line in rows:
            self.assertLessEqual(tui.width(line), 40, repr(line))
        self.assertNotIn(wide, "".join(tui.strip_ansi(r) for r in rows))

    def test_a_run_that_starts_mid_list_carries_no_add_chat_affordance(self):
        """`_Tabs.add_at`'s structural promise, at the one shape that can break it.

        The affordance rides only a run holding the WHOLE list — neither count drawn. A run
        that reaches the END of the list but starts partway into it has no trailing count
        and a leading one, and that is the state where "is there a `+N` anywhere" and "is
        there a trailing field" stop being the same question. 120 columns cuts these
        fifteen into three pages, so a mark on the last draws that page alone with `+13`
        in front of it and nothing after it.
        """
        rows = self._strip(120, 1, here=NAMES[13], note=slots.ADD_CHAT)
        drawn = tui.strip_ansi(rows[0])
        self.assertIn("+13", drawn, f"this is not the run the case is about: {drawn!r}")
        self.assertFalse(drawn.rstrip().endswith(slots.ADD_CHAT),
                         f"a `+` rode a run that starts mid-list: {drawn!r}")
        self.assertEqual({c for c in range(120) if slots.TABS.add_at(0, c)}, set(),
                         "the affordance claims a cell on a run it is not drawn on")

    def test_no_cell_that_stands_for_names_off_the_strip_also_makes_a_chat(self):
        """*Two questions, three methods, and a cell that is neither answers no to both* —
        held over every width and every row count rather than at the one width the rung
        happens to be tested at.

        **The harm is `EVENT_KINDS`' "fires wrongly" and it is silent**: `_bar_events` asks
        `switch_to` first and `add_at` second, so a `+9` that also answered `add_at` would
        MAKE A CHAT where the operator asked to see the ones they have — and the row would
        look exactly the same either way, because both fields are drawn as a `+`.
        """
        for cols in range(20, 300, 3):
            for rows in (1, 2, 3):
                for here in (NAMES[0], NAMES[5], NAMES[13]):
                    self._strip(cols, rows, here=here, note=slots.ADD_CHAT)
                    both = {(r, c) for r in range(rows) for c in range(cols)
                            if slots.TABS.more_at(r, c) and slots.TABS.add_at(r, c)}
                    with self.subTest(cols=cols, rows=rows, here=here):
                        self.assertEqual(both, set(),
                                         "a count cell also makes a chat")

    def test_no_row_of_a_grown_strip_is_wider_than_the_pane(self):
        """The ladder's own refusal, applied to every row rather than the first: a run
        whose second page overflows is a strip drawing part of a name on a row nothing
        else would have looked at."""
        for cols in range(0, 300):
            for rows in (1, 2, 3):
                for drawn in self._strip(cols, rows, note=slots.ADD_CHAT):
                    with self.subTest(cols=cols, rows=rows):
                        self.assertLessEqual(tui.width(drawn), cols, repr(drawn))

    def test_nothing_a_grown_strip_draws_is_outside_ascii(self):
        """`test_frame_bars`' own property, asked of every row rather than of the one.
        A click on this strip is resolved by COLUMN, so a glyph a terminal may draw two
        cells wide moves every field after it and the operator presses `fleet` and lands
        on `default`."""
        with mock.patch.dict(config.FRAME, {"look": {"rules": "visible"}}):
            for cols in range(0, 261):
                for rows in (1, 2, 3):
                    for drawn in self._strip(cols, rows, note=slots.ADD_CHAT):
                        with self.subTest(cols=cols, rows=rows):
                            self.assertTrue(tui.strip_ansi(drawn).isascii(), repr(drawn))

    def test_switching_to_a_tab_that_is_drawn_leaves_every_row_where_it_was(self):
        """The property the cut exists for (`slots._cuts`), which a strip that grew has to
        keep: the run of pages is a function of the names, the width and the row count, so
        pressing a drawn tab redraws the same run with only the `*` moved. A run that
        moved would put a different name under the cell the operator just pressed."""
        def shape(drawn):
            """The rows with the mark taken out — every cell in its own column, and the
            one thing switching is allowed to change removed.

            The two entries of `slots._BAR_MARK` are one cell each and neither a name nor
            a count can contain a `*` (`workspace.valid_name`, `chats.ID_RE`), so blanking
            it is exact rather than approximate. Comparing the map instead would not work:
            `_Tabs.switch_to` answers `None` for the tab you are on, so the marked name
            drops out of it and every switch would look like a move.
            """
            return [tui.strip_ansi(line).replace(slots._BAR_MARK[0], slots._BAR_MARK[1])
                    for line in drawn]

        for rows in (1, 2, 3):
            for cols in (100, 120, 160, 200, 240):
                before = self._strip(cols, rows)
                drawn = {slots.TABS.switch_to(r, c)
                         for r in range(rows) for c in range(cols)} - {None}
                for name in drawn:
                    with self.subTest(rows=rows, cols=cols, name=name):
                        self.assertEqual(
                            shape(self._strip(cols, rows, here=name)), shape(before),
                            "switching to a drawn tab moved the strip under the pointer")

    def test_a_strip_that_shrinks_back_to_one_row_forgets_the_rows_it_lost(self):
        """`_Viewport.blank`'s half, one axis over and one row down. A strip that kept its
        second row's map through a resize would switch to a tab the operator can see is
        not on screen."""
        self._strip(160, 2)
        self.assertTrue({slots.TABS.switch_to(1, c) for c in range(160)} - {None})
        self._strip(160, 1)
        self.assertEqual({slots.TABS.switch_to(1, c) for c in range(160)}, {None})


class AStripGrowsOnlyIntoRowsTheHarnessCanSpare(unittest.TestCase):
    """`layout.slot_sizes` and `layout._grown` — arithmetic, no plane and no tmux.

    #740 settled the competition for rows against the harness once. A strip growing is a
    strip taking rows off it, so what it may take is exactly what the harness has above
    `layout.HARNESS_MIN_ROWS` and nothing else.
    """

    def setUp(self):
        self.enterContext(mock.patch.dict(config.FRAME, _placed()))

    def _sizes(self, window_rows, bar_rows=None, slots_=None):
        return layout.slot_sizes(slots_ or SLOTS, window_rows=window_rows,
                                 content_rows=4, bar_rows=bar_rows)

    def test_a_frame_with_no_strip_that_overflows_is_sized_exactly_as_before(self):
        """The default, and the argument against the fixed two-row launch: a plane whose
        names fit never loses a row. `bar_rows=None` and an empty map are the same
        answer, because a map naming nothing to grow is a frame with nothing to grow."""
        self.assertEqual(self._sizes(50), self._sizes(50, {}))
        self.assertEqual(self._sizes(50, {"chats": 1, "workspaces": 1}),
                         self._sizes(50))
        self.assertEqual(self._sizes(50)["chats"], 1)

    def test_a_strip_that_overflows_is_given_the_rows_it_asked_for(self):
        got = self._sizes(50, {"chats": 3, "workspaces": 2})
        self.assertEqual(got["chats"], 3)
        self.assertEqual(got["workspaces"], 2)

    def test_the_harness_keeps_its_floor_however_many_rows_a_strip_wants(self):
        """The bound, asked of `harness_rows` rather than of the growth: a strip that grew
        past this would be `resize-pane -y` taking rows out of the one pane the frame is
        drawn around, which tmux grants without complaint."""
        for window_rows in range(14, 60):
            with self.subTest(window_rows=window_rows):
                got = self._sizes(window_rows, {"chats": 3, "workspaces": 3})
                ungrown = self._sizes(window_rows)
                harness = layout.harness_rows(got, window_rows=window_rows)
                if layout.harness_rows(ungrown,
                                       window_rows=window_rows) >= layout.HARNESS_MIN_ROWS:
                    self.assertGreaterEqual(harness, layout.HARNESS_MIN_ROWS)
                self.assertLessEqual(got["chats"], 3)

    def test_a_window_with_nothing_spare_grows_nothing_at_all(self):
        """And it degrades in silence, which is `visible_slots`' own manner one rung up:
        the strip is drawn, at the height it always had."""
        got = self._sizes(21, {"chats": 3, "workspaces": 3})
        self.assertEqual(got["chats"], 1)
        self.assertEqual(got["workspaces"], 1)

    def test_two_strips_share_a_short_budget_rather_than_one_taking_it_all(self):
        """A row at a time in split order. Handing the whole budget to whichever was split
        first would leave the other looking broken for a reason nothing on screen
        explains."""
        rows = next(r for r in range(14, 60)
                    if layout.harness_rows(self._sizes(r), window_rows=r)
                    == layout.HARNESS_MIN_ROWS + 1)
        got = self._sizes(rows, {"chats": 3, "workspaces": 3})
        self.assertEqual(sorted([got["chats"], got["workspaces"]]), [1, 2])
        got = self._sizes(rows + 1, {"chats": 3, "workspaces": 3})
        self.assertEqual([got["chats"], got["workspaces"]], [2, 2])

    def test_the_repo_table_is_not_what_pays_for_a_grown_strip(self):
        """`layout._DROP_ORDER` gives the bars up BEFORE `repos` when rows run short, so
        they cannot be fed from it when rows are plentiful. The table keeps its content's
        height and the harness's own slack is what is spent."""
        ungrown = self._sizes(50)
        grown = self._sizes(50, {"chats": 3, "workspaces": 3})
        self.assertEqual(grown["repos"], ungrown["repos"])
        self.assertEqual(layout.harness_rows(grown, window_rows=50),
                         layout.harness_rows(ungrown, window_rows=50) - 4)

    def test_a_name_nothing_is_drawing_is_dropped_rather_than_grown(self):
        """The filter `slot_sizes` already applies to its own list, applied to this one:
        a slot that is not being drawn is not a pane to grow."""
        got = self._sizes(50, {"chats": 3, "acme.metrics": 4})
        self.assertNotIn("acme.metrics", got)
        self.assertEqual(got["chats"], 3)

    def test_a_want_below_the_height_a_strip_already_has_never_shrinks_it(self):
        """This grows and never trims. A plane that pinned its strip to three rows in its
        own `[[frame.component]]` table asked for three rows (#687); a measurement saying
        its names fit on one is not that operator changing their mind.

        **Asked BESIDE a strip that is growing**, which is what makes the shortfall test in
        `layout._grown` observable at all: the deal runs for as many rounds as the growing
        strip needs, so a round that stopped asking whether this one still wants a row
        would hand it one on every one of them.
        """
        with mock.patch.dict(config.FRAME, _placed(chats=3)):
            got = layout.slot_sizes(SLOTS, window_rows=50, content_rows=4,
                                    bar_rows={"chats": 1})
            self.assertEqual(got["chats"], 3)
            grown = layout.slot_sizes(SLOTS, window_rows=50, content_rows=4,
                                      bar_rows={"chats": 5})
            self.assertEqual(grown["chats"], 5)
            beside = layout.slot_sizes(SLOTS, window_rows=50, content_rows=4,
                                       bar_rows={"chats": 1, "workspaces": 3})
        self.assertEqual((beside["chats"], beside["workspaces"]), (3, 3),
                         "a strip that wanted nothing was dealt the rows its neighbour "
                         "asked for")


class TheLauncherSplitsThePaneTheStripAskedFor(PersonaIso, unittest.TestCase):
    """The seam end to end, with a real plane under it and no tmux.

    `commands_frame._slot_sizes` is the boundary where a plane becomes a pane height, and
    `layout.panel_argvs` is where a pane height becomes `split-window -l`. Both launch
    paths and the `window-resized` recompute go through the first.
    """

    def setUp(self):
        super().setUp()
        for name in NAMES:
            (config.WORKSPACES_DIR / name).mkdir(parents=True, exist_ok=True)
        self.enterContext(mock.patch.dict(config.FRAME, _placed()))

    def test_a_frame_nobody_has_pressed_the_key_in_launches_one_row_deep(self):
        """**The default #880 changed, at the widths where the old one was visible.**
        `bar_rows_wanted` still measures what these fifteen names need — three rows at 160
        columns — and the boundary hands it a ceiling of `layout.BAR_ROWS_DEFAULT` instead,
        so a plane with many names no longer launches two rows deep and two rows shorter in
        the harness. What it launches with instead is `+N`, which is clickable and opens
        the palette; a collapsed strip is one press from the complete list.
        """
        for cols in (100, 160, 200, 360):
            with self.subTest(cols=cols):
                got = commands_frame._slot_sizes("f-1", SLOTS, window_rows=50,
                                                 pane_cols=cols, order=SLOTS,
                                                 window_cols=cols)
                self.assertEqual(got["workspaces"], 1, got)

    def test_the_boundary_asks_the_strip_and_hands_the_answer_to_the_arithmetic(self):
        """With the ceiling raised, which since #880 is the only way to reach a strip
        taller than one row: `state.bar_rows` is what `layout.bar_rows_cap` reads and the
        boundary carries down."""
        state.record_bar_rows("f-1", 3)
        got = commands_frame._slot_sizes("f-1", SLOTS, window_rows=50, pane_cols=200,
                                         order=SLOTS, window_cols=160)
        self.assertEqual(got["workspaces"], 3, got)

    def test_a_raised_ceiling_is_not_a_height_and_a_wide_window_stays_at_one_row(self):
        """**A cap and not a demand.** The key raises what a strip MAY grow to; what it
        does grow to is still what its names need, so a pane wide enough for every name
        stays one row however high the ceiling has been cycled.

        320 columns and not 262: every workspace tab reserves `slots.TAB_COUNT_W` for its
        chat count (#880), so these fifteen names need 307 columns for one row rather than
        262. It was 352 while that field was six cells wide; #903 narrowed it to three."""
        state.record_bar_rows("f-1", 3)
        got = commands_frame._slot_sizes("f-1", SLOTS, window_rows=50, pane_cols=320,
                                         order=SLOTS, window_cols=320)
        self.assertEqual(got["workspaces"], 1, got)

    def test_the_height_the_strip_asked_for_reaches_split_window(self):
        """The measurement #687 filed for the pinned height, asked for the measured one:
        a number that stopped at `slot_sizes` would be the inert value the whole seam is
        written against.

        The ceiling is raised first, because since #880 a launch is one row deep and the
        seam this is about would then be carrying a number no press had changed."""
        state.record_bar_rows("f-1", 3)
        sizes = commands_frame._launch_sizes("f-1", SLOTS, window_cols=160,
                                             window_rows=50)
        argvs = layout.panel_argvs(slots=SLOTS, session="s", socket="k",
                                   harness_pane="%1", sizes=sizes)
        bar, = [a for a in argvs if "workspaces" in a]
        self.assertEqual(bar[bar.index("-l") + 1], "3")

    def test_the_strips_own_pane_width_is_what_it_is_measured_at(self):
        """#500 one slot over. A strip split AFTER the sidebar is carved out of a pane the
        sidebar has already narrowed by 23 columns, so measuring it at the window's width
        would size it for names its own pane cannot draw."""
        inset = ["right", "top", "workspaces", "bottom", "repos"]
        self.assertEqual(layout.pane_cols(inset, "workspaces", window_cols=160), 137)
        self.assertEqual(layout.pane_cols(SLOTS, "workspaces", window_cols=160), 160)
        state.record_bar_rows("f-1", 3)
        # 320 and not 360, and the number moved with the count field: these fifteen names
        # need 307 columns for one row (#903 took the field from six cells to three), so
        # the window has to be wide enough for the strip at full width and NOT wide enough
        # for it 23 columns in. At 360 both fit on one row and the case measured nothing.
        wide = commands_frame._slot_sizes("f-1", SLOTS, window_rows=50, pane_cols=200,
                                          order=SLOTS, window_cols=320)
        narrow = commands_frame._slot_sizes(
            "f-1", inset, window_rows=50, pane_cols=200, order=inset,
            window_cols=320)
        self.assertEqual(wide["workspaces"], 1)
        self.assertEqual(narrow["workspaces"], 2,
                         "the strip was sized from the window rather than its pane")

    def test_a_pane_width_is_asked_for_by_either_of_a_components_two_names(self):
        """`layout._key`'s contract, kept by the walk that generalised out of
        `repos_cols`: a component id and its committed slot name are one component
        (`builtins.component_id`), so they must reach one answer and not two."""
        inset = ["right", "top", "workspaces", "bottom", "repos"]
        self.assertEqual(layout.pane_cols(inset, "identity", window_cols=160),
                         layout.pane_cols(inset, "top", window_cols=160))
        self.assertEqual(layout.pane_cols(inset, "repos", window_cols=160),
                         layout.repos_cols(inset, window_cols=160))

    def test_the_real_strips_go_through_the_same_seam(self):
        """`slots.BARS` is charter's own two entries and not a fixture: the chat strip
        reads `chats.roster` and the workspace strip reads `switch.workspaces`, and both
        are what `bar_rows_wanted` measures."""
        self.assertEqual(sorted(slots.BARS), ["chats", "workspaces"])
        for i in range(12):
            _plant(f"api.{i}", workspace="api")
        with mock.patch.dict(config.FRAME, {"components": ()}):
            self.assertGreater(
                slots.bar_rows_wanted("api.0", "chats", pane_cols=60,
                                      cap=layout.BAR_MAX_ROWS), 1)


class TheFloorKeepsTheHeightItWasBornWith(unittest.TestCase):
    """What tmux 3.2 gets, stated rather than assumed.

    **The launch has no version gate.** `layout.panel_argvs` writes `split-window -l <n>`
    and that has worked on every tmux charter supports, so a strip whose names overflow is
    born the right height at the floor exactly as it is at 3.7c.

    **What the floor loses is the RE-computation.** `window-resized` is a hook tmux 3.3
    added; on 3.2 `set-hook -w window-resized` answers `invalid option`, rc 1 — measured
    here on `~/.local/share/charter-testing/tmux-3.2` and recorded by
    `tmuxctl.RESIZE_HOOK_FLOOR`. So dragging the divider on a 3.2 frame re-lays out
    nothing, and the strip keeps the height it was born with until the frame is
    relaunched. That is a degradation and an honest one: it is the band
    `tmuxctl.below_resize_floor_message` already tells the operator about, and it is what
    #829's decision took over a fixed two-row launch that would cost every plane a row.
    """

    def test_the_resize_hook_floor_sits_above_the_floor_charter_supports(self):
        self.assertGreater(tmuxctl.RESIZE_HOOK_FLOOR, tmuxctl.FLOOR)
        self.assertEqual(tmuxctl.FLOOR, (3, 2))
        self.assertEqual(tmuxctl.RESIZE_HOOK_FLOOR, (3, 3))

    def test_below_that_floor_nothing_is_armed_and_nothing_is_written(self):
        """`_install_resize_hook`'s gate, which is why a 3.2 frame never recomputes. Not
        an error and not a warning on the launch path — the operator is told once, by
        `below_resize_floor_message`, and the frame comes up."""
        wrote = []
        with mock.patch.object(tmuxctl, "version", return_value=(3, 2)), \
                mock.patch.object(tmuxctl, "run", side_effect=lambda *a, **k: wrote.append(a)):
            commands_frame._install_resize_hook(
                "sock", harness_pane="%1", panes={"chats": "%2"}, v=(3, 2),
                env=None, fid="f-1")
        self.assertEqual(wrote, [])

    def test_the_operator_is_told_what_the_band_costs_them(self):
        """The sentence the degradation lives in, read rather than spelled — a strip that
        stops re-sizing on a resize is exactly the thing this message is about."""
        said = tmuxctl.below_resize_hook_message((3, 2))
        self.assertIn("3.3", said)
        self.assertIn("window-resized", said)
        self.assertIn("charter frame-resize", said,
                      "the band names no way out, so a strip stuck at one row is final")

    def test_a_launch_at_the_floor_still_splits_the_pane_the_names_need(self):
        """The half that is NOT gated. `panel_argvs` is version-free, so the strip is born
        at its measured height on 3.2 as on 3.7c — which makes the floor's loss "it does
        not follow a resize", never "it is one row forever"."""
        sizes = layout.slot_sizes(SLOTS, window_rows=50, content_rows=4,
                                  bar_rows={"workspaces": 2})
        argvs = layout.panel_argvs(slots=SLOTS, session="s", socket="k",
                                   harness_pane="%1", sizes=sizes)
        bar, = [a for a in argvs if "workspaces" in a]
        self.assertEqual(bar[bar.index("-l") + 1], "2")
        self.assertNotIn("3.3", " ".join(bar))


if __name__ == "__main__":
    unittest.main()
