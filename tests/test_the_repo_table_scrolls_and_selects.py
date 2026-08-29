"""The `repos` pane takes the wheel and the pointer — #607's first consumer.

`frame/events.py` shipped the whole path a release ago and nothing used it: `component
.EVENT_KINDS` was a closed vocabulary, `DELIVERED` named five kinds charter could carry,
and every one of charter's own six components declared `events = ()`. So a provider could
declare `scroll`, pass validation, and be the only thing on the machine receiving anything.
`repos` declares `scroll` and `click` now, which is the same sequencing §4b asks for one
contract over: charter's own panel is the worked example, not the exception.

**What is asserted here is a VALUE wherever a value is what the code computes.** The
clamping in `slots._Viewport` and `slots._scroll_limit` is exactly the place a test that
computes its expectation from the thing under test survives every mutation — `_scroll_limit
(20, 14)` is asserted to be `7` and not to be "positive", the window at offset 1 is compared
against a ranking this module works out for itself, and the attention row's detail is
compared against the sentence it should read rather than against "contains the name".

**The scroll that does NOTHING is tested as hard as the one that does.** The `repos` pane
is sized to its own content (`layout.repos_rows`), so the ordinary plane is one where every
repo already fits and the wheel must move nothing, repaint nothing and cost nothing — an
accident that happened to cancel would be indistinguishable from a design that refuses.
"""

from __future__ import annotations

import os
import sys
import unittest
from unittest import mock

from charter import statusline, tui
from charter.frame import (builtin_actions, builtins, chrome, events, gather, overlay,
                           panel, slots, state)
from charter.frame import action as faction

from tests._isolation import PersonaIso

FID = "f-scroll"

#: A pane wide enough for the table (`statusline._LEFT_W` is 95) with room to spare, so
#: nothing here is accidentally testing the too-narrow refusal.
WIDE = 200


def _row(name, *, branch="main", dirty=False, ahead=0, behind=0, ci=None, change=None,
         sigil="", current=False, repo=None, worktree_count=0) -> dict:
    """A `gather`-cache-shaped row — the fields `gather._entry` writes."""
    d = {"name": name, "branch": branch, "dirty": dirty, "tracked_dirty": False,
         "ahead": ahead, "behind": behind, "ci": ci, "change": change, "sigil": sigil,
         "current": current, "worktree_count": worktree_count}
    if repo is not None:
        d["repo"] = repo
    return d


def _data(repos, *, current=None, worktrees=()) -> dict:
    return {"gathered_at": 0.0, "workspace": "w", "current_repo": current,
            "repos": list(repos), "worktrees": list(worktrees)}


def _names(lines) -> list[str | None]:
    return [ln.repo for ln in lines]


class TheOffsetIsBoundedByTheDataAndThePane(unittest.TestCase):
    """`slots._scroll_limit` — how far the window may move, and when it may not at all.

    Every case here is a number this function computes, asserted as a number. A test that
    only asked "can this scroll?" would go green over a clamp deleted, because the answer
    to that question is `True` for both `20 - 13` and `20`.
    """

    def test_a_pane_tall_enough_for_its_repos_does_not_scroll_at_all(self):
        """**The tested nothing.** `layout.repos_rows` sizes this pane to its content, so
        the ordinary plane has exactly as many rows as repos and the wheel must be inert.
        Asserted at the boundary and one either side of it, because the guard that makes it
        true is a comparison and an off-by-one there is a table that slides by one row on
        every plane charter ships."""
        self.assertEqual(slots._scroll_limit(6, 6), 0)
        self.assertEqual(slots._scroll_limit(5, 6), 0)
        self.assertEqual(slots._scroll_limit(1, 14), 0)

    def test_the_limit_leaves_the_overflow_line_its_row(self):
        """`_table_lines` reserves `…(+N more)` OUT of the budget rather than trimming it
        off the end, so a 14-row pane draws 13 repo rows — and the last repo is reachable
        only if this subtracts the same row. Twenty repos, thirteen rows of them, so the
        window's last position is 7; at 7 it shows ranks 7..19 and nothing is out of
        reach."""
        self.assertEqual(slots._scroll_limit(20, 14), 7)
        self.assertEqual(slots._scroll_limit(7, 6), 2)

    def test_a_one_row_pane_is_all_overflow_line_and_does_not_scroll(self):
        """A budget of exactly one is spent on `…(+N more)` — "there is more here than
        fits" outranks "here is an arbitrary one of them" — so every offset over such a
        table draws the identical line. A limit taken from the repo count alone would let
        the wheel repaint that one line forty times, each repaint byte-identical."""
        self.assertEqual(slots._scroll_limit(40, 1), 0)
        self.assertEqual(slots._scroll_limit(40, 0), 0)


