"""The pickers: one overlay, a different row source, and the switch that repaints.

Phase 2, Task 6. `frame/choose.py` is `frame/overlay.py` over a list of NAMES instead of
a list of actions — the seam Task 4 named in as many words when it wrote
`palette.rows(offers)` beside `Palette(catalogue=…)`.

Four properties get most of this file's length, because each is a rule the plan states
and a first implementation loses:

* **A workspace picker lists workspaces, and switching repaints every panel against the
  new plane.** That second half is the whole of #411: a switch that writes a pointer some
  panels may read is the bug, and one that moves the frame's own identity and BUMPS it is
  the fix. Every case that chooses a name here asserts the version moved.
* **A hostile name renders as ONE row and runs nothing.** Measured against real hostile
  names rather than reasoned about (#472), and measured on the drawn pane rather than on
  the row, because "one row" is a property of what reaches the terminal.
* **A refused switch says so on screen.** #517: "a menu that silently fails against a lock
  is worse than no menu." Both refusals a picker can produce are asserted — the launch pin
  before the picker opens, and a name that stopped existing after it did — and so is the
  lock override, which succeeds and still has to name what it overrode.
* **The picker is drawn in the palette's OWN pane.** `palette.own_the_tty`'s *then*, which
  is what makes that true, is pinned against a real tty here; the whole chain from a real
  `F2` to a moved frame is `tests/test_frame_palette_integration.py`.
"""

from __future__ import annotations

import os
import pty
import unittest
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame, config, persona, tui, workspace
from charter.frame import choose, component, overlay, palette, state, switch

from tests._isolation import PersonaIso

#: Names a filesystem accepts and charter did not mint. Every one of them is a committed
#: value in the sense `contain.py` means: a directory somebody can add in a commit.
#:
#: `\n` and `\r` forge a second row; U+2028 is the separator #472 was filed over and
#: `str.splitlines` honours it where `split("\n")` does not; the CSI turns the rest of the
#: row a colour and can move the cursor; `"` and `#` are what a tmux command line and a
#: tmux FORMAT read as structure.
HOSTILE = ("line\nbreak",
           "car\rriage",
           "sep\u2028arator",
           "para\u2029graph",
           "next\u0085line",
           "esc\x1b[31mape",
           'quo"te ; run-shell "touch /tmp/pwned',
           "hash#{session_name}")


def _plane_workspaces(*names: str) -> None:
    for n in names:
        (config.WORKSPACES_DIR / n).mkdir(parents=True, exist_ok=True)


def _plane_personas(*names: str) -> None:
    for n in names:
        d = config.PERSONAS_DIR / n
        d.mkdir(parents=True, exist_ok=True)
        (d / "persona.md").write_text("# p\n")


class _Frame(PersonaIso):
    """One frame on an isolated plane, with nothing pinned and nothing on a screen."""

    FID = "f-pick"
    PIN = {"CHARTER_WORKSPACE": "", "CHARTER_PERSONA": ""}

    def setUp(self) -> None:
        super().setUp()
        self.enterContext(mock.patch.dict(
            os.environ, {"CHARTER_SESSION_ID": self.FID}, clear=True))
        _plane_workspaces("alpha", "beta")
        _plane_personas("forge", "scribe")
        state.frame_dir(self.FID, create=True)
        state.record_server(self.FID, "charter")
        state.record_harness_pane(self.FID, "%3")
        state.record_identity(self.FID, {"CHARTER_SESSION_ID": self.FID, **self.PIN})
        state.record_workspace(self.FID, "alpha")
        self.said = self.enterContext(
            mock.patch.object(commands_frame, "_say_on_screen"))
        self.enterContext(mock.patch.object(commands_frame, "_close_palette"))


