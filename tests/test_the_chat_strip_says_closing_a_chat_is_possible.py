"""**The chat strip draws a `-` beside its `+`, and the `-` opens a question — #921.**

*"now for example no way to reactivelly close harness session, we have plus button - that
adding new session tab, but no any option to delete/stop, no any modal/drawer for
confirmation."*

It was possible the whole time: `F2 → chat: close`, or a right press on a tab. Neither is
written anywhere on the frame, which advertises exactly two things — `F2 palette` on the
attention row and `+` on this strip. So the operator read a row that offers to MAKE a chat
and nothing else, and concluded the system would not let them close one. **That conclusion
was correct given the evidence on screen.**

Three properties carry this file, because each is a rule the decision states and a first
implementation loses:

* **It does not close on the press.** `slots.ADD_CHAT` may act on a press because nothing
  is destroyed; this may not, so it does not act. What it spawns is byte for byte the
  `charter frame-palette --tab` a RIGHT press on this chat's own tab already spawns, whose
  `chat: close` row is a doorway onto `leave.confirm_rows`. §4i in one line — a pointer
  opens the question; the keyboard answers it — and the strongest form of "adds a route and
  no authority" available: the two gestures build the identical argv.
* **Both affordances or neither, at every width.** `slots._compose` measures `+` and `-` as
  one field (`slots._affordances`), so there is no width and no rung at which the strip
  offers to make a chat and not to unmake one. A `-` that dropped first would be #921
  arriving again at a narrower pane.
* **It re-cuts nothing.** A `×` per tab would cost a column on every tab and move every
  cut, which `slots.TAB_COUNT_W` reserves against and `slots.TAB_SPINNER` was refused for.
  One glyph at the END of the row cannot, and this measures that it does not rather than
  arguing it — at every width from 0 to 220 and at one, two and three rows.

The column half of the strip generally is `tests/test_frame_bars.py`; the real-tmux half of
a press is `tests/test_a_real_click_on_a_real_tab_bar_switches.py`. What this file adds is
the fourth affordance and the fifth gesture.
"""

from __future__ import annotations

import os
import unicodedata
import unittest
from unittest import mock

from charter import config, tui, util
from charter.frame import slots, state

from tests.test_a_click_on_a_tab_bar_switches import _ABarThatWasDrawn, _press
from tests.test_frame_chat_switch import _plant

#: The argv a `-` press must produce — `frame/builtins._TAB_MENU` with the ACTIVE chat
#: appended. Spelled from the constant rather than from a literal, because the property is
#: that the two routes to the tab menu agree, not that either says a particular string.
_MENU = ("frame-palette", "--tab")


class TheStripDrawsBothAffordances(_ABarThatWasDrawn, unittest.TestCase):
    """What the row says about itself — the whole of what #921 reported."""

    def setUp(self):
        super().setUp()
        self.enterContext(mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "api"},
                                          clear=False))
        for chat in ("api.1", "api.2"):
            _plant(chat, workspace="api")
        self.row = tui.strip_ansi(slots.chats_bar("api.1", self.WIDTH)[0])

    def test_the_row_ends_in_a_plus_and_a_minus(self):
        """The sentence the strip now says: making is offered, and so is unmaking."""
        self.assertTrue(
            self.row.rstrip().endswith(slots.ADD_CHAT + " " + slots.CLOSE_CHAT),
            f"the strip does not offer to close a chat: {self.row!r}")

    def test_each_glyph_owns_exactly_its_own_cell(self):
        """`add_at` and `close_at` are two questions with two answers, one cell each, and
        the cell between them is neither's — `slots._tab_columns`' rule for the gap between
        two tabs, kept where a wrong answer would open a teardown for a press that meant
        *new*."""
        plus = self._column_of(self.row, slots.ADD_CHAT)
        minus = self._column_of(self.row, slots.CLOSE_CHAT)
        cells = range(self.WIDTH)
        self.assertEqual([c for c in cells if slots.TABS.add_at(0, c)], [plus])
        self.assertEqual([c for c in cells if slots.TABS.close_at(0, c)], [minus])
        self.assertEqual(minus, plus + 2, f"the pair is not adjacent: {self.row!r}")
        self.assertFalse(slots.TABS.add_at(0, plus + 1))
        self.assertFalse(slots.TABS.close_at(0, plus + 1))

    def test_the_minus_is_not_a_tab_and_not_a_count(self):
        """Four questions, four methods. A cell that is one of them answers no to the other
        three, which is what stops a press on `-` switching to a chat or opening a picker
        over the chats it could not draw."""
        minus = self._column_of(self.row, slots.CLOSE_CHAT)
        self.assertIsNone(slots.TABS.switch_to(0, minus))
        self.assertIsNone(slots.TABS.tab_at(0, minus))
        self.assertFalse(slots.TABS.more_at(0, minus))
        self.assertFalse(slots.TABS.add_at(0, minus))

    def test_the_glyph_is_ascii_and_one_cell(self):
        """**`slots._BAR_RULE`'s rule is the ROW's, not any one constant's**: a click here
        is resolved by COLUMN, so the glyph to put on it is the one whose width no terminal
        disagrees about.

        `−` U+2212 measures one cell by `tui.width` and is East-Asian *Neutral*, and it is
        still not what ships: it is a codepoint a terminal font may not carry, and a
        fallback into a CJK face draws two cells whatever the table says. What that buys is
        a prettier dash; what it risks is *fires wrongly* rather than *never fires*.
        """
        self.assertEqual(slots.CLOSE_CHAT, "-")
        self.assertTrue(slots.CLOSE_CHAT.isascii(), repr(slots.CLOSE_CHAT))
        self.assertEqual(tui.width(slots.CLOSE_CHAT), 1)
        self.assertEqual(unicodedata.east_asian_width(slots.CLOSE_CHAT), "Na")

    def test_the_workspace_strip_offers_neither(self):
        """**The same decision as its missing `+`, not a second one.** The rule a strip
        earns a `-` from is *a surface that advertises making a thing must advertise
        unmaking it*; a strip that advertises neither owes neither, and there is no route
        to removing a workspace anywhere in the frame — `workspace: close` is specified,
        unimplemented, and filed on its own."""
        for name in ("alpha", "beta"):
            (config.WORKSPACES_DIR / name).mkdir(parents=True, exist_ok=True)
        state.frame_dir("f1", create=True)
        state.record_workspace("f1", "alpha")
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": ""}):
            row = tui.strip_ansi(slots.workspaces_bar("f1", self.WIDTH)[0])
        self.assertNotIn(slots.CLOSE_CHAT, row)
        self.assertEqual([c for c in range(self.WIDTH) if slots.TABS.close_at(0, c)], [])


