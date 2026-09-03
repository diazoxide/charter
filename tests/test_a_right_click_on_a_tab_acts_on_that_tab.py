"""Right-click a chat tab and charter draws a menu about THAT tab — #846.

*"All functionality in the F2 menu — extract them and integrate the features on
components. Tabs should have right-click context-menu support."*

Two measurements shape every case here and neither is what was expected.

**Right-click already reaches charter's panels and needs no tmux bind.** Measured on tmux
3.2 and 3.7c with tmux's own `mouse` off — charter's shipped default — and with it on: a
pane that requests what `overlay.MOUSE_ON` requests is handed button 2's press and its
release, pane-relative, and no `bind -n` fires at all. With `mouse on`, tmux's *default*
`MouseDown3Pane` tests `#{mouse_any_flag}`, which is 1 precisely because this pane asked
for reporting, so it takes the `send-keys -M` branch and tmux's own menu never appears. A
custom `bind -n MouseDown3Pane` that omitted `send -M` is the one thing that would swallow
the press, so nothing here needed a bind to work.

What that branch also does is `select-pane -t =` first, which took the keyboard off the
harness on every right-click — #634's defect one button over, closed since by #848 with a
binding that keeps the `send-keys -M` this file depends on and adds nothing to it. It is
issued as its own `bind-key` argv (`commands_frame._menu_button_bind_argv`), never as a
line in the config text, which is what the case below still asserts.

**And exactly two palette rows are about a specific tab** — `chat: previous transcript`
and `chat: close`. Everything else charter offers is about the frame (detach, the
densities, the chromes, next/previous) or about the plane (the pickers, `charter: quit`),
and none of those has a tab to sit on. So this is not a reduction of the palette: `F2`
keeps every row it had, and this is a second, faster route to two of them.

The real-tmux half — that a button-2 report injected into a real client really reaches a
real panel pane and really opens a pane — is in
`tests/test_a_real_click_on_a_real_tab_bar_switches.py`, beside the left-click cases it
is the counterpart of.
"""

from __future__ import annotations

import os
import unittest
from unittest import mock

from charter import commands_frame, tui, util
from charter.frame import (builtin_actions, builtins, component, leave, overlay,
                           palette, slots, state, tabmenu)

from tests._isolation import PersonaIso
from tests.test_frame_chat_switch import _plant


def _click(row: int, col: int, *, name: str, pressed: bool = True):
    """One decoded button press at (*row*, *col*) of the component's own canvas.

    `overlay.Event` and not a stand-in: `events.Dispatcher._on_canvas` hands the handler
    exactly this, with the operator's `[frame] pad` already subtracted, so a case that
    invented its own record would be asserting against a shape nothing produces. The
    button NAME is `overlay._SGR_BUTTONS`' own — `right` is what a decoded button 2 has
    become since #607, and it has been dropped by one comparison ever since.
    """
    return overlay.Event(overlay.CLICK, name, row=row, col=col, pressed=pressed)


class _AWatchedSpawn(PersonaIso):
    """Every case in this file records what was started instead of starting it.

    **`chat: close` stops a harness.** Nothing here may reach a real `charter frame-close`,
    so `builtin_actions._spawn` — the one door every one of these paths goes through — is
    the seam, and what is asserted is the argv that would have run.
    """

    def setUp(self):
        super().setUp()
        self.spawned = []
        self.enterContext(mock.patch.object(
            builtin_actions, "_spawn",
            side_effect=lambda argv, *, fid: self.spawned.append((argv, fid))))