class AWorkspacePickerListsWorkspaces(_Frame, unittest.TestCase):
    """Step 1, whole: the list, and what choosing off it does to the frame."""

    def test_the_picker_lists_every_workspace_with_the_one_in_use_marked(self):
        """The names, and the mark as a FIELD rather than as two characters on the front
        of one (#749). The title is what the operator typed to get here, so it has to be
        the name and nothing else — `frame/palette.exact` compares against it."""
        roster = choose.roster(choose.WORKSPACE, self.FID)
        self.assertEqual([(r.title, r.mark) for r in roster.rows],
                         [("alpha", True), ("beta", False), ("default", False)])

    def test_choosing_a_row_is_allowed_and_still_moves_no_chat(self):
        """Was `test_choosing_a_row_switches_the_frame_and_bumps_it_so_panels_repaint`,
        which asserted `state.workspace_for` following the keypress — the defect §4j
        forbids, stated as a requirement — and then
        `test_choosing_a_row_is_refused_and_moves_nothing`, which asserted #789's blanket
        refusal.

        §4b's answer is the third one and it is both: the row is CHOSEN, and the chat's
        workspace does not move, because what moves is the tmux client. No bump either —
        the version a workspace switch earns is `_apply_arrangement`'s, after the client
        has actually gone somewhere, and this call is a decision rather than an effect."""
        roster = choose.roster(choose.WORKSPACE, self.FID)
        was = state.version(self.FID)
        row = next(r for r in roster.rows if roster.name_of(r) == "beta")
        out = choose.switch_to(roster.noun, self.FID, roster.name_of(row))
        self.assertTrue(out.ok, out.message)
        self.assertEqual(state.workspace_for(self.FID), "alpha")
        self.assertEqual(state.version(self.FID), was)

    def test_a_persona_picker_is_the_same_thing_one_noun_over(self):
        roster = choose.roster(choose.PERSONA, self.FID)
        self.assertEqual([(r.title, r.mark) for r in roster.rows],
                         [("forge", False), ("scribe", False)])
        was = state.version(self.FID)
        out = choose.switch_to(choose.PERSONA, self.FID, "scribe")
        self.assertTrue(out.ok, out.message)
        self.assertEqual(switch.current_persona(self.FID), "scribe")
        self.assertNotEqual(state.version(self.FID), was)

    def test_the_name_a_row_stands_for_is_matched_by_ID_and_never_by_TITLE(self):
        """A row resolves through its id, and a row that merely LOOKS the same does not.

        The title is the bare name since #749, so "the string on screen differs from the
        string to switch to" is no longer the thing that would catch a title match — the
        difference is containment, and it only shows on a hostile name
        (`test_a_hostile_name_is_one_row_and_switches_to_the_raw_name`). What still has to
        hold, and what a title match would lose, is that a row this roster did not build
        stands for no name however it is spelled: `overlay.Surface.render` draws what it
        is given, and a foreign row carrying `alpha` must not switch this frame anywhere.
        """
        roster = choose.roster(choose.WORKSPACE, self.FID)
        for row, name in zip(roster.rows, roster.names):
            self.assertEqual(roster.name_of(row), name)
        self.assertIsNone(roster.name_of(overlay.Row(id="not:mine", title="alpha")))
        self.assertIsNone(roster.name_of(
            overlay.Row(id="workspace:n99", title="alpha", mark=True)))


