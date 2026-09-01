"""§4j, restored: `{workspace}-{hash}` is identity, not a property — #733, #788, #791.

Spec `2026-08-25-agentic-ide-foundation.md:661` settled it and
`2026-08-28-phase5-workspace-and-chat-tabs.md:442` reaffirmed it:

> **A chat belongs to its workspace for life.** Moving a chat between workspaces sounds
> convenient and means the harness's own context — its cwd, its files, its history — is
> suddenly about a different plane. A conversation wanted elsewhere is a new chat.

**`frame/switch.py` predates that sentence by one day and never re-read it.** It was
written when a frame WAS a workspace, one to one, and `docs/frame.md` honestly said
"switching from the picker moves the frame". Phase 5 then made a frame a **chat**, and
"moves the frame" quietly became "moves the chat" — which §4j had already forbidden.

**§4b then made the switch DO something again, and every assertion here is now made
across a switch that says yes** — which is the stronger statement and the reason this
module was not simply left alone. What a workspace switch moves is the tmux CLIENT
(`commands_frame._switch_client`); the chat it leaves keeps its harness, its window, its
pid and its workspace. A `to_workspace` that answered "yes" and re-pointed one rung on the
way would put #733 and #788 straight back, and nothing about the refusal it used to be
would have caught it.

**The two directions of the one strand that produced, both closed here.**

* **#733, backward.** `F2 → workspace → gamma` inside `alpha.1` re-pointed that chat and
  left its tmux window in session `alpha`. `alpha.2` — a live window of the very session
  the operator is looking at — could never see `alpha.1` again: no palette route, and
  `charter frame-chat alpha.1` answered `no chat 'alpha.1' here`.
* **#788, forward.** The same keypress put `gamma`'s chats on `alpha.1`'s bar, where
  `chats.check` approved every one of them and `cmd_chat` then refused every one of them
  — because their windows are in tmux session `gamma` and this client is in `alpha`. A
  pre-flight that approves what the action refuses is worse than no pre-flight.

Neither is a subset of the other and both have one cause, so both are asserted against
the cause rather than against their symptoms: **nothing re-points a chat's workspace.**

**#791 is the same two symptoms through a door neither issue named**, and it is why
asserting against the cause was not enough on its own: both issues named
`switch.to_workspace`, so closing them closed one writer and left the other standing.
`charter workspace use gamma` typed at the agent calls
`workspace.set_active(..., session_id=session.current())`, and inside a frame
`session.current()` is the CHAT's id (ADR 0019) — so the write landed on
`workspace.for_session(<chat>)`, which was the middle rung of `state.own_workspace`. A
command that answers "which workspace is this session working in" was answering "which
workspace does this chat belong to" as well, and the measured outcome was #733 and #788
line for line.

**Refusing `workspace use` inside a frame was rejected on evidence, and the rejection is
why the rung went instead.** The only available test for "inside a frame" is
`session.current()`, which reads `$CHARTER_SESSION_ID` — and every agent spawned from a
frame inherits it (measured: a subagent shell three levels down still reports the frame's
id). A refusal keyed on that fires on agents doing ordinary CLI work in isolated
worktrees, none of which is in a frame in any meaningful sense. So the pointer is still
written and `workspace use` still works everywhere; what changed is that
`state.own_workspace` no longer reads it, and membership is the launch records alone.

**No tmux here, deliberately** — the strand is a state defect and reproduces on two
directories. `tests/test_frame_chat_switch.TheSwitchEstablishesTheWindowItIsMovingTo`
keeps the tmux half: a plane that arrived in the split state by some other route (a
migration, a hand-edited record) is still refused by `_pane_place`, and that guard is
not softened by this one existing.

`os.environ` is cleared in every case: `state.workspace_for` and `state.own_workspace`
both read `$CHARTER_WORKSPACE` and `$CHARTER_SESSION_ID`, so a developer running the
suite inside a live frame would otherwise be supplying half of every fixture
(#519/#521/#528).
"""

from __future__ import annotations

import os
import shutil
import unittest
from unittest import mock

from charter import config, workspace
from charter.frame import chats, choose, state, switch

from tests._isolation import PersonaIso


