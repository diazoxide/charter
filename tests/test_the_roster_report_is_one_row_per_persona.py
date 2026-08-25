"""A persona DIRECTORY name is a committed value, and the roster reports print it (#472).

`persona.list_personas()` globs ``personas/*/``, asks only for a leading underscore and a
``persona.md``, and never asks `valid_name`. A filesystem forbids ``/`` and NUL and
nothing else, so a commit can add a directory whose name holds a line separator —
``evil\\u2028  fake     Fake role   vault2`` — and `charter persona list` then prints two
table rows for one persona on disk, the second one entirely attacker-chosen and wearing
charter's own column layout. `charter persona stats` does the same one command over.

This is #453's mechanism (a committed value crossing into a format that has structure,
without being escaped for it) on the two surfaces that report the roster. It is not a
privilege boundary — nothing is executed off the back of a table row — but a forged row
can name a persona that does not exist, or hide one that does, in the report a steward
prunes from.

**The property is "one physical line per persona", and it is measured, not counted into
this file.** Every case renders the same roster twice — once with a separator in the name,
once with a benign name of the same shape — and compares the two reports. A hard-coded row
count would also be satisfied by a report that printed nothing at all.

**The second property is that the columns still line up**, and it is the half a bound at
the `print` alone does not give you. Both renderers size their columns from the names
(`nw = max(len(n) for n in names) + 2`), so a bound applied only where the row is printed
measures a string longer than the one it prints — every column after PERSONA shifts, for
every persona in the table, whenever one committed name holds a separator. `one_line`
*grows* a name (a separator becomes a four-character escape), so this is not hypothetical:
it is what the fix does if the bound lands on the wrong side of the arithmetic. The
alignment case below asserts the last column of each data row begins where the header's
does — the last one because its offset is the sum of every width the renderer computed, so
one probe catches a mis-measure in any of them.

**The active marker compares identity, and the rows render a bound.** Those are different
questions and this file pins them apart: two personas whose names differ only in
characters `one_line` escapes must not both come out marked active, and the marked one
must be the one that is actually selected.

The next spelling: a roster surface this file does not drive. `list` and `stats` are the
two named in #472; `lint` was bounded with #453 and is pinned in
`test_a_server_name_cannot_declare_a_server.py`. Anything else that turns
`list_personas()` into a table — a future `persona tree`, a JSON emitter — needs the same
treatment, and `SEPARATORS` here is deliberately not the bound: the bound is
`contain.one_line`, whose codespace-wide sweep lives in that same module.
"""

from __future__ import annotations

import io
import os
import re
import unittest
from contextlib import redirect_stderr, redirect_stdout
from unittest import mock

from charter import commands_persona, config, contain, persona
from tests._isolation import PersonaIso

#: Every spelling of "this line ends here" that `str.splitlines` honours, beyond the
#: `\r`/`\n` pair everyone thinks of. Written as escapes, never as the literal character:
#: a raw U+2028 in a fixture is invisible in the editor of whoever reads this next. NOT the
#: bound — `contain.one_line` is, and it is asked of the whole codespace elsewhere. This
#: exists so a failure names which spelling failed.
SEPARATORS = {
    "LF": "\n",
    "CR": "\r",
    "CRLF": "\r\n",
    "U+2028 LINE SEPARATOR": "\u2028",
    "U+2029 PARAGRAPH SEPARATOR": "\u2029",
    "U+0085 NEL": "\x85",
    "U+000B VERTICAL TAB": "\v",
    "U+000C FORM FEED": "\f",
    "U+001C FILE SEPARATOR": "\x1c",
}

#: The row the payload forges: charter's own column layout, a persona that does not exist,
#: and a vault it does not have.
FORGED_ROW = "  fake     Fake role   vault2"


class Args:
    """`cmd_persona_list` and `cmd_persona_stats` read attributes off argparse's namespace
    with `getattr(..., default)`; this is the shape they both accept."""

    name = None
    recent_days = 14


class RosterBase(PersonaIso):
    def _roster(self, *names: str) -> None:
        """Replace the roster with exactly *names*, each an ordinary loadable persona.

        Ordinary on purpose: a persona that fails to load takes a different path through
        both commands, and the defect here is on the path where everything is fine.
        """
        import shutil

        for sub in config.PERSONAS_DIR.iterdir():
            shutil.rmtree(sub) if sub.is_dir() else sub.unlink()
        for i, n in enumerate(names):
            self.make_persona(n, role=f"Role{i}")

    @staticmethod
    def _run(fn) -> list[str]:
        """The physical lines *fn* writes to stdout."""
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            fn(Args())
        return out.getvalue().splitlines()

    def _list(self) -> list[str]:
        return self._run(commands_persona.cmd_persona_list)

    def _stats(self) -> list[str]:
        return self._run(commands_persona.cmd_persona_stats)

    def _skip_unless_nameable(self, name: str) -> None:
        """Some filesystems refuse some of these names. Say so rather than passing."""
        try:
            (config.PERSONAS_DIR / name).mkdir()
        except OSError:
            self.skipTest(f"filesystem refuses the name {name!r}")
        (config.PERSONAS_DIR / name).rmdir()


