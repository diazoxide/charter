"""#936: a framed chat was briefed for `default` and told to pick its workspace.

Two defects, and the issue measured why neither fix is enough without the other.

**The id.** SessionStart resolved the workspace by the harness's own session id, the
`session_id` in its stdin payload, while ADR 0019 makes the CHAT id (`$CHARTER_SESSION_ID`)
the charter session. `session.current` ranks an explicit id first, so the payload id won,
and every rung keyed on the chat missed: the per-session pointer, the lock, and
`workspace.for_frame` (#597's answer to #524). The chat's first context therefore held
`default`'s todos, listed its own workspace as "another", and asked it to confirm a
workspace. The PostToolUse memory nudge made the same call with the same id.

**The lock.** A chat was locked only when the operator PICKED at launch
(`commands_frame._pin_workspace` returns early otherwise, #518). So resolving by the chat
id alone makes the nudge name the right workspace and still ask the question. Following it
in a silent-launch chat SUCCEEDED: the chat's commands moved to the other workspace while
the frame kept drawing it in its own, and `workspace use <own>` to undo that was refused.

**What is true now.** A chat's launch record is its lock (`state.own_workspace`: the pin
it was launched under, then the workspace the launch resolved). Deriving it writes
nothing, so #518's "a launch that resolved silently must keep writing none" holds to the
letter. `charter workspace use <other>` inside a chat is refused and names the fix:
opening a chat in that workspace.

**#794 rejected exactly this refusal, and why it is right now is pinned here too.** Its
evidence was that every agent spawned from a frame inherits `$CHARTER_SESSION_ID`, so a
refusal keyed on it fires on sub-agents doing ordinary CLI work. That is still true, and a
sub-agent's `workspace use` is the same call with the same id as the chat's own. It is
refused for the same reason: the pointer it would write is its PARENT chat's, so a
sub-agent that succeeded would move the chat it serves. The routes a sub-agent does its
work by stay open, because neither writes anything: the tree it stands in (the cwd rung
outranks every pointer) and `--workspace <name>` per command.

**`charter handoff` is why the launch case is here.** That command opens a chat in a named
workspace through `cmd_launch(workspace=W)` with no picker, and the new chat has to start
working at once. That needs a chat BORN locked to W and briefed for it, so this module
drives the launcher itself rather than planting the record.
"""

from __future__ import annotations

import io
import json
import os
import sys
import unittest
from contextlib import redirect_stderr, redirect_stdout
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame, commands_workspace, config, hooks, todos, workspace
from charter.frame import state

from tests._isolation import PersonaIso, PlaneIso, run_hook
from tests.test_a_chat_records_where_it_was_started import _DrivesTheLauncher

#: The chat, as the launcher names it, and what `$CHARTER_SESSION_ID` holds inside it.
CHAT = "north.1"
#: Claude Code's own session id: what arrives as `session_id` in every hook payload, and
#: never the chat's. The issue's fixtures used one id for both sides, so the two could not
#: differ there; they differ here on purpose.
HARNESS_SID = "497f0e5a-0000-4000-8000-000000000936"


def _plant(fid: str, *, ws: str, pin: str = "") -> None:
    """Make *fid* a chat charter launched into *ws*, WITHOUT the picker.

    The production writers and never a hand-written file, which is
    `tests/test_frame_chat_switch._plant`'s rule. `pin` is the `$CHARTER_WORKSPACE` the
    launcher had: `_frame_identity_env` records an empty value for an unpinned launch, which
    is the case the issue measured.
    """
    state.frame_dir(fid, create=True)
    state.record_workspace(fid, ws)
    state.record_identity(fid, {"CHARTER_HARNESS": "Claude Code",
                                "CHARTER_WORKSPACE": pin, "CHARTER_PERSONA": ""})


def _context(r) -> str:
    return (r or {}).get("hookSpecificOutput", {}).get("additionalContext", "")


