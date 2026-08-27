"""A roster table's columns line up for EVERY name, not for the short ASCII ones (#508).

`charter persona stats` printed its name into `{...:<28}`, and that is two failures
wearing one constant:

* **28 is a guess about content.** `:<28` pads a short value and does nothing at all to a
  long one — it pushes, so MEM, RECENT, VERIFY, DUP, DISP and STATUS land somewhere on
  that row that they land on no other row. A persona is a DIRECTORY under `personas/`, so
  the name arrives from whoever last pushed, and `contain.one_line` *grows* it: a
  separator becomes a four-character escape, so a committed 22-character name renders at
  30 without anyone having typed a long one.
* **`:<28` counts characters, and a terminal lays out CELLS.** A CJK name is two cells per
  glyph and a combining mark is zero, so an 8-glyph name that fits the constant three
  times over is padded to 28 characters and drawn as 36 columns — its row shifts by 8
  without ever going near the boundary. `persona list` measured its own columns and still
  had this half, because it measured them with `len`.

**The property is that the columns line up, not that any field has a particular width.**
So nothing here asserts a number the renderer also spells: a test pinning `28`, or
pinning the new width, passes forever and sees neither failure. Every case renders the
real report and compares row offsets against the header's.

**And offsets are read in cells, never in characters.** `row.index(marker)` is a character
index, which is exactly the unit that was wrong in the first place — a test using it would
report the CJK row as aligned while the terminal drew it eight columns out. `tui.width` of
the prefix is the measurement, and using it here is the same act as using it there.

Fixtures are real names rather than `"x" * 29`: one exactly at the old boundary, one over
it, one CJK, one carrying a combining mark, one emoji, and a short ASCII control. The
control is what says a failure is about the awkward name rather than about the table.
"""

from __future__ import annotations

import io
import os
import unittest
from contextlib import redirect_stderr, redirect_stdout
from unittest import mock

from charter import commands_persona, config, contain, tui
from tests._isolation import PersonaIso

#: The status every fixture persona below lands on: no memories, and no dispatch log at
#: all, so `status` stays the memory-derived "dormant" for all of them. That the whole
#: tail is IDENTICAL across rows is what lets a single offset probe stand for the sum of
#: every width the renderer computed.
DORMANT = "✗ dormant"

#: `persona list`'s last column for a persona with no vault — the same probe #472 uses,
#: measured in cells here rather than in characters.
NO_VAULT = "no vault"


class Args:
    name = None
    recent_days = 14


class RosterWidths(PersonaIso):
    """Names chosen so each one fails a DIFFERENT half of the defect."""

    #: `len` and `tui.width` disagree on all but the first three.
    #:
    #: Two of these are CONTROLS and are expected to pass with or without the fix, which
    #: is stated here because a case that cannot fail is worth nothing unless the reader
    #: knows it is a control. `short ascii control` is one. So is the 28-char name: it
    #: fills `{:<28}` exactly, so the old renderer aligned it correctly — the boundary was
    #: never the broken case, ONE OVER it was, and having both is what says so.
    #:
    #: No name ends in `---`. That is not cosmetic: a persona whose name ends in a triple
    #: hyphen closes its own frontmatter fence early, so `persona.load` returns a partial
    #: parse and the ROLE column comes back empty — a real defect, reported separately,
    #: and one that would quietly make this fixture assert less than it looks like it does.
    NAMES = {
        "short ascii control": "ok",
        "exactly the old 28-char boundary": "persona-with-twentyeight-abc",
        "one character over the boundary": "persona-with-twentynine-abcde",
        "well over the boundary": "a-persona-name-far-past-any-fixed-column-width",
        "CJK — two cells per glyph": "日本語のペルソナ",
        "combining mark — zero cells": "équipe-mémoire",
        "emoji — two cells, one character": "🚀-launcher",
    }

    def setUp(self) -> None:
        super().setUp()
        # Pinned rather than inherited: `$COLUMNS` is read by `tui` and has flipped tests
        # in this suite before (#544). Nothing under test reads it today — the roster
        # tables are deliberately not clipped to the terminal — and pinning it is what
        # keeps that true by accident-detection rather than by hope.
        self.enterContext(mock.patch.dict(os.environ, {"COLUMNS": "200"}))

    def _roster(self, *names: str) -> list[str]:
        """Replace the roster with exactly *names* and return the ones the filesystem
        actually kept — a name is a directory, and not every filesystem takes every one."""
        import shutil

        for sub in config.PERSONAS_DIR.iterdir():
            shutil.rmtree(sub) if sub.is_dir() else sub.unlink()
        kept = []
        for n in names:
            try:
                self.make_persona(n, role="Role")
            except OSError:
                continue
            kept.append(n)
        if len(kept) < 2:
            self.skipTest("filesystem refuses these names")
        return kept

    @staticmethod
    def _run(fn) -> list[str]:
        out = io.StringIO()
        with redirect_stdout(out), redirect_stderr(io.StringIO()):
            fn(Args())
        return out.getvalue().splitlines()

    @staticmethod
    def _cells_before(line: str, marker: str) -> int:
        """How many terminal CELLS precede *marker* on *line*.

        Not `line.index(marker)`. That is a character count, and a character count is the
        bug: it reports the CJK row as starting its tail in the same place as the ASCII
        one while the terminal draws it eight columns further right.
        """
        return tui.width(line[:line.index(marker)])

    def _assert_aligned(self, rows: list[str], header: str, head_col: str,
                        marker: str) -> None:
        """Every row puts *marker* where *header* puts *head_col*."""
        want = self._cells_before(header, head_col)
        data = [r for r in rows if marker in r]
        self.assertTrue(data, "no data rows to check:\n" + "\n".join(rows))
        for r in data:
            self.assertEqual(
                self._cells_before(r, marker), want,
                f"this row's last column starts at cell "
                f"{self._cells_before(r, marker)}, the header's at {want} — the column "
                f"was sized by a guess, or measured in characters:\n" + "\n".join(rows))


