"""The frame's whole shape, decided without tmux.

Layout is pure so the feature is testable on a machine that has never installed tmux, and
so the argv rule is enforced by a unit test rather than by review: every element of every
command is a separate string, because tmux shell-interprets a joined one (pinned against
3.7c) and workspace, repo and branch names all reach here from committed files.

`session_argv` and `panel_argvs` are two functions rather than one `plan()` because the
launcher must run the first, read the harness's pane id off its stdout, and only then
build the splits — see `charter/frame/layout.py`'s module docstring for the measured tmux
3.7c failure (pane-index renumbering) a single up-front plan cannot avoid.
"""

from __future__ import annotations

import sys
import unittest
from unittest import mock

from charter import util
from charter.frame import layout


SESSION = dict(session="charter-demo-1234", conf="/tmp/f/tmux.conf",
               socket="charter", cols=200, rows=50,
               harness_argv=["claude", "--resume", "a;b"])

PANELS = dict(session="charter-demo-1234", socket="charter", harness_pane="%0")


def _direction(cmd: list[str]) -> str:
    """`-v` (splits along rows) or `-h` (splits along columns) — whichever is present."""
    return "-v" if "-v" in cmd else "-h"


def _size(cmd: list[str]) -> str:
    """The value passed to `-l`, as the literal string tmux would see on argv."""
    return cmd[cmd.index("-l") + 1]


class VisibleSlots(unittest.TestCase):
    def test_a_wide_tall_terminal_keeps_every_slot(self):
        self.assertEqual(
            layout.visible_slots(["top", "bottom", "right"], 200, 50, 100, 20),
            ["top", "bottom", "right"])

    def test_the_side_panel_goes_first_when_the_terminal_is_narrow(self):
        self.assertEqual(
            layout.visible_slots(["top", "bottom", "right"], 80, 50, 100, 20),
            ["top", "bottom"])

    def test_the_top_goes_next_when_the_terminal_is_short(self):
        self.assertEqual(
            layout.visible_slots(["top", "bottom", "right"], 200, 12, 100, 20),
            ["bottom"])

    def test_the_bottom_is_never_dropped_whatever_the_shortage(self):
        """The reason `bottom` is absent from every filter above. It is the attention
        strip — one alert and the command that fixes it — which is the whole reason a
        cramped terminal wants a frame at all, and since #515 it is one fixed row, so
        there is nothing for a shortage to negotiate over. Asserted across all three
        shortages the filters answer — narrow, short, and neither — so a `bottom` added
        to any filter is red whichever one it was added to. The last clause (below HALF
        the floors) is a different rule and still empties the list outright;
        `test_a_tiny_terminal_keeps_nothing` owns that one."""
        for cols, rows in ((80, 50), (200, 12), (100, 20)):
            with self.subTest(cols=cols, rows=rows):
                self.assertIn("bottom", layout.visible_slots(
                    ["top", "bottom", "repos", "right"], cols, rows, 100, 20))

    def test_the_table_goes_when_its_pane_would_be_too_narrow_to_draw_one(self):
        """#515. `slots._table_cap` answers 0 below `statusline._LEFT_W` and
        `_table_lines` refuses outright there, so a `repos` pane split at that width is a
        bordered rectangle with nothing in it — which reads as "this workspace has no
        repos" on a plane that has fourteen. While the table shared `bottom` with the
        attention row the same case was invisible: the pane went on drawing the row.

        Asserted at the boundary from both sides, and through the PANE's width rather
        than the window's: a `[frame] slots` naming `right` first insets this pane by
        `SLOT_SIZE["right"] + _BORDER_COLS`, so a window wide enough for the table can
        still leave the pane too narrow for it. That second case is the one a filter
        reading `cols` directly would get wrong."""
        from charter import statusline
        edge = statusline._LEFT_W
        self.assertIn("repos", layout.visible_slots(
            ["top", "bottom", "repos"], edge, 50, 100, 20))
        self.assertNotIn("repos", layout.visible_slots(
            ["top", "bottom", "repos"], edge - 1, 50, 100, 20))
        inset = layout.SLOT_SIZE["right"] + layout._BORDER_COLS
        self.assertIn("repos", layout.visible_slots(
            ["right", "top", "bottom", "repos"], edge + inset, 50, 100, 20))
        self.assertNotIn("repos", layout.visible_slots(
            ["right", "top", "bottom", "repos"], edge + inset - 1, 50, 100, 20))

    def test_the_width_the_table_needs_is_read_from_the_renderer_not_copied(self):
        """The property, not a spelling. `layout._table_min_cols` reads
        `statusline._LEFT_W` — the same constant `slots._table_cap` refuses below — so
        the launcher's drop and the renderer's silence can never come apart. A literal
        here would be a guard that matched today's number and stopped matching the
        renderer's the first time a column width moved.

        **`_LEFT_W` is MOVED for the assertion, and that is the whole test.** Asserted
        against the constant where it stands, this passes just as happily on a
        `_table_min_cols` whose body is `return 95` — 95 is today's `_LEFT_W`, so the
        copy and the read are indistinguishable and the mutation survives. Moving the
        constant separates them: the branch answers 102, a literal answers 95."""
        from charter import statusline
        with mock.patch.object(statusline, "_LEFT_W", statusline._LEFT_W + 7):
            self.assertEqual(layout._table_min_cols(), statusline._LEFT_W)

    def test_a_tiny_terminal_keeps_nothing(self):
        """Below the floor the harness gets the whole terminal. Degrading to a bare
        harness is the same move `statusline.render` makes when it runs out of width."""
        self.assertEqual(layout.visible_slots(["top", "bottom"], 40, 8, 100, 20), [])


