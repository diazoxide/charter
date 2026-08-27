"""A repo row narrower than the table gives up a column, not the end of one (#506).

`statusline.render` composed every repo row at `_LEFT_W` (95) and let `_columns` crop it
to whatever the terminal actually had. Every cell after the branch sits at a fixed offset
past the name and the branch, so on a narrower pane the CI mark and the open change were
cut off the right-hand end — and because the dirty marker rides on the BRANCH cell and
survived, a dirty, CI-failing, unpushed repo with an open change read as one that was
merely dirty.

    w= 90 |  ├─ charter    main*    ✗ failed      …|
    w= 80 |  ├─ charter    main*    ✗ fa…|
    w= 70 |  ├─ charter    main*          …|

An 80-column terminal is ordinary, and `$COLUMNS` is the status-line PANE's width — so
any split reaches it. This is the exact reading `frame/slots._table_lines` refuses one
surface over ("too narrow for the table is NO table, not a cut one", #488); the status
line cannot answer with nothing, because it is what an operator sees when they are not in
a frame, so it takes #506's other honest option — a narrower row SHAPE, decided where the
row is composed.

**The property under test is not "the row fits".** It fitted before: a crop always fits.
It is that a column is either drawn WHOLE or not drawn at all, and that the two cells a
reader acts on are the last ones to go. `✗ fa…` is worse than no CI cell, because it is a
false-clean reading wearing a real one's clothes.

Two levels, deliberately. The plan cases pin the losing ORDER at every width, where a
matrix is readable; the render cases pin that the crop at the end of `render()` no longer
eats what the plan decided — which is the actual defect, and which no amount of unit
testing of `_row_plan` would have caught.
"""

from __future__ import annotations

import os
import re
import unittest
from pathlib import Path
from unittest import mock

from charter import statusline as sl
from charter import tui
from tests._isolation import PersonaIso

D = Path("/tmp/a-plane/workspaces/w/charter")

#: A repo in the state that made #506 worth filing: dirty, unpushed, red pipeline, open
#: change. Every one of those four is a different cell, so a row drawn from it exercises
#: the whole losing order at once.
STATES = {D: {"dirty": True, "ahead": 2}}
BRANCHES = {D: "main"}
GL = {D: {"ci": "failed", "change": "500", "sigil": "#"}}

#: What the CI cell says when it is drawn at all. The LABEL, not the glyph: `✗` alone
#: survives a crop that ate the word, and "the glyph is there" is exactly the assertion
#: that would have passed against the defect.
CI_WHOLE = "✗ failed"
CHANGE_WHOLE = "#500"


def _row(budget: int) -> str:
    plan = sl._row_plan(budget)
    return sl._tree_cells(f"  ├─ ", "charter", D, STATES, BRANCHES, GL,
                          plan=plan).render(budget)[0]


