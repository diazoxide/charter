"""Switching a frame's workspace and persona, and picking one before it opens — #517/#518.

Both features are one mechanism — list the names, contain them, perform the switch,
repaint the panels — so they are tested against one set of properties:

* a switch moves the frame's OWN identity, not a pointer some panels may or may not read
  (#411/#412 are what "own identity" means here: `state.workspace_for`'s rungs);
* a switch that cannot take effect is REFUSED and says so, never reported as done;
* the switcher's own lock does not lock the switcher out of its second use;
* nothing creates a workspace except the one confirmed step in `cmd_launch`;
* a non-interactive launch never blocks;
* no name — a committed value — reaches a tmux command slot or forges a line.

`os.environ` is cleared in every case that resolves a workspace or a persona: both
ladders read `$CHARTER_WORKSPACE`/`$CHARTER_PERSONA` and `$CHARTER_SESSION_ID`, so a
developer running the suite inside a live frame would otherwise be supplying half of
every fixture (#519/#521/#528).
"""

from __future__ import annotations

import io
import json
import os
import unittest
from unittest import mock

from charter import cli, commands_frame, config, persona, tui, workspace
from charter.frame import builtin_actions, choose, palette, picker, state, switch

from tests._isolation import PersonaIso


def _plane_workspaces(*names: str) -> None:
    for n in names:
        (config.WORKSPACES_DIR / n).mkdir(parents=True, exist_ok=True)


def _plane_personas(*names: str) -> None:
    for n in names:
        d = config.PERSONAS_DIR / n
        d.mkdir(parents=True, exist_ok=True)
        (d / "persona.md").write_text("# p\n")


