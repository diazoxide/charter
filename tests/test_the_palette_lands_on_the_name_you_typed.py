"""Three properties of the `F2` palette, reported together and fixed together.

A UX audit drove a real frame through a real outer tmux with real keystrokes and filed
five things; three of them are this surface, and two of the three turned out to be one
change seen from two sides.

**#732 — typing a workspace's exact name did not switch to it.** `docs/frame.md` sells
`F2`, the name, Enter as *the* route for an operator who knows where they are going. A
chat id is `<workspace>.<n>`, so on a plane with a workspace `alpha` the doorway row
`chat: alpha.1 — pick another` holds `alpha` as a substring of its title — and the
doorways are in the catalogue while the names are gathered after it, so the doorway sorted
first. On the reported plane that doorway was also *refused* (one chat, nothing to pick),
so Enter opened nothing, switched nothing, and answered a question about chats. The
`alpha` row was one Down away the whole time.

**#749 — the palette had two left insets.** Rows that could carry a `*` composed it into
their own title; rows that could not composed nothing. Text started at column 3 for the
doorways and the actions and at column 5 for the density and chrome rows, so the cursor
sat two columns from the text on one half of the list and four on the other — against
`docs/frame.md`'s own stated rule that every row's text starts in the same column.

**#746 — a repo selection cost a whole palette per row.** `repo: select the next row` is
the only action charter offers whose natural use is repeated, and with `[frame] mouse =
false` shipped as the default it is the repo table's only interaction model. Every Enter
closed the pane and killed the process, so three rows down a fourteen-row table was
fourteen keystrokes and three ~3-second cycles.

**The three are one change and this file is why.** #749 moved the mark off the title and
into `overlay.Row.mark`; that is what makes "is this row's name what the operator typed"
a comparison rather than a prefix-strip against a constant, which is what #732's ordering
needed. #746 is separate machinery but the same surface, and its feedback lands in the
header for a reason the other two share: the palette pane is zoomed over the whole window
(`overlay.modal_argvs`), so nothing behind it is on screen to look at.

**What is deliberately NOT here: refused rows being hidden.** #512's rule is that an
operator cannot ask about an option they cannot see, and the audit confirmed the rule
works. A refused row is still listed and still carries its reason; what changed is only
which row the cursor starts on.
"""

from __future__ import annotations

import os
import unittest
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame, tui
from charter.frame import actions, overlay, palette, state

from tests._isolation import PersonaIso
from tests.test_frame_pickers import _plane_personas, _plane_workspaces


#: A line separator `str.splitlines` honours, which is the bound #472 settled on: the
#: property is "renders as one line", not "holds no `\\n`". Spelled with an escape rather
#: than the character itself, because a U+2028 in this file's own source is a line the
#: next reader cannot see.
_SEPARATOR = "\u2028"


def _row(rid: str, title: str, note: str = "", *, mark: bool = False,
         refused: bool = False) -> overlay.Row:
    return overlay.Row(id=rid, title=title, note=note, mark=mark, refused=refused)


#: The reported plane, as rows, in the order `_draw_palette` composes them: the four
#: doorways, then the actions, then — once something has been typed — the names.
#:
#: Spelled here rather than driven through `commands_frame._draw_palette` because the
#: defect is an ORDERING and the ordering is `palette.narrow`'s; a plane fixture would put
#: a frame directory, four listers and a tmux socket between the test and the two lines it
#: is about. `tests/test_frame_palette_names.py` drives the real surface over a real plane
#: and asserts the same rule from that end.
_DOORWAYS = (
    _row("pick:workspace", "workspace: default — pick another"),
    _row("pick:persona", "persona: steward — pick another"),
    _row("pick:change", "change — pick one",
         "no cross-repo change in this workspace", refused=True),
    _row("pick:chat", "chat: alpha.1 — pick another",
         "this workspace has one chat — open another with `charter <harness>`",
         refused=True),
)
_ACTIONS = (
    _row("frame.detach", "detach — leave the harness running"),
    _row("repo.next", "repo: select the next row"),
    _row("density.full", "density: full", mark=True),
)
_NAMES = (
    _row("workspace:n0", "alpha", "workspace"),
    _row("persona:n0", "steward", "persona", mark=True),
    _row("chat:n0", "alpha.1", "this workspace has one chat", mark=True, refused=True),
)