class SessionArgv(unittest.TestCase):
    def test_the_harness_argv_survives_as_separate_elements_after_the_separator(self):
        """The security property: tmux does not shell-interpret separate argv elements
        but does interpret a joined string (pinned against 3.7c), and harness arguments
        come from the operator's own command line. Checking the exact tail — not just
        membership of `"--resume"` and `"a;b"` — also catches a `--` dropped or moved,
        and the harness argv reordered or truncated, none of which the brief's weaker
        membership check would have caught."""
        cmd = layout.session_argv(**SESSION)
        joined = [part for part in cmd if "claude --resume" in part]
        self.assertEqual(joined, [], "harness argv was joined into one string")
        self.assertEqual(cmd[cmd.index("--") + 1:], SESSION["harness_argv"])

    def test_the_command_is_a_list_of_separate_strings(self):
        cmd = layout.session_argv(**SESSION)
        self.assertIsInstance(cmd, list)
        for part in cmd:
            self.assertIsInstance(part, str)

    def test_the_socket_is_named(self):
        """One private server, never the operator's. Every command carries `-L`."""
        self.assertEqual(layout.session_argv(**SESSION)[:3], ["tmux", "-L", "charter"])

    def test_it_asks_tmux_to_print_the_pane_id(self):
        """Pins that `session_argv` actually requests the pane id, not merely `-P` in
        isolation: the whole two-function split depends on the launcher being able to
        read a real pane id off stdout, and `-P` with the wrong (or missing) `-F` prints
        something else just as silently as omitting the flag. Catches `-P` present but
        `-F`/`'#{pane_id}'` dropped or misspelled."""
        cmd = layout.session_argv(**SESSION)
        self.assertIn("-P", cmd)
        i = cmd.index("-P")
        self.assertEqual(cmd[i + 1:i + 3], ["-F", "#{pane_id}"])

    def test_the_session_is_created_detached(self):
        """`-d`: launched from a script with no tty to hand tmux. Without it tmux
        attaches and the call never returns, which a test would see as a hang rather
        than a clean failure — so this is worth pinning explicitly."""
        self.assertIn("-d", layout.session_argv(**SESSION))


class PanelArgvs(unittest.TestCase):
    def test_every_command_is_a_list_of_separate_strings(self):
        for cmd in layout.panel_argvs(slots=["top", "bottom"], **PANELS):
            self.assertIsInstance(cmd, list)
            for part in cmd:
                self.assertIsInstance(part, str)

    def test_it_asks_tmux_to_print_each_panels_pane_id(self):
        """Mirrors `SessionArgv.test_it_asks_tmux_to_print_the_pane_id`: a caller needs
        each panel's own pane id to re-assert its fixed size after tmux's own layout
        engine redistributes every pane proportionally on a resize (measured against
        tmux 3.7c — see this function's own docstring). `-P`/`-F` must land BEFORE the
        `--` separator (tmux's own option, never part of the `charter panel …` argv
        after it) — the same placement `session_argv` already uses."""
        for cmd in layout.panel_argvs(slots=["top", "bottom"], **PANELS):
            self.assertIn("-P", cmd)
            i = cmd.index("-P")
            self.assertEqual(cmd[i + 1:i + 3], ["-F", "#{pane_id}"])
            self.assertLess(i, cmd.index("--"),
                            "-P/-F must be split-window's own options, before --")

    def test_each_visible_slot_gets_one_panel_command(self):
        cmds = layout.panel_argvs(slots=["top", "bottom"], **PANELS)
        panels = [c for c in cmds if "panel" in c]
        self.assertEqual(len(panels), 2)
        slots = {c[c.index("panel") + 1] for c in panels}
        self.assertEqual(slots, {"top", "bottom"})

    def test_the_socket_is_named_on_every_command(self):
        for cmd in layout.panel_argvs(slots=["top"], **PANELS):
            self.assertEqual(cmd[:3], ["tmux", "-L", "charter"])

    def test_every_split_targets_the_harness_pane_id_not_a_session_index(self):
        """The bug this two-function design exists to prevent (measured against tmux
        3.7c — see the module docstring): tmux renumbers pane INDICES on every split, so
        a target like `f"{session}:0.0"` stops naming the harness after the first split
        ever runs, and the next split divides the previous split's own panel instead —
        which eventually fails outright once that panel is one row tall. Fails if any
        split falls back to a `session:0.0`-style target instead of the pane id it was
        given."""
        cmds = layout.panel_argvs(slots=["top", "bottom", "right"], **PANELS)
        for cmd in cmds:
            self.assertEqual(cmd[cmd.index("-t") + 1], "%0")
            self.assertNotIn(f"{PANELS['session']}:0.0", cmd)

    def test_every_slot_targets_the_same_pane_even_after_earlier_splits(self):
        """Companion to the test above, guarding against a plausible half-fix: only the
        FIRST split corrected to use the pane id, with later ones drifting back to some
        target derived from loop position (e.g. incrementing an index, or chaining off
        the pane an earlier split in this same list would have created). Every split must
        name the one id `panel_argvs` was handed, regardless of its position in *slots*."""
        cmds = layout.panel_argvs(slots=["top", "bottom", "right"], **PANELS)
        targets = {cmd[cmd.index("-t") + 1] for cmd in cmds}
        self.assertEqual(targets, {"%0"})


