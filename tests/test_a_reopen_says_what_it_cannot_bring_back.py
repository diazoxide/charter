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
from charter import workspace as ws_mod
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
                exit_code=None, closed=False, homeless=False, cwd_gone=False,
                cwd_outside=False)
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
        """Still the chat's own workspace and still reported — what #867 changed is that
        its DIRECTORY is remade rather than the chat being stood in the plane root, which
        is repairing the boundary, not moving the chat out of it."""
        note = leave.note(_doomed(resume="c", workspace="gone", homeless=True))

        self.assertIn("gone", note)
        self.assertIn(leave.RESUMES, note)
        self.assertIn("remade", note)

    def test_a_chat_with_no_workspace_record_is_told_it_cannot_be_reopened(self):
        # The one chat this design cannot bring back, and the note promises nothing —
        # not even the resume it has an id for, because there is no session to rebuild it
        # into and choosing one would be §4j's re-homing arriving as a convenience.
        note = leave.note(_doomed(resume="c", workspace=""))

        self.assertEqual(note, leave.NOT_REOPENED)
        self.assertNotIn(leave.RESUMES, note)

    def test_a_chat_it_cannot_place_still_reports_the_code_it_ended_with(self):
        self.assertEqual(leave.note(_doomed(workspace="", exit_code=3)),
                         f"{leave.NOT_REOPENED} · already ended on its own (3)")

    def test_a_directory_that_has_gone_names_the_fallback(self):
        """And the fallback is the WORKSPACE, since #867 — a row still promising the plane
        root would be promising something the restore stopped doing."""
        note = leave.note(_doomed(resume="c", cwd="/nowhere", cwd_gone=True))

        self.assertIn("/nowhere", note)
        self.assertIn("reopens in its workspace", note)
        self.assertNotIn("plane root", note)

    def test_a_directory_outside_the_workspace_says_the_chat_will_be_moved(self):
        """The clause #867 added, and the reason it had to exist: this state used to cost
        the operator nothing to be told about, because the restore stood the chat back in
        that directory. It now moves it, and a preview that said nothing would be option
        (b) — *"loses information and silently moves you"* — arriving on the quit surface
        instead of the restore one."""
        note = leave.note(_doomed(resume="c", cwd="/elsewhere", cwd_outside=True))

        self.assertIn("/elsewhere", note)
        self.assertIn("outside its workspace", note)

    def test_a_directory_inside_the_workspace_is_not_worth_a_clause(self):
        """`cwd_outside=False` — the ordinary chat, whose directory comes back as it is."""
        self.assertEqual(leave.note(_doomed(resume="c", cwd="/tmp")), leave.RESUMES)

    def test_no_directory_recorded_names_the_workspace_too(self):
        """The third cwd clause, which the same reword moved off the plane root."""
        note = leave.note(_doomed(resume="c", cwd=""))

        self.assertIn("no directory recorded", note)
        self.assertIn("reopens in its workspace", note)

    def test_a_chat_that_already_ended_says_with_which_code(self):
        self.assertIn("(7)", leave.note(_doomed(resume="c", exit_code=7)))

    def test_a_clean_chat_says_only_the_one_thing_it_is_about(self):
        self.assertEqual(leave.note(_doomed(resume="c")), leave.RESUMES)


