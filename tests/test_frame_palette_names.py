"""Typing at the top of the palette matches NAMES, and the doorways stay.

Phase 2, Task 8. Task 6 (#571) was right to stop registering an `Action` per workspace —
the contract is fire-and-report, and forty of them meant forty `run`s each starting a
second charter process — but the doorway it left behind cost the operator a keypress on
the thing they do most: switching workspace went from `F2` → type → Enter to `F2` → Enter
→ type → Enter. This puts the keypress back **without putting the actions back**.

Four properties get most of this file's length, because each is a rule the decision states
and a first implementation loses:

* **Names are rows, never `Action`s.** The registry does not grow with the plane, nothing
  is spawned by listing a name, and no id a name row carries could be an action id.
* **The top level stays cheap.** An operator who opened `F2` to press `detach` must not pay
  to enumerate forty workspaces, so the roster is read on the FIRST KEYSTROKE and never on
  an empty query — measured here by counting the listers `choose.names_of` asks, which is
  what "the roster is read" means.
* **The doorways still work.** Browsing without knowing the name is the case they exist
  for, and they are asserted on the surface `_draw_palette` actually constructs — the
  measurement Task 6 recorded is that deleting them reddened nothing outside a tmux-gated
  module that skips on a machine with no tmux.
* **A hostile name is one row and runs nothing on THIS path too.** `overlay.Surface.render`
  contains before `tui.width` sees anything, and Task 6 deliberately added no second call
  because a second one would be unpinnable. This asserts the existing one covers the new
  route rather than asking for another.

The name rows and the picker's rows are the same objects (`choose.labelled` only re-notes
them), so everything `tests/test_frame_pickers.py` pins about a picker row — the id-not-
title mapping, the raw name reaching `switch.py`, the refusals — holds here unrestated.
"""

from __future__ import annotations

import os
import unittest
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame, config, tui
from charter.frame import (builtin_actions, choose, component, overlay, palette, state,
                           switch)

from tests._isolation import PersonaIso
from tests.test_frame_pickers import HOSTILE, _plane_personas, _plane_workspaces

#: Every character `str.splitlines` treats as ending a line, which is the bound #472
#: settled on: the property is "renders as one line", not "holds no `\\n`". Spelled with
#: escapes rather than the characters themselves, because a U+2028 in this file's own
#: source is a line the next reader cannot see.
_SEPARATORS = ("\n", "\r", "\u2028", "\u2029", "\u0085")
#: What a name row's id has and no action id can: `frame/choose.NAME_ID`'s counter, behind
#: the `:` that keeps the two namespaces apart. Used to say "the rows that stand for a
#: name" without re-deriving the format string.
_NAME_ROW = ":n"

#: `HOSTILE` with a findable prefix on each name, and the prefix is load-bearing rather
#: than a detail: these names have to be reached *by typing*, and the eight share no single
#: character between them (`next<U+0085>line` holds no `a`; the quoting one holds no `e`).
#: What is prefixed is a plain two-letter word, so the row still ends up holding the
#: hostile name unaltered — which is what the containment cases measure.
_FIND = "zq"
_FINDABLE = tuple(_FIND + n for n in HOSTILE)