def _plant(fid: str, *, ws: str, pane: str = "%1") -> None:
    """Make *fid* look like a chat charter launched into *ws*.

    The production writers and never a hand-written file — `record_workspace` is what
    `frame_workspace` reads back — which is `tests/test_frame_chat_switch._plant`'s rule
    and the reason a fixture that stopped agreeing with the launcher fails here rather
    than passing against itself.
    """
    state.frame_dir(fid, create=True)
    state.record_workspace(fid, ws)
    state.record_harness_pane(fid, pane)
    state.record_identity(fid, {"CHARTER_HARNESS": "Claude Code",
                                "CHARTER_WORKSPACE": "", "CHARTER_PERSONA": ""})


class TheSwitchNoLongerMovesTheChat(PersonaIso, unittest.TestCase):
    """`switch.to_workspace` — every rung it used to write, and the one it says instead."""

    def setUp(self):
        super().setUp()
        self.enterContext(mock.patch.dict(os.environ, {}, clear=True))
        for n in ("alpha", "gamma"):
            (config.WORKSPACES_DIR / n).mkdir(parents=True, exist_ok=True)
        _plant("alpha.1", ws="alpha", pane="%1")
        _plant("alpha.2", ws="alpha", pane="%2")
        _plant("gamma.1", ws="gamma", pane="%3")

    def test_the_chat_is_still_in_the_workspace_it_was_launched_for(self):
        """The invariant itself, asked of the one rule every frame surface asks
        (`state.workspace_for`) rather than of the files the switch used to write — which
        is this repository's rule about asserting a layer below the code that prints.

        The switch SAYS YES here, and that is what makes this the strong form: the chat
        stays in `alpha` while the client is cleared to go to `gamma`."""
        out = switch.to_workspace("alpha.1", "gamma")
        self.assertTrue(out.ok, out.message)
        self.assertEqual(state.workspace_for("alpha.1"), "alpha")
        self.assertEqual(state.own_workspace("alpha.1"), "alpha")

    def test_neither_rung_the_switch_used_to_write_is_written(self):
        """Both halves, because they were two writes and either one alone re-creates the
        defect: the per-session pointer under the chat's id (`workspace.for_session`,
        `own_workspace`'s middle rung) and the launch record (`state.frame_workspace`,
        its last). A switch that wrote one of them would be #411's shape — an outcome
        reported for one noun and performed on another."""
        switch.to_workspace("alpha.1", "gamma")
        self.assertIsNone(workspace.for_session("alpha.1"))
        self.assertEqual(state.frame_workspace("alpha.1"), "alpha")

    def test_nothing_repaints_because_the_check_alone_moved_nothing(self):
        """A panel repaints because the version moved (`frame/panel.py`'s contract), and
        this function is a decision rather than an effect: the bumps a workspace switch
        earns are `_apply_arrangement`'s, after the client has actually moved and the
        panels have been re-laid-out. Bumping here would repaint every panel of this chat
        into an identical plane before anything had happened."""
        before = state.version("alpha.1")
        switch.to_workspace("alpha.1", "gamma")
        self.assertEqual(state.version("alpha.1"), before)

    def test_the_sentence_is_about_the_workspace_and_not_about_the_chat(self):
        """#517's rule — "a menu that silently fails is worse than no menu". What the
        operator is told is which workspace they are going to, because that is what
        happened; a switch that announced the CHAT moving would be describing the defect
        §4j names."""
        out = switch.to_workspace("alpha.1", "gamma")
        self.assertIn("gamma", out.message)
        self.assertNotIn("alpha.1", out.message)

    def test_a_name_that_is_not_a_workspace_is_refused_by_its_own_reason(self):
        """Each refusal is a thing the operator can act on, which is what #789's single
        unconditional sentence could not be: a typo now says it is a typo, and a name
        outside the alphabet says that instead."""
        self.assertIn("no workspace 'nope'",
                      switch.to_workspace("alpha.1", "nope").message)
        self.assertIn("cannot name a workspace",
                      switch.to_workspace("alpha.1", "../escape").message)
        self.assertIn("cannot name a workspace",
                      switch.to_workspace("alpha.1", "").message)

    def test_nothing_is_created_by_a_switch_or_by_a_refusal(self):
        """`switch.py`'s standing rule (#518: "a picker that creates on a typo leaves
        litter"), which a refusal must keep rather than inherit by accident."""
        before = sorted(p.name for p in config.WORKSPACES_DIR.iterdir())
        switch.to_workspace("alpha.1", "nope")
        switch.to_workspace("alpha.1", "gamma")
        self.assertEqual(sorted(p.name for p in config.WORKSPACES_DIR.iterdir()), before)

    def test_a_persona_switch_is_untouched(self):
        """§4j is about a chat's workspace and says nothing about who is reading it. A
        persona is not an identity the chat's cwd, files or history are about, so
        `to_persona` still moves and still bumps — stated here because the two functions
        sit side by side and a change to one is a plausible accident in the other."""
        (config.PERSONAS_DIR / "reviewer").mkdir(parents=True, exist_ok=True)
        (config.PERSONAS_DIR / "reviewer" / "persona.md").write_text("# p\n")
        out = switch.to_persona("alpha.1", "reviewer")
        self.assertTrue(out.ok, out.message)


