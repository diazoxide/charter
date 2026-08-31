"""The three panels that advertised something and answered nothing — #742, #751, #753.

Before this change `repos`, `chats` and `workspaces` were the only components on the
machine that declared any event at all. The identity strip, the attention strip and the
sidebar drew nouns the palette already opens by name, drew a roster with a cursor-looking
marker on the active row, and drew the words `F2 palette` — *the only affordance charter
advertises on screen* — and handled nothing.

**What is asserted here is a COLUMN and a ROW wherever a column or a row is what the code
computes.** `_Doors` and `_Chips` are maps from a number to a meaning, and a test that
asked "is anything published" would go green over a map off by two — which is precisely the
defect a published map exists to prevent, since the alternative was re-deriving the number
when the click arrived. So the door on `' ⬢ alpha    ◆ steward'` is asserted to be columns
0–7 and 10–20 and not to be "non-empty", and the persona on row 3 is asserted to be `forge`.

**Every rung of both ladders is asserted, INCLUDING the rungs that draw nothing.** That is
`slots._Viewport.blank`'s finding and `slots._bar.row`'s discipline, and it is the half a
test suite skips: a strip that kept a stale map through a resize opens the palette from a
cell the operator can see is empty, and a persona column that kept one through a `terse`
switch adopts a persona whose row is no longer drawn. `TheMapDescribesWhatWasDrawn` is
those cases and nothing else.

**The refusals are tested as hard as the answers.** A component that answers every click is
as wrong as one that answers none: the heading, the `…(+N more)` row, the gap between two
fields, the space past the last one, the release half of a drag, the button that is not
left, and the row you are already on all have to answer nothing, and an accident that
happened to cancel would be indistinguishable from a design that refuses.
"""

from __future__ import annotations

import unittest
from unittest import mock

from charter import statusline as sl
from charter import tui
from charter.frame import builtins, events, overlay, slots, state, tmuxctl

from tests._isolation import PersonaIso

FID = "f-doors"

#: A pane wide enough for both strips to draw every field they have, so nothing here is
#: accidentally measuring the starved rung the ladder cases below are for.
WIDE = 120


def _chip(name, *, active=False, badges="") -> sl.PersonaChip:
    """One persona row shaped the way `statusline._persona_chip_cells` shapes it.

    The head carries the marker, the name and the vault dot exactly as that function
    composes them (`▸` for the active persona, `▫` otherwise), because `slots._persona_rows`
    measures what it is handed and a head built some other way would be a different column.
    """
    head = (f"{sl._MAGENTA}{sl._MARK_ACTIVE} {sl._BOLD}{name}{sl._R}" if active
            else f"{sl._DIM}{sl._MARK_IDLE} {name}{sl._R}")
    return sl.PersonaChip(name, head, badges, 0, active=active)


def _roster(*chips):
    """`statusline._persona_chip_cells` answering *chips* — the one read `persona_section`
    makes of the plane."""
    return mock.patch.object(sl, "_persona_chip_cells", lambda: list(chips))


def _click(row=0, col=0, *, pressed=True, name="left"):
    return overlay.Event(overlay.CLICK, name, row=row, col=col, pressed=pressed)


class _Strip(PersonaIso):
    """A base that draws either strip at a width and hands back the flat row.

    Both strips are composed out of pieces and then truncated, and every case below wants
    the same two things: what the row SAYS with the escapes gone, and which columns the
    paint published. Doing it here keeps a case to its own assertion.

    **`PersonaIso` rather than a bare `TestCase`, and `tests/_envguard.py` is why**: both
    `_top` and `_bottom` read `$CHARTER_WORKSPACE` and `$CHARTER_SESSION_ID`, which are
    exactly the names an operator's own frame exports — sixteen tests failed on that and
    one went falsely green (#519, #521, #528). The base declares the whole charter
    environment unset, which is the answer CI gives.
    """

    def setUp(self) -> None:
        super().setUp()
        slots.DOORS.forget()
        self.addCleanup(slots.DOORS.forget)

    def top_row(self, *, width=WIDE, terse=False, gauge=(), line=None, sidebar=False):
        """`slots._top` drawn at *width*, with the escapes stripped."""
        parts = sl.PersonaLine(f"  {sl._MAGENTA}◆{sl._R} {sl._BOLD}steward{sl._R}",
                               f"  {sl._DIM}◇ personas 6{sl._R}", "")
        with mock.patch.object(slots, "content_width", lambda n: width), \
             mock.patch.object(slots, "verbosity",
                               lambda f: "terse" if terse else "full"), \
             mock.patch.object(slots, "_frame_workspace", lambda f: "alpha"), \
             mock.patch.object(slots, "_sidebar_live", lambda f: sidebar), \
             mock.patch.object(sl, "recorded_context_gauge", lambda s: list(gauge)), \
             mock.patch.object(sl, "_persona_line_parts",
                               lambda: parts if line is None else line()):
            return tui.strip_ansi(slots._top(FID))

    def bottom_row(self, *, width=WIDE, terse=False, repo="", todos=7, alerts=(),
                   news=(), inflight="", operator_socket=False, notice=""):
        """`slots._bottom` drawn at *width*, with the escapes stripped."""
        with mock.patch.object(state, "notice", lambda f: notice), \
             mock.patch.object(slots, "content_width", lambda n: width), \
             mock.patch.object(slots, "verbosity",
                               lambda f: "terse" if terse else "full"), \
             mock.patch.object(slots, "_frame_workspace", lambda f: "alpha"), \
             mock.patch.object(slots, "_selected_detail", lambda f: repo), \
             mock.patch.object(slots, "_inflight_field", lambda: inflight), \
             mock.patch.object(sl, "_todo_count", lambda ws: todos), \
             mock.patch.object(sl, "_alerts", lambda ws: list(alerts)), \
             mock.patch.object(sl, "_session_news", lambda s, inflight: list(news)), \
             mock.patch.object(tmuxctl, "is_operator_socket",
                               lambda s: operator_socket):
            return tui.strip_ansi(slots._bottom(FID))

    def doors(self) -> set[int]:
        """Every column the last paint published, as a set of ints."""
        return {c for c in range(-2, 400) if slots.DOORS.opens_palette(c)}

    def under(self, text: str, cols) -> str:
        """*text* with a `^` under each of *cols* — what a failure prints."""
        return "\n" + text + "\n" + "".join(
            "^" if i in cols else " " for i in range(len(text)))


