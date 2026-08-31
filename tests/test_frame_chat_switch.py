"""Phase 5 Stage 5b, Task 4 and Task 6: switching between the chats of one workspace.

Two halves, and they are separated the way the code is. `frame/chats.py` decides —
which chats a workspace holds, which one you are in, and whether a name may be switched
to — and touches no tmux, so everything in :class:`TheRosterIsTheDirectory` and
:class:`TheCheckSaysWhichRefusalFired` runs against a plane and nothing else.
`commands_frame.cmd_chat` performs, and :class:`TheSwitchIsFourStepsInOneOrder` measures
the argv it sends with a fake server underneath it.

**The order the four steps go in is the only thing that makes the panels correct**, and
it is asserted as an order rather than as a set: `select-window` first, because a
background window keeps stale geometry on tmux 3.7c and on tmux 3.2 alike and tmux
resizes it AT the switch; then the teardown, then the split, so the new panels are born
in a window that has already been resized. `tests/test_frame_tmux_integration
.SwitchingBetweenChatsMovesTheClientAndThePanes` is where that is measured against a
real server with a real attached client, because it is the half a fake cannot make a
claim about.
"""

from __future__ import annotations

import contextlib
import io
import os
import unittest
from unittest import mock

from charter import commands_frame, config, contain, instance, workspace
from charter.frame import chats, choose, slots, state, switch, tmuxctl
from tests._isolation import PersonaIso
from tests._tmuxsocket import OPERATOR_SOCKET
# The one list of hostile display strings this suite has, imported rather than retyped:
# a second copy is one that stops growing when somebody finds a ninth shape.
from tests.test_frame_pickers import HOSTILE


def _plant(fid: str, *, workspace: str, pane: str = "%1",
           harness: str = "Claude Code") -> None:
    """Make *fid* look like a chat charter launched into *workspace*.

    The production writers, never a hand-written file: `record_workspace` is what
    `frame_workspace` reads back and `record_identity` is what `state.identity` does, so
    a test that stopped agreeing with either of them fails here rather than passing
    against its own fixture.
    """
    state.frame_dir(fid, create=True)
    state.record_workspace(fid, workspace)
    state.record_harness_pane(fid, pane)
    state.record_identity(fid, {"CHARTER_HARNESS": harness})


class TheRosterIsTheDirectory(PersonaIso, unittest.TestCase):
    """Spec §3.5: there is no per-workspace index of chats, so `.charter/frame/` is the
    list. What that costs and what it buys are both measured here."""

    def test_a_workspaces_chats_are_the_directories_that_name_it(self):
        _plant("api.1", workspace="api")
        _plant("api.2", workspace="api")
        _plant("web.1", workspace="web")
        self.assertEqual(chats.of_workspace("api"), ["api.1", "api.2"])
        self.assertEqual(chats.of_workspace("web"), ["web.1"])

    def test_membership_is_the_recorded_workspace_and_never_the_ids_prefix(self):
        """Spec §3.2's rename property, and the one that makes the id a NAME rather than
        a pointer. A workspace renamed under two live chats keeps both: their ids still
        spell the old name and their `workspace` file is what says where they belong."""
        _plant("old.1", workspace="old")
        _plant("old.2", workspace="old")
        for fid in ("old.1", "old.2"):
            state.record_workspace(fid, "renamed")
        self.assertEqual(chats.of_workspace("renamed"), ["old.1", "old.2"])
        self.assertEqual(chats.of_workspace("old"), [])

    def test_an_old_shape_frame_is_not_a_chat(self):
        """`{workspace}-{pid}` is Stage 5a's other id shape and it is still launchable.
        `state._launcher_pid` is the discriminator — asked once, in `chats.is_chat`, so
        a bar cannot offer to `select-window` at a frame that is a whole tmux session."""
        _plant("api-4242", workspace="api")
        _plant("api.1", workspace="api")
        self.assertFalse(chats.is_chat("api-4242"))
        self.assertTrue(chats.is_chat("api.1"))
        self.assertEqual(chats.of_workspace("api"), ["api.1"])

    def test_a_directory_name_outside_the_id_alphabet_is_not_a_chat(self):
        """A name off `os.scandir` is whatever is on disk, and it goes on to a palette
        row, a `frame-chat` argv and a tmux `-t`. `chats.ID_RE` is that alphabet, asked
        where the name enters charter's vocabulary rather than at each of the three
        places it leaves it."""
        root = state._root()
        root.mkdir(parents=True, exist_ok=True)
        for hostile in ("api.1;kill-server", "api 1", "api\t1"):
            (root / hostile).mkdir()
            state.record_workspace(hostile, "api")
        _plant("api.1", workspace="api")
        self.assertEqual(chats.of_workspace("api"), ["api.1"])

    def test_the_order_is_the_ordinal_and_not_the_directorys(self):
        """Ordinal order, so `api.1` stays leftmost on the bar where an operator learned
        to look for it. Ten sorts after two, which string order would not do."""
        for n in (10, 2, 1, 3):
            _plant(f"api.{n}", workspace="api")
        self.assertEqual(chats.of_workspace("api"),
                         ["api.1", "api.2", "api.3", "api.10"])

    def test_a_chat_whose_ordinal_cannot_be_read_sorts_last_rather_than_vanishing(self):
        """The migration and corruption case. A chat charter cannot read the ordinal of
        is still a chat whose window may be on screen, so it is ordered rather than
        dropped — dropping it would leave it out of the picker while it is running."""
        _plant("api.2", workspace="api")
        _plant("api.notanumber", workspace="api")
        self.assertEqual(chats.of_workspace("api"), ["api.2", "api.notanumber"])

    def test_an_ordinal_too_long_to_convert_sorts_last_rather_than_raising(self):
        """`int("9" * 5000)` raises `ValueError` on CPython — the int-str conversion
        limit, not an overflow — and unbounded that exception comes out of `sorted`, in a
        panel's render path.

        **Not through the directory**, which cannot hold a name that long (`NAME_MAX` is
        255 on every filesystem this runs on, so `mkdir` answers `ENAMETOOLONG` first).
        Through the fid `roster` folds in, which is `$CHARTER_SESSION_ID` — a value from
        the environment, not from a directory listing, and the one input to this sort with
        no length bound in front of it.
        """
        _plant("api.2", workspace="api")
        huge = "api." + "9" * 5000
        self.assertTrue(chats.is_chat(huge),
                        "if this stopped being a chat the sort would never see it and "
                        "this test would be measuring the wrong guard")
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "api"}, clear=False):
            self.assertEqual([c.id for c in chats.roster(huge)], ["api.2", huge])

    def test_the_sort_key_reads_the_LAST_dot_and_refuses_what_is_not_an_ordinal(self):
        """`_order`, at the four inputs that tell its clauses apart — the sweep found all
        four unpinned because every earlier test used a plain `api.<n>`.

        * `a.b.3` — `rpartition` takes the LAST dot and reads `3`; `partition` takes the
          first and reads `b.3`, which is not an ordinal at all.
        * `12345` — a name that is all digits and has NO dot. Without the `sep` clause it
          sorts as ordinal 12,345 instead of last, which puts a chat charter never minted
          in the middle of the bar.
        * `api.x` — a short non-digit tail. Without `tail.isdigit()` this reaches
          `int("x")` and the sort raises.
        * `api.99999` — exactly `_MAX_ORDINAL_DIGITS` digits, so `<=` draws it as an
          ordinal and `<` sorts it last.
        """
        self.assertEqual(chats._order("a.b.3"), (0, 3, "a.b.3"))
        self.assertEqual(chats._order("12345"), (1, 0, "12345"))
        self.assertEqual(chats._order("api.x"), (1, 0, "api.x"))
        edge = "api." + "9" * chats._MAX_ORDINAL_DIGITS
        self.assertEqual(chats._order(edge), (0, int("9" * chats._MAX_ORDINAL_DIGITS),
                                              edge))
        over = "api." + "9" * (chats._MAX_ORDINAL_DIGITS + 1)
        self.assertEqual(chats._order(over), (1, 0, over))
        # And through the list, which is where the ordering is actually read.
        for fid in ("a.b.3", "12345", "api.x", "api.2"):
            _plant(fid, workspace="w")
        self.assertEqual(chats.of_workspace("w"),
                         ["api.2", "a.b.3", "12345", "api.x"])

    def test_a_chat_with_no_pane_record_at_all_is_refused_rather_than_raising(self):
        """`state.harness_pane` answers ``None`` for a frame that has none — a chat
        launched by a charter that predates the record — and `re.fullmatch(None)` is a
        `TypeError`, not a refusal. The earlier test wrote a BAD pane string, which is a
        different input and left the fallback unpinned."""
        _plant("w.1", workspace="w")
        state.frame_dir("w.2", create=True)
        state.record_workspace("w.2", "w")
        self.assertIsNone(state.harness_pane("w.2"))
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "w"}, clear=False):
            out = chats.check("w.1", "w.2")
        self.assertFalse(out.ok)
        self.assertIn("no usable record", out.message)

    def test_a_harness_with_a_TRAILING_space_is_trimmed_too(self):
        """`.strip()`, not `.lstrip()`. The earlier test recorded three spaces, for which
        both answer `""` — so the half that matters on a real value was unpinned. A note
        is a column beside a name, and a trailing space in it moves nothing visible and
        makes two identical harnesses compare unequal."""
        _plant("w.1", workspace="w", harness="  Codex  ")
        self.assertEqual(chats.harness_of("w.1"), "Codex")

    def test_the_roster_marks_the_chat_asking_and_carries_each_ones_harness(self):
        _plant("api.1", workspace="api", harness="Claude Code")
        _plant("api.2", workspace="api", harness="Codex")
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "api"}, clear=False):
            roster = chats.roster("api.2")
        self.assertEqual([(c.id, c.harness, c.active) for c in roster],
                         [("api.1", "Claude Code", False),
                          ("api.2", "Codex", True)])

    def test_the_chat_asking_is_in_its_own_roster_even_with_no_record(self):
        """A frame whose `workspace` file could not be read is still the chat you are
        typing in. A bar that omitted the active chat would draw a list the operator is
        not in."""
        _plant("api.1", workspace="api")
        state.frame_dir("api.9", create=True)     # no `workspace` written at all
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "api"}, clear=False):
            roster = chats.roster("api.9")
        self.assertEqual([c.id for c in roster], ["api.1", "api.9"])
        self.assertTrue(roster[-1].active)

    def test_others_is_the_roster_without_you(self):
        _plant("api.1", workspace="api")
        _plant("api.2", workspace="api")
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "api"}, clear=False):
            self.assertEqual(chats.others("api.1"), ["api.2"])
            self.assertEqual(chats.others("api.2"), ["api.1"])

    def test_a_loose_file_under_the_frame_root_is_not_a_chat(self):
        """The directory is the list, and a file in it is not a directory. `exit`, a
        temp file a write was interrupted mid-`os.replace`, anything — none of them has a
        frame directory's shape and none may become a row.

        **The per-session pointer is planted deliberately, and without it this test cannot
        see its own guard** (#733). Membership used to be a file INSIDE the frame's
        directory, so a loose file was refused by the read whatever the filter did; it now
        also asks `workspace.for_session`, whose file lives under `SESSIONS_DIR` and knows
        nothing about whether the frame root holds a directory or a stray byte of a
        half-written temp file. Measured: without the `is_dir()` filter this case goes red
        on the second name and green on the first, which is the same test passing against
        the defect it exists to catch.
        """
        _plant("api.1", workspace="api")
        (state._root() / "api.2").write_text("not a chat\n")
        workspace.set_active("api", session_id="api.2", force=True, terminal_id="")
        self.assertEqual(workspace.for_session("api.2"), "api",
                         "the pointer this case turns on was not written")
        self.assertEqual(chats.of_workspace("api"), ["api.1"])

    def test_a_chat_with_no_recorded_workspace_belongs_to_none(self):
        """`frame_workspace` answers `None` for the migration case and the corrupt one,
        and `None` is not a workspace name — so the chat is in nobody's list rather than
        in everybody's."""
        _plant("api.1", workspace="api")
        state.frame_dir("api.2", create=True)
        self.assertEqual(chats.of_workspace("api"), ["api.1"])

    def test_the_roster_invents_nothing_for_a_frame_that_is_not_a_chat(self):
        """`roster` folds the asking frame in so a chat whose record is unreadable is
        still on its own bar — but only when it IS a chat. An old `{workspace}-{pid}`
        frame, or no frame at all, must not become a row that `select-window` would be
        aimed at."""
        _plant("api.1", workspace="api")
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "api"}, clear=False):
            self.assertEqual([c.id for c in chats.roster("")], ["api.1"])
            self.assertEqual([c.id for c in chats.roster("api-4242")], ["api.1"])
            # And it is not folded in twice when the scan already found it.
            self.assertEqual([c.id for c in chats.roster("api.1")], ["api.1"])

    def test_a_harness_recorded_as_whitespace_is_no_harness(self):
        """`state.identity` does not strip — it hands back what the file holds — so a
        note of three spaces would be a column of blank that reads as a harness charter
        could not name. Empty says the honest thing."""
        _plant("api.1", workspace="api")
        state.record_identity("api.1", {"CHARTER_HARNESS": "   "})
        self.assertEqual(chats.harness_of("api.1"), "")

    def test_a_frame_root_that_cannot_be_scanned_offers_no_chats_and_does_not_raise(self):
        """A bar that could not scan draws nothing; it does not take the panel down with
        it. `os.scandir` on a plane that has never launched a frame is the ordinary form
        of this, and it is the same answer."""
        self.assertEqual(chats.of_workspace("api"), [])
        with mock.patch("os.scandir", side_effect=OSError("nope")):
            self.assertEqual(chats.of_workspace("api"), [])

    def test_the_harness_comes_from_the_frames_own_record_not_this_process(self):
        """`state.identity`, never `os.environ` — a palette is a `run-shell` child of a
        server shared between every frame on the machine, so this process's own
        `$CHARTER_HARNESS` may be another chat's."""
        _plant("api.1", workspace="api", harness="Codex")
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "Something Else"},
                             clear=False):
            self.assertEqual(chats.harness_of("api.1"), "Codex")
        self.assertEqual(chats.harness_of("api.404"), "")