class TestPersonaStatsColumnsLineUp(RosterWidths):
    def test_every_name_puts_the_status_column_where_the_header_does(self):
        """One case per name, so a failure says WHICH kind of name broke the table."""
        for label, name in self.NAMES.items():
            with self.subTest(name=label):
                kept = self._roster("ok", name)
                if name not in kept:
                    self.skipTest(f"filesystem refuses {name!r}")
                rows = self._run(commands_persona.cmd_persona_stats)
                header = next(r for r in rows if r.startswith("PERSONA"))
                self._assert_aligned(rows, header, "STATUS", DORMANT)

    def test_the_whole_awkward_roster_lines_up_at_once(self):
        """Together, not one at a time: the column is sized from the widest name in the
        table, so the interesting row is the one that is NOT the widest and still has to
        be padded to a width somebody else's name decided."""
        self._roster(*self.NAMES.values())
        rows = self._run(commands_persona.cmd_persona_stats)
        header = next(r for r in rows if r.startswith("PERSONA"))
        self._assert_aligned(rows, header, "STATUS", DORMANT)

    def test_every_name_is_still_readable_off_its_row(self):
        """Sizing the column, not clipping to it — and the half the alignment cases miss.

        A column padded with `tui.pad` stays aligned however badly it was measured,
        because `pad` TRUNCATES whatever does not fit. That makes alignment the wrong
        probe for the *measuring*: put the old constant back and the columns still line
        up, with the name quietly cut off inside one of them. Measured, not reasoned —
        it is what the hand-check for this change actually found. So this asks the other
        question, whether the value is still there, and that is what the measuring is for.

        #472 asks this report to name each persona in its bounded spelling, because the
        steward reading it acts on that name: `persona show`, `persona retire`. A prefix
        is not a name they can look up.
        """
        kept = self._roster(*self.NAMES.values())
        rows = self._run(commands_persona.cmd_persona_stats)
        for name in kept:
            with self.subTest(name=name):
                self.assertTrue(
                    any(contain.one_line(name) in r for r in rows),
                    f"{name!r} appears in full on no row — the column was sized by a "
                    f"guess and the name was cut to fit it:\n" + "\n".join(rows))

    def test_a_wide_number_is_still_readable_off_its_row(self):
        """The same question for the numeric columns, and the same reason. A DISP tally
        clipped to `1234…` reports a number nobody can act on, and every row would go on
        lining up perfectly while it happened."""
        self._roster("ok", "other")
        from charter import dispatch

        with mock.patch.object(dispatch, "tally",
                               return_value={"ok": 1234567, "other": 2}):
            rows = self._run(commands_persona.cmd_persona_stats)
        self.assertTrue(any("1234567" in r for r in rows),
                        "the dispatch tally was cut to fit a column sized by a guess:\n"
                        + "\n".join(rows))

    def test_a_committed_separator_still_lines_up(self):
        """`contain.one_line` GROWS the name, and the growth happens before the measure.

        This is #472's fixture asked #508's question: measuring the raw name and printing
        the rendered one misaligns every row in the table, and it is what a fix that
        bounds the value at the `print` produces.

        A REAL separator, not a run of spaces. `one_line` leaves spaces alone, so a
        space-padded fixture renders to itself, raw and bounded are the same string,
        and the case passes under exactly the mutation it exists to catch — measured,
        and it did. U+2028 becomes a six-character escape, which is the growth the
        measure has to happen after. Built with `chr` so the fixture is one
        unambiguous codepoint rather than an escape a later editor might normalise.
        """
        name = "evil" + chr(0x2028) + "  fake     Fake role"
        try:
            (config.PERSONAS_DIR / name).mkdir()
            (config.PERSONAS_DIR / name).rmdir()
        except OSError:
            self.skipTest(f"filesystem refuses {name!r}")
        self.assertNotEqual(
            contain.one_line(name), name,
            "fixture: this name must GROW when bounded, or the case asserts "
            "nothing about measure-versus-print order")
        self._roster("ok", name)
        rows = self._run(commands_persona.cmd_persona_stats)
        header = next(r for r in rows if r.startswith("PERSONA"))
        self.assertTrue(any(contain.one_line(name) in r for r in rows), rows)
        self._assert_aligned(rows, header, "STATUS", DORMANT)