class BothAffordancesOrNeither(unittest.TestCase):
    """**They are one field as far as the ladder is concerned.**

    A `-` measured on its own would be dropped at a width where the `+` still fit, and the
    strip would go back to saying exactly what #921 reported it saying — on a narrower pane
    rather than on every pane, which is worse, because nobody would think to look.
    """

    NAMES = [f"api.{i}" for i in range(1, 8)]

    def _cells(self, width: int, rows: int):
        _lines, _cols, _more, add, close = slots._compose(
            self.NAMES, "api.3", width, note=slots.ADD_CHAT, close=slots.CLOSE_CHAT,
            rows=rows)
        return list(add), list(close)

    def test_no_width_draws_one_without_the_other(self):
        for width in range(0, 220):
            for rows in (1, 2, 3):
                add, close = self._cells(width, rows)
                self.assertEqual(bool(add), bool(close),
                                 f"width {width}, {rows} row(s): + {add} / - {close}")

    def test_and_some_width_draws_them(self):
        """The case above passes on a strip that draws neither anywhere, so this is what
        says the sweep measured something."""
        self.assertTrue(any(self._cells(w, 1)[1] for w in range(0, 220)))

    def test_the_pair_never_shares_a_row_with_an_overflow_count(self):
        """`_Tabs.add_at`'s structural promise, restated for its neighbour: a strip
        carrying the affordances carries no `+N` on any row, so the two fields that begin
        with a `+` are never on screen together to be confused with each other."""
        for width in range(0, 220):
            for rows in (1, 2, 3):
                _l, _c, more, add, close = slots._compose(
                    self.NAMES, "api.3", width, note=slots.ADD_CHAT,
                    close=slots.CLOSE_CHAT, rows=rows)
                if add or close:
                    self.assertEqual(list(more), [],
                                     f"width {width}, {rows} row(s) drew both")


class TheMinusReCutsNothing(unittest.TestCase):
    """**The width question the decision turns on, measured rather than argued.**

    Anything that changes a tab's field width moves every cut, and the cell the operator
    was about to press holds another chat's name a moment later — #767's double press, and
    the reason `slots.TAB_COUNT_W` is reserved unconditionally and `slots.TAB_SPINNER` took
    the mark's cell instead of buying one. A `-` at the END of the row changes no field, so
    the map of which cell holds which tab must be byte-identical with it and without it.
    """

    NAMES = [f"api.{i}" for i in range(1, 8)]

    def test_every_tab_lands_where_it_landed_before(self):
        for width in range(0, 220):
            for rows in (1, 2, 3):
                without = slots._compose(self.NAMES, "api.3", width,
                                         note=slots.ADD_CHAT, close="", rows=rows)[1]
                with_it = slots._compose(self.NAMES, "api.3", width,
                                         note=slots.ADD_CHAT,
                                         close=slots.CLOSE_CHAT, rows=rows)[1]
                self.assertEqual(without, with_it,
                                 f"the `-` re-cut the strip at width {width}, "
                                 f"{rows} row(s)")