class MembershipIsTheChatsOwnAnswerAndNotTheAskersAnswer(PersonaIso,
                                                         unittest.TestCase):
    """#733: a chat could be DRAWING `alpha` and be excluded from `alpha`'s roster.

    `roster` keys on `state.workspace_for` — the pin, the per-session pointer, the launch
    record, a local resolve. `of_workspace` decided membership with the launch record
    ALONE. So the pointer was a rung the roster read and membership could not see, and the
    two halves of one question were asked at two depths.

    The fix is `state.own_workspace`: the same ladder with the two rungs that answer for
    the ASKING PROCESS taken off either end, asked by `of_workspace` and by
    `workspace_for` alike. One ladder asked twice — the shape `workspace.chosen` already
    names — rather than two that agree today.

    **The writer in these fixtures changed and the ladder did not.** They used to reach
    the pointer rung through `switch.to_workspace`, which wrote it and the record both;
    §4j has since made that a refusal (`tests/test_a_chat_belongs_to_its_workspace_for_
    life.py`), so what is left writing a per-session pointer under a chat's id is
    `workspace.set_active(..., session_id=<chat>)` — which is `charter workspace use`
    typed at the agent, and `commands_frame._pin_workspace` at launch. That is the
    production writer these cases now use, and every rung and every ordering below is
    unchanged.

    **What #733's other half decided, and it was not decided here.** Whether `F2 →
    workspace` moves the chat or the frame is settled: neither. A chat belongs to its
    workspace for life (spec §4j), which is why the strand cannot form that way any more.
    This class is the half that survives it — a chat may still be DRAWING a workspace its
    record does not name, because a pin or a pointer says so, and it must be in that
    workspace's roster when it is.
    """

    def setUp(self):
        super().setUp()
        # Both ladders read `$CHARTER_WORKSPACE` and `$CHARTER_SESSION_ID`, so a
        # developer running this inside a live frame would be supplying half of every
        # fixture (#519/#521/#528). The cases that need a rung state it themselves.
        self.enterContext(mock.patch.dict(os.environ, {}, clear=True))
        for n in ("alpha", "gamma"):
            (config.WORKSPACES_DIR / n).mkdir(parents=True, exist_ok=True)
        _plant("alpha.1", workspace="alpha", pane="%1")
        _plant("alpha.2", workspace="alpha", pane="%2")

    def test_a_chat_that_is_drawing_a_workspace_is_in_that_workspaces_roster(self):
        """The incoherence, stated as the invariant it broke.

        After `charter workspace use gamma` typed inside `alpha.1`, that chat draws
        `gamma` — its panels, its status line and every command it runs say `gamma` —
        while `of_workspace` read the launch record and left it in `alpha`.

        Both halves are asserted because either list alone can be right by accident: the
        chat has to arrive in the roster it is drawing AND leave the one it is not.
        """
        workspace.set_active("gamma", session_id="alpha.1", force=True, terminal_id="")
        self.assertEqual(state.workspace_for("alpha.1"), "gamma")
        self.assertIn("alpha.1", chats.of_workspace("gamma"))
        self.assertNotIn("alpha.1", chats.of_workspace("alpha"))

    def test_the_pointer_is_read_from_the_chat_on_the_other_side_too(self):
        """The asymmetry, which is the defect #733 actually had, restated on the writer
        that is left.

        The issue says the moved chat "has no route back". It had one — `charter workspace
        use alpha`, typed inside it — and what it did not have was any way to become
        visible AGAIN to the chats it left, because the repair writes the pointer and
        membership read only the record. So the property is that the pointer is read from
        BOTH sides at once: `alpha.2` sees `alpha.1` leave and sees it come back, with
        nothing typed inside `alpha.2` either time.

        Was `test_the_repair_is_visible_from_the_chat_that_was_left_behind`, whose fixture
        opened the strand with `switch.to_workspace` — §4j's refusal now.
        """
        workspace.set_active("gamma", session_id="alpha.1", force=True, terminal_id="")
        self.assertEqual(chats.others("alpha.2"), [],
                         "the pointer rung this test is about is not being read")
        workspace.set_active("alpha", session_id="alpha.1", force=True, terminal_id="")
        self.assertEqual(chats.others("alpha.2"), ["alpha.1"])
        self.assertTrue(chats.check("alpha.2", "alpha.1").ok,
                        chats.check("alpha.2", "alpha.1").message)

    def test_membership_is_not_the_asking_processes_pin(self):
        """Why membership cannot simply ask `state.workspace_for(n)`, which is the
        one-liner this defect invites and the reason `of_workspace` read the record and
        nothing else in the first place.

        Rung 0 of that ladder is `$CHARTER_WORKSPACE` in the process ASKING. It answers
        the same value for every `n`, so `workspace_for(n)` used as a membership test
        would put every chat on the plane into whichever workspace the palette process
        happened to be pinned to, and take them all out of their own.
        """
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "gamma"}, clear=False):
            self.assertEqual(state.workspace_for("alpha.1"), "gamma",
                             "the rung this test is about has moved")
            self.assertEqual(state.workspace_for("alpha.2"), "gamma")
            self.assertEqual(chats.of_workspace("gamma"), [])
            self.assertEqual(chats.of_workspace("alpha"), ["alpha.1", "alpha.2"])

    def test_membership_is_not_the_asking_processes_own_resolve(self):
        """The other end of the same ladder, and the rung the narrow version of that
        one-liner still reaches.

        Rung 3 is `workspace.resolve()`, which answers for the asking process's session,
        terminal and cwd. A chat that recorded nothing of its own — the migration case —
        would join whatever workspace the ASKER had chosen, and appear on a bar it has
        never been in.
        """
        state.frame_dir("alpha.9", create=True)     # no record, no pointer, no pin
        workspace.set_active("gamma", session_id="an-asker", force=True, terminal_id="")
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "an-asker"},
                             clear=False):
            self.assertEqual(state.workspace_for("alpha.9"), "gamma",
                             "the rung this test is about has moved")
            self.assertNotIn("alpha.9", chats.of_workspace("gamma"))

    def test_a_chat_with_nothing_recorded_owns_no_answer_and_still_falls_through(self):
        """`own_workspace` answers ``None`` where `workspace_for` answers a name, and
        that difference is the whole point of the split: "this chat says nothing" is a
        real answer for membership, and "charter still has to draw SOMETHING" is a
        different question with a different last rung."""
        state.frame_dir("alpha.9", create=True)
        self.assertIsNone(state.own_workspace("alpha.9"))
        self.assertEqual(state.workspace_for("alpha.9"), workspace.resolve())

    def test_a_pinned_chat_belongs_to_its_pin_and_not_to_what_was_typed_inside_it(self):
        """The rung a two-rung membership test would have got WRONG, measured.

        `$CHARTER_WORKSPACE` at launch is in every panel pane's process environment for
        as long as the pane lives, so `charter workspace use gamma` inside a pinned chat
        writes a pointer nothing draws — `commands_workspace` warns exactly that.
        Membership follows the pin because the screen does; a `for_session or
        frame_workspace` pair would strand a pinned chat in precisely the way #733
        describes, on a plane where it is coherent today.

        Read from `state.identity` — the launcher's own record — and never from
        `os.environ`, for `switch._pin`'s reason: this runs as a child of a tmux server
        shared between every frame on the machine.
        """
        state.record_identity("alpha.1", {"CHARTER_HARNESS": "Claude Code",
                                          "CHARTER_WORKSPACE": "alpha"})
        workspace.set_active("gamma", session_id="alpha.1", force=True, terminal_id="")
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "alpha"}, clear=False):
            self.assertEqual(state.workspace_for("alpha.1"), "alpha")
        self.assertEqual(chats.of_workspace("alpha"), ["alpha.1", "alpha.2"])
        self.assertEqual(chats.of_workspace("gamma"), [])

    def test_an_empty_recorded_pin_is_not_a_pin_here_either(self):
        """`commands_frame._frame_identity_env` emits every name, present or not, so an
        unpinned launch records `CHARTER_WORKSPACE=""`. Testing for presence rather than
        for truth would take every ordinary chat out of every roster — the same
        measurement `switch.to_workspace`'s own pin check already carries."""
        state.record_identity("alpha.1", {"CHARTER_WORKSPACE": ""})
        self.assertEqual(state.own_workspace("alpha.1"), "alpha")
        self.assertEqual(chats.of_workspace("alpha"), ["alpha.1", "alpha.2"])

    def test_a_recorded_pin_with_a_TRAILING_space_is_still_that_pin(self):
        """`.strip()`, and both halves of it — the deletion sweep found `lstrip` passing
        every other case here, exactly as it once did for `chats.harness_of`.

        Padding on the left is what a whitespace-only record has, and for that both
        answers agree on `""`. Padding on the RIGHT is what an operator who exported
        `CHARTER_WORKSPACE="alpha "` leaves, and there the two diverge: `lstrip` keeps the
        trailing space, `workspace.valid_name` refuses the name, and membership falls
        through to the pointer — while `workspace_for`'s own rung 0 strips the same value
        and reads it as a pin. That is a chat whose panels draw `alpha` sitting in
        `gamma`'s roster: #733 rebuilt out of one missing character. Stripped here on the
        same terms as that rung, because both are reading one variable.
        """
        state.record_identity("alpha.1", {"CHARTER_WORKSPACE": "alpha "})
        workspace.set_active("gamma", session_id="alpha.1", force=True, terminal_id="")
        self.assertEqual(state.own_workspace("alpha.1"), "alpha")
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "alpha "}, clear=False):
            self.assertEqual(state.workspace_for("alpha.1"), "alpha")
        self.assertEqual(chats.of_workspace("alpha"), ["alpha.1", "alpha.2"])
        self.assertEqual(chats.of_workspace("gamma"), [])

    def test_a_recorded_pin_that_cannot_name_a_workspace_is_not_a_pin(self):
        """The record is charter's own file, but the value lands in `workspace_dir()`'s
        join and in `of_workspace`'s comparison, and #442 is what an unchecked `../../`
        in that position already cost once. A pin that cannot name a workspace falls
        through to the rung below rather than becoming a workspace nothing can be a
        member of."""
        state.record_identity("alpha.1", {"CHARTER_WORKSPACE": "../../escape"})
        self.assertEqual(state.own_workspace("alpha.1"), "alpha")
        self.assertEqual(chats.of_workspace("alpha"), ["alpha.1", "alpha.2"])

    def test_the_pointer_outranks_the_record_here_exactly_as_it_does_there(self):
        """The rung ORDER is the fix, not merely the rung set. `charter workspace use`
        outranks what the launcher resolved in `workspace_for`, so it has to outrank it
        in membership too — reversed, a repaired chat would stay in the workspace the
        switch left it in while its own panels drew the other one."""
        workspace.set_active("gamma", session_id="alpha.1", force=True, terminal_id="")
        self.assertEqual(state.frame_workspace("alpha.1"), "alpha")
        self.assertEqual(state.own_workspace("alpha.1"), "gamma")
        self.assertEqual(chats.of_workspace("gamma"), ["alpha.1"])
        self.assertEqual(chats.of_workspace("alpha"), ["alpha.2"])

    def test_the_two_questions_walk_one_ladder(self):
        """`workspace_for` is `own_workspace` with the asking process's rungs on either
        end, rather than a second copy of the middle — the shape `workspace.chosen`
        already names in as many words ("one ladder, asked twice, not two ladders that
        agree today"). A rung added to one is a rung the other asks."""
        for fid in ("alpha.1", "alpha.2"):
            self.assertEqual(state.workspace_for(fid), state.own_workspace(fid))
        workspace.set_active("gamma", session_id="alpha.1", force=True, terminal_id="")
        self.assertEqual(state.workspace_for("alpha.1"),
                         state.own_workspace("alpha.1"))
        workspace.set_active("alpha", session_id="alpha.1", force=True, terminal_id="")
        self.assertEqual(state.workspace_for("alpha.1"),
                         state.own_workspace("alpha.1"))