class TestTheSkillsBlockIsMeasuredToo(RosterWidths):
    """The second block the same command prints, and it carried the same constant.

    `{shown:<26}` under `SKILLS — declared vs actually invoked`. It is a name column with
    no header of its own, so it looks less like a table than the one above and misaligns
    in exactly the same way: one drifted persona with a long name and the `unused:` label
    on its row no longer lines up with the label on any other.

    Written because the hand-check found nothing covering this block at all — the
    constant could be put straight back and the whole suite stayed green.
    """

    #: Declared but never invoked, so `skilluse.drift` reports `unused` and the block
    #: renders. `make_persona` writes frontmatter, and `skills:` is comma-separated.
    SKILL = "some-declared-skill"

    def _drifting(self, *names: str) -> list[str]:
        import shutil

        for sub in config.PERSONAS_DIR.iterdir():
            shutil.rmtree(sub) if sub.is_dir() else sub.unlink()
        for n in names:
            self.make_persona(n, role="Role", skills=self.SKILL)
        return self._run(commands_persona.cmd_persona_stats)

    def test_the_labels_line_up_across_drifted_personas(self):
        rows = self._drifting("ok", self.NAMES["well over the boundary"])
        block = [r for r in rows if "unused:" in r]
        self.assertEqual(len(block), 2, "\n".join(rows))
        at = {tui.width(r[:r.index("unused:")]) for r in block}
        self.assertEqual(
            len(at), 1,
            "the `unused:` label starts in a different column on each row — the name "
            f"column was sized by a guess (offsets {sorted(at)}):\n" + "\n".join(block))

    def test_a_long_name_is_not_cut_out_of_the_block(self):
        """`tui.pad` truncates, so alignment alone would survive a constant here too."""
        long = self.NAMES["well over the boundary"]
        rows = self._drifting("ok", long)
        self.assertTrue(any(long in r and "unused:" in r for r in rows),
                        "the drifted persona is not named in full:\n" + "\n".join(rows))


class TestPersonaListColumnsLineUp(RosterWidths):
    """The sibling table. It measured its columns and still had half the defect: `len`
    over the bounded names, so every non-ASCII name shifted its own row."""

    def test_every_name_puts_the_last_column_where_the_header_does(self):
        for label, name in self.NAMES.items():
            with self.subTest(name=label):
                kept = self._roster("ok", name)
                if name not in kept:
                    self.skipTest(f"filesystem refuses {name!r}")
                rows = self._run(commands_persona.cmd_persona_list)
                header = next(r for r in rows if "VAULT STATUS" in r)
                self._assert_aligned(rows, header, "VAULT STATUS", NO_VAULT)

    def test_the_whole_awkward_roster_lines_up_at_once(self):
        self._roster(*self.NAMES.values())
        rows = self._run(commands_persona.cmd_persona_list)
        header = next(r for r in rows if "VAULT STATUS" in r)
        self._assert_aligned(rows, header, "VAULT STATUS", NO_VAULT)

    def test_every_name_is_still_readable_off_its_row(self):
        """The half alignment cannot see, and the one that caught `len` here.

        `tui.pad` truncates, so once the rows are padded in cells they line up however
        the column was measured — a `len` measurement merely makes the column too NARROW,
        and the name is cut rather than pushed. Every alignment case in this class passes
        with `len` restored; this one does not. It is the difference between a table that
        looks right and a report a steward can act on.

        One name at a time on purpose: `len` under-measures only when the widest-in-cells
        name is not also the widest-in-characters, so a roster containing one long ASCII
        name hides the defect for every other name in it.
        """
        for label, name in self.NAMES.items():
            with self.subTest(name=label):
                kept = self._roster("ok", name)
                if name not in kept:
                    self.skipTest(f"filesystem refuses {name!r}")
                rows = self._run(commands_persona.cmd_persona_list)
                self.assertTrue(
                    any(contain.one_line(name) in r for r in rows),
                    f"{name!r} appears in full on no row — the PERSONA column was "
                    f"measured in characters, so it is too narrow for this name and "
                    f"`tui.pad` cut it:\n" + "\n".join(rows))