class SwitchingWorkspace(PersonaIso, unittest.TestCase):
    """`switch.to_workspace` — what moves, what refuses, and what is said either way."""

    FID = "f-switch"

    def setUp(self):
        super().setUp()
        self.enterContext(mock.patch.dict(os.environ, {}, clear=True))
        _plane_workspaces("alpha", "beta")
        state.frame_dir(self.FID, create=True)
        # What a launch that pinned NOTHING records: every name present and empty, which
        # is `commands_frame._frame_identity_env`'s own shape.
        state.record_identity(self.FID, {"CHARTER_SESSION_ID": self.FID,
                                         "CHARTER_WORKSPACE": "", "CHARTER_PERSONA": ""})
        state.record_workspace(self.FID, "alpha")

    def test_a_switch_moves_what_every_frame_surface_asks(self):
        """`state.workspace_for` is the one rule every panel, the gather and the status
        line ask (#512's own docstring says so), so that — not the file the switch
        happened to write — is what "the frame moved" has to mean. Asserting on the
        pointer file instead would be this repo's "a test asserting one layer BELOW the
        code that prints"."""
        self.assertEqual(state.workspace_for(self.FID), "alpha")
        out = switch.to_workspace(self.FID, "beta")
        self.assertTrue(out.ok, out.message)
        self.assertEqual(state.workspace_for(self.FID), "beta")

    def test_a_switch_bumps_the_frame_so_panels_repaint(self):
        """A panel repaints because the version moved (`frame/panel.py`'s contract).
        Writing the pointer without bumping is exactly the "some panels may or may not
        read it" failure #411 was filed for — the frame would keep drawing the old plane
        until something else happened to bump."""
        before = state.version(self.FID)
        switch.to_workspace(self.FID, "beta")
        self.assertNotEqual(state.version(self.FID), before)

    def test_the_launch_record_moves_too_so_a_respawned_panel_agrees(self):
        """Rung 1 (the pointer) is what live panels read; rung 2 (`record_workspace`) is
        what `_relayout_pane_env` replays onto panes split LATER and what a panel resolves
        if the pointer is ever gone. Moving only rung 1 leaves a frame whose next density
        change draws the old workspace beside the new one."""
        switch.to_workspace(self.FID, "beta")
        self.assertEqual(state.frame_workspace(self.FID), "beta")

    def test_switching_twice_is_not_refused_by_the_first_switchs_own_lock(self):
        """`workspace.set_active` LOCKS the session to what it selected, so the switcher's
        own first write takes a lock that its second write would hit. A switcher that
        works exactly once is the "silently fails against a lock" failure #517 names,
        arrived at by charter's own hand."""
        self.assertTrue(switch.to_workspace(self.FID, "beta").ok)
        second = switch.to_workspace(self.FID, "alpha")
        self.assertTrue(second.ok, second.message)
        self.assertEqual(state.workspace_for(self.FID), "alpha")

    def test_overriding_a_lock_is_said_out_loud(self):
        """An agent inside the frame locked it with `charter workspace use`; the operator
        then picks another off the menu. The switch happens — they are at the keyboard —
        but the message has to name what was overridden, or the agent's next command acts
        on a workspace nobody was told about."""
        workspace.set_active("alpha", session_id=self.FID)
        out = switch.to_workspace(self.FID, "beta")
        self.assertTrue(out.ok)
        self.assertIn("alpha", out.message)
        self.assertIn("lock", out.message)

    def test_no_terminal_pointer_is_written_for_somebody_elses_terminal(self):
        """#411, arriving on this command. A switch runs as a `run-shell` child of
        charter's private tmux server — SHARED between every frame on the machine, and
        holding the environment of whichever launcher started it, possibly days ago in
        another terminal. `workspace.set_active` normally writes a per-terminal pointer
        keyed on `$TERM_SESSION_ID`/`$TMUX_PANE`/`$STY`/`$SSH_TTY`, so without
        `terminal_id=""` a switch inside frame B moves the workspace of the terminal that
        launched frame A."""
        os.environ["TERM_SESSION_ID"] = "someone-elses-terminal"
        before = sorted(p.name for p in config.TERMINALS_DIR.iterdir()) \
            if config.TERMINALS_DIR.exists() else []
        self.assertTrue(switch.to_workspace(self.FID, "beta").ok)
        after = sorted(p.name for p in config.TERMINALS_DIR.iterdir()) \
            if config.TERMINALS_DIR.exists() else []
        self.assertEqual(after, before)

    def test_an_unknown_workspace_is_refused_and_creates_nothing(self):
        before = sorted(p.name for p in config.WORKSPACES_DIR.iterdir())
        out = switch.to_workspace(self.FID, "nope")
        self.assertFalse(out.ok)
        self.assertIn("nope", out.message)
        self.assertEqual(sorted(p.name for p in config.WORKSPACES_DIR.iterdir()), before)
        self.assertEqual(state.workspace_for(self.FID), "alpha")

    def test_a_name_outside_the_alphabet_is_refused(self):
        out = switch.to_workspace(self.FID, "../escape")
        self.assertFalse(out.ok)
        self.assertEqual(state.workspace_for(self.FID), "alpha")

    def test_a_pinned_frame_is_refused_rather_than_told_it_moved(self):
        """`$CHARTER_WORKSPACE` was set at launch, so every panel pane holds it in its own
        process environment and no file charter writes can take it out — rung 0 of
        `state.workspace_for` outranks rungs 1 and 2 both. Reporting "switched" and then
        drawing the pin is the failure; reporting the pin is the honest answer."""
        state.record_identity(self.FID, {"CHARTER_WORKSPACE": "alpha"})
        out = switch.to_workspace(self.FID, "beta")
        self.assertFalse(out.ok)
        self.assertIn("alpha", out.message)
        self.assertIn("CHARTER_WORKSPACE", out.message)
        self.assertIsNone(workspace.for_session(self.FID))

    def test_the_pin_is_read_from_the_frame_not_from_this_process(self):
        """The switcher runs as a `run-shell` child of a tmux server shared between every
        frame on the machine, so this process's `$CHARTER_WORKSPACE` may be ANOTHER
        frame's (`state.record_identity` measures exactly that). Reading the pin from
        `os.environ` would refuse a switch on a frame that was never pinned."""
        os.environ["CHARTER_WORKSPACE"] = "someone-elses"
        self.assertTrue(switch.to_workspace(self.FID, "beta").ok)

    def test_an_empty_recorded_pin_is_not_a_pin(self):
        """`_frame_identity_env` emits every name, present or not, so a frame that pinned
        nothing records `CHARTER_WORKSPACE=""`. Treating presence rather than truth as the
        pin would refuse every switch on every ordinary frame."""
        self.assertEqual(state.identity(self.FID).get("CHARTER_WORKSPACE"), "")
        self.assertTrue(switch.to_workspace(self.FID, "beta").ok)

    def test_a_refusal_message_cannot_forge_a_second_line(self):
        """A workspace name is a committed value and a message is a line of charter's own
        output (`contain.py`, #453). The name here is refused for its alphabet anyway —
        what is under test is that the name it ECHOES is contained, since the same echo
        carries a plane-supplied name on the "no workspace named" path."""
        out = switch.to_workspace(self.FID, "beta\nrefused — everything is fine")
        self.assertFalse(out.ok)
        self.assertNotIn("\n", out.message)