class TheCheckSaysWhichRefusalFired(PersonaIso, unittest.TestCase):
    """Every refusal names itself, because an assertion about a bare `ok is False` cannot
    tell two guards in sequence apart — this repository's commonest defect, and the
    reason the deletion sweep asks for the reason rather than the refusal."""

    def setUp(self):
        super().setUp()
        _plant("api.1", workspace="api")
        _plant("api.2", workspace="api")
        self._env = mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "api"},
                                    clear=False)
        self._env.start()
        self.addCleanup(self._env.stop)

    def test_a_name_outside_the_alphabet_is_refused_as_a_name(self):
        out = chats.check("api.1", "api.2;kill-server")
        self.assertFalse(out.ok)
        self.assertIn("cannot name a chat", out.message)

    def test_an_absent_name_is_refused_as_a_name_rather_than_as_unknown(self):
        """Both spellings of absent, and `None` is the one that would be a `TypeError`
        out of `re.fullmatch` rather than a refusal. `charter frame-chat` cannot produce
        it — argparse gives a string — but this is the one rule two callers share, and a
        rule that raises for one of the ways of being asked nothing is not one."""
        for absent in ("", None):
            self.assertIn("cannot name a chat",
                          chats.check("api.1", absent).message, repr(absent))

    def test_an_unknown_chat_is_refused_with_the_ones_this_workspace_has(self):
        out = chats.check("api.1", "api.7")
        self.assertFalse(out.ok)
        self.assertIn("no chat 'api.7' here", out.message)
        self.assertIn("api.2", out.message)

    def test_a_chat_of_another_workspace_is_unknown_here(self):
        _plant("web.1", workspace="web")
        self.assertIn("no chat 'web.1' here", chats.check("api.1", "web.1").message)

    def test_switching_to_the_chat_you_are_in_is_refused_and_says_so(self):
        """Refused rather than performed: it would tear this chat's panels down and split
        them again for no change on screen, which is ~90 ms of blank panes nobody asked
        for."""
        out = chats.check("api.1", "api.1")
        self.assertFalse(out.ok)
        self.assertIn("already in chat 'api.1'", out.message)

    def test_a_chat_with_no_usable_pane_record_is_refused_with_the_fix(self):
        """#475's boundary: this value comes off disk and is about to be a tmux `-t`."""
        state.record_harness_pane("api.2", "%1;kill-server")
        out = chats.check("api.1", "api.2")
        self.assertFalse(out.ok)
        self.assertIn("no usable record", out.message)
        self.assertIn("relaunch that chat", out.message)

    def test_a_chat_on_the_other_tmux_server_is_refused(self):
        """#684, the half that is a record rather than a reading of tmux.

        Charter runs frames on two servers — its own `-L charter` and, inside an
        operator's tmux, theirs — and a workspace can hold a chat on each, because
        membership here is the `workspace` FILE and says nothing about where a chat runs.
        Pane ids are per-server, so `%3` recorded by a chat on one server names a real,
        live, unrelated pane on the other, and the `select-window` charter would send
        would be told it worked."""
        state.record_server("api.1", commands_frame.SOCKET)
        state.record_server("api.2", OPERATOR_SOCKET)
        out = chats.check("api.1", "api.2")
        self.assertFalse(out.ok)
        self.assertIn("not on this frame's tmux server", out.message)

    def test_a_chat_whose_server_charter_never_recorded_is_refused_too(self):
        """No default is filled in for a missing marker. Every chat this charter launches
        records one on both paths, and `of_workspace` keeps pre-chat `{workspace}-{pid}`
        frames out of the roster entirely — so an absent value is a truncated record, and
        "charter cannot tell" is the same answer as "somewhere else" for something about
        to move a client."""
        state.record_server("api.1", commands_frame.SOCKET)
        out = chats.check("api.1", "api.2")
        self.assertFalse(out.ok)
        self.assertIn("not on this frame's tmux server", out.message)

    def test_a_switch_that_may_go_ahead_names_where_it_is_going(self):
        out = chats.check("api.1", "api.2")
        self.assertTrue(out.ok)
        self.assertEqual(out.message, "chat → api.2")

    def test_every_message_is_one_line_whatever_the_name_carried(self):
        """A message is a line of charter's own output and reaches `display-message`,
        which is a tmux FORMAT. `contain.one_line` is what stops a name forging a second
        line of it (#453)."""
        for hostile in ("api\n✗ nope", "api x", "api\x1b[31mR", "api#(touch /tmp/x)"):
            msg = chats.check("api.1", hostile).message
            self.assertEqual(msg, contain.one_line(msg))
            self.assertNotIn("\n", msg)


