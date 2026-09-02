"""`chat: the next tab` and its three siblings — the keyboard's half of the two strips.

**The gap these close is stated in the code they are the answer to.**
`frame/builtins._bar_events` argues at length that a tab bar switches where the repo table
only selects, and one of its three reasons is that nothing could ever finish a two-step
gesture on a bar: `key` is in `component.EVENT_KINDS` and deliberately **not** in
`events.DELIVERED`, because tmux routes typing to the ACTIVE pane and that pane is the
harness the frame exists to protect. So a bar has no keyboard of its own — and
`component.EVENT_KINDS` asks for exactly one thing in that situation: *give every pointer
affordance a key as well.* `builtin_actions._register_selection` is charter keeping that
rule for the repo table. These four rows are charter keeping it for the two strips.

**Why they are worth more than the pickers already there.** `F2` → `chat` → a name reaches
any chat at any width and is what a narrow frame has instead of a bar at all. What it is
not is *the next one*: an operator cycling between two agents pays a pane cycle, a list and
a choice to move one step along a strip they can see. And on the planes where `[frame]
mouse` is off — the shipped default — these rows are the only route to that step at all.

**What is asserted here and what is asserted elsewhere.** The walk, the wrap, the two
starts and the argv are here. That the argv reaches a real chat switch through a real tmux
is `tests/test_frame_chat_switch.py`'s and `tests/test_a_real_click_on_a_real_tab_bar
_switches.py`'s — these rows spawn the command a tab click already spawns, which is the
one property that keeps a keyboard walk and a pointer walk from teaching two answers.
"""

from __future__ import annotations

import os
import unittest
from unittest import mock

from charter import config, util
from charter.frame import action as faction
from charter.frame import builtin_actions, builtins, chats, state, switch

from tests._isolation import PersonaIso
from tests.test_frame_chat_switch import _plant

FID = "api.2"


class _AStripAKeyboardCanWalk(PersonaIso, unittest.TestCase):
    """Three chats in `api`, three workspaces on the plane, and a fake `_spawn`."""

    CHATS = ("api.1", "api.2", "api.3")

    #: Made on top of whatever `PersonaIso` scaffolds, which is why the cases below read
    #: `switch.workspaces()` rather than this tuple: a plane always has a `default`, so a
    #: case that assumed its own three would be asserting about a list charter does not
    #: return. `_plant` also makes an `api` directory for the chats.
    WORKSPACES = ("alpha", "beta", "gamma")

    def setUp(self):
        super().setUp()
        # **`$CHARTER_WORKSPACE` is deliberately EMPTY.** It pins which workspace this
        # process draws (`state.workspace_for`'s first rung), and a fixture that set one
        # would answer half of every case here for charter: `switch.current_workspace`
        # would come back from the environment rather than from the record, so the walk
        # would start from a name that is not on the strip at all — which is a real state
        # (`test_a_frame_on_no_tab_at_all_…`) and must be the case that measures it, not
        # the fixture. `tests/test_a_real_click_on_a_real_tab_bar_switches.py` empties it
        # for the same reason.
        self.enterContext(mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": ""},
                                          clear=False))
        for chat in self.CHATS:
            _plant(chat, workspace="api")
        for name in (*self.WORKSPACES, "api"):
            (config.WORKSPACES_DIR / name).mkdir(parents=True, exist_ok=True)
        # **One record answers both strips, and that is charter's shape rather than this
        # fixture's shortcut.** `chats.roster` asks "which workspace is this frame
        # drawing" and `switch.current_workspace` asks the same question — both go through
        # `state.workspace_for` — so the chat strip is `api`'s chats and the workspace
        # strip starts at `api`. A fixture that recorded a second workspace for this frame
        # to make the workspace cases read prettily would be testing a state charter
        # cannot be in.
        self.here = "api"
        self.spawned: list = []
        self.enterContext(mock.patch(
            "charter.frame.builtin_actions._spawn",
            side_effect=lambda argv, *, fid: self.spawned.append((argv, fid))))

    def _reg(self):
        return builtin_actions.build(FID, current_density="normal", current_chrome="off")

    def _run(self, aid: str, fid: str = FID):
        a = self._reg().get(aid)
        return a.run(faction.build(a.touches, fid=fid, snapshot={}))

    def _offer(self, aid: str):
        return next(o for o in self._reg().offers(fid=FID, snapshot={}) if o.id == aid)