class TheConfirmationIsAWarningNotAMenu(PersonaIso, unittest.TestCase):
    """The rows the operator reads before the keypress commits anything."""

    def _plan(self, *chats_):
        return leave.Plan(chats=tuple(chats_), focus="alpha")

    def test_every_chat_row_is_refused_and_the_row_that_runs_is_where_the_cursor_lands(self):
        # `palette.narrow` puts the cursor on the FIRST row when nothing has been typed,
        # refused or not — so the confirming row has to be first or the surface opens with
        # Enter bound to nothing. Measured, with the earlier last-row ordering.
        rows = leave.confirm_rows(
            self._plan(_doomed(chat="alpha.1", resume="c"),
                       _doomed(chat="alpha.2", harness="opencode")),
            verb=leave.QUIT)

        self.assertEqual([r.refused for r in rows], [False, True, True])
        self.assertTrue(leave.goes_through(rows[0], leave.QUIT))

    def test_the_cursor_really_lands_on_the_row_that_runs(self):
        # The property the ordering exists for, asserted through the surface rather than
        # through the list: this is what an operator sees selected when it opens.
        from charter.frame import palette
        p = self._plan(_doomed(chat="alpha.1", resume="c", active=True))
        surface = palette.Palette(catalogue=leave.confirm_rows(p, verb=leave.QUIT),
                                  label=leave.QUIT)
        surface.render(80, 12)

        self.assertTrue(leave.goes_through(surface.selected, leave.QUIT))

    def test_each_chat_row_carries_that_chats_own_sentence(self):
        rows = leave.confirm_rows(
            self._plan(_doomed(chat="alpha.1", resume="c"),
                       _doomed(chat="alpha.2", harness="opencode")),
            verb=leave.QUIT)

        self.assertEqual(rows[1].note, leave.RESUMES)
        self.assertIn("opencode", rows[2].note)

    def test_the_chat_you_are_looking_at_is_marked(self):
        rows = leave.confirm_rows(self._plan(_doomed(active=True)), verb=leave.QUIT)

        self.assertTrue(rows[1].mark)

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

        self.assertIn("not come back", rows[0].title)
        self.assertTrue(leave.goes_through(rows[0], leave.CLOSE))

    def test_the_summary_a_close_prints_is_closes_and_not_quits(self):
        # The stderr warning and the confirming row share one function, so a close that
        # printed "quit — stop 1 chat" above its own row would be describing the wrong
        # command — the exact confusion the two titles exist to keep apart.
        p = self._plan(_doomed(resume="c"))

        self.assertTrue(leave.summary(p, verb=leave.QUIT).startswith("quit —"))
        self.assertTrue(leave.summary(p, verb=leave.CLOSE).startswith("close alpha.1 —"))
        self.assertEqual(leave.summary(p), leave.summary(p, verb=leave.QUIT),
                         "quit is the default, because it is the caller with no verb")

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
                mock.patch.object(commands_frame.tmuxctl, "operator_server",
                                  return_value=None), \
                mock.patch.object(commands_frame, "_attach_after_reopen",
                                  return_value=0):
            return commands_frame.cmd_reopen(SimpleNamespace())

    def test_inside_the_operators_own_tmux_it_refuses_rather_than_half_reopening(self):
        self._record(self._chat())
        with mock.patch.object(commands_frame.sys.stdout, "isatty",
                               return_value=True), \
                mock.patch.object(commands_frame.tmuxctl, "version",
                                  return_value=(3, 7)), \
                mock.patch.object(commands_frame.tmuxctl, "operator_server",
                                  return_value=("/tmp/sock", "s0")), \
                mock.patch.object(commands_frame, "cmd_launch") as launch:
            self.assertEqual(commands_frame.cmd_reopen(SimpleNamespace()), 1)
            launch.assert_not_called()
        self.assertIsNotNone(reopen.read(), "and the record is left to try again")

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

    def test_a_recorded_directory_outside_the_workspace_is_not_where_it_comes_back(self):
        """#867, end to end: the workspace is the isolation boundary, so that is where the
        launcher is standing — not in the directory the record happens to carry.

        The fixture is the operator's own `harness-wrapper.1`, in miniature: a chat that
        says ``workspace = alpha`` and ``cwd =`` somewhere else entirely, which is the state
        typing `charter` in the plane root produced before the tab machinery existed."""
        where = config.ROOT / "somewhere"
        where.mkdir()
        self._record(self._chat(workspace="alpha", cwd=str(where)))

        self.assertEqual(self._reopen(), 0)

        self.assertEqual(self.calls[0].workspace, "alpha")
        self.assertEqual(self.calls[0].cwd,
                         str(ws_mod.workspace_dir("alpha").resolve()))

    def test_a_recorded_directory_inside_the_workspace_is_kept_exactly(self):
        """The other half of the same rule, and the reason the test is containment rather
        than equality: a chat recorded in ``workspaces/<ws>/<repo>`` is standing in one of
        that workspace's clones ON PURPOSE, and hauling it up to the workspace root would
        be a new silent move put in place of the old one."""
        clone = ws_mod.ensure("alpha") / "some-repo"
        clone.mkdir()
        self._record(self._chat(workspace="alpha", cwd=str(clone)))

        self.assertEqual(self._reopen(), 0)

        self.assertEqual(self.calls[0].cwd, str(clone.resolve()))

    def test_a_directory_that_has_gone_falls_back_to_the_workspace(self):
        """It used to fall back to the plane root, which #867 is the end of: a workspace
        the chat named is a better answer than the one directory every workspace shares."""
        self._record(self._chat(cwd="/nowhere-at-all"))

        self.assertEqual(self._reopen(), 0)

        self.assertEqual(self.calls[0].cwd,
                         str(ws_mod.workspace_dir("alpha").resolve()))

    def test_a_workspace_with_no_directory_yet_is_made_before_the_chat_lands_in_it(self):
        """#867's second half. `_launch_root` answers `config.ROOT` for a workspace that has
        no directory — a real state, since `switch.workspaces` offers `default` whether or
        not its directory exists — so a restore that reused it would put the chat in the
        plane root by a second route while reporting the workspace."""
        self.assertFalse(ws_mod.workspace_dir("alpha").exists(),
                         "or this case is not about a workspace that has to be made")
        self._record(self._chat(cwd="/nowhere-at-all"))

        self.assertEqual(self._reopen(), 0)

        self.assertTrue(ws_mod.workspace_dir("alpha").is_dir())
        self.assertEqual(self.calls[0].cwd,
                         str(ws_mod.workspace_dir("alpha").resolve()))

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

    def test_the_manifest_outranks_a_stale_pointer_left_on_a_recycled_ordinal(self):
        """The dependency this design was written around, closed by #791 and pinned here.

        **A reopen mints a FRESH chat id — and very often the SAME one.** `new_chat_id`
        walks upward from 1 and `reap` frees the ordinals a quit's chats held, so
        `alpha.1` quit and reopened is `alpha.1` again. That made the per-session pointer a
        live hazard rather than a theoretical one: while `.charter/sessions/<fid>.workspace`
        was a rung of `state.own_workspace`, a `charter workspace use gamma` typed inside
        the OLD `alpha.1` would have decided the NEW `alpha.1`'s membership, over the
        workspace the manifest recorded and the launcher wrote.

        #791 took that rung out (`own_workspace` is now the launch pin, then the record), so
        the manifest is authoritative. This plants exactly that stale pointer and asserts the
        chat comes back in the workspace it was recorded in.
        """
        from charter import workspace as ws_mod
        # The pointer the previous chat left behind, under the id the new one will inherit.
        ws_mod.set_active("gamma", session_id="alpha.1", terminal_id="", force=True)
        self.assertEqual(ws_mod.for_session("alpha.1"), "gamma")
        self._record(self._chat(chat="alpha.1", workspace="alpha"))

        self.assertEqual(self._reopen(), 0)

        self.assertEqual(self.calls[0].workspace, "alpha")
        self.assertEqual(state.own_workspace("alpha.1"), "alpha",
                         "a pointer from the chat that held this ordinal must not decide "
                         "the membership of the chat that inherited it")

    def test_the_record_is_consumed_so_a_second_reopen_does_not_double_the_tabs(self):
        self._record(self._chat())

        self.assertEqual(self._reopen(), 0)

        self.assertIsNone(reopen.read())

    def test_a_partial_reopen_leaves_behind_exactly_the_retry(self):
        # Deleting the record whole would throw away the chats that did NOT come back,
        # which is precisely the state an operator would want to try again; leaving it whole
        # would open the ones that did a second time.
        self._record(self._chat(chat="alpha.1", workspace="alpha"),
                     self._chat(chat="beta.1", workspace="beta"))
        real = self._launch

        def _launch(args):
            if args.workspace == "beta":
                return 1
            return real(args)

        with mock.patch.object(commands_frame, "cmd_launch", side_effect=_launch), \
                mock.patch.object(commands_frame.sys.stdout, "isatty",
                                  return_value=True), \
                mock.patch.object(commands_frame.tmuxctl, "version",
                                  return_value=(3, 7)), \
                mock.patch.object(commands_frame.tmuxctl, "operator_server",
                                  return_value=None), \
                mock.patch.object(commands_frame, "_attach_after_reopen",
                                  return_value=0):
            self.assertEqual(commands_frame.cmd_reopen(SimpleNamespace()), 0)

        m = reopen.read()
        self.assertEqual([c.chat for c in m.all_chats()], ["beta.1"])
        self.assertEqual(m.focus, "alpha", "and the focus is the one the quit recorded")

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