class TestPersonaListGetsOneRowPerPersona(RosterBase):
    def test_a_separator_in_a_directory_name_adds_no_row(self):
        self._roster("good", f"evil{FORGED_ROW}")
        benign = self._list()
        for label, sep in SEPARATORS.items():
            with self.subTest(separator=label):
                name = f"evil{sep}{FORGED_ROW}"
                self._skip_unless_nameable(name)
                self._roster("good", name)
                rows = self._list()
                self.assertEqual(
                    len(rows), len(benign),
                    f"{label} in a committed directory name added a physical line to "
                    f"`charter persona list`:\n" + "\n".join(rows))

    def test_the_row_still_names_the_persona_in_its_bounded_spelling(self):
        """Suppressing the name would satisfy the row count and tell the steward nothing.
        Asserting the *bounded* spelling is also what says the count held because the name
        was bounded, rather than because some other guard dropped the persona."""
        for label, sep in SEPARATORS.items():
            with self.subTest(separator=label):
                name = f"evil{sep}{FORGED_ROW}"
                self._skip_unless_nameable(name)
                self._roster("good", name)
                rows = self._list()
                self.assertTrue(
                    any(contain.one_line(name) in r for r in rows),
                    f"the persona is not named in its bounded spelling:\n"
                    + "\n".join(rows))

    def test_the_columns_line_up_for_every_persona(self):
        """The half a bound at the `print` alone does not give you: the widths are measured
        from the names, so they have to be measured from the *bounded* names."""
        for label, sep in SEPARATORS.items():
            with self.subTest(separator=label):
                name = f"evil{sep}{FORGED_ROW}"
                self._skip_unless_nameable(name)
                self._roster("good", name)
                rows = self._list()
                header = next(r for r in rows if "PERSONA" in r and "VAULT STATUS" in r)
                at = header.index("VAULT STATUS")
                # The last column, so its offset is the sum of every width this renderer
                # computed — one probe that catches a mis-measure in any of them. Both
                # personas carry no vault, so both rows end in the same word.
                data = [r for r in rows if "no vault" in r]
                self.assertEqual(len(data), 2, rows)
                for r in data:
                    self.assertEqual(
                        r.index("no vault"), at,
                        f"the VAULT STATUS column of this row starts at "
                        f"{r.index('no vault')}, the header's at {at} — the widths were "
                        f"measured from a string other than the one printed:\n"
                        + "\n".join(rows))

    def test_the_committed_rung_of_the_active_pointer_refuses_a_separator(self):
        """Where the committed half of `resolve_active()` actually stands, asserted rather
        than assumed: `personas/.default` and `charter.toml`'s `[persona] default` both go
        through `persona.reference_ok`, which is `valid_name` — so neither can carry a
        separator into the line above the table. Written down here because the next reader
        of this file will otherwise reach for the same fixture I did."""
        name = "evil\u2028  and another line"
        self._skip_unless_nameable(name)
        self._roster("good", name)
        (config.PERSONAS_DIR / ".default").write_text(name)
        self.assertIsNone(persona.resolve_active(),
                          "the committed default rung stopped refusing a hostile name")

    def test_the_active_line_stays_one_line(self):
        """The rungs that are NOT gated — `$CHARTER_PERSONA`, `--persona`, the local
        pointer — are the operator's own, so this is depth rather than a boundary. It is
        one line above a table of one-line rows, and it costs a `one_line` call to keep it
        that way."""
        for label, sep in SEPARATORS.items():
            with self.subTest(separator=label):
                name = f"evil{sep}  and another line"
                self._roster("good")
                with mock.patch.dict(os.environ, {"CHARTER_PERSONA": name}):
                    rows = self._list()
                head = [r for r in rows if r.startswith("Active persona:")]
                self.assertEqual(len(head), 1, rows)
                self.assertIn(contain.one_line(name), head[0])
                self.assertEqual([r for r in rows if "and another line" in r], head, rows)