class SwitchingPersona(PersonaIso, unittest.TestCase):
    FID = "f-persona"

    def setUp(self):
        super().setUp()
        self.enterContext(mock.patch.dict(os.environ, {}, clear=True))
        _plane_personas("forge", "scribe")
        state.frame_dir(self.FID, create=True)
        state.record_identity(self.FID, {"CHARTER_PERSONA": "", "CHARTER_WORKSPACE": ""})

    def test_the_pointer_lands_under_the_frames_id_not_this_processs(self):
        """Inside a frame the frame IS the charter session (ADR 0019), which is why a
        panel sees the change at all: `persona.resolve_active`'s session rung is keyed on
        `$CHARTER_SESSION_ID`, and in a panel that is the frame id. A switcher that let
        `session.current()` read the id out of its own environment would write under a
        server-inherited id — another frame's, or none."""
        os.environ["CHARTER_SESSION_ID"] = "a-different-frame"
        out = switch.to_persona(self.FID, "forge")
        self.assertTrue(out.ok, out.message)
        self.assertEqual(persona.for_session(self.FID), "forge")
        self.assertIsNone(persona.for_session("a-different-frame"))

    def test_a_switch_bumps_the_frame(self):
        before = state.version(self.FID)
        switch.to_persona(self.FID, "forge")
        self.assertNotEqual(state.version(self.FID), before)

    def test_no_terminal_pointer_is_written_for_somebody_elses_terminal(self):
        """The persona half of the same #411 leak — see the workspace case."""
        os.environ["TERM_SESSION_ID"] = "someone-elses-terminal"
        self.assertTrue(switch.to_persona(self.FID, "forge").ok)
        listed = (sorted(p.name for p in config.TERMINALS_DIR.iterdir())
                  if config.TERMINALS_DIR.exists() else [])
        self.assertEqual([n for n in listed if n.endswith(".persona")], [])

    def test_an_unknown_persona_is_refused(self):
        out = switch.to_persona(self.FID, "nobody")
        self.assertFalse(out.ok)
        self.assertIsNone(persona.for_session(self.FID))

    def test_a_pinned_frame_is_refused(self):
        state.record_identity(self.FID, {"CHARTER_PERSONA": "forge"})
        out = switch.to_persona(self.FID, "scribe")
        self.assertFalse(out.ok)
        self.assertIn("CHARTER_PERSONA", out.message)
        self.assertIsNone(persona.for_session(self.FID))

    def test_the_marked_row_is_the_one_the_panels_are_showing(self):
        """`switch.current_persona` must answer for the FRAME. `persona.resolve_active`
        would answer for this process — whose `$CHARTER_PERSONA` belongs to whichever
        launcher started the shared tmux server."""
        os.environ["CHARTER_PERSONA"] = "someone-elses"
        switch.to_persona(self.FID, "scribe")
        self.assertEqual(switch.current_persona(self.FID), "scribe")