class _ABarThatWasDrawn(_AWatchedSpawn):
    """A workspace with three chats, and the bar painted over it before every click.

    **The bar is DRAWN, never hand-published**, for `test_a_click_on_a_tab_bar_switches`'
    reason: the column map is the paint's own output (`slots._bar`), and a case that
    published a map by hand would be measuring a fixture rather than the strip the
    operator pressed.
    """

    #: Wide enough for every name on rung 1. The narrow rungs have their own cases in
    #: `tests/test_frame_bars.py`; this file is about what happens once a tab is resolved.
    WIDTH = 200

    FID = "api.1"

    def setUp(self):
        super().setUp()
        slots.TABS.forget()
        self.addCleanup(slots.TABS.forget)
        self.enterContext(mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "api"},
                                          clear=False))
        for chat in ("api.1", "api.2", "api.3"):
            _plant(chat, workspace="api")
        self.row = slots.chats_bar(self.FID, self.WIDTH)[0]

    def _handler(self, cid: str):
        """*cid*'s handler out of the REAL registry, closed over :attr:`FID`.

        `builtins.build(fid)` rather than a handler reached directly, so every case here
        also asserts that `Component.__post_init__` accepted the declaration: it refuses
        an `on_event` with no `events` and `events` with no `on_event`.
        """
        return builtins.build(self.FID).get(cid).on_event

    def _column_of(self, field: str, row: str | None = None) -> int:
        """Which COLUMN of the drawn row *field* starts in.

        **`tui.width` of what comes before it, never the character index**: the tab you
        are on is drawn as a reverse-video block, so every field to its right sits further
        into the STRING than into the pane.
        """
        row = self.row if row is None else row
        at = row.index(field)
        self.assertNotIn(field, row[at + 1:], f"{field!r} is not unique in {row!r}")
        return tui.width(row[:at])


class ARightClickResolvesToTheTabItLandedOn(_ABarThatWasDrawn, unittest.TestCase):
    """`slots._Tabs.tab_at`: which tab a cell holds, including the one you are on."""

    def test_the_tab_you_are_on_answers_itself(self):
        """**The one place this differs from `switch_to`, and the whole reason it is a
        second method.** Switching to the tab you are already on is 41 tmux calls to
        arrive where you already are, so `switch_to` refuses it. Closing the chat you are
        IN is the ordinary case of closing a chat — it is what `F2 → chat: close` has
        always meant — so a menu that refused the marked tab would refuse the commonest
        click there is.
        """
        at = self._column_of("*" + self.FID)
        self.assertIsNone(slots.TABS.switch_to(0, at))
        self.assertEqual(slots.TABS.tab_at(0, at), self.FID)

    def test_every_other_tab_answers_the_name_it_was_drawn_with(self):
        for name in ("api.2", "api.3"):
            at = self._column_of(" " + name)
            self.assertEqual(slots.TABS.tab_at(0, at), name)

    def test_no_cell_answers_two_kinds_of_question(self):
        """**A cell that can answer two kinds of question is a defect, not a test gap.**

        #840 found `add_at` claiming the cell a `+N` count is drawn in, on a row that
        looked identical either way — so a click asking to *see* the other chats would
        have *made* one. The same hazard arrives here with a third reader of the same map:
        a `+` or a `+9` that also answered `tab_at` would put a CLOSE menu behind a field
        that means "make one" or "show me the rest".
        """
        for row in range(3):
            for col in range(self.WIDTH):
                kinds = [k for k, hit in (
                    ("tab", slots.TABS.tab_at(row, col) is not None),
                    ("more", slots.TABS.more_at(row, col)),
                    ("add", slots.TABS.add_at(row, col))) if hit]
                self.assertLessEqual(len(kinds), 1, f"({row}, {col}) answers {kinds}")

    def test_a_cell_no_tab_was_drawn_into_answers_nothing(self):
        """The heading, and every column past the last name. `switch_to` already promises
        this and `tab_at` reads the identical mapping, so the promise is structural: a
        cell nothing drew into is absent from the map, whatever asks about it."""
        for col in range(self._column_of("chats")):
            self.assertIsNone(slots.TABS.tab_at(0, col), f"column {col}")
        for col in range(tui.width(self.row) + 1, self.WIDTH):
            self.assertIsNone(slots.TABS.tab_at(0, col), f"column {col}")