class PanelCommand(unittest.TestCase):
    """One definition of what a panel pane runs, because two modules start one.

    The launcher splits a pane for it; `commands_frame.cmd_respawn` brings the same
    panel back with `respawn-pane` after its `pane-died` hook fires (#382). Two
    hand-written copies of this argv drift, and the drift only ever shows up after
    something has already died once — which is the worst possible moment to discover
    the respawned panel is running a slightly different command.
    """

    def test_the_split_runs_exactly_what_a_respawn_would_run(self):
        split = layout.panel_argvs(slots=["bottom"], session="f-1", socket="charter",
                                   harness_pane="%0")[0]
        self.assertEqual(split[split.index("--") + 1:],
                         layout.panel_command(slot="bottom", session="f-1"))

    def test_it_carries_the_slot_and_the_session_the_cli_requires(self):
        """`cli.build_parser`'s `panel` parser takes `<slot> --session <fid>` and makes
        `--session` required — a command missing either is a pane that fails at startup,
        which is the hole #382's first half exists to make legible rather than to
        create a second source of."""
        self.assertEqual(
            layout.panel_command(slot="top", session="f-9"),
            [*util.self_relaunch_argv(), "panel", "top", "--session", "f-9"])

    def test_the_interpreter_half_is_the_shared_helpers_and_carries_dash_p(self):
        """#390, and the reason this function stopped taking a *charter_argv* at all.

        A `charter_argv` PARAMETER is what let the launcher and `cmd_respawn` disagree:
        the launcher moved to `util.self_relaunch_argv()` and the respawn kept a
        hand-built `[sys.executable, "-m", "charter"]`, so a respawned panel would have
        imported whatever `charter/` package sat in the pane's own cwd — a charter
        checkout, for anyone dogfooding. Asserted against the LITERAL prefix as well as
        against the helper, so a helper that itself lost `-P` cannot make this test
        agree with a broken production path (the helper's own shape is pinned in
        `tests/test_self_relaunch_argv.py`, but a test that only compares two things
        that move together proves neither)."""
        cmd = layout.panel_command(slot="bottom", session="f-1")
        self.assertEqual(cmd[:4], [sys.executable, "-P", "-m", "charter"])


class PanelGeometry(unittest.TestCase):
    """Pins the one property nothing else in this file checks: each slot's actual shape.

    `visible_slots` decides WHICH slots appear and `panel_argvs`' targeting tests pin
    WHERE the split lands, but nothing until now pinned the split itself — direction,
    `-b`, and `-l <size>` are three independent pieces of code (a membership check, a
    second membership check, and a dict lookup) that happen to agree with the intended
    frame only because each was written correctly, not because anything forces them to
    agree. Swap two `SLOT_SIZE` values, or transpose the two membership checks, and every
    other test in this file stays green while the frame ships sideways.
    """

    def test_horizontal_edges_split_vertically_and_top_goes_before_the_harness(self):
        """`top` and `bottom` are full-width, one-row strips, so both are cut with a
        VERTICAL split (`-v` divides the terminal along its rows — the axis that makes a
        one-row strip; `-h`, used by `right` below, divides it into side-by-side
        columns instead, which is the wrong shape for either of these). `top` is placed
        BEFORE the harness pane (`-b`); `bottom`, asserted right alongside it for
        contrast, goes after (no `-b`) — that contrast is what makes this one test with
        `bottom` in it rather than an isolated claim about `top`. Both are exactly
        `SLOT_SIZE["top"] == SLOT_SIZE["bottom"] == 1` row, asserted as the literal
        `"1"` rather than read back through `layout.SLOT_SIZE`: reading it back would
        still pass after `SLOT_SIZE` is swapped to `{"top": 22, ...}`, since the emitted
        `-l` always equals whatever the dict currently says — the literal is what makes
        the swap visible.

        Catches (verified by hand, see fix-round section of the task report): mutation 1
        (direction inverted and `-b` membership swapped) — `top`'s direction flips to
        `-h` and its `-b` disappears, both asserted here. Catches mutation 2 (`SLOT_SIZE`
        rows/cols swapped to 22/1) via the literal `"1"`.
        """
        top, bottom = layout.panel_argvs(slots=["top", "bottom"], **PANELS)
        self.assertEqual(_direction(top), "-v")
        self.assertEqual(_direction(bottom), "-v")
        self.assertIn("-b", top)
        self.assertNotIn("-b", bottom)
        self.assertEqual(_size(top), "1")
        self.assertEqual(_size(bottom), "1")

    def test_the_side_edge_splits_horizontally_and_goes_after_the_harness(self):
        """Mirror of the test above, for the other axis. `right` is the side column, so
        it is cut with a HORIZONTAL split (`-h` — the axis that makes a column; `-v`,
        used by `top`/`bottom` above, would instead slice off a row), and it goes AFTER
        the harness (no `-b`), unlike `top`. `SLOT_SIZE["right"] == 22` columns, again
        the literal `"22"` rather than read back through `layout.SLOT_SIZE`, for the
        reason given above.

        `left` used to be asserted here beside it, as the `-b` half of the contrast;
        #488 retired the slot, and the `-b`/no-`-b` contrast is now carried by
        `top`/`bottom` in the test above, which still has both halves.

        Catches: a direction inverted on this axis and not the other (the test above
        rules out the converse), a stray `-b`, and `SLOT_SIZE`'s rows/cols swapped.
        """
        right, = layout.panel_argvs(slots=["right"], **PANELS)
        self.assertEqual(_direction(right), "-h")
        self.assertNotIn("-b", right)
        self.assertEqual(_size(right), "22")

    def test_left_is_not_a_slot_this_module_will_size_or_split(self):
        """#488 retired the sidebar, and this pins BOTH halves of that rather than only
        the registry: `SLOT_SIZE` no longer carries it, and `slot_sizes` — the one thing
        callers ask for a size now — drops it rather than inventing one. A `left` left
        in either place would be a pane charter splits and nothing draws in, which is
        exactly the permanently-dead pane `_drawable_slots`' unimplemented filter exists
        to prevent."""
        self.assertNotIn("left", layout.SLOT_SIZE)
        self.assertNotIn("left", layout._DROP_ORDER)
        self.assertEqual(
            layout.slot_sizes(["top", "left", "bottom", "repos"], window_rows=50,
                              content_rows=3),
            {"top": 1, "bottom": 1, "repos": 3})


