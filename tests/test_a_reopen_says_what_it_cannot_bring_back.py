"""§4e/§4f: what comes back, what does not, and the sentence that says which — per chat.

**The warning is the feature, not the decoration.** §4f asks for it *at the moment the
operator is deciding, not after*, and §4e is explicit that resume is Claude-only because
nothing else writes a session id (§2.8). So there are four sentences and not one, and which
one a chat gets is a fact about that chat rather than a hedge:

* it has an id → the conversation resumes;
* charter does not know what harness it was → say that, do not guess;
* it is Claude Code with no id yet → say that, because the harness CAN resume;
* it is a harness that records no id → name the harness, because "reopens empty" alone
  would reasonably be filed as a bug.

**And a chat that cannot be resumed still comes back.** Silently not reopening it would make
a chat vanish across a restart, which is the opposite of what the requirement asked for. The
directory, the workspace and the persona return either way; only the conversation is gone.

`cmd_launch` is patched throughout the reopen cases, and that is the point of the split
rather than a convenience: what `_reopen_one` owes the launcher is an argv, a workspace, a
directory to stand in and a `Reopening` to fill in — and those four are exactly what is
asserted. The launcher's own behaviour is covered by its own tests and, on a real server, by
`tests/test_quit_and_reopen_on_a_real_tmux.py`.
"""

from __future__ import annotations

import unittest
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame, config, persona
from charter.frame import leave, reopen, state

from tests._isolation import PersonaIso

SERVER = commands_frame.SOCKET


def _doomed(**kw):
    """One `leave.Doomed` with every field defaulted, so a case states only what it is about.

    A helper rather than a fixture object: the record is a NamedTuple and the thing under
    test is a pure function of it, so the shortest honest test is one that hands it a value.
    """
    base = dict(chat="alpha.1", workspace="alpha", persona="", harness="claude-code",
                cwd="/tmp", resume="", server=SERVER, live=True, active=False,
                exit_code=None, closed=False, homeless=False, cwd_gone=False)
    base.update(kw)
    return leave.Doomed(**base)


class TheWarningNamesWhatEachChatLoses(PersonaIso, unittest.TestCase):
    """Four resume sentences, and the qualifications that only appear when they are true."""

    def test_a_chat_with_an_id_is_promised_its_conversation(self):
        self.assertEqual(leave.note(_doomed(resume="conv-1")), leave.RESUMES)

    def test_a_chat_charter_cannot_identify_says_so(self):
        self.assertEqual(leave.note(_doomed(harness="")), leave.NO_RESUME_UNKNOWN)

    def test_claude_code_with_no_id_yet_says_it_is_the_id_that_is_missing(self):
        self.assertEqual(leave.note(_doomed(harness="claude-code")),
                         leave.NO_RESUME_YET)

    def test_another_harness_is_named_rather_than_left_as_reopens_empty(self):
        note = leave.note(_doomed(harness="opencode"))

        self.assertIn("opencode", note)
        self.assertEqual(note, leave.NO_RESUME_HARNESS.format(harness="opencode"))

    def test_only_claude_code_is_resumable_and_it_is_asked_of_the_registry(self):
        from charter.harness import claude_code
        self.assertTrue(leave.resumable_harness(claude_code.NAME))
        self.assertFalse(leave.resumable_harness("opencode"))
        self.assertFalse(leave.resumable_harness("codex"))
        self.assertFalse(leave.resumable_harness(""))

    def test_a_missing_workspace_is_reported_and_never_re_homed(self):
        note = leave.note(_doomed(resume="c", workspace="gone", homeless=True))

        self.assertIn("gone", note)
        self.assertIn(leave.RESUMES, note)

    def test_a_chat_with_no_workspace_record_says_it_reopens_unplaced(self):
        self.assertIn("unplaced", leave.note(_doomed(resume="c", workspace="")))

    def test_a_directory_that_has_gone_names_the_fallback(self):
        note = leave.note(_doomed(resume="c", cwd="/nowhere", cwd_gone=True))

        self.assertIn("/nowhere", note)
        self.assertIn("plane root", note)

    def test_a_chat_that_already_ended_says_with_which_code(self):
        self.assertIn("(7)", leave.note(_doomed(resume="c", exit_code=7)))

    def test_a_clean_chat_says_only_the_one_thing_it_is_about(self):
        self.assertEqual(leave.note(_doomed(resume="c")), leave.RESUMES)


