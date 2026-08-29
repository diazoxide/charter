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
`changes` a section rather than a pane), and each placed pane is ~7 of a switch's 41 tmux
invocations — measured at ~360 ms on tmux 3.7c and ~395 ms at the 3.2 floor. So the bars
ship as components a `[[frame.component]]` table can place, and
:class:`ABarIsPlaceableByConfig` is what says that route actually works end to end rather
than leaving two components nothing could ever ask for.
"""

from __future__ import annotations

import os
import unittest
from unittest import mock

from charter import config, instance, tui
from charter.frame.component import Fixed
from charter.frame import builtins, layout, slots, state
from tests._isolation import PersonaIso
from tests.test_frame_chat_switch import _plant


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

    def test_a_row_with_no_room_for_every_name_keeps_yours_and_counts_the_rest(self):
        row = self._row(30)
        self.assertIn("*api.2", row)
        self.assertIn("+2", row)
        self.assertNotIn("api.1", row)
        self.assertNotIn("api.3", row)

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
            text = row[0] if row else ""
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

    def test_one_chat_carries_the_add_affordance_and_two_do_not(self):
        row = self._row(120, names=["api.1"], here="api.1", note=slots.ADD_CHAT)
        self.assertIn(slots.ADD_CHAT, row)
        self.assertNotIn(slots.ADD_CHAT, self._row(120))

    def test_the_affordance_is_dropped_before_any_name_is(self):
        """It is a reminder and the names are the readout, so it goes first."""
        row = self._row(28, names=["api.1"], here="api.1", note=slots.ADD_CHAT)
        self.assertIn("api.1", row)
        self.assertNotIn(slots.ADD_CHAT, row)

    def test_no_names_at_all_is_no_row(self):
        self.assertEqual(slots._bar("chats", [], "api.1", 200), [])

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


class TheChatBarReadsThePlane(PersonaIso, unittest.TestCase):
    """`slots.chats_bar` over a real frame directory."""

    def setUp(self):
        super().setUp()
        self._env = mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "api"},
                                    clear=False)
        self._env.start()
        self.addCleanup(self._env.stop)

    def test_the_bar_hides_the_second_name_when_there_is_one_chat_and_says_how(self):
        """Stage 5b's exit criterion, first half: "the chat bar is absent with one chat".
        Absent means it stops being a list — it still says which chat you are in and how
        to get a second, because a row that vanished would leave an operator with no way
        to learn the feature exists."""
        _plant("api.1", workspace="api")
        row = slots.chats_bar("api.1", 200)[0]
        self.assertIn("api.1", row)
        self.assertIn(slots.ADD_CHAT, row)
        self.assertIn("charter <harness>", row,
                      "the affordance must name something that works today")

    def test_the_bar_lists_both_chats_when_there_are_two(self):
        """The other half: "present with two"."""
        _plant("api.1", workspace="api")
        _plant("api.2", workspace="api")
        row = slots.chats_bar("api.2", 200)[0]
        self.assertIn("api.1", row)
        self.assertIn("*api.2", row)
        self.assertNotIn(slots.ADD_CHAT, row)

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
    """

    ALL = ["chats", "workspaces", "top", "bottom", "repos", "right"]

    def _kept(self, cols, rows):
        frame = config.FRAME
        return layout.visible_slots(list(self.ALL), cols, rows,
                                    frame["min_cols"], frame["min_rows"])

    def test_a_roomy_terminal_keeps_both_bars(self):
        self.assertEqual(self._kept(200, 50), self.ALL)

    def test_a_short_terminal_gives_up_both_bars_with_the_identity_row(self):
        kept = self._kept(200, 16)
        for gone in ("chats", "workspaces", "top", "right"):
            self.assertNotIn(gone, kept)
        self.assertIn("bottom", kept,
                      "the attention strip is the one slot that never goes")

    def test_the_drop_list_is_what_decides_and_not_a_second_copy_of_it(self):
        """The property that makes `_DROP_ORDER` a constant: every row-edge name in it is
        one a short terminal actually loses. Deleting an entry has to change this."""
        kept = self._kept(200, 16)
        for name in layout._ROW_DROPS:
            self.assertNotIn(name, kept, f"{name} is in _ROW_DROPS and survived")
        self.assertEqual(layout._ROW_DROPS, ("chats", "workspaces", "top"))