class ReposIsSizedToItsContent(unittest.TestCase):
    """#488, moved to `repos` by #515: the one variable-height slot, and `repos_rows` is
    the whole of how tall it gets. Pure arithmetic, so it is pinned here with no tmux and
    no cache.

    The property is `floor <= rows <= what the window can spare`, and each of the three
    clauses is asserted where the OTHER two cannot be what produced the answer — a single
    "it returns a plausible number" test would pass with any two of them deleted.
    """

    def test_a_small_plane_gets_exactly_the_rows_its_content_wants(self):
        """Neither bound is binding here: 4 rows of content in a 50-row window is well
        above the floor and well under the cap, so the answer can only have come from the
        content itself. Deleting the `min` or the `max` leaves this green — which is why
        the two tests below exist as well."""
        self.assertEqual(
            layout.repos_rows(content_rows=4, window_rows=50,
                              slots=["top", "bottom", "repos"]),
            4)

    def test_it_never_goes_below_one_row(self):
        """A plane with no clones has no table, and `slots.repos_rows_wanted` answers 0.
        Zero would be a pane tmux refuses to split at all, so the floor is one — and the
        row is spent saying there are no clones and how to get one (`slots._empty_lines`)
        rather than left blank."""
        for content in (0, 1):
            with self.subTest(content=content):
                self.assertEqual(
                    layout.repos_rows(content_rows=content, window_rows=50,
                                      slots=["top", "bottom", "repos"]),
                    layout.SLOT_SIZE["repos"])

    def test_the_harness_keeps_its_floor_however_much_the_table_wants(self):
        """The measured failure this cap exists for (tmux 3.7c): `resize-pane -y 40` in a
        20-row window is not refused — tmux grants it out of the neighbour and leaves the
        harness pane 1 row tall. Asserted as the HARNESS's remaining rows rather than as
        a literal height, so the arithmetic is checked rather than restated: whatever
        `bottom` takes, plus the strips and their borders, must leave at least
        `HARNESS_MIN_ROWS`."""
        slots = ["top", "bottom", "repos"]
        rows = 20
        got = layout.repos_rows(content_rows=99, window_rows=rows, slots=slots)
        # top(1) + bottom(1) + a border for each of the three strips
        harness = (rows - got - layout.SLOT_SIZE["top"] - layout.SLOT_SIZE["bottom"]
                   - 3 * layout._BORDER_ROWS)
        self.assertGreaterEqual(harness, layout.HARNESS_MIN_ROWS)
        self.assertLess(got, 99, "the cap did not bind at all")

    def test_every_fixed_strips_own_rows_are_counted_against_the_cap(self):
        """The cap is what the window can spare, and the fixed strips are part of what it
        has already spent. A frame drawing `top` must therefore give the table strictly
        fewer rows than one that is not, at the same window size — and so must one
        drawing `bottom`, which is the row #515 added to the arithmetic. Both are
        asserted: a `repos_rows` still subtracting `top` alone (the pre-#515 formula)
        passes the first pair and fails the second."""
        both = layout.repos_rows(content_rows=99, window_rows=26,
                                 slots=["top", "bottom", "repos"])
        no_top = layout.repos_rows(content_rows=99, window_rows=26,
                                   slots=["bottom", "repos"])
        no_bottom = layout.repos_rows(content_rows=99, window_rows=26,
                                      slots=["top", "repos"])
        self.assertLess(both, no_top)
        self.assertLess(both, no_bottom)

    def test_the_side_column_costs_columns_not_rows(self):
        """`right` is a `-h` split: it takes width from the harness and no rows at all.
        Counting it here would shorten the table for no reason on every wide terminal."""
        self.assertEqual(
            layout.repos_rows(content_rows=99, window_rows=50,
                              slots=["top", "bottom", "repos", "right"]),
            layout.repos_rows(content_rows=99, window_rows=50,
                              slots=["top", "bottom", "repos"]))

    def test_the_floor_wins_over_a_cap_that_has_gone_negative(self):
        """A window with no rows to spare at all. The alternative — returning the cap —
        is a zero or negative `-l`, which is a split tmux refuses outright, so the frame
        would come up with no bottom pane in exactly the terminal most likely to have an
        alert worth reading."""
        self.assertEqual(
            layout.repos_rows(content_rows=9, window_rows=6,
                              slots=["top", "bottom", "repos"]),
            layout.SLOT_SIZE["repos"])