class APickerRowIsNotAnAction(_Frame, unittest.TestCase):
    """The ids, and why they are spelled the way they are.

    `frame/action.py` holds every action id to `component._ID_RE`, and `_draw_palette`
    dispatches on the id of whatever the surface answered with. If a picker's ids used
    that alphabet, a provider shipping `pick.workspace` would take the keypress.
    """

    def test_no_id_this_module_mints_could_ever_be_an_action_id(self):
        ids = ([r.id for r in choose.open_rows(self.FID)]
               + [r.id for r in choose.roster(choose.WORKSPACE, self.FID).rows]
               + [r.id for r in choose.roster(choose.PERSONA, self.FID).rows])
        self.assertTrue(ids)
        for rid in ids:
            self.assertFalse(component.usable_id(rid),
                             f"{rid!r} is a usable action id, so a provider can collide "
                             f"with it and take the keypress")

    def test_the_doorway_is_recognised_and_nothing_else_is(self):
        opens = {r.id: choose.noun_of(r) for r in choose.open_rows(self.FID)}
        self.assertEqual(opens, {"pick:workspace": choose.WORKSPACE,
                                 "pick:persona": choose.PERSONA,
                                 "pick:change": choose.CHANGE,
                                 "pick:chat": choose.CHAT})
        for stranger in ("frame.detach", "density.full", "workspace:n0", "acme.deploy"):
            self.assertIsNone(choose.noun_of(overlay.Row(id=stranger, title="x")))


class AHostileNameIsOneRowAndRunsNothing(_Frame, unittest.TestCase):
    """Step 4, measured rather than reasoned about.

    The names are injected through `switch.workspaces`/`switch.personas` rather than
    written to disk, because those two listers already refuse a name outside
    `workspace.valid_name`/`persona.valid_name` — which is a real defence and is asserted
    below, and is also exactly what would make this file test nothing if it were the only
    one. A picker must hold the line for a lister that stops filtering.
    """

    #: A pane narrow enough that the two-column layout has to squeeze, so the width
    #: arithmetic is actually exercised rather than trivially satisfied.
    SIZE = (44, 12)

    def _drawn(self, names) -> list[str]:
        with mock.patch.object(switch, "workspaces", return_value=list(names)):
            roster = choose.roster(choose.WORKSPACE, self.FID)
        surface = palette.Palette(catalogue=roster.rows, label=choose.WORKSPACE)
        return surface.render(*self.SIZE)

    def test_the_lister_refuses_a_hostile_directory_name_outright(self):
        """The first line, and the reason the rest of this class injects rather than
        creates: a directory a commit added is not a workspace charter will offer."""
        for name in HOSTILE:
            (config.WORKSPACES_DIR / name.replace("/", "_")).mkdir(exist_ok=True)
        self.assertEqual(sorted(switch.workspaces()), ["alpha", "beta", "default"])

    def test_every_hostile_name_is_exactly_one_row(self):
        with mock.patch.object(switch, "workspaces",
                               return_value=["alpha", *HOSTILE]):
            roster = choose.roster(choose.WORKSPACE, self.FID)
        self.assertEqual(len(roster.rows), 1 + len(HOSTILE))
        self.assertEqual(len(set(r.id for r in roster.rows)), 1 + len(HOSTILE),
                         "two names sharing a row id is one name that cannot be chosen")

    def test_the_drawn_pane_is_exactly_as_tall_as_the_pane(self):
        """**The property is what reaches the terminal**, not what the row holds. A `Row`
        carries display text raw — `overlay.Row`'s own contract — and `Surface.render` is
        the one place it is contained, immediately before `tui.width` measures it (#472).
        A separator that survived to here would push the footer off the bottom and write a
        line of its own choosing where charter's rows go."""
        drawn = self._drawn(["alpha", *HOSTILE])
        self.assertEqual(len(drawn), self.SIZE[1])
        for line in drawn:
            self.assertNotIn("\n", line, repr(line))
            self.assertEqual(line, "".join(line.splitlines()), repr(line))

    def test_no_hostile_byte_reaches_the_pane(self):
        """Asserted per LINE rather than on a join of them, so the separator this test is
        about cannot be one the test itself put there."""
        for line in self._drawn(["alpha", *HOSTILE]):
            for bad in ("\n", "\r", "\u2028", "\u2029", "\u0085", "\x1b[31m"):
                self.assertNotIn(bad, line, repr(line))

    def test_the_column_arithmetic_sees_the_contained_name_not_the_raw_one(self):
        """#472 exactly: a table that sized its columns from a raw name. `tui.width` —
        never `len` — measures what `contain.one_line` already made one line of, so a name
        holding a separator cannot make the row wider than the pane."""
        long_hostile = "z" * 30 + "\u2028" + "y" * 30
        for line in self._drawn(["alpha", long_hostile]):
            self.assertLessEqual(tui.width(tui.strip_ansi(line)), self.SIZE[0], repr(line))

    def test_a_hostile_name_chosen_off_a_picker_switches_to_nothing(self):
        """"Runs nothing" is the other half. The RAW name is what reaches
        `switch.to_workspace` — repairing it on the way in would be switching to a name
        charter never looked at — and that is the one place it is checked and refused."""
        with mock.patch.object(switch, "workspaces",
                               return_value=["alpha", *HOSTILE]):
            roster = choose.roster(choose.WORKSPACE, self.FID)
            for row in roster.rows[1:]:
                name = roster.name_of(row)
                out = choose.switch_to(choose.WORKSPACE, self.FID, name)
                self.assertFalse(out.ok, f"{name!r} was switched to")
                self.assertEqual(len(out.message.splitlines()), 1, repr(out.message))
        self.assertEqual(state.workspace_for(self.FID), "alpha")
        self.assertEqual(sorted(p.name for p in config.WORKSPACES_DIR.iterdir()),
                         ["alpha", "beta"], "a refused switch created something")

    def test_a_hostile_PERSONA_name_is_refused_the_same_way(self):
        """**The twin, and it was not pinned before this case.** `switch.to_persona` has
        the same `shown = contain.one_line(name)` as `to_workspace`, reaching the same
        `_say_on_screen`; the workspace one is covered twice over and the persona one was
        covered nowhere — deleting it left the whole suite green. Task 6 step 4 says
        "workspace **and** persona", and a picker exists for both.
        """
        with mock.patch.object(switch, "personas", return_value=["forge", *HOSTILE]):
            roster = choose.roster(choose.PERSONA, self.FID)
            self.assertEqual(len(roster.rows), 1 + len(HOSTILE))
            for row in roster.rows[1:]:
                name = roster.name_of(row)
                out = choose.switch_to(choose.PERSONA, self.FID, name)
                self.assertFalse(out.ok, f"{name!r} was switched to")
                self.assertEqual(len(out.message.splitlines()), 1, repr(out.message))
                self.assertEqual(out.message, "".join(out.message.splitlines()),
                                 repr(out.message))
        self.assertIsNone(switch.current_persona(self.FID))