class TheReopenPathSuppressesFourThingsInTheLauncher(PersonaIso, unittest.TestCase):
    """`Reopening` is the whole of the difference, and it is asked for by name.

    Four things in `cmd_launch` turn on it: open-or-focus (§4k) would swallow every chat
    after the first of a workspace, `select-window` would move a client that does not exist
    yet, `_drop_panels` would strip the panels off the sibling this same reopen had just
    drawn, and `attach` would block on the first chat and never build the second.

    **A fifth is gated on `_wants_attach` and is worth naming because the trap is common
    rather than exotic:** `cmd_launch` says *"this plane was quit — put it back with
    `charter reopen`"* when the manifest names its own chat, and `new_chat_id` walks upward
    from 1 while `reap` frees the ordinals a quit's chats held — so a reopen usually gets the
    SAME ids back, and every one of its own launches is named in the manifest it is in the
    middle of acting on. The sentence belongs to a launch that WAS the operator's terminal,
    which is what `_wants_attach` answers and what the two cases below pin.

    The tail those five sit in cannot be reached without a real tmux session and a real
    `attach`, so what is asserted here is the predicate the launcher branches on. The rest is
    exercised on a real server in `tests/test_quit_and_reopen_on_a_real_tmux.py`.
    """

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



class TheKeypressReachesTheTeardown(PersonaIso, unittest.TestCase):
    """`F2` → the doorway → the confirming row → the detached command. **The wiring.**

    Everything else in this file tests a function. This tests the four hops between a
    keypress and `charter frame-quit`, and it is the one part with no other coverage: the
    doorway's id has to be recognised by `_picker` (which builds the confirmation), the
    confirming row's id has to be recognised by `_draw_palette` (which must NOT hand it to
    `ActionRegistry.invoke`, where it is not an action at all), and the per-chat rows have
    to do nothing when one is clicked.

    `palette.own_the_tty` is replaced by a script rather than a terminal: it is handed the
    surface and the `then` callback, exactly as the real one is, and it plays the two
    choices an operator would make. That keeps the hops real — the same `_picker`, the same
    `_draw_palette` — with no pty and no tmux under either.
    """

    def setUp(self):
        super().setUp()
        state.frame_dir("alpha.1", create=True)
        state.record_server("alpha.1", SERVER)
        state.record_workspace("alpha.1", "alpha")
        state.record_harness_pane("alpha.1", "%1")
        state.record_identity("alpha.1", {"CHARTER_HARNESS": "claude-code",
                                          "CHARTER_WORKSPACE": "", "CHARTER_PERSONA": ""})
        state.record_harness_session("alpha.1", "conv-1")

    def _press(self, pick_first, pick_second=None):
        """Drive `_draw_palette` through *pick_first*, then *pick_second* on what it opens."""
        from charter.frame import palette
        started = []

        def _own(surface, *, fd=None, out=None, then=None):
            row = next(r for r in surface.rows if pick_first(r))
            nxt = then(row) if then is not None else None
            if pick_second is None:
                return row
            self.assertIsNotNone(nxt, "the doorway opened nothing")
            return next(r for r in nxt.rows if pick_second(r))

        with mock.patch.dict("os.environ", {"CHARTER_SESSION_ID": "alpha.1"}), \
                mock.patch.object(palette, "own_the_tty", side_effect=_own), \
                mock.patch.object(commands_frame, "_close_palette"), \
                mock.patch.object(commands_frame, "_plane_live",
                                  return_value=({"alpha.1"},
                                                {SERVER: {"alpha.1": "@0"}},
                                                {"alpha.1"})), \
                mock.patch.object(commands_frame, "_start_leaving",
                                  side_effect=lambda fid, verb: started.append(
                                      (fid, verb))), \
                mock.patch.object(commands_frame, "_say_on_screen") as said:
            rc = commands_frame._draw_palette(SimpleNamespace(chat="alpha.1"))
        return rc, started, said

    def test_quit_reaches_the_detached_command_it_is_meant_to_start(self):
        rc, started, _said = self._press(
            lambda r: leave.verb_of(r) == leave.QUIT,
            lambda r: leave.goes_through(r, leave.QUIT))

        self.assertEqual(rc, 0)
        self.assertEqual(started, [("alpha.1", leave.QUIT)])

    def test_close_reaches_its_own_command_and_not_quits(self):
        rc, started, _said = self._press(
            lambda r: leave.verb_of(r) == leave.CLOSE,
            lambda r: leave.goes_through(r, leave.CLOSE))

        self.assertEqual(rc, 0)
        self.assertEqual(started, [("alpha.1", leave.CLOSE)])

    def test_a_chat_row_inside_the_confirmation_starts_nothing(self):
        # It is `refused=True` so Enter never lands on it, but a click still can — and a
        # click that stopped the plane because it landed on the warning would be the worst
        # available outcome of this whole surface.
        rc, started, said = self._press(
            lambda r: leave.verb_of(r) == leave.QUIT,
            lambda r: r.refused and not leave.goes_through(r, leave.QUIT))

        self.assertEqual(rc, 0)
        self.assertEqual(started, [])
        said.assert_not_called()

    def test_the_doorway_row_itself_starts_nothing_if_it_is_chosen_and_refused(self):
        # A close doorway on a palette that cannot resolve its own chat carries a note and
        # `_picker` refuses to open it; the note is said and nothing is started.
        with mock.patch.object(leave, "open_rows",
                               return_value=leave.open_rows("")):
            rc, started, said = self._press(
                lambda r: leave.verb_of(r) == leave.CLOSE)

        self.assertEqual(rc, 0)
        self.assertEqual(started, [])
        said.assert_called_once()
        self.assertIn(leave.NO_CHAT_HERE, said.call_args[0][1])