class _Frame(PersonaIso):
    """One frame on an isolated plane. The same fixture the picker tests use, plus the
    names the decision's own worked example is written against."""

    FID = "f-name"
    PIN = {"CHARTER_WORKSPACE": "", "CHARTER_PERSONA": ""}
    WORKSPACES = ("alpha", "zeb-api", "zebra-ui")
    PERSONAS = ("forge", "zeb")

    def setUp(self) -> None:
        super().setUp()
        self.enterContext(mock.patch.dict(
            os.environ, {"CHARTER_SESSION_ID": self.FID}, clear=True))
        _plane_workspaces(*self.WORKSPACES)
        _plane_personas(*self.PERSONAS)
        state.frame_dir(self.FID, create=True)
        state.record_server(self.FID, "charter")
        state.record_harness_pane(self.FID, "%3")
        state.record_identity(self.FID, {"CHARTER_SESSION_ID": self.FID, **self.PIN})
        state.record_workspace(self.FID, "alpha")
        self.said = self.enterContext(
            mock.patch.object(commands_frame, "_say_on_screen"))
        self.enterContext(mock.patch.object(commands_frame, "_close_palette"))

    # -- the two ways a test drives the palette ----------------------------------- #

    def _surface(self) -> palette.Palette:
        """The surface `_draw_palette` builds, taken off it rather than rebuilt here.

        Rebuilding it in the test would be a second answer to "what is in the catalogue",
        and the mutation that empties the real one would leave this file green.
        """
        seen: list = []
        with mock.patch.object(palette, "own_the_tty",
                               lambda surface, **kw: seen.append(surface)):
            commands_frame.cmd_palette(SimpleNamespace(client="", pane=True))
        return seen[0]

    def _typed(self, query: str) -> palette.Palette:
        """That surface with *query* typed into it one keypress at a time.

        Through `Palette.handle` and never by assigning `query`, because the whole of this
        task lives in what a keystroke triggers — a test that set the attribute would pass
        against a `_refilter` that never consults `query_only` at all.
        """
        surface = self._surface()
        for ch in query:
            surface.handle(overlay.Event(overlay.KEY, ch), 24)
        return surface

    def _titles(self, surface) -> list[str]:
        return [r.title for r in surface.rows]

    def _names(self, surface) -> list[overlay.Row]:
        """Only the rows that stand for a name. The doorways match a query as readily as
        a name does — `workspace: alpha — pick another` contains `alpha` — and that is
        deliberate, so a test about names says which rows it means."""
        return [r for r in surface.rows if _NAME_ROW in r.id]


def _pick(query: str, *, index: int = 0, want: str | None = None):
    """Stand in for the operator: type *query*, then press Enter on one of what is left.

    A double for `palette.own_the_tty` in the shape `tests/test_frame_pickers._pane`
    already uses, one input earlier — that one stands in for an operator who only ever
    presses keys the test hands it, and this one has to actually type, because the rows it
    is about do not exist until something has been typed.

    *want* names a row by ID, which is how a test says "the hostile one" without depending
    on how many doorways and actions happened to match the same query; *index* is the
    positional form, and is what says "whatever is under the cursor", which is the claim
    the keystroke count is about.

    *then* is consulted exactly as the real loop consults it, so a row that turns out to
    be a doorway opens its picker here too rather than being answered with.
    """
    def fake(surface, *, then=None, **kw):
        for ch in query:
            surface.handle(overlay.Event(overlay.KEY, ch), 24)
        while True:
            if want is not None:
                row = next((r for r in surface.rows if r.id == want), None)
            else:
                row = surface.rows[index] if index < len(surface.rows) else None
            if row is None:
                return None
            nxt = then(row) if then is not None else None
            if nxt is None:
                return row
            surface = nxt
    return fake


