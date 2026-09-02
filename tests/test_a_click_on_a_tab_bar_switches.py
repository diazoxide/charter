"""A click on the `chats` or `workspaces` bar switches this frame — and the four gestures
that deliberately do not.

The report this answers is *"I put both bars on my plane, clicked a tab, and nothing
happened."* Nothing was going to: both components shipped registered `needs=()` with no
``events``/``on_event`` at all, so tmux routed the report to the pane, `frame/events.py`
decoded it, and `Dispatcher._deliver` dropped it for a kind the component had not
declared. A bar with no handler is a caption that happens to list names.

**The design question this branch had to answer, and where the answer lives.** `repos` —
charter's first pointer consumer — holds a rule that a click SELECTS and never chooses,
because a pointer event can arrive unpaired (§4i). A tab bar is on the other side of that
rule and `frame/builtins._bar_events` argues why in full; the three cases at
:class:`AClickIsTheWholeGesture` are that argument as assertions: it acts on the PRESS
(which is never the unpaired half), the switch is reversible by the same gesture, and
there is no chooser a select-then-confirm bar could be confirmed with — `key` is a kind
`events.DELIVERED` does not carry, because the harness owns the keyboard.

**Everything the click starts is an existing front door.** It does not switch in this
process: it starts `charter frame-chat <id>` or `charter frame-switch --workspace <name>`
detached, which is the same argv `commands_frame._start_chat_switch` and a hand-typed
switch already use. That is what puts every refusal on the operator's screen — a panel
process has no `display-message` surface of its own, and a click that silently did
nothing is the report this feature is answering.

The column half — which name is at which column, and which cells are not a tab at all —
is `tests/test_frame_bars.AClickResolvesAgainstWhatWasDrawn`. The real-tmux half is
`tests/test_a_real_click_on_a_real_tab_bar_switches.py`.
"""

from __future__ import annotations

import os
import unittest
from unittest import mock

from charter import commands_frame, config, tui, util
from charter.frame import (builtin_actions, builtins, chats, component, events, overlay,
                           slots, state)

from tests._isolation import PersonaIso
from tests.test_frame_chat_switch import _plant


def _press(col: int, *, name: str = "left", pressed: bool = True):
    """One decoded left-button press at *col* of the component's own canvas.

    `overlay.Event` and not a stand-in: `events.Dispatcher._on_canvas` hands the handler
    exactly this, with the operator's `[frame] pad` already subtracted, so a case that
    invented its own record would be asserting against a shape nothing produces.
    """
    return overlay.Event(overlay.CLICK, name, row=0, col=col, pressed=pressed)


class _ABarThatWasDrawn(PersonaIso):
    """A plane with something to switch between, and the bar drawn over it.

    **The bar is DRAWN before every click**, never hand-published, because the column map
    is the paint's own output (`slots._bar`) and a case that published a map by hand would
    be measuring a fixture. Which column to click therefore comes from the row that was
    painted, exactly as it does for the operator.
    """

    #: Wide enough for every rung-1 name. Narrow rungs have their own cases in
    #: `test_frame_bars`; this file is about what happens after a tab is resolved.
    WIDTH = 200

    def setUp(self):
        super().setUp()
        slots.TABS.forget()
        self.addCleanup(slots.TABS.forget)
        self.spawned = []
        self.enterContext(mock.patch.object(
            builtin_actions, "_spawn",
            side_effect=lambda argv, *, fid: self.spawned.append((argv, fid))))

    def _handler(self, cid: str, fid: str):
        """*cid*'s handler out of the REAL registry, closed over *fid*.

        `builtins.build(fid)` rather than a handler reached directly, so every case here
        also asserts that `Component.__post_init__` accepted the declaration: it refuses
        an `on_event` with no `events` and `events` with no `on_event`, so a component
        that lost either half fails at this line rather than in an assertion about a
        gesture.
        """
        return builtins.build(fid).get(cid).on_event

    def _column_of(self, row: str, field: str) -> int:
        """Which COLUMN of the drawn *row* the field *field* starts in.

        **`tui.width` of what comes before it, never the character index.** The bar paints
        the tab you are on as a reverse-video block (`chrome.block`), so a field to the
        right of it sits further into the STRING than it sits into the pane — and a case
        that pressed the character index would be pressing a column the operator's eye is
        not on, which is the one mistake every case in this file exists to catch. `width`
        counts no SGR, so this is the same number it always was on an unpainted row.
        """
        at = row.index(field)
        self.assertNotIn(field, row[at + 1:], f"{field!r} is not unique in {row!r}")
        return tui.width(row[:at])


