"""`charter handoff` opens a chat that is already working on the brief you approved.

The other half of `tests/test_a_handoff_refuses_before_it_changes_anything.py`: what a
handoff that is not refused DOES, in the spec's order — create the workspace, record the
todo, open the chat in the background, keep the full brief in that chat's private state,
tally the event with no names in it, and print where the chat landed.

Three properties are load-bearing beyond "it worked":

* **The first message is the stamp, a blank line and the brief verbatim.** The stamp is
  facts charter knows and carries no instruction, so the new chat — and whoever reads it
  later — can tell the first message was not typed there.
* **The todo carries the title and the provenance, never the brief.** A LIVE workspace
  commits `todos/**`, and a brief never reaches a committed file.
* **The routing mark clears on a handoff, and only after A7 has had its say.** The handoff
  IS the routing answer `routing: require` asked for; a handoff A7 refused opened nothing,
  so that turn still owes one (`tests/test_a_handoff_waits_for_a_yes.py` pins that side).
"""

from __future__ import annotations

import datetime
import json
import os
import unittest
from unittest import mock

from charter import commands_frame, config, dispatch, handoff, hooks, todos, workspace
from charter.frame import state

from tests._isolation import PersonaIso, PlaneIso, no_background_refresh, run_hook
from tests.test_a_handoff_refuses_before_it_changes_anything import BRIEF, _AHandoffFromAlpha
from tests.test_the_chat_bars_plus_makes_a_chat import _a_chat

#: Frozen so the stamp is a string this file can spell out rather than rebuild from the
#: same call it is checking.
WHEN = datetime.datetime(2026, 9, 10, 14, 5)
STAMP = "⟨handoff from chat alpha.1 · workspace alpha · 2026-09-10 14:05⟩"


