"""A close confirmation names ONE chat, and the chat it names is the one that stops — #933.

The report is two screenshots that contradict each other: a tab menu headed `chat
default.1` with `chat: close default.1` under the cursor, and — the operator's account —
one Enter later, a confirmation reading `close default.2`, the chat that was *active*.
Read straight, that is the target being re-resolved at the doorway, from "the chat the
menu names" to "the chat I am in", which would be the worst defect this surface can have.

**It is not what happened, and the operator's own plane is what says so.** With that
plane's frame directories and the live set its server actually reports,
`leave.plan(only="default.1")` produces `close default.1` — never `close default.2`. The
second screenshot's text is producible from exactly two states, `only="default.2"` and a
plane-wide plan with `default.2` the only live chat, and the plane's third live chat rules
the second out. So the two surfaces were two gestures in two chats, and every route below
resolves its target correctly on a real tmux. **That is asserted here rather than argued**,
because the previous two fixes on this surface were both verified on fixtures where the
menu's target and the active chat were the same chat — the one case that cannot tell a
correct target from a re-resolved one.

**What the report DID find, and what this file is really for**, is three things that made
one gesture indistinguishable from the other:

* **#930's finding, one layer up.** Every assertion this surface had about its confirmation
  said the surface *appeared* — `_shown(pane)` starting with `close` — and #932's own case
  says why it could not say more: its fixture's plan is empty, so `leave.confirm_rows` drew
  its nothing-left-to-stop row and passed. A confirmation about the wrong chat passed every
  one of them. Every case here asserts the chat by NAME, on the row, and then follows it to
  `state.was_closed` and to the window's death.
* **The listing a kill is aimed by was not filtered by plane** — `_chat_seats`, the change
  this file's sibling class is about, and the one thing here that really could have killed
  the wrong window.
* **A close took the frame with it**, which is the operator's *"charter seems closing fully
  and showing pure claude code session"* — `_hand_the_client_a_frame`.

The fixture has two chats and both are LIVE, which is what the previous ones lacked in two
ways at once: `@charter_chat` on each window is what `commands_frame._chat_seats` reads, so
`leave.plan` produces real rows here rather than the empty plan #932 recorded, and the
client sits on the SECOND chat, so "the chat the menu names" and "the chat I am in" are
different chats in every case that opens a menu about the first.
"""

from __future__ import annotations

import os
import time
import unittest
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame
from charter.frame import chats, leave, slots, state

from tests._isolation import PersonaIso, make_plane
from tests.test_a_real_click_on_a_real_tab_bar_switches import (
    _HAS_TMUX, _ARealFrameWithBars, _await)