class TheConfirmationIsAWarningNotAMenu(PersonaIso, unittest.TestCase):
    """The rows the operator reads before the keypress commits anything."""

    def _plan(self, *chats_):
        return leave.Plan(chats=tuple(chats_), focus="alpha")

    def test_every_chat_row_is_refused_and_the_only_row_that_runs_is_last(self):
        rows = leave.confirm_rows(
            self._plan(_doomed(chat="alpha.1", resume="c"),
                       _doomed(chat="alpha.2", harness="opencode")),
            verb=leave.QUIT)

        self.assertEqual([r.refused for r in rows], [True, True, False])
        self.assertTrue(leave.goes_through(rows[-1], leave.QUIT))

    def test_each_chat_row_carries_that_chats_own_sentence(self):
        rows = leave.confirm_rows(
            self._plan(_doomed(chat="alpha.1", resume="c"),
                       _doomed(chat="alpha.2", harness="opencode")),
            verb=leave.QUIT)

        self.assertEqual(rows[0].note, leave.RESUMES)
        self.assertIn("opencode", rows[1].note)

    def test_the_chat_you_are_looking_at_is_marked(self):
        rows = leave.confirm_rows(self._plan(_doomed(active=True)), verb=leave.QUIT)

        self.assertTrue(rows[0].mark)

    def test_a_plane_with_nothing_open_gets_no_row_that_runs(self):
        rows = leave.confirm_rows(self._plan(), verb=leave.QUIT)

        self.assertEqual([r.refused for r in rows], [True])
        self.assertEqual(rows[0].title, leave.NOTHING_OPEN)

    def test_the_summary_counts_what_can_come_back(self):
        p = self._plan(_doomed(chat="alpha.1", resume="c"),
                       _doomed(chat="alpha.2", harness="opencode"))

        self.assertEqual(leave.summary(p),
                         "quit — stop 2 chats in 1 workspace; 1 of 2 can resume the "
                         "conversation")

    def test_closes_summary_says_forget_where_quits_says_resume(self):
        rows = leave.confirm_rows(self._plan(_doomed(resume="c")), verb=leave.CLOSE)

        self.assertIn("not come back", rows[-1].title)
        self.assertTrue(leave.goes_through(rows[-1], leave.CLOSE))

    def test_close_is_listed_with_its_reason_when_there_is_no_chat_to_close(self):
        # #512's rule: an option you cannot see is one you cannot ask about. Quit needs no
        # target and stays available; close does, and says so.
        quit_row, close_row = leave.open_rows("")

        self.assertFalse(quit_row.refused)
        self.assertTrue(close_row.refused)
        self.assertEqual(close_row.note, leave.NO_CHAT_HERE)
        # And with a chat, neither is refused and neither carries a note.
        self.assertEqual([r.refused for r in leave.open_rows("alpha.1")], [False, False])
        self.assertEqual([r.note for r in leave.open_rows("alpha.1")], ["", ""])

    def test_the_two_doorways_cannot_be_confused_with_an_action(self):
        from charter.frame import component
        for row in leave.open_rows("alpha.1"):
            self.assertFalse(component.usable_id(row.id),
                             "a provider could ship an action with this id and steal the "
                             "keypress")
            self.assertIsNotNone(leave.verb_of(row))
            self.assertTrue(leave.is_row(row))

    def test_an_action_row_is_not_read_as_one_of_these(self):
        from charter.frame import overlay
        self.assertIsNone(leave.verb_of(overlay.Row(id="frame.detach", title="x")))
        self.assertFalse(leave.is_row(overlay.Row(id="frame.detach", title="x")))
        # A row whose id merely CONTAINS a colon is not this module's either.
        self.assertFalse(leave.is_row(overlay.Row(id="pick:workspace", title="x")))