class AHandoffOpensAChatThatStartsWorking(_AHandoffFromAlpha):
    def setUp(self) -> None:
        super().setUp()
        self.enterContext(mock.patch("charter.handoff._now", return_value=WHEN))

    # -- the stamp and the first message -------------------------------------------------

    def test_the_stamp_is_facts_and_no_instruction(self):
        self.assertEqual(handoff.stamp("alpha.1", "alpha", WHEN), STAMP)

    def test_the_first_message_is_the_stamp_a_blank_line_and_the_brief_verbatim(self):
        rc, _out, _err = self._handoff("beta")
        self.assertEqual(rc, 0)
        self.assertEqual(self.open.call_args.kwargs["first_message"], f"{STAMP}\n\n{BRIEF}")

    def test_the_chat_opens_in_the_named_workspace_from_the_calling_chat(self):
        self._handoff("beta")
        self.assertEqual(self.open.call_args.args, ("beta",))
        self.assertEqual(self.open.call_args.kwargs["caller"], "alpha.1")

    def test_this_workspace_is_named_like_any_other(self):
        """A chat here and a chat elsewhere are one mechanism; only the workspace differs."""
        rc, _out, _err = self._handoff("alpha")
        self.assertEqual(rc, 0)
        self.assertEqual(self.open.call_args.args, ("alpha",))

    def test_the_persona_asked_for_reaches_the_seam(self):
        self.make_persona("forge")
        rc, _out, _err = self._handoff("beta", persona="forge")
        self.assertEqual(rc, 0)
        self.assertEqual(self.open.call_args.kwargs["persona"], "forge")

    # -- the workspace ---------------------------------------------------------------------

    def test_create_makes_a_local_workspace_with_its_vision_before_the_chat_opens(self):
        seen = []
        self.during_open.append(lambda: seen.append(workspace.read_vision("gamma")))
        rc, _out, _err = self._handoff("gamma", create=True, vision="Ship it")
        self.assertEqual(rc, 0)
        self.assertEqual(seen, ["Ship it"])
        # LOCAL, never `set_live`: a workspace created by a handoff commits nothing, and a
        # LIVE one would commit the todo the handoff just recorded.
        self.assertFalse(workspace.is_live("gamma"))

    # -- the todo --------------------------------------------------------------------------

    def test_a_todo_titled_by_the_briefs_first_line_is_recorded_in_the_target(self):
        self._handoff("beta")
        rows = todos.open_todos("beta")
        self.assertEqual([r["title"] for r in rows], ["Fix the widget"])
        self.assertNotIn("It breaks on resize.", rows[0]["text"])

    def test_the_todo_is_recorded_before_the_chat_opens(self):
        """A chat that fails to open leaves the work written down somewhere a person reads."""
        seen = []
        self.during_open.append(lambda: seen.append(todos.count_open("beta")))
        self._handoff("beta")
        self.assertEqual(seen, [1])

    def test_a_brief_whose_first_line_is_blank_is_titled_by_its_first_words(self):
        self._handoff("beta", brief="\n\nFix it\nmore\n")
        self.assertEqual([r["title"] for r in todos.open_todos("beta")], ["Fix it"])

    def test_a_todo_already_on_the_list_is_not_recorded_twice_and_the_chat_still_opens(self):
        """A second chat on the same brief may be exactly what was approved, so the duplicate
        is reported and the handoff continues."""
        todos.add("beta", handoff.todo_text(BRIEF, source_chat="alpha.1",
                                            source_workspace="alpha"))
        rc, _out, err = self._handoff("beta")
        self.assertEqual(rc, 0)
        self.assertEqual(todos.count_open("beta"), 1)
        self.assertIn("not recorded twice", err)

    # -- the private brief -----------------------------------------------------------------

    def test_the_full_brief_is_kept_in_the_new_chats_private_state(self):
        self._handoff("beta")
        self.assertEqual(state.brief("beta.1"), BRIEF)
        p = state.frame_dir("beta.1") / "brief"
        self.assertTrue(p.is_relative_to(config.STATE_DIR), p)
        self.assertEqual(os.stat(p).st_mode & 0o077, 0)

    def test_a_new_frame_claiming_the_id_does_not_take_the_brief(self):
        """`clear_shape` forgets a previous frame's READINGS. A brief is what the chat was
        opened to do, which is the same kind of fact as its workspace."""
        self._handoff("beta")
        state.clear_shape("beta.1")
        self.assertEqual(state.brief("beta.1"), BRIEF)

    # -- the tally -------------------------------------------------------------------------

    def test_a_handoff_event_is_tallied_with_no_names_and_no_text(self):
        self._handoff("beta")
        rows = [o for o in dispatch._read_all() if o.get("event") == "handoff"]
        self.assertEqual(len(rows), 1)
        self.assertEqual(set(rows[0]), {"created", "event", "placement", "ts"})
        self.assertEqual(rows[0]["placement"], "elsewhere")
        self.assertIs(rows[0]["created"], False)
        text = dispatch.path_for().read_text()
        self.assertNotIn("Fix the widget", text)
        self.assertNotIn("beta", text)

    def test_a_handoff_into_this_workspace_is_tallied_as_here(self):
        self._handoff("alpha")
        rows = [o for o in dispatch._read_all() if o.get("event") == "handoff"]
        self.assertEqual([r["placement"] for r in rows], ["here"])

    def test_a_created_workspace_is_tallied_as_created(self):
        self._handoff("gamma", create=True, vision="Ship it")
        rows = [o for o in dispatch._read_all() if o.get("event") == "handoff"]
        self.assertEqual([r["created"] for r in rows], [True])

    def test_a_handoff_event_is_not_a_dispatch(self):
        """The dispatch column is read as "times dispatched as a sub-agent", and it is what
        personas get retired on."""
        self._handoff("beta")
        self.assertEqual(dispatch.tally(), {})
        self.assertIsNone(dispatch.last_seen("beta"))

    # -- what it says ----------------------------------------------------------------------

    def test_the_new_chat_and_its_workspace_are_printed(self):
        rc, out, _err = self._handoff("beta")
        self.assertEqual(rc, 0)
        self.assertEqual(out, "charter handoff: opened chat beta.1 in workspace 'beta', "
                              "started on the brief\n")

    def test_a_chat_that_did_not_open_says_what_stayed(self):
        self.opened = commands_frame.Opened(
            False, "", "could not open a chat in 'beta' — the launcher returned 1 and no "
                       "chat came back")
        rc, _out, err = self._handoff("beta")
        self.assertEqual(rc, 1)
        self.assertIn("the todo is recorded in 'beta'", err)
        self.assertEqual(sorted(state._root().rglob("brief")), [])
        self.assertEqual([o for o in dispatch._read_all() if o.get("event") == "handoff"], [])

    def test_a_created_workspace_that_got_no_chat_is_named_in_what_stayed(self):
        self.opened = commands_frame.Opened(False, "", "could not open a chat in 'gamma'")
        rc, _out, err = self._handoff("gamma", create=True, vision="Ship it")
        self.assertEqual(rc, 1)
        self.assertIn("the workspace 'gamma' was created", err)

    def test_what_stays_says_the_todo_was_already_there_when_it_was(self):
        """What stays has to be true of THIS call: a reader told "the todo is recorded" who
        then finds one older row reads the sentence as a lie about the one thing left behind."""
        todos.add("beta", handoff.todo_text(BRIEF, source_chat="alpha.1",
                                            source_workspace="alpha"))
        self.opened = commands_frame.Opened(False, "", "could not open a chat in 'beta'")
        rc, _out, err = self._handoff("beta")
        self.assertEqual(rc, 1)
        self.assertIn("the todo was already on 'beta's list", err)