def _rows(fid: str, *, density: str = "normal"):
    """The palette's whole catalogue for *fid*, built exactly the way `_draw_palette`
    builds it — the two picker rows, then the action rows, neither from a hand-written
    list."""
    reg = builtin_actions.build(fid, current_density=density)
    return (choose.open_rows(fid)
            + palette.rows(reg.offers(fid=fid, snapshot={})))


def _titles(fid: str, kind: str, *, density: str = "normal") -> list[str]:
    """Every row title whose ID belongs to *kind* (today: `density`).

    Selected by ID and never by what the title says, because the mark charter puts in
    front of the current one is part of the title — a prefix match on the title would
    silently drop exactly the row every one of these tests is about.
    """
    return [r.title for r in _rows(fid, density=density)
            if r.id.startswith(kind + ".")]


def _names(fid: str, noun: str) -> list[str]:
    """Every row title inside *noun*'s picker — the list one keypress past the palette."""
    return [r.title for r in choose.roster(noun, fid).rows]


class ThePaletteOffersAPickerForEach(PersonaIso, unittest.TestCase):
    """#517's whole surface, moved off `display-menu` and then off the action registry:
    one row per noun, saying which name the frame is on, opening the list of the rest.

    **Task 6's correction to Task 4.** The menu capped each list at twelve because a
    `display-menu` is drawn inside the terminal and tmux does not scroll it; Task 4 lifted
    the cap by registering every name as an ACTION, which is a contract that promises
    fire-and-report work and describes a name badly — forty names meant forty ``run``s that
    each started a whole second charter to write two files. The names now live in a picker
    (`frame/choose.py`), which is this same overlay over a different row source, and the
    palette carries the doorway.
    """

    FID = "f-entries"

    def setUp(self):
        super().setUp()
        self.enterContext(mock.patch.dict(os.environ, {}, clear=True))
        state.frame_dir(self.FID, create=True)
        state.record_identity(self.FID, {"CHARTER_WORKSPACE": "", "CHARTER_PERSONA": ""})

    def test_the_palette_opens_with_the_two_pickers_then_charters_own_actions(self):
        """The order is fixed and the pickers are first, so an operator who presses `F2`
        and Enter without reading opens a list rather than detaching their harness."""
        _plane_workspaces("alpha")
        ids = [r.id for r in _rows(self.FID)]
        self.assertEqual(ids[:3], ["pick:workspace", "pick:persona", "frame.detach"])
        self.assertTrue(any(i.startswith("density.") for i in ids[3:6]), ids)

    def test_each_doorway_says_which_name_the_frame_is_on(self):
        """So the palette still answers "which workspace am I on" without opening
        anything, and typing the name still finds the row when it is the one in use."""
        _plane_workspaces("alpha")
        state.record_workspace(self.FID, "alpha")
        titles = {r.id: r.title for r in _rows(self.FID)}
        self.assertEqual(titles["pick:workspace"], "workspace: alpha — pick another")

    def test_a_noun_with_nothing_chosen_says_so_rather_than_naming_nothing(self):
        """A plane with no persona at all is an ordinary answer, not a missing one — and
        `persona: ` with nothing after it is a row that reads as broken."""
        self.assertEqual({r.id: r.title for r in _rows(self.FID)}["pick:persona"],
                         "persona — pick one")

    def test_nothing_chosen_is_the_empty_string_and_never_none(self):
        """charter's own convention for "there is none", one module over: `switch._pin`
        records that "empty is what every charter reader already treats as absent", and
        `choose.current` is declared to answer a name.

        **Pinned on the function rather than on a row, because both callers that exist
        today mask it**: `open_rows` tests `if now` and `roster` compares `n == now`, and
        `None` and `""` behave identically in both. A third caller writing `f"on {now}"`
        would print the word `None`. That masking is the shape this repo has been bitten
        by four times, so the contract is asserted where it is stated.
        """
        self.assertEqual(choose.current(choose.PERSONA, self.FID), "")

    def test_a_long_list_is_not_capped(self):
        """Thirty workspaces are thirty rows in the picker. The menu answered twelve plus
        a row saying how many it had hidden; this surface has nowhere to hide them."""
        _plane_workspaces(*[f"ws{i:02d}" for i in range(30)])
        rows = _names(self.FID, choose.WORKSPACE)
        self.assertGreaterEqual(len(rows), 30, rows)
        self.assertTrue(any("ws29" in r for r in rows), rows)

    def test_an_empty_list_is_simply_no_rows(self):
        """A plane with no personas gets an empty picker — and nothing pretends otherwise.
        The menu needed a placeholder row because `display-menu` refuses a zero-row menu
        outright (`not enough arguments`); `overlay.EMPTY` is what this surface draws."""
        self.assertEqual(_names(self.FID, choose.PERSONA), [])


