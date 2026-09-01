"""The sidebar's `…(+N more)` todos are reachable without leaving the frame — #742.

The issue reported the sidebar as a list that looks like a list you can act on and handles
nothing: *"Click `forge` → nothing. Wheel over the todo list → nothing. `F2` has no todo
action of any kind."* #765 answered the persona half — a click on a name switches, a click
on the badges explains them — and left this half open on the record, because every cheap
route to the hidden todos was worse than the honest count.

**What ships here is a KEY, and the ordering is the design rather than an easier subset.**
`[frame] mouse` is off on the shipped default, so a pointer-only answer would be inert on
exactly the planes this was reported from; and `docs/frame.md`'s rule runs the same way —
a pointer affordance always has a key or a palette row beside it, so the keyboard route is
the half that has to exist. A wheel over the section can be added on top of this later. It
could not have stood in for it.

**What is asserted here is the SENTENCE, not that something happened.** The row's whole
output is one line on the palette's header (`commands_frame._again`), so "it reported
something" is satisfied by every wrong answer this could give — the wrong todo, the wrong
position, a position counted against the clipped list rather than the plane's own total.
Each of those is a literal below.

**And the row that must go on answering nothing is asserted too.** `…(+N more)` stands for
todos it does not name, so it cannot resolve to one — `slots._Chips.hit`'s rule for the
persona column's own overflow line, one section down. This closes #742 by giving the
CONTENT a route, not by making that row a door.
"""

from __future__ import annotations

import os
import sys
import unittest
from unittest import mock

from charter import statusline, tui
from charter.frame import action as faction
from charter.frame import builtin_actions, gather, slots, state
from tests._isolation import PersonaIso

FID = "f-todo-route"


def _snapshot(titles, *, total=None) -> dict:
    return {"gathered_at": 0.0, "workspace": "w", "current_repo": None,
            "repos": [], "worktrees": [],
            "todos": [{"title": t} for t in titles],
            "todo_count": len(titles) if total is None else total}