class TheViewportRemembersOneThingAndClampsIt(unittest.TestCase):
    """`slots._Viewport` — the offset the handler moves and the renderer settles."""

    def setUp(self) -> None:
        self.v = slots._Viewport()

    def test_a_scroll_that_changes_nothing_answers_falsy(self):
        """The answer IS the repaint decision (§4f), so "nothing moved" has to be a
        different value from "moved", not merely a different offset. With no paint behind
        it the limit is zero and every notch is refused."""
        self.assertFalse(self.v.move(1))
        self.assertFalse(self.v.move(-1))
        self.assertEqual(self.v.offset, 0)

    def test_it_stops_at_the_top_and_at_the_bottom(self):
        self.v.settle(3)
        self.assertTrue(self.v.move(1))
        self.assertEqual(self.v.offset, 1)
        self.assertTrue(self.v.move(99))
        self.assertEqual(self.v.offset, 3, "it went past the last window there is")
        self.assertFalse(self.v.move(1), "a notch at the bottom repainted for nothing")
        self.assertTrue(self.v.move(-99))
        self.assertEqual(self.v.offset, 0)
        self.assertFalse(self.v.move(-1), "a notch at the top repainted for nothing")

    def test_an_offset_that_outlives_a_shrunken_list_is_clamped_down(self):
        """A repo is removed while the table is scrolled to the bottom. The stored offset
        is now past the end of a list that no longer has those rows, and the NEXT paint is
        where that is answered: `settle` takes the bound the renderer just computed and
        hands back the largest window this list actually has.

        Asserted as the value 2 rather than as "not 7": an unclamped offset is still an
        integer and still renders *something* — an empty table the operator has to guess
        their way back out of — so "it changed" is not the claim."""
        self.v.settle(7)
        self.v.move(7)
        self.assertEqual(self.v.offset, 7)
        self.assertEqual(self.v.settle(2), 2)
        self.assertEqual(self.v.offset, 2)

    def test_settling_a_pane_with_nothing_to_scroll_puts_it_back_at_the_top(self):
        """The other end of the same clamp: a resize that starves the pane, or a plane
        whose repos were all removed, answers a limit of 0 and the window goes home rather
        than staying at an offset over a table that is not there."""
        self.v.settle(4)
        self.v.move(3)
        self.assertEqual(self.v.settle(0), 0)
        self.assertFalse(self.v.move(1))

    def test_a_row_the_pane_never_drew_is_nobody(self):
        """Out of range answers `None` rather than raising or counting from the end.
        Python's own indexing would report the LAST row for `-1`, which is the one wrong
        answer available here — an event carrying a negative row means charter's own
        arithmetic was wrong, and a plausible answer would hide it."""
        self.v.publish((None, "a", "b"))
        self.assertEqual(self.v.repo_at(1), "a")
        self.assertIsNone(self.v.repo_at(0))
        self.assertIsNone(self.v.repo_at(3))
        self.assertIsNone(self.v.repo_at(-1))