class ARightClickOpensTheMenuAndNothingElse(_ABarThatWasDrawn, unittest.TestCase):
    """What the press starts, and every gesture and surface where it starts nothing."""

    def test_a_right_press_on_a_tab_opens_that_tabs_menu(self):
        self._handler("chats")(_click(0, self._column_of(" api.3"), name="right"))
        self.assertEqual(
            self.spawned,
            [(util.self_relaunch_argv("frame-palette", "--tab", "api.3"), self.FID)])

    def test_a_right_press_on_the_tab_you_are_on_opens_its_menu_too(self):
        self._handler("chats")(_click(0, self._column_of("*" + self.FID), name="right"))
        self.assertEqual(
            self.spawned,
            [(util.self_relaunch_argv("frame-palette", "--tab", self.FID), self.FID)])

    def test_the_child_is_told_which_frame_it_is_rather_than_inheriting_one(self):
        """`_spawn`'s `fid=` is the child's own `$CHARTER_SESSION_ID`. One tmux server is
        shared by every frame on the machine, so a child left to read that variable out of
        an inherited environment can act on somebody else's frame. The pane the menu is
        carved off is the one the operator is LOOKING at — this frame's harness — even
        though the menu is about another tab."""
        self._handler("chats")(_click(0, self._column_of(" api.3"), name="right"))
        self.assertEqual([fid for _argv, fid in self.spawned], [self.FID])

    def test_the_release_starts_nothing(self):
        """**The PRESS is acted on and the release is dropped**, kept word for word from
        every other handler in `frame/builtins.py`: a drag begun on a pane border delivers
        exactly one release (`frame/overlay.py` measured it), so the release is the half
        that can arrive unpaired."""
        self._handler("chats")(
            _click(0, self._column_of(" api.3"), name="right", pressed=False))
        self.assertEqual(self.spawned, [])

    def test_a_right_press_switches_nothing(self):
        """A right-click is not a slower left-click. Which chat this frame is on may not
        move on a gesture that opened a menu about a different one."""
        self._handler("chats")(_click(0, self._column_of(" api.3"), name="right"))
        self.assertNotIn(util.self_relaunch_argv("frame-chat", "api.3"),
                         [argv for argv, _fid in self.spawned])

    def test_a_left_press_is_exactly_what_it_was(self):
        """**The neighbour assertion.** Changing which buttons reach this handler changes
        which events reach the branch below it, and that is not something any tool
        reports. A left click still switches — same argv, same fid, nothing extra."""
        self._handler("chats")(_click(0, self._column_of(" api.3"), name="left"))
        self.assertEqual(
            self.spawned, [(util.self_relaunch_argv("frame-chat", "api.3"), self.FID)])

    def test_a_left_press_on_the_tab_you_are_on_is_still_nothing(self):
        """`slots._Tabs.switch_to`'s rule, which `tab_at` deliberately does not share:
        re-switching is 41 tmux calls to arrive where you are. A handler that had started
        reading the new map for the old gesture would fail here."""
        self._handler("chats")(_click(0, self._column_of("*" + self.FID), name="left"))
        self.assertEqual(self.spawned, [])

    def test_a_middle_press_on_a_tab_does_nothing(self):
        """Middle-click is paste on every terminal an operator has used, and charter takes
        no gesture that already means something else. `overlay._SGR_BUTTONS` names it, so
        it really is decoded and delivered — and dropped here."""
        self._handler("chats")(_click(0, self._column_of(" api.3"), name="middle"))
        self.assertEqual(self.spawned, [])

    def test_a_right_press_on_the_workspaces_bar_does_nothing(self):
        """**Structural, not a branch.** A workspace has no `chat: close` and no
        transcript — both rows this menu holds are about a CHAT — so the workspaces bar is
        handed no menu command at all, exactly as it is handed no `+`
        (`slots.workspaces_bar` publishes no add column, and `frame/builtins._bar_events`
        takes the answer as data rather than deriving it from what the renderer drew)."""
        slots.TABS.forget()
        row = slots.workspaces_bar(self.FID, self.WIDTH)[0]
        for col in range(self.WIDTH):
            self._handler("workspaces")(_click(0, col, name="right"))
        self.assertEqual(self.spawned, [], row)

    def test_a_right_press_on_a_cell_that_is_not_a_tab_does_nothing(self):
        for col in (0, self._column_of("chats"), tui.width(self.row) + 5):
            self._handler("chats")(_click(0, col, name="right"))
        self.assertEqual(self.spawned, [])

    def test_a_right_press_on_the_add_affordance_makes_no_chat_and_no_menu(self):
        """The `+` is the one cell on this bar where a wrong answer CREATES something.
        Neither gesture may reach the other's field."""
        self._handler("chats")(_click(0, self._column_of(slots.ADD_CHAT), name="right"))
        self.assertEqual(self.spawned, [])