class TypingAtTheTopLevelFindsANameDirectly(_Frame, unittest.TestCase):
    """The headline, and the worked example from the decision itself."""

    def test_a_query_matches_workspace_and_persona_names(self):
        self.assertEqual(self._titles(self._typed("zeb")),
                         ["  zeb-api", "  zebra-ui", "  zeb"])

    def test_each_name_says_which_KIND_it_is_so_two_nouns_are_told_apart(self):
        """`zeb` the persona and `zeb-api` the workspace are one keystroke apart and would
        otherwise be two bare words in one list. The kind is the note — the right-hand
        column — which is where `frame/palette.py` already puts what charter has to say
        about a row rather than what the operator typed."""
        rows = self._typed("zeb").rows
        self.assertEqual([(r.title.strip(), r.note) for r in rows],
                         [("zeb-api", "workspace"), ("zebra-ui", "workspace"),
                          ("zeb", "persona")])

    def test_the_name_in_use_keeps_its_mark_here_too(self):
        """A name row is the picker's own row re-noted, so the `*` that answers "which one
        am I on" travels with it. Losing it would make the top-level list the one surface
        in the frame that cannot say where the frame is.

        The doorway matches `alpha` too — it says `workspace: alpha — pick another`, which
        is Task 6's own reason for putting the name in that title — so this asks the name
        rows rather than the first row.
        """
        rows = self._names(self._typed("alpha"))
        self.assertEqual([r.title for r in rows], [f"{choose.MARK[0]}alpha"])

    def test_an_empty_query_is_the_doorways_and_the_actions_and_nothing_else(self):
        """The other half of "the doorways stay": with nothing typed the list is exactly
        what it was before this task, so `F2` Enter still opens a picker and `F2` `d`
        Enter still reaches `detach`."""
        surface = self._surface()
        ids = [r.id for r in surface.rows]
        self.assertEqual(ids[:2], ["pick:workspace", "pick:persona"], ids)
        self.assertIn("frame.detach", ids)
        self.assertEqual([i for i in ids if _NAME_ROW in i], [],
                         "a name reached the list with nothing typed")

    def test_a_query_that_matches_a_doorway_still_shows_the_doorway(self):
        """Browsing and switching are not exclusive: `workspace` finds the doorway, and
        the doorway is what an operator who does not know the name presses."""
        ids = [r.id for r in self._typed("workspace").rows]
        self.assertIn("pick:workspace", ids)

    def test_names_come_after_the_actions_so_no_action_moves(self):
        """`narrow` never reorders, so where the two groups sit is decided once, in
        `Palette._reachable`. Names first would bury `detach` under every name holding a
        `d` — the palette would have bought a keystroke for one job by charging one for
        every other."""
        rows = self._typed("a").rows
        ids = [r.id for r in rows]
        first_name = next(i for i, r in enumerate(rows) if _NAME_ROW in r.id)
        self.assertTrue(all(_NAME_ROW not in r.id for r in rows[:first_name]), ids)
        self.assertIn("frame.detach", ids[:first_name], ids)

    def test_typing_a_name_and_pressing_enter_switches_the_frame_and_bumps_it(self):
        """The whole point, end to end through `_draw_palette`: three keystrokes of name
        and one Enter, with no Enter on a doorway in between."""
        was = state.version(self.FID)
        with mock.patch.object(palette, "own_the_tty", _pick("zebra")):
            self.assertEqual(commands_frame.cmd_palette(
                SimpleNamespace(client="/dev/ttys7", pane=True)), 0)
        self.assertEqual(state.workspace_for(self.FID), "zebra-ui")
        self.assertNotEqual(state.version(self.FID), was)
        self.assertIn("workspace → zebra-ui", self.said.call_args[0][1])

    def test_a_persona_typed_at_the_top_level_takes_the_same_route(self):
        with mock.patch.object(palette, "own_the_tty", _pick("forge")):
            self.assertEqual(commands_frame.cmd_palette(
                SimpleNamespace(client="", pane=True)), 0)
        self.assertEqual(switch.current_persona(self.FID), "forge")
        self.assertIn("persona → forge", self.said.call_args[0][1])

    def test_the_doorway_still_opens_a_picker_and_switching_from_it_still_works(self):
        """**The constraint that is not negotiable, asserted after the change**: browsing
        without knowing the name is the case the doorways exist for, and the route through
        them is untouched.

        Nothing is typed, and row 0 is pressed twice: with an empty query that is the
        workspace doorway, and inside the picker it opens it is the first workspace. The
        frame is moved off `alpha` first so that landing there is a real switch rather than
        a no-op that a broken doorway would also produce.
        """
        state.record_workspace(self.FID, "zeb-api")
        was = state.version(self.FID)
        with mock.patch.object(palette, "own_the_tty", _pick("", index=0)):
            self.assertEqual(commands_frame.cmd_palette(
                SimpleNamespace(client="", pane=True)), 0)
        self.assertEqual(state.workspace_for(self.FID), "alpha")
        self.assertNotEqual(state.version(self.FID), was)
        self.assertIn("workspace → alpha", self.said.call_args[0][1])