class RowPlanCase(unittest.TestCase):
    """`_row_plan` alone: what a pane of N columns buys, at every N worth naming."""

    #: 95 is `_LEFT_W`; 74 is what an 80-column terminal lays out at (`render` takes
    #: `_SAFETY` off `$COLUMNS` and then the box's own chrome off that); the rest walk the
    #: plan down through every drop it can make. `24` is `render`'s own floor.
    WIDTHS = (200, 95, 94, 90, 80, 74, 72, 70, 60, 50, 45, 40, 35, 31, 24)

    def test_no_row_ever_exceeds_the_pane_it_was_planned_for(self):
        for w in self.WIDTHS:
            with self.subTest(width=w):
                self.assertLessEqual(tui.width(_row(w)), w, _row(w))

    def test_the_full_row_is_unchanged_at_the_table_width_and_above(self):
        """The regression guard on everything this did NOT set out to change. A pane at
        `_LEFT_W` or wider draws exactly what it drew before, cell for cell."""
        for w in (sl._LEFT_W, 200):
            with self.subTest(width=w):
                self.assertEqual(sl._row_plan(w), sl._FULL_ROW)

    def test_an_eighty_column_terminal_keeps_the_ci_mark_and_the_change(self):
        """#506's headline case, at the width it was reported from. Both cells WHOLE."""
        row = tui.strip_ansi(_row(74))
        self.assertIn(CI_WHOLE, row, row)
        self.assertIn(CHANGE_WHOLE, row, row)

    def test_the_branch_is_what_narrows_first(self):
        """The widest column and the least urgent: "which branch" is a fact you look up,
        a red pipeline is one you act on. Nothing else moves until it has run out."""
        plan = sl._row_plan(sl._LEFT_W - 10)
        self.assertEqual((plan.name, plan.ci, plan.mr),
                         (sl._FULL_ROW.name, sl._FULL_ROW.ci, sl._FULL_ROW.mr))
        self.assertEqual(plan.branch, sl._FULL_ROW.branch - 10)

    def test_the_ci_cell_is_the_last_of_the_three_to_go(self):
        """A strict order over the widths, asserted as an order rather than as three
        thresholds — a threshold is a constant, and a constant is what #592 is about."""
        gone = {"mr": None, "branch": None, "ci": None}
        for w in range(sl._LEFT_W, 15, -1):
            plan = sl._row_plan(w)
            for cell in gone:
                if gone[cell] is None and not getattr(plan, cell):
                    gone[cell] = w
        self.assertTrue(all(v is not None for v in gone.values()), gone)
        self.assertGreater(gone["mr"], gone["branch"], gone)
        self.assertGreater(gone["branch"], gone["ci"], gone)

    def test_a_cell_is_never_drawn_narrower_than_the_value_it_would_hold(self):
        """Shown whole or dropped whole, and this is the assertion that says so: at no
        width does the CI cell exist while holding a PREFIX of its label. `✗ fa…` is the
        false-clean reading a crop produces, and it is worse than an absent cell."""
        for w in self.WIDTHS:
            with self.subTest(width=w):
                row = tui.strip_ansi(_row(w))
                if "✗" in row:
                    self.assertIn(CI_WHOLE, row, row)
                if "#" in row:
                    self.assertIn(CHANGE_WHOLE, row, row)

    def test_the_markers_outlive_the_branch_name(self):
        """Dirty and ahead are true of the TREE; `main` is only what it is called. So the
        branch cell's floor is the markers, and a cell too narrow to hold both spends
        itself on them rather than on `ma…`."""
        narrow = sl._row_plan(60)
        self.assertTrue(narrow.branch, narrow)
        row = tui.strip_ansi(_row(60))
        self.assertIn("*", row, row)
        self.assertIn("↑2", row, row)

    def test_a_marker_is_never_cut_by_a_branch_name_beside_it(self):
        """The cell that `_row_plan` narrowed is then filled by `_branch_cell_for`, and
        the second half of the same rule lives there: measuring the branch against
        `_BRANCH_W` while the caller pads to something smaller composes 34 columns and
        lets `tui.Cell` cut the markers off the right-hand end — the markers being what
        the narrowing was protecting."""
        cell = sl._branch_cell_for("a-very-long-branch-name-indeed", "", "*↑2",
                                   "*↑2", True, sl._BRANCH_MIN_W)
        self.assertIn("*↑2", tui.strip_ansi(cell), cell)
        self.assertLessEqual(tui.width(cell), sl._BRANCH_MIN_W, cell)

    def test_at_the_floor_the_cell_holds_the_markers_and_no_stub(self):
        """Shown whole or dropped whole, one layer down from `_row_plan`'s own version.

        `assertIn("*↑2")` alone does not say this: a cell drawn as `a-…*↑2` contains the
        markers too, so a stub of the branch name passes that probe while spending the
        reader's last three columns on three characters of a name they cannot look up.
        The assertion has to be that the branch name is NOT there — measured, by a
        hand-check that dropped the floor to zero and watched every other case stay
        green."""
        cell = tui.strip_ansi(sl._branch_cell_for(
            "a-very-long-branch-name-indeed", "", "*↑2", "*↑2", True, sl._BRANCH_MIN_W))
        self.assertEqual(cell, "*↑2", cell)
        self.assertNotIn(tui.ELLIPSIS, cell, cell)

    def test_a_cell_the_plan_dropped_costs_no_gap_either(self):
        """A dropped cell built as a ZERO-WIDTH one still contributes `_GAP`, so every
        cell after it starts two columns further right than the plan believes and the row
        overruns the pane it was planned for — `tui.Row` then truncates, and what it
        truncates is the CI label.

        The BRANCH cell is the one this can be asked about, and that is a property of the
        losing order rather than an accident: the change and the CI mark are dropped last
        and second-last, so each is the final cell on its row when it goes and `_finish`
        strips the gap that would have followed it. The branch is dropped from the
        MIDDLE, with the CI mark still to its right. (`_tree_cells` says the same thing in
        its docstring, so a reader deleting one of the other two guards knows why nothing
        goes red.)"""
        w = 31                                   # narrow enough that the branch is gone
        plan = sl._row_plan(w)
        self.assertFalse(plan.branch, plan)
        self.assertTrue(plan.ci, plan)
        row = tui.strip_ansi(_row(w))
        self.assertLessEqual(tui.width(row), w, row)
        self.assertIn(CI_WHOLE, row, row)
        # And the gap really was the only thing between them: the CI cell begins exactly
        # one gap past the name cell, not two.
        self.assertEqual(tui.width(row[:row.index("✗")]), plan.name + len(sl._GAP), row)

    def test_the_name_is_the_one_cell_that_is_never_dropped(self):
        """A row with no name is not a row about anything."""
        for w in (*self.WIDTHS, 20, 16, 10, 1):
            with self.subTest(width=w):
                self.assertTrue(sl._row_plan(w).name, w)

    def test_two_repos_sharing_a_prefix_stay_apart_at_every_width(self):
        """What the name floor is FOR, asserted as the property rather than as its number.

        A test that spells `_NAME_MIN_W`'s value passes forever and sees nothing — #508's
        own finding about `28`, and the whole reason none of these cases names a width.
        What a floor on the name column actually buys is that a row still says WHICH repo
        it is about, and the sharp case is two repos in one workspace whose names share a
        prefix: `analytics-api` and `analytics-web` are indistinguishable the moment the
        column can no longer reach past `analytics-`.

        Found by the deletion sweep, which dropped a term from the floor and watched every
        other case here stay green. Measured against the mutants it produces: the floor as
        it stands has zero indistinguishable widths, and each smaller one has six, nine and
        nineteen.
        """
        a, b = Path("/tmp/p/analytics-api"), Path("/tmp/p/analytics-web")
        states = {a: {"dirty": True}, b: {}}
        branches = {a: "main", b: "main"}
        gl = {a: {"ci": "failed"}, b: {"ci": "success"}}
        for w in range(28, sl._LEFT_W + 1):
            with self.subTest(width=w):
                plan = sl._row_plan(w)
                cells = [tui.strip_ansi(
                    sl._tree_cells("  ├─ ", d.name, d, states, branches, gl,
                                   plan=plan).render(w)[0])[:plan.name].rstrip()
                    for d in (a, b)]
                self.assertNotEqual(
                    cells[0], cells[1],
                    f"at {w} columns both repos draw {cells[0]!r} — the name column is "
                    f"too narrow to say which row is about which repo")

    def test_a_drop_gives_its_leftover_back_rather_than_leaving_it_blank(self):
        """A dropped cell frees more columns than the deficit demanded. Leaving them
        blank while the repo name is cut to twelve characters spends a narrow pane on
        nothing — so the plan is never more than one cell short of its budget."""
        for w in range(sl._LEFT_W, 24, -1):
            plan = sl._row_plan(w)
            spare = w - sl._plan_width(plan)
            self.assertGreaterEqual(spare, 0, (w, plan))
            if plan.name < sl._FULL_ROW.name or (plan.branch and
                                                 plan.branch < sl._FULL_ROW.branch):
                self.assertEqual(spare, 0, (w, plan, "a shrunk cell left columns unspent"))