class TheChatDoorwaySaysWhyBeforeTheKeypress(PersonaIso, unittest.TestCase):
    """`choose.pin_reason`'s rule, one noun over: an operator cannot ask about an option
    they cannot see (#512), so the doorway carries the reason and the picker is not
    opened over a list of one row that is yourself."""

    def setUp(self):
        super().setUp()
        self._env = mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "api"},
                                    clear=False)
        self._env.start()
        self.addCleanup(self._env.stop)

    def test_one_chat_means_no_picker_and_the_row_says_how_to_get_a_second(self):
        _plant("api.1", workspace="api")
        reason = choose.pin_reason(choose.CHAT, "api.1")
        self.assertEqual(reason, chats.ONLY_CHAT)
        self.assertIn("charter <harness>", reason)

    def test_two_chats_means_the_doorway_opens(self):
        _plant("api.1", workspace="api")
        _plant("api.2", workspace="api")
        self.assertEqual(choose.pin_reason(choose.CHAT, "api.1"), "")

    def test_the_chat_you_are_in_is_the_marked_row(self):
        _plant("api.1", workspace="api")
        _plant("api.2", workspace="api")
        rows = choose.roster(choose.CHAT, "api.2").rows
        self.assertEqual([r.title for r in rows if r.mark], ["api.2"])

    def test_a_chats_row_carries_its_harness_and_the_other_nouns_carry_nothing(self):
        _plant("api.1", workspace="api", harness="Claude Code")
        _plant("api.2", workspace="api", harness="Codex")
        notes = {r.title: r.note for r in choose.roster(choose.CHAT, "api.1").rows}
        self.assertEqual(notes, {"api.1": "Claude Code", "api.2": "Codex"})
        self.assertEqual(choose._note(choose.WORKSPACE, "api"), "")

    def test_only_a_chat_row_carries_a_harness_even_when_the_names_collide(self):
        """`_note`'s `if noun == CHAT`, at the one input that can tell it from an
        unconditional lookup.

        `chats.harness_of` answers `""` for a name with no frame directory, so on an
        ordinary plane dropping the branch changes nothing — which is why the sweep found
        it unpinned. A workspace and a chat CAN share a name (`chats.is_chat` accepts a
        name with no dot; `workspace.valid_name` accepts one), and then an unconditional
        lookup puts a chat's harness in a workspace row's note.
        """
        _plant("shared", workspace="api", harness="Codex")
        self.assertEqual(choose._note(choose.CHAT, "shared"), "Codex")
        self.assertEqual(choose._note(choose.WORKSPACE, "shared"), "",
                         "a workspace row borrowed a chat's harness")
        self.assertEqual(choose._note(choose.PERSONA, "shared"), "")

    def test_no_launch_pin_can_refuse_a_chat_doorway(self):
        """`$CHARTER_SESSION_ID` IS a chat's identity and is set on every frame, so an
        entry in `choose.PIN` for `chat` would refuse the doorway on every frame there
        has ever been."""
        self.assertNotIn(choose.CHAT, choose.PIN)

    def test_a_chat_row_id_can_never_be_an_action_id(self):
        from charter.frame import component
        _plant("api.1", workspace="api")
        _plant("api.2", workspace="api")
        ids = ([choose.OPEN_ID.format(choose.CHAT)]
               + [r.id for r in choose.roster(choose.CHAT, "api.1").rows])
        for rid in ids:
            self.assertFalse(component.usable_id(rid),
                             f"{rid!r} is a usable action id, so a provider shipping an "
                             f"action called `chat` could take the keypress")