class APressOnTheMinusOpensTheQuestion(_ABarThatWasDrawn, unittest.TestCase):
    """**The gesture, and everything it deliberately is not.**"""

    def setUp(self):
        super().setUp()
        self.enterContext(mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "api"},
                                          clear=False))
        for chat in ("api.1", "api.2"):
            _plant(chat, workspace="api")
        self.row = tui.strip_ansi(slots.chats_bar("api.1", self.WIDTH)[0])
        self.on_event = self._handler("chats", "api.1")
        self.minus = self._column_of(self.row, slots.CLOSE_CHAT)

    def test_it_opens_the_tab_menu_about_the_chat_you_are_in(self):
        """The whole of §4i's *a pointer opens the question*: what this starts is a
        SURFACE about one chat, not a teardown."""
        self.on_event(_press(self.minus))
        self.assertEqual(
            self.spawned,
            [(util.self_relaunch_argv(*_MENU, "api.1"), "api.1")])

    def test_it_is_the_same_argv_a_right_press_on_that_tab_builds(self):
        """**Adds a route and no authority**, in the strongest form available: the two
        gestures are one command. A `-` that spelled its own would be a second answer to
        "how does a pointer ask about closing a chat", and the two would part."""
        self.on_event(_press(self._column_of(self.row, "*api.1"), name="right"))
        self.on_event(_press(self.minus))
        by_right, by_minus = self.spawned
        self.assertEqual(by_minus, by_right)

    def test_it_closes_nothing_on_the_press(self):
        """`frame-close` and `frame-quit` are the two commands that stop something, and
        neither may be reached by a pointer event — §4i, and `builtins._bar_events`'
        argument for why the `+` beside it may act and this may not."""
        self.on_event(_press(self.minus))
        for argv, _fid in self.spawned:
            for word in ("frame-close", "frame-quit"):
                self.assertNotIn(word, argv, f"a press stopped something: {argv!r}")

    def test_the_press_acts_and_the_release_does_not(self):
        """A release may arrive with no matching press — a drag that began on a pane
        border — and this module keeps no press state, so the press is the half that is
        never delivered unpaired."""
        self.on_event(_press(self.minus, pressed=False))
        self.assertEqual(self.spawned, [], "an unpaired release opened a surface")
        self.on_event(_press(self.minus))
        self.assertEqual(len(self.spawned), 1)

    def test_the_middle_button_opens_nothing(self):
        self.on_event(_press(self.minus, name="middle"))
        self.assertEqual(self.spawned, [])

    def test_the_cell_between_the_two_glyphs_does_nothing(self):
        """A press aimed between `+` and `-` has no field to be about, and guessing the
        nearer one would be guessing between *make* and *unmake*."""
        self.on_event(_press(self.minus - 1))
        self.assertEqual(self.spawned, [], f"the separator acted: {self.row!r}")

    def test_the_plus_beside_it_still_makes_a_chat(self):
        """The control: two answers now live one cell apart in one handler, so a case
        asserting only the new one would pass with the old one deleted."""
        self.on_event(_press(self._column_of(self.row, slots.ADD_CHAT)))
        self.assertEqual(self.spawned,
                         [(util.self_relaunch_argv("frame-new-chat"), "api.1")])

    def test_the_handler_is_falsy_when_it_opened_the_surface(self):
        """Truthy means *repaint me*, and the pane the menu carves comes off the harness —
        nothing in THIS rectangle changed."""
        self.assertFalse(self.on_event(_press(self.minus)))
        self.assertEqual(len(self.spawned), 1, "the case measured nothing")

    def test_the_workspace_bars_handler_will_not_open_one_either(self):
        """**The property the drawn row cannot reach, and why the handler is handed the
        answer as DATA.** `slots._workspaces_strip` publishes no close cell, so a handler
        that opened a menu on `close_at` regardless would pass every case above and would
        start opening chat menus off the workspace bar the day that renderer grew an
        affordance of its own. This publishes the map by hand and asks the handler."""
        state.frame_dir("f1", create=True)
        slots.TABS.publish({}, "", close=[(0, 7)])
        self.spawned.clear()
        self._handler("workspaces", "f1")(_press(7))
        self.assertEqual(self.spawned, [],
                         "the workspace bar's handler opens a chat menu — it is only the "
                         "renderer that is stopping it")
        slots.TABS.publish({}, "", close=[(0, 7)])
        self._handler("chats", "f1")(_press(7))
        self.assertEqual([argv[-2] for argv, _fid in self.spawned], ["--tab"],
                         "the case measured nothing on the chats bar either")


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