class TheWindowMovesAlongTheRanking(PersonaIso, unittest.TestCase):
    """What the table actually draws at each offset.

    `statusline._pick_rows` ranks the whole list and slices it; the offset moves that slice.
    The expectations below are worked out from the ranking's own documented order — where
    you are, then dirty, then ahead/behind, then CI, then a change, then cache order — and
    never from a second call to the function under test.
    """

    def _lines(self, data, budget, **kw):
        return slots._table_lines(data, WIDE, budget, **kw)

    def test_offset_zero_is_the_table_that_was_there_before(self):
        """The compatibility claim, and the only one that matters for every plane that
        never scrolls: `[0:budget]` is the slice `_pick_rows` has always taken, so an
        unscrolled table is byte-for-byte what it was."""
        data = _data([_row(f"r{i}") for i in range(20)])
        self.assertEqual([ln.text for ln in self._lines(data, 14)],
                         [ln.text for ln in self._lines(data, 14, offset=0)])

    def test_one_notch_drops_the_top_rank_and_reveals_the_next(self):
        """Five repos, a four-row budget: three rows of repos and the overflow line. The
        ranking is `r1` (dirty) first, then `r0`/`r2`/`r3`/`r4` in cache order, so offset 0
        shows r1 + r0 + r2 and offset 1 shows r0 + r2 + r3 — the highest rank leaves and the
        next-highest hidden one arrives.

        Display order is still the cache's, which is why the names come back sorted that
        way rather than in the order they were ranked."""
        data = _data([_row("r0"), _row("r1", dirty=True), _row("r2"), _row("r3"),
                      _row("r4")])
        self.assertEqual(_names(self._lines(data, 4)), ["r0", "r1", "r2", None])
        self.assertEqual(_names(self._lines(data, 4, offset=1)), ["r0", "r2", "r3", None])
        self.assertEqual(_names(self._lines(data, 4, offset=2)), ["r2", "r3", "r4", None])

    def test_the_last_window_reaches_the_lowest_ranked_repo(self):
        """The point of subtracting the overflow line's row in `_scroll_limit`: at the
        limit, the bottom of the ranking is on screen. `r4` is last in cache order with
        nothing on it, so it is the last rank, and it must be reachable."""
        repos = [_row(f"r{i}") for i in range(5)]
        data = _data(repos)
        limit = slots._scroll_limit(5, 4)
        self.assertEqual(limit, 2)
        self.assertIn("r4", _names(self._lines(data, 4, offset=limit)))

    def test_the_overflow_note_still_counts_every_hidden_repo(self):
        """It needs no arithmetic of its own at any offset: what is hidden is "every key
        not in *show*", which is true wherever the window is. Twenty repos, thirteen shown,
        seven hidden — at the top and at the bottom alike."""
        data = _data([_row(f"r{i}") for i in range(20)])
        for offset in (0, 3, 7):
            note = tui.strip_ansi(self._lines(data, 14, offset=offset)[-1].text)
            self.assertIn("(+7 more", note)

    def test_the_overflow_line_is_about_no_repo(self):
        """It stands for the rows that are NOT on screen. Selecting one of them because
        the operator clicked the line saying they exist would be charter answering a
        question nobody asked."""
        data = _data([_row(f"r{i}") for i in range(20)])
        self.assertIsNone(self._lines(data, 14)[-1].repo)

    def test_a_piece_row_belongs_to_the_repo_it_is_a_piece_of(self):
        """One namespace, not two. `gather` writes `repo` on every worktree row, so a
        click on a nested row selects the clone — and a piece named after a sibling clone
        cannot come to mean that clone's selection."""
        data = _data([_row("solo")],
                     worktrees=[_row("bit", repo="solo"), _row("other", repo="solo")])
        self.assertEqual(_names(self._lines(data, 6)), ["solo", "solo", "solo"])


class TheSelectedRowIsDrawnAsSelected(PersonaIso, unittest.TestCase):
    """The highlight — `frame/chrome.reverse`, the same call the persona column makes."""

    def _lines(self, **kw):
        data = _data([_row("alpha"), _row("beta")])
        return slots._table_lines(data, WIDE, 6, **kw)

    def test_the_selected_row_is_reversed_and_no_other_row_is(self):
        lines = self._lines(selected="beta")
        self.assertNotIn("\x1b[7m", lines[0].text, "an unselected row was highlighted")
        self.assertIn("\x1b[7m", lines[1].text)

    def test_the_highlight_survives_the_rows_own_resets(self):
        """The defect `chrome.reverse` exists for: every coloured span in a repo row ends
        in `statusline._R`, and a `\\x1b[7m` wrapped naively round the outside dies at the
        first one. So reverse has to be re-asserted after each — measured here as "more
        than one", which a naive wrapper cannot produce."""
        row = self._lines(selected="alpha")[0].text
        self.assertGreater(row.count("\x1b[7m"), 1)

    def test_the_highlight_reaches_the_pane_s_last_column(self):
        """A highlight that stopped at the text would mark two words rather than a row.
        `chrome.reverse` fills to exactly the width the renderer measured — never one
        more, which `chrome.fill`'s own measurement shows shears the pane."""
        row = self._lines(selected="alpha")[0].text
        self.assertEqual(tui.width(row), WIDE)

    def test_the_row_says_the_same_thing_selected_as_unselected(self):
        """The selection is paint and never content: a highlighted row must not gain,
        lose or move a single visible cell, or the operator's eye has to re-read the table
        every time they point at it."""
        self.assertEqual(chrome.plain(self._lines(selected="alpha")[0].text),
                         chrome.plain(self._lines()[0].text))

    def test_a_selection_naming_no_row_highlights_nothing(self):
        """What a selection left over from a repo that has since gone degrades to — and
        the same answer a hand-edited `selection` file gets, which is why nothing validates
        that file: the only thing charter does with the string is this equality."""
        self.assertEqual([ln.text for ln in self._lines(selected="gone")],
                         [ln.text for ln in self._lines()])