class TestEveryColumnIsMeasuredNotOnlyTheName(RosterWidths):
    """PERSONA was the column the issue named; it was not the only one holding a guess.

    MEM, RECENT, VERIFY, DUP and DISP were `{:>5}`/`{:>8}`/`{:>6}` — the same shape with a
    threshold high enough that nobody had reached it. A seven-digit dispatch tally pushes
    STATUS on its row exactly the way a 29-character name pushed the other six, and
    "nobody has that many yet" is a fact about today's data rather than a property of the
    renderer.

    **Driven through the real command, with the tally stubbed rather than the table called
    directly.** Calling the new helper would make this pass on the branch and *error* on
    an unfixed tree for want of a symbol — which looks like a red and demonstrates
    nothing about the defect. Stubbing `dispatch.tally` runs the same `cmd_persona_stats`
    either way, so the case fails on the old renderer for the reason it names.
    """

    def _stats_with_tally(self, tally: dict) -> list[str]:
        from charter import dispatch

        with mock.patch.object(dispatch, "tally", return_value=tally):
            return self._run(commands_persona.cmd_persona_stats)

    def test_a_wide_dispatch_tally_moves_every_row_together(self):
        self._roster("ok", "other")
        rows = self._stats_with_tally({"ok": 1234567, "other": 2})
        header = next(r for r in rows if r.startswith("PERSONA"))
        self._assert_aligned(rows, header, "STATUS", DORMANT)

    def test_a_wide_name_and_a_wide_number_at_once(self):
        """Both guesses in one table — the case where fixing only the column the issue
        named looks like a fix and still leaves the report misaligned."""
        cjk = self.NAMES["CJK — two cells per glyph"]
        kept = self._roster("ok", cjk)
        if cjk not in kept:
            self.skipTest(f"filesystem refuses {cjk!r}")
        rows = self._stats_with_tally({"ok": 1234567, cjk: 2})
        header = next(r for r in rows if r.startswith("PERSONA"))
        self._assert_aligned(rows, header, "STATUS", DORMANT)


class TestTuiColumn(unittest.TestCase):
    """The measurement itself, at the level a table cannot reach: `tui.column` is where
    "how wide does this column have to be" is decided, and it decides in cells."""

    def test_it_measures_cells_not_characters(self):
        """The whole reason a fixed constant could not simply be widened."""
        self.assertEqual(tui.column("", ["日本語"], gap=0), 6)
        self.assertNotEqual(tui.column("", ["日本語"], gap=0), len("日本語"))

    def test_a_combining_mark_costs_nothing(self):
        self.assertEqual(tui.column("", ["é"], gap=0), 1)

    def test_the_header_is_a_floor(self):
        self.assertEqual(tui.column("PERSONA", ["ok"], gap=0), len("PERSONA"))

    def test_the_widest_cell_wins_over_the_header(self):
        self.assertEqual(tui.column("MEM", ["1", "123456"], gap=0), 6)

    def test_no_cells_still_fits_the_header(self):
        self.assertEqual(tui.column("VAULT", [], gap=0), 5)

    def test_the_gap_is_inside_the_returned_width(self):
        self.assertEqual(tui.column("ok", [], gap=2), 4)

    def test_a_cap_bounds_the_content_and_not_the_gap(self):
        self.assertEqual(tui.column("", ["x" * 100], gap=2, cap=38), 40)

    def test_a_cap_does_not_pad_short_content_out_to_it(self):
        self.assertEqual(tui.column("", ["xx"], gap=0, cap=38), 2)

    def test_padding_to_the_measured_width_produces_equal_cells(self):
        """The pair has to agree or the arithmetic was for nothing: `column` measures and
        `pad` pads, and every cell padded to a computed width must render to it."""
        cells = ["ok", "日本語のペルソナ", "équipe", "🚀-launcher", "x" * 29]
        w = tui.column("PERSONA", cells)
        self.assertEqual({tui.width(tui.pad(c, w)) for c in cells}, {w})


if __name__ == "__main__":
    unittest.main()