class SiblingRowsCase(unittest.TestCase):
    """One plan per render, shared by every row — because the rows are siblings.

    A per-row plan would be the same defect as a per-row width: a table whose rows
    disagree about their columns is not a table, and this layout has paid for that more
    than once (`_tree_cells`' own docstring is about exactly this).
    """

    OTHER = Path("/tmp/a-plane/workspaces/w/iam-service")

    def _pair(self, budget: int) -> tuple[str, str]:
        plan = sl._row_plan(budget)
        states = {**STATES, self.OTHER: {}}
        branches = {**BRANCHES, self.OTHER: "release/2026-08"}
        gl = {**GL, self.OTHER: {"ci": "success"}}
        rows = [sl._tree_cells(f"  ├─ ", label, d, states, branches, gl,
                               plan=plan).render(budget)[0]
                for label, d in (("charter", D), ("iam-service", self.OTHER))]
        return rows[0], rows[1]

    def test_two_rows_start_their_ci_cell_in_the_same_column(self):
        for w in (200, 95, 80, 74, 60, 50):
            with self.subTest(width=w):
                a, b = (tui.strip_ansi(r) for r in self._pair(w))
                if "✗" not in a or "✓" not in b:
                    continue
                self.assertEqual(tui.width(a[:a.index("✗")]),
                                 tui.width(b[:b.index("✓")]),
                                 "\n".join([a, b]))

    #: Real repo names, one per shape a width can be got wrong on. The name column is the
    #: one cell holding a value charter did not mint — a clone is a directory somebody
    #: else made — so it is where a character-versus-cell mistake would enter.
    #:
    #: `svc` is the CONTROL: it is inside every budget, so a failure naming it is about
    #: the table rather than about the name. The combining mark is built with `chr` and
    #: uses `q`, which has no precomposed form with an acute — the obvious `é` spelling
    #: is one codepoint and one cell, so `len` and `tui.width` agree about it and it
    #: quietly tests nothing.
    NAMES = {
        "short ascii control": "svc",
        "exactly the name column's width": "a" * sl._NAME_W,
        "one character over it": "a" * (sl._NAME_W + 1),
        "far past it": "a-repo-name-far-past-any-column-width-anybody-guessed",
        "CJK — two cells per glyph": "日本語のリポジトリ",
        "combining mark — zero cells": "svc-q" + chr(0x0301) + "ueue",
        "emoji — two cells, one character": "🚀-launcher",
    }

    def test_no_real_name_pushes_its_row_past_the_pane(self):
        """`tui.Cell` measures cells, so a name is padded and cut in the unit the terminal
        lays out in. Asserted at the narrow widths, where the row has the least room to
        absorb a mistake, and with the CI cell — the thing #506 is about — still whole."""
        for label, name in self.NAMES.items():
            for w in (95, 80, 74, 60):
                with self.subTest(name=label, width=w):
                    plan = sl._row_plan(w)
                    row = sl._tree_cells("  ├─ ", name, D, STATES, BRANCHES, GL,
                                         plan=plan).render(w)[0]
                    self.assertLessEqual(tui.width(row), w, row)
                    self.assertIn(CI_WHOLE, tui.strip_ansi(row), row)

    def test_an_awkward_name_starts_its_ci_cell_where_an_ordinary_one_does(self):
        """The alignment half. Every name in the matrix is drawn beside `svc`, and the two
        rows have to agree about where the CI column begins — which is the whole reason
        the name cell has a declared width rather than a natural one."""
        for label, name in self.NAMES.items():
            with self.subTest(name=label):
                plan = sl._row_plan(74)
                rows = [tui.strip_ansi(
                    sl._tree_cells("  ├─ ", n, D, STATES, BRANCHES, GL,
                                   plan=plan).render(74)[0])
                    for n in ("svc", name)]
                self.assertEqual(*[tui.width(r[:r.index("✗")]) for r in rows],
                                 "\n".join(rows))