class WhatAReopenPutsBack(PersonaIso, unittest.TestCase):
    """The four items, the resume flag, and the honest fallbacks."""

    def setUp(self):
        super().setUp()
        self.calls = []
        config.private_mkdir(state._root())

    def _launch(self, args):
        """Stand in for the launcher: claim an id, record what a launch records, report.

        It writes the same three records `cmd_launch` writes for the reopen path, because
        the assertions below are about what the reopened chat's own directory then says —
        which is the reading every surface of the frame does.
        """
        self.calls.append(SimpleNamespace(harness=args.harness, rest=list(args.rest),
                                          workspace=args.workspace, cwd=__import__(
                                              "os").getcwd()))
        fid = state.new_chat_id(args.workspace or "default")
        state.record_workspace(fid, args.workspace or "default")
        state.record_cwd(fid, __import__("os").getcwd())
        state.record_harness_pane(fid, "%7")
        args.reopening.fid = fid
        commands_frame._restore_recorded_chat(args.reopening.chat, fid)
        return 0

    def _record(self, *chats_, focus="alpha"):
        by_ws: dict = {}
        for c in chats_:
            by_ws.setdefault(c.workspace, []).append(c)
        reopen.write([reopen.Frame(workspace=w, chats=tuple(cs))
                      for w, cs in by_ws.items()], focus=focus)

    def _chat(self, **kw):
        base = dict(chat="alpha.1", workspace="alpha", persona="", harness="claude-code",
                    cwd=str(config.ROOT), resume="", transcript="", active=True)
        base.update(kw)
        return reopen.Chat(**base)

    def _reopen(self):
        with mock.patch.object(commands_frame, "cmd_launch", side_effect=self._launch), \
                mock.patch.object(commands_frame.sys.stdout, "isatty",
                                  return_value=True), \
                mock.patch.object(commands_frame.tmuxctl, "version",
                                  return_value=(3, 7)), \
                mock.patch.object(commands_frame, "_attach_after_reopen",
                                  return_value=0):
            return commands_frame.cmd_reopen(SimpleNamespace())

    def test_resume_is_appended_to_the_harness_argv_for_claude_with_an_id(self):
        self._record(self._chat(resume="conv-1"))

        self.assertEqual(self._reopen(), 0)

        self.assertEqual(self.calls[0].rest, ["--resume", "conv-1"])
        self.assertEqual(self.calls[0].harness, "claude")

    def test_a_harness_that_records_no_id_is_never_handed_the_flag(self):
        self._record(self._chat(harness="opencode", resume="conv-1"))

        self.assertEqual(self._reopen(), 0)

        self.assertEqual(self.calls[0].rest, [])

    def test_a_chat_with_no_id_still_comes_back_empty(self):
        self._record(self._chat(resume=""))

        self.assertEqual(self._reopen(), 0)

        self.assertEqual(self.calls[0].rest, [])
        self.assertEqual(len(self.calls), 1, "it was reopened rather than dropped")

    def test_the_workspace_and_the_directory_are_the_recorded_ones(self):
        where = config.ROOT / "somewhere"
        where.mkdir()
        self._record(self._chat(workspace="alpha", cwd=str(where)))

        self.assertEqual(self._reopen(), 0)

        self.assertEqual(self.calls[0].workspace, "alpha")
        self.assertEqual(self.calls[0].cwd, str(where.resolve()))

    def test_a_directory_that_has_gone_falls_back_to_the_plane_root(self):
        self._record(self._chat(cwd="/nowhere-at-all"))

        self.assertEqual(self._reopen(), 0)

        self.assertEqual(self.calls[0].cwd, str(config.ROOT.resolve()))

    def test_the_launcher_is_left_standing_where_it_started(self):
        import os
        here = os.getcwd()
        self._record(self._chat())

        self._reopen()

        self.assertEqual(os.getcwd(), here)

    def test_the_persona_pointer_follows_the_chat_onto_its_new_id(self):
        # The recorded chat was `alpha.9`, so the pointer under `alpha.9` is about to name
        # nothing — the reopened chat gets a fresh ordinal (`alpha.1` here) and the pointer
        # has to be written under THAT id or the persona is silently lost.
        self._record(self._chat(chat="alpha.9", persona="steward"))
        self.assertIsNone(persona.for_session("alpha.1"))

        self.assertEqual(self._reopen(), 0)

        self.assertEqual(persona.for_session("alpha.1"), "steward")

    def test_the_captured_transcript_follows_the_chat_onto_its_new_id(self):
        # The old chat's directory is gone (reaped), so the transcript is the only thing
        # left of it — and the row that offers it looks the file up by the NEW chat's id.
        config.write_for(reopen.transcript_path("alpha.9"), "what was on screen\n")
        self._record(self._chat(chat="alpha.9", transcript="alpha.9.transcript"))

        self.assertEqual(self._reopen(), 0)

        self.assertFalse(reopen.transcript_path("alpha.9").exists())
        self.assertEqual(reopen.transcript_path("alpha.1").read_text(),
                         "what was on screen\n")

    def test_a_reopened_chat_draws_no_gauge_until_its_own_first_turn(self):
        """#413, and the reason §4e's gauge gate is not needed at all.

        The spec asked for the gauge to be gated on `state.exit_code(fid) is None`, on the
        grounds that a restored `session` mapping would draw another conversation's `ctx
        78%`. This reopen restores no such mapping and cannot: a reopened chat is a FRESH
        chat id, so its directory has no `session` file, `state.harness_session` answers
        ``None``, and `frame/slots.py`'s rule — no gauge rather than a wrong one — applies
        on its own. The mapping is written on the chat's first turn by Claude Code's own
        `statusLine` hook, from the live payload.

        That makes the argument independent of whether `claude --resume` preserves the
        harness's session id, which is the measurement the gate was traded against. It
        stops being independent at the stage that relaunches into a chat's OWN directory
        (Phase 5 Task 9's amendment says so), and this assertion is what will go red there.
        """
        self._record(self._chat(chat="alpha.9", resume="conv-1"))

        self.assertEqual(self._reopen(), 0)

        newest = self.calls and "alpha.1"
        self.assertIsNone(state.harness_session(newest))
        self.assertIsNone(state.kept_harness_session(newest))

    def test_the_record_is_consumed_so_a_second_reopen_does_not_double_the_tabs(self):
        self._record(self._chat())

        self.assertEqual(self._reopen(), 0)

        self.assertIsNone(reopen.read())

    def test_several_workspaces_all_come_back(self):
        self._record(self._chat(chat="alpha.1", workspace="alpha"),
                     self._chat(chat="alpha.2", workspace="alpha"),
                     self._chat(chat="beta.1", workspace="beta"))

        self.assertEqual(self._reopen(), 0)

        self.assertEqual([c.workspace for c in self.calls], ["alpha", "alpha", "beta"])

    def test_nothing_recorded_is_a_refusal_that_names_what_records_one(self):
        with mock.patch.object(commands_frame.sys.stdout, "isatty", return_value=True):
            self.assertEqual(commands_frame.cmd_reopen(SimpleNamespace()), 1)

    def test_a_pipeline_is_refused_before_anything_is_started(self):
        self._record(self._chat())
        with mock.patch.object(commands_frame.sys.stdout, "isatty",
                               return_value=False), \
                mock.patch.object(commands_frame, "cmd_launch") as launch:
            self.assertEqual(commands_frame.cmd_reopen(SimpleNamespace()), 1)
            launch.assert_not_called()
        self.assertIsNotNone(reopen.read(), "and the record is left to try again")

    def test_a_harness_this_charter_cannot_launch_is_reported_not_guessed(self):
        self._record(self._chat(harness="clyde"))
        with mock.patch.object(config, "HARNESS", {"default": None, "refused": None}):
            self.assertEqual(self._reopen(), 1)
        self.assertEqual(self.calls, [])
        self.assertIsNotNone(reopen.read(), "left in place so it can be tried again")

    def test_a_harness_this_charter_cannot_launch_falls_back_to_the_planes_default(self):
        self._record(self._chat(harness="clyde"))
        with mock.patch.object(config, "HARNESS",
                               {"default": "claude", "refused": None}):
            self.assertEqual(self._reopen(), 0)
        self.assertEqual(self.calls[0].harness, "claude")


