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


class AClickOnTheOverflowCountOpensThePalette(_ABarThatWasDrawn, unittest.TestCase):
    """*"when workspaces are more we are showing `+N` now in tabs — but user can't click
    and see other workspaces."*

    The operator pressed the `+9`. It is the strongest evidence available about what that
    field looks like it does, and what it did was nothing at all: the count is drawn for
    names that are NOT on the row, so `slots._Tabs.switch_to` correctly refused to pick
    one of them, and the refusal was the whole of the behaviour.

    **It opens the palette rather than turning a page**, and the alternative is worth
    saying out loud because it is the obvious one. `slots._page` cuts the list into
    consecutive pages that depend on the NAMES and the WIDTH and on nothing remembered —
    that is what makes a drawn tab safe to press twice, and its own docstring measures what
    a remembered window costs (a panel does not survive `cmd_respawn`, a density change
    re-splits the panes, and two frames on one plane at one width would then disagree). A
    `+N` that paged would need exactly that memory.

    What the palette gives instead is §3.6's own sentence — *the bar is a readout, never
    the mechanism* — with the hand-off happening where the readout runs out of room.

    The COLUMN half of this (which cells are a count, and that nothing else is) is
    `tests/test_frame_bars.AClickResolvesAgainstWhatWasDrawn`; this file is what happens
    once one is pressed.
    """

    #: Narrow enough that this plane's names do not all fit, so the row carries a count at
    #: each end. Chosen against `NAMES` below rather than as a round number — the case
    #: asserts there are two counts, so a width that drew none fails loudly.
    WIDTH = 80

    NAMES = [f"workspace-{i:02d}" for i in range(15)]
    HERE = "workspace-07"

    def setUp(self):
        super().setUp()
        for name in self.NAMES:
            (config.WORKSPACES_DIR / name).mkdir(parents=True, exist_ok=True)
        state.frame_dir("f1", create=True)
        state.record_workspace("f1", self.HERE)
        self.enterContext(mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": ""},
                                          clear=False))
        self.row = tui.strip_ansi(slots.workspaces_bar("f1", self.WIDTH)[0])
        self.on_event = self._handler("workspaces", "f1")
        self.counts = [f.strip() for f in self.row.split(" " * slots._BAR_GAP)
                       if f.strip().startswith("+")]
        self.assertEqual(len(self.counts), 2,
                         f"this width draws no page in the middle: {self.row!r}")

    def _column_of_count(self, count: str) -> int:
        return self._column_of(self.row, count)

    def test_a_press_on_either_count_opens_the_palette(self):
        """Both ends, and the argv spelled out. `frame-palette` with no arguments is what
        `F2` runs and what a click on a door runs (`builtins._strip_events`) — one answer
        to "how does a frame surface open the palette", shared rather than copied."""
        for count in self.counts:
            with self.subTest(count=count):
                self.spawned.clear()
                self.on_event(_press(self._column_of_count(count)))
                self.assertEqual(
                    self.spawned,
                    [(util.self_relaunch_argv("frame-palette"), "f1")])

    def test_it_opens_exactly_the_door_a_strip_click_opens(self):
        """One palette, two front doors on one frame. A count that assembled its own argv
        would be a second answer to a question `_strip_events` already answers, and the
        two would drift the day either grew an option."""
        self.on_event(_press(self._column_of_count(self.counts[0])))
        by_count = self.spawned.pop()
        slots.DOORS.forget()
        slots.render("top", "f1")
        door = next(c for c in range(self.WIDTH) if slots.DOORS.opens_palette(c))
        builtins.build("f1").get("identity").on_event(_press(door))
        self.assertEqual(by_count, self.spawned.pop())

    def test_no_switch_is_started_by_a_count(self):
        """The count names no workspace, so nothing may be switched to on the strength of
        one. A handler that fell through to `frame-switch` with an empty name would reach
        `cmd_switch`, which is the class of wrongness §4i is about."""
        for count in self.counts:
            self.on_event(_press(self._column_of_count(count)))
        self.assertTrue(all(argv[-2:-1] != ["--workspace"] or argv[-1] in self.NAMES
                            for argv, _fid in self.spawned))
        self.assertTrue(all("frame-switch" not in argv for argv, _fid in self.spawned),
                        f"a count started a switch: {self.spawned!r}")

    def test_a_press_on_a_tab_still_switches_and_opens_nothing(self):
        """The control. Both answers live in one handler now, so a case that only asserted
        the new one would pass with the old one deleted."""
        drawn = [n for n in self.NAMES if n in self.row and n != self.HERE]
        self.assertTrue(drawn, f"no switchable tab on this page: {self.row!r}")
        self.on_event(_press(self._column_of(self.row, f" {drawn[0]}")))
        self.assertEqual(
            self.spawned,
            [(util.self_relaunch_argv("frame-switch", "--workspace", drawn[0]), "f1")])

    def test_the_release_opens_nothing_either(self):
        """§4i, kept for the second gesture as well as the first: the press is where the
        operator pointed, and a drag that began on a pane border and ended over this row
        delivers only a release."""
        for count in self.counts:
            self.on_event(_press(self._column_of_count(count), pressed=False))
        self.assertEqual(self.spawned, [])

    def test_the_middle_and_right_buttons_open_nothing(self):
        """Middle-click is paste and right-click is the terminal's own menu, exactly as for
        a tab — the count did not get its own button rule."""
        for button in ("middle", "right"):
            self.on_event(_press(self._column_of_count(self.counts[0]), name=button))
        self.assertEqual(self.spawned, [])

    def test_the_handler_is_still_falsy_when_it_opened_the_palette(self):
        """Truthy means *repaint me* and nothing in this rectangle changed: the palette
        carves its pane off the HARNESS. `_strip_events` answers the same way for the same
        reason."""
        self.assertFalse(self.on_event(_press(self._column_of_count(self.counts[0]))))
        self.assertEqual(len(self.spawned), 1, "the case measured nothing")

    def test_the_chat_bar_hands_off_to_the_same_place(self):
        """One function draws both bars, so one handler answers for both — and the palette
        is the door for either noun. A count that opened a workspace picker from the chat
        bar would be the two bars degrading differently, which `slots._bar` exists to stop.
        """
        for chat in [f"api.{i}" for i in range(1, 13)]:
            _plant(chat, workspace="api")
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "api"}):
            row = tui.strip_ansi(slots.chats_bar("api.6", 44)[0])
        counts = [f.strip() for f in row.split(" " * slots._BAR_GAP)
                  if f.strip().startswith("+")]
        self.assertTrue(counts, f"this width draws no count: {row!r}")
        self.spawned.clear()
        self._handler("chats", "api.6")(_press(self._column_of(row, counts[0])))
        self.assertEqual(self.spawned,
                         [(util.self_relaunch_argv("frame-palette"), "api.6")])