class TheFourRowsAreThere(_AStripAKeyboardCanWalk):
    """The listing — two nouns, two directions, in the order the palette shows them."""

    def test_both_strips_offer_both_directions(self):
        rows = [(a.id, a.title) for a in self._reg().all()
                if a.id.startswith(("chat.next", "chat.previous",
                                    "workspace.next", "workspace.previous"))]
        self.assertEqual(rows, [("chat.next", "chat: the next tab"),
                                ("chat.previous", "chat: the previous tab"),
                                ("workspace.next", "workspace: the next tab"),
                                ("workspace.previous", "workspace: the previous tab")])

    def test_they_sit_beside_the_repo_tables_own_next_and_previous(self):
        """The same gesture on a different list, so an operator who has learned one has
        learned all three — and they are above the densities, which
        `_register_density` records must stay where muscle memory left them."""
        ids = [a.id for a in self._reg().all()]
        self.assertLess(ids.index("repo.previous"), ids.index("chat.next"))
        self.assertLess(ids.index("workspace.previous"), ids.index("density.minimal"))

    def test_none_of_them_keeps_the_palette_open(self):
        """**Unlike the repo rows, and the difference is what the row does to the surface
        it was pressed on.** Moving a selection writes two files and leaves the palette
        standing; a switch moves the CLIENT to another window, and the palette's pane is in
        the window being left. A row that asked to stay open would be asking to stay open
        somewhere the operator no longer is."""
        for aid in ("chat.next", "chat.previous",
                    "workspace.next", "workspace.previous"):
            with self.subTest(aid=aid):
                self.assertFalse(self._reg().get(aid).repeat)
        self.assertTrue(self._reg().get("repo.next").repeat,
                        "the control failed — nothing in this palette repeats")


class AWalkStartsTheSwitchATabClickStarts(_AStripAKeyboardCanWalk):
    """What each row spawns, and that it is the same argv the pointer half spawns."""

    def test_the_next_chat_is_the_one_drawn_next(self):
        self._run("chat.next")
        self.assertEqual(self.spawned,
                         [(util.self_relaunch_argv("frame-chat", "api.3"), FID)])

    def test_the_previous_chat_is_the_one_drawn_before(self):
        self._run("chat.previous")
        self.assertEqual(self.spawned,
                         [(util.self_relaunch_argv("frame-chat", "api.1"), FID)])

    def test_the_next_workspace_is_the_one_drawn_next(self):
        """The name after this frame's own, in `switch.workspaces()`' order — which is the
        order the `workspaces` bar draws, asked of the same function so the two cannot
        disagree. Read off that list rather than spelled, because a plane always carries a
        `default` this fixture did not make."""
        names = switch.workspaces()
        want = names[names.index(self.here) + 1]
        self._run("workspace.next")
        self.assertEqual(
            self.spawned,
            [(util.self_relaunch_argv("frame-switch", "--workspace", want), FID)])

    def test_the_previous_workspace_is_the_one_drawn_before(self):
        names = switch.workspaces()
        want = names[names.index(self.here) - 1]
        self._run("workspace.previous")
        self.assertEqual(self.spawned[0][0][-1], want)

    def test_it_starts_exactly_what_a_tab_click_starts(self):
        """**One switch, two front doors**, and the property that keeps them one: the
        keyboard row and the pointer handler spawn byte-identical argv. A row that
        assembled its own would be a second answer to how a switch is performed, and the
        two would drift the day either grew an option — which is the shape
        `slots.drawable` exists to stop for a different question.

        The pointer half is driven through the REAL registry rather than through
        `_bar_events` directly, so this also says the component is still wired.
        """
        self._run("chat.next")
        by_keyboard = self.spawned.pop()
        row = builtins.build(FID).get("chats").on_event
        from charter.frame import overlay, slots
        slots.chats_bar(FID, 200)
        col = next(c for c in range(200) if slots.TABS.switch_to(c) == "api.3")
        row(overlay.Event(overlay.CLICK, "left", row=0, col=col, pressed=True))
        self.assertEqual(by_keyboard, self.spawned.pop())

    def test_the_two_commands_are_the_ones_the_bars_handler_holds(self):
        """`_STRIPS` respells `builtins._CHAT_SWITCH` and `_WORKSPACE_SWITCH` rather than
        importing them — `frame/builtins.py` is a RENDERER module a palette process has no
        other reason to load. This is the assertion that stands in for the import, so the
        two spellings cannot drift apart in silence."""
        spellings = {s.noun: s.command for s in builtin_actions._STRIPS}
        self.assertEqual(spellings["chat"], builtins._CHAT_SWITCH)
        self.assertEqual(spellings["workspace"], builtins._WORKSPACE_SWITCH)

    def test_the_row_says_which_tab_it_started_for(self):
        """`_select`'s `selected <name>` one strip over: the palette says what it started
        rather than that it started something.

        **In the plane's own `<noun> → <name>` vocabulary**, which is what
        `commands_frame._say_on_screen` says for every other switch (`persona → zeb`). The
        noun comes off `_Strip` rather than out of the argv: `frame-switch --workspace`
        would have made the workspace row report `switch → gamma`, which names a verb
        where every other line on the plane names a noun.
        """
        self.assertEqual(self._run("chat.next"), "chat → api.3")
        self.assertEqual(self._run("chat.previous"), "chat → api.1")
        names = switch.workspaces()
        want = names[names.index(self.here) + 1]
        self.assertEqual(self._run("workspace.next"), f"workspace → {want}")

    def test_the_child_is_told_which_frame_it_is(self):
        """`_spawn`'s `fid=` is the child's own `$CHARTER_SESSION_ID`. One tmux server is
        shared by every frame on the machine, so a child left to read that out of an
        inherited environment can act on somebody else's frame."""
        self._run("chat.next")
        self.assertEqual([fid for _argv, fid in self.spawned], [FID])