class TheHandlerIsWhatTheContractSays(PersonaIso, unittest.TestCase):
    """`builtins._repos_events` — what a pointer event does and, mostly, does not do."""

    def setUp(self) -> None:
        super().setUp()
        state.frame_dir(FID, create=True)
        self.on_event = builtins.build(FID).get("repos").on_event
        slots.VIEWPORT.forget()

    def tearDown(self) -> None:
        slots.VIEWPORT.forget()
        super().tearDown()

    def _click(self, row, *, pressed=True, name="left"):
        return self.on_event(overlay.Event(overlay.CLICK, name, row=row, pressed=pressed))

    def test_the_repo_table_declares_the_two_kinds_charter_can_carry(self):
        """Declared TOGETHER because `events.Dispatcher.open` asks the terminal for one
        request that serves both — a component declaring only one would still pay
        `overlay.MOUSE_ON`'s whole price."""
        c = builtins.build(FID).get("repos")
        self.assertEqual(c.events, ("scroll", "click"))
        self.assertEqual(events.wanted(c), ("scroll", "click"))

    def test_a_click_on_a_drawn_row_selects_that_repo(self):
        slots.VIEWPORT.publish((None, "alpha", "beta"))
        self.assertTrue(self._click(2))
        self.assertEqual(state.selection(FID), "beta")

    def test_a_click_on_the_heading_selects_nothing(self):
        """Row 0 of the pane is `▪ repos N`, which belongs to no repo."""
        slots.VIEWPORT.publish((None, "alpha"))
        self.assertFalse(self._click(0))
        self.assertIsNone(state.selection(FID))

    def test_a_click_below_the_last_row_selects_nothing(self):
        slots.VIEWPORT.publish((None, "alpha"))
        self.assertFalse(self._click(9))
        self.assertIsNone(state.selection(FID))

    def test_the_press_selects_and_the_release_does_not(self):
        """`component.EVENT_KINDS`: a click arrives twice and either half can arrive
        alone — a drag begun on a pane border delivers exactly one release. Acting on the
        press is acting on where the operator pointed; a drag that began elsewhere and
        released over this pane never pointed here."""
        slots.VIEWPORT.publish((None, "alpha"))
        self.assertFalse(self._click(1, pressed=False))
        self.assertIsNone(state.selection(FID))
        self.assertTrue(self._click(1, pressed=True))
        self.assertEqual(state.selection(FID), "alpha")

    def test_only_the_left_button_selects(self):
        """Middle-click is paste on every terminal an operator has ever used and
        right-click opens their emulator's own menu. Acting on either would be charter
        taking a gesture that already means something else."""
        slots.VIEWPORT.publish((None, "alpha"))
        for button in ("middle", "right"):
            self.assertFalse(self._click(1, name=button))
            self.assertIsNone(state.selection(FID), f"{button}-click selected a row")

    def test_re_selecting_the_row_already_selected_is_not_news(self):
        """Neither pane has anything new to draw, so neither is asked to: the handler
        answers falsy and the frame's version does not move. This is also what keeps a
        double-click from bumping the frame twice."""
        slots.VIEWPORT.publish((None, "alpha"))
        self.assertTrue(self._click(1))
        was = state.version(FID)
        self.assertFalse(self._click(1))
        self.assertEqual(state.version(FID), was)

    def test_a_click_bumps_the_frame_so_the_attention_pane_redraws(self):
        """The detail is drawn by `bottom`, which is a different pane and a different
        process — returning truthy repaints THIS panel and only this panel. `state.bump`
        is how every other cross-panel fact in this frame travels."""
        slots.VIEWPORT.publish((None, "alpha"))
        was = state.version(FID)
        self._click(1)
        self.assertNotEqual(state.version(FID), was)

    def test_a_wheel_notch_over_a_pane_that_fits_moves_nothing(self):
        """The whole of "scrolling means nothing when the pane is tall enough", from the
        handler's side: it reads the bound the last paint settled and refuses. No repaint
        is asked for, so the frame does not paint."""
        slots.VIEWPORT.settle(0)
        self.assertFalse(self.on_event(overlay.Event(overlay.SCROLL, "down")))
        self.assertFalse(self.on_event(overlay.Event(overlay.SCROLL, "up")))

    def test_a_wheel_notch_moves_exactly_one_row(self):
        """One report per notch is what a terminal sends and what tmux forwards, so a
        larger step multiplies whatever the operator's mouse already decided."""
        slots.VIEWPORT.settle(5)
        self.assertTrue(self.on_event(overlay.Event(overlay.SCROLL, "down")))
        self.assertEqual(slots.VIEWPORT.offset, 1)
        self.assertTrue(self.on_event(overlay.Event(overlay.SCROLL, "up")))
        self.assertEqual(slots.VIEWPORT.offset, 0)

    def test_the_wheel_never_selects_and_a_click_never_scrolls(self):
        """Two gestures, two pieces of state. A wheel that moved the selection would make
        the highlight follow the pointer, which is the hover charter deliberately does not
        have (SGR 1000, not 1003)."""
        slots.VIEWPORT.settle(5)
        slots.VIEWPORT.publish((None, "alpha"))
        self.on_event(overlay.Event(overlay.SCROLL, "down"))
        self.assertIsNone(state.selection(FID))
        self._click(1)
        self.assertEqual(slots.VIEWPORT.offset, 1, "a click moved the window")

    def test_nothing_it_does_is_irreversible(self):
        """§4i, and the property this handler exists to demonstrate: a pointer event can
        arrive unpaired, so nothing driven by one may be a thing you cannot take back.
        Everything a click here can do is undone by clicking the row you were on."""
        slots.VIEWPORT.publish((None, "alpha", "beta"))
        self._click(1)
        self._click(2)
        self._click(1)
        self.assertEqual(state.selection(FID), "alpha")