class TheTablesWidthIsWhateverTheSplitOrderLeftIt(unittest.TestCase):
    """#500, round 3: `layout.repos_cols` — how wide the `repos` PANE actually is.

    The property, stated once: **a slot that takes columns and is split before `repos`
    has already narrowed the pane `repos` is carved out of.** Not "the window's width",
    which is what every caller was passing, and not "the width when `slots` happens to be
    the shipped list", which is the coincidence that let it through two rounds of review.

    Every number below was measured against real tmux 3.7c on a private socket, window
    110x40, splitting exactly as `panel_argvs` does (`-h -l 22` for `right`, `-v -b -l 1`
    for `top`, `-v -l 7` for the table, every split targeting the harness pane):

        ["top", "repos", "right"]  ->  table 110x7   harness 87x30
        ["right", "top", "repos"]  ->  table  87x7   harness 87x30
        ["top", "right", "repos"]  ->  table  87x7   harness 87x30
        ["repos", "right"]         ->  table 110x7   harness 87x32

    and, for the re-layout case, killing `right` on the second of those widened the same
    table pane from 87 back to 110, while re-splitting `right` off the harness afterwards
    left it at 110.
    """

    def test_the_shipped_order_leaves_the_table_the_whole_window(self):
        """The control. `right` is split off the harness AFTER `repos` and lands beside
        the harness only, so it costs the table nothing — without this the assertions
        below could be satisfied by subtracting `right` unconditionally."""
        self.assertEqual(
            layout.repos_cols(["top", "bottom", "repos", "right"], window_cols=110), 110)

    def test_a_side_slot_split_first_insets_bottom_by_its_columns_and_its_border(self):
        """The defect. `instance.frame_of` keeps an operator's `[frame] slots` order
        verbatim — `tests/test_frame_config.py` calls that a promise — so this is a frame
        charter ships the ability to ask for, not a corner. Both orders that put `right`
        first are asserted, because "before `repos`" is the property and "first in the
        list" is only one spelling of it."""
        for order in (["right", "top", "repos"], ["top", "right", "repos"]):
            with self.subTest(order=order):
                self.assertEqual(layout.repos_cols(order, window_cols=110), 87)

    def test_the_inset_is_the_slots_own_size_and_not_a_literal(self):
        """87 is `110 - SLOT_SIZE["right"] - _BORDER_COLS`, asserted as that arithmetic
        rather than restated, so a change to the sidebar's width moves this answer
        instead of leaving a stale constant that is right for one release."""
        self.assertEqual(
            layout.repos_cols(["right", "repos"], window_cols=110),
            110 - layout.SLOT_SIZE["right"] - layout._BORDER_COLS)

    def test_two_side_slots_before_bottom_are_both_charged(self):
        """The arithmetic accumulates rather than answering "was there a sidebar". No
        second side slot exists today; the next one this frame grows is the reason the
        loop subtracts per slot instead of testing for `right`."""
        self.assertEqual(
            layout.repos_cols(["right", "right", "repos"], window_cols=200),
            200 - 2 * (layout.SLOT_SIZE["right"] + layout._BORDER_COLS))

    def test_a_horizontal_strip_before_bottom_costs_it_no_columns(self):
        """`top` is a `-v` split: it takes rows off the harness and spans its full width.
        Charging it here would shrink the table on every frame that draws a `top`."""
        self.assertEqual(
            layout.repos_cols(["top", "bottom", "repos"], window_cols=110), 110)

    def test_it_never_answers_a_negative_width(self):
        """A terminal narrower than the sidebar. `visible_slots` drops `right` long
        before this, so it is unreachable through the launcher — but the answer feeds a
        `<` against `statusline._LEFT_W`, and a negative there would merely be right by
        accident. Zero says "no columns" and means it."""
        self.assertEqual(layout.repos_cols(["right", "repos"], window_cols=10), 0)

    def test_which_splits_take_columns_is_one_fact_panel_argvs_shares(self):
        """The two places that must agree: this function decides how much width a slot
        costs `bottom`, and `panel_argvs` decides whether tmux is asked for a `-h` at
        all. Two lists of names would drift the day a side slot is added — the frame
        would split it with `-h` and `bottom` would be sized as if it had not.

        Asserted across every slot `instance.FRAME_SLOTS` admits, so a new slot is
        covered the moment it is declared rather than when someone remembers this test.
        """
        from charter import instance
        for slot in instance.FRAME_SLOTS:
            with self.subTest(slot=slot):
                cmd, = layout.panel_argvs(slots=[slot], **PANELS)
                self.assertEqual(
                    _direction(cmd),
                    "-h" if slot in layout._COLUMN_SLOTS else "-v",
                    "panel_argvs and repos_cols disagree about this slot's direction")