class TheReopenPathSuppressesThreeThingsInTheLauncher(PersonaIso, unittest.TestCase):
    """`Reopening` is the whole of the difference, and it is asked for by name."""

    def test_an_ordinary_launch_is_not_a_reopen_and_does_attach(self):
        args = SimpleNamespace()

        self.assertIsNone(commands_frame._reopening(args))
        self.assertTrue(commands_frame._wants_attach(args))

    def test_a_field_that_is_not_a_reopening_is_not_read_as_one(self):
        # A namespace carrying a truthy `reopening` of the wrong type must not turn the
        # launcher's three suppressions on — the seam is a type, not a flag.
        args = SimpleNamespace(reopening=True)

        self.assertIsNone(commands_frame._reopening(args))
        self.assertTrue(commands_frame._wants_attach(args))

    def test_a_reopen_carries_the_recorded_chat_and_takes_the_new_id_back(self):
        rec = reopen.Chat(chat="alpha.9", workspace="alpha", persona="", harness="",
                          cwd="", resume="", transcript="", active=False)
        r = commands_frame.Reopening(rec)
        args = SimpleNamespace(reopening=r)

        self.assertIs(commands_frame._reopening(args), r)
        self.assertFalse(commands_frame._wants_attach(args))
        self.assertEqual(r.fid, "", "empty until a launcher claims one")


if __name__ == "__main__":       # pragma: no cover
    unittest.main()