class AClickOnAChatTabStartsTheChatSwitch(_ABarThatWasDrawn, unittest.TestCase):
    """The `chats` bar, over a real frame directory."""

    def setUp(self):
        super().setUp()
        self.enterContext(mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "api"},
                                          clear=False))
        for chat in ("api.1", "api.2", "api.3"):
            _plant(chat, workspace="api")
        self.row = slots.chats_bar("api.1", self.WIDTH)[0]
        self.on_event = self._handler("chats", "api.1")

    def test_a_press_on_another_chats_tab_starts_the_switch_to_it(self):
        self.on_event(_press(self._column_of(self.row, " api.3")))
        self.assertEqual(
            self.spawned, [(util.self_relaunch_argv("frame-chat", "api.3"), "api.1")])

    def test_it_starts_exactly_what_the_palette_row_starts(self):
        """One switch, two front doors. `commands_frame._start_chat_switch` is what a
        palette row runs, and a bar that assembled its own argv would be a second answer
        to how a chat switch is performed — the shape `frame/slots.drawable` exists to
        stop one question having two."""
        self.on_event(_press(self._column_of(self.row, " api.2")))
        commands_frame._start_chat_switch("api.1", "api.2")
        by_click, by_palette = self.spawned
        self.assertEqual(by_click, by_palette)

    def test_the_child_is_told_which_frame_it_is_rather_than_inheriting_one(self):
        """`_spawn`'s `fid=` is the child's own `$CHARTER_SESSION_ID`. One tmux server is
        shared by every frame on the machine, so a child left to read that variable out of
        an inherited environment can act on somebody else's frame — the trap
        `state.record_identity` measures. The panel was TOLD which frame it draws
        (`charter panel chats --session <fid>`), so it is the one process that knows."""
        self.on_event(_press(self._column_of(self.row, " api.2")))
        self.assertEqual([fid for _argv, fid in self.spawned], ["api.1"])

    def test_no_chat_option_rides_along_with_it(self):
        """The palette's row carries `--chat` because a `bind` text is shared by every
        frame on the socket and `#{@charter_chat}` is the only thing that tells two chats
        of one session apart. A panel has no such ambiguity and `_spawn` states the id
        outright, so the option would be a second spelling of a value the environment
        already carries — provably equal, which the sweep reports and this repo deletes."""
        self.on_event(_press(self._column_of(self.row, " api.2")))
        self.assertNotIn("--chat", self.spawned[0][0])

    def test_only_this_planes_own_chat_directories_are_on_the_bar(self):
        """**Membership is read from the plane, never from a live session name** (#684).
        A chat of another workspace shares the tmux server and may share a session name;
        what decides is `state.frame_workspace`, a file in this plane's own
        `.charter/frame/`. So there is no column for it and no argv that could name it."""
        _plant("web.1", workspace="web")
        row = slots.chats_bar("api.1", self.WIDTH)[0]
        self.assertNotIn("web.1", row)
        for col in range(0, self.WIDTH):
            self.assertNotEqual(slots.TABS.switch_to(col), "web.1", f"column {col}")

    def test_a_tab_the_command_will_refuse_is_still_handed_to_the_command(self):
        """**The refusal belongs where it can be SAID**, which is not here.

        A chat on another tmux server is in this workspace's roster — the roster is the
        `workspace` file, and that file says nothing about where a chat is running — and
        `chats.check` refuses it, because pane ids are per-server and `select-window`
        would aim at a real, live, unrelated pane and be told it worked (#684). Refusing
        in the handler instead would be a second, weaker copy of that rule AND would be
        silent: a panel has no `display-message` surface, so the operator would be back to
        clicking a tab and watching nothing happen. `cmd_chat` refuses with a sentence on
        the frame's own client.
        """
        state.record_server("api.1", "charter")
        state.record_server("api.3", "somebody-elses")
        self.on_event(_press(self._column_of(self.row, " api.3")))
        self.assertEqual(
            self.spawned, [(util.self_relaunch_argv("frame-chat", "api.3"), "api.1")])
        refused = chats.check("api.1", "api.3")
        self.assertFalse(refused.ok)
        self.assertIn("not on this frame's tmux server", refused.message)