def _pane(*rows):
    """Stand in for the palette's pane: hand each row to *then* in turn, answer the last.

    A test double for `palette.own_the_tty`, and deliberately not a copy of its loop —
    that loop is pinned against a real tty by :class:`ThePickerIsDrawnInThePalettesOwnPane`
    below. What this stands in for is the OPERATOR: one row chosen per surface, in order.
    """
    def fake(surface, *, then=None, **kw):
        chosen = None
        for row in rows:
            chosen = row
            # `None` is "the operator left" and never reaches *then* — the real
            # `own_the_tty` guards it, and a double that did not would let a test pass
            # against a contract production does not keep.
            if row is None or then is None or then(row) is None:
                break
        return chosen
    return fake


class ThePaletteOpensThePickerAndActsOnWhatComesBack(_Frame, unittest.TestCase):
    """`_draw_palette`, with the pane faked out: a doorway row, then a name."""

    def _draw(self, *rows) -> int:
        with mock.patch.object(palette, "own_the_tty", _pane(*rows)):
            return commands_frame.cmd_palette(
                SimpleNamespace(client="/dev/ttys7", pane=True))

    def _row(self, noun: str, name: str) -> overlay.Row:
        roster = choose.roster(noun, self.FID)
        return next(r for r in roster.rows if roster.name_of(r) == name)

    def _doorway(self, noun: str) -> overlay.Row:
        return next(r for r in choose.open_rows(self.FID)
                    if choose.noun_of(r) == noun)

    def test_the_palette_is_built_with_a_doorway_for_each_noun_ahead_of_the_actions(self):
        """**What the pane is actually given**, asserted on the surface `_draw_palette`
        constructs rather than on rows a test handed back to it. Every other case in this
        class feeds `own_the_tty` a row directly, so none of them would notice the
        doorways never being put in the catalogue at all — measured: removing
        `choose.open_rows` from the catalogue reddened nothing outside the tmux-gated
        integration module, which skips on a machine with no tmux.

        The order is asserted with them first, so an accidental `F2`-then-Enter opens a
        list rather than detaching the harness.
        """
        seen = []

        def fake(surface, *, then=None, **kw):
            seen.append(surface)
            return None

        with mock.patch.object(palette, "own_the_tty", fake):
            self.assertEqual(commands_frame.cmd_palette(
                SimpleNamespace(client="", pane=True)), 0)
        ids = [r.id for r in seen[0].catalogue]
        self.assertEqual(ids[:2], ["pick:workspace", "pick:persona"], ids)
        self.assertIn("frame.detach", ids)

    def test_choosing_a_name_moves_the_frame_and_says_what_it_did(self):
        """The doorway-then-name route, whole. It was staged on the workspace noun until
        §4j made that one a refusal; every assertion is the same one noun over, which is
        where the property was always generic."""
        was = state.version(self.FID)
        self.assertEqual(self._draw(self._doorway(choose.PERSONA),
                                    self._row(choose.PERSONA, "scribe")), 0)
        self.assertEqual(switch.current_persona(self.FID), "scribe")
        self.assertNotEqual(state.version(self.FID), was)
        self.said.assert_called_once()
        self.assertIn("persona → scribe", self.said.call_args[0][1])
        # On THIS frame, and said as an outcome that happened. Since #729 the sentence is
        # a line in the frame's own state that its attention panel draws, so the argument
        # after the message names the frame rather than a client's terminal — a persona
        # switch moves this frame in place, so the frame it is filed against is this one.
        self.assertEqual(self.said.call_args[0][0], self.FID)
        self.assertIs(self.said.call_args[1]["ok"], True)

    # The workspace twin of the case above is `test_choosing_a_row_is_allowed_and_still_
    # moves_no_chat`, which asserts the half that differs — the chat does not move — and
    # deliberately not the switch itself: a workspace switch is a tmux client moved
    # between sessions, and `tests/test_a_workspace_switch_moves_the_client.py` is where
    # that is measured against a server.

    def test_leaving_the_picker_without_choosing_moves_nothing(self):
        """Escape in the picker cancels the whole palette — `Surface.run` answers ``None``
        and there is nothing above it to go back to. Nothing moves and nothing is said."""
        self.assertEqual(self._draw(self._doorway(choose.PERSONA), None), 0)
        self.assertIsNone(switch.current_persona(self.FID))
        self.said.assert_not_called()

    def test_the_picker_says_which_noun_it_is_and_still_takes_a_click(self):
        """Two things the surface is built with, and neither is cosmetic.

        The heading is the only thing on a picker's screen that says what the names ARE —
        the rows are bare names, deliberately, and a list of unlabelled words one keypress
        after a list of actions is a pane an operator has to guess at. And `mouse` is one
        declaration in `overlay.Surface`: it both asks the terminal for pointer reports and
        decides whether one is acted on, so a picker built without it would be the one
        surface in this frame where a click does nothing.
        """
        opened = []
        doorway = self._doorway(choose.PERSONA)
        surface = commands_frame._picker(doorway, self.FID, opened)
        self.assertTrue(surface.mouse, "a click in the picker would do nothing")
        header = surface.render(60, 10)[0]
        self.assertIn(choose.PERSONA, tui.strip_ansi(header), header)
        self.assertEqual(opened[0].noun, choose.PERSONA)

    def test_an_action_row_still_goes_through_invoke(self):
        """The other branch, so the dispatch above cannot pass by swallowing everything:
        a row that is not a doorway and not a name is an action and is started."""
        with mock.patch.object(commands_frame.builtin_actions, "_spawn") as spawn:
            self.assertEqual(self._draw(overlay.Row(id="frame.detach", title="d")), 0)
        spawn.assert_called_once()