def _neighbours(ctx: str) -> str:
    """The other-workspaces block of a briefing, or ``""``. Parts are joined by a blank
    line and that block holds none of its own, so it is the part that opens with its mark."""
    return next((p for p in ctx.split("\n\n") if p.startswith("⬡")), "")


class AChatsLaunchIsItsLock(PersonaIso, unittest.TestCase):
    """`workspace.is_locked`, asked about a chat."""

    def setUp(self) -> None:
        super().setUp()
        for n in ("north", "gamma"):
            workspace.ensure(n)
        _plant(CHAT, ws="north")

    def test_a_chat_launched_without_the_picker_is_locked_to_its_workspace(self):
        """The silent launch. Measured before: `is_locked` → `None`, under either id."""
        self.assertFalse((config.SESSIONS_DIR / f"{CHAT}.lock").exists(),
                         "the fixture wrote a lock, so this would measure the file")
        self.assertEqual(workspace.is_locked(CHAT), "north")

    def test_deriving_the_lock_writes_nothing(self):
        """#518, to the letter: a launch that resolved silently writes no pointer and no
        lock. Green before the fix, deliberately. A fix that made every launch WRITE a lock
        would pass the case above and fail this one."""
        workspace.is_locked(CHAT)
        left = (sorted(p.name for p in config.SESSIONS_DIR.iterdir())
                if config.SESSIONS_DIR.is_dir() else [])
        self.assertEqual(left, [])

    def test_a_pinned_chat_is_locked_to_its_pin(self):
        """`state.own_workspace`'s first rung. A chat launched under `$CHARTER_WORKSPACE`
        draws and acts on the pin, so the pin is what it is locked to, whatever the launch
        resolved beside it."""
        _plant("north.2", ws="north", pin="gamma")
        self.assertEqual(workspace.is_locked("north.2"), "gamma")

    def test_the_launch_outranks_a_lock_written_under_the_chats_id(self):
        """A forced `workspace use gamma` writes a lock file saying `gamma`. If that file
        outranked the launch, the chat could not go home: `workspace use north` would be
        refused as locked to `gamma`, which is the undo the issue measured failing."""
        workspace.set_active("gamma", session_id=CHAT, terminal_id="", force=True)
        self.assertEqual(workspace.is_locked(CHAT), "north")
        self.assertEqual(workspace.set_active("north", session_id=CHAT, terminal_id=""),
                         "session")

    def test_a_session_that_is_not_a_chat_is_locked_by_what_it_confirmed(self):
        """Outside a frame nothing changes: no launch record, so only a confirmation locks."""
        self.assertIsNone(workspace.is_locked(HARNESS_SID))
        workspace.set_active("gamma", session_id=HARNESS_SID, terminal_id="")
        self.assertEqual(workspace.is_locked(HARNESS_SID), "gamma")


