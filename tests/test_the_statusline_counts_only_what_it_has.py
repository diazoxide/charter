"""Every count and threshold in `statusline.py` that renames its own colour, pinned.

**These are the eighteen the deletion sweep charged this branch for**, and the charge is
right even though the defect is older than the branch: routing `ok`/`warn`/`bad` through
`statusline.accent` rewrote one token on each of these ten lines, and a rewritten line is a
line whose guards you now own. What the sweep actually mutated is the logic AROUND the
token — `if done:`, `if persistent:`, `cost >= _REBUILD_LOUD`, `src ==
"$CHARTER_WORKSPACE"` — and nothing in 9 678 tests could tell the difference when it was
deleted.

**The property they all share is `Presence IS the signal`.** `_session_news`' own docstring
says it: *"a counter that renders every turn becomes furniture within a day, and then a real
guard denial appearing in it gets no more attention than a zero would."* `_mem_badge`,
`_piece_summary` and `_alerts` each make the same promise in their own words. Every
`drop-if` mutation below breaks exactly that promise — it renders `✎0`, `0 done`, `⛊ 0
denied` on every line forever — and every one of them passed.

So each case here asserts the SILENCE and its own control: the row that must not appear
when the count is zero, and the row that must appear when it is not. A test for only the
second half is what let these through.
"""

from __future__ import annotations

import unittest
from unittest import mock

from charter import statusline as sl
from tests._isolation import (PersonaIso, no_background_refresh, shipped_frame)


def _plain(text) -> str:
    """*text* as a terminal with no colour would show it — a string or a list of pieces."""
    from charter import tui
    if not isinstance(text, str):
        text = " ".join(text)
    return tui.strip_ansi(text)


class AMemoryBadgeCountsNothingItDoesNotHave(unittest.TestCase):
    """`statusline._mem_badge` — `✎N` committed, `◌N` scratch, and **nothing at zero**.

    Its own docstring says "'' when both are zero", and nothing asserted the halves: with
    `if persistent:` deleted every persona in the sidebar grows a permanent `✎0`, on the one
    column whose whole design is that an empty cell means "nothing to report".
    """

    def setUp(self):
        super().setUp()
        shipped_frame(self)

    def test_neither_count_draws_nothing_at_all(self):
        self.assertEqual(sl._mem_badge(0, 0), "")

    def test_a_zero_persistent_count_draws_no_committed_badge(self):
        """`drop-if` on `if persistent:` — the badge that would render on every row."""
        self.assertNotIn("✎", sl._mem_badge(0, 3))
        self.assertIn("◌3", _plain(sl._mem_badge(0, 3)))

    def test_a_zero_ephemeral_count_draws_no_scratch_badge(self):
        """`drop-if` on `if ephemeral:`, and the mirror of the case above."""
        self.assertNotIn("◌", sl._mem_badge(4, 0))
        self.assertIn("✎4", _plain(sl._mem_badge(4, 0)))

    def test_both_counts_draw_both_badges_in_their_own_accents(self):
        """The control. Every assertion above is an absence, so this is what proves the
        function draws anything at all — and it pins which accent each half wears, which is
        the token this branch rewrote."""
        got = sl._mem_badge(4, 3)
        self.assertEqual(_plain(got), " ✎4 ◌3")
        self.assertIn(f"{sl.accent('ok')}✎4", got)
        self.assertIn(f"{sl.accent('warn')}◌3", got)


class ABranchCellIsColouredByWhetherItIsDirty(unittest.TestCase):
    """`statusline._branch_cell_for` — `warn` when the clone is dirty, `dim` when it is not.

    Both `collapse-ifexp` arms survived, so nothing said the cell changes at all: a repo
    table where every branch was drawn dirty, or none was, passed the whole suite. This is
    the same cell #736 measured inside a reversed row, which is why it is worth two lines.
    """

    def setUp(self):
        super().setUp()
        shipped_frame(self)

    def test_a_dirty_clone_draws_its_branch_in_warn(self):
        got = sl._branch_cell_for("main", "", is_dirty=True)
        self.assertIn(sl.accent("warn"), got)
        self.assertNotIn(sl._DIM, got)

    def test_a_clean_clone_draws_its_branch_dim(self):
        got = sl._branch_cell_for("main", "", is_dirty=False)
        self.assertIn(sl._DIM, got)
        self.assertNotIn(sl.accent("warn"), got)

    def test_the_two_really_are_different(self):
        """The control, and it is not decoration: both assertions above are satisfied by a
        function that returns one constant, if that constant happened to contain both."""
        self.assertNotEqual(sl._branch_cell_for("main", "", is_dirty=True),
                            sl._branch_cell_for("main", "", is_dirty=False))

    def test_the_branch_name_survives_either_way(self):
        for dirty in (True, False):
            with self.subTest(dirty=dirty):
                self.assertIn("main", _plain(sl._branch_cell_for("main", "",
                                                                 is_dirty=dirty)))