class ARefusedSwitchSaysSoOnScreen(_Frame, unittest.TestCase):
    """Step 6. #517: "a menu that silently fails against a lock is worse than no menu."

    Every one of these ends with the palette's pane killed, so the sentence has nowhere
    else to go — `commands_frame._say_on_screen` is the whole of "on screen" here, and it
    is asserted rather than the tmux call it makes (`test_frame_palette.py` owns that).
    """

    def _draw(self, *rows) -> int:
        with mock.patch.object(palette, "own_the_tty", _pane(*rows)):
            return commands_frame.cmd_palette(
                SimpleNamespace(client="/dev/ttys7", pane=True))

    def test_a_name_that_stopped_existing_is_refused_and_the_frame_says_so(self):
        """The race a picker actually has, staged as the race: `scribe` is on the plane
        when the picker draws its rows and gone by the time `to_persona` re-reads them.

        The two answers are scripted on `switch.personas` in the order the two callers
        ask — the roster first, the switch second — rather than by deleting a directory,
        because the property is that the switch re-checks at all. `to_persona` refuses an
        unknown name rather than creating one, and the refusal is what lands on the screen.

        Staged on the workspace noun until §4j; the re-check it measures belongs to
        whichever nouns still perform, and this is the one that does.
        """
        doorway = next(r for r in choose.open_rows(self.FID)
                       if choose.noun_of(r) == choose.PERSONA)
        row = overlay.Row(id=choose.NAME_ID.format(choose.PERSONA, 1), title="  scribe")
        with mock.patch.object(switch, "personas",
                               side_effect=[["forge", "scribe"], ["forge"]]):
            self.assertEqual(self._draw(doorway, row), 0)
        self.assertIsNone(switch.current_persona(self.FID))
        self.said.assert_called_once()
        self.assertIn("no persona 'scribe'", self.said.call_args[0][1])

    def test_the_session_lock_is_left_standing_and_the_escape_is_named(self):
        """Was `test_overriding_the_session_lock_is_said_out_loud_rather_than_silently`,
        and the thing it measured no longer exists: `switch.to_workspace` overrode the
        lock with `force=True` because a keypress on the picker *is* the operator. §4j
        removed the move, so there is nothing to force.

        What must not have gone with it is the escape. `_pin_workspace`'s own argument for
        taking the lock at launch was that the frame has a way out, and it named `F2 →
        workspace` first — so the property asserted here is the one that argument now
        rests on: the lock a launch or an agent took is still standing after the keypress,
        untouched, and `charter workspace unlock` is what releases it. Silently moving it
        would be worse than either: the agent's next command would act on a workspace
        nobody was told about."""
        workspace.set_active("alpha", session_id=self.FID)
        roster = choose.roster(choose.WORKSPACE, self.FID)
        row = next(r for r in roster.rows if roster.name_of(r) == "beta")
        out = choose.switch_to(roster.noun, self.FID, roster.name_of(row))
        self.assertTrue(out.ok, out.message)
        self.assertEqual(state.workspace_for(self.FID), "alpha")
        self.assertEqual(workspace.is_locked(self.FID), "alpha")