class TheIdentityStripPublishesItsTwoDoors(_Strip):
    """`slots._top` — `⬢ <workspace>` and `◆ <persona>`, and nothing else on the row.

    #751: "both are doorways the palette opens by name". The columns are worked out from
    the pieces `_top` composed rather than by finding `⬢` or `◆` in the finished row, for
    the reason that function already gives about the chips — a fact recovered from a
    rendered row is a second reading of what was drawn, free to drift from the first. So
    these cases assert the arithmetic, at the exact columns.
    """

    def test_the_workspace_chip_is_columns_0_to_7_and_the_persona_head_10_to_20(self):
        """The two doors of ` ⬢ alpha    ◆ steward`, at the columns they are drawn in.

        Asserted as the exact sets, not as "the row has some doors": the whole reason this
        map is published rather than recomputed is that a click arrives as a NUMBER, and a
        map that is right about *how many* doors there are and wrong about *where* is the
        version that opens the palette from a blank cell.
        """
        row = self.top_row()
        self.assertEqual(row[:21], " ⬢ alpha    ◆ steward")
        self.assertEqual(self.doors(), set(range(0, 8)) | set(range(10, 21)),
                         self.under(row[:24], self.doors()))

    def test_a_gauge_between_them_moves_the_persona_door_and_is_not_one(self):
        """`ctx 42%` is a readout — the frame reporting, not offering — so the columns it
        occupies answer nothing, and the persona door starts after it."""
        row = self.top_row(gauge=["ctx 42%"])
        self.assertEqual(row[:30], " ⬢ alpha  ctx 42%    ◆ steward")
        self.assertEqual(self.doors(), set(range(0, 8)) | set(range(19, 30)),
                         self.under(row[:33], self.doors()))
        for col in range(8, 19):
            self.assertFalse(slots.DOORS.opens_palette(col),
                             f"column {col} is the gauge and must answer nothing")

    def test_the_roster_half_is_not_a_door(self):
        """`◇ personas 6` trails the head on a frame with no sidebar (#530), so it is drawn
        on some frames and not others — a door there would exist for a reason no operator
        can see. Only `PersonaLine.head` is published."""
        row = self.top_row(sidebar=False)
        self.assertIn("◇ personas 6", row)
        self.assertEqual(max(self.doors()), 20,
                         self.under(row[:36], self.doors()))

    def test_the_charter_version_is_not_a_door(self):
        """It is right-aligned on the same row and is a build number, not a noun the
        palette has an action for."""
        row = self.top_row()
        at = row.index("charter 0.")
        for col in range(at, len(row)):
            self.assertFalse(slots.DOORS.opens_palette(col),
                             f"column {col} is the charter version")

    def test_a_plane_with_no_personas_still_publishes_the_workspace_door(self):
        """`_persona_line_parts` answers `None` on a plane that defines none, so the head
        is empty and contributes no columns — and the workspace chip is unaffected. The
        half-published map is the failure this asserts against: a `None` that took the
        whole publish with it would leave the paint before it standing."""
        self.top_row()
        self.assertIn(20, self.doors(), "the ordinary paint has a persona door")
        drawn = self.top_row(line=lambda: None)
        self.assertEqual(self.doors(), set(range(0, 8)),
                         self.under(drawn[:24], self.doors()))