class TypingAWholeNameLandsOnIt(unittest.TestCase):
    """#732. The rule, stated once: an exact match first, an actionable one ahead of a
    refused one, everything else in the position the catalogue gave it."""

    def _typed(self, query: str) -> list[overlay.Row]:
        return list(palette.narrow(_DOORWAYS + _ACTIONS + _NAMES, query))

    def test_the_reported_case_puts_the_workspace_under_the_cursor(self):
        """The audit's own transcript, end to end. `alpha` matched three rows; the first
        was a doorway that could not run, so Enter closed the palette having done nothing
        the operator asked for.

        Asserted as the FIRST row rather than as "somewhere in the list", because where it
        is in the list is the whole defect — every one of these rows was already listed.
        """
        got = self._typed("alpha")
        self.assertEqual([r.id for r in got],
                         ["workspace:n0", "pick:chat", "chat:n0"])

    def test_the_row_the_cursor_starts_on_is_one_that_can_run(self):
        """The second half of the report, said about the palette rather than about
        `narrow`: a `Palette` puts its cursor at 0, so "first" and "under the cursor" are
        the same row and the surface is where that is worth pinning."""
        p = palette.Palette(catalogue=_DOORWAYS + _ACTIONS,
                            query_only=lambda: _NAMES)
        for ch in "alpha":
            p.handle(overlay.Event(kind=overlay.KEY, name=ch), 24)
        self.assertEqual(p.rows[p._sel].id, "workspace:n0")
        self.assertFalse(p.rows[p._sel].refused)

    def test_an_exact_match_beats_a_partial_one_even_when_both_can_run(self):
        """`docs/frame.md`'s own worked example, which used to answer the wrong row: `zeb`
        is a persona and a prefix of two workspaces, and the catalogue lists workspaces
        first. Typing a name in full and getting a different name is the defect whether or
        not the row that won could run."""
        rows = (_row("workspace:n0", "zeb-api", "workspace"),
                _row("workspace:n1", "zebra-ui", "workspace"),
                _row("persona:n0", "zeb", "persona"))
        self.assertEqual([r.title for r in palette.narrow(rows, "zeb")],
                         ["zeb", "zeb-api", "zebra-ui"])

    def test_between_two_exact_matches_the_one_that_can_run_comes_first(self):
        """A pinned frame is the case: `$CHARTER_WORKSPACE` refuses the workspace `alpha`
        while a persona of the same name switches fine. Both are exactly what was typed,
        and only one of them does anything."""
        rows = (_row("workspace:n0", "alpha", "cannot switch: $CHARTER_WORKSPACE pins "
                                              "this frame to 'beta'", refused=True),
                _row("persona:n0", "alpha", "persona"))
        self.assertEqual([r.id for r in palette.narrow(rows, "alpha")],
                         ["persona:n0", "workspace:n0"])

    def test_a_refused_row_is_still_listed_with_its_reason(self):
        """#512's rule, which this ordering must not quietly become a way around. The
        chat doorway that started the report is refused, and it is still on the list with
        the sentence that says why — one row further down, not gone."""
        got = self._typed("alpha")
        refused = [r for r in got if r.refused]
        self.assertEqual([r.id for r in refused], ["pick:chat", "chat:n0"])
        for r in refused:
            self.assertTrue(r.note, f"{r.id} is refused and says nothing")

    def test_nothing_typed_is_the_list_it_always_was(self):
        """The cost of ranking, and the bound on it. With an empty query no row is an
        exact match for anything, so `F2` draws the catalogue in the catalogue's order —
        the eleven-row screen `docs/frame.md` pictures, unmoved."""
        catalogue = _DOORWAYS + _ACTIONS
        self.assertEqual([r.id for r in palette.narrow(catalogue, "")],
                         [r.id for r in catalogue])

    def test_a_row_with_no_title_is_not_what_an_empty_query_typed(self):
        """The guard `exact` opens with, and the case that makes it more than a spelling.

        `Action` holds its title to being a string and nothing more, so `title=""` is a
        row a provider can register — and `""` is what `casefold` gives an untyped query.
        Without the guard that row is an exact match for having typed nothing, and `F2`
        on a plane with such a provider installed opens with the cursor on it, above every
        row charter put in the catalogue on purpose.
        """
        rows = (_row("frame.detach", "detach"), _row("acme.nameless", ""))
        self.assertEqual([r.id for r in palette.narrow(rows, "")],
                         ["frame.detach", "acme.nameless"])

    def test_a_partial_query_leaves_every_match_where_it_was(self):
        """The other bound. `re` matches an action and a doorway and neither is a name
        typed in full, so the pair keeps the catalogue's order — a palette whose rows
        shuffled on every keystroke is what the old menu refused and this does too."""
        got = palette.narrow(_DOORWAYS + _ACTIONS, "e")
        self.assertEqual([r.id for r in got],
                         [r.id for r in _DOORWAYS + _ACTIONS
                          if "e" in r.title.casefold()])

    def test_the_case_rule_reaches_the_ordering_too(self):
        """`casefold` on both sides, the same as `matches`. A palette that filtered
        case-insensitively and then ranked case-sensitively would find the row and still
        put the cursor somewhere else, which is indistinguishable from not finding it.

        **Both sides, in both directions**, because one case each way is what an operator
        actually produces: a workspace named `Beta` reached by typing `beta`, and one
        named `beta` reached by a shift key nobody meant to hold. Asserted as two cases in
        one so that neither half can be dropped as the redundant one — the sweep reported
        exactly that about the query's own `casefold`, which only the second case reaches.
        """
        rows = (_row("a.b", "beta — do the thing"), _row("workspace:n0", "Beta"))
        self.assertEqual([r.id for r in palette.narrow(rows, "beta")],
                         ["workspace:n0", "a.b"])
        rows = (_row("a.b", "beta — do the thing"), _row("workspace:n0", "beta"))
        self.assertEqual([r.id for r in palette.narrow(rows, "BETA")],
                         ["workspace:n0", "a.b"])

    def test_the_query_is_contained_and_folded_in_one_place(self):
        """`typed` is the whole of the case rule and #472's rule, asked once for both the
        filter and the ordering.

        Two assertions, one per transform, because either alone leaves the other
        deletable — which is what the sweep reported when `exact` carried its own copy of
        this line. The palette builds its own query out of printable single characters, so
        a separator cannot be typed into it; `narrow` and `matches` are functions anything
        may call, and the guard belongs at the join rather than at whichever writer
        happens to exist today.
        """
        self.assertEqual(palette.typed("ALPHA"), "alpha")
        self.assertNotIn(_SEPARATOR, palette.typed("al" + _SEPARATOR + "pha"))

    def test_an_action_id_typed_in_full_is_exact_too(self):
        """`matches` accepts a provider's documented id (`acme.deploy`) as well as the
        title, so the ordering has to accept the same one — a filter and a ranking that
        disagree about what counts as typing the name is the defect one layer up."""
        rows = (_row("x.y", "something acme.deploy adjacent"),
                _row("acme.deploy", "Deploy to production"))
        self.assertEqual([r.id for r in palette.narrow(rows, "acme.deploy")],
                         ["acme.deploy", "x.y"])

    def test_a_picker_id_is_not_a_name_the_operator_can_type(self):
        """`frame/choose.py`'s ids are charter's own counter and are never drawn, so
        `matches` does not match them — and `exact` must not either, or `workspace:n0`
        would be a query that ranks a row nobody can see the id of."""
        row = _row("workspace:n0", "alpha", "workspace")
        self.assertFalse(palette.exact("workspace:n0", row))
        self.assertFalse(palette.matches("workspace:n0", row))


