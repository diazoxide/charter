"""#513: the frame names charter's own concepts, in charter's own words.

The operator asked "why we have agents in top bar? Agents are not our concept — we have
personas concept", and they were right: `statusline._persona_line` labelled the roster of
other personas `◇ agents`, three lines under a comment that called them personas.

**The property is not the spelling `agents`.** It is that a label the frame draws for a
charter concept must name the concept and not the mechanism ONE harness happens to reach
it with. A persona is charter's — `charter persona`, `personas/`, `docs/personas.md`, the
ADRs, and the no-active branch of this very renderer all say so. A *sub-agent* is Claude
Code's dispatch API. The two are not synonyms, and the frame is drawn identically under
codex and opencode, neither of which has a sub-agent at all: on two of the three harnesses
charter supports, `◇ agents` named a thing that does not exist there.

So the tests below render the surfaces an operator actually reads — `slots.render("top",
…)`, the panel row, and the two `statusline` renderers that feed the roster — and assert
that no word from Claude Code's dispatch vocabulary appears on them. Every persona,
workspace and vault name in the fixtures is chosen to carry none of those words itself, so
a hit can only have come from a label charter wrote.

What this deliberately does NOT assert: that the word "agent" is absent from charter's
source or docs. `.claude/agents/`, `charter persona sync-agents`, the `Task`/`Agent` tool
names in `hooks.py` and every "dispatched as a sub-agent" sentence are Claude Code's own
API surface being named correctly, and renaming those would be the same mistake pointing
the other way. The bound here is the frame's own labels.
"""

from __future__ import annotations

import os
import sys
import unittest
from unittest import mock

from charter import persona, statusline, tui
from charter.frame import slots

from tests._isolation import PersonaIso

#: Claude Code's dispatch vocabulary. A frame label containing any of these is naming a
#: mechanism, and a mechanism only one of charter's three harnesses has.
_HARNESS_DISPATCH_WORDS = ("agent", "agents", "sub-agent", "sub agent", "subagent")

#: Fixture names chosen so none of them contains a dispatch word — otherwise a hit below
#: could come from the operator's own data rather than from a label charter wrote, and
#: the test would be observing the fixture instead of the code.
_PERSONAS = ("steward", "devops", "qa")


def _offending(text: str) -> list[str]:
    low = text.lower()
    return [w for w in _HARNESS_DISPATCH_WORDS if w in low]


class RosterLabelNamesTheConcept(PersonaIso):
    """The roster label on the identity row."""

    def setUp(self) -> None:
        super().setUp()
        for n in _PERSONAS:
            self.make_persona(n, role=f"{n} role")
        for w in _PERSONAS:
            self.assertEqual([], _offending(w),
                             "precondition: a fixture persona name carries a dispatch "
                             "word, so any hit below would be the fixture's, not a label's")

    def _top(self) -> str:
        """The panel row an operator reads, through the real slot renderer."""
        with mock.patch.object(sys.stdout, "fileno", return_value=1, create=True), \
             mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((200, 3))):
            return tui.strip_ansi(slots.render("top", "vocab-frame"))

    def test_the_roster_label_says_personas(self):
        """The positive half: the label is present and it is charter's word. Asserted on
        the rendered row, not on the module constant that produces it — a test comparing
        the label to itself would survive any rename."""
        persona.set_active("steward")
        line = tui.strip_ansi(statusline._persona_line() or "")
        self.assertIn("◇ personas ", line)
        self.assertIn("devops", line)
        self.assertIn("qa", line)

    def test_the_identity_row_carries_no_harness_dispatch_word(self):
        """The property. With an active persona and two others, the row that names them
        must not reach for Claude Code's word for how one of them gets dispatched."""
        persona.set_active("steward")
        line = tui.strip_ansi(statusline._persona_line() or "")
        self.assertEqual([], _offending(line),
                         f"the persona roster line names a harness mechanism: {line!r}")

    def test_the_rendered_panel_row_carries_no_harness_dispatch_word(self):
        """One layer up, at the surface that actually prints. `_persona_line` is only the
        status line's *fallback* path; `slots._top` is what a framed operator sees on
        every repaint, and a label fixed one layer below the printer is not fixed."""
        persona.set_active("steward")
        row = self._top()
        self.assertIn("steward", row)
        self.assertEqual([], _offending(row),
                         f"the frame's `top` row names a harness mechanism: {row!r}")

    def test_no_active_persona_still_names_only_personas(self):
        """The other branch of the same renderer. It already said `persona none`; this
        pins that the branch charter falls into on a fresh plane cannot drift either."""
        persona.clear_active()
        line = tui.strip_ansi(statusline._persona_line() or "")
        self.assertIn("persona", line)
        self.assertEqual([], _offending(line),
                         f"the no-active persona line names a harness mechanism: {line!r}")

    def test_the_sidebar_chips_carry_no_harness_dispatch_word_either(self):
        """The roster's OTHER renderer. `_persona_chips` draws the same personas down the
        status line's right column, and a vocabulary fixed in one of the two places would
        just move the operator's question to the other surface. This asserts the word is
        absent — not what the chips return — so it constrains nothing about their shape."""
        persona.set_active("steward")
        chips = [tui.strip_ansi(c) for c in statusline._persona_chips()]
        self.assertTrue(chips, "precondition: three personas exist, so chips are drawn")
        joined = "\n".join(chips)
        self.assertEqual([], _offending(joined),
                         f"a persona chip names a harness mechanism: {joined!r}")


class TheLabelIsMeasuredNotCounted(PersonaIso):
    """`personas` is two columns wider than `agents`, and the row it sits on is shared
    with the workspace, the vault, the context gauge and the version. The row is bounded
    by `tui.truncate` against the pane, so the two columns cost the tail of the row and
    never an overflow — pinned here because the failure mode of getting this wrong is a
    wrapped panel, which no assertion about the label's text would catch."""

    def setUp(self) -> None:
        super().setUp()
        for n in _PERSONAS:
            self.make_persona(n, role=f"{n} role")
        persona.set_active("steward")

    def test_the_row_still_fits_a_narrow_pane(self):
        for cols in (24, 40, 60, 80, 120):
            with self.subTest(cols=cols):
                with mock.patch.object(sys.stdout, "fileno", return_value=1,
                                       create=True), \
                     mock.patch("os.get_terminal_size",
                                return_value=os.terminal_size((cols, 3))):
                    row = slots.render("top", "vocab-frame")
                for ln in row.split("\n"):
                    self.assertLessEqual(tui.width(tui.strip_ansi(ln)), cols,
                                         f"`top` overflowed a {cols}-column pane: {ln!r}")


if __name__ == "__main__":
    unittest.main()