class TheChatsItLeftBehindCanStillSeeIt(PersonaIso, unittest.TestCase):
    """#733, backward: the strand cannot form, so there is nothing to escape from."""

    def setUp(self):
        super().setUp()
        self.enterContext(mock.patch.dict(os.environ, {}, clear=True))
        for n in ("alpha", "gamma"):
            (config.WORKSPACES_DIR / n).mkdir(parents=True, exist_ok=True)
        _plant("alpha.1", ws="alpha", pane="%1")
        _plant("alpha.2", ws="alpha", pane="%2")
        _plant("gamma.1", ws="gamma", pane="%3")

    def test_the_sibling_still_holds_it_in_its_roster(self):
        """The exact reading #733 reproduces with: two chats in one tmux session, one
        `F2 → workspace` on the first, and `alpha.2`'s picker then offering one row —
        itself — while `alpha.1`'s window sits beside it in the same session."""
        switch.to_workspace("alpha.1", "gamma")
        self.assertEqual(chats.others("alpha.2"), ["alpha.1"])
        self.assertTrue(chats.check("alpha.2", "alpha.1").ok,
                        chats.check("alpha.2", "alpha.1").message)

    def test_it_is_not_offered_the_other_workspaces_chats(self):
        """#788, forward, and the property that issue asks to keep: no surface may offer
        a chat row `cmd_chat` would refuse. `gamma.1`'s window is in tmux session `gamma`
        and this client is in `alpha`, so a row for it is one `_pane_place` is guaranteed
        to reject (#684) — and the roster is where that row used to come from."""
        switch.to_workspace("alpha.1", "gamma")
        self.assertNotIn("gamma.1", chats.others("alpha.1"))
        self.assertFalse(chats.check("alpha.1", "gamma.1").ok)

    def test_both_workspaces_hold_exactly_the_chats_they_were_launched_with(self):
        """Membership from both ends in one assertion, because the defect moved a name
        from one list to the other and either list alone can be right by accident."""
        switch.to_workspace("alpha.1", "gamma")
        self.assertEqual(chats.of_workspace("alpha"), ["alpha.1", "alpha.2"])
        self.assertEqual(chats.of_workspace("gamma"), ["gamma.1"])