class TheMenuIsTheTwoRowsThatHaveATabToSitOn(_AWatchedSpawn, unittest.TestCase):
    """`frame/tabmenu.py`'s catalogue: what a right-click draws, and in what order."""

    FID = "api.1"
    TAB = "api.2"

    def setUp(self):
        super().setUp()
        for chat in ("api.1", "api.2"):
            _plant(chat, workspace="api")

    def test_it_holds_the_transcript_row_and_the_close_doorway_and_nothing_else(self):
        self.assertEqual([r.id for r in tabmenu.catalogue(self.TAB)],
                         [tabmenu.TRANSCRIPT_ID, tabmenu.CLOSE_ID])

    def test_close_is_last(self):
        """`frame/leave.open_rows` puts the destructive row last so it is never one
        `F2 Enter` away — a palette's cursor starts on the first row that can run. A
        right-click menu makes close MORE reachable than `F2` ever did, so that reasoning
        applies more strongly here, not less."""
        self.assertEqual(tabmenu.catalogue(self.TAB)[-1].id, tabmenu.CLOSE_ID)

    def test_every_row_names_the_tab_that_was_clicked(self):
        """A menu that said *this chat* over a tab you are not on would be the palette's
        own row wearing a menu's clothes. The operator right-clicked one tab precisely
        because they meant that one."""
        for row in tabmenu.catalogue(self.TAB):
            self.assertIn(self.TAB, row.title)
        self.assertIn(self.TAB, tabmenu.label(self.TAB))

    def test_the_transcript_row_opens_the_clicked_tabs_transcript(self):
        """The palette's own `chat: transcript` acts on the chat the palette was opened
        in; this one acts on the tab the pointer landed on, which is the whole difference
        between the two routes."""
        from charter.frame import reopen
        path = reopen.transcript_path(self.TAB)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("what was on screen\n", encoding="utf-8")
        row = tabmenu.catalogue(self.TAB)[0]
        self.assertFalse(row.refused)
        self.assertEqual(row.note, "",
                         "a row that CAN run carries no reason it cannot")
        self.assertTrue(tabmenu.chose(row, self.TAB, fid=self.FID))
        self.assertEqual(
            self.spawned,
            [(util.self_relaunch_argv("frame-transcript", "--chat", self.TAB), self.FID)])

    def test_a_tab_with_no_transcript_is_listed_with_its_reason_and_starts_nothing(self):
        """#512's rule, which this surface does not get to relax: an operator cannot ask
        about an option they cannot see. A chat that has never been quit has no capture,
        which is the ordinary state of a chat running normally rather than a fault — so
        the row is drawn, it says so, and pressing it starts nothing."""
        row = tabmenu.catalogue(self.TAB)[0]
        self.assertTrue(row.refused)
        self.assertEqual(
            row.note,
            "no previous transcript for this chat — one is captured when a plane is quit "
            "(`F2 → charter: quit`) and offered on the chat that comes back")
        self.assertFalse(tabmenu.chose(row, self.TAB, fid=self.FID))
        self.assertEqual(self.spawned, [])

    def test_a_tab_that_cannot_be_named_gets_no_menu_at_all(self):
        """`--tab` reaches a state path, so it is held to the alphabet a chat id travels
        under (`chats.ID_RE`) — and a value outside it degrades to *there is no tab menu*
        rather than to a refusal. Nothing in this feature may ever answer an operator with
        an error they did not ask for."""
        for bad in ("", "   ", "../../etc", "api 2", "api/2", "$(id)"):
            self.assertEqual(tabmenu.wanted(_Args(tab=bad)), "",
                             f"{bad!r} was taken for a chat id")
        self.assertEqual(tabmenu.forward(_Args(tab="../../etc")), ())

    def test_a_tab_that_can_be_named_is_carried_to_the_pane(self):
        self.assertEqual(tabmenu.wanted(_Args(tab=self.TAB)), self.TAB)
        self.assertEqual(tabmenu.forward(_Args(tab=self.TAB)), ("--tab", self.TAB))

    def test_a_padded_name_is_refused_rather_than_trimmed_into_another_chat(self):
        """**No `strip`, and that is a decision the sweep asked for.**

        `commands_frame._pressers_chat` and `cmd_close` do strip, because their value is a
        tmux format expanded into a shell-quoted `run-shell` string. This one cannot be
        padded: its only producer is `slots._Tabs.tab_at`, and `chats.is_chat` holds every
        name off `os.scandir` to `chats.ID_RE` before it reaches a roster. Trimming would
        therefore repair nothing — and if a padded name ever did arrive, it would silently
        retarget the menu at a DIFFERENT chat, which is #838's defect. Refusing cannot.
        """
        self.assertEqual(tabmenu.wanted(_Args(tab=f" {self.TAB} ")), "")
        self.assertEqual(tabmenu.wanted(_Args(tab=f"{self.TAB}\t")), "")

    def test_a_padded_directory_never_reaches_the_menu_in_the_first_place(self):
        """The measurement the case above rests on, asserted rather than asserted-about:
        `chats.of_workspace` is where a name off `os.scandir` enters charter's vocabulary,
        and it refuses one `ID_RE` cannot spell — so no cell of any bar answers it."""
        from charter.frame import chats, slots
        state.frame_dir(f"{self.TAB} ", create=True)
        state.record_workspace(f"{self.TAB} ", "api")
        self.assertNotIn(f"{self.TAB} ", chats.of_workspace("api"))
        slots.TABS.forget()
        self.addCleanup(slots.TABS.forget)
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "api"}, clear=False):
            slots.chats_bar(self.FID, 200)
        self.assertNotIn(f"{self.TAB} ",
                         {slots.TABS.tab_at(0, c) for c in range(200)})

    def test_the_palette_that_is_not_a_tab_menu_forwards_nothing(self):
        """`F2` carries no `--tab`, so `frame-palette --pane` is spelled exactly as it was
        and `_draw_palette` is what runs. A forward that emitted an empty option would
        make every `F2` a tab menu about no chat."""
        self.assertEqual(tabmenu.forward(_Args()), ())
        self.assertEqual(tabmenu.wanted(_Args()), "")