class ColumnSizesIsTheHalfThatCanBeAppliedFirst(unittest.TestCase):
    """`column_sizes` exists so `commands_frame._reassert_sizes` can put the side panels
    back at their own width BEFORE it asks tmux how wide the pane beside them is (#510) —
    tmux redistributes every pane proportionally on a window resize, so a sidebar mid-drag
    answers for a geometry that is one command away from not existing.

    Pinned here rather than only through that caller, and `tools/sweep.py` is why: deleting
    the edge filter left the whole suite green, because `_apply_sizes` filters again by
    `_RESIZE_FLAG` on its way to the `-x` and swallowed every extra entry. Two guards in
    sequence, the second hiding the first. This asks the function its own question.
    """

    def test_it_answers_the_column_slots_and_only_those(self):
        self.assertEqual(layout.column_sizes(["top", "bottom", "repos", "right"]),
                         {"right": layout.SLOT_SIZE["right"]})

    def test_a_list_with_no_side_panel_answers_empty_rather_than_the_strips(self):
        """The direction the deleted filter fails in: with it gone this answered `top`,
        `bottom` and `repos` as well, and a caller applying the result as `-x` would be
        asserting a WIDTH on three horizontal strips."""
        self.assertEqual(layout.column_sizes(["top", "bottom", "repos"]), {})

    def test_it_agrees_with_slot_sizes_about_how_wide_a_side_panel_is(self):
        """Two loops, one fact. Both read `_size_of`, so the day they disagree is the day
        one of them grew a table of its own — which is the shape `layout.py`'s own
        docstring says these constants exist to stop."""
        slots = ["top", "bottom", "repos", "right"]
        full = layout.slot_sizes(slots, window_rows=50, content_rows=6)
        columns = layout.column_sizes(slots)
        self.assertTrue(columns, "nothing was compared — the check is vacuous")
        for slot, cells in columns.items():
            self.assertEqual(cells, full[slot], slot)

    def test_an_unknown_name_is_dropped_rather_than_raised_on(self):
        """`[frame] slots` is committed, untrusted input, and `slot_sizes` already
        degrades rather than refusing for exactly that reason."""
        self.assertEqual(layout.column_sizes(["sideways", "right"]),
                         {"right": layout.SLOT_SIZE["right"]})


class SlotSizesAnswersEverySlotAtOnce(unittest.TestCase):
    def test_the_fixed_slots_keep_their_declared_size(self):
        got = layout.slot_sizes(["top", "bottom", "right"], window_rows=50,
                                content_rows=5)
        self.assertEqual(got["top"], layout.SLOT_SIZE["top"])
        self.assertEqual(got["right"], layout.SLOT_SIZE["right"])

    def test_only_the_table_moves_with_the_window(self):
        """The one slot whose answer depends on the window it is in — which is why
        `_reassert_sizes` recomputes on every resize instead of re-applying a constant.
        `bottom` is asserted alongside `top` since #515: it went back to being a fixed
        row, and a `slot_sizes` that kept treating it as the variable one would still
        answer plausibly at every window size while starving the harness by a row."""
        tall = layout.slot_sizes(["top", "bottom", "repos", "right"], window_rows=50,
                                 content_rows=99)
        short = layout.slot_sizes(["top", "bottom", "repos", "right"], window_rows=22,
                                  content_rows=99)
        self.assertEqual(tall["top"], short["top"])
        self.assertEqual(tall["bottom"], short["bottom"])
        self.assertEqual(tall["right"], short["right"])
        self.assertGreater(tall["repos"], short["repos"])

    def test_a_size_map_is_what_panel_argvs_splits_with(self):
        """The seam that makes any of this reach tmux: `panel_argvs` takes the map and
        emits it as `-l`. Asserted as the literal string tmux would see, so a map that is
        built correctly and then ignored is red."""
        sizes = layout.slot_sizes(["repos"], window_rows=50, content_rows=7)
        cmd, = layout.panel_argvs(slots=["repos"], sizes=sizes, **PANELS)
        self.assertEqual(_size(cmd), "7")

    def test_a_missing_entry_degrades_to_the_fixed_size_rather_than_raising(self):
        """`panel_argvs` reads *sizes* with a per-slot fallback. A `KeyError` here would
        be raised from inside a launch, where the whole frame is lost over one slot."""
        cmd, = layout.panel_argvs(slots=["repos"], sizes={"top": 4}, **PANELS)
        self.assertEqual(_size(cmd), str(layout.SLOT_SIZE["repos"]))