class APieceSummaryCountsOnlyTheOutcomesItHas(unittest.TestCase):
    """`statusline._piece_summary` — `N done` and `N abandoned` appear only when non-zero.

    Both `drop-if`s survived. With them deleted, a workspace whose pieces are all still
    running renders `pieces 3 0 done 0 abandoned` on every turn — three numbers where the
    row's whole design is that a number present means something happened.
    """

    def setUp(self):
        super().setUp()
        shipped_frame(self)

    def _summary(self, decls):
        """*decls* is one declaration per worktree, `None` for "still running"."""
        from charter import pieces as _p, worktree as _wt
        names = [f"wt{i}" for i in range(len(decls))]
        # `.name` is assigned AFTER construction on every mock here, and that is not a
        # style choice: `MagicMock(name="repo")` names the MOCK and leaves `.name` a child
        # mock, so a fixture built the obvious way silently hands `_piece_summary` a
        # directory whose name is a `MagicMock` — it counted one worktree instead of three
        # and every assertion below passed for the wrong reason.
        repo = mock.MagicMock()
        repo.name = "repo"
        repo.is_dir.return_value = True
        root = mock.MagicMock()
        root.iterdir.return_value = [repo]
        dirs = []
        for n in names:
            d = mock.MagicMock()
            d.name = n
            dirs.append(d)
        by_name = dict(zip(names, decls))
        with mock.patch.object(_wt, "root", lambda ws: root), \
                mock.patch.object(_wt, "dirs_for", lambda ws, repo: dirs), \
                mock.patch.object(_p, "declaration_for",
                                  lambda ws, repo, wt: by_name[wt]):
            return sl._piece_summary("w")

    def test_pieces_that_are_all_still_running_report_no_outcome_at_all(self):
        """`drop-if` on both. Three pieces, none finished: the row says how many there are
        and nothing else."""
        got = _plain(self._summary([None, None, None]))
        self.assertEqual(got, "pieces 3",
                         "a piece with no outcome yet was counted as one that had")

    def test_a_finished_piece_is_counted_and_an_abandoned_one_is_not(self):
        got = _plain(self._summary([{"event": "done"}, None]))
        self.assertIn("1 done", got)
        self.assertNotIn("abandoned", got)

    def test_an_abandoned_piece_is_counted_and_a_finished_one_is_not(self):
        got = _plain(self._summary([{"event": "gave-up"}, None]))
        self.assertIn("1 abandoned", got)
        # `"1 done"` and not `"done"`: **`abandoned` ends in the same five letters**, so the
        # obvious spelling of this assertion is false about a row that is entirely correct.
        # It failed here, which is the only reason it is not still in the file.
        self.assertNotIn("1 done", got)

    def test_each_outcome_wears_its_own_accent(self):
        """The control for both, and the token this branch rewrote: `done` is `ok` and
        `abandoned` is `warn`."""
        got = self._summary([{"event": "done"}, {"event": "gave-up"}])
        self.assertIn(f"{sl.accent('ok')}1 done", got)
        self.assertIn(f"{sl.accent('warn')}1 abandoned", got)