class ThePaneWiresItAllTogether(PersonaIso, unittest.TestCase):
    """`slots._repos` — the one place the bound, the map and the highlight are decided."""

    def setUp(self) -> None:
        super().setUp()
        state.frame_dir(FID, create=True)
        slots.VIEWPORT.forget()

    def tearDown(self) -> None:
        slots.VIEWPORT.forget()
        super().tearDown()

    def _paint(self, *, cols=WIDE, rows=24) -> str:
        with mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((cols, rows))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            return slots.render("repos", FID)

    def test_a_paint_records_which_repo_each_pane_row_is_about(self):
        """Row 0 is the heading and belongs to nobody; the table starts at row 1. That
        offset by one is the whole of what a click has to get right, and it is decided
        here rather than in the handler."""
        gather.save(FID, _data([_row("alpha"), _row("beta")]))
        self._paint(rows=4)
        self.assertIsNone(slots.VIEWPORT.repo_at(0))
        self.assertEqual(slots.VIEWPORT.repo_at(1), "alpha")
        self.assertEqual(slots.VIEWPORT.repo_at(2), "beta")

    def test_a_paint_of_a_pane_that_fits_leaves_nothing_to_scroll(self):
        """Two repos in a pane with room for two: the bound the handler will read is 0,
        so the wheel is inert on the ordinary plane by construction rather than by luck."""
        gather.save(FID, _data([_row("alpha"), _row("beta")]))
        self._paint(rows=3)
        self.assertEqual(slots.VIEWPORT.limit, 0)

    def test_a_paint_of_a_pane_that_does_not_fit_leaves_room_to_scroll(self):
        """Twenty repos in a pane with room for four rows of table: three repo rows and
        the note, so the window's last position is 17."""
        gather.save(FID, _data([_row(f"r{i}") for i in range(20)]))
        self._paint(rows=5)
        self.assertEqual(slots.VIEWPORT.limit, 17)

    def test_a_pane_with_no_table_in_it_is_clicked_on_nobody(self):
        """The three one-line answers this pane draws instead of a table — not gathered
        yet, gathered and empty, too narrow — publish no map at all, so a click on the
        sentence is not a selection. A map left over from an earlier paint would sell the
        operator a repo that is no longer on screen."""
        gather.save(FID, _data([_row("alpha")]))
        self._paint(rows=4)
        self.assertEqual(slots.VIEWPORT.repo_at(1), "alpha")
        gather.save(FID, _data([]))
        self._paint(rows=4)
        self.assertIsNone(slots.VIEWPORT.repo_at(1))
        gather.discard(FID)
        self._paint(rows=4)
        self.assertIsNone(slots.VIEWPORT.repo_at(1))
        gather.save(FID, _data([_row("alpha")]))
        self._paint(cols=40, rows=4)
        self.assertIsNone(slots.VIEWPORT.repo_at(1))

    def test_a_starved_pane_puts_the_window_back_rather_than_keeping_a_stale_bound(self):
        """A resize takes the table's rows away. `settle` runs on every paint, not only
        the ones that draw rows, so the bound the handler reads is this pane's rather than
        whatever the last taller one recorded."""
        gather.save(FID, _data([_row(f"r{i}") for i in range(20)]))
        self._paint(rows=8)
        self.assertGreater(slots.VIEWPORT.limit, 0)
        self._paint(rows=1)
        self.assertEqual(slots.VIEWPORT.limit, 0)

    def test_the_pane_draws_the_selection_the_frame_recorded(self):
        gather.save(FID, _data([_row("alpha"), _row("beta")]))
        state.record_selection(FID, "beta")
        rows = self._paint(rows=4).split("\n")
        self.assertNotIn("\x1b[7m", rows[1])
        self.assertIn("\x1b[7m", rows[2])

    def test_the_scrolled_window_is_what_the_pane_paints(self):
        """The offset is not a number kept beside the table — it is the table. Scrolled
        by one, the pane's own rows are the ones offset 1 composes."""
        gather.save(FID, _data([_row(f"r{i}") for i in range(20)]))
        self._paint(rows=6)
        slots.VIEWPORT.move(1)
        got = [tui.strip_ansi(ln) for ln in self._paint(rows=6).split("\n")]
        with mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((WIDE, 6))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            want = slots._table_lines(gather.cached(FID),
                                      slots.content_width("repos"), 5, offset=1)
        self.assertEqual(got[1:], [tui.strip_ansi(ln.text) for ln in want])


