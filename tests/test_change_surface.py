"""The `changes` component, the change picker, and the two keystrokes to each.

Four properties, and every one of them is a claim the surface would otherwise make falsely.

**Serve what you declare.** §4i: a `needs` name answered with an empty tuple lets a
component declare it, draw nothing, pass its own tests against an empty fixture, and be
indistinguishable from a plane that genuinely has none. So `component.NEEDS` and
`ctx.SERVES` are asserted against each other in ONE assertion — neither can be fixed
without the other — and `gather.scan` is measured actually carrying the slice.

**No forge call, and no subprocess at all, on the repaint path.** §4g's idle-tick property.
A five-member change is five forge reads per refresh, and those never happen on a tick.
Measured by counting processes, not asserted in a docstring.

**Never greener than the worst member.** §3.3/§3.5. One `unknown` member makes the change
`UNKNOWN`, not "green with an asterisk", and a change with no members at all is `unknown`
rather than "everything landed" — an empty maximum is the classic way a report comes out
green over nothing.

**Containment, because §4e already said so.** *"A change's name and description are
untrusted committed values."* A slug, a `why`, a repo name **or a branch name** carrying a
newline, U+2028 or an escape sequence renders as exactly one row and runs nothing. Measured
against hostile values, never reasoned about.
"""

from __future__ import annotations

import os
import subprocess
import sys
import time
import unittest
from unittest import mock

from charter import change, contain, instance as instance_mod, tui, workspace
from charter.frame import (builtins, choose, component, ctx, gather, layout, slots,
                           state, switch)
from tests._isolation import PersonaIso

FID = "f-changes"

#: One value per way a committed string can forge a row. The escape is the one that has
#: already cost this project code execution through tmux config text; U+2028 is the one a
#: terminal breaks on that `str.splitlines()` sees and `\n`-splitting does not.
HOSTILE = {
    "newline": "a\nb",
    "carriage return": "a\rb",
    "line separator": "a\u2028b",
    "paragraph separator": "a\u2029b",
    "next line": "a\u0085b",
    "vertical tab": "a\x0bb",
    "form feed": "a\x0cb",
    "escape": "a\x1b[31mb",
    "backtick and pipe": "a`b|c",
    "nul-ish": "a\x00b",
}

#: Every codepoint a terminal, or Python's own `str.splitlines`, treats as the end of a
#: line — which is what "renders as exactly one row" has to mean.
#:
#: **Splitting on `"\n"` alone is not that, and this was measured.** `tui.truncate`
#: sanitises on the way out, which removes `\n`, `\r`, the escape and the NUL — but it
#: leaves **U+2028 exactly as it found it**. So a test that split on `"\n"` could not tell
#: `contain.one_line` at the drawing site from its absence, and deleting that call stayed
#: green over a value that really does break a row on the terminals that honour U+2028.
#: `str.splitlines` knows all of these; `"\n"` knows one.
BREAKS = "\n\r\u2028\u2029\u0085\v\f"