class APinnedFrameIsToldBeforeItPressesAnything(_Frame, unittest.TestCase):
    """Task 4's rule, kept: the row is listed, it says why, and it opens nothing.

    `$CHARTER_PERSONA` was set at launch, so it is in every panel pane's environment for
    as long as the pane lives and nothing charter writes outranks it. A picker full of
    names none of which can be switched to would be an offer charter already knows it
    cannot honour.

    **It was the WORKSPACE noun until §4j, and had to move rather than be copied.** A
    workspace doorway carried #789's blanket refusal on every frame, pinned or not, so a
    pin-shaped assertion on it would have stayed green with the pin taken away — measuring
    nothing, which is the failure this repository distrusts most. §4b removed that refusal
    and did **not** put the pin back (`choose.PIN` says why: the pin is about which
    workspace this CHAT is in, and a switch moves a client), so the workspace doorway now
    has no reason at all and the persona is the only noun a launch pin still decides.
    """

    PIN = {"CHARTER_WORKSPACE": "", "CHARTER_PERSONA": "forge"}

    def test_the_doorway_carries_the_reason(self):
        row = next(r for r in choose.open_rows(self.FID)
                   if choose.noun_of(r) == choose.PERSONA)
        self.assertIn("cannot switch: $CHARTER_PERSONA pins this frame", row.note)
        self.assertIn("'forge'", row.note)

    def test_the_other_noun_is_not_pinned_by_the_first_ones_pin(self):
        """One pin per noun. Reading a pin as "this frame cannot switch anything" would
        take another picker away for a reason that is not about it.

        Both halves, because either alone can be right by accident: the workspace row
        carries no reason at all, and in particular not one naming the persona's own
        variable on a row that is not about personas."""
        row = next(r for r in choose.open_rows(self.FID)
                   if choose.noun_of(r) == choose.WORKSPACE)
        self.assertFalse(row.refused)
        self.assertNotIn("CHARTER_PERSONA", row.note)

    def test_the_pin_is_contained_before_it_becomes_a_line_on_a_status_area(self):
        """**The one containment in this branch that `overlay.Surface.render` does not
        already do**, and the sweep found it unpinned.

        `$CHARTER_PERSONA` is whatever the launching environment held, so
        `CHARTER_PERSONA=$'a\\nb' charter claude` puts a newline in `state.identity`. The
        reason built from it goes two places: a row's note, where `render` contains it —
        and, when the row is pressed, straight into `_say_on_screen`, whose docstring
        states the split in as many words: callers close the newline half with
        `contain.one_line`, `tmuxctl.inert_format` closes the `#` half, "both, because they
        are different properties: one line, and inert". `inert_format` does not touch a
        separator, so nothing downstream would.

        Asserted on what reaches `_say_on_screen`, not on the drawn pane — the pane would
        pass either way, which is exactly how this line came to have no test.
        """
        state.record_identity(self.FID, {"CHARTER_PERSONA": "al\npha\u2028x"})
        row = next(r for r in choose.open_rows(self.FID)
                   if choose.noun_of(r) == choose.PERSONA)
        with mock.patch.object(palette, "own_the_tty", _pane(row)):
            commands_frame.cmd_palette(SimpleNamespace(client="", pane=True))
        said = self.said.call_args[0][1]
        self.assertEqual(said, "".join(said.splitlines()), repr(said))
        for bad in ("\n", "\r", "\u2028", "\u2029", "\u0085"):
            self.assertNotIn(bad, said, repr(said))
        self.assertIn("\\x0a", said, repr(said))

    def test_pressing_it_opens_no_picker_and_the_reason_reaches_the_screen(self):
        row = next(r for r in choose.open_rows(self.FID)
                   if choose.noun_of(r) == choose.PERSONA)
        opened = []
        self.assertIsNone(commands_frame._picker(row, self.FID, opened))
        self.assertEqual(opened, [])
        with mock.patch.object(palette, "own_the_tty", _pane(row)):
            self.assertEqual(commands_frame.cmd_palette(
                SimpleNamespace(client="/dev/ttys7", pane=True)), 0)
        self.said.assert_called_once()
        self.assertIn("$CHARTER_PERSONA pins this frame", self.said.call_args[0][1])
        self.assertIsNone(persona.for_session(self.FID))

    def test_the_workspace_doorway_opens_a_picker_even_on_a_pinned_frame(self):
        """The negative control for the pin, and the case that would have caught #789's
        cost: a persona pin closes the persona doorway and must not close the workspace
        one. THIS is the class that measures the doorway path end to end, so the assertion
        is that `_picker` returns a surface and puts the roster in *opened* — #732's rule
        read the other way round."""
        row = next(r for r in choose.open_rows(self.FID)
                   if choose.noun_of(r) == choose.WORKSPACE)
        opened = []
        self.assertIsNotNone(commands_frame._picker(row, self.FID, opened))
        self.assertEqual([r.noun for r in opened], [choose.WORKSPACE])
        self.assertEqual(state.workspace_for(self.FID), "alpha")