class SessionNewsCountsOnlyWhatHappened(unittest.TestCase):
    """`statusline._session_news` — *"Deliberately silent when nothing is happening.
    Presence IS the signal."*

    Both `drop-if`s survived, so nothing held that sentence to its word. With them gone,
    every session renders `⛊ 0 denied ✎ 0 recorded` from its first turn — and a real guard
    denial then arrives in a counter the reader has been ignoring for a day.
    """

    def setUp(self):
        super().setUp()
        shipped_frame(self)

    def _news(self, events):
        from charter import trace
        with mock.patch.object(trace, "read", lambda sid: [{"event": e} for e in events]):
            return sl._session_news("s1", inflight=False)

    def test_a_session_with_no_denials_says_nothing_about_denials(self):
        got = _plain(self._news(["memory"]))
        self.assertNotIn("denied", got)
        self.assertIn("✎ 1", got)

    def test_a_session_with_no_memories_says_nothing_about_memories(self):
        got = _plain(self._news(["deny"]))
        self.assertNotIn("recorded", got)
        self.assertIn("⛊ 1", got)

    def test_a_session_where_nothing_happened_says_nothing(self):
        self.assertEqual(self._news([]), [])

    def test_a_counter_with_nothing_to_say_does_not_silence_the_ones_after_it(self):
        """**The refusal is what keeps the row from ending early**, and that is the half a
        `drop-if` here breaks in a way no absence can see.

        `_session_news` has ONE `except Exception: pass` around the whole trace block, so a
        counter that stops asking `kinds.get(...)` and reaches for `kinds[...]` does not
        render a zero — it raises, the handler swallows it, and every counter BELOW it is
        silently dropped. `dispatch` is drawn after `memory`, so a session that dispatched
        without recording anything is exactly the shape that loses a row.

        Asserting only "no `recorded` row" cannot catch it: the mutant does not draw one
        either. What it costs is the row that never got its turn."""
        got = _plain(self._news(["dispatch", "dispatch"]))
        self.assertNotIn("recorded", got)
        self.assertIn("⇢ 2 dispatched", got,
                      "a counter above it swallowed the rest of the row")

    def test_each_counter_wears_its_own_accent(self):
        """The control, and the tokens this branch rewrote: a denial is `bad`, a recorded
        memory is `ok`, and a dispatch is neither — it is `dim`, and must stay `dim` when a
        plane recolours the other two."""
        got = " ".join(self._news(["deny", "memory", "dispatch"]))
        self.assertIn(f"{sl.accent('bad')}⛊ 1 denied", got)
        self.assertIn(f"{sl.accent('ok')}✎ 1", got)
        self.assertIn(f"{sl._DIM}⇢ 1 dispatched", got)


class AReinitAlertFiresOnlyWhenAWorkspaceNeedsOne(PersonaIso, unittest.TestCase):
    """`statusline._alerts` — *"They render only when real, so they cost no rows on a
    healthy control plane."*

    The `drop-if` on `if stale:` survived: with it deleted, every healthy plane grows a
    full-width `⚠ reinit 0 ws` alert on every turn — a row that carries a command to fix a
    problem that does not exist.
    """

    def setUp(self):
        super().setUp()
        shipped_frame(self)

    def _alerts(self, stale):
        # `PersonaIso` rather than a plain case: `_alerts` ends at `_plane_root_alert`,
        # which asks `_repo_states` for the plane root and that opens a cache directory
        # under `.charter`. `tests._planeguard` refuses the write into the developer's real
        # plane, which is the guard doing its job and the reason this class has a plane of
        # its own.
        from charter import workspace as _ws
        with mock.patch.object(_ws, "list_workspaces", lambda: ["a", "b"]), \
                mock.patch.object(_ws, "needs_reinit", lambda w: w in stale):
            return _plain(sl._alerts("active"))

    def test_a_plane_whose_workspaces_are_current_gets_no_reinit_row(self):
        self.assertNotIn("reinit", self._alerts([]))

    def test_a_plane_with_a_stale_workspace_gets_one_and_it_counts_them(self):
        """The control. `active` is excluded by the comprehension, so `b` alone is stale."""
        got = self._alerts(["b"])
        self.assertIn("reinit", got)
        self.assertIn("1 ws", got)

    def test_the_row_carries_the_command_that_fixes_it(self):
        self.assertIn("charter ws reinit --all", self._alerts(["a", "b"]))