class APressOnThePlusMakesAChat(_ABarThatWasDrawn, unittest.TestCase):
    """*"`+` button not working for creating new session."*

    **The affordance was a sentence and the sentence was the defect.** It read
    `+ charter <harness> opens another`, which is true, which names the command that does
    it, and which sits at the end of a row of clickable tabs beginning with a `+`. Every
    terminal an operator has used puts a `+` there and every one of them means *new*.

    `slots.ADD_CHAT` is a `+` now and this is what pressing it starts. What it starts is
    not `charter <harness>`: `builtin_actions._spawn` hands its child all three streams on
    `/dev/null`, and `cmd_launch` reads a non-tty stdout as "this process cannot be the
    operator's terminal" and `os.execvp`s the bare harness into the void. `attach=False`
    is the seam that says *build the frame, do not become the terminal*, it has existed
    since `_open_workspace` needed it, and `charter frame-new-chat` is the first spelling
    that can ask for it.

    The tmux half — that a chat really appears, in this workspace's session, with the
    client on it — is `tests/test_a_real_click_on_a_real_tab_bar_switches.py`; what this
    file can say is which press starts what.
    """

    def setUp(self):
        super().setUp()
        self.enterContext(mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "api"},
                                          clear=False))
        for chat in ("api.1", "api.2"):
            _plant(chat, workspace="api")
        self.row = tui.strip_ansi(slots.chats_bar("api.1", self.WIDTH)[0])
        self.on_event = self._handler("chats", "api.1")
        self.plus = self._column_of(self.row, slots.ADD_CHAT)

    def test_a_press_on_the_plus_starts_the_new_chat_command(self):
        """The argv spelled out. A `--chat` would be a second spelling of a value the
        child's environment already carries — `_spawn` sets `$CHARTER_SESSION_ID` to the
        *fid* below, which is what `_pressers_chat` falls back to — and that is
        `_CHAT_SWITCH`'s own recorded reason for not carrying one either."""
        self.on_event(_press(self.plus))
        self.assertEqual(self.spawned,
                         [(util.self_relaunch_argv("frame-new-chat"), "api.1")])

    def test_it_names_no_chat_and_no_workspace(self):
        """**#518's line held.** There is nothing here for an operator to type: a chat's
        id is allocated (`state.workspace_prefix`) and its workspace is the one this chat
        is in for life (§4j). An argv carrying either would be a name charter would have to
        validate, which is the whole reason the workspace bar has no `+` at all."""
        self.on_event(_press(self.plus))
        argv = self.spawned[0][0]
        self.assertEqual(argv[-1], "frame-new-chat")
        for word in ("api", "api.1", "api.2", "--workspace", "--chat"):
            self.assertNotIn(word, argv[argv.index("charter"):],
                             f"{word!r} rode along on the argv: {argv!r}")

    def test_the_press_acts_and_the_release_does_not(self):
        """§4i, and it is worth re-asking for this gesture rather than inheriting it: a
        switch is undone by the same click and making a chat is not. What carries it is
        the clause that always did — the press is the half that is never delivered
        unpaired — plus the fact that a chat made by mistake destroys nothing: the chat it
        came from keeps its harness and its conversation, and `charter frame-close` is the
        way back. The gesture §4i really forbids on a pointer is `frame-quit`, which stops
        every harness on the plane and lives behind a palette confirmation."""
        self.on_event(_press(self.plus, pressed=False))
        self.assertEqual(self.spawned, [], "an unpaired release made a chat")
        self.on_event(_press(self.plus))
        self.assertEqual(len(self.spawned), 1)

    def test_the_middle_and_right_buttons_make_nothing(self):
        for button in ("middle", "right"):
            self.on_event(_press(self.plus, name=button))
        self.assertEqual(self.spawned, [])

    def test_the_cell_beside_it_makes_nothing(self):
        """The gap belongs to neither field, exactly as it does between two tabs — and
        here it matters more, because the neighbour is a tab and picking the nearer field
        would switch instead of create."""
        for col in (self.plus - 1, self.plus - 2, self.plus + 1):
            self.on_event(_press(col))
        self.assertEqual(self.spawned, [],
                         f"a cell beside the `+` acted: {self.row!r}")

    def test_a_press_on_a_tab_still_switches(self):
        """The control: three answers live in one handler now, so a case that only
        asserted the new one would pass with the other two deleted."""
        self.on_event(_press(self._column_of(self.row, " api.2")))
        self.assertEqual(self.spawned,
                         [(util.self_relaunch_argv("frame-chat", "api.2"), "api.1")])

    def test_the_handler_is_falsy_when_it_made_a_chat(self):
        """Truthy means *repaint me* and nothing this process can see has changed: the
        chat is built by another process and ends in a `state.bump` this panel's poll is
        already watching."""
        self.assertFalse(self.on_event(_press(self.plus)))
        self.assertEqual(len(self.spawned), 1, "the case measured nothing")

    def test_the_workspace_bar_draws_no_such_thing_to_press(self):
        """**A chat is a press; a workspace is a name.** A new chat has nothing for an
        operator to type — its id is allocated and its workspace is fixed for life (§4j) —
        while a new workspace is a directory and a name #518 refuses to create on a typo.
        So the affordance is the chat bar's, and no column of a workspace bar is one."""
        for name in ("alpha", "beta"):
            (config.WORKSPACES_DIR / name).mkdir(parents=True, exist_ok=True)
        state.frame_dir("f1", create=True)
        state.record_workspace("f1", "alpha")
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": ""}):
            row = tui.strip_ansi(slots.workspaces_bar("f1", self.WIDTH)[0])
        self.assertNotIn(slots.ADD_CHAT, row)
        self.spawned.clear()
        handler = self._handler("workspaces", "f1")
        for col in range(self.WIDTH):
            handler(_press(col))
        self.assertTrue(all("frame-new-chat" not in argv for argv, _fid in self.spawned),
                        f"a workspace tab made a chat: {self.spawned!r}")

    def test_the_workspace_bars_handler_is_not_wired_for_creation_either(self):
        """**The property the case above cannot reach, and the reason the handler is handed
        the command as DATA rather than deriving it from what the renderer drew.**

        Above, no column is an affordance because `workspaces_bar` passes no note — so a
        handler that spawned on `add_at` regardless would pass, and would start making
        chats off the workspace bar on the day that renderer grew a note of its own. This
        publishes the map by hand, which is what `slots.TABS` exists to let a test do, and
        asks the handler directly.

        The chat bar is asked the same question with the same hand-made map, so this is
        not "the workspaces handler ignores everything" wearing an assertion.
        """
        slots.TABS.publish({}, "", add=[7])
        self.spawned.clear()
        self._handler("workspaces", "f1")(_press(7))
        self.assertEqual(self.spawned, [],
                         "the workspace bar's handler makes chats — it is only the "
                         "renderer that is stopping it")
        slots.TABS.publish({}, "", add=[7])
        self._handler("chats", "f1")(_press(7))
        self.assertEqual([argv[-1] for argv, _fid in self.spawned], ["frame-new-chat"],
                         "the control failed: the chat bar's handler is not wired either")