class TheWalkWrapsAndStartsWhereTheDirectionCameFrom(_AStripAKeyboardCanWalk):
    """`_walk` — the arithmetic, shared with the repo table's own two rows."""

    def test_next_from_the_last_tab_wraps_to_the_first(self):
        """**Wrapping rather than stopping**, which is `_register_selection`'s decision
        unchanged: a row that visibly does nothing reads as broken and costs the operator a
        whole `F2` to find out."""
        self._run("chat.next", fid="api.3")
        self.assertEqual(self.spawned[0][0][-1], "api.1")

    def test_previous_from_the_first_tab_wraps_to_the_last(self):
        self._run("chat.previous", fid="api.1")
        self.assertEqual(self.spawned[0][0][-1], "api.3")

    def test_a_frame_on_no_tab_at_all_enters_from_the_end_the_direction_came_from(self):
        """The workspace bar draws this state for a frame whose recorded workspace has
        been deleted. `next` must land on the first name and `previous` on the last — one
        sentinel for both was the first version of the repo walk and was wrong, and this
        is that decision reaching a second list."""
        state.record_workspace(FID, "nowhere-at-all")
        names = switch.workspaces()
        self._run("workspace.next")
        self._run("workspace.previous")
        self.assertEqual([argv[-1] for argv, _fid in self.spawned],
                         [names[0], names[-1]])

    def test_the_repo_table_walks_by_the_same_function(self):
        """One walk, three callers — so the palette cannot teach two different answers to
        "what does next mean". Asserted by exercising `_walk` directly on both shapes of
        list rather than by reading the source."""
        for step, start, want in ((1, -1, "b"), (-1, 0, "c")):
            with self.subTest(step=step):
                self.assertEqual(
                    builtin_actions._walk(["a", "b", "c"], "a", step, start), want)
                self.assertEqual(
                    builtin_actions._walk(["a", "b", "c"], "gone", step, start),
                    "a" if step > 0 else "c")