class TheAttentionStripPublishesTheHotkeyHint(_Strip):
    """`slots._bottom` — `F2 palette`, and nothing else on the row.

    #751's sharpest case: it is the only affordance charter advertises on screen and it was
    the one thing on the row that could not be operated the way it is drawn.
    """

    def test_the_hint_is_the_door_and_the_todo_count_beside_it_is_not(self):
        """`7 todos · F2 palette` — columns 10 to 19 and no others. The todo count is a
        readout; the hint is a key name beside a noun, which is a button everywhere else an
        operator has seen one."""
        row = self.bottom_row()
        self.assertEqual(row, "7 todos · F2 palette")
        self.assertEqual(self.doors(), set(range(10, 20)), self.under(row, self.doors()))

    def test_a_field_added_in_front_of_the_hint_moves_its_door_with_it(self):
        """#729's `notice` dwells at the HEAD of this row for a few seconds after a
        switch, and the door has to move by exactly its width plus a separator.

        **This is the case that pins the walk against the next field somebody adds.** The
        columns are walked out of `_fit_fields`' surviving set rather than searched for in
        the joined row, so a seventh field needs no case of its own here — and a version
        that hard-coded the hint's offset would put the palette on a cell holding the tail
        of somebody's notice. The notice itself answers nothing: it is the outcome of
        something the operator already chose, which is a readout in the same sense the
        todo count is.
        """
        plain = self.bottom_row()
        self.assertEqual(min(self.doors()), 10, self.under(plain, self.doors()))
        row = self.bottom_row(notice="persona → docs")
        self.assertTrue(row.startswith("persona → docs · "), row)
        at = row.index("F2 palette")
        self.assertEqual(self.doors(), set(range(at, at + 10)),
                         self.under(row, self.doors()))
        for col in range(0, len("persona → docs")):
            self.assertFalse(slots.DOORS.opens_palette(col),
                             f"column {col} is the notice and must answer nothing")

    def test_the_separator_between_two_fields_answers_nothing(self):
        """The three cells of ` · ` belong to neither field, and charter will not pick the
        nearer one — `slots._Tabs` refuses its own `_BAR_GAP` for the same reason."""
        self.bottom_row()
        for col in (7, 8, 9):
            self.assertFalse(slots.DOORS.opens_palette(col),
                             f"column {col} is the ` · ` before the hint")

    def test_the_selected_repos_detail_answers_nothing(self):
        """It is the readout of a row selected on ANOTHER pane, so a click on it could only
        mean "select what is already selected" — the one gesture `slots._Tabs.switch_to`
        and `builtins._repos_events` both already refuse."""
        row = self.bottom_row(repo=f"{sl._DIM}▪{sl._R} ledger")
        at = row.index("▪ ledger")
        self.assertEqual(self.doors(), set(range(10, 20)), self.under(row, self.doors()))
        for col in range(at, len(row)):
            self.assertFalse(slots.DOORS.opens_palette(col),
                             f"column {col} is the repo detail")

    def test_the_space_past_the_last_field_answers_nothing(self):
        """A 120-column pane drawing a 20-column row has a hundred cells nothing was drawn
        into. `_door_columns` clips to the width, so they are absent rather than guarded."""
        self.bottom_row()
        for col in (20, 21, 60, 119, 400):
            self.assertFalse(slots.DOORS.opens_palette(col))


class TheMapDescribesWhatWasDrawn(_Strip):
    """Every rung of both ladders publishes, including the rungs that draw no door.

    `slots._Viewport.blank`'s whole finding, one surface over: three of `_repos`' four
    exits cleared the click map and none of them cleared the scroll bound. A strip that
    kept a stale map through a resize would open the palette from a cell the operator can
    see is empty, and *the paint that stops drawing a field is the paint that has to say
    so* — nothing later can.
    """

    def test_terse_keeps_one_field_on_the_attention_strip_and_it_is_not_the_hint(self):
        """`_fit_fields(limit=1)` keeps the highest-priority field with anything to say,
        which on a quiet plane is the todo count. So the row has no door at all — and that
        has to be *published*, not left over from the paint before it."""
        self.bottom_row()
        self.assertTrue(self.doors(), "the wide paint should have published a door")
        row = self.bottom_row(terse=True)
        self.assertEqual(row, "7 todos")
        self.assertEqual(self.doors(), set(), self.under(row, self.doors()))

    def test_a_starved_row_drops_the_hint_first_and_publishes_the_absence(self):
        """`hotkey` is last in `_fit_fields`' priority list, so it is the first field a
        narrow pane loses. The map has to lose it in the same paint."""
        self.bottom_row()
        row = self.bottom_row(width=12)
        self.assertNotIn("palette", row)
        self.assertEqual(self.doors(), set(), self.under(row, self.doors()))

    def test_a_frame_in_the_operators_own_tmux_advertises_no_hotkey_and_opens_no_door(self):
        """Charter binds no key inside a tmux it did not start, so `hotkey_text` is empty
        there — and an empty field contributes no columns rather than a zero-width door at
        somebody else's column."""
        row = self.bottom_row(operator_socket=True)
        self.assertNotIn("palette", row)
        self.assertEqual(self.doors(), set(), self.under(row, self.doors()))

    def test_a_narrow_identity_strip_clips_its_doors_to_what_survived_the_truncate(self):
        """At 14 columns ` ⬢ alpha    ◆ steward` is drawn as ` ⬢ alpha    ◆…`. A door
        published at the columns the persona head WOULD have had is a door on a cell that
        is not on the screen."""
        row = self.top_row(width=14)
        self.assertEqual(len(row), 14)
        self.assertEqual(max(self.doors()), 13, self.under(row, self.doors()))

    def test_a_zero_width_strip_publishes_no_door_at_all(self):
        """`tui.truncate(s, 0)` is `""`, and a map over a row nothing was drawn on is the
        empty one."""
        self.top_row()
        self.assertTrue(self.doors())
        self.assertEqual(self.top_row(width=0), "")
        self.assertEqual(self.doors(), set())