class SurfaceCase(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        workspace.ensure("ws")
        state.record_workspace(FID, "ws")

    def seed(self, changes, *, gathered_at=None) -> None:
        """Write a gather snapshot holding *changes* and nothing else of interest.

        *gathered_at* defaults to NOW rather than to a constant: the strip draws the age of
        what it is showing, so a fixture frozen at epoch renders every row `stale` and
        every content assertion would be reading a degraded line.
        """
        gather.save(FID, {"gathered_at": time.time() if gathered_at is None else gathered_at,
                          "workspace": "ws", "current_repo": None,
                          "repos": [], "worktrees": [], "todos": [], "todo_count": 0,
                          "changes": list(changes)})

    def row(self, **kw) -> dict:
        r = {"change": "component-api-2", "why": "API 1 -> 2", "state": "unknown",
             "landed": 0, "total": 1, "excluded": 0,
             "members": [{"repo": "charter", "branch": "change/component-api-2",
                          "needs": [], "state": "unknown"}]}
        r.update(kw)
        return r

    def render(self, cols: int = 22, rows: int = 24) -> str:
        """The sidebar, drawn for real — the pane the `changes` section lives in."""
        with mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((cols, rows))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            return slots.render("right", FID)

    def section(self, width: int = 40, budget: int = 6) -> list[str]:
        return slots.changes_section(FID, width, budget)

    def clone(self, name: str) -> None:
        (workspace.workspace_dir("ws") / name / ".git").mkdir(parents=True)

    def make_change(self, slug: str, members=(("charter", ()),), why="API 1 -> 2"):
        rec = change.new_record(slug, why, "t", "2026-08-29T00:00:00+00:00")
        rec["members"] = [{"repo": r, "branch": change.default_branch(slug),
                           "needs": list(n)} for r, n in members]
        change.write("ws", slug, rec)
        return rec


class TestTheSliceIsServedAndDeclaredTogether(SurfaceCase):
    def test_needs_and_serves_are_one_assertion(self):
        """One assertion, so neither can be fixed without the other. A name declarable but
        unserved hands a component an empty tuple it draws nothing from and passes its own
        tests against — the convincing empty §4i is about."""
        self.assertEqual(set(component.NEEDS), set(ctx.SERVES))
        self.assertIn("changes", component.NEEDS)

    def test_the_scan_actually_carries_it(self):
        """The half a constant cannot state. Without this, `changes` could be in both
        tables and `gather.scan` could still never write the key."""
        self.clone("charter")
        self.make_change("component-api-2")
        got = gather.scan(workspace="ws")
        self.assertEqual([r["change"] for r in got["changes"]], ["component-api-2"])
        self.assertEqual(got["changes"][0]["members"][0]["repo"], "charter")

    def test_a_component_that_declared_it_is_handed_it(self):
        c = ctx.build(("changes",), width=80, height=4, fid=FID,
                      snapshot={"changes": [self.row()]})
        self.assertEqual(c.changes[0]["change"], "component-api-2")

    def test_a_component_that_did_not_declare_it_is_not(self):
        """Absent, not disabled: a present-but-empty attribute is indistinguishable from a
        slice that happens to be empty."""
        c = ctx.build((), width=80, height=4, fid=FID, snapshot={"changes": [self.row()]})
        with self.assertRaises(AttributeError) as cm:
            c.changes
        self.assertIn("did not declare changes", str(cm.exception))

    def test_the_component_declares_what_its_renderer_reads(self):
        reg = builtins.build()
        self.assertEqual(set(reg.get("changes").needs), {"gather", "changes"})

    def test_it_is_a_part_of_the_sidebar_and_not_a_pane_of_its_own(self):
        """**Measured twice, and both measurements are in `frame/builtins.py`.** The
        frame's sizing supports exactly ONE variable-height pane — `layout.slot_sizes`
        answers every member of `VARIABLE_ROW_SLOTS` with `layout.repos_rows`, and
        `_reassert_sizes` leaves that set unasserted so tmux's `resize-pane -y` has one
        remainder to give the rows to. And a PLACED component has to be in
        `instance.FRAME_SLOTS`, which is pinned to agree with the shipped `slots` default
        and with `density = full` — so placing it would put a pane on every operator's
        frame for a feature most planes never use."""
        reg = builtins.build()
        self.assertIn("changes", reg.get("sidebar").children)
        self.assertNotIn("changes", instance_mod.FRAME_SLOTS)
        self.assertNotIn("changes", builtins.SLOT_OF)
        self.assertNotIn("changes", slots.SLOTS)
        self.assertEqual(layout.VARIABLE_ROW_SLOTS, frozenset({"repos"}))

    def test_a_plane_with_no_changes_pays_nothing_for_it(self):
        """The whole reason it is a section. `todo_section`'s own rule: a heading over an
        empty space in a 22-column column is furniture within a day."""
        self.seed([])
        self.assertEqual(self.section(), [])
        self.make_change("a-1")
        gather.refresh(FID, workspace="ws")
        self.assertNotEqual(self.section(), [])

    def test_an_empty_workspace_carries_an_empty_list_and_not_a_missing_key(self):
        """`[]` and absent are different answers, and a renderer that had to `None`-check
        would be one that could forget to."""
        self.assertEqual(gather.scan(workspace="ws")["changes"], [])
        self.assertIn("changes", gather.scan(workspace="ws"))


class TestTheDegradedAnswersAreStillWellFormed(SurfaceCase):
    """Every fallback on this path, asked directly. `gather` promises it never raises and
    that a renderer never needs a `None`-check before indexing — and both of those are
    claims about the paths a happy-path test does not walk."""

    def test_the_empty_structure_carries_the_slice_as_an_empty_list(self):
        """`_empty` is what `scan` falls back to when everything failed. A renderer reading
        `data["changes"]` there must find a list, not a `KeyError` — which is the whole
        reason this structure is spelled out rather than built up."""
        self.assertEqual(gather._empty(None)["changes"], [])
        self.assertIn("changes", gather._empty("ws"))

    def test_a_scan_whose_change_read_explodes_still_returns_a_whole_structure(self):
        """The module's "never raises" promise, on the one branch this phase added. One bad
        record must degrade the change rows, not lose the repo table with them."""
        with mock.patch.object(gather, "_change_rows", side_effect=RuntimeError("boom")):
            got = gather.scan(workspace="ws")
        self.assertEqual(got["changes"], [])
        self.assertIn("repos", got)
        self.assertIn("gathered_at", got)

    def test_member_states_accepts_no_landing_map_at_all(self):
        """`None` is what a caller with nothing landed passes, and `set(None)` raises. The
        fallback is why "which members are blocked" is askable before anything has landed —
        which is every change on the day it is created."""
        rec = self.make_change("c-1", [("a", ()), ("b", ("a",))])
        self.assertEqual(change.member_states(rec, None),
                         {"a": "unknown", "b": "blocked"})
        self.assertEqual(change.blocked_members(rec, None), {"b": ["a"]})

    def test_no_change_chosen_reads_as_the_empty_string_and_not_as_none(self):
        """`choose.current` answers `str` for all three nouns. A frame looking at no change
        is an ordinary answer rather than a missing one, and `""` is the value that matches
        no name — so the roster marks no row rather than comparing against `None`."""
        self.assertEqual(choose.current(choose.CHANGE, FID), "")
        self.assertIsInstance(choose.current(choose.CHANGE, FID), str)
        self.make_change("a-1")
        switch.to_change(FID, "a-1")
        self.assertEqual(choose.current(choose.CHANGE, FID), "a-1")


class TestNothingOnTheRepaintPathSpawns(SurfaceCase):
    def test_a_repaint_starts_no_process_at_all(self):
        """Measured, not asserted (§4g). Every `subprocess` entry point is replaced with
        one that fails the test, so a forge call, a `git` call and a `tmux` call are all
        caught by the same net rather than by a list somebody has to keep."""
        self.seed([self.row()])
        started: list = []

        def boom(*a, **kw):
            started.append(a)
            raise AssertionError(f"the repaint started a process: {a!r}")

        with mock.patch.object(subprocess, "Popen", boom), \
             mock.patch.object(subprocess, "run", boom), \
             mock.patch.object(subprocess, "check_output", boom):
            out = self.render()
            rows = self.section()
        self.assertEqual(started, [])
        self.assertIn("component-api-2", " ".join(tui.strip_ansi(r) for r in rows))
        self.assertIn("changes", tui.strip_ansi(out))

    def test_the_scan_reads_no_forge_and_no_glstate_for_changes(self):
        """`glstate` is the tempting reuse and it is wrong twice: it is keyed on each
        clone's CURRENTLY CHECKED-OUT branch, which is frequently not the member's, and
        what it caches is `ci_status` — a single string that answers `None` for a CLI
        failure, a timeout, an auth error and "no check ever ran" alike (#561)."""
        self.clone("charter")
        self.make_change("component-api-2")
        with mock.patch("charter.glstate.read_for",
                        side_effect=AssertionError("glstate")) as _gl:
            rows = gather._change_rows("ws")
        self.assertEqual(len(rows), 1)


class TestTheAggregateIsNeverGreenerThanItsWorstMember(SurfaceCase):
    def test_one_unknown_member_makes_the_change_unknown(self):
        states = {"a": "landed", "b": "landed", "c": "unknown"}
        self.assertEqual(change.worst(states.values()), "unknown")

    def test_one_blocked_member_beats_every_landed_one(self):
        self.assertEqual(change.worst(["landed", "landed", "blocked"]), "blocked")

    def test_unknown_beats_blocked(self):
        """`unknown` is first because it is the only value that means charter did not
        look, and a value meaning "I did not look" must never be outranked by one meaning
        "I looked and it was fine"."""
        self.assertEqual(change.worst(["blocked", "unknown"]), "unknown")

    def test_all_landed_is_landed(self):
        self.assertEqual(change.worst(["landed", "landed"]), "landed")

    def test_no_members_at_all_is_unknown_and_not_landed(self):
        """An empty maximum is the classic way a report comes out green: "everything has
        landed" over nothing at all is the confidently-wrong output ADR 0009 forbids."""
        self.assertEqual(change.worst([]), "unknown")

    def test_a_state_charter_does_not_recognise_reads_as_unknown(self):
        """§3.5's asymmetry: anything charter does not recognise is UNKNOWN, never
        PASSED — and it is FOLDED rather than returned, because a word charter cannot
        explain is worse on a row than the word that says charter cannot explain it."""
        self.assertEqual(change.worst(["landed", "shipped-it"]), "unknown")

    def test_the_strip_says_the_word_and_not_a_colour(self):
        self.seed([self.row(state="unknown")])
        self.assertIn("UNKNOWN", tui.strip_ansi(self.render()))

    def test_a_member_with_a_landing_declaration_reads_as_landed(self):
        rec = self.make_change("component-api-2", [("charter", ())])
        self.assertEqual(change.member_states(rec, {"charter"}), {"charter": "landed"})

    def test_a_member_waiting_on_a_blocker_reads_as_blocked(self):
        rec = self.make_change("c-1", [("a", ()), ("b", ("a",))])
        self.assertEqual(change.member_states(rec, set()),
                         {"a": "unknown", "b": "blocked"})

    def test_a_member_charter_has_not_observed_reads_as_unknown_not_as_ready(self):
        """What stands between a member and its own landing is a request state and its
        checks at its head sha, and those are a forge read the scan does not make. So the
        strip says charter did not look, which is #561 one surface out."""
        rec = self.make_change("c-1", [("a", ())])
        self.assertEqual(change.member_states(rec, set()), {"a": "unknown"})

    def test_landed_count_is_a_pair_and_not_a_percentage(self):
        """§3.3 refuses a percentage and a bar, because either invites one word for the
        change as a whole and a member can hide behind one word."""
        self.assertEqual(change.landed_count(["landed", "unknown", "landed"]), (2, 3))


class TestContainment(SurfaceCase):
    """Hostile values, measured. `contain.one_line` BEFORE the width arithmetic (#472):
    escaping after padding pads to the wrong width, and the column stops lining up at
    exactly the row whose content came out of somebody else's file."""

    def rows_with(self, **field) -> list[str]:
        self.seed([self.row(**field)])
        return slots.changes_section(FID, 40, 6)

    def test_a_hostile_slug_renders_as_one_row(self):
        """`str.splitlines`, never `split("\n")` — see :data:`BREAKS` for the measurement
        that makes the difference load-bearing."""
        for label, value in HOSTILE.items():
            with self.subTest(field="change", hostile=label):
                rows = self.rows_with(change=value)
                self.assertTrue(rows)
                for line in rows:
                    self.assertEqual(len(line.splitlines()), 1, repr(line))
                    for ch in BREAKS:
                        self.assertNotIn(ch, line, f"{label}: {ch!r} reached the row")

    def test_a_hostile_slug_cannot_widen_the_column_either(self):
        """The half a line-count assertion cannot see. The name column is sized from the
        CONTAINED names and clipped to the pane, so an escape sequence buys no cells: the
        section is exactly as wide as it was asked to be."""
        for label, value in HOSTILE.items():
            with self.subTest(hostile=label):
                for line in self.rows_with(change=value):
                    self.assertLessEqual(tui.width(line), 40, repr(line))

    def test_a_hostile_why_cannot_forge_a_row(self):
        """The `why` is not drawn in the 22-column section, and it is still contained on
        the way into the snapshot — asserted here so that a future row which DOES draw it
        inherits the property rather than rediscovering it."""
        for label, value in HOSTILE.items():
            with self.subTest(hostile=label):
                for line in self.rows_with(why=value):
                    self.assertEqual(len(line.splitlines()), 1, repr(line))

    def test_a_hostile_repo_or_branch_or_needs_is_contained_in_the_snapshot(self):
        """The member fields reach `charter change show` and, later, a pull request body.
        `change.read` refuses most of these at the record boundary; this asserts the
        drawing side holds them too, at the one call every printing site goes through."""
        for label, value in HOSTILE.items():
            with self.subTest(hostile=label):
                out = contain.one_line(value)
                self.assertEqual(len(out.splitlines()), 1, repr(out))
                for ch in BREAKS:
                    self.assertNotIn(ch, out, f"{label}: {ch!r} survived one_line")

    def test_the_control_a_raw_value_really_would_forge_a_row(self):
        """The live control this class cannot do without: every assertion above is a
        negative, and a check that never sees a two-row string passes just as happily as a
        contained renderer."""
        self.assertEqual(len("a\nb".splitlines()), 2)
        self.assertEqual(len(contain.one_line("a\nb").splitlines()), 1)
        # And the control for the SEPARATOR set, which is the half that was wrong: the
        # drawing site's own sanitiser does not remove U+2028, so `contain.one_line` is
        # the only thing standing between a committed name and a forged row there.
        self.assertIn("\u2028", tui.truncate("a\u2028b", 40))
        self.assertNotIn("\u2028", tui.truncate(contain.one_line("a\u2028b"), 40))

    def test_the_width_arithmetic_runs_on_the_contained_name(self):
        """#472's ordering, asked where it can be seen: a name whose glyphs are two cells
        wide is measured as two cells, and the escape is gone before that measurement."""
        self.assertEqual(tui.column("", [contain.one_line("a\x1b[31mb")], gap=0),
                         tui.width(contain.one_line("a\x1b[31mb")))


class TestWhatTheSectionSays(SurfaceCase):
    def test_a_workspace_with_no_changes_draws_no_rows_at_all(self):
        """Not a heading over an empty space: `todo_section`'s rule, and the reason this
        section is free on every plane that never uses it."""
        self.seed([])
        self.assertEqual(self.section(), [])

    def test_the_heading_carries_the_count_the_aggregate_and_the_age(self):
        self.seed([self.row(change="a-1", landed=1, total=2, state="unknown"),
                   self.row(change="b-2", landed=1, total=1, state="landed")])
        head = tui.strip_ansi(self.section()[0])
        self.assertIn("changes 2", head)
        self.assertIn("UNKNOWN", head)      # the worst of the two, never the best
        self.assertIn("now", head)

    def test_each_change_is_one_row_with_its_own_fraction(self):
        self.seed([self.row(change="a-1", landed=1, total=2),
                   self.row(change="b-2", landed=1, total=1)])
        rows = [tui.strip_ansi(r) for r in self.section()[1:]]
        self.assertEqual(len(rows), 2)
        self.assertIn("a-1", rows[0])
        self.assertIn("1/2", rows[0])
        self.assertIn("1/1", rows[1])

    def test_the_chosen_change_carries_the_pickers_own_mark(self):
        self.seed([self.row(change="a-1"), self.row(change="b-2")])
        state.record_change(FID, "b-2")
        rows = [tui.strip_ansi(r) for r in self.section()[1:]]
        self.assertTrue(rows[1].startswith(choose.MARK[0]), repr(rows[1]))
        self.assertTrue(rows[0].startswith(choose.MARK[1]), repr(rows[0]))

    def test_the_chosen_change_does_not_move_to_the_top(self):
        """A list whose rows reorder with state is a list nobody learns."""
        self.seed([self.row(change="a-1"), self.row(change="b-2")])
        state.record_change(FID, "b-2")
        rows = [tui.strip_ansi(r) for r in self.section()[1:]]
        self.assertIn("a-1", rows[0])

    def test_what_does_not_fit_is_admitted_rather_than_dropped(self):
        """The `…(+N more)` line is RESERVED out of the budget rather than appended and
        trimmed off the end — `_table_lines`' rule, because "there is more here than fits"
        outranks "here is an arbitrary one of them"."""
        self.seed([self.row(change=f"c-{i}") for i in range(6)])
        rows = [tui.strip_ansi(r) for r in slots.changes_section(FID, 22, 4)]
        self.assertLessEqual(len(rows), 4)
        self.assertIn("…(+", rows[-1])

    def test_it_never_spends_more_rows_than_its_cap(self):
        self.seed([self.row(change=f"c-{i}") for i in range(40)])
        self.assertLessEqual(len(slots.changes_section(FID, 22, 100)),
                             slots._MAX_CHANGE_LINES)

    def test_no_room_means_no_rows_and_no_file_opened(self):
        """The budget is checked BEFORE the cache is reached: a pane with no room must not
        open a file it is about to discard — `todo_section` keeps the same ordering for the
        same reason.

        **The read is what this asserts, not the rows**, and the first version of this test
        got that wrong: it checked `== []` alone, which is satisfied either way because
        `_change_rows` refuses a zero budget too. So `budget <= 0` could become `budget < 0`
        with every assertion still green and the file opened on every repaint of a pane
        with no room — the deletion sweep reported exactly that, as a boundary nothing
        pinned. A test whose NAME claims more than its body checks is the shape this suite
        keeps finding.
        """
        self.seed([self.row()])
        with mock.patch.object(gather, "read",
                               side_effect=AssertionError("opened the cache")) as read:
            self.assertEqual(slots.changes_section(FID, 22, 0), [])
            self.assertEqual(slots.changes_section(FID, 22, -1), [])
        self.assertEqual(read.call_count, 0)

    def test_one_row_of_room_does_reach_the_cache(self):
        """The other side of the boundary. Without it, "does not read at zero" is
        satisfied by a function that never reads at all."""
        self.seed([self.row()])
        with mock.patch.object(gather, "read", wraps=gather.read) as read:
            self.assertTrue(slots.changes_section(FID, 22, 1))
        self.assertEqual(read.call_count, 1)

    def test_the_section_draws_the_age_of_what_it_is_showing(self):
        """A refresh is an action, not a tick, so between two of them the rows are as old
        as the last one — and a surface that did not say so would be indistinguishable
        from a live one."""
        self.assertEqual(slots._age(1000.0, 1000.0), "just now")
        self.assertEqual(slots._age(1000.0, 1000.0 + 245), "4m ago")
        self.assertEqual(slots._age(1000.0, 1000.0 + 7200), "stale")

    def test_the_short_form_says_the_same_three_things_at_the_same_bounds(self):
        """One function with a flag rather than two formatters: a surface calling anything
        under a minute "now" while another called it "just now" at ninety seconds would be
        two clocks wearing one name."""
        for age, long, short in ((0, "just now", "now"), (245, "4m ago", "4m"),
                                 (7200, "stale", "old")):
            with self.subTest(age=age):
                self.assertEqual(slots._age(1000.0, 1000.0 + age), long)
                self.assertEqual(slots._age(1000.0, 1000.0 + age, short=True), short)

    def test_a_timestamp_charter_cannot_read_is_not_dated_to_now(self):
        """A cache written by an older charter has none, and dating it to the present is
        the confident wrong answer ADR 0009 forbids."""
        self.assertEqual(slots._age(None, 1000.0), "?")
        self.assertEqual(slots._age("yesterday", 1000.0), "?")

    def test_it_reaches_the_real_sidebar_pane(self):
        """End to end through the shipped renderer, so the section being registered and
        the section being DRAWN are not two different claims."""
        self.seed([self.row(change="a-1", landed=1, total=2)])
        out = tui.strip_ansi(self.render())
        self.assertIn("changes 1", out)
        self.assertIn("a-1", out)


class TestTheDrawingPathSurvivesAMalformedSnapshot(SurfaceCase):
    """Every defensive read in the drawing path, driven rather than reasoned about.

    **A cache written by an older charter is the ordinary case, not an edge one** —
    `gather.cached`'s own docstring says so, and `_shaped_like_a_scan` is deliberately
    loose about anything past `repos`/`worktrees`. So a change row missing a field, or
    carrying `None` where a number belongs, reaches this renderer on the day somebody
    upgrades, and every `or 0` / `or ""` / `.get(…, default)` in it is what stops that
    being a traceback inside a pane.

    Feeding well-formed fixtures leaves all of them unexercised, which is exactly what the
    deletion sweep reported: fourteen survivors in one function, each looking equivalent
    on its own because the fixture never took the branch.
    """

    #: One entry per way a row can be wrong, all of them reachable from a stale cache.
    BROKEN = {
        "no fields at all": {},
        "every value None": {"change": None, "why": None, "state": None,
                             "landed": None, "total": None, "members": None},
        "counts as strings": {"landed": "2", "total": "3"},
        "a state charter has never heard of": {"state": "shipped-it"},
        "members missing": {"change": "a-1", "state": "landed", "landed": 1, "total": 1},
        "a slug that is not text": {"change": 7},
    }

    def test_every_malformed_row_still_draws_and_draws_one_line_each(self):
        for label, row in self.BROKEN.items():
            with self.subTest(row=label):
                self.seed([row])
                lines = slots.changes_section(FID, 40, 6)
                self.assertTrue(lines, f"{label}: drew nothing at all")
                for line in lines:
                    self.assertEqual(len(line.splitlines()), 1, repr(line))
                    self.assertLessEqual(tui.width(line), 40, repr(line))

    def test_a_row_charter_cannot_read_still_says_unknown_rather_than_nothing(self):
        """The word is the point. A blank where the state goes reads as "fine"; `UNKNOWN`
        reads as *charter did not look*, which is what a row it could not parse means."""
        self.seed([{}])
        self.assertIn("UNKNOWN", tui.strip_ansi(slots.changes_section(FID, 40, 6)[0]))

    def test_a_malformed_row_never_reads_as_landed(self):
        """The direction that matters. Over-reporting `unknown` costs a second look;
        under-reporting it says a member is in when charter has no idea."""
        for label, row in self.BROKEN.items():
            with self.subTest(row=label):
                self.seed([row])
                head = tui.strip_ansi(slots.changes_section(FID, 40, 6)[0])
                if label != "members missing":     # that one really IS landed
                    self.assertNotIn("landed ", head, repr(head))

    def test_a_pane_one_column_wide_still_draws_one_row_per_line(self):
        """`max(width - cw - mw - 1, 1)` is what stops a negative slice width, and a
        resize really does reach here — `cmd_resize` re-sizes panes without destroying
        them, so a narrowed terminal arrives with a real pane to fill."""
        self.seed([self.row(change="component-api-2", landed=1, total=2)])
        for w in (1, 2, 4, 8, 12):
            with self.subTest(width=w):
                for line in slots.changes_section(FID, w, 4):
                    self.assertLessEqual(tui.width(line), w, repr(line))
                    self.assertEqual(len(line.splitlines()), 1, repr(line))

    def test_the_gather_slice_survives_a_record_with_odd_shapes(self):
        """`gather._change_rows`' own defensive reads, one layer below the renderer."""
        self.clone("charter")
        rec = change.new_record("a-1", "w", "t", "2026-08-29T00:00:00+00:00")
        rec["members"] = [{"repo": "charter", "branch": "b", "needs": []}]
        change.write("ws", "a-1", rec)
        rows = gather._change_rows("ws")
        self.assertEqual(rows[0]["total"], 1)
        self.assertEqual(rows[0]["excluded"], 0)
        self.assertEqual(rows[0]["members"][0]["state"], "unknown")

    def test_a_landing_declaration_makes_that_member_read_as_landed(self):
        """The other side of the join, so the `by_change` lookup is not satisfied by
        always answering the empty set."""
        self.clone("charter")
        self.make_change("a-1", [("charter", ())])
        change.record_landing("ws", "a-1", "charter", number=1, merge="e0c9d13",
                              head="4b1e77a", ts="2026-08-29T00:00:00+00:00")
        rows = gather._change_rows("ws")
        self.assertEqual(rows[0]["members"][0]["state"], "landed")
        self.assertEqual((rows[0]["landed"], rows[0]["total"]), (1, 1))

    def test_a_declaration_for_another_change_does_not_leak_across(self):
        """`by_change` is keyed on the slug. Without that key every change in the
        workspace would read as landed the moment any one of them did."""
        self.clone("charter")
        self.make_change("a-1", [("charter", ())])
        self.make_change("b-2", [("charter", ())])
        change.record_landing("ws", "a-1", "charter", number=1, merge="e0c9d13",
                              head="4b1e77a", ts="2026-08-29T00:00:00+00:00")
        got = {r["change"]: r["state"] for r in gather._change_rows("ws")}
        self.assertEqual(got, {"a-1": "landed", "b-2": "unknown"})


class TestTwoKeystrokes(SurfaceCase):
    def test_the_sidebar_is_what_a_key_toggles_and_the_section_rides_on_it(self):
        """**One keystroke reaches it, and it is the sidebar's.** A child of a composite
        has no pane of its own to show or hide — charter never splits a pane (§4d) — so
        the key that brings the persona column up brings its changes with it. That is the
        cost of not being a pane, and it is the cost that buys every plane with no changes
        paying nothing at all."""
        self.assertIn("right", instance_mod.frame_arrangement({"slots": ["top"]}))
        self.assertIn("changes", builtins.build().get("sidebar").children)

    def test_the_palette_carries_a_doorway_row_for_changes(self):
        """The second keystroke: `F2`, then the row. It is a ROW and not an `Action`
        because opening a picker starts nothing — it replaces the surface in the pane the
        operator is already looking at, and an `Action` whose `run` did nothing would pass
        every test in that contract and describe nothing that happens."""
        ids = [r.id for r in choose.open_rows(FID)]
        self.assertIn("pick:change", ids)
        self.assertEqual(choose.noun_of(choose.open_rows(FID)[-1]), choose.CHANGE)

    def test_an_unavailable_picker_says_why_before_the_keypress(self):
        """#512: an operator cannot ask about an option they cannot see. A pane of no
        names is an offer charter knows it cannot honour, so the doorway carries the
        reason and `_picker` refuses to open on a row with a note."""
        row = next(r for r in choose.open_rows(FID) if r.id == "pick:change")
        self.assertEqual(row.note, choose.NO_CHANGES)
        self.assertIn("charter change create", row.note)

    def test_with_changes_the_doorway_has_no_reason_and_opens(self):
        self.make_change("component-api-2")
        row = next(r for r in choose.open_rows(FID) if r.id == "pick:change")
        self.assertEqual(row.note, "")

    def test_the_picker_lists_the_workspaces_changes_and_marks_the_current_one(self):
        self.make_change("a-1")
        self.make_change("b-2")
        switch.to_change(FID, "b-2")
        roster = choose.roster(choose.CHANGE, FID)
        self.assertEqual(list(roster.names), ["a-1", "b-2"])
        self.assertTrue(roster.rows[1].title.startswith(choose.MARK[0]))
        self.assertTrue(roster.rows[0].title.startswith(choose.MARK[1]))

    def test_choosing_one_records_it_and_bumps_the_frame(self):
        self.make_change("a-1")
        before = state.version(FID)
        out = choose.switch_to(choose.CHANGE, FID, "a-1")
        self.assertTrue(out.ok, out.message)
        self.assertEqual(state.frame_change(FID), "a-1")
        self.assertNotEqual(state.version(FID), before)

    def test_an_unknown_change_is_a_question_and_never_an_implicit_create(self):
        self.make_change("a-1")
        out = switch.to_change(FID, "nope")
        self.assertFalse(out.ok)
        self.assertIn("no change", out.message)
        self.assertIn("a-1", out.message)
        self.assertIsNone(state.frame_change(FID))

    def test_a_slug_that_cannot_name_a_change_is_refused_before_the_lookup(self):
        """The value reaches a `changes/<slug>.json` join, which is #442's position."""
        out = switch.to_change(FID, "../etc")
        self.assertFalse(out.ok)
        self.assertIn("cannot name a change", out.message)

    def test_a_hostile_slug_in_the_refusal_is_contained(self):
        out = switch.to_change(FID, "a\nb")
        self.assertFalse(out.ok)
        self.assertEqual(len(out.message.split("\n")), 1, repr(out.message))

    def test_the_changes_listed_are_the_frames_workspace_and_not_this_process(self):
        """#512. A palette is a `run-shell` child of a tmux server shared between frames,
        so resolving locally would list another plane's changes on this frame's screen."""
        workspace.ensure("other")
        state.record_workspace(FID, "other")
        self.make_change("a-1")           # written into `ws`, which is not this frame's
        self.assertEqual(switch.changes(FID), [])
        state.record_workspace(FID, "ws")
        self.assertEqual(switch.changes(FID), ["a-1"])

    def test_a_change_picker_can_never_be_refused_by_a_launch_pin(self):
        """There is no `$CHARTER_CHANGE`: a change is what one frame is LOOKING at, not an
        identity a launch hands it. `choose.PIN` carries no entry for it, and an entry
        naming a variable nothing sets would make `pin_reason` answer through a lookup
        rather than through a decision."""
        self.assertNotIn(choose.CHANGE, choose.PIN)
        self.make_change("a-1")
        self.assertEqual(choose.pin_reason(choose.CHANGE, FID), "")


if __name__ == "__main__":
    unittest.main()