class TheTodosHaveAPaletteRow(PersonaIso, unittest.TestCase):
    """`builtin_actions._register_todos` — `F2`, `todo`, Enter, Enter, Enter."""

    def setUp(self) -> None:
        super().setUp()
        state.frame_dir(FID, create=True)

    def _reg(self):
        return builtin_actions.build(FID, current_density="normal", current_chrome="off")

    def _run(self, reg, snapshot):
        a = reg.get("todo.next")
        return a.run(faction.build(a.touches, fid=FID, snapshot=snapshot))

    def test_the_palette_offers_one_row_for_the_todos(self):
        self.assertEqual([a.title for a in self._reg().all() if a.id.startswith("todo.")],
                         ["todo: read the next open todo"])

    def test_it_reads_the_todos_out_in_the_order_the_sidebar_draws_them(self):
        """`open_todos` is oldest-first and the sidebar's rows are its own top, so the row
        after the last one on screen is the next press — not a fresh sample. Asserted as
        the three exact sentences, because "it said something" is satisfied by any order."""
        reg, snap = self._reg(), _snapshot(["oldest", "middle", "newest"])
        self.assertEqual([self._run(reg, snap) for _ in range(3)],
                         ["todo 1/3: oldest", "todo 2/3: middle", "todo 3/3: newest"])

    def test_it_wraps_rather_than_pressing_nothing_at_the_end(self):
        """`_select`'s argument for wrapping: a row that visibly does nothing reads as
        broken and costs a whole `F2` to find out. The POSITION is what stops a wrap
        looking like a repeat, which is why the fraction is in the sentence."""
        reg, snap = self._reg(), _snapshot(["one", "two"])
        self.assertEqual([self._run(reg, snap) for _ in range(3)],
                         ["todo 1/2: one", "todo 2/2: two", "todo 1/2: one"])

    def test_the_cursor_belongs_to_one_palette_and_starts_again_at_the_top(self):
        """The cell is closed over in the registry, and `build` makes a fresh registry per
        palette. An operator who opens `F2` to read their todos means the first one — a
        cursor that survived the last palette would answer from a list they have stopped
        looking at."""
        snap = _snapshot(["one", "two"])
        first = self._reg()
        self.assertEqual(self._run(first, snap), "todo 1/2: one")
        self.assertEqual(self._run(first, snap), "todo 2/2: two")
        self.assertEqual(self._run(self._reg(), snap), "todo 1/2: one")

    def test_it_writes_nothing_and_bumps_nothing(self):
        """`_select` records the repo selection through `state` because a PANE redraws
        from it. Nothing redraws from this — the answer is the sentence — so a `state`
        write plus the `state.bump` that wakes every panel would be four panels repainting
        per keypress to move a number nobody else reads."""
        was = state.version(FID)
        self._run(self._reg(), _snapshot(["one", "two"]))
        self.assertEqual(state.version(FID), was)

    def test_it_starts_no_process_at_all(self):
        """Unlike the density and surface rows, and for `_select`'s measured reason: this
        is arithmetic over a snapshot the palette already read, and `cmd_palette` joins the
        invocation before it closes the pane."""
        with mock.patch.object(builtin_actions.subprocess, "Popen",
                               side_effect=AssertionError("started a process")):
            self.assertEqual(self._run(self._reg(), _snapshot(["one"])), "todo 1/1: one")

    def test_the_position_is_counted_against_the_planes_own_total(self):
        """`gather._MAX_TODOS` bounds the LIST the cache holds and `todo_count` records
        what was there before the bound, so a plane with four hundred open todos and
        twenty cached must not be told `3/20` under a sidebar heading saying `todos 400`.
        The row says which list it is walking and how long the real one is."""
        snap = _snapshot([f"todo {i}" for i in range(20)], total=400)
        self.assertEqual(self._run(self._reg(), snap), "todo 1/20 of 400: todo 0")

    def test_an_unclipped_list_says_so_by_saying_nothing_extra(self):
        """The `of N` clause is the CLIP, not decoration: on the ordinary plane the cache
        holds everything and a second number would be the same number twice."""
        self.assertEqual(self._run(self._reg(), _snapshot(["one", "two"])),
                         "todo 1/2: one")

    def test_a_workspace_with_nothing_open_is_told_why_rather_than_pressing_nothing(self):
        reg = self._reg()
        offers = {o.id: o for o in reg.offers(fid=FID, snapshot=_snapshot([]))}
        self.assertFalse(offers["todo.next"].available)
        self.assertIn("charter workspace todo", offers["todo.next"].reason)
        self.assertTrue({o.id: o for o in reg.offers(
            fid=FID, snapshot=_snapshot(["one"]))}["todo.next"].available)

    def test_run_on_an_empty_list_answers_nothing_rather_than_raising(self):
        """`available` refuses the row, so the palette never reaches this — but `run` is
        callable without that check and `% 0` is the one way this could raise instead of
        answer. `Palette.report`'s own contract for an empty answer is "an action that
        answered nothing has nothing to report"."""
        self.assertEqual(self._run(self._reg(), _snapshot([])), "")
        self.assertEqual(self._run(self._reg(), {}), "")

    def test_a_todo_with_no_title_reads_as_nothing_and_never_as_the_word_None(self):
        """The other half of "a gather cache is JSON another process wrote". A row with no
        `title` key is a row, so it is counted and walked — and the sentence ends after the
        colon rather than reporting Python's `None` as the thing you are supposed to do."""
        snap = _snapshot([])
        snap["todos"] = [{}, {"title": "real"}]
        snap["todo_count"] = 2
        said = self._run(self._reg(), snap)
        self.assertEqual(said, "todo 1/2: ")
        self.assertNotIn("None", said)

    def test_a_row_that_is_not_a_row_is_skipped_by_both_halves(self):
        """A gather cache is JSON another process wrote; `ctx` contains it no further than
        making the list a tuple. `available` and the walk ask the same filter, so a row
        listed as available cannot then answer "nothing"."""
        snap = _snapshot([])
        snap["todos"] = ["not a mapping", {"title": "real"}, 7]
        reg = self._reg()
        self.assertTrue({o.id: o for o in reg.offers(
            fid=FID, snapshot=snap)}["todo.next"].available)
        self.assertEqual(self._run(reg, snap), "todo 1/1: real")

    def test_it_survives_being_repeated_which_is_what_makes_it_one_palette(self):
        """`repeat=True`. Without it each Enter closes the pane, kills the process and
        re-splits for the next — the fourteen-keystroke, three-cycle cost #746 measured on
        the repo rows, paid once per hidden todo."""
        self.assertTrue(self._reg().get("todo.next").repeat)