class WindowInTheOperatorsServer(unittest.TestCase):
    """`new-window` and `respawn-pane` — the frame built INSIDE a tmux the operator is
    already in, instead of a private server nested in their pane.

    Two commands rather than one for a measured reason (tmux 3.7c). `remain-on-exit` is
    what keeps a dead harness pane askable long enough for its exit status to be read,
    and there is no way to set it ON a pane that does not exist yet — every option tmux
    would otherwise inherit it from is global or session-scoped, and writing either on
    somebody else's server is exactly what this path exists not to do. So the window is
    created running a placeholder that never exits, `remain-on-exit` is set on that
    pane, and only THEN is the harness respawned into the same pane (`respawn-pane -k`
    keeps the pane's `%N` id, verified against 3.7c). The race the private-server path
    closes with `_PLACEHOLDER_CONF` is not narrowed here — it is removed.
    """

    def test_the_window_is_created_in_the_operators_own_session(self):
        cmd = layout.window_argv(socket="/private/tmp/tmux-502/default", session="$1",
                                 window="charter-demo-1234", cwd="/work/repo")
        self.assertEqual(cmd[:3], ["tmux", "-S", "/private/tmp/tmux-502/default"])
        self.assertIn("new-window", cmd)
        self.assertEqual(cmd[cmd.index("-t") + 1], "$1")
        self.assertEqual(cmd[cmd.index("-n") + 1], "charter-demo-1234")

    def test_the_window_is_created_in_the_background(self):
        """`-d`: the operator is switched to the frame once it is BUILT, never onto a
        half-drawn window with a placeholder running in it."""
        cmd = layout.window_argv(socket="/s", session="$1", window="f", cwd="/work/repo")
        self.assertIn("-d", cmd)

    def test_both_the_window_and_the_pane_ids_are_asked_for(self):
        """The pane id scopes every split and the exit-status query; the window id is
        what `kill-window` targets at the end. Indices are useless for both — tmux
        renumbers windows and panes (see this module's own docstring)."""
        cmd = layout.window_argv(socket="/s", session="$1", window="f", cwd="/work/repo")
        self.assertEqual(cmd[cmd.index("-F") + 1], "#{window_id} #{pane_id}")
        self.assertIn("-P", cmd)

    def test_the_placeholder_is_what_the_window_is_created_running(self):
        """After the `--`, so tmux runs it as a program rather than reading it as one
        of `new-window`'s own options — the same placement `session_argv` and
        `panel_argvs` already use for the harness and the panels.

        That the placeholder never exits on its own cannot be asserted from an argv;
        `tests/test_frame_tmux_integration.py` runs this exact command against a real
        server and reads back a pane still alive."""
        cmd = layout.window_argv(socket="/s", session="$1", window="f", cwd="/work/repo")
        self.assertTrue(layout.PLACEHOLDER, "a window has to be created running something")
        self.assertEqual(cmd[cmd.index("--") + 1:], layout.PLACEHOLDER)
        self.assertEqual(cmd.count("--"), 1)

    def test_the_harness_replaces_the_placeholder_in_the_same_pane(self):
        cmd = layout.respawn_argv(socket="/s", harness_pane="%7", env={}, cwd="/work/repo",
                                  harness_argv=["claude", "--resume", "a;b"])
        self.assertIn("respawn-pane", cmd)
        self.assertIn("-k", cmd)
        self.assertEqual(cmd[cmd.index("-t") + 1], "%7")
        self.assertEqual(cmd[cmd.index("--") + 1:], ["claude", "--resume", "a;b"])

    def test_the_harness_argv_is_never_joined(self):
        """The same rule the rest of this module pins: `a;b` reaching tmux as one
        element is inert, and as part of a joined string is a command separator."""
        cmd = layout.respawn_argv(socket="/s", harness_pane="%7", env={}, cwd="/work/repo",
                                  harness_argv=["claude", "-p", "hi; touch INJ"])
        self.assertEqual(cmd[-1], "hi; touch INJ")

    def test_charters_environment_rides_on_the_respawn_not_the_session(self):
        """`-e` puts a variable on THIS pane's own process and nowhere else. The
        private-server path carries `CHARTER_SESSION_ID` in the environment
        `new-session` inherits, which is not available here — the server is already
        running, and it is the operator's. `set-environment -t <their session>` would
        reach the harness, and would also hand every new shell they open a frame id
        that is not theirs.

        Re-measured against tmux 3.7c, correcting what this docstring used to claim:
        `respawn-pane -e` does NOT replace the pane's environment, it OVERLAYS it — a
        server holding `FOO`/`BAZ`, respawned with only `-e BAR=`, produced a pane with
        all three. That is why this call may carry a named few rather than everything
        (#446, `commands_frame._guest_harness_env`)."""
        cmd = layout.respawn_argv(socket="/s", harness_pane="%7", cwd="/work/repo",
                                  env={"CHARTER_SESSION_ID": "demo-1",
                                       "CHARTER_HARNESS": "claude-code"},
                                  harness_argv=["claude"])
        self.assertIn("-e", cmd)
        pairs = [cmd[i + 1] for i, a in enumerate(cmd) if a == "-e"]
        self.assertEqual(sorted(pairs),
                         ["CHARTER_HARNESS=claude-code", "CHARTER_SESSION_ID=demo-1"])
        # ...and before the `--`, so they are `respawn-pane`'s own options and never
        # get grafted onto the harness's argv.
        self.assertTrue(all(cmd.index(p) < cmd.index("--") for p in pairs))


    def test_the_harness_starts_where_charter_was_typed(self):
        """A pane in a server charter did not start inherits the SESSION's directory —
        wherever the operator was when they first ran `tmux`, which is not where they
        typed `charter claude`. The panels split off this pane inherit its directory in
        turn, and `workspace.resolve()` reads exactly that."""
        for cmd in (layout.window_argv(socket="/s", session="$1", window="f",
                                       cwd="/work/repo"),
                    layout.respawn_argv(socket="/s", harness_pane="%7", env={},
                                        cwd="/work/repo", harness_argv=["claude"])):
            with self.subTest(cmd=cmd[3]):
                self.assertEqual(cmd[cmd.index("-c") + 1], "/work/repo")


    def test_a_panel_can_be_handed_an_environment_of_its_own(self):
        """A pane created on a server charter did not start inherits THAT SERVER's
        environment — whatever the operator's shell had when they first ran `tmux`, days
        ago. On charter's own server the panels inherit the launcher's environment
        because `new-session` is what starts the server; there is no such moment here, so
        the same values ride on `split-window -e`, exactly as the harness's do on
        `respawn-pane -e`.

        Omitted entirely when there is nothing to carry, so the private-server path's
        command is byte-for-byte what it always was."""
        with_env = layout.panel_argvs(slots=["top"], session="f", socket="/s",
                                      harness_pane="%3",
                                      env={"CHARTER_ROOT": "/plane"})[0]
        self.assertEqual(with_env[with_env.index("-e") + 1], "CHARTER_ROOT=/plane")
        self.assertLess(with_env.index("-e"), with_env.index("--"),
                        "`-e` is `split-window`'s own option, never part of the panel's "
                        "argv")
        without = layout.panel_argvs(slots=["top"], session="f", socket="/s",
                                     harness_pane="%3")[0]
        self.assertNotIn("-e", without)