class TheSidebarPublishesWhichPersonaIsOnWhichRow(unittest.TestCase):
    """`slots.persona_section` and `slots._Chips` — #742's persona half.

    The column is drawn as an inverted-row list with a cursor-looking marker on the active
    one, which is the visual vocabulary of something you pick from. What is asserted is the
    ROW each persona landed on, because the ladder is not invertible: `_cap_personas` drops
    the tail of an order and `terse` caps the list again, so "which persona is on row 4"
    cannot be worked back from the roster and the pane's height.
    """

    def setUp(self) -> None:
        slots.CHIPS.forget()
        self.addCleanup(slots.CHIPS.forget)

    def switched(self, row: int):
        """The persona a click on *row*'s NAME cell switches to, or ``None``.

        Column 0 is inside the name cell on every rung these cases draw — the badge column
        is right-aligned — so this asks the question every case here was written to ask
        before the badges became clickable too. `TheBadgeColumnExplainsInstead` is the
        other cell.
        """
        hit = slots.CHIPS.hit(row, 0)
        return None if hit is None or hit.explain else hit.name

    def test_row_0_is_the_heading_and_the_personas_start_at_row_1(self):
        with _roster(_chip("steward", active=True), _chip("docs"), _chip("forge")):
            lines = slots.persona_section(22, 10, terse=False)
        self.assertEqual(tui.strip_ansi(lines[0]), "▪ personas 3")
        self.assertIsNone(self.switched(0), "the heading names no persona")
        self.assertEqual(self.switched(2), "docs")
        self.assertEqual(self.switched(3), "forge")

    def test_the_persona_you_are_already_being_answers_nothing(self):
        """`_Tabs.switch_to`'s rule, kept in the map object rather than in the handler so
        it is a property of one class with one test. It is also what stops a double-click
        switching twice."""
        with _roster(_chip("steward", active=True), _chip("docs")):
            slots.persona_section(22, 10, terse=False)
        self.assertIsNone(self.switched(1))
        self.assertEqual(self.switched(2), "docs")

    def test_the_overflow_row_answers_nothing_because_it_stands_for_rows_not_drawn(self):
        """A three-row budget draws the heading, one persona and `…(+2 more)`. That last
        row is a sentence about the list — `_Tabs` refuses its own `+14` for the identical
        reason — so the persona a click there would have to guess at is not published."""
        with _roster(_chip("steward", active=True), _chip("docs"), _chip("forge")):
            lines = slots.persona_section(22, 3, terse=False)
        self.assertEqual(tui.strip_ansi(lines[2]).strip(), "…(+2 more)")
        self.assertIsNone(self.switched(2))

    def test_a_row_below_the_column_answers_nothing(self):
        """The sidebar puts a blank row and then the todos under the personas. Neither is
        published, and a row past the end of a dict is absent rather than the LAST one —
        which is what a tuple indexed with a big number would not have been."""
        with _roster(_chip("steward", active=True), _chip("docs")):
            slots.persona_section(22, 10, terse=False)
        for row in (3, 4, 9, 400, -1):
            self.assertIsNone(self.switched(row), f"row {row}")

    def test_a_plane_with_no_personas_publishes_an_empty_map(self):
        """The rung that draws `no personas` publishes too — `_Viewport.blank`'s finding.
        A map left standing from the paint before it would switch this frame to a persona
        nobody can see on the row that was clicked."""
        with _roster(_chip("steward", active=True), _chip("docs")):
            slots.persona_section(22, 10, terse=False)
        self.assertEqual(self.switched(2), "docs")
        with _roster():
            lines = slots.persona_section(22, 10, terse=False)
        self.assertEqual(tui.strip_ansi(lines[0]), "no personas")
        for row in range(0, 6):
            self.assertIsNone(self.switched(row), f"row {row}")

    def test_terse_republishes_the_shorter_column(self):
        """`_TERSE_ROWS` caps the list again, so a density change moves what is on row 4.
        The paint that stops drawing a row is the paint that has to say so."""
        chips = [_chip("steward", active=True)] + [_chip(f"p{i}") for i in range(6)]
        with _roster(*chips):
            slots.persona_section(22, 20, terse=False)
        self.assertEqual(self.switched(4), "p2")
        with _roster(*chips):
            lines = slots.persona_section(22, 20, terse=True)
        self.assertEqual(len(lines), 1 + slots._TERSE_ROWS)
        self.assertIsNone(self.switched(5),
                          "terse drew four rows; row 5 is not one of them")

    def test_row_zero_resolves_like_any_other_row(self):
        """`slots._Chips` is indexed by row and knows nothing about headings.

        **Pinned against the object rather than through the renderer, and the deletion
        sweep is why.** `persona_section` always draws `▪ personas N` first, so row 0
        always holds `None` — which makes `0 <= row` and `0 < row` behave identically
        through every paint, and the sweep reported the boundary as an exact equivalent.
        It is not one: that row 0 is a heading is the *composition's* choice, not this
        class's contract, and the day a column is drawn without one the two spellings stop
        agreeing. `_Viewport.repo_at` carries the same lower bound for the same reason and
        argues it — a negative index into a tuple answers the LAST row, which is the one
        wrong answer available here.
        """
        slots.CHIPS.publish(("docs", "forge"), "forge", 0)
        self.assertEqual(self.switched(0), "docs")
        self.assertIsNone(self.switched(1), "row 1 is the one you are on")
        self.assertIsNone(self.switched(-1),
                          "a negative row must not answer the last one")

    def test_a_frame_being_no_persona_makes_every_row_clickable(self):
        """`_persona_chip_cells` marks nobody active on a frame that has adopted no
        persona — the state a frame is in before its first `charter persona use`, and the
        one a plane with three personas and no default sits in indefinitely.

        The mark this column compares against is then the empty string, which is a name no
        chip carries, so every row switches. Asserted because the alternative is worse than
        it looks: a default that happened to equal the first persona's name would make
        exactly one row of a fresh frame silently dead.
        """
        with _roster(_chip("docs"), _chip("forge")):
            slots.persona_section(22, 10, terse=False)
        self.assertEqual(self.switched(1), "docs")
        self.assertEqual(self.switched(2), "forge")

    def test_the_name_published_is_the_raw_one_and_not_the_drawn_one(self):
        """`_persona_rows` runs every name through `tui.Cell`, which truncates it into a
        22-column pane (#472). What goes into `charter frame-switch --persona` has to be
        the name on disk — `_Tabs.publish`'s rule one axis over."""
        long = "a-persona-with-a-very-long-name"
        with _roster(_chip("steward", active=True), _chip(long)):
            lines = slots.persona_section(22, 10, terse=False)
        self.assertNotIn(long, tui.strip_ansi(lines[2]), "the drawn name should be cut")
        self.assertEqual(self.switched(2), long)