class CloseIsConfirmedAndItNamesTheTabYouClicked(_AWatchedSpawn, unittest.TestCase):
    """The doorway, the warning it opens, and the argv that goes through."""

    FID = "api.1"
    TAB = "api.2"

    def setUp(self):
        super().setUp()
        for chat in ("api.1", "api.2"):
            _plant(chat, workspace="api")

    def test_the_close_row_starts_nothing_by_itself(self):
        """**A doorway, not an action.** Pressing it replaces the surface with the
        warning; a version that closed the chat here would put the one irreversible thing
        charter's frame can do exactly one keypress from a pointer gesture."""
        self.assertFalse(tabmenu.chose(tabmenu.catalogue(self.TAB)[-1], self.TAB,
                                       fid=self.FID))
        self.assertEqual(self.spawned, [])

    def test_the_confirmation_is_about_the_clicked_tab_alone(self):
        """`leave.plan(only=…)` — the same plan, the same warning and the same teardown as
        `F2 → chat: close`, with one target. A second enumeration for this route would be
        a second answer to *what does stopping this cost*."""
        rows = tabmenu.confirm_rows(self.TAB, live={"api.1", "api.2"})
        titles = " ".join(r.title for r in rows)
        self.assertIn(self.TAB, titles)
        self.assertNotIn(self.FID, titles)

    def test_the_confirming_row_is_the_one_leave_mints(self):
        """Same id as the palette's, so the two routes cannot come to disagree about which
        keypress commits — and so a row that merely describes stays `refused`."""
        rows = tabmenu.confirm_rows(self.TAB, live={"api.1", "api.2"})
        self.assertTrue(leave.goes_through(rows[0], leave.CLOSE))
        self.assertFalse(rows[0].refused)
        for row in rows[1:]:
            self.assertTrue(row.refused, row)

    def test_going_through_closes_the_tab_and_says_where_the_key_was_pressed(self):
        """`charter frame-close <tab> --chat <this frame>`: the POSITIONAL is which chat
        to close and `--chat` is where the keypress came from, which is what puts
        `cmd_close`'s sentence on the screen the operator is actually looking at."""
        rows = tabmenu.confirm_rows(self.TAB, live={"api.1", "api.2"})
        self.assertTrue(tabmenu.chose(rows[0], self.TAB, fid=self.FID))
        self.assertEqual(
            self.spawned,
            [(util.self_relaunch_argv("frame-close", self.TAB, "--chat", self.FID),
              self.FID)])

    def test_a_row_that_only_describes_closes_nothing(self):
        rows = tabmenu.confirm_rows(self.TAB, live={"api.1", "api.2"})
        for row in rows[1:]:
            self.assertFalse(tabmenu.chose(row, self.TAB, fid=self.FID), row)
        self.assertEqual(self.spawned, [])

    def test_the_doorway_opens_the_confirmation_and_nothing_else_opens_anything(self):
        """`palette.own_the_tty`'s *then*: a doorway replaces the surface in the pane the
        operator is already looking at, and every other row ends the menu. A second pane
        would race this one's teardown — `commands_frame._close_palette` selects the
        harness, kills this pane and re-arms the hatch as ONE chained tmux command."""
        rows = tabmenu.catalogue(self.TAB)
        self.assertIsNone(tabmenu.opens(rows[0], self.TAB, live=set()))
        nxt = tabmenu.opens(rows[-1], self.TAB, live={"api.1", "api.2"})
        self.assertIsNotNone(nxt)
        self.assertEqual(nxt.catalogue,
                         tabmenu.confirm_rows(self.TAB, live={"api.1", "api.2"}))
        self.assertEqual(nxt.label, leave.CLOSE)

    def test_the_confirmations_own_rows_open_nothing_further(self):
        """A two-level tree and not an open one: the warning's rows describe or commit,
        and neither opens a third surface."""
        for row in tabmenu.confirm_rows(self.TAB, live={"api.1", "api.2"}):
            self.assertIsNone(tabmenu.opens(row, self.TAB, live=set()), row)

    def test_a_row_that_could_not_run_says_why_on_the_frames_own_row(self):
        """The pane a refusal would otherwise be drawn in is the one the menu is about to
        kill, so the sentence goes to the frame's attention row — a different pane and a
        different process (`commands_frame._say_on_screen`)."""
        said = []
        self.enterContext(mock.patch.object(
            commands_frame, "_say_on_screen",
            side_effect=lambda fid, message, **kw: said.append((fid, message))))
        tabmenu.act(tabmenu.catalogue(self.TAB)[0], self.TAB, fid=self.FID)
        self.assertEqual(self.spawned, [])
        self.assertEqual(
            said,
            [(self.FID,
              "no previous transcript for this chat — one is captured when a plane is "
              "quit (`F2 → charter: quit`) and offered on the chat that comes back")])

    def test_a_row_with_nothing_to_say_writes_no_notice(self):
        """The notice is a WRITE, so an empty one would blank whatever the attention row
        was already carrying. `leave`'s *nothing left to stop* row starts nothing and
        carries no note, which is the row that makes this reachable."""
        said = []
        self.enterContext(mock.patch.object(
            commands_frame, "_say_on_screen",
            side_effect=lambda fid, message, **kw: said.append((fid, message))))
        row = tabmenu.confirm_rows("api.9", live=set())[0]
        self.assertEqual(row.note, "")
        tabmenu.act(row, "api.9", fid=self.FID)
        self.assertEqual(said, [])
        self.assertEqual(self.spawned, [])

    def test_a_cancel_starts_nothing_and_says_nothing(self):
        """**Escape, `F12`, or the pane's writer going away** — `overlay.Surface.run`
        answers `None` for all three, and it is the commonest way this surface ends,
        because the row under the cursor when Escape is pressed is one keypress from a
        confirmation that stops a harness. Nothing was chosen, so nothing is started and
        nothing is said."""
        said = []
        self.enterContext(mock.patch.object(
            commands_frame, "_say_on_screen",
            side_effect=lambda fid, message, **kw: said.append((fid, message))))
        tabmenu.act(None, self.TAB, fid=self.FID)
        self.assertEqual(said, [])
        self.assertEqual(self.spawned, [])

    def test_a_row_that_ran_says_nothing_at_all(self):
        """What a started action surfaces through is `inflight`, the frame's existing
        spinner, and never a second sentence here — `_draw_palette`'s rule kept."""
        said = []
        self.enterContext(mock.patch.object(
            commands_frame, "_say_on_screen",
            side_effect=lambda fid, message, **kw: said.append((fid, message))))
        tabmenu.act(tabmenu.confirm_rows(self.TAB, live={"api.1", "api.2"})[0], self.TAB,
                    fid=self.FID)
        self.assertEqual(said, [])
        self.assertEqual(len(self.spawned), 1)

    def test_a_tab_with_nothing_to_stop_offers_no_keypress_that_commits(self):
        """`leave.confirm_rows`' own promise, inherited whole: a plan with nothing in it
        draws one refused row and NO confirming row, so there is no Enter that quietly
        succeeds at nothing."""
        rows = tabmenu.confirm_rows("api.9", live=set())
        self.assertEqual([r.refused for r in rows], [True])
        self.assertFalse(tabmenu.chose(rows[0], "api.9", fid=self.FID))
        self.assertEqual(self.spawned, [])