class _TwoLiveChatsWithTheSecondOnScreen(_ARealFrameWithBars):
    """`alpha.1` and `alpha.2`, both live on one server, the client on `alpha.2`.

    **Both halves of "live" are here and neither was in the fixtures this replaces.**
    `_ARealChatBarOverTwoChats` builds two chats and two windows but marks neither with
    `@charter_chat`, so `commands_frame._chat_seats` reports nothing, `leave.plan` marks
    every chat `live=False`, and `leave.confirm_rows` draws *no chats are open on this
    plane* — a confirmation with no confirming row, which is the surface #932's case
    admitted it was asserting against. Setting the option is what makes the plan real, and a
    real plan is the only thing that can name the wrong chat.

    The client is on the SECOND chat, and the chats bar is that chat's own panel, so a menu
    opened about `alpha.1` is a menu about a chat the operator is not in — the case both
    previous fixes were verified without.
    """

    WS = "alpha"

    def setUp(self) -> None:
        super().setUp()
        self.first, self.here = f"{self.WS}.1", f"{self.WS}.2"
        self.other_harness = self._start_session(self.first, session=self.WS)
        second = self._tmux("new-window", "-t", self.WS, "-P", "-F", "#{pane_id}",
                            "cat", env=self.env)
        self.assertEqual(second.returncode, 0, second.stderr)
        self.harness = second.stdout.strip()
        for chat, pane in ((self.first, self.other_harness),
                           (self.here, self.harness)):
            state.frame_dir(chat, create=True)
            state.record_workspace(chat, self.WS)
            state.record_server(chat, self.socket)
            state.record_harness_pane(chat, pane)
            state.record_identity(chat, {"CHARTER_SESSION_ID": chat,
                                         "CHARTER_ROOT": str(self.plane),
                                         "CHARTER_HARNESS": "claude-code"})
            # `commands_frame._chat_option_argv`'s own write, made the way the launcher
            # makes it: this is what `_chat_seats` reads, and without it the plan below is
            # the empty one #932 was measured against.
            marked = self._tmux("set-option", "-w", "-t", self._window_of(pane),
                                commands_frame._CHAT_OPTION, chat)
            self.assertEqual(marked.returncode, 0, marked.stderr)
        self._source_conf(self.WS)
        self.assertEqual(self._tmux("select-window", "-t", self.harness).returncode, 0)
        self.bar = self._split_bar(self.harness, "chats", self.here)
        state.record_panes(self.here, panels={"chats": self.bar})
        self.assertEqual(self._tmux("select-pane", "-t", self.harness).returncode, 0)
        self.fd = self._attach(self.WS)
        self.assertTrue(
            _await(lambda: self.first in self._bar_row(self.bar)),
            f"the chat bar never painted both chats: {self._bar_row(self.bar)!r}")
        self.assertIn(f"*{self.here}", self._bar_row(self.bar),
                      "the client is not on the second chat, so every case below is the "
                      "one-chat case the previous fixtures already covered")

    #: What the chat strip ends in — `slots._affordances`, composed from the constants.
    TAIL = f"{slots.ADD_CHAT} {slots.CLOSE_CHAT}"

    def _minus(self) -> int:
        """The column the `-` is drawn in, read off the pane tmux painted."""
        row = self._bar_row(self.bar).rstrip()
        self.assertTrue(row.endswith(self.TAIL),
                        f"the row does not end in the affordances: {row!r}")
        return len(row) - 1

    def _overlay_from(self, do) -> str:
        """Do *do*, and hand back the one pane it opened on this frame."""
        before = set(self._panes(self.WS))
        do()
        self.assertTrue(_await(lambda: set(self._panes(self.WS)) - before, timeout=20.0),
                        "the gesture opened no pane at all, so nothing below is measured")
        return (set(self._panes(self.WS)) - before).pop()

    def _menu_about(self, target: str, do) -> str:
        """Open a menu with *do* and assert it is about *target* before anything is typed.

        The menu names its chat on both rows (`tabmenu.transcript_title`,
        `tabmenu.close_title`) and in its heading (`tabmenu.label`), so this is where a
        target that was wrong from the start would be caught — before Enter, and separately
        from the surface it opens.
        """
        pane = self._overlay_from(do)
        self.assertTrue(
            _await(lambda: f"chat: close {target}" in self._shown(pane), timeout=20.0),
            f"the pane never drew a menu about {target}: {self._shown(pane)!r}")
        return pane

    def _confirmation_on(self, pane: str) -> str:
        """Press Enter and hand back the confirmation it draws in the same pane."""
        os.write(self.fd, b"\r")
        self.assertTrue(
            _await(lambda: self._shown(pane).split("\n")[0].startswith(leave.CLOSE),
                   timeout=20.0),
            f"Enter did not reach the close confirmation: {self._shown(pane)!r}")
        # The drawer is `_as_a_drawer`'s `resize-pane -Z`, a round trip after the paint, so
        # the rows can still be arriving when the heading lands.
        self.assertTrue(
            _await(lambda: len(self._shown(pane).strip().split("\n")) > 1, timeout=10.0),
            f"the confirmation drew a heading and no rows: {self._shown(pane)!r}")
        return self._shown(pane)

    def _says_only(self, shown: str, target: str, other: str) -> None:
        """The assertion #929 asked for, made about the sentence rather than the surface.

        `leave._close_summary` spells the confirming row `close <chat> — stop it and forget
        it`, so the chat is IN the row the cursor is on and a confirmation about the wrong
        one is a string this can read. Both halves are needed: naming the right chat while
        also listing the other is `leave.plan`'s `only=` lost, which is the plane-wide plan
        wearing close's title.
        """
        self.assertIn(f"close {target} —", shown,
                      f"the confirmation does not say it will close {target}")
        self.assertNotIn(other, shown,
                         f"the confirmation about {target} also names {other}")


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class AMenuAboutAnotherTabStopsThatTabAndOnlyThatTab(
        _TwoLiveChatsWithTheSecondOnScreen, unittest.TestCase):
    """A right press on the tab you are NOT on — the shape of the report's first screenshot.

    Every previous case on this surface opened its menu about the chat the frame was
    already on, where a target read from the frame and a target read from the pointer are
    the same string. This is the one that can tell them apart, and it follows the answer all
    the way down: what the menu says, what the confirmation says, and which window dies.
    """

    def test_the_menu_the_confirmation_and_the_kill_are_one_chat(self):
        pane = self._menu_about(
            self.first,
            lambda: self._click(self.bar,
                                col=self._column_of(self.bar, f" {self.first}"),
                                button=2))
        self.assertIn(f"chat {self.first}", self._shown(pane).split("\n")[0],
                      "the menu's own heading does not name the chat it is about")

        shown = self._confirmation_on(pane)
        self._says_only(shown, self.first, self.here)

        os.write(self.fd, b"\r")
        self.assertTrue(_await(lambda: state.was_closed(self.first), timeout=20.0),
                        "the confirmation's Enter never reached `state.record_closed`")
        self.assertTrue(
            _await(lambda: self._window_of(self.other_harness) not in
                   self._tmux("list-windows", "-a", "-F", "#{window_id}").stdout.split(),
                   timeout=20.0),
            "the chat was marked closed and its window is still on the server")
        self.assertFalse(state.was_closed(self.here),
                         "closing the other tab marked the chat the operator was in")
        self.assertEqual(chats.of_workspace(self.WS), [self.here])

    def test_the_chat_the_operator_is_in_keeps_its_window_and_its_frame(self):
        """The other half of "one chat": a close aimed at another tab moves nobody.

        `_hand_the_client_a_frame` is deliberately silent unless the closed chat was the
        presser's own — a switch here would drag the operator off the chat they were reading
        to answer a question they did not ask.
        """
        window = self._current_window(self.WS)
        pane = self._menu_about(
            self.first,
            lambda: self._click(self.bar,
                                col=self._column_of(self.bar, f" {self.first}"),
                                button=2))
        self._confirmation_on(pane)
        os.write(self.fd, b"\r")
        self.assertTrue(_await(lambda: state.was_closed(self.first), timeout=20.0))
        time.sleep(2.0)

        self.assertEqual(self._current_window(self.WS), window,
                         "closing another tab moved the client off the chat it was on")
        self.assertIn(self.harness, self._panes(self.WS),
                      "closing another tab took this chat's harness with it")


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class TheMinusStopsTheChatItWasPressedIn(_TwoLiveChatsWithTheSecondOnScreen,
                                        unittest.TestCase):
    """`slots.CLOSE_CHAT` with a sibling chat on the strip beside it.

    The `-` is *"byte for byte the argv the right press builds, with the frame's own chat
    where `tab_at` would have put the tab under the pointer"* — so with two chats on the
    strip the two gestures aim at DIFFERENT chats, and this is the case that says the `-`
    aims at the one the operator is in rather than at whichever tab is drawn first.
    """

    def test_it_names_and_stops_the_chat_the_operator_is_in(self):
        pane = self._menu_about(
            self.here, lambda: self._click(self.bar, col=self._minus()))

        shown = self._confirmation_on(pane)
        self._says_only(shown, self.here, self.first)

        os.write(self.fd, b"\r")
        self.assertTrue(_await(lambda: state.was_closed(self.here), timeout=20.0),
                        "the `-` route never reached `state.record_closed`")
        self.assertFalse(state.was_closed(self.first),
                         "the `-` marked the tab the operator was not on")
        self.assertEqual(chats.of_workspace(self.WS), [self.first])

    def test_the_frame_comes_back_on_the_chat_the_operator_lands_on(self):
        """**The operator's third symptom, and it is not a crash** — #933.

        *"after again closing it — charter seems closing fully and showing pure claude code
        session."* The frame is laid out on one chat at a time (`cmd_chat`'s step 2 is
        `_apply_arrangement(<chat being left>, want=[])`), so every chat the operator is not
        on is a window holding its harness and nothing else. A `kill-window` moves them onto
        one of those without going through `cmd_chat`, and what they see is the harness
        filling the screen with no strips, no attention row and no `F2` hint — charter
        having exited, reached by pressing the row that says *stop it and do not bring it
        back*.

        `state.panes` is the record `_split_panels` writes last, so a chat that has it has
        been laid out — the same observable `test_the_click_leaves_the_keyboard_on_a_harness`
        waits for, because it is the same switch reached through the same front door.
        """
        self.assertEqual(state.panes(self.first), {},
                         "the surviving chat already has panels, so this case would pass "
                         "on the state it exists to create")
        pane = self._menu_about(
            self.here, lambda: self._click(self.bar, col=self._minus()))
        self._confirmation_on(pane)

        os.write(self.fd, b"\r")
        self.assertTrue(_await(lambda: state.was_closed(self.here), timeout=20.0))
        self.assertTrue(
            _await(lambda: bool(state.panes(self.first)), timeout=25.0),
            f"the chat the operator was handed has no frame: {state.panes(self.first)!r}")


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class AWindowOnANOTHERPlaneIsNotThisPlanesChat(PersonaIso, unittest.TestCase):
    """`_chat_seats`, against two real planes on ONE real server — the wrong-target defect.

    **One tmux server carries every plane on the machine** (§3.3), and chat ids collide by
    construction: `default` is `config.DEFAULT_WORKSPACE_FALLBACK` and `default.1` is the id
    every plane's first chat gets. Measured on the reporting operator's own socket while
    this was written: three planes, each with a `default.1` in its frame root.

    `_plane_live` turns the listing into `windows[server][chat]` — one entry per id, **last
    row wins** — and `_stop_chats` aims `kill-window` by it. So before this change a close on
    one plane could kill another plane's window, which is a destructive act on a target the
    operator never named. This is the case that would have gone red, and it needs no client
    and no pointer: it is about what one command asks a server and what it does with the
    answer.

    **The marker is a veto and never a finder**, which is `_plane_session`'s rule and the
    reason the second case here matters as much as the first: a session an older charter
    created carries no marker, so reading "unmarked" as "not mine" would make every
    pre-marker frame read as dead — and a quit that read a live plane as empty would record
    nothing and kill nothing while telling the operator it had done both.
    """

    WS = "default"

    def setUp(self) -> None:
        super().setUp()
        from charter.frame import tmuxctl

        from tests import _tmuxreap
        v = tmuxctl.version()
        if v is None or v < tmuxctl.FLOOR:
            self.skipTest(f"the frame's floor is tmux {tmuxctl.FLOOR[0]}."
                          f"{tmuxctl.FLOOR[1]}; this machine has {v}")
        self.socket = _tmuxreap.name("twoplanes")
        self.addCleanup(lambda: self._tmux("kill-server"))
        self.plane = make_plane(self, "schema = 1\n")
        self.chat = f"{self.WS}.1"

    def _tmux(self, *args: str):
        import subprocess
        return subprocess.run(["tmux", "-L", self.socket, *args], capture_output=True,
                              text=True, timeout=20)

    def _session(self, name: str, *, chat: str, plane: str) -> str:
        """One session holding one chat window, marked for *plane*.

        The two options are written the way charter writes them — `@charter_chat` on the
        WINDOW and `@charter_plane` on the SESSION, both aimed at the harness pane, which is
        `_chat_option_argv`'s and `_plane_option_argv`'s own targeting.
        """
        made = self._tmux("-f", "/dev/null", "new-session", "-d", "-s", name,
                          "-P", "-F", "#{pane_id}", "cat")
        self.assertEqual(made.returncode, 0, made.stderr)
        pane = made.stdout.strip()
        self.assertEqual(self._tmux("set-option", "-w", "-t", pane,
                                    commands_frame._CHAT_OPTION, chat).returncode, 0)
        if plane:
            self.assertEqual(self._tmux("set-option", "-t", pane,
                                        commands_frame._PLANE_OPTION, plane).returncode, 0)
        return pane

    def _chat_on_this_plane(self, pane: str) -> None:
        state.frame_dir(self.chat, create=True)
        state.record_workspace(self.chat, self.WS)
        state.record_server(self.chat, self.socket)
        state.record_harness_pane(self.chat, pane)
        state.record_identity(self.chat, {"CHARTER_HARNESS": "claude-code"})

    def test_only_this_planes_window_is_a_seat(self):
        mine = self._session("mine", chat=self.chat,
                             plane=commands_frame._this_plane())
        theirs = self._session("theirs", chat=self.chat, plane="/somewhere/else/.charter")

        seats = commands_frame._chat_seats(self.socket)

        self.assertEqual([w for _c, w, _a in seats], [self._window_id(mine)],
                         f"another plane's window is a seat on this plane: {seats!r}")
        self.assertNotIn(self._window_id(theirs), [w for _c, w, _a in seats])

    def test_an_unmarked_session_is_still_this_planes(self):
        """The control the veto needs. A filter that dropped every window it could not
        positively claim would pass the case above and break every frame launched by a
        charter that predates the marker."""
        older = self._session("older", chat=self.chat, plane="")

        seats = commands_frame._chat_seats(self.socket)

        self.assertEqual([w for _c, w, _a in seats], [self._window_id(older)])

    def test_a_close_leaves_the_other_planes_window_standing(self):
        """**The whole defect, end to end**: the id is the same, the server is the same, and
        the window that dies is this plane's."""
        mine = self._session("mine", chat=self.chat,
                             plane=commands_frame._this_plane())
        theirs = self._session("theirs", chat=self.chat, plane="/somewhere/else/.charter")
        self._chat_on_this_plane(mine)

        rc = commands_frame.cmd_close(SimpleNamespace(chat_id=self.chat, chat=""))

        self.assertEqual(rc, 0)
        self.assertTrue(state.was_closed(self.chat))
        alive = self._tmux("list-windows", "-a", "-F", "#{window_id}").stdout.split()
        self.assertIn(self._window_id(theirs), alive,
                      "closing this plane's chat killed another plane's window")
        self.assertNotIn(self._window_id(mine), alive,
                         "this plane's own window survived, so the kill went elsewhere")

    def _window_id(self, pane: str) -> str:
        return self._tmux("display-message", "-p", "-t", pane,
                          "#{window_id}").stdout.strip()


