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

from charter import commands_frame, config, tui, workspace
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
        roster = choose.roster(choose.WORKSPACE, self.FID)
        self.assertEqual([r.title for r in roster.rows],
                         ["* alpha", "  beta", "  default"])

    def test_choosing_a_row_switches_the_frame_and_bumps_it_so_panels_repaint(self):
        roster = choose.roster(choose.WORKSPACE, self.FID)
        was = state.version(self.FID)
        row = next(r for r in roster.rows if roster.name_of(r) == "beta")
        out = choose.switch_to(roster.noun, self.FID, roster.name_of(row))
        self.assertTrue(out.ok, out.message)
        self.assertEqual(state.workspace_for(self.FID), "beta")
        self.assertNotEqual(state.version(self.FID), was)

    def test_a_persona_picker_is_the_same_thing_one_noun_over(self):
        roster = choose.roster(choose.PERSONA, self.FID)
        self.assertEqual([r.title for r in roster.rows], ["  forge", "  scribe"])
        was = state.version(self.FID)
        out = choose.switch_to(choose.PERSONA, self.FID, "scribe")
        self.assertTrue(out.ok, out.message)
        self.assertEqual(switch.current_persona(self.FID), "scribe")
        self.assertNotEqual(state.version(self.FID), was)

    def test_the_name_a_row_stands_for_is_matched_by_ID_and_never_by_TITLE(self):
        """The title carries `choose.MARK` and a name `overlay.Surface.render` contains
        before drawing, so the string on screen is not the string to switch to. A surface
        that matched on what it drew would switch to a repaired name or to nothing."""
        roster = choose.roster(choose.WORKSPACE, self.FID)
        for row, name in zip(roster.rows, roster.names):
            self.assertEqual(roster.name_of(row), name)
            self.assertNotEqual(row.title, name)
        self.assertIsNone(roster.name_of(overlay.Row(id="not:mine", title="alpha")))


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
                                 "pick:persona": choose.PERSONA})
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

    def test_choosing_a_name_moves_the_frame_and_says_what_it_did(self):
        was = state.version(self.FID)
        self.assertEqual(self._draw(self._doorway(choose.WORKSPACE),
                                    self._row(choose.WORKSPACE, "beta")), 0)
        self.assertEqual(state.workspace_for(self.FID), "beta")
        self.assertNotEqual(state.version(self.FID), was)
        self.said.assert_called_once()
        self.assertIn("workspace → beta", self.said.call_args[0][1])
        self.assertEqual(self.said.call_args[0][2], "/dev/ttys7")

    def test_a_persona_name_takes_the_same_route(self):
        self.assertEqual(self._draw(self._doorway(choose.PERSONA),
                                    self._row(choose.PERSONA, "scribe")), 0)
        self.assertEqual(switch.current_persona(self.FID), "scribe")
        self.assertIn("persona → scribe", self.said.call_args[0][1])

    def test_leaving_the_picker_without_choosing_moves_nothing(self):
        """Escape in the picker cancels the whole palette — `Surface.run` answers ``None``
        and there is nothing above it to go back to. Nothing moves and nothing is said."""
        self.assertEqual(self._draw(self._doorway(choose.WORKSPACE), None), 0)
        self.assertEqual(state.workspace_for(self.FID), "alpha")
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
        doorway = self._doorway(choose.WORKSPACE)
        surface = commands_frame._picker(doorway, self.FID, opened)
        self.assertTrue(surface.mouse, "a click in the picker would do nothing")
        header = surface.render(60, 10)[0]
        self.assertIn(choose.WORKSPACE, tui.strip_ansi(header), header)
        self.assertEqual(opened[0].noun, choose.WORKSPACE)

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
        """The race a picker actually has, staged as the race: `beta` is on the plane when
        the picker draws its rows and gone by the time `to_workspace` re-reads them.

        The two answers are scripted on `switch.workspaces` in the order the two callers
        ask — the roster first, the switch second — rather than by deleting a directory,
        because the property is that the switch re-checks at all. `to_workspace` refuses an
        unknown name rather than creating one, and the refusal is what lands on the screen.
        """
        doorway = next(r for r in choose.open_rows(self.FID)
                       if choose.noun_of(r) == choose.WORKSPACE)
        listed = ["alpha", "beta", "default"]
        row = overlay.Row(id=choose.NAME_ID.format(choose.WORKSPACE, 1), title="  beta")
        with mock.patch.object(switch, "workspaces",
                               side_effect=[listed, ["alpha", "default"]]):
            self.assertEqual(self._draw(doorway, row), 0)
        self.assertEqual(state.workspace_for(self.FID), "alpha")
        self.said.assert_called_once()
        self.assertIn("no workspace 'beta'", self.said.call_args[0][1])

    def test_overriding_the_session_lock_is_said_out_loud_rather_than_silently(self):
        """`workspace.set_active` locks the session to what it selected, so an agent's
        `charter workspace use` mid-task is exactly what the lock is for — and a keypress
        on the picker overrides it, because the operator is at the keyboard. What must not
        happen is the override being silent: the agent's next command would act on a
        workspace nobody was told about."""
        workspace.set_active("alpha", session_id=self.FID)
        roster = choose.roster(choose.WORKSPACE, self.FID)
        row = next(r for r in roster.rows if roster.name_of(r) == "beta")
        doorway = next(r for r in choose.open_rows(self.FID)
                       if choose.noun_of(r) == choose.WORKSPACE)
        self.assertEqual(self._draw(doorway, row), 0)
        self.assertEqual(state.workspace_for(self.FID), "beta")
        said = self.said.call_args[0][1]
        self.assertIn("lock moved from 'alpha'", said)


class APinnedFrameIsToldBeforeItPressesAnything(_Frame, unittest.TestCase):
    """Task 4's rule, kept: the row is listed, it says why, and it opens nothing.

    `$CHARTER_WORKSPACE` was set at launch, so it is in every panel pane's environment for
    as long as the pane lives and nothing charter writes outranks it. A picker full of
    names none of which can be switched to would be an offer charter already knows it
    cannot honour.
    """

    PIN = {"CHARTER_WORKSPACE": "alpha", "CHARTER_PERSONA": ""}

    def test_the_doorway_carries_the_reason(self):
        row = next(r for r in choose.open_rows(self.FID)
                   if choose.noun_of(r) == choose.WORKSPACE)
        self.assertIn("cannot switch: $CHARTER_WORKSPACE pins this frame", row.note)
        self.assertIn("'alpha'", row.note)

    def test_the_other_noun_is_not_pinned_by_the_first_ones_pin(self):
        """One pin per noun. Reading either as "this frame cannot switch anything" would
        take the persona picker away for a reason that is not about personas."""
        row = next(r for r in choose.open_rows(self.FID)
                   if choose.noun_of(r) == choose.PERSONA)
        self.assertEqual(row.note, "")

    def test_pressing_it_opens_no_picker_and_the_reason_reaches_the_screen(self):
        row = next(r for r in choose.open_rows(self.FID)
                   if choose.noun_of(r) == choose.WORKSPACE)
        opened = []
        self.assertIsNone(commands_frame._picker(row, self.FID, opened))
        self.assertEqual(opened, [])
        with mock.patch.object(palette, "own_the_tty", _pane(row)):
            self.assertEqual(commands_frame.cmd_palette(
                SimpleNamespace(client="/dev/ttys7", pane=True)), 0)
        self.said.assert_called_once()
        self.assertIn("$CHARTER_WORKSPACE pins this frame", self.said.call_args[0][1])
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