class TheRosterIsNeverReadUntilSomethingIsTyped(_Frame, unittest.TestCase):
    """**The cost promise, pinned.** Nothing else in this file would notice a palette that
    enumerated the plane eagerly and merely hid the rows — it would draw identically.

    Measured on `switch.workspaces` and `switch.personas`, which is what `choose.names_of`
    asks and therefore what "the roster is read" means. Both are directory globs off the
    plane, once per call.
    """

    def setUp(self) -> None:
        super().setUp()
        self.ws = self.enterContext(mock.patch.object(
            switch, "workspaces", side_effect=lambda: list(self.WORKSPACES)))
        self.pe = self.enterContext(mock.patch.object(
            switch, "personas", side_effect=lambda: list(self.PERSONAS)))

    def _reads(self) -> int:
        return self.ws.call_count + self.pe.call_count

    def test_opening_the_palette_reads_no_roster_at_all(self):
        """`F2` on a plane with forty workspaces costs what it cost with none."""
        self._surface()
        self.assertEqual(self._reads(), 0)

    def test_pressing_an_action_with_nothing_typed_reads_no_roster_either(self):
        """**The whole of the case this protects**: the operator opened the palette to
        detach, and never asked a question about names at all. The query stays empty
        through the whole of `_draw_palette`."""
        with mock.patch.object(palette, "own_the_tty",
                               _pick("", want="frame.detach")), \
             mock.patch.object(builtin_actions, "_spawn") as spawn:
            commands_frame.cmd_palette(SimpleNamespace(client="", pane=True))
        spawn.assert_called_once()
        self.assertEqual(self._reads(), 0)

    def test_the_first_keystroke_is_what_reads_it(self):
        surface = self._surface()
        self.assertEqual(self._reads(), 0)
        surface.handle(overlay.Event(overlay.KEY, "z"), 24)
        self.assertEqual((self.ws.call_count, self.pe.call_count), (1, 1))

    def test_every_keystroke_after_the_first_reads_nothing_more(self):
        """Once per palette, not once per keystroke: `zebra` is one glob of `workspaces/`,
        not five. `Palette._found` is what makes that true and this is what would go red
        if it were dropped."""
        self._typed("zebra")
        self.assertEqual((self.ws.call_count, self.pe.call_count), (1, 1))

    def test_backspacing_to_an_empty_query_and_typing_again_reads_nothing_more(self):
        surface = self._typed("z")
        for _ in range(3):
            surface.handle(overlay.Event(overlay.KEY, "backspace"), 24)
            surface.handle(overlay.Event(overlay.KEY, "z"), 24)
        self.assertEqual(surface.query, "z")
        self.assertEqual((self.ws.call_count, self.pe.call_count), (1, 1))

    def test_an_empty_query_hides_the_names_it_already_read_rather_than_re_reading(self):
        """Backspacing back to nothing returns the list to the doorways and the actions.
        A palette that kept showing what it had gathered would make the top level depend
        on what had been typed a moment ago."""
        surface = self._typed("z")
        surface.handle(overlay.Event(overlay.KEY, "backspace"), 24)
        self.assertEqual([r.id for r in surface.rows if _NAME_ROW in r.id], [])

    def test_the_doorway_reuses_the_roster_typing_already_read(self):
        """One roster per noun per palette — `commands_frame._roster`. Two reads of one
        noun are two lists that agree only while the plane holds still, and `_chosen_name`
        answers from whichever was appended first."""
        opened: list = []
        commands_frame._name_rows(self.FID, opened)
        doorway = next(r for r in choose.open_rows(self.FID)
                       if choose.noun_of(r) == choose.WORKSPACE)
        commands_frame._picker(doorway, self.FID, opened)
        self.assertEqual((self.ws.call_count, self.pe.call_count), (1, 1))
        self.assertEqual([r.noun for r in opened], [choose.WORKSPACE, choose.PERSONA])