class TheDoorwayOpensAgainAndStillMovesNoChat(PersonaIso, unittest.TestCase):
    """Task 4's rule, and the direction §4b turns it: a refusal the palette can see BEFORE
    the keypress is drawn on the row, so the operator is not offered a list of moves that
    would not happen — and a workspace picker is no longer such a list. #789 closed this
    doorway on every frame, which was the visible half of the bar's fifteen dead rows.
    What is asserted here is that it opens and that opening it re-points nothing."""

    FID = "alpha.1"

    def setUp(self):
        super().setUp()
        self.enterContext(mock.patch.dict(os.environ, {}, clear=True))
        for n in ("alpha", "gamma"):
            (config.WORKSPACES_DIR / n).mkdir(parents=True, exist_ok=True)
        _plant(self.FID, ws="alpha")

    def test_the_workspace_doorway_carries_no_reason_at_all(self):
        """`""` is what `pin_reason` answers when the picker is worth opening, and a
        workspace picker now always is: the names behind it are names a client can be
        moved to. The one refusal a row can still earn is a live tmux reading and is
        deliberately not taken here — see `choose.pin_reason`."""
        self.assertEqual(choose.pin_reason(choose.WORKSPACE, self.FID), "")

    def test_the_row_opens_a_picker_rather_than_a_sentence(self):
        """#732: a doorway with a reason is one `commands_frame._picker` will not open,
        and `refused` is the field that says so rather than the note being parsed. Both
        halves are asserted, because either alone can be right by accident."""
        row = next(r for r in choose.open_rows(self.FID)
                   if choose.noun_of(r) == choose.WORKSPACE)
        self.assertFalse(row.refused)
        self.assertEqual(row.note, "")

    def test_pressing_a_name_still_leaves_this_chat_in_its_own_workspace(self):
        """The doorway opening is only safe because what is behind it moves a client, so
        the invariant is re-asserted at the surface that reopened — the palette's own
        dispatch (`choose.switch_to`), not `switch.to_workspace` directly."""
        out = choose.switch_to(choose.WORKSPACE, self.FID, "gamma")
        self.assertTrue(out.ok, out.message)
        self.assertEqual(state.workspace_for(self.FID), "alpha")

    def test_the_other_three_doorways_are_untouched(self):
        """One noun, not four. A chat switch moves the client and leaves every chat where
        it is; a persona switch moves who is reading; a change switch moves what one panel
        is looking at. None of them is a chat changing planes."""
        _plant("alpha.2", ws="alpha", pane="%2")
        self.assertEqual(choose.pin_reason(choose.PERSONA, self.FID), "")
        self.assertEqual(choose.pin_reason(choose.CHAT, self.FID), "")

    def test_the_names_are_listed_and_pressable(self):
        """`_name_rows`' own rule: a name the operator has already typed is a question
        they asked, so the workspaces are rows. Under #789 every one of them carried a
        refusal; under §4b none of them does, and a row that still did would be the bar's
        dead listing arriving through the palette instead. With no reason the note is the
        KIND rather than empty — `choose.labelled`'s own rule, and the field that says
        "unavailable" is `refused`."""
        self.assertIn("gamma", choose.names_of(choose.WORKSPACE, self.FID))
        rows = choose.labelled(choose.roster(choose.WORKSPACE, self.FID),
                               choose.pin_reason(choose.WORKSPACE, self.FID))
        self.assertTrue(rows)
        self.assertTrue(all(not r.refused and r.note == choose.WORKSPACE
                            for r in rows))