class TheDoorwayScopesAndRefusesWhereItSaysItDoes(PersonaIso, unittest.TestCase):
    """`_picker`'s two `leave` branches, and `_draw_palette`'s note.

    **Every case here was asked for by the deletion sweep.** `TheKeypressReachesTheTeardown`
    drives the four hops end to end and that is what it is for — but it only ever walks the
    path where everything works, so five refusals underneath it were unpinned: dropping
    `verb is not None` from the refusal, dropping the refusal itself, collapsing the
    `only=` scoping either way, and dropping the `own_workspace` fallback all left the whole
    9,895-test suite green.

    They are asked here at `_picker`'s own seam rather than through the palette, because
    what each one decides is a property of the surface it builds: which chats the
    confirmation lists, and whether it opens at all.
    """

    def setUp(self):
        super().setUp()
        for fid, ws in (("alpha.1", "alpha"), ("alpha.2", "alpha"), ("beta.1", "beta")):
            state.frame_dir(fid, create=True)
            state.record_server(fid, SERVER)
            state.record_workspace(fid, ws)
            state.record_harness_pane(fid, "%1")
            state.record_identity(fid, {"CHARTER_HARNESS": "claude-code",
                                        "CHARTER_WORKSPACE": "", "CHARTER_PERSONA": ""})
        self.live = ({"alpha.1", "alpha.2", "beta.1"},
                     {SERVER: {"alpha.1": "@0", "alpha.2": "@1", "beta.1": "@2"}},
                     set())

    def _open(self, row, fid="alpha.1"):
        with mock.patch.object(commands_frame, "_plane_live", return_value=self.live):
            return commands_frame._picker(row, fid, [])

    def _titles(self, surface):
        return [r.title for r in surface.rows]

    def test_quits_confirmation_lists_every_chat_on_the_plane(self):
        surface = self._open(leave.open_rows("alpha.1")[0])

        self.assertIsNotNone(surface)
        listed = " ".join(self._titles(surface))
        for fid in ("alpha.1", "alpha.2", "beta.1"):
            self.assertIn(fid, listed)

    def test_closes_confirmation_lists_only_the_chat_it_was_opened_in(self):
        # The `only=` scoping. Collapsed either way this becomes quit wearing close's title,
        # or a close that can never name a chat.
        surface = self._open(leave.open_rows("alpha.2")[1], fid="alpha.2")

        self.assertIsNotNone(surface)
        listed = " ".join(self._titles(surface))
        self.assertIn("alpha.2", listed)
        self.assertNotIn("alpha.1", listed)
        self.assertNotIn("beta.1", listed)

    def test_a_refused_doorway_opens_nothing(self):
        # `leave.open_rows("")` refuses the close row, because there is no chat to close.
        # Without the refusal the surface would open over every chat on the plane under a
        # title that says it is about one.
        refused = leave.open_rows("")[1]
        self.assertTrue(refused.note)

        self.assertIsNone(self._open(refused))

    def test_an_action_row_is_not_a_doorway_and_opens_nothing_here(self):
        from charter.frame import overlay
        self.assertIsNone(self._open(overlay.Row(id="frame.detach", title="detach")))

    def test_a_chat_with_no_workspace_of_its_own_still_gets_a_confirmation(self):
        # `state.own_workspace(fid) or ""` — the fallback the sweep dropped. `leave.plan`
        # takes `focus` as text, and `None` would reach the manifest as the workspace a
        # reopen attaches to.
        state.frame_dir("ghost.9", create=True)
        state.record_server("ghost.9", SERVER)
        self.assertIsNone(state.own_workspace("ghost.9"))

        surface = self._open(leave.open_rows("ghost.9")[0], fid="ghost.9")

        self.assertIsNotNone(surface)