class FindingNothingIsAnANSWERAndNotAMissingOne(unittest.TestCase):
    """`Palette.query_only` answering with no rows is remembered, like any other answer.

    **A survivor of this task's own hand sweep, and it is not equivalent.** The memo asks
    `self._found is None` — "has it been asked yet" — and the obvious-looking `if not
    self._found` reads the same on a plane that has names and differently on one that does
    not: an empty tuple is falsy, so every further keystroke would ask again. Nothing above
    would notice, because `switch.workspaces` folds `default` in and therefore never
    answers with nothing.

    Asserted on a constructed `Palette` rather than through `_draw_palette`, because that
    is where the attribute's contract lives: this class is a general surface, and a caller
    whose rows are gathered from somewhere with nothing in it is an ordinary one.
    """

    def test_a_query_only_that_finds_nothing_is_still_only_asked_once(self):
        asked: list = []
        p = palette.Palette(catalogue=(overlay.Row(id="a.b", title="ship it"),),
                            query_only=lambda: asked.append(1) or ())
        for ch in "beta":
            p.handle(overlay.Event(overlay.KEY, ch), 24)
        self.assertEqual(len(asked), 1, "an empty answer was re-asked on every keystroke")

    def test_and_it_is_not_asked_at_all_until_something_is_typed(self):
        asked: list = []
        p = palette.Palette(catalogue=(overlay.Row(id="a.b", title="ship it"),),
                            query_only=lambda: asked.append(1) or ())
        self.assertEqual(asked, [])
        p.handle(overlay.Event(overlay.KEY, "backspace"), 24)
        self.assertEqual(asked, [], "backspace on an empty query gathered")


class ANameIsStillNotAnAction(_Frame, unittest.TestCase):
    """**The cost Task 6 removed, and it stays removed.** An `Action` promises
    fire-and-report work; a row promises a string on a screen."""

    def test_the_registry_does_not_grow_with_the_plane(self):
        """Forty workspaces and forty personas are eighty rows and zero offers. This is
        the mutation that would undo Task 6 while every display assertion above stayed
        green."""
        reg = builtin_actions.build(self.FID, current_density="full")
        before = len(reg.offers(fid=self.FID, snapshot={}))
        _plane_workspaces(*[f"ws{i:02d}" for i in range(40)])
        _plane_personas(*[f"pe{i:02d}" for i in range(40)])
        reg = builtin_actions.build(self.FID, current_density="full")
        self.assertEqual(len(reg.offers(fid=self.FID, snapshot={})), before)
        self.assertGreaterEqual(len(self._typed("0").rows), 8)

    def test_listing_a_name_starts_no_process(self):
        """`Action.run` is what spawns, and a name row has none. Asserted against
        `builtin_actions._spawn`, which is the one place a palette row becomes a second
        charter."""
        with mock.patch.object(builtin_actions, "_spawn") as spawn:
            self._typed("zeb")
        spawn.assert_not_called()

    def test_no_id_a_name_row_carries_could_ever_be_an_action_id(self):
        """`_draw_palette` dispatches on the id of whatever the surface answered with, and
        a name row now sits in the same list as the actions. If one of these ids were
        spellable as an action id, a provider shipping it would take the keypress —
        `frame/choose.py`'s `:` is what keeps the two namespaces apart."""
        ids = [r.id for r in self._typed("zeb").rows if _NAME_ROW in r.id]
        self.assertTrue(ids)
        for rid in ids:
            self.assertFalse(component.usable_id(rid), rid)

    def test_the_filter_ignores_a_row_id_that_is_not_an_action_id(self):
        """The other side of that: a name row's id is charter's own counter, so matching
        it would make `n` list every name on the plane and `persona` list every persona.

        Asserted at both ends — the rule in `palette.matches`, and the consequence in the
        drawn list — because the rule alone would pass a `matches` that had been narrowed
        to the title and lost `acme.deploy` with it.
        """
        self.assertTrue(palette.matches("acme", overlay.Row(id="acme.deploy", title="x")))
        self.assertFalse(palette.matches("n0", overlay.Row(id="workspace:n0", title="x")))
        self.assertEqual([r.id for r in self._typed("n1").rows], [])

    def test_the_kind_label_is_a_label_and_never_a_search_term(self):
        """Typing `persona` finds the doorway that says so, not every persona the plane
        has. The kind is in the note, and the note is the one column `matches` refuses."""
        ids = [r.id for r in self._typed("persona").rows]
        self.assertEqual(ids, ["pick:persona"], ids)