class AHostileChatRendersAsOneRowAndRunsNothing(PersonaIso, unittest.TestCase):
    """Task 6, step 4, measured against the values that actually reach the surface.

    **There is no `label` file in this stage, so this measures what there IS.** Spec §3.5
    asks for one per chat; nothing here writes one (renaming a chat is not a task in this
    stage) and a reader with no writer is a line no test can turn red. What a chat is
    called today is two strings off disk with two alphabets, and both are exercised:

    * the **id**, which is a DIRECTORY NAME under `.charter/frame/` and therefore whatever
      is on disk. It is held to `chats.ID_RE` where it enters charter's vocabulary, so a
      hostile one is not a chat at all and contributes no row.
    * the **harness**, which is read out of the frame's `identity` file, has an OPEN
      alphabet (it is a harness's own display name) and goes in the row's note. It is
      contained where it is drawn — `overlay.Surface.render`, immediately before
      `tui.width` measures it (#472) — and never anywhere else.

    The hostile values are injected through `state.record_identity`, the production
    writer, so a test that stopped agreeing with it fails here rather than passing against
    its own fixture.
    """

    #: A pane narrow enough that the two-column layout has to squeeze, so the width
    #: arithmetic is exercised rather than trivially satisfied — `test_frame_pickers`'
    #: own size, for the same reason.
    SIZE = (44, 12)

    def setUp(self):
        super().setUp()
        self._env = mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "api"},
                                    clear=False)
        self._env.start()
        self.addCleanup(self._env.stop)
        for i, bad in enumerate(HOSTILE, start=1):
            _plant(f"api.{i}", workspace="api", pane=f"%{i}", harness=bad)

    def _drawn(self) -> list[str]:
        from charter.frame import palette
        roster = choose.roster(choose.CHAT, "api.1")
        surface = palette.Palette(catalogue=roster.rows, label=choose.CHAT)
        return surface.render(*self.SIZE)

    def test_every_chat_is_exactly_one_row_however_its_harness_is_spelled(self):
        roster = choose.roster(choose.CHAT, "api.1")
        self.assertEqual(len(roster.rows), len(HOSTILE))
        self.assertEqual(len({r.id for r in roster.rows}), len(HOSTILE),
                         "two chats sharing a row id is one chat that cannot be chosen")

    def test_the_drawn_pane_is_exactly_as_tall_as_the_pane(self):
        drawn = self._drawn()
        self.assertEqual(len(drawn), self.SIZE[1])
        for line in drawn:
            self.assertEqual(line, "".join(line.splitlines()), repr(line))

    def test_no_hostile_byte_reaches_the_pane(self):
        """Asserted per LINE rather than on a join of them, so the separator this test is
        about cannot be one the test itself put there."""
        for line in self._drawn():
            for bad in ("\n", "\r", " ", " ", "", "\x1b[31m"):
                self.assertNotIn(bad, line, repr(line))

    def test_the_column_arithmetic_sees_the_contained_harness_not_the_raw_one(self):
        """#472 exactly. `tui.width` — never `len` — measures what `contain.one_line` has
        already made one line of, so a note holding a separator cannot make a row wider
        than the pane."""
        from charter import tui
        state.record_identity("api.1", {"CHARTER_HARNESS":
                                        "z" * 30 + " " + "y" * 30})
        for line in self._drawn():
            self.assertLessEqual(tui.width(tui.strip_ansi(line)), self.SIZE[0],
                                 repr(line))

    def _switch_argvs(self) -> list[list[str]]:
        state.record_server("api.1", commands_frame.SOCKET)
        state.record_server("api.2", commands_frame.SOCKET)
        fake = _FakeServer()
        with mock.patch.object(tmuxctl, "run", fake), \
             mock.patch.object(commands_frame.tmuxctl, "run", fake), \
             mock.patch.object(tmuxctl, "version", lambda: (3, 7)), \
             mock.patch.object(commands_frame.tmuxctl, "version", lambda: (3, 7)), \
             mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "api.1"}, clear=False):
            commands_frame.cmd_chat(mock.Mock(chat_id="api.2", chat="api.1"))
        self.assertTrue(fake.calls, "nothing was sent, so this measures nothing")
        return fake.calls

    def test_a_harness_name_reaches_a_command_line_only_as_an_e_VALUE(self):
        """**The stronger property, stated as it is actually true — and the spec's
        version of it is not.**

        Task 6 step 4 asks that a display string "never reaches a tmux argv or a tmux
        format string at all". The second half is the one that matters and it holds
        absolutely (see the test below). The FIRST half cannot: `_relayout_pane_env`
        replays what the LAUNCH put on `-e` onto every pane a re-layout splits, and
        `CHARTER_HARNESS` has been in `layout.CARRIABLE` since #411 — so a chat's harness
        name is on the `split-window` command line of every panel that chat ever gets,
        with or without this switch.

        That is not the same risk and this test is what says so. `-e NAME=VALUE` is one
        argv ELEMENT, handed to `execve` and never parsed by tmux, so a newline or an
        escape sequence in it is an odd environment variable and nothing more. What
        §7.3's measurement is about is a display string reaching a place tmux EVALUATES —
        and the value half of a `-e` is not one.

        So the property pinned here is the exact one: a hostile harness name appears in a
        tmux argv only as `-e CHARTER_HARNESS=<it>`, and never as a command word, a
        target, or an option value of any other kind.
        """
        for argv in self._switch_argvs():
            for i, word in enumerate(argv):
                for bad in HOSTILE:
                    if bad not in word:
                        continue
                    self.assertEqual(word, f"CHARTER_HARNESS={bad}",
                                     f"{bad!r} reached a tmux command line as {word!r}")
                    self.assertEqual(argv[i - 1], "-e",
                                     f"{word!r} was not the value half of a `-e`")

    def test_no_display_string_reaches_a_tmux_FORMAT(self):
        """The half that holds absolutely, and the one §7.3 measured the cost of.

        `rename-window 'chat#{pane_pid}X'` stores the name **already expanded** —
        `chat4327X` on 3.7c, `chat49359X` on 3.2 — and `#{E:@opt}` expands a `#{...}`
        sitting in a user option's value. So a display string inside a `#{…}` would be a
        format injection on the first hop. Every `#{…}` this switch sends is a literal
        charter wrote itself.
        """
        for argv in self._switch_argvs():
            for word in argv:
                if "#{" not in word:
                    continue
                for bad in HOSTILE:
                    self.assertNotIn(bad, word,
                                     f"{bad!r} is inside the tmux format {word!r}")
                self.assertIn(word, ("#{pane_id}",
                                     "#{window_width}:#{window_height}",
                                     "#{session_id}\t#{window_id}",
                                     "#{window_id}",
                                     # #714's reconciliation asks the window which panes
                                     # it holds and which components they draw. Named by
                                     # the PRODUCTION constant rather than re-spelled, so
                                     # this allow-list cannot be widened by a rename it
                                     # never saw — the same reason `test_frame_launcher`'s
                                     # fake matches the two pane options on theirs.
                                     commands_frame._PANEL_LIST_FORMAT),
                              f"an unexpected tmux format reached the switch: {word!r}")

    def test_every_chat_is_still_reachable_at_200_80_and_40_columns(self):
        """**Stage 5b's first exit criterion**: every chat reachable in ≤2 keystrokes at
        200, 80 and 40 columns — *including* the widths where no bar can be drawn.

        Two keystrokes is `F2` and then either an arrow or a filter; what has to be true
        at every width is that the row is in the surface and still maps back to its own
        chat. Asserted through `Roster.name_of`, which is what
        `commands_frame._chosen_name` calls — matching on the drawn title instead would be
        matching on a string `Surface.render` has already contained, and would pass while
        the switch went to the wrong chat or to none.

        The chat bar draws nothing at all at 40 columns for this many chats, which is the
        point: the palette is a full pane (§4k) and does not care how wide the frame is.
        """
        from charter.frame import overlay, palette
        roster = choose.roster(choose.CHAT, "api.1")
        self.assertEqual(len(roster.rows), len(HOSTILE))
        for cols in (200, 80, 40):
            with self.subTest(cols=cols):
                surface = palette.Palette(catalogue=roster.rows, label=choose.CHAT)
                drawn = surface.render(cols, 20)
                self.assertEqual(len(drawn), 20)
                for row in roster.rows:
                    self.assertIsNotNone(roster.name_of(row))
                # And typing narrows to exactly one of them, which is the second keystroke.
                surface.handle(overlay.Event(overlay.KEY, "2"), 20)
                self.assertEqual([r.id for r in surface.rows], ["chat:n1"],
                                 f"typing `2` at {cols} columns did not reach api.2")
                self.assertEqual(roster.name_of(surface.rows[0]), "api.2")
        # And the width at which the BAR gives up its names is not a width at which the
        # picker gives up anything — which is the whole of "the bar is a readout and the
        # palette is the mechanism".
        from charter import tui
        narrow = slots.chats_bar("api.1", 40)
        self.assertTrue(all("api.8" not in ln for ln in narrow),
                        f"the bar still listed every chat at 40 columns: {narrow!r}")
        for ln in narrow:
            self.assertLessEqual(tui.width(ln), 40)

    def test_a_chat_id_outside_the_alphabet_is_refused_before_it_becomes_an_argv(self):
        """"Runs nothing" is the other half, and it is asked of the ID rather than of the
        note: the id is the only one of the two that a switch ever puts on a command
        line."""
        for bad in HOSTILE:
            out = chats.check("api.1", bad)
            self.assertFalse(out.ok, f"{bad!r} was accepted as a chat")
            self.assertIn("cannot name a chat", out.message)
            self.assertEqual(out.message, "".join(out.message.splitlines()),
                             repr(out.message))


class _FakeServer:
    """A tmux that records what it was asked and answers what a test told it to.

    Everything `cmd_chat` sends goes through `tmuxctl.run`, so one patch is the whole
    server. `_relayout` and `_split_panels` go through it too, which is what makes the
    ORDER of the four steps assertable in one list.

    **It carries a placement, and since #684 that is not decoration.** A pane belongs to a
    window and a window to a session, `select-window` sets the SESSION's current window,
    and a fake that answered one format for every `display-message` could not tell the
    switch that worked from the one that moved somebody else's session — which is the
    whole defect. So: :attr:`place` says where each pane is, :attr:`current` says which
    window each session is on, and `select-window` moves the target pane's session rather
    than a boolean. A test that wants the broken shape states it by putting the target in
    another session, the way the real server put it there.
    """

    def __init__(self, *, select_rc: int = 0):
        self.calls: list[list[str]] = []
        self.select_rc = select_rc
        #: ``{pane id: (session id, window id)}`` — what `#{session_id}:#{window_id}`
        #: reports for a pane. A pane that is not here is one tmux cannot resolve, and
        #: tmux answers that with rc 0 and empty output (measured on 3.7c), which is why
        #: this fake answers it the same way rather than with a non-zero status.
        self.place = {"%1": ("$0", "@0"), "%2": ("$0", "@1")}
        #: ``{session id: window id}`` — the window each session is currently on.
        self.current = {"$0": "@0"}

    def __call__(self, what, argv, **kw):
        import subprocess
        self.calls.append(list(argv))
        rc, out = 0, ""
        verb = self._verb(argv)
        if verb == "select-window":
            rc = self.select_rc
            if rc == 0:
                seat = self.place.get(self._target(argv))
                if seat is not None:
                    self.current[seat[0]] = seat[1]
        elif verb == "split-window":
            out = "%9"
        elif verb == "display-message":
            out = self._display(argv)
        return subprocess.CompletedProcess(argv, rc, out, "")

    def _display(self, argv) -> str:
        """What tmux would print for the format charter actually sent.

        Told apart by the FORMAT, never by the order the calls arrive in: the switch asks
        three different questions of `display-message` and a fake that answered them all
        with one string would let any of them stand in for the others.
        """
        fmt, target = argv[-1], self._target(argv)
        if fmt == commands_frame._PANE_PLACE_FORMAT:
            seat = self.place.get(target)
            return f"{seat[0]}\t{seat[1]}" if seat else ""
        if fmt == commands_frame._WINDOW_ID_FORMAT:
            return self.current.get(target, "")
        if fmt == commands_frame._PANE_WIDTH_FORMAT:
            return "80"
        return "80:24"

    @staticmethod
    def _target(argv) -> str:
        """The `-t` argument, or ``""`` — what tmux would be aiming at."""
        return argv[argv.index("-t") + 1] if "-t" in argv else ""

    @staticmethod
    def _verb(argv) -> str:
        """The tmux command word — the first element after the socket flags.

        Spelled by scanning rather than by index, because `tmuxctl.server_argv` puts a
        different number of elements in front of it for `-L` and `-S`.
        """
        for i, word in enumerate(argv):
            if word in ("-L", "-S", "-f"):
                continue
            if i and argv[i - 1] in ("-L", "-S", "-f"):
                continue
            if word.startswith("-") or word.endswith("tmux"):
                continue
            return word
        return ""

    def verbs(self) -> list[str]:
        return [self._verb(a) for a in self.calls]