class ThePaletteCannotGoStale(PersonaIso, unittest.TestCase):
    """The menu was a SNAPSHOT on disk; the palette is not, and this is what that buys.

    `menu.record` wrote every label and every mark once, so a switch that did not
    re-record left the F2 menu naming the workspace the frame had LEFT, and a workspace
    made after launch never appeared at all — `_rerecord_menu` existed only to paper over
    that, and had to be called from every command that could move the frame. Nothing calls
    anything now: `builtin_actions.build` resolves the density mark and `frame/choose.py`
    resolves the names and their marks when the palette opens, so there is no second copy
    to keep in step.
    """

    FID = "f-palette-follows"

    def setUp(self):
        super().setUp()
        self.enterContext(mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": self.FID},
                                          clear=True))
        # No screen to report on in a test: `_say_on_screen` is the tmux half and is
        # covered by its own cases. What is under test is what the palette offers after.
        self.said = self.enterContext(mock.patch.object(commands_frame, "_say_on_screen"))
        _plane_workspaces("alpha", "beta")
        _plane_personas("forge", "scribe")
        state.frame_dir(self.FID, create=True)
        state.record_identity(self.FID, {"CHARTER_SESSION_ID": self.FID,
                                         "CHARTER_WORKSPACE": "", "CHARTER_PERSONA": ""})
        state.record_workspace(self.FID, "alpha")

    def _switch(self, **kw):
        return commands_frame.cmd_switch(mock.Mock(**{"workspace": None, "persona": None,
                                                      **kw}))

    def test_the_mark_moves_with_the_frame(self):
        # `default` is folded in whether or not its directory exists — `switch.workspaces`
        # matches `commands_workspace.cmd_workspace_use` there.
        self.assertEqual(_names(self.FID, choose.WORKSPACE),
                         ["* alpha", "  beta", "  default"])
        self.assertEqual(self._switch(workspace="beta"), 0)
        self.assertEqual(_names(self.FID, choose.WORKSPACE),
                         ["  alpha", "* beta", "  default"])

    def test_the_doorway_names_the_workspace_the_frame_moved_to(self):
        """The mark inside the picker and the name on the palette row are one read
        (`choose.current`), so the two surfaces cannot disagree about where the frame is."""
        self._switch(workspace="beta")
        self.assertEqual({r.id: r.title for r in _rows(self.FID)}["pick:workspace"],
                         "workspace: beta — pick another")

    def test_a_persona_switch_moves_its_own_mark_too(self):
        self.assertEqual(_names(self.FID, choose.PERSONA), ["  forge", "  scribe"])
        self._switch(persona="scribe")
        self.assertEqual(_names(self.FID, choose.PERSONA), ["  forge", "* scribe"])

    def test_a_workspace_made_after_launch_is_offered(self):
        """The same staleness seen from the other side: the menu's table was written at
        launch, so a plane that grew a workspace since had no row for it."""
        _plane_workspaces("gamma")
        self.assertIn("  gamma", _names(self.FID, choose.WORKSPACE))

    def test_a_pinned_frame_is_offered_the_row_WITH_ITS_REASON(self):
        """Step 4 of the plan, on the exact example it names. `$CHARTER_WORKSPACE` was set
        at launch and sits in every panel pane's environment, so nothing charter writes can
        outrank it — and a palette that silently dropped the row would leave the operator
        unable to ask why a thing they remember is missing."""
        state.record_identity(self.FID, {"CHARTER_WORKSPACE": "alpha"})
        row = {r.id: r for r in _rows(self.FID)}["pick:workspace"]
        self.assertIn("$CHARTER_WORKSPACE pins this frame", row.note)
        self.assertIn("'alpha'", row.note)

    def test_an_unpinned_frames_doorway_carries_no_reason_at_all(self):
        """The other direction, so the reason above cannot pass by always being there:
        `available` and "has no reason" are one decision in `choose.pin_reason`, and a
        non-empty note is what `_draw_palette` refuses the keypress on."""
        self.assertEqual({r.id: r for r in _rows(self.FID)}["pick:workspace"].note, "")
        self.assertEqual({r.id: r for r in _rows(self.FID)}["pick:persona"].note, "")

    def test_the_density_mark_is_read_from_the_frames_own_record(self):
        """A switch does not change the density, and nothing re-writes it: the mark is
        derived when the palette opens, from `_current_density`."""
        state.record_density(self.FID, "full")
        self._switch(workspace="beta")
        titles = _titles(self.FID, "density",
                         density=commands_frame._current_density(self.FID))
        self.assertIn("* density: full", titles)