class TheBadgeColumnExplainsInstead(unittest.TestCase):
    """`slots._Chips.hit` on the BADGE cell — #753's in-frame half.

    The auditor read `frame/slots.py` to learn that `◦` is the ordinary state of a persona
    with no vault. The docs table added by this change answers that outside the frame;
    this is the answer *inside* it, and it lands on the dwell #729 built rather than on a
    surface of its own.

    **What is asserted is the CELL boundary**, because that is the whole mechanism: a
    column one to the left of the badge cell must still switch, and one column into it
    must explain. A test that only asked "does a badge click explain" would go green over
    a boundary off by the width of a name.
    """

    def setUp(self) -> None:
        slots.CHIPS.forget()
        self.addCleanup(slots.CHIPS.forget)

    def _drawn(self, *chips, width=22, height=10):
        with _roster(*chips):
            return slots.persona_section(width, height, terse=False)

    def test_a_pane_too_narrow_for_a_name_gets_no_badge_column_rather_than_a_negative_one(self):
        """`slots._badge_width`'s own contract — *a column is never a negative number of
        cells* — asserted on the function rather than on what a caller does next.

        **This case exists because the deletion sweep found the old spelling to be an
        exact equivalent.** The floor used to sit inside the `min`
        (`min(widest, max(0, width - _NAME_MIN_W))`), where dropping it changed nothing an
        input could observe: a negative width fell straight into `_persona_rows`'
        ``badge_w <= 0`` branch and was treated as zero there. Measured over 3,280 cell
        configurations at every width from 0 to 40, the unclamped version drew byte-identical
        rows and resolved every click identically.

        Moved outside the `min` it is this function's answer instead of a caller's
        accident, and one call pins it. `_NAME_MIN_W` is 12, and `_PAD_MIN_CONTENT` is the
        same constant — so `pad_for` already guarantees a padded pane keeps 12 columns and
        the only way here is a pane narrower than that, which a plane can ask for with a
        `[[frame.component]]` of its own.
        """
        cells = [_chip("a", badges=" ⚑"), _chip("b", badges=" ✎47 ⚡3")]
        widest = max(tui.width(c.badges) for c in cells)
        self.assertEqual(slots._NAME_MIN_W, 12, "this case is written against that floor")
        for width in (0, 1, 8, 11, 12):
            self.assertEqual(slots._badge_width(cells, width), 0,
                             f"width {width} leaves nothing for a name, so no badge column")
        self.assertEqual(slots._badge_width(cells, 13), 1,
                         "one column past the floor buys exactly one badge cell")
        self.assertEqual(slots._badge_width(cells, 100), widest,
                         "a wide pane gives the badges the widest one on screen")

    def test_the_column_the_badges_were_drawn_in_is_where_explaining_starts(self):
        """`▫ docs ◦             ⚑` on a 22-column pane: the `⚑` cell is one column wide
        at the right edge, so column 21 explains and column 20 — the last cell of the name
        — still switches."""
        self._drawn(_chip("steward", active=True), _chip("docs", badges="⚑"))
        at = slots.CHIPS._badge_at
        self.assertEqual(at, 21, "the badge cell should be the pane's last column")
        self.assertEqual(slots.CHIPS.hit(2, at), slots._Hit(True, "docs"))
        self.assertEqual(slots.CHIPS.hit(2, at - 1), slots._Hit(False, "docs"))

    def test_the_badges_on_the_persona_you_are_explain_where_the_name_refuses(self):
        """The asymmetry, stated: re-adopting yourself is the nothing `_Tabs.switch_to`
        refuses, but *"what does the flag on my own row mean"* is the commonest form of
        the question this answers."""
        self._drawn(_chip("steward", active=True, badges="⚑"), _chip("docs"))
        at = slots.CHIPS._badge_at
        self.assertIsNone(slots.CHIPS.hit(1, 0), "the name you are on switches nothing")
        self.assertEqual(slots.CHIPS.hit(1, at), slots._Hit(True, "steward"))

    def test_a_row_naming_no_persona_explains_nothing_even_in_the_badge_column(self):
        """`_persona_rows` draws the `…(+N more)` row FULL WIDTH with no badge cell, so a
        click where the badges would have been is a click on a sentence about the list."""
        lines = self._drawn(_chip("steward", active=True, badges="⚑"),
                            _chip("docs"), _chip("forge"), height=3)
        self.assertEqual(tui.strip_ansi(lines[2]).strip(), "…(+2 more)")
        for col in (0, 3, 21, 40):
            self.assertIsNone(slots.CHIPS.hit(2, col), f"column {col}")
        self.assertIsNone(slots.CHIPS.hit(0, 21), "the heading explains nothing")

    def test_a_column_with_no_badges_at_all_is_all_name(self):
        """`_badge_width` answers zero when no chip carries a badge, so no badge cell is
        drawn and `badge_at` is published as 0 — which `hit` reads as "there is no badge
        column" rather than as "column 0 is one"."""
        self._drawn(_chip("steward", active=True), _chip("docs"))
        self.assertEqual(slots.CHIPS._badge_at, 0)
        for col in (0, 10, 21):
            self.assertEqual(slots.CHIPS.hit(2, col), slots._Hit(False, "docs"),
                             f"column {col} should still be the name")

    def test_the_legend_names_every_glyph_the_docs_table_does(self):
        """One legend for every row, because `PersonaChip.badges` is a RENDERED string by
        the time this module sees it and a handler is handed no ctx (§4f) — so saying what
        *this* persona's badges mean would need either decoding glyphs back out or a
        second reading of the plane. It has to be worth reading on its own, which means
        naming every glyph `docs/frame.md`'s own table does."""
        for glyph in ("◦", "!", "⚑", "✗", "✎", "◌", "⚡"):
            self.assertIn(glyph, slots.BADGE_LEGEND, f"{glyph} is unexplained")

    def test_the_legend_fits_an_ordinary_attention_row(self):
        """`state.say` hands it to `_bottom`, where it is top priority and still
        `tui.truncate`d to the pane. 120 columns is the width `docs/frame.md` uses for
        every frame it draws, and a legend cut on an ordinary terminal would be a reminder
        that needs the reference to make sense of it."""
        self.assertLessEqual(tui.width(slots.BADGE_LEGEND), 120)

    def test_the_legend_is_one_line(self):
        """`state.say` stores an expiry line and then the text, so a newline in the value
        reads back truncated at it — that function contains its argument for exactly this,
        and a constant that needed containing would be relying on the repair."""
        self.assertNotIn("\n", slots.BADGE_LEGEND)


