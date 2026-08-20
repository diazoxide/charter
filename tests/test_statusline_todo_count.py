"""How many todos this workspace still has open — visible without running a command.

A todo that has to be asked for is a todo nobody asks for. The store already exists
(`charter ws todo`), but until something says a list is *there*, a session's only route
to it is remembering to look — which is precisely the failure the store was built to end:
work consciously deferred leaving no trace.

The count goes in the **top zone**, beside the workspace name, because the status line
has one organising rule and it decides this on its own: *a count lives next to what it
counts*. Open todos are a property of the ACTIVE WORKSPACE — not of the session (the
bottom strip), not of the repo column, and not of the persona column. The same rule is
why `repos N` heads the repo column and `personas N` heads the persona column rather
than all three sitting in a row along the top, which is how they used to render.

Beside the name rather than on a line of its own, too. A row costs a row on *every*
turn, and the thing it would carry is usually one digit.

And zero renders **nothing**, the discipline `_session_news` and `_mem_badge` already
keep: a `todo 0` that never changes becomes furniture within a day, and then a real
`todo 7` in that spot draws no more attention than the zero did. Presence is the signal.
"""

from __future__ import annotations

import os
import re
import unittest
from unittest import mock

from charter import statusline, todos, workspace
from tests._isolation import PersonaIso


def _plain(s: str) -> str:
    return re.sub(r"\x1b\[[0-9;]*m", "", s)


def _raw(payload=None, width=200):
    """Rendered lines exactly as emitted, frame included."""
    old = os.environ.get("COLUMNS")
    os.environ["COLUMNS"] = str(width)
    try:
        return [_plain(ln) for ln in statusline.render(payload or {}).split("\n")]
    finally:
        if old is None:
            os.environ.pop("COLUMNS", None)
        else:
            os.environ["COLUMNS"] = old


def _lines(payload=None, width=200):
    """Content lines with the frame and the zone dividers stripped."""
    out = []
    for ln in _raw(payload, width):
        if not ln.strip() or set(ln.strip()) <= set("┌─┐└┘├┤"):
            continue
        if ln.startswith("│ ") and ln.rstrip().endswith("│"):
            ln = ln[2:].rstrip()[:-1].rstrip()
        out.append(ln)
    return out


def _top(payload=None, width=200) -> str:
    """Zone 1 — the row that answers *where am I*, which is always the first one."""
    return _lines(payload, width)[0]


_USAGE = {"session_id": "todo-count",
          "context_window": {"used_percentage": 22,
                             "current_usage": {"cache_read_input_tokens": 100,
                                               "cache_creation_input_tokens": 10}}}


class TodoIso(PersonaIso):
    """Two workspaces, so "the active one" is a claim a test can actually falsify.

    `default` is what `workspace.resolve` answers with when nothing has pinned anything —
    no env var, no cwd inside a workspace tree, no session or terminal pointer — which is
    exactly the state `PersonaIso` leaves behind.
    """

    def setUp(self) -> None:
        super().setUp()
        workspace.ensure("default")
        workspace.ensure("other")

    def open_one(self, ws: str, text: str) -> None:
        todos.add(ws, text)

    def close_all(self, ws: str) -> None:
        """Done deletes — the journal is the permanent record of what happened."""
        for p in todos.todos_dir(ws).glob("*.md"):
            if p.name != "MEMORY.md":
                p.unlink()