class ThePickerDecidesNothingOnItsOwn(unittest.TestCase):
    """`frame/picker.py` renders and reads. It creates nothing, and it cannot hang."""

    ROWS = [picker.Row("alpha", 3), picker.Row("beta", 0)]

    def _ask(self, answers, name_ok=lambda n: n.replace("-", "").isalnum()):
        written: list[str] = []
        it = iter(answers)

        def read():
            try:
                return next(it)
            except StopIteration:
                return None

        got = picker.ask(self.ROWS, "alpha", read=read, write=written.append,
                         name_ok=name_ok)
        return got, "".join(written)

    def test_a_number_picks_that_row(self):
        got, _ = self._ask(["2"])
        self.assertEqual(got, picker.Choice(picker.USE, "beta"))

    def test_empty_takes_the_marked_row(self):
        got, _ = self._ask([""])
        self.assertEqual(got, picker.Choice(picker.USE, "alpha"))

    def test_a_name_can_be_typed_instead_of_a_number(self):
        got, _ = self._ask(["beta"])
        self.assertEqual(got, picker.Choice(picker.USE, "beta"))

    def test_q_cancels(self):
        got, _ = self._ask(["q"])
        self.assertEqual(got, picker.Choice(picker.CANCEL, ""))

    def test_end_of_input_cancels_rather_than_waiting(self):
        """The one answer that can never be a hang. A closed stdin is not a workspace."""
        got, _ = self._ask([])
        self.assertEqual(got, picker.Choice(picker.CANCEL, ""))

    def test_creating_needs_an_explicit_yes(self):
        got, _ = self._ask(["n", "feature-x", ""])
        self.assertNotEqual(got.action, picker.CREATE)

    def test_creating_confirmed_returns_a_create(self):
        got, _ = self._ask(["n", "feature-x", "y"])
        self.assertEqual(got, picker.Choice(picker.CREATE, "feature-x"))

    def test_a_name_that_already_exists_is_a_use_not_a_second_create(self):
        got, _ = self._ask(["n", "beta"])
        self.assertEqual(got, picker.Choice(picker.USE, "beta"))

    def test_a_name_outside_the_alphabet_never_becomes_a_create(self):
        got, _ = self._ask(["n", "../escape", "y", "q"])
        self.assertEqual(got.action, picker.CANCEL)

    def test_an_answer_that_never_arrives_ends_rather_than_spins(self):
        """A tty ends on EOF; this is the stream that neither answers nor ends. An
        unbounded loop there is a hang with no message — the failure #518 says a picker
        must never be."""
        written: list[str] = []
        got = picker.ask(self.ROWS, "alpha", read=lambda: "?", write=written.append,
                         name_ok=lambda n: True)
        self.assertEqual(got.action, picker.CANCEL)

    def test_two_names_that_differ_only_invisibly_are_drawn_differently(self):
        """A workspace name is a committed value, contained BEFORE the column arithmetic
        that lays the list out (#472).

        The property is NOT "a newline cannot add a row" — `tui.pad` already closes that
        by replacing every control character with a space, so a test asserting it would
        be green with the containment deleted (measured). What `contain.one_line` adds is
        that the replacement is VISIBLE: without it, `a b` and `a\\nb` are two different
        workspaces drawn as the same row, and the operator picks one of them blind."""
        rows = [picker.Row("a b", 1), picker.Row("a\nb", 1)]
        text = "\n".join(tui.strip_ansi(ln)
                         for ln in picker.render(rows, "a b", 80).splitlines())
        self.assertIn("\\x0a", text)
        listed = [ln for ln in text.splitlines() if ln.strip()[:1].isdigit()]
        self.assertEqual(len(listed), 2, text)
        self.assertNotEqual(listed[0][6:].strip(), listed[1][6:].strip())

    def test_the_list_is_measured_with_display_width_not_length(self):
        """`tui.width`, never `len` — the column would drift by one cell per wide
        character otherwise, which `_persona_chips`' own comment says has broken this
        layout twice."""
        rows = [picker.Row("ws", 1), picker.Row("日本", 1)]
        # Line by line: `tui.strip_ansi` runs `tui.sanitize`, which escapes every control
        # character — a newline included — so stripping the whole block first would leave
        # one line and `splitlines()` nothing to split.
        text = "\n".join(tui.strip_ansi(ln)
                         for ln in picker.render(rows, "ws", 80).splitlines())
        # `tui.pad` clamps as well as pads, so a column sized with `len` does not drift —
        # it EATS the name: `name_w` would be 2 for both, and `pad("日本", 2)` renders
        # `"… "`. The name surviving intact is the property; the alignment underneath it
        # is `tui.pad`'s and is true either way, which is why asserting on the columns
        # would have been a guard no mutation could turn red.
        self.assertIn("日本", text)
        self.assertNotIn("…", text)
        rows_shown = [ln for ln in text.splitlines() if ln.strip()[:1].isdigit()]
        self.assertEqual(len(rows_shown), 2, text)
        starts = {tui.width(ln[:ln.index("1 repo")]) for ln in rows_shown}
        self.assertEqual(len(starts), 1, rows_shown)