class TheOtherDoorMovesNoChatEither(PersonaIso, unittest.TestCase):
    """#791: `charter workspace use <name>`, typed at the agent inside a chat.

    Every case here is #733's or #788's own reading, made through the writer #789 left
    standing. The fixture is the one both issues reproduce with — two chats in `alpha`,
    one in `gamma`, no tmux — and the door is `workspace.set_active` with no `session_id`,
    which is exactly what `commands_workspace.cmd_use` calls: the id it lands on is
    whatever `session.current()` answers, and inside a frame that is the chat.

    **`$CHARTER_SESSION_ID` is set here on purpose and it is the whole fixture.** The
    class above clears the environment because the switch takes the chat id as an
    argument; this command does not, and a case that passed the id explicitly would be
    testing a call `charter workspace use` never makes.
    """

    def setUp(self):
        super().setUp()
        self.enterContext(mock.patch.dict(os.environ, {}, clear=True))
        for n in ("alpha", "gamma"):
            (config.WORKSPACES_DIR / n).mkdir(parents=True, exist_ok=True)
        _plant("alpha.1", ws="alpha", pane="%1")
        _plant("alpha.2", ws="alpha", pane="%2")
        _plant("gamma.1", ws="gamma", pane="%3")

    def _use(self, name: str) -> str:
        """`charter workspace use <name>` typed inside `alpha.1`, and nothing else.

        No `session_id=`, no `terminal_id=`: the point of the defect is which id the
        command resolves for itself, so supplying one would remove the thing under test.
        """
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "alpha.1"}):
            return workspace.set_active(name, force=True)

    def test_the_chat_is_still_in_the_workspace_it_was_launched_for(self):
        """The invariant, asked of both halves of the one ladder: what the frame DRAWS
        (`state.workspace_for`) and what it IS (`state.own_workspace`). Measured before
        the fix, both answered `gamma`."""
        self._use("gamma")
        self.assertEqual(state.own_workspace("alpha.1"), "alpha")
        self.assertEqual(state.workspace_for("alpha.1"), "alpha")

    def test_the_sibling_still_holds_it_in_its_roster(self):
        """#733, backward, through this door: `alpha.2` is a live window of the very tmux
        session the operator is looking at, and it could not see `alpha.1` at all —
        measured, `chats.others('alpha.2')` came back empty and `charter frame-chat
        alpha.1` answered `no chat 'alpha.1' here — have: alpha.2`."""
        self._use("gamma")
        self.assertEqual(chats.others("alpha.2"), ["alpha.1"])
        self.assertTrue(chats.check("alpha.2", "alpha.1").ok,
                        chats.check("alpha.2", "alpha.1").message)

    def test_it_is_not_offered_the_other_workspaces_chats(self):
        """#788, forward, through this door. Measured before the fix,
        `chats.others('alpha.1')` was `['gamma.1']` and `chats.check` approved it — a row
        whose window is in tmux session `gamma` while this client is in `alpha`, which
        `cmd_chat` then refuses (#684). A pre-flight that approves what the action
        refuses is worse than no pre-flight."""
        self._use("gamma")
        self.assertNotIn("gamma.1", chats.others("alpha.1"))
        self.assertFalse(chats.check("alpha.1", "gamma.1").ok)

    def test_both_workspaces_hold_exactly_the_chats_they_were_launched_with(self):
        """Membership from both ends, because the defect moved a name from one list to
        the other and either list alone can be right by accident. Measured before:
        `alpha` held `['alpha.2']` and `gamma` held `['alpha.1', 'gamma.1']`."""
        self._use("gamma")
        self.assertEqual(chats.of_workspace("alpha"), ["alpha.1", "alpha.2"])
        self.assertEqual(chats.of_workspace("gamma"), ["gamma.1"])

    def test_the_command_still_does_its_own_job_and_that_is_the_point(self):
        """**The fix is not "stop writing the pointer", and this is the case that says
        so.** `charter workspace use` answers "which workspace is this session working
        in": the pointer and the lock are what it exists to write, and a shell inside the
        frame that types it must still find `workspace.resolve` answering the new name —
        that is what makes every `charter clone`, `charter repos` and `charter ws current`
        in that session act on it.

        Deleting the write instead would have taken the command away from every framed
        session and from every agent that inherits `$CHARTER_SESSION_ID` from one, which
        is the refusal this fix exists to avoid. What stopped is the pointer deciding a
        chat's IDENTITY, not the pointer existing.
        """
        self.assertEqual(self._use("gamma"), "session")
        self.assertEqual(workspace.for_session("alpha.1"), "gamma")
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "alpha.1"}):
            self.assertEqual(workspace.resolve(cwd=config.ROOT), "gamma")

    def test_a_pointer_left_behind_by_a_reaped_chat_decides_nothing(self):
        """#731's first half, and the reason a reopened chat (§4e/§4f) can trust its own
        launch record.

        `state.new_chat_id` allocates the lowest free ordinal from `.charter/frame/`, so
        an id is free the moment its directory is reaped — while `state.reap` leaves
        `sessions/<fid>.workspace` behind. The next `charter claude --workspace alpha`
        that lands on `alpha.1` therefore inherits the previous frame's pointer, and while
        that pointer was a membership rung it OUTRANKED the launcher that had just been
        told `alpha` explicitly: measured, the relaunched chat drew `gamma`, joined
        `gamma`'s roster and left `alpha`'s.

        Nothing here reaps the pointer — #731's remaining half — and nothing needs to for
        identity: a stale pointer is no longer a rung of `own_workspace`, so a chat's
        workspace is what its launch recorded whatever an earlier tenant of its ordinal
        wrote.
        """
        self._use("gamma")
        shutil.rmtree(state.frame_dir("alpha.1"))
        self.assertEqual(workspace.for_session("alpha.1"), "gamma",
                         "the stale pointer this case is about was reaped after all")
        _plant("alpha.1", ws="alpha", pane="%9")    # the relaunch, same ordinal
        self.assertEqual(state.own_workspace("alpha.1"), "alpha")
        self.assertEqual(state.workspace_for("alpha.1"), "alpha")
        self.assertEqual(chats.of_workspace("alpha"), ["alpha.1", "alpha.2"])
        self.assertEqual(chats.of_workspace("gamma"), ["gamma.1"])

    def test_a_pinned_chat_is_unmoved_for_the_rung_above_rather_than_this_one(self):
        """The pin is still the first rung and still outranks everything below it, which
        is what keeps this fix from being "membership is `frame_workspace`". Stated here
        because a reader of the diff sees one rung removed and the obvious next deletion
        is the other one — `tests/test_frame_chat_switch.Membership…` measures why that
        strands a pinned chat."""
        state.record_identity("alpha.1", {"CHARTER_HARNESS": "Claude Code",
                                          "CHARTER_WORKSPACE": "gamma"})
        self.assertEqual(state.own_workspace("alpha.1"), "gamma")
        self.assertEqual(chats.of_workspace("gamma"), ["alpha.1", "gamma.1"])