class AStripWithNowhereElseToGoSaysSo(_AStripAKeyboardCanWalk):
    """A strip of one — reported by the RUN, not by `available`.

    **The palette's own cost promise is why.**
    `test_frame_palette_names.TheRosterIsNeverReadUntilSomethingIsTyped` pins that opening
    the palette reads no roster at all — *`F2` on a plane with forty workspaces costs what
    it cost with none* — and `available` is asked for every row every time the surface is
    drawn. So a row that could only answer by enumerating the plane does not answer until
    it is pressed, by an operator who asked.

    The test is "did the step land where it started", which is sharper than a count and is
    the same one `slots._Tabs.switch_to` applies to a click on the tab you are on.
    """

    def test_a_workspace_with_one_chat_says_so_and_starts_nothing(self):
        """A row that wrapped to the chat the operator is already in would spend a whole
        pane cycle arriving nowhere — which `slots._Tabs.switch_to` refuses for the
        pointer, in as many words."""
        for chat in ("api.2", "api.3"):
            _plant(chat, workspace="other")
        self.assertEqual(chats.of_workspace("api"), ["api.1"])
        state.record_workspace("api.1", "api")
        for aid in ("chat.next", "chat.previous"):
            with self.subTest(aid=aid):
                self.assertEqual(self._run(aid, fid="api.1"), chats.ONLY_CHAT)
        self.assertEqual(self.spawned, [], "a strip of one started a switch")

    def test_opening_the_palette_still_reads_no_workspace_roster(self):
        """**The promise these rows must not break**, asserted where they could break it:
        `available` runs for every row on every draw, so an availability that asked
        `switch.workspaces()` would enumerate the plane on every `F2` — including the ones
        opened to detach, where the operator asked no question about names at all.

        Asked by counting calls rather than by reading the source, so a future edit that
        moves the read into a helper is caught too.
        """
        with mock.patch.object(switch, "workspaces",
                               side_effect=switch.workspaces) as ws:
            self._reg().offers(fid=FID, snapshot={})
        self.assertEqual(ws.call_count, 0,
                         "drawing the palette enumerated this plane's workspaces")

    def _leave_one_workspace(self) -> str:
        """Every workspace directory gone, which leaves exactly one name.

        `switch.workspaces()` folds `config.DEFAULT_WORKSPACE` in whether or not its
        directory exists — a fresh plane that has never made one would otherwise offer an
        empty list — so deleting everything is what a plane with one workspace IS, and the
        one that survives is `default`. Deleting a hand-written pair by name was the first
        version and left `default` and `api` standing, so the rows stayed available and
        the case passed for the wrong reason.

        The frame is then put ON that survivor, because the row is refused for having
        nowhere ELSE to go — a frame on a workspace that no longer exists has somewhere to
        go and is a case of its own.
        """
        for d in sorted(config.WORKSPACES_DIR.iterdir()):
            for path in sorted(d.rglob("*"), reverse=True):
                path.rmdir() if path.is_dir() else path.unlink()
            d.rmdir()
        self.assertEqual(switch.workspaces(), [config.DEFAULT_WORKSPACE],
                         switch.workspaces())
        state.record_workspace(FID, config.DEFAULT_WORKSPACE)
        return config.DEFAULT_WORKSPACE

    def test_a_plane_with_one_workspace_says_so_and_starts_nothing(self):
        self._leave_one_workspace()
        for aid in ("workspace.next", "workspace.previous"):
            with self.subTest(aid=aid):
                self.assertEqual(self._run(aid), builtin_actions.ONLY_WORKSPACE)
        self.assertEqual(self.spawned, [], "a plane of one workspace started a switch")

    def test_the_refusal_names_the_way_out(self):
        """`NO_REPOS`' rule: a refusal that names no way out is a row an operator reads
        once and never again."""
        self.assertIn("charter workspace create", builtin_actions.ONLY_WORKSPACE)
        self.assertIn("charter <harness>", chats.ONLY_CHAT)

    def test_a_frame_on_a_workspace_that_is_gone_is_still_offered_the_one_that_is_not(self):
        """**Where a count gets it wrong.** `len(names) > 1` reads "is there anywhere to
        go" as "are there two places", and those differ on exactly this state: a frame
        whose recorded workspace has been deleted is on no name at all, so the one
        remaining workspace IS somewhere to send it — and a count would refuse the only
        row that could. The workspace bar draws this state; `slots._bar` has its own case
        for it."""
        self._leave_one_workspace()
        state.record_workspace(FID, "deleted-out-from-under-this-frame")
        self._run("workspace.next")
        self.assertEqual([argv[-1] for argv, _fid in self.spawned],
                         [config.DEFAULT_WORKSPACE])

    def test_a_plane_with_several_of_each_starts_all_four(self):
        """The control. Every case above is a negative, and a walk that started nothing
        ever would satisfy all of them."""
        for aid in ("chat.next", "chat.previous",
                    "workspace.next", "workspace.previous"):
            with self.subTest(aid=aid):
                self.spawned.clear()
                self.assertNotIn("one chat", str(self._run(aid)))
                self.assertEqual(len(self.spawned), 1)

    def test_every_row_is_offered_whatever_the_plane_holds(self):
        """The other side of the cost promise: these four are never hidden, because
        hiding them is what would cost the read. An operator who presses one on a strip of
        one is told; an operator who never presses one pays nothing."""
        self._leave_one_workspace()
        for chat in ("api.2", "api.3"):
            _plant(chat, workspace="other")
        offered = {o.id: o.available for o in self._reg().offers(fid=FID, snapshot={})}
        for aid in ("chat.next", "chat.previous",
                    "workspace.next", "workspace.previous"):
            self.assertTrue(offered[aid], aid)