class WorkspaceUseInsideAChat(PersonaIso, unittest.TestCase):
    """`charter workspace use`, typed in the chat or run by a sub-agent inside it.

    **Those are one call**: a sub-agent inherits `$CHARTER_SESSION_ID`, and the command
    takes no id of its own, so the id `set_active` keys on is the chat's either way. That is
    why no case here passes `session_id=`.
    """

    def setUp(self) -> None:
        super().setUp()
        self.enterContext(mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": CHAT}))
        for n in ("north", "gamma"):
            workspace.ensure(n)
        _plant(CHAT, ws="north")

    def _run(self, fn, **kw) -> tuple[int, str]:
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = fn(SimpleNamespace(**kw))
        return rc, err.getvalue()

    def _use(self, name: str, *, force: bool = False) -> tuple[int, str]:
        return self._run(commands_workspace.cmd_workspace_use,
                         name=name, force=force, create=False)

    def test_another_workspace_is_refused(self):
        """Measured before: rc 0, and the chat's commands then acted on `gamma` while the
        frame drew it in `north`."""
        rc, _ = self._use("gamma")
        self.assertEqual(rc, 2)
        self.assertEqual(workspace.resolve(cwd=config.ROOT), "north")
        self.assertIsNone(workspace.for_session(CHAT), "the refusal wrote the pointer anyway")

    def test_the_refusal_names_opening_a_chat_there(self):
        """A conversation wanted elsewhere is a new chat (§4j), so the way out is a chat in
        `gamma`. `unlock` is not named because it releases nothing here."""
        _rc, err = self._use("gamma")
        # This sentence's OWN words, not only the shared way-out clause below it: asserting
        # the clause alone leaves the lead free to say anything at all, which is how a
        # re-spelling of it survived the whole module when measured.
        self.assertIn("locked to 'north', the workspace this chat was launched in", err)
        self.assertIn("would leave the chat itself in 'north'", err)
        self.assertIn("open a chat", err)
        self.assertIn("F2 → workspace", err)
        self.assertNotIn("workspace unlock", err)

    def test_the_refusal_names_the_route_a_subagent_has(self):
        """A sub-agent cannot press a tab or open the palette. One command in another
        workspace is `--workspace`, which writes nothing and so moves no chat."""
        _rc, err = self._use("gamma")
        self.assertIn("--workspace gamma", err)

    def test_its_own_workspace_is_a_successful_no_op(self):
        rc, _ = self._use("north")
        self.assertEqual(rc, 0)
        self.assertEqual(workspace.resolve(cwd=config.ROOT), "north")
        self.assertEqual(workspace.is_locked(), "north")

    def test_force_still_moves_the_sessions_commands_and_never_the_chat(self):
        """#794's protection, kept: `set_active` still writes the pointer when asked to, and
        every `charter` command in the session acts on it. What it never moves is the chat,
        and the lock stays the chat's own, so going home needs no second `--force`."""
        rc, _ = self._use("gamma", force=True)
        self.assertEqual(rc, 0)
        self.assertEqual(workspace.resolve(cwd=config.ROOT), "gamma")
        self.assertEqual(state.own_workspace(CHAT), "north")
        self.assertEqual(workspace.is_locked(), "north")
        self.assertEqual(self._use("north")[0], 0)

    def test_the_routes_a_subagent_works_by_outrank_the_pointer(self):
        """What #794 feared the refusal would break. A sub-agent in `workspaces/delta/…`
        acts on `delta` because it stands there, and `--workspace delta` names it outright.

        **A pointer to a THIRD workspace is written first, and that is the whole case.**
        Asked after a refusal alone, this asserted only that the cwd rung answers — there
        was no pointer for it to outrank, because the refused command wrote none. So the
        forced switch to `gamma` puts one there, and `delta` still wins from both routes."""
        workspace.ensure("delta")
        self._use("gamma", force=True)
        self.assertEqual(workspace.for_session(CHAT), "gamma",
                         "the pointer this case is about was never written")
        tree = config.WORKSPACES_DIR / "delta" / "api"
        tree.mkdir(parents=True, exist_ok=True)
        self.assertEqual(workspace.resolve(cwd=tree), "delta")
        self.assertEqual(workspace.resolve("delta"), "delta")

    def test_a_forced_switch_does_not_claim_a_lock_charter_does_not_hold(self):
        """ADR 0013: charter names a divergence between its own records rather than
        printing the happier one.

        Measured before: `workspace use gamma --force` answered *"Active workspace
        re-locked to 'gamma' … 🔒 locked for this session"*, and the very next
        `workspace use delta` answered *"locked to 'north'"*. Two commands, two answers, and
        the first one was the wrong one: `--force` moves this session's commands and never
        the chat, so the lock is still the workspace the chat was launched in."""
        _rc, err = self._use("gamma", force=True)
        # The PHRASE, not just the name: `'{locked}'` is interpolated, so asserting only
        # `north` passes against any sentence at all — measured, a re-spelling of this line
        # survived the whole module.
        self.assertIn("Active workspace set to 'gamma'", err)
        self.assertIn("this session's commands only", err)
        self.assertIn("still locked to 'north'", err,
                      "the lock that actually stands was not named")
        self.assertNotIn("re-locked", err)

    def test_workspace_current_names_the_lock_it_is_not_resolving_to(self):
        """The same divergence on the command that exists to explain the resolution, and
        the flow #794 protected: a sub-agent standing in another workspace's tree resolves
        by its cwd, which is right, while the lock is still the chat's own."""
        tree = config.WORKSPACES_DIR / "gamma" / "api"
        tree.mkdir(parents=True, exist_ok=True)
        with mock.patch("os.getcwd", return_value=str(tree)):
            rc, err = self._run(commands_workspace.cmd_workspace_current)
        self.assertEqual(rc, 0)
        self.assertIn("locked to 'north'", err)

    def test_create_with_use_makes_the_workspace_and_names_the_same_fix(self):
        """`create --use` goes through the same `set_active`, so it meets the same lock. The
        workspace is still made: a refusal to SWITCH is not a refusal to create."""
        rc, err = self._run(commands_workspace.cmd_workspace_create,
                            name="delta", use=True, force=False, repos=[])
        self.assertEqual(rc, 2)
        self.assertTrue((config.WORKSPACES_DIR / "delta").is_dir())
        self.assertIn("Workspace 'delta' was created.", err)
        self.assertIn("open a chat", err)
        self.assertNotIn("start a new session", err)

    def test_unlock_says_the_launch_holds_it_and_releases_nothing(self):
        """Measured before: rc 0 and "No workspace lock was set for this session", in a
        chat that is now locked. A command that answers "nothing to unlock" beside a lock
        that refuses the next switch is two answers to one question."""
        rc, err = self._run(commands_workspace.cmd_workspace_unlock)
        self.assertEqual(rc, 2)
        self.assertIn("locked to 'north', the workspace it was launched in", err)
        self.assertIn("nothing unlocks that", err)
        self.assertIn("open a chat", err)
        self.assertEqual(workspace.is_locked(), "north")

    def test_a_picked_launch_names_no_unlock_either(self):
        """`_pin_workspace`'s announcement named `charter workspace unlock` as the way out
        of the lock it takes. Under a lock the launch holds, that is a promise the command
        cannot keep."""
        said: list[str] = []
        with mock.patch("charter.util.info", side_effect=said.append):
            commands_frame._pin_workspace("north", "north.2", True)
        self.assertEqual(len(said), 1, said)
        self.assertNotIn("workspace unlock", said[0])
        self.assertIn("open a chat", said[0])


class TheSessionStartReconcileAsksTheChatToo(PersonaIso, unittest.TestCase):
    """`charter workspace _reconcile`, the FOURTH SessionStart workspace question.

    It is a real hook (`charter/cli.py`, asserted shipped by `tests/test_plugin.py`), and it
    read `$CLAUDE_CODE_SESSION_ID` or the payload's `session_id` and seeded a pointer under
    it. Inside a frame that is the HARNESS's id, and nothing on this plane reads it:
    `workspace.chosen` resolves by `session.current()`, which is the CHAT's id there, and
    `frame/state._forget_session` reaps `<chat id>.*`. So the write was unread and unreaped —
    the same miskey as #936's other three, in the one hook that WRITES.

    Seeding under the chat id instead is not the fix either, and this class pins that too. A
    chat's workspace is its launch record; a pointer seeded from the pane charter itself
    created for the harness would outrank that record in `chosen` and move the chat's
    commands off it, silently, which is the failure #936 is about.
    """

    PANE = "%7"

    def setUp(self) -> None:
        super().setUp()
        self.enterContext(mock.patch.dict(
            os.environ, {"CHARTER_SESSION_ID": CHAT, "TMUX_PANE": self.PANE}))
        for n in ("north", "gamma"):
            workspace.ensure(n)
        _plant(CHAT, ws="north")
        # What the reconcile seeds FROM: a selection left on this pane. Written the way
        # `tests/test_workspace_lock` writes one, through the id `session.terminal` mints.
        tid = workspace._terminal_id(self.PANE)
        config.TERMINALS_DIR.mkdir(parents=True, exist_ok=True)
        (config.TERMINALS_DIR / f"{tid}.workspace").write_text("gamma\n")

    def _reconcile(self) -> None:
        old = sys.stdin
        sys.stdin = io.StringIO(json.dumps({"session_id": HARNESS_SID}))
        try:
            with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
                commands_workspace.cmd_workspace_reconcile(SimpleNamespace())
        finally:
            sys.stdin = old

    def test_no_pointer_lands_under_the_harness_id(self):
        """The file the old key wrote: `.charter/sessions/<harness uuid>.workspace`, which
        nothing resolves by and no reap collects."""
        self._reconcile()
        self.assertFalse((config.SESSIONS_DIR / f"{HARNESS_SID}.workspace").exists())

    def test_no_pointer_lands_under_the_chat_id_either(self):
        """Keying it on the chat would be worse than useless: the chat's own commands would
        move to the pane's leftover workspace, outranking the launch record."""
        self._reconcile()
        self.assertFalse((config.SESSIONS_DIR / f"{CHAT}.workspace").exists())

    def test_the_chat_still_resolves_and_is_locked_to_its_launch(self):
        self._reconcile()
        self.assertEqual(workspace.resolve(cwd=config.ROOT), "north")
        self.assertEqual(workspace.is_locked(), "north")


class SessionStartBriefsTheChatForItsOwnWorkspace(PlaneIso):
    """The briefing, with a chat id and a DIFFERENT payload id, as Claude Code sends it."""

    def setUp(self) -> None:
        super().setUp()
        self.enterContext(mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": CHAT}))
        # A hook inside a chat refreshes that frame's gather in the background. Nothing
        # here is about the gather, and the suite spends no child it was not asked to.
        self.enterContext(mock.patch("charter.frame.notify.plane_changed"))
        for n in (config.DEFAULT_WORKSPACE, "north", "gamma"):
            workspace.ensure(n)
        todos.add(config.DEFAULT_WORKSPACE, "A TODO IN THE DEFAULT WORKSPACE")
        todos.add("north", "A TODO IN NORTH")
        _plant(CHAT, ws="north")

    def _briefing(self) -> str:
        return _context(run_hook(hooks.sessionstart, {"session_id": HARNESS_SID}))

    def test_a_chat_is_not_told_to_pick_a_workspace(self):
        """Measured before: "No workspace is locked for this session yet (it would
        otherwise default to `default`)"."""
        self.assertNotIn("Confirm the workspace", self._briefing())

    def test_a_picked_launch_is_not_told_either(self):
        """The picked launch's lock sat under the chat id and the hook looked under the
        payload id, so it was told it had no workspace and to run a command that is
        refused."""
        workspace.set_active("north", session_id=CHAT, terminal_id="", force=True)
        self.assertNotIn("Confirm the workspace", self._briefing())

    def test_the_todo_digest_is_the_chats_workspace(self):
        ctx = self._briefing()
        self.assertIn("A TODO IN NORTH", ctx)
        self.assertNotIn("A TODO IN THE DEFAULT WORKSPACE", ctx)

    def test_its_own_workspace_is_not_listed_as_another(self):
        """Measured before: the chat's own workspace among "17 other workspaces on this
        plane"."""
        block = _neighbours(self._briefing())
        self.assertIn("`gamma`", block, "the block this case reads was not found")
        self.assertNotIn("`north`", block)

class OutsideAFrameThePayloadIdStillDecides(PlaneIso):
    """The fallback. A harness outside a frame has no chat id, and its payload id is the only
    session it has. There is no `$CHARTER_SESSION_ID` here at all: `PlaneIso` starts from an
    environment that never saw charter, so the payload id is the only id either rung can
    read. Green before the fix, deliberately: a fix that read the chat id alone would pass
    every case above and fail these."""

    def setUp(self) -> None:
        super().setUp()
        for n in (config.DEFAULT_WORKSPACE, "gamma"):
            workspace.ensure(n)
        todos.add(config.DEFAULT_WORKSPACE, "A TODO IN THE DEFAULT WORKSPACE")

    def _briefing(self) -> str:
        return _context(run_hook(hooks.sessionstart, {"session_id": HARNESS_SID}))

    def test_an_unconfirmed_session_is_still_asked(self):
        self.assertIn("Confirm the workspace", self._briefing())

    def test_a_lock_confirmed_under_the_payload_id_still_silences_it(self):
        workspace.set_active("gamma", session_id=HARNESS_SID, terminal_id="")
        ctx = self._briefing()
        self.assertNotIn("Confirm the workspace", ctx)
        self.assertNotIn("A TODO IN THE DEFAULT WORKSPACE", ctx)


class ThePostToolUseMemoryNudgeAsksTheChatToo(PlaneIso):
    """`_mem_cadence_nudge` resolved by the payload id as well. The issue read it from code;
    this runs it. It names the workspace only when that workspace is LIVE, so `north` is the
    one live workspace here and `default`, where the payload id fell through to, is not."""

    def setUp(self) -> None:
        super().setUp()
        self.enterContext(mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": CHAT}))
        self.enterContext(mock.patch("charter.frame.notify.plane_changed"))
        self.enterContext(mock.patch.object(workspace, "is_live",
                                            side_effect=lambda w: w == "north"))
        for n in (config.DEFAULT_WORKSPACE, "north"):
            workspace.ensure(n)
        _plant(CHAT, ws="north")

    def test_the_cadence_nudge_names_the_chats_workspace(self):
        payload = {"session_id": HARNESS_SID, "tool_name": "Edit",
                   "tool_input": {"file_path": str(config.ROOT / "notes" / "scratch.txt")}}
        said = [run_hook(hooks.posttooluse, payload) for _ in range(hooks._MEM_NUDGE_EVERY)]
        self.assertIn("workspace **north**", _context(said[-1]))


class AChatOpenedInANamedWorkspaceIsBornLockedToIt(_DrivesTheLauncher, PlaneIso):
    """`cmd_launch` with `--workspace` and no picker: the call `charter handoff` makes.

    `_DrivesTheLauncher` launches into `alpha` with stdin not a terminal, so `_picker_wanted`
    answers no and `_pin_workspace` writes nothing. That is the path whose chat had no lock.
    """

    def setUp(self) -> None:
        super().setUp()
        workspace.ensure("gamma")
        calls = self._launch()
        self.assertEqual(calls["rc"], 0)
        made = sorted(d.name for d in state._root().iterdir() if d.is_dir())
        self.assertEqual(made, ["alpha.1"])
        self.fid = made[0]

    def test_it_is_locked_to_the_workspace_it_was_opened_in(self):
        self.assertEqual(workspace.is_locked(self.fid), "alpha")
        self.assertIsNone(workspace.for_session(self.fid),
                          "no picker ran, so the launch must have written nothing (#518)")

    def test_its_first_briefing_asks_nothing(self):
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": self.fid}), \
                mock.patch("charter.frame.notify.plane_changed"):
            ctx = _context(run_hook(hooks.sessionstart, {"session_id": HARNESS_SID}))
        self.assertNotIn("Confirm the workspace", ctx)

    def test_it_cannot_be_moved_to_another_workspace_by_a_command(self):
        err = io.StringIO()
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": self.fid}), \
                redirect_stdout(io.StringIO()), redirect_stderr(err):
            rc = commands_workspace.cmd_workspace_use(
                SimpleNamespace(name="gamma", force=False, create=False))
        self.assertEqual(rc, 2, err.getvalue())


if __name__ == "__main__":
    unittest.main()