class TheAttentionRowSaysWhatWasPicked(PersonaIso, unittest.TestCase):
    """`slots._selected_detail` and `slots._detail_text` — the operator's "tooltip"."""

    def setUp(self) -> None:
        super().setUp()
        state.frame_dir(FID, create=True)

    def test_a_busy_repo_is_read_back_in_words(self):
        """Every field the table draws as a glyph, said. Asserted as the sentence rather
        than as "contains the name": a detail that dropped a field would still contain the
        name, and the field it dropped is the one the operator clicked to see."""
        self.assertEqual(
            tui.strip_ansi(slots._detail_text(
                _row("charter", branch="fix/x", dirty=True, ahead=2, behind=1,
                     ci="failed", change=123, sigil="!", worktree_count=3))),
            "▪ charter · fix/x · dirty · 2 ahead · 1 behind · CI failed · !123 · ⑂3")

    def test_a_quiet_repo_says_clean_rather_than_saying_nothing(self):
        """An absence standing in for a claim is the defect this repository keeps finding:
        a reader cannot tell "nothing is wrong" from "this field was cut". The word comes
        from `_needs_attention`, the same predicate the `…(+N more), all clean` note asks,
        so the two cannot come to disagree about what clean means."""
        self.assertEqual(tui.strip_ansi(slots._detail_text(_row("charter"))),
                         "▪ charter · main · clean")

    def test_a_repo_with_no_branch_says_so_rather_than_leaving_the_cell_empty(self):
        self.assertIn("· ? ·", tui.strip_ansi(slots._detail_text(_row("x", branch=""))))

    def test_nothing_selected_is_nothing_on_the_row(self):
        """What every plane that has never clicked anything gets: `_fit_fields` drops an
        empty field whole, so the attention row is what it was before this existed."""
        gather.save(FID, _data([_row("alpha")]))
        self.assertEqual(slots._selected_detail(FID), "")

    def test_a_selection_naming_a_repo_that_is_gone_says_nothing(self):
        """The table drew no highlight for it either, so both surfaces go quiet together.
        Saying "gone" here would leave the attention row as the only thing on screen
        claiming that repo ever existed."""
        gather.save(FID, _data([_row("alpha")]))
        state.record_selection(FID, "vanished")
        self.assertEqual(slots._selected_detail(FID), "")

    def test_a_frame_whose_gather_has_not_landed_says_nothing_rather_than_scanning(self):
        """`gather.cached` and never `gather.read`: a panel must not sweep, and `bottom`
        is the animated slot — a fallback scan here would walk every clone on the plane
        five times a second for the length of every dispatch."""
        gather.discard(FID)
        state.record_selection(FID, "alpha")
        with mock.patch.object(gather, "scan",
                               side_effect=AssertionError("the panel swept")):
            self.assertEqual(slots._selected_detail(FID), "")

    def test_a_frame_with_nothing_selected_does_not_read_the_gather_for_it(self):
        """**The cost claim, counted rather than timed** — a wall-clock assertion on a
        shared box measures the box, which is `test_frame_slots`' own rule one budget
        over. `bottom` is the frame's ANIMATED slot: it repaints at `panel.TICK` for the
        whole length of every dispatch, so a `gather.cached` added unconditionally here
        would be a JSON read five times a second on every plane, forever, for a field
        that is empty on almost all of them.

        What it pays instead is one `read_text` of a file that is usually not there, and
        the gather read only when a row has actually been picked."""
        gather.save(FID, _data([_row("alpha")]))
        with mock.patch.object(gather, "cached",
                               side_effect=AssertionError("read the gather")) as never:
            self.assertEqual(slots._selected_detail(FID), "")
            self.assertEqual(never.call_count, 0)
        state.record_selection(FID, "alpha")
        with mock.patch.object(gather, "cached",
                               wraps=gather.cached) as once:
            self.assertNotEqual(slots._selected_detail(FID), "")
            self.assertEqual(once.call_count, 1, "the detail read the gather twice")

    def test_the_detail_is_the_last_field_on_the_attention_row(self):
        """"The right side" of a row composed left to right and joined with ` · ` is the
        last field, and last is where it goes."""
        gather.save(FID, _data([_row("alpha")]))
        state.record_selection(FID, "alpha")
        with mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((WIDE, 24))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            row = tui.strip_ansi(slots.render("bottom", FID))
        self.assertTrue(row.rstrip().endswith("▪ alpha · main · clean"), row)