class ARefusedDoorwaySaysItsReasonOnScreen(PersonaIso, unittest.TestCase):
    """`_draw_palette`'s `if chosen.note and leave.verb_of(chosen) is not None`.

    The sweep found the `chosen.note` half unpinned: without it, choosing one of the
    warning's own per-chat rows — which are `refused=True`, so a click can still land on one
    — would put that chat's note on the frame's attention row as though something had been
    refused. Nothing was refused; the row is the warning.
    """

    def setUp(self):
        super().setUp()
        state.frame_dir("alpha.1", create=True)
        state.record_server("alpha.1", SERVER)
        state.record_workspace("alpha.1", "alpha")
        state.record_harness_pane("alpha.1", "%1")
        state.record_identity("alpha.1", {"CHARTER_HARNESS": "claude-code",
                                          "CHARTER_WORKSPACE": "", "CHARTER_PERSONA": ""})

    def _choose(self, pick_first, pick_second=None):
        from charter.frame import palette

        def _own(surface, *, fd=None, out=None, then=None):
            row = next(r for r in surface.rows if pick_first(r))
            nxt = then(row) if then is not None else None
            if pick_second is None:
                return row
            self.assertIsNotNone(nxt)
            return next(r for r in nxt.rows if pick_second(r))

        with mock.patch.dict("os.environ", {"CHARTER_SESSION_ID": "alpha.1"}), \
                mock.patch.object(palette, "own_the_tty", side_effect=_own), \
                mock.patch.object(commands_frame, "_close_palette"), \
                mock.patch.object(commands_frame, "_plane_live",
                                  return_value=({"alpha.1"},
                                                {SERVER: {"alpha.1": "@0"}},
                                                {"alpha.1"})), \
                mock.patch.object(commands_frame, "_start_leaving"), \
                mock.patch.object(commands_frame, "_say_on_screen") as said:
            commands_frame._draw_palette(SimpleNamespace(chat="alpha.1"))
        return said

    def test_a_refused_doorway_puts_its_reason_on_the_attention_row(self):
        with mock.patch.object(leave, "open_rows", return_value=leave.open_rows("")):
            said = self._choose(lambda r: leave.verb_of(r) == leave.CLOSE)

        said.assert_called_once()
        self.assertEqual(said.call_args[0][1], leave.NO_CHAT_HERE)

    def test_a_per_chat_warning_row_says_nothing_because_nothing_was_refused(self):
        said = self._choose(lambda r: leave.verb_of(r) == leave.QUIT,
                            lambda r: r.refused)

        said.assert_not_called()


if __name__ == "__main__":       # pragma: no cover
    unittest.main()
