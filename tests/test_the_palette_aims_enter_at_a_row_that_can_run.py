"""**The cursor opens on a row that can run — #931.**

Filed off a real frame while #929 was being reproduced, and filed rather than folded in
because it needed a decision. The tab menu — what a right press on a tab opens, and what
the chat strip's `-` opens — has two rows, transcript then close. Captured on a chat that
has never been quit, which is the ordinary state of a chat running normally:

    chat alpha.1 · 2 to choose from
    >   chat: previous transcript — alpha.1     no previous transcript for this chat…
        chat: close alpha.1 — stop it and do not bring it back

The cursor opened on a **refused** row, so `-` then Enter started nothing at all: the
surface closed, the note went to the attention row, and the chat was still there. That is
one keystroke from the report #929 came from — *"after choosing close - its not closing"* —
reached by a different route.

**The rule was already written down six times; it was never the code.** `leave.open_rows`,
`commands_frame._catalogue`, `commands_frame._as_a_drawer`, `tabmenu.catalogue`,
`docs/frame.md` and the 0.56.0 release note all say *the cursor starts on the first row
that can run*, and each puts a destructive row at the bottom of a list on the strength of
it. `Palette._refilter` said ``self._sel = 0``. `palette.aim` is that sentence, and this
file is the pin on it.

**The two rules that looked opposed, and why they were not.** *Do not open on a row that
does nothing* (`leave.confirm_rows`' own measurement) and *do not open on a destructive
row* (`leave.open_rows`') are in direct conflict on a two-row menu where the first row is
usually refused — but only if one mechanism has to serve both. It does not: the second is
enforced by ORDER, which nothing here moves, and the first by which listed row the cursor
is put on. `TheOrderingGuardIsNotWhatMoved` is the half of this file that proves the first
change did not buy itself out of the second.

**Where that leaves the tab menu, said plainly because it is the one surface it bites.**
On a chat with no capture the only row that can run is `chat: close`, so the cursor opens
there. It is a DOORWAY — `tabmenu.chose` refuses it by id and `tabmenu.opens` replaces the
surface with `leave.confirm_rows` — so there are still two keypresses and a warning naming
the chat between the pointer and a stopped harness, which is what `leave.open_rows`' guard
is actually counting. `F2` is the surface that guard was written for, and there every row
above close can run.

**What was rejected**, each asserted below where it would show:

* opening with NO row selected — `overlay.Surface.selected` answers ``None`` for *there
  are no rows*, and `tabmenu.act` and `commands_frame._draw_palette` both read ``None`` as
  a CANCEL, so Enter on an unaimed surface would tear the pane down silently and change
  nothing. That is the reported defect reintroduced by its own fix;
* reordering the menu so close comes first — `leave.open_rows`' guard, at the one surface
  that opens under a pointer;
* dropping the refused row — #512;
* skipping refused rows under the arrow keys as well — a list that will not stop on a row
  is a list whose reason column cannot be read.

Every assertion here says what the surface DOES. #930's finding about this same `-` is
that *"every assertion about the `-` said what it didn't do, so a `-` wired to nothing
passed them all"*, and a cursor rule is even easier to test that way: `assertNotEqual(sel,
refused_row)` passes on a palette with no rows at all.
"""

from __future__ import annotations

import unittest
from unittest import mock

from charter import commands_frame
from charter.frame import builtin_actions, leave, overlay, palette, reopen, tabmenu

from tests._isolation import PersonaIso
from tests.test_frame_chat_switch import _plant


def _row(rid: str, *, refused: bool = False) -> overlay.Row:
    return overlay.Row(id=rid, title=f"row {rid}", refused=refused)