class TheCountSitsWithWhatItCounts(TodoIso):
    def test_the_open_count_renders_in_the_top_zone(self):
        """Zone 1 answers *where am I*, and "three things still open here" is part of
        that answer — it is true of the workspace, not of this session."""
        self.open_one("default", "prove the live gh issue create path")
        self.open_one("default", "come back to the labels on the report form")
        self.open_one("default", "decide whether personas keep their own queues")
        self.assertIn("todo 3", _top(_USAGE))

    def test_the_count_sits_beside_the_workspace_name(self):
        """Beside the name, ahead of `ws N`: the count belongs to the workspace it
        follows, and `ws N` counts a different thing entirely (how many workspaces exist
        to switch to). Reading order is the only thing saying which noun a count is
        about, so it has to be right.

        This is the ordinary render — no reinit tip, because nothing is stale. The one
        item that may come between the name and the count is covered below.
        """
        self.open_one("default", "one thing worth remembering to do")
        top = _top(_USAGE)
        name, todo, ws = top.index("default"), top.index("todo"), top.index("ws ")
        self.assertLess(name, todo, top)
        self.assertLess(todo, ws, top)

    def test_the_count_appears_nowhere_but_the_top_zone(self):
        """`⛊ 1 denied` once rendered inside the repo column and read as news about a
        repo. One place per fact, and for this fact that place is zone 1."""
        self.open_one("default", "a thing to do")
        rest = _lines(_USAGE)[1:]
        for ln in rest:
            self.assertNotIn("todo", ln, f"the count leaked out of zone 1: {ln!r}")

    def test_the_count_is_the_active_workspaces_own(self):
        """A workspace is the unit of task isolation; a count summing every workspace's
        todos would be a number no single command could ever reproduce."""
        for i in range(2):
            self.open_one("default", f"mine number {i} and nothing like the others")
        for i in range(5):
            self.open_one("other", f"belongs to another task entirely, {i}")
        self.assertIn("todo 2", _top(_USAGE))

    def test_a_todo_recorded_in_another_workspace_alone_shows_nothing(self):
        self.open_one("other", "somebody else's deferred work")
        self.assertNotIn("todo", _top(_USAGE))


class TheWarningOutranksTheCount(TodoIso):
    """Zone 1's left-to-right order IS its truncation order, so the order encodes a
    priority — and a warning outranks information.

    `⚠ reinit` is the only item on this row reporting something BROKEN, and it carries
    the command that fixes it. The todo count is a fact about a healthy workspace. On a
    pane with room for one of them and not the other, the warning is the one that has to
    survive; a count crowding it out would trade a fixable problem for a number.

    The count pays nothing for this in practice: the tip renders only when the on-disk
    structure is genuinely stale, so nearly every turn has no tip at all and the count
    still sits directly against the name whose todos it counts.

    `_stale_structure` is stubbed rather than a real half-migrated workspace being built:
    what is under test is the ORDER of the assembled row, and manufacturing stale
    on-disk layout would test `workspace.needs_reinit` instead.
    """

    def setUp(self) -> None:
        super().setUp()
        self.open_one("default", "something still outstanding in this workspace")
        self.enterContext(mock.patch.object(statusline, "_stale_structure",
                                            return_value=True))

    def test_the_reinit_warning_comes_before_the_count(self):
        top = _top(_USAGE)
        # `rindex` for the workspace count: the tip's own text ends in `charter ws
        # reinit`, so a forward search for "ws " finds the warning, not the count.
        name, warn, todo, ws = (top.index("default"), top.index("⚠ reinit"),
                                top.index("todo"), top.rindex("ws "))
        self.assertLess(name, warn, top)
        self.assertLess(warn, todo, top)
        self.assertLess(todo, ws, top)

    def test_a_pane_that_fits_only_one_of_them_keeps_the_warning(self):
        """The case the ordering exists for, asserted where it actually bites. At this
        width the row has room for the name and the whole tip but not the count, and it
        is the count that must give way."""
        top = _top(_USAGE, width=52)
        self.assertIn("⚠ reinit", top, top)
        self.assertIn("charter ws reinit", top, "the tip's command must survive with it")
        self.assertNotIn("todo", top, top)

    def test_both_render_in_full_once_there_is_room(self):
        top = _top(_USAGE, width=120)
        self.assertIn("⚠ reinit", top)
        self.assertIn("todo 1", top)

    def test_a_healthy_workspace_puts_the_count_straight_after_the_name(self):
        """The tip is the exception, not a permanent gap between name and count."""
        with mock.patch.object(statusline, "_stale_structure", return_value=False):
            top = _top(_USAGE)
        self.assertNotIn("reinit", top)
        self.assertLess(top.index("default"), top.index("todo"))
        self.assertLess(top.index("todo"), top.index("ws "))