class TheSwitchIsFourStepsInOneOrder(PersonaIso, unittest.TestCase):
    """Task 4, against a fake server: what `cmd_chat` sends, and in which order.

    The ORDER is the requirement. `select-window` has to come first because tmux resizes
    the target window at the switch and never before it (measured on 3.7c and on 3.2), so
    a split issued earlier is a pane born at a width that is not its window's — the
    defect `panel._component_text`'s `width=slots._width()` guard exists for. And the
    teardown has to come after the select for the same measurement read the other way:
    the window being left is the one that is now stale.
    """

    def setUp(self):
        super().setUp()
        _plant("api.1", workspace="api", pane="%1")
        _plant("api.2", workspace="api", pane="%2")
        state.record_server("api.1", commands_frame.SOCKET)
        state.record_server("api.2", commands_frame.SOCKET)
        state.record_panes("api.1", panels={"top": "%3", "bottom": "%4"})
        self.fake = _FakeServer()
        self._patches = [
            mock.patch.object(tmuxctl, "run", self.fake),
            mock.patch.object(commands_frame.tmuxctl, "run", self.fake),
            mock.patch.object(tmuxctl, "version", lambda: (3, 7)),
            mock.patch.object(commands_frame.tmuxctl, "version", lambda: (3, 7)),
            mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "api.1",
                                         "CHARTER_WORKSPACE": "api"}, clear=False),
        ]
        for p in self._patches:
            p.start()
            self.addCleanup(p.stop)

    def _switch(self, target="api.2"):
        return commands_frame.cmd_chat(
            mock.Mock(chat_id=target, chat="api.1"))

    def test_the_client_is_moved_before_anything_is_torn_down_or_split(self):
        self._switch()
        verbs = self.fake.verbs()
        self.assertIn("select-window", verbs)
        first = verbs.index("select-window")
        for later in ("kill-pane", "split-window"):
            if later in verbs:
                self.assertLess(first, verbs.index(later),
                                f"{later} was issued before the window had been "
                                f"selected, so it acted on stale geometry")

    def test_the_window_is_named_by_the_targets_own_harness_pane(self):
        self._switch()
        sel = next(a for a in self.fake.calls
                   if _FakeServer._verb(a) == "select-window")
        self.assertEqual(sel[-2:], ["-t", "%2"])

    def test_the_chat_being_left_loses_its_panels(self):
        self._switch()
        killed = [a[-1] for a in self.fake.calls
                  if _FakeServer._verb(a) == "kill-pane"]
        self.assertEqual(sorted(killed), ["%3", "%4"])
        self.assertEqual(state.panes("api.1"), {})

    def test_the_chat_being_entered_gets_panels_and_a_bump(self):
        before = state.version("api.2")
        self._switch()
        self.assertIn("split-window", self.fake.verbs())
        self.assertTrue(state.panes("api.2"))
        self.assertNotEqual(state.version("api.2"), before)

    def test_a_target_that_kept_its_panels_still_has_its_rules_re_asserted(self):
        """#686. Every switch test in this file — and in
        `tests/test_frame_tmux_integration.py` — plants an EMPTY pane map for the target,
        so all of them exercised the `missing != []` branch and none of them looked at a
        border option. This plants the other branch.

        It is one `charter claude` away: the second chat's window is created with
        `new-window -d` and selected, so chat 1 keeps its panels alive **and recorded**,
        and the operator's first `F2` back into chat 1 finds nothing missing. Before this
        fix that switch wrote zero window and zero pane options — geometry and hooks
        re-asserted, not one of `_CHROME`'s five, not #657's two, not `remain-on-exit`.
        """
        want = commands_frame._drawable_slots(
            80, 24, commands_frame._visible_now("api.2", config.FRAME))
        self.assertTrue(want, "the fixture has nothing to keep, so nothing is measured")
        state.record_panes("api.2",
                           panels={s: f"%{20 + i}" for i, s in enumerate(want)})
        self._switch()
        self.assertNotIn("split-window", self.fake.verbs(),
                         "the target was missing a slot after all, so this is the branch "
                         "that already worked")
        options = [a[-2] for a in self.fake.calls
                   if _FakeServer._verb(a) == "set-option" and "-u" not in a]
        for name in commands_frame._chrome_values():
            self.assertIn(name, options,
                          f"the window's {name} was never asserted on the chat being "
                          f"switched into")
        harness = [a for a in self.fake.calls
                   if _FakeServer._verb(a) == "set-option" and "-p" in a
                   and a[a.index("-t") + 1] == "%2"]
        self.assertTrue(harness, "#657's rules around the harness were not re-asserted")

    def test_a_window_that_is_gone_costs_a_sentence_and_no_panels(self):
        """The one refusal `chats.check` deliberately does not guess at. Nothing is torn
        down: the teardown is below the select for exactly this case."""
        self.fake.select_rc = 1
        said = []
        with mock.patch.object(commands_frame, "_say_on_screen",
                               lambda fid, msg, *a, **k: said.append(msg)):
            self._switch()
        self.assertEqual(len(said), 1)
        self.assertIn("has no window any more", said[0])
        self.assertNotIn("kill-pane", self.fake.verbs())
        self.assertEqual(state.panes("api.1"), {"top": "%3", "bottom": "%4"})

    def test_outside_a_frame_it_touches_no_tmux_at_all(self):
        """**Found by hand against the real server, not by a test.** `_say_on_screen`'s
        `-t <fid>` with an empty target resolves to whichever session on the SHARED
        `-L charter` server was attached most recently — so `charter frame-chat api.2`
        typed in an ordinary shell drew charter's refusal across another operator's frame.

        This refusal issues a real tmux command aimed at a session it has no business
        naming, which is what makes the guard worth having at all.

        **The refusal is now said on the asker's own stderr and the status is non-zero**
        (#734): rc 0 and zero bytes was indistinguishable from the switch working, and
        `docs/frame.md` makes this command the documented fallback when the palette's
        doorway is refused. What must not change, and is what this test is for, is that
        nothing goes near tmux — stderr is the one surface that is certainly the asker's.
        """
        said = []
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": ""}, clear=False), \
             mock.patch.object(commands_frame, "_say_on_screen",
                               lambda fid, msg, *a, **k: said.append(msg)), \
             contextlib.redirect_stderr(io.StringIO()) as stderr:
            self.assertNotEqual(
                commands_frame.cmd_chat(mock.Mock(chat_id="api.2", chat="")), 0)
        self.assertEqual(self.fake.calls, [])
        self.assertEqual(said, [], "a refusal was aimed at a frame this is not in")
        self.assertIn("charter frame-chat", stderr.getvalue())

    def test_an_absent_or_padded_chat_id_is_refused_rather_than_raising(self):
        """`(… or "").strip()`, both halves. `None` reaches `str.strip` as an
        `AttributeError` without the fallback, and a padded id passes `ID_RE` only once
        the trailing space is gone — `lstrip` leaves it and the name is refused as
        unknown, which is a different sentence for a name that is really fine."""
        said = []
        with mock.patch.object(commands_frame, "_say_on_screen",
                               lambda fid, msg, *a, **k: said.append(msg)):
            commands_frame.cmd_chat(mock.Mock(chat_id=None, chat="api.1"))
            self.assertIn("cannot name a chat", said[-1])
            self.assertEqual(self.fake.calls, [])
            commands_frame.cmd_chat(mock.Mock(chat_id="  api.2  ", chat="api.1"))
        self.assertIn("select-window", self.fake.verbs(),
                      "a padded id was not trimmed back to a real chat")

    def test_a_pane_record_that_vanishes_between_the_check_and_the_send(self):
        """**The one survivor CI's first sweep left, and it was a real hazard rather than
        an equivalent.** `cmd_chat` reads the target's pane a second time — the check
        answered about the record a moment ago — and with an `or ""` fallback a record
        that had gone in between became `select-window -t ""`. An empty tmux target is
        not nothing: it resolves to the CURRENT window, so charter would report a switch
        that did not happen and then tear this chat's panels down around it.

        Driven by making the SECOND read answer differently from the first, which is the
        only shape that can tell a re-read from a carried value.
        """
        real = state.harness_pane
        seen = []

        def racing(fid):
            seen.append(fid)
            if fid == "api.2" and seen.count("api.2") > 1:
                return None
            return real(fid)

        said = []
        with mock.patch.object(state, "harness_pane", racing), \
             mock.patch.object(commands_frame.state, "harness_pane", racing), \
             mock.patch.object(chats.state, "harness_pane", racing), \
             mock.patch.object(commands_frame, "_say_on_screen",
                               lambda fid, msg, *a, **k: said.append(msg)):
            self.assertEqual(self._switch(), 0)
        self.assertEqual(self.fake.calls, [],
                         "an empty `-t` target reached tmux")
        self.assertEqual(len(said), 1)
        self.assertIn("stopped being one while charter was switching", said[0])

    def test_a_refused_name_never_reaches_tmux_at_all(self):
        said = []
        with mock.patch.object(commands_frame, "_say_on_screen",
                               lambda fid, msg, *a, **k: said.append(msg)):
            self._switch("api.9")
        self.assertEqual(self.fake.calls, [])
        self.assertIn("no chat 'api.9' here", said[0])

    def test_the_switch_lays_out_the_same_with_the_window_resized_hook_removed(self):
        """Task 4, step 5, and the measurement behind it: on tmux 3.2 —
        `tmuxctl.FLOOR` — `set-hook -w window-resized` answers `invalid option`, rc=1.
        The hook does not exist there.

        **Charter does still install it above `tmuxctl.RESIZE_HOOK_FLOOR`, and that is
        not what this is about.** `_relayout` arms it for the window's own later resizes,
        which is a different event from the switch. What the switch may not do is *rely*
        on it to correct geometry it did not assert itself — so the property is that the
        panels come out identical with the hook and without it, which is what a 3.2
        operator gets.
        """
        def panes_at(version):
            state.record_panes("api.2", panels={})
            with mock.patch.object(tmuxctl, "version", lambda: version), \
                 mock.patch.object(commands_frame.tmuxctl, "version",
                                   lambda: version):
                self.fake.calls.clear()
                self._switch()
            hooked = any("window-resized" in " ".join(a) for a in self.fake.calls)
            splits = [a for a in self.fake.calls
                      if _FakeServer._verb(a) == "split-window"]
            return sorted(state.panes("api.2")), len(splits), hooked

        at_37 = panes_at((3, 7))
        # Put the panels back where the 3.7 pass found them, so the second pass starts
        # from the same frame rather than from the one the first left behind.
        state.record_panes("api.1", panels={"top": "%3", "bottom": "%4"})
        at_32 = panes_at((3, 2))
        self.assertEqual(at_37[:2], at_32[:2],
                         "the switch laid the target out differently without the hook")
        self.assertTrue(at_37[2], "3.7 should still arm the hook for later resizes")
        self.assertFalse(at_32[2],
                         "the hook does not exist at the floor and must not be sent")

    def test_the_target_gets_its_own_visible_arrangement_and_not_this_chats(self):
        """`_visible_now(target, …)` — the chat being entered draws what IT was told to
        hide, not what the chat being left was."""
        frame = config.FRAME
        arrangement = instance.frame_arrangement(frame)
        state.record_hidden("api.2", [n for n in arrangement if n != "top"])
        self._switch()
        self.assertEqual(sorted(state.panes("api.2")), ["top"])

    def test_a_chat_whose_own_pane_record_is_broken_switches_nothing(self):
        """**This test's property changed with #684, and the change is the finding.**

        It used to say the client moves even when the chat being LEFT cannot be tidied,
        on the argument that leaving the operator where they asked not to be is the worse
        of two failures. That argument held while the only cost of not knowing where you
        are standing was an untidy background window. It does not hold now: the record
        that says where to tear down (`state.harness_pane(fid)`) is the same record that
        says which tmux SESSION this client is in, and without it charter can neither
        check that the target is a window this client can be moved to nor tell afterwards
        whether it moved. Selecting anyway would aim `select-window` at a pane that may
        belong to another session — which returns 0 and moves that session's screen.

        So one broken record now refuses the whole switch, with a sentence, and touches
        no tmux beyond the two readings that failed."""
        state.record_harness_pane("api.1", "not-a-pane")
        said = []
        with mock.patch.object(commands_frame, "_say_on_screen",
                               lambda fid, msg, *a, **k: said.append(msg)):
            self._switch()
        self.assertEqual(len(said), 1)
        self.assertIn("cannot find this chat's own window", said[0])
        self.assertNotIn("select-window", self.fake.verbs())
        self.assertEqual(state.panes("api.1"), {"top": "%3", "bottom": "%4"})

    def test_a_tmux_whose_version_charter_cannot_read_moves_the_client_and_no_panes(self):
        """`_relayout_target` refuses on an unreadable version because every builder
        below it takes one. The switch itself needs none — `select-window` has been in
        tmux forever, and the placement readings around it are `display-message` — so the
        client still moves and nothing is split blind.

        This is what keeps "the two re-layouts are attempted independently, and the
        client's move is not conditional on either" pinned: both re-layouts are refused
        here and the switch still happens."""
        with mock.patch.object(tmuxctl, "version", lambda: None), \
             mock.patch.object(commands_frame.tmuxctl, "version", lambda: None):
            self._switch()
        verbs = self.fake.verbs()
        self.assertIn("select-window", verbs)
        self.assertEqual([v for v in verbs if v not in ("display-message",
                                                        "select-window")], [],
                         "something was split or killed on a tmux charter could not "
                         "read the version of")
        self.assertEqual(state.panes("api.1"), {"top": "%3", "bottom": "%4"})

    def test_the_presser_chat_is_the_one_left_not_the_session_variable(self):
        """One bind text is shared by every frame on the socket, so `$CHARTER_SESSION_ID`
        cannot tell two chats of one workspace apart — `--chat` can, and it is what
        decides whose panels are torn down."""
        state.record_panes("api.2", panels={"top": "%7"})
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "api.2"}, clear=False):
            commands_frame.cmd_chat(mock.Mock(chat_id="api.1", chat="api.2"))
        killed = [a[-1] for a in self.fake.calls
                  if _FakeServer._verb(a) == "kill-pane"]
        self.assertEqual(killed, ["%7"])


