"""§4j, restored: `{workspace}-{hash}` is identity, not a property — #733 and #788.

Spec `2026-08-25-agentic-ide-foundation.md:661` settled it and
`2026-08-28-phase5-workspace-and-chat-tabs.md:442` reaffirmed it:

> **A chat belongs to its workspace for life.** Moving a chat between workspaces sounds
> convenient and means the harness's own context — its cwd, its files, its history — is
> suddenly about a different plane. A conversation wanted elsewhere is a new chat.

**`frame/switch.py` predates that sentence by one day and never re-read it.** It was
written when a frame WAS a workspace, one to one, and `docs/frame.md` honestly said
"switching from the picker moves the frame". Phase 5 then made a frame a **chat**, and
"moves the frame" quietly became "moves the chat" — which §4j had already forbidden.

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
        is this repository's rule about asserting a layer below the code that prints."""
        out = switch.to_workspace("alpha.1", "gamma")
        self.assertFalse(out.ok, out.message)
        self.assertEqual(state.workspace_for("alpha.1"), "alpha")
        self.assertEqual(state.own_workspace("alpha.1"), "alpha")

    def test_neither_rung_the_switch_used_to_write_is_written(self):
        """Both halves, because they were two writes and either one alone re-creates the
        defect: the per-session pointer under the chat's id (`workspace.for_session`,
        `own_workspace`'s middle rung) and the launch record (`state.frame_workspace`,
        its last). A refusal that wrote one of them would be #411's shape — a switch
        reported as refused and half performed."""
        switch.to_workspace("alpha.1", "gamma")
        self.assertIsNone(workspace.for_session("alpha.1"))
        self.assertEqual(state.frame_workspace("alpha.1"), "alpha")

    def test_nothing_repaints_for_a_move_that_did_not_happen(self):
        """A panel repaints because the version moved (`frame/panel.py`'s contract), so
        bumping here would make every panel of this chat re-render an identical plane —
        and would be charter agreeing on screen that something happened."""
        before = state.version("alpha.1")
        switch.to_workspace("alpha.1", "gamma")
        self.assertEqual(state.version("alpha.1"), before)

    def test_the_refusal_names_the_route_that_exists(self):
        """#517's rule — "a menu that silently fails is worse than no menu" — and the
        whole operator-facing surface of this change. Until §4b's `switch-client` lands
        there is nothing a workspace keypress CAN do, so the sentence has to carry the
        thing that works today, spelled the way the launcher's own flag is spelled
        (`--workspace`, `cli.py`; the harness launcher has no `-w`)."""
        out = switch.to_workspace("alpha.1", "gamma")
        self.assertIn("--workspace", out.message)
        self.assertIn("charter", out.message)

    def test_a_name_that_is_not_a_workspace_gets_the_same_answer(self):
        """One refusal, not a queue of them. A chat cannot move to a workspace that
        exists, so "no such workspace" is a narrower and less true reason than the one
        that applies — and answering the narrower one first would teach an operator that
        creating the workspace is what is missing."""
        for name in ("nope", "../escape", ""):
            out = switch.to_workspace("alpha.1", name)
            self.assertFalse(out.ok)
            self.assertEqual(out.message, switch.FOR_LIFE)

    def test_nothing_is_created_by_the_refusal(self):
        """`switch.py`'s standing rule (#518: "a picker that creates on a typo leaves
        litter"), which a refusal must keep rather than inherit by accident."""
        before = sorted(p.name for p in config.WORKSPACES_DIR.iterdir())
        switch.to_workspace("alpha.1", "nope")
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


class TheDoorwaySaysSoBeforeTheKeypress(PersonaIso, unittest.TestCase):
    """Task 4's rule, inherited whole: a refusal the palette can see BEFORE the keypress
    is drawn on the row, so the operator is not offered a list of moves that would not
    happen. `choose.pin_reason` is where that lives and this is one more reason arriving
    through it — the same shape a pinned frame and a workspace with no changes already
    have, rather than a new mechanism beside them."""

    FID = "alpha.1"

    def setUp(self):
        super().setUp()
        self.enterContext(mock.patch.dict(os.environ, {}, clear=True))
        for n in ("alpha", "gamma"):
            (config.WORKSPACES_DIR / n).mkdir(parents=True, exist_ok=True)
        _plant(self.FID, ws="alpha")

    def test_the_workspace_doorway_carries_the_reason_the_keypress_would_give(self):
        """One sentence, read once — `switch.to_workspace` and the row cannot describe
        one frame two ways, which is the rule the pin reason was built to keep."""
        self.assertEqual(choose.pin_reason(choose.WORKSPACE, self.FID),
                         switch.to_workspace(self.FID, "gamma").message)

    def test_the_row_is_marked_refused_so_it_opens_no_picker(self):
        """#732: a doorway with a reason is one `commands_frame._picker` will not open,
        and `refused` is the field that says so rather than the note being parsed."""
        row = next(r for r in choose.open_rows(self.FID)
                   if choose.noun_of(r) == choose.WORKSPACE)
        self.assertTrue(row.refused)
        self.assertEqual(row.note, switch.FOR_LIFE)

    def test_the_other_three_doorways_are_untouched(self):
        """One noun, not four. A chat switch moves the client and leaves every chat where
        it is; a persona switch moves who is reading; a change switch moves what one panel
        is looking at. None of them is a chat changing planes."""
        _plant("alpha.2", ws="alpha", pane="%2")
        self.assertEqual(choose.pin_reason(choose.PERSONA, self.FID), "")
        self.assertEqual(choose.pin_reason(choose.CHAT, self.FID), "")

    def test_the_names_are_still_listed_with_the_reason_beside_them(self):
        """`_name_rows`' own rule, and it is not softened here: a name the operator has
        already typed is a question they asked, so the workspaces are still rows — they
        carry the reason and refuse when pressed, rather than vanishing from the palette
        and leaving the query answered with an empty pane (#512)."""
        self.assertIn("gamma", choose.names_of(choose.WORKSPACE, self.FID))
        rows = choose.labelled(choose.roster(choose.WORKSPACE, self.FID),
                               choose.pin_reason(choose.WORKSPACE, self.FID))
        self.assertTrue(rows)
        self.assertTrue(all(r.refused and r.note == switch.FOR_LIFE for r in rows))