class ThePinnedWorkspaceMarkerSaysWhereThePinCameFrom(PersonaIso, unittest.TestCase):
    """`statusline.render` — the `*` beside the workspace name means `$CHARTER_WORKSPACE`
    pinned it, and nothing else does.

    All three mutations survived: both arms of the conditional and the environment
    variable's own SPELLING. With the literal retuned the marker never renders, and an
    operator whose shell pins a workspace has no way to see that it is pinned — which is
    the whole of what the glyph is for.
    """

    def setUp(self):
        super().setUp()
        no_background_refresh(self)
        shipped_frame(self)

    def _row(self, src):
        with mock.patch.object(sl, "_active", lambda *a, **k: ("alpha", src)):
            return sl.render({})

    def test_a_workspace_pinned_by_the_environment_is_marked(self):
        self.assertIn("alpha*", _plain(self._row("$CHARTER_WORKSPACE")))

    def test_a_workspace_chosen_any_other_way_is_not(self):
        for src in ("cwd", "pointer", "default", ""):
            with self.subTest(src=src):
                got = _plain(self._row(src))
                self.assertIn("alpha", got)
                self.assertNotIn("alpha*", got)

    def test_the_marker_is_drawn_in_warn(self):
        """The token this branch rewrote. It is `warn` rather than `dim` because a pin is
        something to notice — it overrides every other way a workspace is chosen."""
        self.assertIn(f"{sl.accent('warn')}*", self._row("$CHARTER_WORKSPACE"))


class ALoudRebuildIsTheOneThatCrossedTheLine(unittest.TestCase):
    """The `↻N` rebuild counter's threshold — `bad` at or above `_REBUILD_LOUD`, `warn`
    below it, on **both** surfaces that draw it.

    Six mutations survived here: the two arms and the boundary, at each of the two call
    sites. Nothing said the counter changes colour at all, and nothing said the two
    surfaces agree about where — which is `ThePanelsGaugeReadsTheSameAsTheFooters`' own
    correctness condition said about the third number on that strip.

    **At the boundary and not in the middle of a band**, which is that class's rule: a test
    at 100 000 and 300 000 passes with the comparison shifted by one either way.
    """

    def setUp(self):
        super().setUp()
        shipped_frame(self)

    def _live(self, cost):
        """`_context_gauge` — the footer's, off a live payload."""
        payload = {"context_window": {"used_percentage": 50,
                                      "current_usage": {"cache_read_input_tokens": 100,
                                                        "cache_creation_input_tokens": 50}},
                   "session_id": "s1"}
        with mock.patch.object(sl, "_rebuilds", lambda rows: (2, cost)), \
                mock.patch.object(sl, "record_usage", lambda p: []), \
                mock.patch.object(sl, "_history", lambda sid: []):
            return " ".join(sl._context_gauge(payload))

    def _panel(self, cost):
        """`recorded_context_gauge` — the frame panel's, off the recorded history."""
        with mock.patch.object(sl, "_rebuilds", lambda rows: (2, cost)), \
                mock.patch.object(sl, "_usage_rows", lambda sid: []):
            return " ".join(sl.recorded_context_gauge("s1"))

    def test_a_rebuild_cost_at_the_line_is_loud_on_both_surfaces(self):
        """`>=`, so exactly `_REBUILD_LOUD` is already loud. A `>` here is the shift the
        sweep asked about and nothing answered."""
        for name, got in (("footer", self._live(sl._REBUILD_LOUD)),
                          ("panel", self._panel(sl._REBUILD_LOUD))):
            with self.subTest(surface=name):
                self.assertIn(f"{sl.accent('bad')}↻2", got)

    def test_one_token_below_the_line_is_not(self):
        """The other side of the same boundary, and the half a `>` mutation moves."""
        for name, got in (("footer", self._live(sl._REBUILD_LOUD - 1)),
                          ("panel", self._panel(sl._REBUILD_LOUD - 1))):
            with self.subTest(surface=name):
                self.assertIn(f"{sl.accent('warn')}↻2", got)
                self.assertNotIn(sl.accent("bad"), got)

    def test_the_two_surfaces_agree_about_where_the_line_is(self):
        """#413's own correctness condition, asked of the third number: a loud rebuild in
        the footer beside a quiet one in the panel would be undebuggable from the screen."""
        for cost in (0, sl._REBUILD_LOUD - 1, sl._REBUILD_LOUD, sl._REBUILD_LOUD * 2):
            with self.subTest(cost=cost):
                loud = sl.accent("bad") in self._live(cost)
                self.assertEqual(loud, sl.accent("bad") in self._panel(cost))

    def test_a_session_with_no_rebuilds_draws_no_counter(self):
        """The control that the counter is conditional at all, and the reason the two
        assertions above are about colour rather than presence."""
        with mock.patch.object(sl, "_rebuilds", lambda rows: (0, 0)), \
                mock.patch.object(sl, "_usage_rows", lambda sid: []):
            self.assertNotIn("↻", " ".join(sl.recorded_context_gauge("s1")))


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