class ANonInteractiveLaunchNeverBlocks(PersonaIso, unittest.TestCase):
    """#518's hard requirement: `charter <harness>` runs from scripts and other agents."""

    def _args(self, **kw):
        return mock.Mock(**{"workspace": None, "pick": False, **kw})

    def test_a_pipe_is_never_asked(self):
        with mock.patch.dict(os.environ, {}, clear=True), \
             mock.patch("sys.stdin", io.StringIO("")), \
             mock.patch("sys.stdout") as out:
            out.isatty.return_value = True
            self.assertFalse(commands_frame._picker_wanted(self._args(), None))

    def test_a_redirected_stdout_is_never_asked(self):
        with mock.patch.dict(os.environ, {}, clear=True), \
             mock.patch("sys.stdin") as sin, mock.patch("sys.stdout") as out:
            sin.isatty.return_value = True
            out.isatty.return_value = False
            self.assertFalse(commands_frame._picker_wanted(self._args(), None))

    def _tty(self):
        sin = self.enterContext(mock.patch("sys.stdin"))
        out = self.enterContext(mock.patch("sys.stdout"))
        sin.isatty.return_value = True
        out.isatty.return_value = True

    def test_an_env_pin_outranks_even_an_explicit_pick(self):
        """`$CHARTER_WORKSPACE` is the top of `resolve`'s precedence and means "this one,
        I have decided" — the documented way to aim an unattended agent.

        Asserted WITH `--pick`, because that is the only case in which the check earns
        its line: without it, the pin is also what `workspace.chosen` answers, so the
        bottom rung refuses the picker anyway and a test asking without `--pick` would
        pass with the check deleted. It is also the right answer on its own terms — a
        pin cannot be moved from inside the frame either (`switch.to_workspace` refuses
        it), so offering a choice that cannot take effect would be the worse outcome."""
        self._tty()
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "pinned"}, clear=True):
            self.assertFalse(
                commands_frame._picker_wanted(self._args(pick=True), "pinned"))

    def test_an_explicit_workspace_flag_outranks_even_an_explicit_pick(self):
        self._tty()
        with mock.patch.dict(os.environ, {}, clear=True):
            self.assertFalse(commands_frame._picker_wanted(
                self._args(workspace="alpha", pick=True), "alpha"))

    def test_nothing_chosen_is_what_asks(self):
        self._tty()
        with mock.patch.dict(os.environ, {}, clear=True):
            self.assertTrue(commands_frame._picker_wanted(self._args(), None))
            self.assertFalse(commands_frame._picker_wanted(self._args(), "already"))

    def test_pick_asks_even_when_something_chose(self):
        self._tty()
        with mock.patch.dict(os.environ, {}, clear=True):
            self.assertTrue(
                commands_frame._picker_wanted(self._args(pick=True), "already"))