class TheSelectionHasAKeyboardRoute(PersonaIso, unittest.TestCase):
    """`builtin_actions._register_selection` — `F2`, arrow keys, Enter.

    `[frame] mouse` ships off and charter does not own the harness, so a component whose
    only route to a piece of state is a click has no route to it on most planes
    (`component.EVENT_KINDS`). These rows are charter keeping its own rule about its own
    first pointer affordance.
    """

    def setUp(self) -> None:
        super().setUp()
        state.frame_dir(FID, create=True)

    def _ctx(self, names):
        a = builtin_actions.build(FID, current_density="normal",
                                  current_chrome="off").get("repo.next")
        return faction.build(a.touches, fid=FID,
                             snapshot={"repos": [_row(n) for n in names]})

    def _run(self, aid, names):
        reg = builtin_actions.build(FID, current_density="normal", current_chrome="off")
        a = reg.get(aid)
        return a.run(faction.build(a.touches, fid=FID,
                                   snapshot={"repos": [_row(n) for n in names]}))

    def test_the_palette_offers_both_directions(self):
        reg = builtin_actions.build(FID, current_density="normal", current_chrome="off")
        self.assertEqual([a.title for a in reg.all() if a.id.startswith("repo.")],
                         ["repo: select the next row", "repo: select the previous row"])

    def test_next_from_nothing_starts_at_the_top_and_previous_at_the_bottom(self):
        """The two ends a walk with no cursor under it can be entered from. One sentinel
        for both directions was the first version and was wrong: `-1` gives `next` its
        first row and gives `previous` the row SECOND from the bottom, which is a cursor
        appearing in the middle of a list nobody had put one in."""
        self._run("repo.next", ["a", "b", "c"])
        self.assertEqual(state.selection(FID), "a")
        state.record_selection(FID, "")
        self._run("repo.previous", ["a", "b", "c"])
        self.assertEqual(state.selection(FID), "c")

    def test_it_walks_the_tables_display_order(self):
        state.record_selection(FID, "a")
        self._run("repo.next", ["a", "b", "c"])
        self.assertEqual(state.selection(FID), "b")
        self._run("repo.previous", ["a", "b", "c"])
        self.assertEqual(state.selection(FID), "a")

    def test_it_wraps_at_both_ends(self):
        """Stepping off the end and stopping makes a palette row that visibly does
        nothing, which reads as broken and costs a whole `F2` to find out."""
        state.record_selection(FID, "c")
        self._run("repo.next", ["a", "b", "c"])
        self.assertEqual(state.selection(FID), "a")
        self._run("repo.previous", ["a", "b", "c"])
        self.assertEqual(state.selection(FID), "c")

    def test_a_selection_naming_a_repo_that_is_gone_re_enters_from_the_end(self):
        state.record_selection(FID, "vanished")
        self._run("repo.next", ["a", "b"])
        self.assertEqual(state.selection(FID), "a")

    def test_it_bumps_the_frame_so_both_panes_redraw(self):
        was = state.version(FID)
        self._run("repo.next", ["a"])
        self.assertNotEqual(state.version(FID), was)

    def test_the_work_is_done_before_run_returns(self):
        """These two rows do NOT spawn, unlike every other built-in action, and that is
        safe for one measured reason: the work is two atomic file writes and `cmd_palette`
        joins the invocation before it closes the pane. A subprocess would buy nothing and
        cost a whole interpreter start."""
        note = self._run("repo.next", ["a", "b"])
        self.assertEqual(state.selection(FID), "a")
        self.assertEqual(note, "selected a")

    def test_a_plane_with_no_clones_is_told_why_rather_than_pressing_nothing(self):
        reg = builtin_actions.build(FID, current_density="normal", current_chrome="off")
        offers = {o.id: o for o in reg.offers(fid=FID, snapshot={"repos": []})}
        self.assertFalse(offers["repo.next"].available)
        self.assertIn("charter clone", offers["repo.next"].reason)
        self.assertTrue(
            {o.id: o for o in reg.offers(
                fid=FID, snapshot={"repos": [_row("a")]})}["repo.next"].available)