class APinnedNounListsItsNamesWithTheReason(_Frame, unittest.TestCase):
    """#512, one surface along: an operator cannot ask about an option they cannot see.

    The doorway refuses to OPEN a picker for a pinned noun — a pane of names none of which
    can be switched to is an offer charter knows it cannot honour — but a name that has
    already been typed is a question the operator asked, and an empty pane is the wrong
    answer to it. So the row is listed and it carries the sentence.
    """

    PIN = {"CHARTER_WORKSPACE": "alpha", "CHARTER_PERSONA": ""}

    def test_the_names_are_listed_and_every_one_says_why_it_cannot_run(self):
        rows = [r for r in self._typed("zeb").rows if r.id.startswith("workspace:")]
        self.assertTrue(rows)
        for row in rows:
            self.assertIn("$CHARTER_WORKSPACE pins this frame to 'alpha'", row.note)

    def test_the_other_noun_keeps_its_kind_label(self):
        """One pin, one noun. Reading either as "this frame cannot switch anything" would
        take the persona names away for a reason that is not about personas."""
        rows = [r for r in self._typed("zeb").rows if r.id.startswith("persona:")]
        self.assertEqual([r.note for r in rows], ["persona"])

    def test_pressing_one_says_the_same_sentence_and_moves_nothing(self):
        """The row said why before the keypress; the keypress says it again where the
        operator is looking, because the pane the row was drawn in is about to be killed.
        Both come from one read of `state.identity`, so they cannot disagree."""
        with mock.patch.object(palette, "own_the_tty", _pick("zebra")):
            self.assertEqual(commands_frame.cmd_palette(
                SimpleNamespace(client="", pane=True)), 0)
        self.assertEqual(state.workspace_for(self.FID), "alpha")
        self.said.assert_called_once()
        self.assertIn("$CHARTER_WORKSPACE pins this frame", self.said.call_args[0][1])

    def test_the_pin_is_contained_before_it_becomes_a_note_and_a_status_line(self):
        """`$CHARTER_WORKSPACE` is whatever the launching environment held, so a newline in
        it is a committed value on two surfaces: eighty row notes, and one
        `display-message`. `choose.pin_reason` contains it once, for both."""
        state.record_identity(self.FID, {"CHARTER_WORKSPACE": "al\npha\u2028x"})
        for line in self._typed("zeb").render(60, 12):
            for bad in _SEPARATORS:
                self.assertNotIn(bad, line, repr(line))
        with mock.patch.object(palette, "own_the_tty", _pick("zebra")):
            commands_frame.cmd_palette(SimpleNamespace(client="", pane=True))
        said = self.said.call_args[0][1]
        self.assertEqual(said, "".join(said.splitlines()), repr(said))