class WhatChoseAndWhatMerelyFellBack(PersonaIso, unittest.TestCase):
    """`workspace.chosen` is `resolve`'s ladder minus its built-in — one ladder, two
    questions, so a rung added to one reaches the other (#518)."""

    def setUp(self):
        super().setUp()
        self.enterContext(mock.patch.dict(os.environ, {}, clear=True))

    def test_nothing_chose_is_none_while_resolve_still_answers(self):
        self.assertIsNone(workspace.chosen())
        self.assertEqual(workspace.resolve(), config.DEFAULT_WORKSPACE)

    def test_every_rung_that_answers_resolve_also_answers_chosen(self):
        self.assertEqual(workspace.chosen("explicit"), "explicit")
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "env"}, clear=True):
            self.assertEqual(workspace.chosen(), "env")
        workspace.set_active("alpha", session_id="s-1")
        self.assertEqual(workspace.chosen(session_id="s-1"), "alpha")

    def test_a_declared_default_counts_as_a_choice(self):
        """Somebody nominated it (#193) — unlike `config.DEFAULT_WORKSPACE`, which is the
        name charter falls back to when there is nothing to read."""
        workspace.set_declared_default("nominated")
        self.assertEqual(workspace.chosen(), "nominated")


class TheLauncherOwnsItsOwnFlags(unittest.TestCase):
    """`--workspace <name>` is charter's, and it takes a value — #518's escape hatch."""

    def test_a_value_flag_consumes_two_tokens(self):
        """Scanning it as one would leave the workspace NAME as the harness's `argv[0]` —
        `charter frame --workspace foo -- true` would try to execute `foo`."""
        head, rest = cli._split_frame_argv(["frame", "--workspace", "foo", "--", "true"])
        self.assertEqual(head, ["frame", "--workspace", "foo"])
        self.assertEqual(rest, ["--", "true"])

    def test_the_equals_form_is_one_token(self):
        head, rest = cli._split_frame_argv(["claude", "--workspace=foo", "-p", "hi"])
        self.assertEqual(head, ["claude", "--workspace=foo"])
        self.assertEqual(rest, ["-p", "hi"])

    def test_a_trailing_value_flag_is_left_for_argparse_to_refuse(self):
        """argparse's own "expected one argument" is the right message, from the part
        that owns the flag — better than silently consuming past the end."""
        head, rest = cli._split_frame_argv(["claude", "--workspace"])
        self.assertEqual(head, ["claude", "--workspace"])
        self.assertEqual(rest, [])

    def test_pick_is_a_leading_flag_and_nothing_after_it_is(self):
        head, rest = cli._split_frame_argv(["claude", "--pick", "-p", "--pick"])
        self.assertEqual(head, ["claude", "--pick"])
        self.assertEqual(rest, ["-p", "--pick"])

    def test_the_flags_reach_the_launcher(self):
        args = cli.build_parser().parse_args(["claude", "--workspace", "alpha", "--pick"])
        self.assertEqual(args.workspace, "alpha")
        self.assertTrue(args.pick)


if __name__ == "__main__":
    unittest.main()
