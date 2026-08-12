"""Horizontal rules that make the status line's zones visible.

The layout has always been grouped by scope — *where I am*, then *what is here*, then
*this session* — but the three read as one undifferentiated stack of rows. A rule under
the workspace line and another above the session strip draws the grouping that the
layout already had.

The rules are frame furniture, not content: they are inserted after layout and rendered
by `_boxed`, so they cannot perturb the column maths that ran inside it — the same reason
the box itself is applied last.
"""
from __future__ import annotations

import os
import re
import unittest

from charter import statusline
from tests._isolation import PersonaIso


def _plain(s: str) -> str:
    return re.sub(r"\x1b\[[0-9;]*m", "", s)


def _raw(payload=None, width=200):
    old = os.environ.get("COLUMNS")
    os.environ["COLUMNS"] = str(width)
    try:
        return [_plain(ln) for ln in statusline.render(payload or {}).split("\n") if ln]
    finally:
        if old is None:
            os.environ.pop("COLUMNS", None)
        else:
            os.environ["COLUMNS"] = old


def _is_rule(ln: str) -> bool:
    return bool(ln.strip()) and set(ln.strip()) <= set("├─┤")


_USAGE = {"session_id": "zone-rules",
          "context_window": {"used_percentage": 22,
                             "current_usage": {"cache_read_input_tokens": 100,
                                               "cache_creation_input_tokens": 10}}}


class TestZoneRules(PersonaIso):
    def test_a_rule_sits_directly_under_the_workspace_line(self):
        """The workspace line is zone one on its own; everything below it describes
        what is *in* that workspace."""
        rows = _raw(_USAGE)
        self.assertTrue(_is_rule(rows[2]), rows[:4])

    def test_the_workspace_line_is_still_the_first_content_row(self):
        rows = _raw(_USAGE)
        self.assertIn("⬢", rows[1])
        self.assertFalse(_is_rule(rows[1]))

    def test_a_rule_sits_directly_above_the_session_strip(self):
        """The strip describes the session, not the workspace — so it gets separated
        from the tree the same way the workspace line is."""
        rows = _raw(_USAGE)
        strip_at = next(i for i, ln in enumerate(rows) if "ctx" in ln)
        self.assertTrue(_is_rule(rows[strip_at - 1]), rows[strip_at - 2:strip_at + 1])

    def test_there_are_exactly_two_rules_when_a_strip_is_present(self):
        self.assertEqual(sum(1 for ln in _raw(_USAGE) if _is_rule(ln)), 2)

    def test_there_is_one_rule_when_there_is_no_session_strip(self):
        """A fresh session has no usage yet, so no strip — and a rule above a row that
        does not exist would be a divider separating nothing from the bottom border."""
        self.assertEqual(sum(1 for ln in _raw({}) if _is_rule(ln)), 1)


class TestRulesMatchTheFrame(PersonaIso):
    def test_a_rule_is_exactly_as_wide_as_the_top_border(self):
        """A rule narrower or wider than the box is the most visible possible defect."""
        rows = _raw(_USAGE)
        for ln in rows:
            if _is_rule(ln):
                self.assertEqual(len(ln), len(rows[0]), f"{ln!r} vs {rows[0]!r}")

    def test_a_rule_uses_tee_connectors_not_corners(self):
        """`├`/`┤` join the side borders; `┌`/`┐` would read as a second box starting."""
        for ln in _raw(_USAGE):
            if _is_rule(ln):
                self.assertTrue(ln.startswith("├") and ln.rstrip().endswith("┤"), ln)

    def test_the_frame_still_opens_and_closes_with_corners(self):
        rows = _raw(_USAGE)
        self.assertTrue(set(rows[0]) <= set("┌─┐"), rows[0])
        self.assertTrue(set(rows[-1]) <= set("└─┘"), rows[-1])


class TestTheSentinelNeverReachesTheTerminal(PersonaIso):
    def test_no_marker_leaks_into_a_normal_render(self):
        self.assertNotIn("\x00", statusline.render(_USAGE))

    def test_no_marker_leaks_when_the_pane_is_too_narrow_to_frame(self):
        """`_boxed` returns the body unframed below its minimum width. A rule has no
        meaning without side borders to join, so it must be dropped, not printed raw."""
        body = f"top{statusline._RULE_LINE}bottom".replace(
            statusline._RULE_LINE, "\n" + statusline._RULE_LINE + "\n")
        out = statusline._boxed(body, 10)
        self.assertNotIn("\x00", out)

    def test_a_rule_never_becomes_the_line_the_brand_lands_on(self):
        """`_with_brand` appends to the last line. A brand welded onto a divider would
        be both ugly and, since the divider is pure box-drawing, unreadable."""
        rows = [ln for ln in _raw(_USAGE) if ln.strip()]
        self.assertFalse(_is_rule(rows[-2]), rows[-3:])


class TestRenderStillNeverCrashes(PersonaIso):
    def test_narrow_panes_render_without_raising(self):
        for w in (24, 30, 40, 60, 80, 120, 200):
            out = statusline.render(_USAGE) if w == 200 else None
            with self.subTest(width=w):
                self.assertTrue(_raw(_USAGE, w))


if __name__ == "__main__":
    unittest.main()