class EveryRowStartsInTheSameColumn(unittest.TestCase):
    """#749. One inset, measured on the drawn line rather than on the model."""

    #: Where a row's text begins: two cells of cursor, two of mark. Derived from the
    #: constants rather than written as `4`, so a marker that changes width moves this
    #: with it instead of turning every case below red at once.
    INSET = tui.width(overlay._MARK[0]) + tui.width(overlay.ROW_MARK[0])

    #: Wide enough that `_title_width`'s half-the-row cap truncates nothing here. A
    #: truncated title cannot be found on the line, and "the title is missing" is not the
    #: failure any of these cases is about — the narrow-pane arithmetic has its own case
    #: at the bottom of this class.
    WIDE = 120

    def _drawn(self, rows) -> list[str]:
        body = overlay.Surface(rows=tuple(rows)).render(self.WIDE, len(rows) + 4)
        return [tui.strip_ansi(ln) for ln in body[1:1 + len(rows)]]

    def _text_starts(self, rows) -> set[int]:
        """Which CELL each row's title begins in, as a set.

        Measured against the title rather than with `lstrip`, because the two things in
        front of it are not both spaces: the selected row opens with `> ` and a marked row
        carries `* `, so "how many leading spaces" answers 0 and 2 for rows whose text is
        in the same column. That reading is the defect said backwards.
        """
        return {tui.width(ln[:ln.index(r.title)])
                for ln, r in zip(self._drawn(rows), rows)}

    def test_a_markable_row_and_a_plain_one_line_up(self):
        """The reported screen in miniature: a `density` row that carries the `*` beside a
        `detach` row that has no mark to carry. They used to start two columns apart."""
        starts = self._text_starts([_row("frame.detach", "detach"),
                                    _row("density.full", "density: full", mark=True)])
        self.assertEqual(starts, {self.INSET})

    def test_every_row_of_the_reported_palette_starts_at_one_column(self):
        """All of them, as reported. Asserted as a SET of one so a failure names how many
        left edges the list has rather than which row happens to be checked first."""
        self.assertEqual(self._text_starts(_DOORWAYS + _ACTIONS + _NAMES),
                         {self.INSET})

    def test_the_mark_says_which_row_the_frame_is_on(self):
        """Reserving the column is only half of it — the column still has to hold the
        mark. A palette that lined up by drawing two spaces on every row would pass the
        case above and lose the one thing the column is for."""
        lines = self._drawn([_row("a.b", "beta"), _row("a.c", "alpha", mark=True)])
        self.assertNotIn(overlay.ROW_MARK[0], lines[0])
        self.assertIn(overlay.ROW_MARK[0] + "alpha", lines[1])

    def test_the_cursor_and_the_mark_are_two_columns_not_one(self):
        """They answer different questions — where you are pointing, and where the frame
        is — and the row the frame is on is very often not the row under the cursor. A
        surface that reused one column would make the second unaskable."""
        rows = (_row("a.b", "beta"), _row("a.c", "alpha", mark=True))
        surface = overlay.Surface(rows=rows)
        surface.move(1)
        line = tui.strip_ansi(surface.render(self.WIDE, 6)[2])
        self.assertTrue(line.startswith(overlay._MARK[0] + overlay.ROW_MARK[0]), line)

    def test_the_note_column_is_sized_against_both_markers(self):
        """The arithmetic half, and it is the NOTE that pays for getting it wrong.

        `_title_width` splits what is left of the pane after the markers and the gap.
        Counting only the cursor's two cells makes the title column two wider than the row
        can hold, and the trailing `tui.truncate` then takes those two cells off the right
        — out of the note. Task 4's rule is that an unavailable action is listed *with its
        reason*, and a reason cut by two characters still LOOKS like an answer, which is
        the false-clean failure #512 is about one surface over.

        Measured against a pane sized so the note fits exactly: at :data:`WIDE`-minus-what
        the row needs, the whole reason is on the line, and two stolen cells take the end
        of it off. Both halves are asserted — that it fits, and that nothing overflows —
        because a version that never truncated at all would pass one of them.
        """
        title, note = "t" * 40, "n" * 17
        rows = (_row("a.b", title, note),)
        width = 40
        line = tui.strip_ansi(overlay.Surface(rows=rows).render(width, 6)[1])
        self.assertIn(note, line, line)
        self.assertLessEqual(tui.width(line), width, line)