class TheSwitchEstablishesTheWindowItIsMovingTo(PersonaIso, unittest.TestCase):
    """#684: `select-window` at another session's pane returns **0**.

    Measured on real tmux 3.7c, own socket: two sessions A and B, the client in A,
    `select-window -t %N` where `%N` is a window of B — rc **0**, B's current window
    changed, A's did not, the client did not move. So the status check that follows it
    could never have told a switch from a no-op, and what came next was
    `_apply_arrangement(fid, want=[])`: charter tearing down the panels of the chat still
    on screen, having reported a switch that did not happen.

    Reachable because membership is a RECORD. `chats.of_workspace` matches
    `state.own_workspace`, whose pointer rung `charter workspace use <other>` typed at the
    agent writes ("it moves the panels too" is a documented promise) — so after one
    `charter workspace use beta` inside chat `api.1`, `of_workspace("beta")` returns
    `api.1` (a window of tmux session `api`) beside `beta.1`, in one roster, and
    `chats.check` says ok. §4j closed the other route into this state — `F2 → workspace`
    is a refusal now (#733, #788) — and this guard is not softened by that: a plane can
    still arrive here through the pointer, through a migration, or through a record
    charter did not write.

    The fixture is that plane: both chats say workspace `beta`, both are on charter's own
    server, and their windows are in two different tmux sessions — which is the one fact
    only tmux holds.
    """

    def setUp(self):
        super().setUp()
        _plant("api.1", workspace="beta", pane="%1")
        _plant("beta.1", workspace="beta", pane="%2")
        for fid in ("api.1", "beta.1"):
            state.record_server(fid, commands_frame.SOCKET)
        state.record_panes("api.1", panels={"top": "%3", "bottom": "%4"})
        self.fake = _FakeServer()
        # `api.1`'s window is in session `$0`; `beta.1`'s is in session `$1` — which is
        # what `cmd_launch` builds, one tmux session per workspace, and what no record on
        # disk can say.
        self.fake.place = {"%1": ("$0", "@0"), "%2": ("$1", "@7")}
        self.fake.current = {"$0": "@0", "$1": "@7"}
        for p in (mock.patch.object(tmuxctl, "run", self.fake),
                  mock.patch.object(commands_frame.tmuxctl, "run", self.fake),
                  mock.patch.object(tmuxctl, "version", lambda: (3, 7)),
                  mock.patch.object(commands_frame.tmuxctl, "version", lambda: (3, 7)),
                  mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "api.1",
                                               "CHARTER_WORKSPACE": "beta"},
                                  clear=False)):
            p.start()
            self.addCleanup(p.stop)
        # Every refusal in `cmd_chat` goes to the screen and nowhere else, so this is
        # where a test reads one.
        self.said: list[str] = []
        said = mock.patch.object(commands_frame, "_say_on_screen",
                                 lambda fid, msg, *a, **k: self.said.append(msg))
        said.start()
        self.addCleanup(said.stop)

    def test_the_roster_really_does_offer_the_other_sessions_chat(self):
        """The premise, asserted rather than assumed: without this the tests below would
        be measuring a switch nothing would ever have attempted."""
        self.assertEqual(chats.of_workspace("beta"), ["api.1", "beta.1"])
        self.assertTrue(chats.check("api.1", "beta.1").ok,
                        "the reading half already refuses this, so the tmux half is not "
                        "what this class is about any more")

    def test_a_cross_session_switch_selects_nothing_and_says_why(self):
        commands_frame.cmd_chat(mock.Mock(chat_id="beta.1", chat="api.1"))
        self.assertNotIn("select-window", self.fake.verbs(),
                         "charter aimed a select-window at another session's window, "
                         "which returns 0 and moves that session")
        self.assertEqual(len(self.said), 1)
        self.assertIn("another tmux session", self.said[0])

    def test_it_does_not_tear_down_the_panels_of_the_chat_on_screen(self):
        """The cost the refusal exists to stop. `want=[]` on `api.1` is what used to
        follow the rc-0 reading."""
        commands_frame.cmd_chat(mock.Mock(chat_id="beta.1", chat="api.1"))
        self.assertNotIn("kill-pane", self.fake.verbs())
        self.assertEqual(state.panes("api.1"), {"top": "%3", "bottom": "%4"})

    def test_the_other_sessions_current_window_is_left_where_it_was(self):
        """Not only "this client did not move" — the OTHER session's screen is also
        untouched, which is what a `select-window` sent anyway would have changed."""
        commands_frame.cmd_chat(mock.Mock(chat_id="beta.1", chat="api.1"))
        self.assertEqual(self.fake.current, {"$0": "@0", "$1": "@7"})

    def test_the_same_two_chats_in_one_session_switch_normally(self):
        """The control. Nothing about the workspace record changed — only which tmux
        session the target's window is in — so a refusal that fired here would be
        refusing the ordinary case."""
        self.fake.place["%2"] = ("$0", "@1")
        commands_frame.cmd_chat(mock.Mock(chat_id="beta.1", chat="api.1"))
        self.assertEqual(self.said, [])
        self.assertIn("select-window", self.fake.verbs())
        self.assertEqual(state.panes("api.1"), {})

    def test_a_target_whose_window_is_gone_is_learnt_before_anything_is_selected(self):
        """`display-message -p` against a target tmux cannot resolve answers rc **0**,
        empty stdout and no stderr (measured on 3.7c) — so the reading, not the status,
        is what says the window is absent. Learnt one step earlier than it used to be,
        which is what makes it a fact about the window rather than about a command."""
        self.fake.place["%2"] = ("$0", "@1")
        del self.fake.place["%2"]
        commands_frame.cmd_chat(mock.Mock(chat_id="beta.1", chat="api.1"))
        self.assertNotIn("select-window", self.fake.verbs())
        self.assertEqual(len(self.said), 1)
        self.assertIn("has no window any more", self.said[0])

    def test_an_answer_missing_its_session_is_read_as_no_window(self):
        """Both halves of the placement are held, and each says a different thing when it
        is missing. Asserted on the SENTENCE rather than on "it refused": with the session
        half unchecked an empty string would go on to be compared against this chat's real
        session id, refuse for looking like another session, and pass a test that only
        asked whether something was refused."""
        self.fake.place["%2"] = ("", "@1")
        commands_frame.cmd_chat(mock.Mock(chat_id="beta.1", chat="api.1"))
        self.assertNotIn("select-window", self.fake.verbs())
        self.assertIn("has no window any more", self.said[0])

    def test_an_answer_missing_its_window_is_read_as_no_window_too(self):
        """The other half. Unchecked, an empty window id reaches the reading taken after
        the select — where it would refuse for "this client did not move", which is a
        different fact said about a switch that should never have been attempted."""
        self.fake.place["%2"] = ("$0", "")
        commands_frame.cmd_chat(mock.Mock(chat_id="beta.1", chat="api.1"))
        self.assertNotIn("select-window", self.fake.verbs())
        self.assertIn("has no window any more", self.said[0])

    def test_a_session_id_that_is_not_tmuxs_own_never_becomes_a_target(self):
        """#475\'s rule, on the one value of the placement that goes back OUT to tmux.

        `_pane_place`\'s session id is what `_session_window` sends as `-t` afterwards, so
        its ALPHABET is load-bearing and not decoration — `$0;kill-server` in that position
        is the shape that already cost this project a `kill-server` armed on every window
        resize. The window id beside it is only ever compared, which is why its own
        strictness is not asserted here and its docstring says so.

        Driven through a whole switch as well as through the reader, because the property
        is "it never reaches tmux", not "the function returned None".
        """
        self.fake.place["%2"] = ("$0;kill-server", "@1")
        self.assertIsNone(commands_frame._pane_place("sock", "%2"))
        commands_frame.cmd_chat(mock.Mock(chat_id="beta.1", chat="api.1"))
        for argv in self.fake.calls:
            self.assertNotIn("$0;kill-server", argv,
                             "a session id charter did not recognise reached a tmux "
                             "command line")
        self.assertIn("has no window any more", self.said[0])

    def test_a_placement_that_is_not_two_fields_answers_nothing(self):
        """The reading is `split("\\t")` and a field COUNT rather than a `partition`, and
        that is the deletion sweep's doing: with exactly one separator by construction,
        `partition` and `rpartition` are the same program and no test could ever tell them
        apart. A count says the same thing and is a guard a test can redden — which is
        also how `_chat_being_left` asks it (#688).

        Asked of the reader directly, because a real tmux cannot be made to answer with
        the wrong number of fields."""
        for answer in ("$0", "", "$0\t@1\textra"):
            with self.subTest(answer=answer):
                with mock.patch.object(
                        tmuxctl, "run",
                        lambda *a, **k: __import__("subprocess").CompletedProcess(
                            [], 0, answer, "")), \
                     mock.patch.object(
                        commands_frame.tmuxctl, "run",
                        lambda *a, **k: __import__("subprocess").CompletedProcess(
                            [], 0, answer, "")):
                    self.assertIsNone(commands_frame._pane_place("sock", "%1"))

    def test_the_placement_reading_never_makes_a_target_of_a_non_pane(self):
        """#475's rule at the boundary the placement reading adds, asserted of the reader
        itself.

        Asked of `_pane_place` directly and not through a switch, because every route into
        it from `cmd_chat` goes past a guard that would refuse first (`chats.check`'s pane
        record, `chats.pane_of`'s own `PANE_ID_RE`) — a test driven from the top would
        pass on THOSE and say nothing about this one, which is exactly the "a guard that
        passes because a different guard caught it" shape.

        The value matters because `display-message -p` does not fail on a bad target: an
        empty `-t` resolves to the CURRENT window, so `""` would have this function report
        the asker's own place as the target's and agree that the switch may go ahead. And
        `None` — what `chats.pane_of` answers for an unusable record — is not even a
        string `subprocess` can put in an argv."""
        for bad in (None, "", "not-a-pane", "%1;kill-server", " %1"):
            with self.subTest(bad=bad):
                self.fake.calls.clear()
                self.assertIsNone(commands_frame._pane_place("sock", bad))
                self.assertEqual(self.fake.calls, [],
                                 "a value that is not a pane id reached tmux as a target")

    def test_a_select_that_reports_success_without_moving_tears_nothing_down(self):
        """The gate is on the client having MOVED, not on a command having exited 0 —
        the two are different facts and this is the one the panels ride on. Driven by a
        server that accepts the `select-window` and leaves the session where it was,
        which is what tmux itself does whenever the target is not this session's."""
        self.fake.place["%2"] = ("$0", "@1")
        real_call = self.fake.__call__

        def deaf(what, argv, **kw):
            before = dict(self.fake.current)
            out = real_call(what, argv, **kw)
            self.fake.current = before          # the select "worked" and moved nothing
            return out

        with mock.patch.object(tmuxctl, "run", deaf), \
             mock.patch.object(commands_frame.tmuxctl, "run", deaf):
            commands_frame.cmd_chat(mock.Mock(chat_id="beta.1", chat="api.1"))
        self.assertIn("select-window", self.fake.verbs())
        self.assertNotIn("kill-pane", self.fake.verbs())
        self.assertEqual(state.panes("api.1"), {"top": "%3", "bottom": "%4"})
        self.assertEqual(len(self.said), 1)
        self.assertIn("did not move", self.said[0])