class TheHandlersActOnThePressAndNothingElse(unittest.TestCase):
    """`builtins._strip_events` and `builtins._persona_events`.

    The half of §4i both handlers inherit word for word from `_repos_events`: a `click`
    release can arrive with no matching press — `frame/overlay.py` measured a drag begun on
    a pane border delivering exactly one release — so charter acts on the press, which is
    where the operator actually pointed.
    """

    def setUp(self) -> None:
        slots.DOORS.forget()
        slots.CHIPS.forget()
        self.addCleanup(slots.DOORS.forget)
        self.addCleanup(slots.CHIPS.forget)
        self.spawned: list[tuple] = []
        patch = mock.patch("charter.frame.builtin_actions._spawn",
                           lambda argv, *, fid: self.spawned.append((tuple(argv), fid)))
        patch.start()
        self.addCleanup(patch.stop)
        #: Every `state.say` the handler made, as `(fid, message, seconds)`. Captured
        #: rather than written, because what is under test is that the handler reaches for
        #: the frame's own dwell at all — `state.say`'s file format is its own module's.
        self.said: list[tuple] = []
        say = mock.patch.object(
            state, "say",
            lambda f, msg, *, seconds=None: self.said.append((f, msg, seconds)))
        say.start()
        self.addCleanup(say.stop)

    # -- the strips ---------------------------------------------------------- #

    def test_a_press_on_a_door_starts_the_palette_for_this_frame(self):
        slots.DOORS.publish(range(10, 20))
        on_event = builtins._strip_events(FID)
        self.assertFalse(on_event(_click(col=12)), "nothing this pane draws has changed")
        self.assertEqual(len(self.spawned), 1)
        argv, fid = self.spawned[0]
        self.assertEqual(argv[-1], "frame-palette")
        self.assertEqual(fid, FID, "the frame is closed over, never read back out of "
                                   "an environment one tmux server shares")

    def test_a_release_over_a_door_opens_nothing(self):
        slots.DOORS.publish(range(10, 20))
        self.assertFalse(builtins._strip_events(FID)(_click(col=12, pressed=False)))
        self.assertEqual(self.spawned, [], "a drag begun elsewhere never pointed here")

    def test_a_middle_or_right_press_on_a_door_opens_nothing(self):
        """Middle-click is paste on every terminal an operator has used and right-click
        opens their emulator's own menu — `builtins._ACT_BUTTON`'s whole argument."""
        slots.DOORS.publish(range(10, 20))
        for button in ("middle", "right"):
            self.assertFalse(builtins._strip_events(FID)(_click(col=12, name=button)))
        self.assertEqual(self.spawned, [])

    def test_a_press_on_a_cell_no_field_was_drawn_into_opens_nothing(self):
        slots.DOORS.publish(range(10, 20))
        for col in (0, 9, 20, 4096, -1):
            self.assertFalse(builtins._strip_events(FID)(_click(col=col)), f"col {col}")
        self.assertEqual(self.spawned, [])

    def test_the_palette_argv_is_the_one_the_hotkey_bind_runs(self):
        """`commands_frame.conf_text` binds `frame-palette "#{client_name}"`; this is the
        same subcommand with the client left off, because a panel is not a `run-shell`
        child of a keypress and has no presser to name."""
        slots.DOORS.publish({3})
        builtins._strip_events(FID)(_click(col=3))
        argv, _ = self.spawned[0]
        self.assertEqual(argv[-1:], ("frame-palette",))

    # -- the sidebar --------------------------------------------------------- #

    def test_a_press_on_a_persona_row_starts_that_frames_persona_switch(self):
        slots.CHIPS.publish((None, "steward", "docs"), "steward", 0)
        on_event = builtins._persona_events(FID)
        self.assertFalse(on_event(_click(row=2)),
                         "the switch bumps the frame from its own process")
        self.assertEqual(len(self.spawned), 1)
        argv, fid = self.spawned[0]
        self.assertEqual(argv[-3:], ("frame-switch", "--persona", "docs"))
        self.assertEqual(fid, FID)

    def test_a_press_on_the_persona_you_are_starts_nothing(self):
        slots.CHIPS.publish((None, "steward"), "steward", 0)
        self.assertFalse(builtins._persona_events(FID)(_click(row=1)))
        self.assertEqual(self.spawned, [])

    def test_a_release_on_a_persona_row_starts_nothing(self):
        slots.CHIPS.publish((None, "steward", "docs"), "steward", 0)
        self.assertFalse(builtins._persona_events(FID)(_click(row=2, pressed=False)))
        self.assertEqual(self.spawned, [])

    def test_a_press_on_a_row_naming_no_persona_starts_nothing(self):
        slots.CHIPS.publish((None, "steward", "docs"), "steward", 0)
        for row in (0, 3, 400, -1):
            self.assertFalse(builtins._persona_events(FID)(_click(row=row)), f"row {row}")
        self.assertEqual(self.spawned, [])

    def test_a_press_on_the_badge_column_says_the_legend_and_starts_nothing(self):
        """#753 in the frame: the answer goes on the attention row through #729's dwell,
        which is a different pane and a different process — `state.say` bumps the version
        and that pane's own poll draws it. Nothing is spawned, because nothing is being
        started."""
        slots.CHIPS.publish((None, "steward", "docs"), "steward", 20)
        self.assertFalse(builtins._persona_events(FID)(_click(row=2, col=21)))
        self.assertEqual(self.spawned, [], "explaining a glyph starts no process")
        self.assertEqual(len(self.said), 1)
        fid, message, seconds = self.said[0]
        self.assertEqual(fid, FID)
        self.assertEqual(message, slots.BADGE_LEGEND)
        self.assertEqual(seconds, state.REFUSAL_SECONDS,
                         "a legend is read rather than glanced at, so it takes the "
                         "longer of the two dwells")

    def test_a_press_on_the_name_beside_it_still_switches(self):
        """The control for the case above: the two cells of one row mean two things, and a
        handler that had collapsed them would pass one of these and fail the other."""
        slots.CHIPS.publish((None, "steward", "docs"), "steward", 20)
        self.assertFalse(builtins._persona_events(FID)(_click(row=2, col=3)))
        self.assertEqual(self.said, [], "switching says nothing on the attention row")
        self.assertEqual(self.spawned[0][0][-3:],
                         ("frame-switch", "--persona", "docs"))

    def test_a_release_on_the_badge_column_says_nothing(self):
        """§4i does not stop applying because the outcome is a sentence: a drag begun on a
        pane border delivers exactly one release, and it never pointed here."""
        slots.CHIPS.publish((None, "steward", "docs"), "steward", 20)
        self.assertFalse(
            builtins._persona_events(FID)(_click(row=2, col=21, pressed=False)))
        self.assertEqual(self.said, [])
        self.assertEqual(self.spawned, [])

    def test_a_press_on_the_badge_column_of_the_persona_you_are_still_explains(self):
        slots.CHIPS.publish((None, "steward"), "steward", 20)
        self.assertFalse(builtins._persona_events(FID)(_click(row=1, col=21)))
        self.assertEqual(len(self.said), 1)
        self.assertEqual(self.spawned, [])

    def test_the_switch_argv_is_the_one_the_workspace_bar_uses_one_option_over(self):
        """`commands_frame.cmd_switch` takes both nouns, and going through it is what keeps
        `switch.to_persona`'s refusals reaching the operator on the frame's own status
        line — a surface no panel process has."""
        self.assertEqual(builtins._PERSONA_SWITCH[0], builtins._WORKSPACE_SWITCH[0])
        self.assertEqual(builtins._PERSONA_SWITCH[1], "--persona")