class ARepeatableRowKeepsThePaletteOpen(PersonaIso, unittest.TestCase):
    """#746. The palette is the pane; a repeat is what stops paying for it per row.

    `PersonaIso` because `ActionRegistry.invoke` writes an `inflight` record before it
    starts the work — the frame's spinner cannot miss a fast action — and that is a write
    into the plane's state directory. `tests/_planeguard.py` refuses it against the real
    one, correctly.
    """

    def _palette(self) -> palette.Palette:
        offers = (actions.Offer(id="repo.next", title="repo: select the next row",
                                available=True, reason="", repeat=True),
                  actions.Offer(id="frame.detach", title="detach", available=True,
                                reason=""))
        return palette.Palette(catalogue=palette.rows(offers))

    def test_the_repeat_flag_reaches_the_row_source_from_the_action(self):
        """`Action` → `Offer` → the caller that decides whether to close. Three hops, and
        the one that matters is that charter's own repo rows declare it — a flag nothing
        sets is a feature that ships off."""
        from charter.frame import builtin_actions
        reg = actions.ActionRegistry()
        builtin_actions._register_selection(reg)
        self.assertEqual(sorted(a.id for a in reg.all() if a.repeat),
                         ["repo.next", "repo.previous"])

    def _reg(self):
        """A registry with one repeatable action and one that is not, both able to run.

        `frame/action.py`'s own contract objects rather than a stand-in, so the flag under
        test travels the route it travels in production: `Action` → `ActionRegistry.get` →
        the branch in `commands_frame._again`.
        """
        from charter.frame import action
        reg = actions.ActionRegistry()
        self.ran = []
        reg.register(action.Action(
            id="repo.next", title="repo: select the next row", repeat=True,
            run=lambda ctx: self.ran.append("next") or "selected auth"))
        reg.register(action.Action(
            id="frame.detach", title="detach",
            run=lambda ctx: self.ran.append("detach") or "detaching"))
        return reg

    def test_a_repeatable_row_hands_the_same_surface_back_and_runs_the_action(self):
        """The whole of #746 at the seam `own_the_tty` reads: a `Surface` back means the
        pane is not torn down, and it is the SAME object, so the query, the rows and the
        cursor are the ones the operator left."""
        from charter import commands_frame
        reg, p = self._reg(), self._palette()
        got = commands_frame._again(p.rows[0], p, reg, fid="f-1", snapshot={})
        self.assertIs(got, p)
        self.assertEqual(self.ran, ["next"])
        self.assertIn("selected auth", p.heading)

    def test_an_ordinary_row_still_ends_the_palette(self):
        """The bound on it. `None` is what `own_the_tty` reads as "this row is the
        answer", and every row that is not declared repeatable must still give it — a
        palette that stayed open on `detach` would be a pane over a detaching client."""
        from charter import commands_frame
        reg, p = self._reg(), self._palette()
        detach = next(r for r in p.rows if r.id == "frame.detach")
        self.assertIsNone(commands_frame._again(detach, p, reg, fid="f-1", snapshot={}))
        self.assertEqual(self.ran, [])

    def test_a_repeatable_row_that_REFUSES_keeps_the_palette_and_says_why(self):
        """The other branch of what a repeat reports, and the one a docstring claimed
        without anything asserting it.

        `repo: select the next row` refuses on a workspace with no clones, and that is an
        ordinary state rather than an edge: a plane whose gather has not run yet, or one
        that genuinely has nothing cloned. The palette must not close — nothing was
        started, and closing would cost the operator the pane for a keypress that did
        nothing — and the reason must land where a repeat's outcome always lands, because
        the attention row it would otherwise use is under the zoom.

        The deletion sweep found this: `inv.reason if not inv.started else (inv.note or
        inv.error)` collapsed to its second branch and no test noticed, because every case
        here ran an action that starts.
        """
        from charter import commands_frame
        from charter.frame import action
        reg = actions.ActionRegistry()
        reg.register(action.Action(
            id="repo.next", title="repo: select the next row", repeat=True,
            run=lambda ctx: self.fail("a refused action must not be run"),
            available=lambda ctx: False,
            reason_unavailable=lambda ctx: "this workspace has no clones"))
        p = self._palette()
        got = commands_frame._again(p.rows[0], p, reg, fid="f-1", snapshot={})
        self.assertIs(got, p, "a refusal closed the palette")
        self.assertIn("this workspace has no clones", p.heading)

    def test_a_row_that_is_not_an_action_at_all_ends_the_palette(self):
        """A picker row reaches this function too — `_draw_palette` asks `_picker` first,
        and a doorway that opened nothing falls through to here. `ActionRegistry.get`
        raises for an id it does not hold, and that is an ordinary answer here rather than
        a traceback into a pane that is about to stop existing."""
        from charter import commands_frame
        reg, p = self._reg(), self._palette()
        row = _row("workspace:n0", "alpha", "workspace")
        self.assertIsNone(commands_frame._again(row, p, reg, fid="f-1", snapshot={}))

    def test_reporting_leaves_the_query_and_the_cursor_exactly_where_they_were(self):
        """The property that makes a repeat a repeat. `Palette._refilter` puts the cursor
        back at the top by design, so a repeat that re-filtered would move it off the row
        the operator is holding Enter on — one move per typed filter, which is the cost
        the report is about, reintroduced."""
        p = self._palette()
        for ch in "next":
            p.handle(overlay.Event(kind=overlay.KEY, name=ch), 24)
        before = (p.query, p._sel, [r.id for r in p.rows])
        p.report("selected auth")
        self.assertEqual((p.query, p._sel, [r.id for r in p.rows]), before)

    def test_the_header_says_what_the_repeat_just_did(self):
        """The overlay pane is zoomed over the window (`overlay.modal_argvs`), so the repo
        table the selection is moving through is not drawn. The header is the only surface
        the operator can be told on."""
        p = self._palette()
        p.report("selected auth")
        self.assertIn("selected auth", p.heading)

    def test_a_second_repeat_replaces_the_first_sentence_and_does_not_append(self):
        """Held in a field and recomposed, never appended: a heading that grew by one
        clause per Enter would be a header that scrolls the list off the top of the pane
        after a dozen moves."""
        p = self._palette()
        p.report("selected auth")
        p.report("selected billing")
        self.assertIn("selected billing", p.heading)
        self.assertNotIn("selected auth", p.heading)

    def test_the_next_keystroke_clears_it(self):
        """It is a report about the last keypress, not a second label. Typing is a new
        question and the answer to the old one must not stand over it.

        Asserted as the WHOLE heading rather than as the sentence being absent: an empty
        report that still emitted its separator would leave `charter /n · ` on screen —
        a header with a dangling `·` and nothing after it, which "the sentence is gone"
        cannot tell from the header being right. The sweep reported exactly that gap.
        """
        p = self._palette()
        p.report("selected auth")
        p.handle(overlay.Event(kind=overlay.KEY, name="n"), 24)
        self.assertEqual(p.heading, palette.HEADING + palette.PROMPT + "n")

    def test_an_empty_report_leaves_the_header_exactly_as_it_was(self):
        """The same property one step earlier: an action that answered nothing has nothing
        to say, and the header must not gain a separator for it."""
        p = self._palette()
        p.report("")
        self.assertEqual(p.heading, palette.HEADING)

    def test_the_query_still_shows_beside_the_report(self):
        """Both, because both are live: what is being filtered, and what the last Enter
        did. A report that replaced the query would leave the operator unable to see why
        the list is three rows long."""
        p = self._palette()
        for ch in "next":
            p.handle(overlay.Event(kind=overlay.KEY, name=ch), 24)
        p.report("selected auth")
        self.assertIn("next", p.heading)
        self.assertIn("selected auth", p.heading)

    def test_a_report_is_contained_before_it_reaches_the_header(self):
        """#472's rule at one more join. An action's answer is a string charter's own code
        wrote today and a provider's code writes tomorrow, and the header is measured and
        drawn — a newline in it is two rows where the surface counted one."""
        p = self._palette()
        p.report("selected\nauth now")
        self.assertEqual(len(p.heading.splitlines()), 1, repr(p.heading))