class TheRowIdsAreNotActionIdsAndTheFallbacksAreReachable(PersonaIso, unittest.TestCase):
    """The two things nobody reads and both of which decide something.

    `tests/test_what_a_quit_says_is_spelled_where_it_is_asserted.py` makes the identical
    pair of claims about `frame/leave.py`'s ids, and this is that file's argument one
    surface over: the properties that are load-bearing first, the literals second, because
    the sweep's re-tuning preserves punctuation and every shape assertion below holds for
    `ubc:dmptf` too.
    """

    def test_no_menu_row_id_can_be_an_action_id(self):
        """The `:` is the whole mechanism. A provider that shipped an action called
        `tab:close` could take the keypress that closes a chat; `component.usable_id` is
        what makes that unsayable, and it is what `palette.matches` gates id-matching on —
        so neither id is reachable by typing either."""
        for rid in (tabmenu.TRANSCRIPT_ID, tabmenu.CLOSE_ID):
            self.assertFalse(component.usable_id(rid), rid)
        for row in tabmenu.catalogue("api.2"):
            self.assertFalse(palette.matches(row.id, row))

    def test_the_two_shapes_are_these_two_strings(self):
        """The hand-spelled half, and the only thing that kills a `retune-string` mutant."""
        self.assertEqual(tabmenu.TRANSCRIPT_ID, "tab:transcript")
        self.assertEqual(tabmenu.CLOSE_ID, "tab:close")
        self.assertEqual(tabmenu.TAB_OPTION, "--tab")

    def test_the_two_titles_and_the_heading_are_these_words(self):
        """Operator-visible prose, spelled where it is asserted. A menu that stopped naming
        the tab would still pass every `assertIn(TAB, …)` above if the name were the only
        thing left in the string."""
        self.assertEqual(tabmenu.label("api.2"), "chat api.2")
        self.assertEqual(tabmenu.transcript_title("api.2"),
                         "chat: previous transcript — api.2")
        self.assertEqual(tabmenu.close_title("api.2"),
                         "chat: close api.2 — stop it and do not bring it back")

    def test_a_pane_that_was_told_nothing_still_hands_the_harness_back(self):
        """**Every fallback in `handback` is reachable, and this is the state that reaches
        them.** A pane split below `tmuxctl.PANE_ENV_FLOOR` is given no `-e` payload at
        all, so `$CHARTER_SESSION_ID` is whatever the shared server holds — which may be
        nothing — and a frame charter has lost the record of answers `None` for its
        harness pane. `None` is not `""`: it would reach a tmux target as the four
        characters `None`, naming a pane that cannot exist, where `""` is what every
        charter reader already treats as absent."""
        self.assertEqual(tabmenu.handback({}), ("", commands_frame.SOCKET, "", ""))

    def test_the_reachable_half_is_a_frame_id_that_came_back_empty(self):
        """**Which of these fallbacks production really reaches, measured rather than
        assumed.** `layout._env_argv` emits `-e NAME=` for every name including an empty
        one, and tmux exports it — measured on 3.7c, the name is present in the split
        pane's environment with an empty value — and `_relayout_pane_env` never declines
        to build the payload at all, because `tmuxctl.PANE_ENV_FLOOR` is 3.0 and charter's
        own floor is 3.2. So the two `env` names are always PRESENT; what is reachable is
        an empty VALUE, which is what `commands_frame._pressers_chat` answers for a frame
        whose `--chat` arrived empty and whose `$CHARTER_SESSION_ID` did too.

        That state is this case, and it is what reaches the other two fallbacks: charter's
        own socket for a frame with no recorded server (`builtin_actions._server`'s rule),
        and `""` rather than `None` for a harness pane charter has lost the record of —
        the state `builtin_actions.NO_LAYOUT` exists to describe.
        """
        self.assertEqual(
            tabmenu.handback({"CHARTER_SESSION_ID": "", "TMUX_PANE": "%3"}),
            ("", commands_frame.SOCKET, "", "%3"))

    def test_a_pane_that_was_told_everything_uses_what_it_was_told(self):
        _plant("api.1", workspace="api", pane="%7")
        state.record_server("api.1", "sock-of-its-own")
        self.assertEqual(
            tabmenu.handback({"CHARTER_SESSION_ID": "api.1", "TMUX_PANE": "%9"}),
            ("api.1", "sock-of-its-own", "%7", "%9"))