class TheCursorOpensOnTheFirstRowThatCanRun(unittest.TestCase):
    """`palette.aim` on its own, over the five shapes a list of rows comes in."""

    def test_a_runnable_first_row_keeps_the_cursor_where_it_always_was(self):
        """The compatibility claim this whole change rests on: `F2`'s catalogue leads with
        rows that can run, so the palette an operator knows opens on the row it always
        opened on. A version of `aim` that preferred a refused row, or counted from the
        end, fails here and nowhere else."""
        self.assertEqual(palette.aim((_row("a"), _row("b", refused=True), _row("c"))), 0)

    def test_a_refused_first_row_is_skipped_and_the_next_runnable_one_is_taken(self):
        """The reported case, at its smallest. `1`, not "not 0": which row Enter is aimed
        at is the whole defect."""
        self.assertEqual(palette.aim((_row("a", refused=True), _row("b"))), 1)

    def test_it_skips_a_RUN_of_refused_rows_rather_than_stepping_over_one(self):
        """`leave.confirm_rows` draws one refused row per doomed chat, so "the next one"
        and "the next runnable one" are different answers on any plane with two chats."""
        self.assertEqual(
            palette.aim((_row("a", refused=True), _row("b", refused=True),
                         _row("c", refused=True), _row("d"))), 3)

    def test_a_list_where_nothing_can_run_aims_at_its_first_row(self):
        """Not −1, and not "no selection". A surface where every row is refused is not a
        surface whose cursor lies — `leave.confirm_rows`' nothing-left-to-stop plan is
        exactly one such row and its title says so. The cursor sits on it and Enter answers
        with its note."""
        self.assertEqual(palette.aim((_row("a", refused=True),
                                      _row("b", refused=True))), 0)

    def test_an_empty_list_answers_zero_so_the_window_arithmetic_is_handed_an_index(self):
        """`overlay.Surface._window` clamps against `len(rows)` and `Surface.selected`
        tests `self.rows` before indexing, so 0 is the value both already expect for a
        palette that filtered everything out."""
        self.assertEqual(palette.aim(()), 0)


class APaletteAimsItselfOnEveryEdit(unittest.TestCase):
    """The same rule at the surface, which is where an operator meets it."""

    CATALOGUE = (_row("refused.1", refused=True), _row("runs"),
                 _row("refused.2", refused=True))

    def test_the_palette_opens_with_enter_aimed_at_the_row_that_can_run(self):
        p = palette.Palette(catalogue=self.CATALOGUE)

        self.assertEqual(p.selected.id, "runs")
        self.assertFalse(p.selected.refused)

    def test_the_row_the_cursor_is_on_is_the_one_the_pane_marks(self):
        """One rung down: the text a terminal receives. `overlay.Surface.render` draws
        `_MARK[0]` on `_sel` and nothing on the rest, so this is the `>` the operator sees
        rather than the index behind it."""
        drawn = [ln for ln in palette.Palette(catalogue=self.CATALOGUE).render(60, 8)]
        marked = [ln for ln in drawn if overlay._MARK[0] in ln]

        self.assertEqual(len(marked), 1, drawn)
        self.assertIn("row runs", marked[0])

    def test_typing_re_aims_rather_than_going_back_to_the_top(self):
        """`_refilter` is called on every keystroke, so the rule has to hold for the list
        the operator narrowed to as well as for the one the palette opened with. Typing
        `refused` leaves two rows, neither runnable, and typing it away must land back on
        the runnable one rather than on whichever index survived."""
        p = palette.Palette(catalogue=self.CATALOGUE)
        for ch in "refused":
            p.handle(overlay.Event(kind=overlay.KEY, name=ch), 24)
        self.assertEqual([r.id for r in p.rows], ["refused.1", "refused.2"])
        self.assertEqual(p.selected.id, "refused.1",
                         "a list where nothing can run still aims at its first row")

        for _ in "refused":
            p.handle(overlay.Event(kind=overlay.KEY, name="backspace"), 24)
        self.assertEqual(p.selected.id, "runs")

    def test_the_arrow_keys_still_stop_on_a_refused_row(self):
        """**The rejected alternative, pinned where it would show.** Only the OPENING
        position is charter's to choose: an operator pressing `up` is aiming for
        themselves, and a row the cursor cannot land on is a row whose reason column cannot
        be read on a surface that has no other way to show it."""
        p = palette.Palette(catalogue=self.CATALOGUE)
        p.handle(overlay.Event(kind=overlay.KEY, name="up"), 24)

        self.assertEqual(p.selected.id, "refused.1")
        self.assertTrue(p.selected.refused)

    def test_enter_on_a_palette_with_rows_is_never_a_cancel(self):
        """**The other rejected alternative.** `overlay.Surface.handle` answers
        :data:`overlay.CHOOSE` and `run` hands back `selected`; if that were ``None`` for
        "nothing is aimed at", `tabmenu.act` and `commands_frame._draw_palette` would both
        take it for an Escape and close the pane having done nothing — the reported defect,
        rebuilt out of its own fix."""
        p = palette.Palette(catalogue=self.CATALOGUE)

        self.assertEqual(p.handle(overlay.Event(kind=overlay.KEY, name="enter"), 24),
                         overlay.CHOOSE)
        self.assertIsNotNone(p.selected)