class ServerSelection(unittest.TestCase):
    def test_a_split_can_be_aimed_at_the_operators_server_too(self):
        """`panel_argvs` and `session_argv` grew no new parameter for this: the socket
        they already take is now either charter's own server NAME or a socket PATH, and
        `tmuxctl.server_argv` is the one place that difference turns into `-L` or `-S`."""
        cmds = layout.panel_argvs(slots=["top"], session="f", socket="/tmp/tmux-1/default",
                                  harness_pane="%3")
        self.assertEqual(cmds[0][:3], ["tmux", "-S", "/tmp/tmux-1/default"])


class NothingUnnamedReachesACommandLine(unittest.TestCase):
    """#446 — a tmux `-e` is argv, and argv is not private.

    #412 narrowed `session_argv`'s `-e` to a named list and left `respawn_argv`'s
    passing `dict(os.environ, …)` whole, because the rule lived at the CALL SITES. The
    same defect, twice, a release apart. So the rule moved to the one funnel every `-e`
    in charter goes through (`layout._env_argv`), and these are that funnel's tests:
    a value that has no business on a command line cannot get onto one from ANY caller,
    including one written next month.

    Never a real credential in a fixture, and never an assertion that prints a whole
    environment: the decoy below is a synthetic string and the failure messages name
    only which builder leaked it.
    """

    #: A value no environment could produce by accident, so "it did not appear" is a
    #: fact about the builder rather than about the string being unlikely.
    DECOY = "decoy-value-9f2c1b-must-never-reach-argv"

    def _hostile_env(self):
        """One decoy under every spelling this guard could plausibly be written to miss.

        `CHARTER_SOMETHING` is the sharp one: a `CHARTER_`-prefix glob — the shape the
        allowlist deliberately is NOT — would carry it happily, and it is exactly how a
        variable invented next release gets onto a command line nobody re-reviewed.
        """
        return {
            "CHARTER_SESSION_ID": "demo-1",
            "OP_SERVICE_ACCOUNT_TOKEN": self.DECOY,
            "CHARTER_BRIDGE_TOKEN": self.DECOY,
            "charter_session_id": self.DECOY,
            "CHARTER_SESSION_ID_EXTRA": self.DECOY,
            "SSH_AUTH_SOCK": self.DECOY,
            "": self.DECOY,
        }

    def _builders(self, env):
        return {
            "respawn_argv": lambda: layout.respawn_argv(
                socket="/tmp/tmux-1/default", harness_pane="%7", env=env, cwd="/w",
                harness_argv=["claude"]),
            "session_argv": lambda: layout.session_argv(
                session="f", conf="/c", socket="charter", cols=80, rows=24,
                harness_argv=["claude"], env=env),
            "panel_argvs": lambda: layout.panel_argvs(
                slots=["top"], session="f", socket="charter", harness_pane="%3",
                env=env),
        }

    def test_no_builder_will_put_an_unlisted_name_on_a_command_line(self):
        for name, build in self._builders(self._hostile_env()).items():
            with self.subTest(builder=name):
                with self.assertRaises(ValueError) as caught:
                    build()
                # The refusal must name what it refused and NOT what was in it.
                self.assertNotIn(self.DECOY, str(caught.exception),
                                 f"{name}'s own refusal printed the value it refused")

    def test_the_refusal_is_loud_rather_than_a_silent_drop(self):
        """Dropping the extras quietly would leave a caller believing it had handed the
        harness a variable that never arrived, AND would make the next leak invisible
        instead of impossible. The raise is the guard."""
        with self.assertRaises(ValueError):
            layout.respawn_argv(socket="charter", harness_pane="%7",
                                env={"CHARTER_SESSION_ID": "d", "LANG": "en_US.UTF-8"},
                                cwd="/w", harness_argv=["claude"])

    def test_every_carriable_name_really_does_travel(self):
        """The other direction, so the guard above cannot be satisfied by a builder that
        carries nothing at all. Asserted against `CARRIABLE` itself rather than a second
        copy of the list — a copy is a tautology when the constant is what changed."""
        env = {name: f"v-{name}" for name in layout.CARRIABLE}
        cmd = layout.respawn_argv(socket="charter", harness_pane="%7", env=env, cwd="/w",
                                  harness_argv=["claude"])
        carried = {cmd[i + 1].split("=", 1)[0] for i, a in enumerate(cmd) if a == "-e"}
        self.assertEqual(carried, set(layout.CARRIABLE))

    def test_nothing_carriable_is_a_credential_by_name(self):
        """The property that makes putting any of these on an argv acceptable at all.
        A name is not proof, but a name that ANNOUNCES a secret is proof of the
        opposite, and this is the check that fires when the list grows."""
        for name in layout.CARRIABLE:
            for word in ("TOKEN", "SECRET", "PASSWORD", "KEY", "CREDENTIAL", "AUTH"):
                self.assertNotIn(word, name.upper(), name)


if __name__ == "__main__":
    unittest.main()