class WhichChatACloseHandsYou(PersonaIso, unittest.TestCase):
    """`_hand_the_client_a_frame`, asked directly — the three branches a real frame cannot
    all turn red.

    The tmux class above measures the outcome an operator sees, and it is the one that
    matters; what it cannot do is distinguish "the switch was skipped" from "the switch ran
    and happened to land where the client already was". Two of these branches are exactly
    that shape, so they are asked here, of the function, with `cmd_chat` standing in for the
    switch it would start.
    """

    WS = "alpha"

    def setUp(self):
        super().setUp()
        self.asked = []
        self.enterContext(mock.patch.object(
            commands_frame, "cmd_chat",
            side_effect=lambda args: self.asked.append(args.chat_id)))

    def _plant(self, *chats_):
        for chat in chats_:
            state.frame_dir(chat, create=True)
            state.record_workspace(chat, self.WS)

    def test_closing_your_own_chat_hands_you_the_first_surviving_tab(self):
        self._plant(f"{self.WS}.1", f"{self.WS}.2")

        commands_frame._hand_the_client_a_frame(f"{self.WS}.1", fid=f"{self.WS}.1")

        self.assertEqual(self.asked, [f"{self.WS}.2"])

    def test_the_chat_being_closed_is_never_what_you_are_handed(self):
        """**The mark has not been written yet when this runs**, deliberately — the switch
        has to go before the kill, and `state.record_closed` goes with the kill. So the
        chat about to stop is still on `chats.of_workspace`'s list (#930 drops it from the
        moment it is marked, which is later), and it is dropped here by name.

        The case closes the FIRST tab, because that is the only one where the difference
        shows: with the closed chat second, the first survivor is the right answer whether
        or not anything filtered it out, and `chats.check` would then refuse the switch as
        *already here* — silently, on a row nobody can read.
        """
        self._plant(f"{self.WS}.1", f"{self.WS}.2", f"{self.WS}.3")

        commands_frame._hand_the_client_a_frame(f"{self.WS}.1", fid=f"{self.WS}.1")

        self.assertEqual(self.asked, [f"{self.WS}.2"])

    def test_closing_another_tab_hands_you_nothing(self):
        """A close aimed at a chat you are not in moves nobody. Without this branch the
        switch would fire on every close and drag the operator off the chat they were
        reading — and on the two-chat frame above it would land them where they already
        were, which is the shape no end-to-end case can see."""
        self._plant(f"{self.WS}.1", f"{self.WS}.2")

        commands_frame._hand_the_client_a_frame(f"{self.WS}.1", fid=f"{self.WS}.2")

        self.assertEqual(self.asked, [])

    def test_the_workspaces_last_chat_hands_you_nothing(self):
        """Killing a session's last window ends the session and gives the client its
        terminal back, which is the right outcome and not one to switch away from."""
        self._plant(f"{self.WS}.1")

        commands_frame._hand_the_client_a_frame(f"{self.WS}.1", fid=f"{self.WS}.1")

        self.assertEqual(self.asked, [])

    def test_a_switch_that_raises_never_costs_the_close(self):
        """**The courtesy runs in front of a teardown**, so an exception escaping it would
        leave the chat open, unmarked and un-killed while the operator watched their
        confirmation disappear — *"after choosing close it's not closing"*, the report this
        area is answering, reintroduced by the fix for another half of it.

        Asked of `cmd_close` rather than of the helper, because "the close still happens" is
        a fact about the command and the helper alone cannot say it.
        """
        self._plant(f"{self.WS}.1", f"{self.WS}.2")
        for chat in (f"{self.WS}.1", f"{self.WS}.2"):
            state.record_server(chat, commands_frame.SOCKET)
            state.record_harness_pane(chat, "%1")
        live = ({f"{self.WS}.1", f"{self.WS}.2"},
                {commands_frame.SOCKET: {f"{self.WS}.1": "@0", f"{self.WS}.2": "@1"}},
                set())
        commands_frame.cmd_chat.side_effect = RuntimeError("the switch blew up")

        with mock.patch.object(commands_frame, "_plane_live", return_value=live), \
                mock.patch.object(commands_frame, "_stop_chats", return_value=1):
            rc = commands_frame.cmd_close(
                SimpleNamespace(chat_id=f"{self.WS}.1", chat=f"{self.WS}.1"))

        self.assertEqual(rc, 0)
        self.assertTrue(state.was_closed(f"{self.WS}.1"),
                        "a switch that raised took the close's own marker with it")

    def test_a_chat_with_no_recorded_workspace_hands_you_nothing(self):
        """`state.own_workspace` answers ``None`` for the migration case (`leave.plane_chats`
        is the scan that exists for it), and a workspace charter cannot name has no roster to
        pick a survivor from. Nothing is guessed at.

        **No guard stands behind this and it is measured rather than guarded**:
        `chats.of_workspace` answers ``[]`` for ``None`` and for ``""`` alike, so the roster
        is empty and the one survivor guard answers. An `if ws` in front of it read as a
        second refusal and the sweep found it idle — see `_hand_the_client_a_frame`.
        """
        state.frame_dir(f"{self.WS}.1", create=True)

        commands_frame._hand_the_client_a_frame(f"{self.WS}.1", fid=f"{self.WS}.1")

        self.assertEqual(self.asked, [])


if __name__ == "__main__":
    unittest.main()