class TheRepeatIsWiredAndNotMerelyDeclared(PersonaIso, unittest.TestCase):
    """The palette `_draw_palette` actually builds, asked whether its `then` repeats.

    **The mutation this exists for is `then=lambda row: _picker(...)`** — the shape the
    function had before #746 — which leaves `Action.repeat`, `Offer.repeat` and
    `commands_frame._again` all present, all tested, and reached by nothing. A flag
    nothing consults is a feature that ships off, and this repository has shipped one
    before (#168: the plane-root guard, inert on any plane not running the plugin, with
    `doctor` showing a green tick over it).

    **The repo rows are deliberately left UNAVAILABLE here**, and that is the case rather
    than a gap in the fixture: this frame has no clones, so `repo: select the next row`
    refuses. A repeatable row that refuses still keeps the palette open and still says why
    in the header — which is the behaviour a palette that is not closing owes the
    operator, and it means this case pins the wiring without a gather, a clone or a repo
    table standing between the test and the one line it is about.
    """

    FID = "f-repeat"

    def setUp(self) -> None:
        super().setUp()
        self.enterContext(mock.patch.dict(
            os.environ, {"CHARTER_SESSION_ID": self.FID}, clear=True))
        _plane_workspaces("alpha")
        _plane_personas("steward")
        state.frame_dir(self.FID, create=True)
        state.record_server(self.FID, "charter")
        state.record_harness_pane(self.FID, "%3")
        state.record_identity(self.FID, {"CHARTER_SESSION_ID": self.FID})
        state.record_workspace(self.FID, "alpha")
        self.enterContext(mock.patch.object(commands_frame, "_say_on_screen"))
        self.enterContext(mock.patch.object(commands_frame, "_close_palette"))

    def _then(self):
        """The surface and the `then` `_draw_palette` handed `own_the_tty`, taken off it.

        Both, because the test has to press a row of the real catalogue on the real
        surface — rebuilding either here would be a second answer to what the palette is,
        and the mutation that unwires the real one would leave this green.
        """
        seen: list = []
        with mock.patch.object(palette, "own_the_tty",
                               lambda surface, *, then=None, **kw: seen.append(
                                   (surface, then))):
            commands_frame.cmd_palette(SimpleNamespace(client="", pane=True))
        return seen[0]

    def test_pressing_a_repo_row_hands_the_palette_back_instead_of_closing_it(self):
        surface, then = self._then()
        row = next(r for r in surface.rows if r.id == "repo.next")
        self.assertIs(then(row), surface)

    def test_pressing_an_ordinary_row_still_ends_the_palette(self):
        surface, then = self._then()
        row = next(r for r in surface.rows if r.id == "frame.detach")
        self.assertIsNone(then(row))

    def test_a_doorway_still_opens_its_picker_first(self):
        """`_picker` is asked before the repeat, and a doorway is neither an action nor
        repeatable — so a wiring that put the repeat first would answer a doorway with the
        palette it was already looking at."""
        surface, then = self._then()
        row = next(r for r in surface.rows if r.id == "pick:workspace")
        nxt = then(row)
        self.assertIsInstance(nxt, palette.Palette)
        self.assertIsNot(nxt, surface)
        self.assertEqual(nxt.label, "workspace")