class ThePanelDeliversToCharterSOwnComponent(PersonaIso, unittest.TestCase):
    """`panel._run` — charter's own panels are drawn off `slots.SLOTS`, and one of them
    now also has to be DRIVEN. Until this, that branch built no registry at all, so a
    built-in was the one kind of component that could declare `events` and never receive
    any."""

    def test_only_the_repo_table_gets_an_event_path(self):
        reg = builtins.build(FID)
        self.assertIsNotNone(panel._dispatcher(reg, "repos"))
        for cid in ("identity", "attention", "sidebar"):
            self.assertIsNone(panel._dispatcher(reg, cid),
                              f"{cid} pays for an input path it declared nothing for")

    def test_a_handler_that_raises_costs_its_pane_and_says_so(self):
        """§4b's answer for a provider, given to charter's own: a component that quietly
        stopped being interactive is indistinguishable from one nobody has clicked yet.
        The trade is stated and it is real — a working `render` loses its pane because the
        HANDLER broke."""
        c = builtins.build(FID).get("repos")
        broken = c.__class__(id=c.id, title=c.title, edge=c.edge, size=c.size,
                             render=c.render, needs=c.needs, events=c.events,
                             on_event=lambda ev: (_ for _ in ()).throw(
                                 RuntimeError("no")))
        evs = events.Dispatcher(broken, stream=mock.MagicMock())
        with mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((WIDE, 6))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True), \
             mock.patch.object(events.pane, "size",
                               return_value=os.terminal_size((WIDE, 6))):
            evs._deliver(overlay.Event(overlay.CLICK, "left", row=1, pressed=True))
            self.assertIn("repos stopped taking events", evs.failure or "")
            said = panel._failure_text(evs, "repos")
        self.assertIn("repos stopped taking events", tui.strip_ansi(said))

    def test_a_panel_still_taking_events_paints_its_rows(self):
        """The other side of the same branch, so "" is read as "nothing to say" rather
        than as a message of its own."""
        self.assertEqual(panel._failure_text(None, "repos"), "")
        evs = events.Dispatcher(builtins.build(FID).get("repos"),
                                stream=mock.MagicMock())
        self.assertEqual(panel._failure_text(evs, "repos"), "")


class ASelectionBelongsToOneFrame(PersonaIso, unittest.TestCase):
    """`state.record_selection` / `state.selection` / `state.clear_shape`."""

    def test_a_new_frame_claiming_a_recycled_id_does_not_inherit_a_selection(self):
        """The mildest of the files `clear_shape` clears and the same shape: a brand-new
        frame comes up with one repo highlighted and its detail on the attention row
        because somebody pointed at it in a session that is over. The highlight is a claim
        about an action this operator did not take."""
        state.frame_dir(FID, create=True)
        state.record_selection(FID, "alpha")
        self.assertEqual(state.selection(FID), "alpha")
        state.clear_shape(FID)
        self.assertIsNone(state.selection(FID))

    def test_never_recorded_reads_as_nothing_selected(self):
        """The directory exists and the file does not, which is the ordinary state of
        every frame nobody has clicked in — a missing file, not a missing frame."""
        state.frame_dir("f-untouched", create=True)
        self.assertIsNone(state.selection("f-untouched"))

    def test_a_truncated_or_emptied_file_is_nothing_selected_and_not_an_empty_name(self):
        """`None`, never ``""``. Two readers ask this — `slots._table_lines` matches it
        against a row's name and `builtin_actions._select` looks it up in a list — and an
        empty string is a THIRD value both of them would have to know about, for a file
        that says exactly what a missing one says."""
        d = state.frame_dir("f-empty", create=True)
        (d / "selection").write_text("\n")
        self.assertIsNone(state.selection("f-empty"))

    def test_a_frame_with_no_directory_answers_nothing_rather_than_raising(self):
        self.assertIsNone(state.selection("no-such-frame"))

    def test_recording_for_an_id_no_directory_can_be_made_for_is_a_no_op(self):
        """Every writer in `frame/state.py` makes this promise and this one is written
        from a HANDLER — `frame/builtins._repos_events`, on the panel's own event path,
        where an exception costs the component its events for the life of the pane
        (`frame/events.py` retires a handler that raises). Degrading is the only answer
        that leaves the table clickable."""
        state.record_selection("../escape", "alpha")
        self.assertIsNone(state.selection("../escape"))


class TheRankingStillAnswersEveryOtherCaller(unittest.TestCase):
    """`statusline._pick_rows` gained an *offset* and every existing caller omits it."""

    def test_the_default_is_the_slice_it_always_took(self):
        dirs = [statusline.Path(f"/tmp/r{i}") for i in range(6)]
        st = {d: {} for d in dirs}
        self.assertEqual(statusline._pick_rows(dirs, 3, None, st, {}),
                         statusline._pick_rows(dirs, 3, None, st, {}, offset=0))
        self.assertEqual(statusline._pick_rows(dirs, 3, None, st, {}), dirs[:3])

    def test_an_offset_walks_down_the_ranking(self):
        dirs = [statusline.Path(f"/tmp/r{i}") for i in range(6)]
        st = {d: {} for d in dirs}
        self.assertEqual(statusline._pick_rows(dirs, 3, None, st, {}, offset=2), dirs[2:5])

    def test_past_the_end_answers_fewer_rows_rather_than_clamping(self):
        """The caller that scrolls is the one that knows how many rows the pane has;
        a clamp here would be a second, weaker copy of `slots._scroll_limit`."""
        dirs = [statusline.Path(f"/tmp/r{i}") for i in range(3)]
        st = {d: {} for d in dirs}
        self.assertEqual(statusline._pick_rows(dirs, 3, None, st, {}, offset=2), dirs[2:])
        self.assertEqual(statusline._pick_rows(dirs, 3, None, st, {}, offset=9), [])


if __name__ == "__main__":
    unittest.main()