class TheComponentsDeclareWhatTheyHandle(unittest.TestCase):
    """The registry half — a declaration is what makes `panel._run` build a dispatcher.

    #742 and #751 were both, mechanically, the same defect `chats` and `workspaces` had
    before #725: a component with no `events` is a caption that happens to draw nouns.
    `events.Dispatcher._deliver` drops a kind the component never declared before it
    reaches any handler, so the declaration is the feature.
    """

    def setUp(self) -> None:
        self.reg = builtins.build(FID)
        self.by_id = {c.id: c for c in self.reg.all()}

    def test_the_two_strips_and_the_sidebar_declare_click(self):
        for cid in ("identity", "attention", "sidebar"):
            self.assertEqual(self.by_id[cid].events, ("click",), cid)
            self.assertEqual(events.wanted(self.by_id[cid]), ("click",), cid)

    def test_no_charter_component_is_left_handling_nothing_but_the_sidebars_parts(self):
        """The four PLACED panels all declare a kind now. The three that do not are the
        sidebar's own parts, and `registry.Registry` refuses a part that declares one."""
        silent = sorted(c.id for c in self.reg.all() if not c.events)
        self.assertEqual(silent, ["changes", "personas", "todos"])

    def test_a_part_declaring_events_is_still_refused(self):
        """The rule this change had to work with rather than around: a part is never
        placed, so charter would dispatch to the parent and a child's declaration would
        build a handler that received nothing, ever."""
        from charter.frame.component import Component, Fill
        from charter.frame.registry import ComponentError, Registry

        reg = Registry()
        reg.register(Component(id="part", title="part", edge="right", size=Fill(),
                               needs=(), render=lambda ctx: [],
                               events=("click",), on_event=lambda ev: False))
        with self.assertRaises(ComponentError) as caught:
            reg.register(Component(id="whole", title="whole", edge="right", size=Fill(),
                                   needs=(), render=lambda ctx: [],
                                   children=("part",)))
        self.assertIn("declares events", str(caught.exception))

    def test_neither_strip_declares_scroll(self):
        """A one-row pane has nothing a wheel could move, so a handler for it could only
        ever answer False. `events.Dispatcher.open` charges the same `overlay.MOUSE_ON` for
        one pointer kind as for two, so declaring it would cost nothing and mean nothing —
        which is exactly what makes it wrong to declare."""
        for cid in ("identity", "attention"):
            self.assertNotIn("scroll", self.by_id[cid].events, cid)


if __name__ == "__main__":
    unittest.main()