class AClickOnAWorkspaceTabStartsTheWorkspaceSwitch(_ABarThatWasDrawn,
                                                    unittest.TestCase):
    """The `workspaces` bar, over a real plane."""

    def setUp(self):
        super().setUp()
        for name in ("alpha", "beta", "gamma"):
            (config.WORKSPACES_DIR / name).mkdir(parents=True, exist_ok=True)
        state.frame_dir("f1", create=True)
        state.record_workspace("f1", "beta")
        self.enterContext(mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": ""},
                                          clear=False))
        self.row = slots.workspaces_bar("f1", self.WIDTH)[0]
        self.on_event = self._handler("workspaces", "f1")

    def test_a_press_on_another_workspaces_tab_starts_the_switch_to_it(self):
        self.on_event(_press(self._column_of(self.row, " gamma")))
        self.assertEqual(
            self.spawned,
            [(util.self_relaunch_argv("frame-switch", "--workspace", "gamma"), "f1")])

    def test_the_name_goes_last_so_it_cannot_be_read_as_a_flag(self):
        """`frame-switch` takes the workspace as the VALUE of `--workspace`, and
        `frame-chat` takes the chat as a positional — two shapes, one rule this handler
        depends on: the name is the last argument on both, so `_bar_events` can hold the
        difference as a prefix and nothing else."""
        self.on_event(_press(self._column_of(self.row, " alpha")))
        self.assertEqual(self.spawned[0][0][-1], "alpha")

    def test_the_workspace_this_frame_is_on_is_the_FRAMEs_and_not_this_processs(self):
        """#512, arriving at the click. The mark comes from `state.workspace_for(fid)` and
        the map is published beside it in the same paint, so the tab that answers "you are
        already here" is the one the operator can see marked — never one this panel
        process would have resolved for itself out of a shared server's environment."""
        self.assertIn("*beta", self.row)
        for col in range(0, self.WIDTH):
            self.assertNotEqual(slots.TABS.switch_to(col), "beta", f"column {col}")
        self.assertEqual(self.spawned, [])