class ZeroRendersNothing(TodoIso):
    """A counter that renders every turn is furniture, and a real number inside
    furniture is invisible. Absence has to mean something, so 0 renders nothing."""

    def test_a_workspace_with_nothing_open_shows_no_count(self):
        self.assertNotIn("todo", _top(_USAGE))

    def test_the_whole_status_line_is_free_of_it_when_the_count_is_zero(self):
        for ln in _lines(_USAGE):
            self.assertNotIn("todo", ln, ln)

    def test_the_count_vanishes_again_once_the_last_todo_is_done(self):
        """Not the same assertion as the empty case: a count that appeared and then
        stayed at 0 would be exactly the furniture this rule exists to prevent."""
        self.open_one("default", "the last outstanding thing")
        self.assertIn("todo 1", _top(_USAGE))
        self.close_all("default")
        self.assertNotIn("todo", _top(_USAGE))


class AlignmentSurvivesIt(TodoIso):
    """The layout is framed and column-aligned, and every past defect in it came from a
    character rendering wider than `tui.width` believed. A new element on the top line
    must be provably incapable of that."""

    def setUp(self) -> None:
        super().setUp()
        for i in range(4):
            self.open_one("default", f"deferred item number {i}, distinct from the rest")

    def test_the_frame_stays_square_at_every_pane_width(self):
        """The right border is the ruler: a row that renders wider than counted pushes
        its own `│` past the others."""
        for w in (24, 40, 60, 80, 100, 131, 160, 200, 240):
            with self.subTest(width=w):
                widths = {statusline.tui.width(ln) for ln in _raw(_USAGE, w) if ln.strip()}
                self.assertEqual(len(widths), 1, f"ragged frame at {w}: {sorted(widths)}")

    def test_no_row_ever_exceeds_the_pane(self):
        for w in (24, 40, 60, 80, 100, 131, 160, 200, 240):
            with self.subTest(width=w):
                for ln in _raw(_USAGE, w):
                    self.assertLessEqual(statusline.tui.width(ln), w, ln)

    def test_it_introduces_no_character_a_font_could_draw_wide(self):
        """The rule the layout paid for twice — a `◫` on the repo header, then a `◈` on
        the personas header — is that a new element may not bring a glyph whose width
        only the Unicode tables vouch for. Diffing the top line against the same line
        without any todos isolates exactly what this feature added."""
        with_todos = set(_top(_USAGE))
        self.close_all("default")
        without = set(_top(_USAGE))
        for ch in with_todos - without:
            self.assertLess(ord(ch), 128,
                            f"the todo count introduced {ch!r} (U+{ord(ch):04X}); the top "
                            f"line should have gained ASCII only")

    def test_the_top_line_is_still_identity_and_navigation_only(self):
        """The count is about the workspace named on this row. Nothing else may follow
        it in — the repo count describes the left column, the gauges the session."""
        top = _top(_USAGE)
        for foreign in ("repos", "vaults", "personas", "ctx", "cache", "⚡"):
            self.assertNotIn(foreign, top, top)


class ItStillNeverRaises(TodoIso):
    """The module docstring's hard promise: `render` never raises, and falls back to a
    minimal string if it must. A count is the least important thing on the line, so an
    unreadable todo store must cost the count and nothing else — falling back to
    `⬢ charter` because a directory could not be listed would trade the whole status
    line for one digit."""

    def test_an_unreadable_todo_store_costs_only_the_count(self):
        with mock.patch.object(todos, "count_open", side_effect=OSError("no")):
            top = _top(_USAGE)
        self.assertIn("default", top)
        self.assertIn("ws ", top, "the rest of zone 1 must survive an unreadable store")
        self.assertNotIn("todo", top)

    def test_it_does_not_raise_when_the_store_raises(self):
        for boom in (OSError("no"), RuntimeError("boom"), ValueError("what")):
            with self.subTest(error=type(boom).__name__):
                with mock.patch.object(todos, "count_open", side_effect=boom):
                    self.assertIn("charter", statusline.render(_USAGE))

    def test_the_counter_itself_answers_zero_rather_than_propagating(self):
        with mock.patch.object(todos, "count_open", side_effect=OSError("no")):
            self.assertEqual(statusline._todo_count("default"), 0)

    def test_a_workspace_that_was_never_scaffolded_counts_zero(self):
        """The status line renders against whatever workspace is active, including one
        that exists only as a pointer — it may not require the store to have been
        created first."""
        self.assertEqual(statusline._todo_count("never-created"), 0)


if __name__ == "__main__":
    unittest.main()