class ThePickerIsDrawnInThePalettesOwnPane(unittest.TestCase):
    """`palette.own_the_tty`'s *then*, against a REAL tty.

    **This is what stops the picker being a second pane.** A palette row that started a
    second charter to split its own overlay pane would race `_close_palette`, which
    selects the harness, kills this pane and re-arms the escape hatch as ONE chained tmux
    command the instant a row has been chosen. Handing the next surface to the same loop
    has no interleaving to lose.
    """

    def setUp(self) -> None:
        self.master, self.slave = pty.openpty()
        self.addCleanup(self._close)
        self.out = open(os.devnull, "w")
        self.addCleanup(self.out.close)
        self.first = palette.Palette(catalogue=(overlay.Row(id="pick:workspace",
                                                            title="workspace"),))
        self.second = palette.Palette(catalogue=(overlay.Row(id="workspace:n1",
                                                             title="  beta"),))

    def _close(self) -> None:
        for fd in (self.master, self.slave):
            try:
                os.close(fd)
            except OSError:
                pass

    def _own(self, then):
        return palette.own_the_tty(self.first, fd=self.slave, out=self.out, then=then)

    def test_the_next_surface_runs_in_the_same_raw_mode_window_and_its_row_comes_back(self):
        import termios

        modes = []
        with mock.patch.object(self.first, "run",
                               side_effect=lambda **kw: (
                                   modes.append(termios.tcgetattr(self.slave)),
                                   self.first.rows[0])[1]), \
             mock.patch.object(self.second, "run",
                               side_effect=lambda **kw: (
                                   modes.append(termios.tcgetattr(self.slave)),
                                   self.second.rows[0])[1]) as second_run:
            got = self._own(lambda row: self.second if row.id == "pick:workspace"
                            else None)
        second_run.assert_called_once()
        self.assertEqual(got, self.second.rows[0],
                         "the row the PICKER answered with is what comes back")
        self.assertEqual(len(modes), 2)
        for mode in modes:
            self.assertFalse(mode[3] & termios.ECHO,
                             "the tty left raw mode between the two surfaces")

    def test_no_then_is_the_surface_answering_for_itself(self):
        """The default, and every caller that has one surface: the row comes straight
        back and nothing else runs."""
        with mock.patch.object(self.first, "run", return_value=self.first.rows[0]), \
             mock.patch.object(self.second, "run") as second_run:
            self.assertEqual(self._own(None), self.first.rows[0])
        second_run.assert_not_called()

    def test_a_cancel_never_reaches_then(self):
        """``None`` is "the operator left". Asking a doorway what to open next for a row
        that does not exist would be a `None` dereference on the one path that must not
        raise — the palette's pane is handed back in a `finally` either way, but a
        traceback there is charter printing into a pane it is about to kill."""
        asked = []
        with mock.patch.object(self.first, "run", return_value=None):
            self.assertIsNone(self._own(lambda row: asked.append(row)))
        self.assertEqual(asked, [])


if __name__ == "__main__":                          # pragma: no cover
    unittest.main()