class TheTabMenuAimsEnterAtSomethingItCanDo(PersonaIso, unittest.TestCase):
    """The reported surface, built the way `tabmenu.draw` builds it."""

    FID = "alpha.1"
    TAB = "alpha.1"

    def setUp(self) -> None:
        super().setUp()
        for chat in ("alpha.1", "alpha.2"):
            _plant(chat, workspace="alpha")

    def _menu(self) -> palette.Palette:
        """`tabmenu.draw`'s own two lines, so what is aimed at here is what a `-` aims
        at rather than a list this file composed to look like one."""
        return palette.Palette(catalogue=tabmenu.catalogue(self.TAB),
                               label=tabmenu.label(self.TAB), mouse=True)

    def _capture(self) -> None:
        path = reopen.transcript_path(self.TAB)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("what was on screen\n", encoding="utf-8")

    def test_on_a_chat_never_quit_enter_is_aimed_at_the_close_doorway(self):
        """The report, fixed. Said as *which row*, because "not the transcript row" would
        pass on a menu that had lost the close row too."""
        self.assertEqual(self._menu().selected.id, tabmenu.CLOSE_ID)

    def test_that_enter_really_opens_the_warning_that_names_the_chat(self):
        """**What the fix BUYS, which is the assertion #930 says this surface kept
        missing.** Not "Enter is not refused" — the surface the keypress reaches, built by
        `tabmenu.opens` from the row the cursor is really on, carrying `leave`'s own
        confirming row for this one chat."""
        chosen = self._menu().selected

        nxt = tabmenu.opens(chosen, self.TAB, live=("alpha.1", "alpha.2"))

        self.assertIsNotNone(nxt, "Enter on the aimed row opened no surface at all")
        self.assertEqual(nxt.label, leave.CLOSE)
        self.assertEqual(nxt.catalogue[0].id, leave.GO_ID.format(leave.CLOSE))
        self.assertIn(self.TAB, nxt.catalogue[0].title)

    def test_the_refused_row_is_still_listed_above_it_with_its_reason(self):
        """#512, which this change does not get to relax: the cursor moved and the rows did
        not. An operator who pressed `-` can still see that a transcript is a thing this
        menu offers, and read why they are not being offered one."""
        p = self._menu()

        self.assertEqual([r.id for r in p.rows],
                         [tabmenu.TRANSCRIPT_ID, tabmenu.CLOSE_ID])
        self.assertTrue(p.rows[0].refused)
        self.assertEqual(
            p.rows[0].note,
            "no previous transcript for this chat — one is captured when a plane is quit "
            "(`F2 → charter: quit`) and offered on the chat that comes back")

    def test_the_reason_is_on_screen_beside_the_row_and_not_only_in_the_data(self):
        """The refusal has to be legible to somebody who has just pressed `-`, which is a
        different question from whether the row carries a note: the row is no longer the
        one under the cursor, so the dim right-hand column is the only place it is said."""
        drawn = "\n".join(self._menu().render(140, 8))

        self.assertIn("no previous transcript for this chat", drawn)
        self.assertIn("chat: previous transcript", drawn)

    def test_a_chat_WITH_a_transcript_still_opens_on_its_transcript_row(self):
        """The negative control the case above needs, and the compatibility claim: nothing
        about a chat that has a capture changes. The cursor is on the first row because the
        first row can run, which is the same answer `_sel = 0` used to give for a different
        reason."""
        self._capture()
        p = self._menu()

        self.assertEqual(p.selected.id, tabmenu.TRANSCRIPT_ID)
        self.assertFalse(p.selected.refused)

    def test_and_that_enter_really_opens_that_chats_transcript(self):
        """Same shape as the close case: what the keypress STARTS, off the row the cursor
        is really on."""
        self._capture()
        spawned = []
        with mock.patch("charter.frame.builtin_actions._spawn",
                        side_effect=lambda argv, *, fid: spawned.append((argv, fid))):
            self.assertTrue(tabmenu.chose(self._menu().selected, self.TAB, fid=self.FID))

        self.assertEqual(len(spawned), 1, spawned)
        self.assertIn("frame-transcript", spawned[0][0])
        self.assertIn(self.TAB, spawned[0][0])

    def test_the_aimed_row_still_starts_nothing_by_itself(self):
        """§4i, and the reason a cursor on `chat: close` is answerable at all: the row the
        Enter lands on is a DOORWAY. `tabmenu.chose` refuses it by id, so the keypress that
        stops a harness is the NEXT one, on a surface that names the chat."""
        spawned = []
        with mock.patch("charter.frame.builtin_actions._spawn",
                        side_effect=lambda argv, *, fid: spawned.append((argv, fid))):
            started = tabmenu.chose(self._menu().selected, self.TAB, fid=self.FID)

        self.assertFalse(started)
        self.assertEqual(spawned, [])