class AClickIsTheWholeGesture(_ABarThatWasDrawn, unittest.TestCase):
    """Which pointer events act, which do not, and what a handler answers afterwards.

    **This is §4i kept rather than waived.** The rule is that the irreversible half of an
    interaction never rides on a pointer event, because one can arrive unpaired — a drag
    begun on a pane border delivers exactly one release, measured. A switch rides on a
    click here, and these cases are the three reasons that is sound rather than an
    exception: the PRESS is what is acted on and a press is never the unpaired half; the
    switch is undone by the same gesture that made it, because the tab you left is still
    on the bar; and no `key` reaches a panel at all, so a bar that merely SELECTED would
    draw a second mark nothing on the machine could act on.
    """

    def setUp(self):
        super().setUp()
        for name in ("alpha", "beta"):
            (config.WORKSPACES_DIR / name).mkdir(parents=True, exist_ok=True)
        state.frame_dir("f1", create=True)
        state.record_workspace("f1", "alpha")
        self.enterContext(mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": ""},
                                          clear=False))
        self.row = slots.workspaces_bar("f1", self.WIDTH)[0]
        self.beta = self._column_of(self.row, " beta")
        self.component = builtins.build("f1").get("workspaces")
        self.on_event = self.component.on_event

    def test_the_press_acts_and_the_release_does_not(self):
        """`_repos_events`' reading of §4i, kept word for word. A click arrives twice and
        either half can arrive alone; the press is where the operator POINTED, so a drag
        that began elsewhere and happens to release over the bar delivers only a release
        and switches nothing."""
        self.on_event(_press(self.beta, pressed=False))
        self.assertEqual(self.spawned, [], "an unpaired release switched the frame")
        self.on_event(_press(self.beta))
        self.assertEqual(len(self.spawned), 1)

    def test_the_middle_and_right_buttons_are_left_to_the_terminal(self):
        """Middle-click is paste on every terminal an operator has ever used and
        right-click opens their emulator's own menu, so acting on either would be charter
        taking a gesture that already means something else."""
        for button in ("middle", "right"):
            self.on_event(_press(self.beta, name=button))
        self.assertEqual(self.spawned, [])

    def test_a_switch_is_reversible_by_the_same_gesture_that_made_it(self):
        """**The §4i property, asserted rather than asserted-about.** What makes a click
        an acceptable carrier here is that the tab you left is still on the bar: switching
        back is the identical gesture on the identical column, and nothing was created,
        destroyed or started along the way."""
        self.on_event(_press(self.beta))
        state.record_workspace("f1", "beta")
        back = slots.workspaces_bar("f1", self.WIDTH)[0]
        self.assertIn("*beta", back)
        self.on_event(_press(self._column_of(back, " alpha")))
        self.assertEqual(
            [argv[-1] for argv, _fid in self.spawned], ["beta", "alpha"],
            "the way back is not the same gesture on the same bar")

    def test_there_is_no_keypress_a_selection_could_have_been_confirmed_with(self):
        """**Why a bar switches where the table selects.** A select-then-choose bar needs
        a chooser, and a panel has none: `key` is in `component.EVENT_KINDS` — a provider
        may declare it — and deliberately NOT in `events.DELIVERED`, because tmux routes
        typing to the ACTIVE pane and that pane is the harness charter exists to protect.
        So a second mark on this bar would be a state nothing could ever act on."""
        self.assertIn("key", component.EVENT_KINDS)
        self.assertNotIn("key", events.DELIVERED)

    def test_the_handler_answers_falsy_even_when_it_started_a_switch(self):
        """Truthy means *repaint me* (§4f), and nothing this process can see has changed:
        the switch happens in another process and ends in a `state.bump` of its own, which
        is the version this panel's poll already watches. Answering truthy would buy one
        immediate repaint of a byte-identical row — the cost `slots._Viewport.move`
        refuses in as many words."""
        self.assertFalse(self.on_event(_press(self.beta)))
        self.assertEqual(len(self.spawned), 1, "the case measured nothing")
        self.assertFalse(self.on_event(_press(0)))

    def test_both_bars_declare_click_and_nothing_else(self):
        """`scroll` is absent deliberately: a bar is one row with nothing to scroll to, so
        a handler for it could only ever answer False. `events.Dispatcher.open` charges the
        same `overlay.MOUSE_ON` for one pointer kind as for two — which is why declaring
        the second costs nothing, and never a reason to declare one that does nothing."""
        for cid in ("chats", "workspaces"):
            c = builtins.build("f1").get(cid)
            self.assertEqual(c.events, ("click",), cid)
            self.assertEqual(events.wanted(c), ("click",), cid)
            self.assertIsNotNone(c.on_event, cid)

    def test_a_wheel_notch_never_reaches_the_handler(self):
        """The declaration is what decides, and `Dispatcher._deliver` is where it is
        applied — a kind the component did not declare is dropped before the handler is
        reached, which is why `_bar_events` carries no `ev.kind` branch of its own."""
        d = events.Dispatcher(self.component)
        for direction in ("up", "down"):
            self.assertFalse(
                d._deliver(overlay.Event(overlay.SCROLL, direction, row=0,
                                         col=self.beta)))
        self.assertEqual(self.spawned, [])