class TheFactsAHandoffIsMadeOf(PersonaIso):
    """`charter/handoff.py` holds no I/O beyond one stream, so its strings are checkable with
    no plane, no workspace and no tmux — and `state.brief`'s four ways of saying nothing are
    one answer."""

    def test_a_brief_with_no_words_has_no_title(self):
        """Total, so the todo writer never sees `None`. A brief this empty is refused long
        before a todo is written; the answer still has to be a string."""
        self.assertEqual(handoff.title("\n  \n"), "")

    def test_the_printed_command_survives_a_create_with_no_vision(self):
        """`--create` without `--vision` is refused before this is reached, so the empty
        vision is this function's total answer rather than a case the command can produce."""
        self.assertEqual(
            handoff.terminal_command(cli_name="claude", workspace="g", extra=[], create=True,
                                     vision=None, persona=None),
            "charter workspace create g --vision '' && charter claude --workspace g")

    def test_a_chat_charter_knows_nothing_about_has_no_brief(self):
        self.assertIsNone(state.brief("nobody.9"))

    def test_an_empty_brief_file_is_no_brief(self):
        """A chat shown an empty labelled block would read it as "your brief was blank"."""
        state.record_brief("beta.1", "")
        self.assertIsNone(state.brief("beta.1"))

    def test_a_brief_charter_cannot_write_is_not_an_exception(self):
        """Every writer under `state` degrades to "charter does not know" rather than raising
        into whatever was calling it."""
        state.record_brief("../escape", "x")
        self.assertIsNone(state.brief("../escape"))


class TheHookClearsTheRoutingMark(PlaneIso):
    """`routing: require` marks a turn that saw the roster and dispatched nothing. A handoff
    IS that routing answer, so it clears the mark the way a dispatch does."""

    def setUp(self) -> None:
        super().setUp()
        self.enterContext(mock.patch.dict(os.environ, {"CHARTER_HARNESS": "claude-code"},
                                          clear=True))
        workspace.ensure("alpha")

    def _bash(self, command):
        return run_hook(hooks.pretooluse, {
            "session_id": "s", "tool_input": {"command": command},
            "cwd": str(workspace.workspace_dir("alpha"))})

    def test_a_handoff_clears_the_routing_mark_like_a_dispatch(self):
        hooks._route_mark_set("s", ["forge"])
        self._bash("charter handoff beta <<'BRIEF'\nfix it\nBRIEF")
        self.assertIsNone(hooks._route_mark_take("s"))

    def test_a_command_that_only_mentions_handoff_keeps_the_mark(self):
        hooks._route_mark_set("s", ["forge"])
        self._bash("echo 'charter handoff beta'")
        self.assertEqual(hooks._route_mark_take("s"), ["forge"])

    def test_a_handed_off_chat_with_its_brief_recorded_may_hand_off_again(self):
        """No depth limit: every hop needs its own yes (spec: Consent). The chat that was
        handed off carries its brief in private state, which is the variant Task 4 could not
        write — and it changes nothing about whether the guard lets the next handoff through."""
        _a_chat("beta.1", ws="beta", pane="%9")
        state.record_brief("beta.1", "x")
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "claude-code",
                                          "CHARTER_SESSION_ID": "beta.1"}, clear=True):
            out = self._bash("charter handoff beta <<'BRIEF'\nFix the widget.\nBRIEF")
        decision = ((out or {}).get("hookSpecificOutput") or {}).get("permissionDecision")
        self.assertNotEqual(decision, "deny", out)


class OutsideAPlaneTheMarkIsNobodysBusiness(PersonaIso):
    """The mark lives under a plane's own state directory. Outside one, `config.STATE_DIR` is
    whatever repository the plugin is enabled in, and charter writes nothing there (#852)."""

    def test_outside_a_plane_it_touches_no_mark(self):
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "claude-code"}, clear=True):
            hooks._route_mark_set("s", ["forge"])
            run_hook(hooks.pretooluse, {
                "session_id": "s", "cwd": str(self.tmp),
                "tool_input": {"command": "charter handoff beta <<'BRIEF'\nfix it\nBRIEF"}})
            self.assertEqual(hooks._route_mark_take("s"), ["forge"])


class AHandedOffChatIsBornInItsWorkspace(PlaneIso):
    """#936's property, observed from outside: a chat a handoff opened is locked to the
    workspace it was launched in, so its SessionStart asks no workspace question.

    `beta.1` is planted exactly the way `_launch` leaves a chat — the server marker, the
    launch-recorded workspace, the identity the launch emptied (`open_in_background` empties
    `$CHARTER_WORKSPACE` so the CALLING chat's pin cannot file the new chat under its own
    workspace), and the harness pane.
    """

    def setUp(self) -> None:
        super().setUp()
        no_background_refresh(self)
        workspace.ensure("beta")
        state.frame_dir("beta.1", create=True)
        state.record_server("beta.1", commands_frame.SOCKET)
        state.record_workspace("beta.1", "beta")
        state.record_identity("beta.1", {"CHARTER_WORKSPACE": "",
                                         "CHARTER_HARNESS": "claude-code"})
        state.record_harness_pane("beta.1", "%9")
        self.enterContext(mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "beta.1",
                                                       "TMUX_PANE": "%9"}, clear=True))

    def test_a_handed_off_chat_is_asked_no_workspace_question(self):
        out = run_hook(hooks.sessionstart, {"session_id": "s"})
        context = json.dumps(out or {})
        self.assertNotIn("Confirm the workspace before any repo work", context)


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