class TheCountTheHeadingDrawsIsTheCountTheRowWalks(PersonaIso, unittest.TestCase):
    """`slots.todo_total` — one answer, read by the sidebar's heading and by the palette
    row, because two spellings of "how many are open" is the drift #742's fix cannot have.
    """

    def _render(self, *, cols=40, rows=26) -> list[str]:
        with mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((cols, rows))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            return [tui.strip_ansi(ln)
                    for ln in slots.render("right", FID).split("\n")]

    def test_the_heading_and_the_row_agree_on_a_clipped_cache(self):
        snap = _snapshot([f"todo {i}" for i in range(20)], total=400)
        gather.save(FID, snap)
        self.assertIn(f"{statusline._HEAD_PAD}todos 400", self._render())
        reg = builtin_actions.build(FID, current_density="normal", current_chrome="off")
        a = reg.get("todo.next")
        said = a.run(faction.build(a.touches, fid=FID, snapshot=snap))
        self.assertIn("of 400", said)

    def test_a_count_smaller_than_the_list_is_refused_by_the_shared_answer(self):
        """A cache whose two halves disagree — `todo_count` below the number of items it
        actually holds. Falling back to the list is what stops the frame reporting fewer
        todos than it is drawing."""
        self.assertEqual(slots.todo_total({"todos": [{"title": "a"}, {"title": "b"}],
                                           "todo_count": 1}), 2)

    def test_a_cache_written_before_the_count_existed_answers_what_it_holds(self):
        self.assertEqual(slots.todo_total({"todos": [{"title": "a"}]}), 1)

    def test_nothing_at_all_is_zero_rather_than_a_raise(self):
        self.assertEqual(slots.todo_total({}), 0)

    def test_a_count_that_is_not_a_number_answers_the_list_rather_than_raising(self):
        """The `isinstance` is what stands between a corrupt cache and a `TypeError` on a
        SIDEBAR REPAINT. `gather`'s cache is JSON another process wrote, so `todo_count` is
        whatever is in that file; the arithmetic below it is a `max` against an `int`, and
        `max("many", 2)` raises rather than answering. A pane that raises is a dead pane
        the operator has to relaunch to be rid of, so anything that is not a count is
        treated as no count at all — which is the same answer an older cache gets."""
        for junk in ("many", None, [7], {"n": 7}, 4.5):
            with self.subTest(todo_count=junk):
                self.assertEqual(
                    slots.todo_total({"todos": [{"title": "a"}], "todo_count": junk}), 1)


class TheOverflowRowStillAnswersNothing(PersonaIso, unittest.TestCase):
    """#742 is closed by giving the CONTENT a route, never by making that row a door.

    `slots._Chips.hit` is the sidebar's one resolver and it publishes a row per persona;
    every row below the persona column — the blank separators, every todo row and its
    `…(+N more)` — is out of bounds. A row that stands for todos it does not name cannot
    resolve to one, which is the same case the persona column's own overflow line carries.
    """

    def test_no_row_of_the_todo_section_resolves_to_anything(self):
        for i in range(6):
            self.make_persona(f"p{i}")
        gather.save(FID, _snapshot([f"todo {i}" for i in range(9)]))
        with mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((22, 20))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            out = slots.render("right", FID)
        lines = [tui.strip_ansi(ln) for ln in out.split("\n")]
        first_todo = next(i for i, ln in enumerate(lines) if "todos" in ln)
        self.assertTrue(any("…(+" in ln for ln in lines[first_todo:]),
                        f"this pane is not overflowing its todos: {lines}")
        for row in range(first_todo, len(lines)):
            for col in (0, 2, 10, 21):
                self.assertIsNone(slots.CHIPS.hit(row, col),
                                  f"row {row} col {col} of the todo section resolved: "
                                  f"{lines[row]!r}")


if __name__ == "__main__":      # pragma: no cover
    unittest.main()