class AHostileNameFoundByTypingIsOneRowAndRunsNothing(_Frame, unittest.TestCase):
    """**Containment, on the new route, without a second containment.**

    `overlay.Surface.render` runs `contain.one_line` over every title and note *before*
    `tui.width` sees them (#472). Task 6 deliberately added no second call on the way into
    a row, because a second one would be masked by this one and no test could go red over
    its deletion — the shape this repository has now been bitten by six times. So this
    class asserts the property on the DRAWN PANE reached by typing, and asserts that the
    one call is enough.

    The names are injected through the listers rather than written to disk, because those
    two already refuse a name outside `workspace.valid_name` — a real defence, asserted in
    `tests/test_frame_pickers.py`, and also exactly what would make this class test nothing
    if it were the only line.
    """

    #: Narrow enough that the two-column layout has to squeeze, so the width arithmetic is
    #: exercised rather than trivially satisfied.
    SIZE = (44, 20)

    FIND, NAMES = _FIND, _FINDABLE

    def _hostile(self) -> palette.Palette:
        self.enterContext(mock.patch.object(
            switch, "workspaces", return_value=["alpha", *self.NAMES]))
        self.enterContext(mock.patch.object(
            switch, "personas", return_value=list(self.NAMES)))
        return self._typed(self.FIND)

    def test_every_hostile_name_is_its_own_row_and_the_pane_is_the_pane(self):
        surface = self._hostile()
        self.assertEqual(len(self._names(surface)), 2 * len(HOSTILE))
        drawn = surface.render(*self.SIZE)
        self.assertEqual(len(drawn), self.SIZE[1])
        for line in drawn:
            self.assertEqual(line, "".join(line.splitlines()), repr(line))

    def test_no_hostile_byte_reaches_the_pane(self):
        """Per LINE rather than on a join of them, so the separator this is about cannot
        be one the test itself put there."""
        for line in self._hostile().render(*self.SIZE):
            for bad in _SEPARATORS + ("\x1b[31m",):
                self.assertNotIn(bad, line, repr(line))

    def test_the_column_arithmetic_sees_the_contained_name_not_the_raw_one(self):
        """#472 exactly, now with a note beside the title: the kind label shares the row
        with a name whose raw form is two lines long, and `_title_width` sizes from what
        `contain.one_line` already made one line of."""
        long_hostile = self.FIND + "z" * 30 + "\u2028" + "y" * 30
        self.enterContext(mock.patch.object(
            switch, "workspaces", return_value=["alpha", long_hostile]))
        self.enterContext(mock.patch.object(switch, "personas", return_value=[]))
        for line in self._typed(self.FIND).render(*self.SIZE):
            self.assertLessEqual(tui.width(tui.strip_ansi(line)), self.SIZE[0],
                                 repr(line))

    def test_the_raw_name_is_what_reaches_the_switch_and_the_switch_refuses_it(self):
        """"Runs nothing" is the other half, and the RAW name is what must arrive:
        repairing it on the way in would be switching to a name charter never looked at.
        `switch.to_workspace` is the one place it is checked.

        The row is named by ID rather than by position, because how many doorways and
        actions a query happens to match is not what this is about.
        """
        for name in self.NAMES:
            with self.subTest(name=name):
                self.said.reset_mock()
                row_id = choose.NAME_ID.format(choose.WORKSPACE, 1)
                with mock.patch.object(switch, "workspaces",
                                       return_value=["alpha", name]), \
                     mock.patch.object(switch, "personas", return_value=[]), \
                     mock.patch.object(palette, "own_the_tty",
                                       _pick(self.FIND, want=row_id)):
                    self.assertEqual(commands_frame.cmd_palette(
                        SimpleNamespace(client="", pane=True)), 0)
                self.said.assert_called_once()
                said = self.said.call_args[0][1]
                self.assertEqual(len(said.splitlines()), 1, repr(said))
                self.assertEqual(said, "".join(said.splitlines()), repr(said))
                self.assertEqual(state.workspace_for(self.FID), "alpha")
        self.assertEqual(sorted(p.name for p in config.WORKSPACES_DIR.iterdir()),
                         sorted(self.WORKSPACES), "a refused switch created something")

    def test_the_one_containment_is_the_surfaces_own_and_a_second_would_be_dead(self):
        """**The line this class refuses to ask for.** A `contain.one_line` on the way into
        a name row would sit under `Surface.render`'s, which runs over every title and note
        unconditionally — so deleting the inner one changes no pane, reddens no test, and
        is a line the deletion sweep would find and nobody could defend.

        Asserted as the fact that makes it dead: what a row HOLDS is raw, and what is drawn
        is contained. A future edit that starts repairing rows on the way in breaks the
        first half here rather than being noticed by nothing.
        """
        self.enterContext(mock.patch.object(
            switch, "workspaces", return_value=[self.FIND + "a\nb"]))
        self.enterContext(mock.patch.object(switch, "personas", return_value=[]))
        surface = self._typed(self.FIND)
        row, = self._names(surface)
        self.assertIn("\n", row.title,
                      "the row was repaired on the way in, which makes render's call dead")
        self.assertNotIn("\n", "".join(surface.render(*self.SIZE)))


if __name__ == "__main__":                          # pragma: no cover
    unittest.main()