class TheWorktreeSummaryLineCase(unittest.TestCase):
    """The other line `_repo_rows` emits, and it carried the same constant.

    A repo with worktrees nobody is drawing as full rows gets one summary line beneath it
    — `│ ╰─ piece · piece · piece`. It carried its own `_LEFT_W` while everything above it
    moved to the pane's real width.

    **And the sweep's answer was to delete the crop, not to re-point it.** `tui.Text`
    clamps the finished line to the same budget, so cropping the joined names first and
    the line afterwards produce the same string for every input — the pre-crop was
    redundant either way it was spelled, which is exactly the "equivalent mutant and dead
    code are the same finding" `tools/sweep.py` argues for. What these cases pin is the
    property that survived it: the line fits the pane, and it says when it left something
    out.
    """

    def _lines(self, budget: int, pieces: int = 6) -> list[str]:
        wt_dirs = [Path(f"/tmp/a-plane/.worktrees/w/charter/piece-number-{i}")
                   for i in range(pieces)]
        with mock.patch("charter.worktree.dirs_for", return_value=wt_dirs):
            rows = sl._repo_rows([D], "w", None, STATES, BRANCHES, GL, (), budget)
        return [tui.strip_ansi(ln) for r in rows for ln in r.render(budget)]

    def test_the_summary_line_fits_the_pane_it_was_composed_for(self):
        for w in (95, 80, 74, 60, 50, 40):
            with self.subTest(width=w):
                for line in self._lines(w):
                    self.assertLessEqual(tui.width(line), w, line)

    def test_it_says_when_it_left_a_piece_out(self):
        """A summary that silently drops pieces is a summary a reader trusts wrongly. The
        `…` is the only thing that says otherwise, and it has to survive whatever crop the
        line went through."""
        line = next(ln for ln in self._lines(60) if "piece-number-0" in ln)
        self.assertTrue(line.endswith(tui.ELLIPSIS),
                        f"the overflow mark is not on the line, so the reader is not "
                        f"told anything was dropped:\n{line!r}")

    def test_a_pane_wide_enough_shows_every_piece(self):
        """The control on the case above: with room for all six, none of them is dropped
        and there is no overflow mark to find. Without this, a line that dropped
        EVERYTHING would satisfy the ellipsis assertion perfectly."""
        lines = [ln for ln in self._lines(200) if "piece-number-0" in ln]
        self.assertEqual(len(lines), 1, lines)
        for i in range(6):
            self.assertIn(f"piece-number-{i}", lines[0], lines[0])
        self.assertNotIn(tui.ELLIPSIS, lines[0], lines[0])


