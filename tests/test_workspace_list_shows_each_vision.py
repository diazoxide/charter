"""`charter workspace list` shows each workspace's vision, because that is what gets matched.

A handoff proposal picks its target by reading other workspaces' visions against the ask
(`docs/handoff.md`, and the "Where this could run" block). Until now the only surface that
answered "what is that workspace for" was SessionStart's neighbours digest, which is
bounded to a few rows and clipped — so the command an agent is told to run said `WORKSPACE
MODE CLONES REPOS` and nothing about what any of them is for.

**Untruncated, as the trailing field.** A vision clipped at a column width is a match made
against half a sentence; nothing after it needs the row to keep its shape, so it costs
alignment nothing to leave it whole. `REPOS` joins the measured columns instead
(`tui.column`), which is what keeps every row's VISION starting in the same cell.

**Escaped at the render.** A vision is committed text somebody else wrote, and this is a
table drawn on a terminal: a newline in one forges a row, and an ANSI sequence redraws the
screen the operator is reading the table on. `contain.one_line` is the answer charter
already keeps for exactly this shape of field.
"""
from __future__ import annotations

import io
import unittest
from contextlib import redirect_stderr, redirect_stdout
from types import SimpleNamespace
from unittest import mock

from charter import commands_workspace, config, tui, workspace
from tests._isolation import PersonaIso


class TheVisionColumn(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        config.WORKSPACES_DIR.mkdir(parents=True, exist_ok=True)
        # `alpha` carries a clone so the REPOS column has a value to be measured from —
        # the column that stops being trailing here — and `beta` carries none, which is
        # the `—` case the dash test is about.
        (workspace.workspace_dir("alpha") / "api" / ".git").mkdir(parents=True)
        workspace.workspace_dir("beta").mkdir(parents=True, exist_ok=True)

    def listing(self) -> list[str]:
        out = io.StringIO()
        with redirect_stdout(out), redirect_stderr(io.StringIO()):
            commands_workspace.cmd_workspace_list(SimpleNamespace())
        return out.getvalue().splitlines()

    def header(self) -> str:
        return next(ln for ln in self.listing() if "WORKSPACE" in ln)

    def rows(self, lines: list[str] | None = None) -> list[str]:
        lines = self.listing() if lines is None else lines
        return [ln for ln in lines
                if ln and not ln.startswith("Active workspace:")
                and "WORKSPACE" not in ln]

    def row_for(self, name: str) -> str:
        hit = [r for r in self.rows() if r[2:].startswith(name)]
        self.assertEqual(len(hit), 1, f"{name!r} is on {len(hit)} row(s)")
        return hit[0]

    def test_the_header_names_the_vision_column_last(self):
        """`REPOS` was the trailing field and is now a column; `VISION` takes its place."""
        self.assertTrue(self.header().rstrip().endswith("VISION"), self.header())

    def test_each_row_carries_its_visions_first_line(self):
        """The first line is the vision the neighbours digest already shows and the one a
        proposal matches against. The rest of `## Vision` is a section, not a label."""
        workspace.set_vision("alpha", "Ship it\nmore")
        lines = self.listing()
        self.assertTrue(self.rows(lines)[0].rstrip().endswith("Ship it"),
                        "\n".join(lines))
        self.assertNotIn("more", "\n".join(lines))

    def test_a_workspace_with_no_vision_shows_a_dash(self):
        """An empty cell reads as a rendering fault; the dash says nobody recorded one —
        which is also what makes that workspace un-proposable.

        Asked of `alpha`, which HAS a clone, deliberately: REPOS draws its own dash for a
        workspace with none, and a row ending in one would then be answered by the column
        next door however this cell rendered.
        """
        self.assertTrue(self.row_for("alpha").rstrip().endswith("—"),
                        self.row_for("alpha"))

    def test_the_always_present_workspace_shows_a_dash_with_no_directory(self):
        """`default` is folded into the listing whether or not it has a directory (#745),
        so the vision reader has to answer for a workspace that is not on disk.

        Counted rather than matched at the end: `default` has no clones either, so its row
        carries a dash in REPOS as well — and a row whose VISION cell rendered as nothing
        would still END in a dash. Two is the answer only a drawn vision cell gives.
        """
        row = self.row_for(config.DEFAULT_WORKSPACE)
        self.assertTrue(row.rstrip().endswith("—"), row)
        self.assertEqual(row.count("—"), 2, row)

    def test_a_control_character_in_a_vision_cannot_forge_a_row(self):
        """Committed text reaching a terminal. `\\x1b[2J` clears the screen the table is
        being read on, and a newline would draw a row charter did not.

        Asserted on the exact escape the render must produce and the exact raw sequence
        that must be absent, rather than on "an ESC appears somewhere": charter's own
        styling puts real escapes in other output by design, so that test could not tell
        charter's rendering from the attack.
        """
        workspace.set_vision("alpha", "a\x1b[2Jb")
        lines = self.listing()
        self.assertTrue(self.row_for("alpha").rstrip().endswith("a\\x1b[2Jb"),
                        self.row_for("alpha"))
        self.assertNotIn("\x1b[2J", "\n".join(lines))
        self.assertEqual(len(self.rows(lines)), len(workspace.list_workspaces()) + 1)

    def test_a_newline_in_a_vision_cannot_forge_a_row(self):
        """The other half of the same property, and the one the first-line rule already
        covers — stated separately so deleting either one goes red."""
        workspace.set_vision("alpha", "goal\nworkspaces/zzz  local  0  —  forged")
        lines = self.listing()
        self.assertEqual(len(self.rows(lines)), len(workspace.list_workspaces()) + 1)
        self.assertNotIn("forged", "\n".join(lines))

    def test_the_vision_column_starts_at_one_cell_for_every_row(self):
        """`REPOS` stopped being the trailing field and has to be padded now. Without
        that, a workspace with a longer repo list pushes its own vision right and the
        column stops being one."""
        (workspace.workspace_dir("beta") / "a-much-longer-repository-name"
         / ".git").mkdir(parents=True)
        workspace.set_vision("alpha", "Alpha vision")
        workspace.set_vision("beta", "Beta vision")
        at = set()
        for name, vision in (("alpha", "Alpha vision"), ("beta", "Beta vision")):
            row = tui.strip_ansi(self.row_for(name))
            at.add(tui.width(row[:row.rindex(vision)]))
        self.assertEqual(len(at), 1,
                         f"the vision column starts at cells {sorted(at)}")

    def test_a_vision_that_cannot_be_read_costs_its_cell_and_not_the_listing(self):
        """A `workspace.md` that is a symlink out of the plane, a FIFO, or simply gone. The
        listing is how an operator finds out what exists; one unreadable charter must cost
        one cell."""
        with mock.patch("charter.workspace.read_vision", side_effect=OSError):
            lines = self.listing()
        self.assertEqual(len(self.rows(lines)), len(workspace.list_workspaces()) + 1)
        self.assertTrue(self.rows(lines)[0].rstrip().endswith("—"),
                        "\n".join(lines))

    def test_a_long_vision_is_not_truncated(self):
        """The trailing field is not clipped, and that is the point of putting it last: a
        proposal matched against half a sentence is a proposal made on half the evidence.
        """
        long = "deliver " + "the whole billing rewrite and its migrations " * 4
        workspace.set_vision("alpha", long)
        self.assertTrue(self.row_for("alpha").rstrip().endswith(long.strip()))


if __name__ == "__main__":
    unittest.main()