class AnEmulatorThatEatsButtonTwoCostsNothing(_ABarThatWasDrawn, unittest.TestCase):
    """The risk this feature is built around, as a property rather than a hope.

    tmux forwards button 2 to a reporting pane — measured on 3.2 and 3.7c by injecting SGR
    bytes into a real client over a real pty. That injection **bypasses the terminal
    emulator**, and whether an emulator forwards button 2 to a mouse-reporting application
    or serves its own context menu is emulator-dependent and configurable: iTerm2 3.6.11
    ships `"Button,1,1,," -> kContextMenuPointerAction` alongside a profile whose
    `Mouse Reporting` is on, and the precedence between those two is not determinable from
    the plist.

    So on some machines the press will simply never arrive, and the only acceptable
    failure mode is *nothing happens* — never a refusal, never a traceback, never a
    half-open menu.
    """

    def test_nothing_new_is_asked_of_the_terminal(self):
        """A feature that needed 1002 or 1003 to see a right-click would change what every
        panel writes to its own tty. It does not: button 2 arrives in the same 1000+1006
        report a left click does, so a panel's request is byte-for-byte what it was."""
        from charter.frame import events
        self.assertEqual(events.wanted(builtins.build(self.FID).get("chats")),
                         (overlay.CLICK,))
        self.assertEqual(overlay.MOUSE_ON, "\x1b[?1006h\x1b[?1000h")

    def test_the_menu_button_is_never_a_line_in_the_config_text(self):
        """**A bind is what would break this, not what enables it.** tmux's own default
        `MouseDown3Pane` takes its `send -M` branch because `#{mouse_any_flag}` is 1 for a
        pane that asked for reporting; a custom `bind -n MouseDown3Pane` that omitted
        `send -M` would swallow the press instead.

        **#848 added one, and the config text still names no button-2 or button-3 key** —
        which is not an accident this case now tolerates but the property it now pins. That
        bind wraps whatever the server already had, so it carries a page of tmux's own
        `display-menu` as an argument; written into the text `source-file` parses, one
        unbalanced brace in an operator's own binding would take `mouse`, `history-limit`
        and the palette's hotkey down with it. It is issued as its own `bind-key` argv
        instead (`commands_frame._menu_button_argv`), and this is what would notice a line
        for it appearing here."""
        from charter import commands_frame
        text = commands_frame.conf_text(hotkey="F2", mouse=True, history_limit=2000,
                                        session=self.FID)
        for key in ("MouseDown3Pane", "MouseDown2Pane", "MouseUp3Pane"):
            self.assertNotIn(key, text)

    def test_the_palette_keeps_every_row_this_menu_holds(self):
        """**`F2` stays complete.** A right-click menu is invisible until you try it, so
        it is a shortcut and never the only route. Both rows are still reachable exactly
        the way they always were, and this file removes nothing from the palette."""
        reg = builtin_actions.build(self.FID, current_density="normal",
                                    current_chrome="off")
        titles = [o.title for o in reg.offers(fid=self.FID, snapshot={})]
        self.assertIn("chat: previous transcript", titles)
        self.assertIn("chat: close — stop this chat and do not bring it back",
                      [r.title for r in leave.open_rows(self.FID)])

    def test_a_button_charter_has_no_name_for_starts_nothing(self):
        """`overlay._SGR_BUTTONS` names three buttons and drops every other number, so a
        thumb button cannot arrive wearing `left`'s name. If one ever did, the handler
        answers nothing rather than acting on it."""
        for name in ("up", "down", ""):
            self._handler("chats")(_click(0, self._column_of(" api.3"), name=name))
        self.assertEqual(self.spawned, [])


class _Args:
    """The parsed argv `frame-palette` hands its two halves, as a stand-in for one.

    `argparse.Namespace` in production; a plain object here so a case can spell the one
    attribute it is about without building a parser that would then also be under test.
    """

    def __init__(self, **kw):
        for name, value in kw.items():
            setattr(self, name, value)