class ThePaletteStartsTheSwitchRatherThanPerformingIt(PersonaIso, unittest.TestCase):
    """§4g: a row returns having started. The palette's pane is killed the instant it has
    invoked, and `kill-pane` hands SIGHUP to that pane's process group — so a switch that
    ran in the palette's own process would be racing its own teardown for three of its
    four tmux calls."""

    def setUp(self):
        super().setUp()
        _plant("api.1", workspace="api", pane="%1")
        _plant("api.2", workspace="api", pane="%2")
        self._env = mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "api"},
                                    clear=False)
        self._env.start()
        self.addCleanup(self._env.stop)

    def test_the_palette_spawns_frame_chat_detached_with_the_target_on_the_argv(self):
        spawned = []
        with mock.patch.object(commands_frame.builtin_actions, "_spawn",
                               lambda argv, *, fid: spawned.append((argv, fid))):
            commands_frame._start_chat_switch("api.1", "api.2")
        self.assertEqual(len(spawned), 1)
        argv, fid = spawned[0]
        self.assertEqual(argv[-2:], ["frame-chat", "api.2"])
        self.assertEqual(fid, "api.1")

    def test_the_palette_starts_nothing_for_a_chat_the_check_refused(self):
        """**`if out.ok and noun == choose.CHAT`, both halves, through the real
        `_draw_palette`.** The sweep found the whole `if` and its `out.ok` conjunct
        unpinned: every other case here calls `_start_chat_switch` directly, so none of
        them would notice the palette spawning a switch for a name `chats.check` has just
        refused — the chat you are already in, or one whose window record is unusable.

        Driven with `palette.own_the_tty` faked out the way `tests/test_frame_pickers.py`
        drives it, so the row travels the path a keypress does.
        """
        from types import SimpleNamespace

        from charter.frame import palette

        def pane(*rows):
            """`tests/test_frame_pickers._pane`: one row per surface, in order — the
            doorway first, which is what opens the picker and puts its roster where
            `_chosen_name` looks, then the name."""
            def fake(surface, *, then=None, **kw):
                chosen = None
                for row in rows:
                    chosen = row
                    if row is None or then is None or then(row) is None:
                        break
                return chosen
            return fake

        doorway = next(r for r in choose.open_rows("api.1")
                       if choose.noun_of(r) == choose.CHAT)

        def row_for(name):
            roster = choose.roster(choose.CHAT, "api.1")
            return next(r for r in roster.rows if roster.name_of(r) == name)

        for target, expect in (("api.1", 0), ("api.2", 1)):
            with self.subTest(target=target):
                spawned = []
                with mock.patch.dict(os.environ,
                                     {"CHARTER_SESSION_ID": "api.1"}, clear=False), \
                     mock.patch.object(commands_frame.builtin_actions, "_spawn",
                                       lambda argv, *, fid: spawned.append(argv)), \
                     mock.patch.object(commands_frame, "_say_on_screen"), \
                     mock.patch.object(commands_frame, "_close_palette"), \
                     mock.patch.object(palette, "own_the_tty",
                                       pane(doorway, row_for(target))):
                    rc = commands_frame.cmd_palette(
                        SimpleNamespace(client="/dev/ttys7", pane=True))
                self.assertEqual(rc, 0)
                self.assertEqual(len(spawned), expect,
                                 f"the palette started {len(spawned)} switch(es) for "
                                 f"{target!r}")

    def test_choose_switch_to_decides_and_does_not_perform(self):
        """`choose.switch_to` is asked in the palette's own process, which may make no
        tmux call — so for `chat` it returns the check and `_draw_palette` starts the
        command. The other three nouns still perform."""
        with mock.patch.object(tmuxctl, "run") as ran:
            out = choose.switch_to(choose.CHAT, "api.1", "api.2")
        self.assertTrue(out.ok)
        ran.assert_not_called()