class TestTheActiveMarkerIsIdentityNotDisplay(RosterBase):
    """Bounding is a display transform: two different names can share one bounded form.

    `one_line` maps ``evil\\n`` and ``evil\\u2028`` — and ``evil\\x0a`` typed literally —
    onto the same rendered string. Deciding the marker from that rendered string marks
    every one of them active as soon as one of them is, which is a report that lies about
    which persona a dispatch will use. The marker asks the raw names.
    """

    #: Two DIFFERENT directory names with ONE rendered form: `one_line` escapes the
    #: separator in the first to the literal characters that spell the second. Written this
    #: way rather than as two separator-bearing names \u2014 those render to *different*
    #: escapes, so a roster of them would be marked correctly by a rendered-form comparison
    #: too, and the case would pass under the mutation it exists to catch.
    TWINS = ("evil\n", r"evil\x0a")

    def test_only_the_selected_persona_is_marked(self):
        for n in self.TWINS:
            self._skip_unless_nameable(n)
        self.assertEqual(len({contain.one_line(n) for n in self.TWINS}), 1,
                         "fixture: these must share one rendered form or the case is "
                         "asserting nothing about display-versus-identity")
        self._roster(*self.TWINS)
        persona.set_active(self.TWINS[1])
        self.assertEqual(persona.resolve_active(), self.TWINS[1],
                         "fixture: exactly one of the two is selected")
        rows = [r for r in self._list() if r.startswith("* ")]
        self.assertEqual(
            len(rows), 1,
            "both twins are marked active, so the marker is comparing what is DISPLAYED. "
            "One of these two is what a dispatch resolves to and the other is not:\n"
            + "\n".join(self._list()))
        self.assertIn(contain.one_line(self.TWINS[1]), rows[0])

    def test_the_marked_row_is_the_one_that_is_selected(self):
        """And it is the persona charter would actually resolve, not merely *a* row."""
        self._roster("evil\u2028fake", "evil")
        persona.set_active("evil\u2028fake")
        self.assertEqual(persona.resolve_active(), "evil\u2028fake")
        rows = [r for r in self._list() if r.startswith("* ")]
        self.assertEqual(len(rows), 1, rows)
        self.assertIn(contain.one_line("evil\u2028fake"), rows[0])


class TestPersonaStatsGetsOneRowPerPersona(RosterBase):
    def test_a_separator_in_a_directory_name_adds_no_row(self):
        self._roster("good", f"evil{FORGED_ROW}")
        benign = self._stats()
        for label, sep in SEPARATORS.items():
            with self.subTest(separator=label):
                name = f"evil{sep}{FORGED_ROW}"
                self._skip_unless_nameable(name)
                self._roster("good", name)
                rows = self._stats()
                self.assertEqual(
                    len(rows), len(benign),
                    f"{label} in a committed directory name added a physical line to "
                    f"`charter persona stats`:\n" + "\n".join(rows))

    def test_the_row_still_names_the_persona_in_its_bounded_spelling(self):
        for label, sep in SEPARATORS.items():
            with self.subTest(separator=label):
                name = f"evil{sep}{FORGED_ROW}"
                self._skip_unless_nameable(name)
                self._roster("good", name)
                rows = self._stats()
                self.assertTrue(
                    any(contain.one_line(name) in r for r in rows),
                    f"the persona is not named in its bounded spelling:\n"
                    + "\n".join(rows))

    def test_the_columns_of_the_hostile_row_come_from_the_real_name(self):
        """A bound is a display transform and must not become the lookup key.

        The DISP column is `disp.get(r["persona"])` against the committed dispatch log,
        keyed by the name as it is on disk. Bounding the roster once at the top \u2014 the
        obvious over-correction \u2014 makes every such lookup miss, and the row then reports 0
        dispatches for a persona that has been dispatched, plus the "never dispatched"
        status charter retires personas on. A row-count-only test lets that straight
        through, which is why the count is not the only thing asserted here.
        """
        name = "evil\u2028fake"
        self._skip_unless_nameable(name)
        self._roster("good", name)
        from charter import dispatch

        self.assertIsNotNone(dispatch.record(name), "fixture: the dispatch was not recorded")
        self.assertEqual(dispatch.tally().get(name), 1, "fixture: the tally must see it")
        rows = [r for r in self._stats() if contain.one_line(name) in r]
        self.assertEqual(len(rows), 1, rows)
        self.assertNotIn("never dispatched", rows[0])
        # DISP is the last number before the status glyph.
        m = re.search(r"(\d+)\s+[●○✗⚑⬡◇·]\s", rows[0])
        self.assertIsNotNone(m, rows[0])
        self.assertEqual(m.group(1), "1",
                         f"the DISP column lost the dispatch this persona actually "
                         f"had:\n{rows[0]}")


class TestABenignRosterIsUnchanged(RosterBase):
    """A bound that mangles an ordinary roster gets turned off by the first person it
    annoys — and both reports are read by a human every day."""

    def test_ordinary_names_are_printed_verbatim(self):
        self._roster("steward", "release-manager", "db-admin")
        rows = self._list()
        for n in ("steward", "release-manager", "db-admin"):
            self.assertEqual(len([r for r in rows if n in r]), 1, rows)
        stats = self._stats()
        for n in ("steward", "release-manager", "db-admin"):
            self.assertEqual(len([r for r in stats if r.startswith(n)]), 1, stats)


if __name__ == "__main__":
    unittest.main()