class RenderCase(PersonaIso):
    """End to end through `statusline.render`, which is where #506 actually lived.

    `_row_plan` could be perfect and the defect remain: the rows were composed at
    `_LEFT_W` and cropped afterwards by `_columns`, so what has to be pinned is that
    `render` hands the composer the width it is really going to get. Everything below the
    layout is stubbed — git, the forge cache, the worktree scan — because none of it is
    what is under test and all of it is slow.
    """

    def setUp(self) -> None:
        super().setUp()
        from charter import glstate, workspace
        workspace.ensure("w")
        workspace.scaffold("w")
        self.enterContext(mock.patch.object(sl, "_repo_trees", return_value=[D]))
        self.enterContext(mock.patch.object(sl, "_repo_states", return_value=STATES))
        self.enterContext(mock.patch.object(sl, "_branch", return_value="main"))
        self.enterContext(mock.patch.object(sl, "_detail_worktrees", return_value=[]))
        self.enterContext(mock.patch.object(glstate, "read_for", return_value=GL))
        self.enterContext(mock.patch.object(glstate, "maybe_spawn", return_value=None))

    def render(self, columns: int) -> list[str]:
        with mock.patch.dict(os.environ, {"COLUMNS": str(columns)}):
            out = sl.render({"workspace": {"current_dir": str(D)}})
        return [re.sub(r"\x1b\[[0-9;]*m", "", ln) for ln in out.split("\n")]

    def repo_row(self, columns: int) -> str:
        # The tree glyph is `└─` on the last repo and `├─` otherwise, and the one repo
        # here is the last one. Matched on either so the probe is about the ROW
        # rather than about which elbow `_repo_rows` chose.
        rows = [ln for ln in self.render(columns)
                if "charter" in ln and ("├─" in ln or "└─" in ln)]
        self.assertEqual(len(rows), 1, "\n".join(self.render(columns)))
        return rows[0]

    def test_the_ci_mark_survives_an_eighty_column_terminal(self):
        """The reported case, through the real renderer. `$COLUMNS` is what Claude Code
        sets to the pane width, so 80 here is an ordinary split, not a stress test."""
        row = self.repo_row(80)
        self.assertIn(CI_WHOLE, row, row)

    def test_the_open_change_survives_an_eighty_column_terminal(self):
        self.assertIn(CHANGE_WHOLE, self.repo_row(80), self.repo_row(80))

    def test_no_narrow_pane_ever_shows_half_a_ci_label(self):
        """Every width from the table down to the floor: if the glyph is on the row, the
        word is too. This is the assertion the old renderer fails at nine widths."""
        for cols in range(40, 106, 2):
            with self.subTest(columns=cols):
                row = self.repo_row(cols)
                if "✗" in row:
                    self.assertIn(CI_WHOLE, row, row)

    def test_the_dirty_marker_is_not_what_pays_for_the_ci_mark(self):
        """The reading #506 is named for, asserted from the other end: at 80 columns the
        old row said "dirty, on main" about a repo whose pipeline was red. Both facts are
        on the row now, and neither displaced the other."""
        row = self.repo_row(80)
        self.assertIn("main*", row, row)
        self.assertIn(CI_WHOLE, row, row)

    def test_a_wide_terminal_is_unchanged(self):
        """The control. At 200 columns nothing about this touches the row at all, so a
        failure here says the change reached a width it had no business reaching."""
        row = self.repo_row(200)
        for want in ("charter", "main*", "↑2", CI_WHOLE, CHANGE_WHOLE):
            self.assertIn(want, row, row)

    def test_no_rendered_line_ever_exceeds_the_pane(self):
        """`tui`'s one hard guarantee, re-asked because this changed what is handed to
        it: a line that overruns wraps, and a wrapped line shears every column below."""
        for cols in (40, 60, 74, 80, 95, 131, 200):
            with self.subTest(columns=cols):
                for line in self.render(cols):
                    self.assertLessEqual(tui.width(line), cols, line)


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