class TheOrderingGuardIsNotWhatMoved(PersonaIso, unittest.TestCase):
    """**The half that proves the fix did not buy itself out of the other rule.**

    `leave.open_rows` keeps its destructive rows last *"because the palette's cursor starts
    on the first row that can run"* — a premise that only became true in #931. If aiming
    the cursor had been paid for by moving a row, this class is where it would show.
    """

    FID = "alpha.1"

    def setUp(self) -> None:
        super().setUp()
        for chat in ("alpha.1", "alpha.2"):
            _plant(chat, workspace="alpha")

    def test_the_two_rows_that_stop_something_are_still_last(self):
        rows = leave.open_rows(self.FID)

        self.assertEqual([r.id for r in rows],
                         [leave.OPEN_ID.format(leave.QUIT),
                          leave.OPEN_ID.format(leave.CLOSE)])

    def test_F2_still_opens_on_a_row_that_stops_nothing(self):
        """The guard's whole point, asked of the catalogue the palette is really built
        from rather than of a list composed to look like one: `F2` `Enter` may not reach
        `charter: quit` or `chat: close`. Aiming the cursor moves it FORWARD past refused
        rows, and both of those live at the end, so the only way this could break is a
        plane on which nothing else can run."""
        reg = builtin_actions.build(self.FID, current_density="normal",
                                    current_chrome="off")
        p = palette.Palette(
            catalogue=commands_frame._palette_catalogue(self.FID, reg, snapshot={}))

        self.assertFalse(leave.is_row(p.selected),
                         f"F2 Enter is aimed at {p.selected.id}, which stops something")
        self.assertFalse(p.selected.refused)


class AConfirmationStillAnswersItsOwnQuestion(PersonaIso, unittest.TestCase):
    """`leave.confirm_rows`, whose row order was decided by the defect this change fixes.

    Its docstring records putting the confirming row FIRST because *"the surface opened
    with `> alpha.1 · claude-code` selected and Enter bound to nothing at all"*. That is now
    fixed at the source, so the placement is a reading-order decision — and the surface has
    to keep working either way, which is what these two cases are.
    """

    def setUp(self) -> None:
        super().setUp()
        for chat in ("alpha.1", "alpha.2"):
            _plant(chat, workspace="alpha")

    def test_enter_is_aimed_at_the_confirming_row_and_not_at_a_warning_row(self):
        p = palette.Palette(
            catalogue=leave.confirm_rows(
                leave.plan(live=("alpha.1", "alpha.2"), focus="alpha"), verb=leave.QUIT),
            label=leave.QUIT)

        self.assertEqual(p.selected.id, leave.GO_ID.format(leave.QUIT))
        self.assertTrue(all(r.refused for r in p.rows[1:]),
                        "the per-chat rows stopped being a warning")

    def test_a_plan_with_nothing_to_stop_aims_at_the_row_that_says_so(self):
        """The one surface charter draws where NOTHING can run, and the case that pins
        `aim`'s fallback: there is no confirming row at all, so a cursor that refused to
        land on a refused row would have nowhere to go."""
        p = palette.Palette(
            catalogue=leave.confirm_rows(leave.plan(live=(), focus="alpha"),
                                         verb=leave.QUIT),
            label=leave.QUIT)

        self.assertEqual(len(p.rows), 1)
        self.assertEqual(p.selected.title, leave.NOTHING_OPEN)
        self.assertTrue(p.selected.refused)


if __name__ == "__main__":
    unittest.main()
