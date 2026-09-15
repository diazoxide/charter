"""Every tab is linked to exactly one harness session, never guessed.

Claude Code is handed the id charter chooses (`--session-id <uuid> --name <name>`); Codex and
opencode report theirs, and charter records the first one each start reports. Only the chat's
own harness may change the link — a harness nested inside the chat changes nothing. Resume is
offered only when the conversation exists.

Measured 2026-09-15 (claude 2.1.272 live; codex-cli 0.147.0 and opencode 1.18.23 read from
their tagged source, not run). The readings are copied into ADR 0024.
"""

from __future__ import annotations

import json
import os
import re
import unittest
import uuid
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame, config, contain, profiles
from charter.frame import launcher, leave, reopen, state, tmuxctl
from charter.harness import claude_code, codex, opencode, registry
from charter.harness.base import Harness
from tests import _gitguard
from tests._isolation import PersonaIso, make_plane, wired_as_today

FID = "beta.1"


def _chat(fid: str = FID, *, kind: str = "claude-code", pane: str = "",
          server: str = "srv") -> Path:
    assert state.claim_chat_id(fid), fid
    state.record_identity(fid, {"CHARTER_HARNESS": kind})
    state.record_server(fid, server)
    if pane:
        state.record_harness_pane(fid, pane)
    return state.frame_dir(fid)


class TheLinkIsRecordedBesideTheChat(PersonaIso, unittest.TestCase):
    """The records a link is made of, each in the chat's own directory."""

    def setUp(self) -> None:
        super().setUp()
        self.d = _chat()

    def test_a_session_id_is_held_to_a_shape_that_cannot_be_read_as_a_flag(self):
        """The id reaches a harness argv (`--resume <id>`), and one starting with `-` would
        be read as a flag. It arrives in a hook payload a chat's own shell can write."""
        for bad in ("-rf", "a b", "x" * 200, "", "a\nb", "../x", "ses;rm"):
            with self.subTest(sid=bad):
                self.assertFalse(state.record_harness_session(FID, bad))
                self.assertIsNone(state.kept_harness_session(FID))
        for good in ("e8962ccc-d263-4996-9881-c774dc586d3f", "ses_0123456789abABCDEFGHIJKLmn",
                     "x" * 128, "1234"):
            with self.subTest(sid=good):
                self.assertTrue(state.record_harness_session(FID, good))
                self.assertEqual(state.kept_harness_session(FID), good)

    def test_forgetting_the_link_takes_both_files(self):
        state.record_harness_session(FID, "u1")
        state.clear_harness_session(FID)
        self.assertIsNone(state.harness_session(FID))
        self.assertIsNone(state.kept_harness_session(FID))
        state.clear_harness_session("no.such")      # never raises

    def test_the_conversation_is_an_absolute_path_and_nothing_else(self):
        self.assertTrue(state.record_conversation(FID, "/abs/t.jsonl"))
        self.assertEqual(state.conversation(FID), "/abs/t.jsonl")
        for bad in ("t.jsonl", "/a\x00b", "/" + "x" * (contain.PATH_DISPLAY_LIMIT + 1), "",
                    None, 7):
            with self.subTest(path=bad):
                self.assertFalse(state.record_conversation(FID, bad))
                self.assertEqual(state.conversation(FID), "/abs/t.jsonl")
        self.assertTrue(state.record_conversation(
            FID, "/" + "x" * (contain.PATH_DISPLAY_LIMIT - 1)))
        state.clear_conversation(FID)
        self.assertIsNone(state.conversation(FID))

    def test_a_conversation_is_never_recorded_for_a_chat_with_no_directory(self):
        self.assertFalse(state.record_conversation("gamma.9", "/abs/t"))
        self.assertFalse((state._root() / "gamma.9").exists())

    def test_the_harness_pid_is_a_positive_number(self):
        state.record_harness_pid(FID, 4242)
        self.assertEqual(state.harness_pid(FID), 4242)
        for bad in (0, -3):
            with self.subTest(pid=bad):
                state.record_harness_pid(FID, bad)
                self.assertEqual(state.harness_pid(FID), 4242)
        for text in ("0", "-3", "x", "", "4.2"):
            with self.subTest(on_disk=text):
                (self.d / "harness.pid").write_text(text)
                self.assertIsNone(state.harness_pid(FID))

    def test_only_the_first_report_of_a_start_adopts(self):
        self.assertFalse(state.adopted(FID))
        self.assertTrue(state.adopt_report(FID))
        self.assertTrue(state.adopted(FID))
        self.assertFalse(state.adopt_report(FID))
        state.record_harness_pid(FID, 100)
        state.clear_adoption(FID)
        self.assertFalse(state.adopted(FID))
        self.assertIsNone(state.harness_pid(FID))
        self.assertTrue(state.adopt_report(FID))

    def test_an_adoption_for_a_chat_with_no_directory_is_refused(self):
        self.assertFalse(state.adopt_report("gamma.9"))
        self.assertFalse((state._root() / "gamma.9").exists())

    def test_the_adoption_is_claimed_through_config_exclusively(self):
        with mock.patch.object(state.config, "create_for", return_value=False) as create:
            self.assertFalse(state.adopt_report(FID))
        create.assert_called_once_with(self.d / "session.adopted", "")

    def test_a_start_says_whether_it_resumed(self):
        self.assertFalse(state.resumed_start(FID))
        state.record_start(FID, resumed=True)
        self.assertTrue(state.resumed_start(FID))
        state.record_start(FID, resumed=False)
        self.assertFalse(state.resumed_start(FID))

    def test_none_of_the_link_records_is_part_of_the_shape(self):
        """`clear_shape` runs when a launch claims the id; the link is what a reopen needs
        on the far side of it."""
        state.record_harness_session(FID, "u1")
        state.record_conversation(FID, "/abs/t")
        state.record_harness_pid(FID, 9)
        state.adopt_report(FID)
        state.record_start(FID, resumed=True)
        state.clear_shape(FID)
        self.assertEqual(state.kept_harness_session(FID), "u1")
        self.assertEqual(state.conversation(FID), "/abs/t")
        self.assertEqual(state.harness_pid(FID), 9)
        self.assertTrue(state.adopted(FID))
        self.assertTrue(state.resumed_start(FID))


class AChatIsFoundByItsPane(PersonaIso, unittest.TestCase):
    """opencode's hook overwrites `$CHARTER_SESSION_ID` with its own id, so its chat is found
    from `$TMUX_PANE` against the panes charter recorded (ruled, open question 1 (A))."""

    def test_exactly_one_chat_recorded_on_the_pane_is_the_answer(self):
        _chat("beta.1", pane="%4", server="srv")
        _chat("beta.2", pane="%5", server="srv")
        self.assertEqual(state.chat_in_pane("%4", "srv"), "beta.1")

    def test_a_pane_two_chats_record_resolves_to_none(self):
        _chat("beta.1", pane="%4", server="srv")
        _chat("beta.2", pane="%4", server="srv")
        self.assertIsNone(state.chat_in_pane("%4", "srv"))

    def test_a_pane_on_another_server_is_not_this_one(self):
        _chat("beta.1", pane="%4", server="other")
        self.assertIsNone(state.chat_in_pane("%4", "srv"))

    def test_the_server_is_compared_as_the_socket_it_names(self):
        """`$TMUX` names a socket PATH; a chat's record names the server by name."""
        _chat("beta.1", pane="%4", server="srv")
        self.assertEqual(state.chat_in_pane("%4", tmuxctl.socket_path("srv")), "beta.1")

    def test_a_chat_with_no_server_record_is_on_the_legacy_server(self):
        _chat("beta.1", pane="%4", server="srv")
        (state.frame_dir("beta.1") / "server").unlink()
        self.assertEqual(state.chat_in_pane("%4", tmuxctl.LEGACY_SOCKET), "beta.1")

    def test_an_old_frame_directory_is_not_a_chat(self):
        _chat("beta.1", pane="%4", server="srv")
        state.record_server("beta-4242", "srv")
        state.record_harness_pane("beta-4242", "%4")
        self.assertEqual(state.chat_in_pane("%4", "srv"), "beta.1")

    def test_a_pane_nobody_recorded_is_none(self):
        _chat("beta.1", pane="%4", server="srv")
        self.assertIsNone(state.chat_in_pane("%9", "srv"))

    def test_an_unlistable_frame_root_is_none(self):
        with mock.patch.object(state.os, "scandir", side_effect=OSError(13, "denied")):
            self.assertIsNone(state.chat_in_pane("%4", "srv"))


class _NoResume(Harness):
    """A harness charter has measured no resume for."""
    name = "nosuch"


class EachHarnessSaysHowItsSessionIsNamed(PersonaIso, unittest.TestCase):
    """Asked of the registry: three harnesses need three spellings, so no single `extra` is
    right for all of them — `first_message_argv`'s own reason for being a member."""

    def test_claude_code(self):
        h = registry.get(claude_code.NAME)
        self.assertEqual(h.new_session_argv("u", "n"), ["--session-id", "u", "--name", "n"])
        self.assertEqual(h.resume_argv("u", "n"), ["--resume", "u", "--name", "n"])
        self.assertTrue(h.chooses_session_id)
        self.assertTrue(h.reports_harness_pid)
        self.assertTrue(h.names_its_transcript)
        self.assertFalse(h.resume_needs_cwd)
        self.assertEqual(h.reports_session_at, "sessionstart")
        self.assertEqual(h.session_flags, ("--session-id", "--resume", "-r", "--continue",
                                           "-c", "--fork-session"))

    def test_codex(self):
        h = registry.get(codex.NAME)
        self.assertEqual(h.new_session_argv("s", "n"), [])
        self.assertEqual(h.resume_argv("s", "n"), ["resume", "s"])
        self.assertFalse(h.chooses_session_id)
        self.assertFalse(h.reports_harness_pid)
        self.assertTrue(h.names_its_transcript)
        self.assertFalse(h.resume_needs_cwd)
        self.assertEqual(h.reports_session_at, "sessionstart")
        self.assertEqual(h.session_flags, ("resume", "fork"))

    def test_opencode(self):
        h = registry.get(opencode.NAME)
        self.assertEqual(h.new_session_argv("s", "n"), [])
        self.assertEqual(h.resume_argv("s", "n"), ["-s", "s"])
        self.assertFalse(h.chooses_session_id)
        self.assertFalse(h.reports_harness_pid)
        self.assertFalse(h.names_its_transcript)
        self.assertTrue(h.resume_needs_cwd)
        self.assertEqual(h.reports_session_at, "tool")
        self.assertEqual(h.session_flags, ("-s", "--session", "-c", "--continue"))

    def test_a_harness_nobody_measured_offers_nothing(self):
        h = _NoResume()
        self.assertEqual(h.new_session_argv("s", "n"), [])
        self.assertIsNone(h.resume_argv("s", "n"))
        self.assertEqual(h.session_flags, ())
        self.assertEqual(h.reports_session_at, "")
        self.assertFalse(h.chooses_session_id or h.names_its_transcript
                         or h.reports_harness_pid or h.resume_needs_cwd)

    def test_resumable_is_the_registrys_answer(self):
        for name in (claude_code.NAME, codex.NAME, opencode.NAME):
            with self.subTest(harness=name):
                self.assertTrue(leave.resumable_harness(name))
        with mock.patch.dict(registry.KINDS, {"nosuch": _NoResume}):
            self.assertFalse(leave.resumable_harness("nosuch"))
            self.assertFalse(leave.conversation_exists("nosuch", "s1", ""))
        self.assertFalse(leave.resumable_harness(""))
        self.assertFalse(leave.resumable_harness("never-registered"))


def _doomed(**kw):
    base = dict(chat="beta.1", workspace="beta", persona="", harness="claude-code",
                cwd="/tmp", resume="", server="srv", live=True, active=False,
                exit_code=None, closed=False, homeless=False, cwd_gone=False,
                cwd_outside=False)
    base.update(kw)
    return leave.Doomed(**base)


def _recorded(**kw):
    base = dict(chat="beta.1", workspace="beta", persona="", harness="claude-code", cwd="",
                resume="", transcript="", active=False)
    base.update(kw)
    return reopen.Chat(**base)


class ResumeIsOfferedOnlyWhereTheConversationExists(PersonaIso, unittest.TestCase):
    """"The conversation exists" is a `stat`, asked when a surface is about to offer resume.
    Charter reads nothing inside the file."""

    def setUp(self) -> None:
        super().setUp()
        self.t = self.tmp / "t.jsonl"
        self.t.write_text("{}\n")

    def test_a_named_transcript_that_is_a_file_resumes(self):
        self.assertTrue(leave.conversation_exists("claude-code", "u1", str(self.t)))
        self.assertEqual(leave._resume_clause(
            _doomed(resume="u1", conversation=str(self.t))), leave.RESUMES)
        self.assertTrue(commands_frame._resumes(
            _recorded(resume="u1", conversation=str(self.t))))

    def test_a_claude_chat_with_a_link_and_no_transcript_yet_does_not(self):
        """C1: Claude Code reports the chosen id at SessionStart and writes no transcript
        until the first prompt."""
        for conv in (str(self.tmp / "not-yet.jsonl"), "", str(self.tmp)):
            with self.subTest(conversation=conv):
                self.assertFalse(leave.conversation_exists("claude-code", "u1", conv))
                self.assertEqual(leave._resume_clause(
                    _doomed(resume="u1", conversation=conv)), leave.NO_RESUME_YET)
                self.assertFalse(commands_frame._resumes(
                    _recorded(resume="u1", conversation=conv)))

    def test_a_codex_chat_that_took_no_turn_has_no_link_and_no_resume(self):
        """X1: Codex reports at its first turn, not at launch."""
        self.assertFalse(leave.conversation_exists("codex", "", ""))
        self.assertEqual(leave._resume_clause(_doomed(harness="codex")), leave.NO_RESUME_YET)
        self.assertTrue(leave.conversation_exists("codex", "s1", str(self.t)))
        self.assertFalse(leave.conversation_exists("codex", "s1", ""))

    def test_an_opencode_report_is_enough(self):
        """opencode names no transcript, so a reported session is the whole evidence."""
        self.assertTrue(leave.conversation_exists("opencode", "ses_x", ""))
        self.assertEqual(leave._resume_clause(_doomed(harness="opencode", resume="ses_x")),
                         leave.RESUMES)

    def test_no_link_is_never_a_resume(self):
        for name in (claude_code.NAME, codex.NAME, opencode.NAME):
            with self.subTest(harness=name):
                self.assertFalse(leave.conversation_exists(name, "", str(self.t)))

    def test_a_chat_charter_cannot_name_the_harness_of_is_not_offered_resume(self):
        self.assertFalse(leave.conversation_exists("", "u1", str(self.t)))
        self.assertEqual(leave._resume_clause(
            _doomed(harness="", resume="u1", conversation=str(self.t))),
            leave.NO_RESUME_UNKNOWN)

    def test_a_harness_with_no_resume_is_named(self):
        with mock.patch.dict(registry.KINDS, {"nosuch": _NoResume}):
            self.assertEqual(leave._resume_clause(_doomed(harness="nosuch", resume="s1")),
                             leave.NO_RESUME_HARNESS.format(harness="nosuch"))

    def test_the_quit_summary_counts_what_can_resume(self):
        p = leave.Plan(chats=(_doomed(chat="beta.1", resume="u1", conversation=str(self.t)),
                              _doomed(chat="beta.2", resume="u2")), focus="beta")
        self.assertIn("1 of 2 can resume", leave.summary(p))

    def test_a_plan_carries_each_chats_conversation(self):
        fid = state.new_chat_id("beta")
        state.record_identity(fid, {"CHARTER_HARNESS": "claude-code"})
        state.record_workspace(fid, "beta")
        state.record_harness_session(fid, "u1")
        state.record_conversation(fid, str(self.t))
        (c,) = leave.plan(live={fid}, focus="beta").chats
        self.assertEqual((c.resume, c.conversation), ("u1", str(self.t)))
        state.clear_conversation(fid)
        (c,) = leave.plan(live={fid}, focus="beta").chats
        self.assertEqual(c.conversation, "")


_UUID_SHAPED = re.compile(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}")


class TheLauncherAddsTheSession(PersonaIso, unittest.TestCase):
    """The session words are added by the launcher at the `exec`, from the chat's own record
    — after `framed_chat()` has proved the pane — and never cross tmux."""

    def setUp(self) -> None:
        super().setUp()
        make_plane(self)
        wired_as_today(self)
        self.enterContext(mock.patch.dict(
            os.environ, {"CHARTER_ROOT": str(config.ROOT), "PATH": os.environ.get("PATH", ""),
                         **_gitguard.environment()}, clear=True))
        self.enterContext(mock.patch.object(launcher.shutil, "which",
                                            return_value="/usr/bin/harness"))
        self.framed = self.enterContext(mock.patch.object(launcher, "framed_chat",
                                                          return_value=FID))
        self.enterContext(mock.patch.object(launcher, "_wait_for_the_operator"))
        self.execs: list[dict] = []
        self.exec = self.enterContext(mock.patch.object(
            launcher.os, "execvpe", side_effect=self._exec))
        _chat(FID, kind="claude-code")

    def _exec(self, program, argv, env):
        # What the chat's record says at the moment the pane stops being charter's.
        self.execs.append(dict(argv=list(argv), link=state.kept_harness_session(FID),
                               conversation=state.conversation(FID),
                               pid=state.harness_pid(FID), adopted=state.adopted(FID),
                               resumed=state.resumed_start(FID)))

    def _launch(self, profile="claude", rest=("--", "-p", "x"), **kw) -> int:
        return launcher.cmd_frame_launch(SimpleNamespace(
            profile=profile, attended=False, rest=list(rest), **kw))

    def test_a_framed_claude_start_gets_a_fresh_uuid_and_its_id_as_name(self):
        state.record_conversation(FID, "/abs/earlier.jsonl")
        self.assertEqual(self._launch(), 0)
        (e,) = self.execs
        self.assertEqual(e["argv"][:2], ["claude", "--session-id"])
        u = e["argv"][2]
        self.assertEqual(e["argv"][3:], ["--name", FID, "-p", "x"])
        self.assertEqual(uuid.UUID(u).version, 4)
        self.assertEqual(e["link"], u, "the link must exist before the harness does")
        self.assertIsNone(e["conversation"], "an earlier start's transcript is not this one's")
        self.assertFalse(e["resumed"])

    def test_each_start_chooses_a_new_id(self):
        self._launch()
        self._launch()
        self.assertNotEqual(self.execs[0]["argv"][2], self.execs[1]["argv"][2])

    def test_every_start_clears_the_adoption(self):
        for resume in (False, True):
            with self.subTest(resume=resume):
                state.record_harness_session(FID, "u-linked")
                state.record_harness_pid(FID, 77)
                state.adopt_report(FID)
                self.execs.clear()
                self._launch(rest=[], resume=resume)
                (e,) = self.execs
                self.assertIsNone(e["pid"])
                self.assertFalse(e["adopted"])

    def test_an_unframed_start_gets_no_session(self):
        self.framed.return_value = None
        state.record_harness_session(FID, "u-linked")
        state.record_harness_pid(FID, 77)
        state.adopt_report(FID)
        self.assertEqual(self._launch(), 0)
        (e,) = self.execs
        self.assertEqual(e["argv"], ["claude", "-p", "x"])
        self.assertEqual((e["link"], e["pid"], e["adopted"]), ("u-linked", 77, True),
                         "a launch with no frame writes no chat's record")

    def test_the_operators_own_session_flag_wins(self):
        """Nothing is added, so the harness is never handed `--session-id` beside a
        `--resume` (C4: refused). The link is then what the harness reports: the start clears
        the previous one, so its first report adopts rather than being read as a stranger."""
        for rest in (["--resume", "abc"], ["--resume=abc"], ["-r", "abc"], ["-c"],
                     ["--continue"], ["--fork-session"], ["--session-id", "abc"]):
            with self.subTest(rest=rest):
                state.record_harness_session(FID, "u-linked")
                self.execs.clear()
                self._launch(rest=["--", *rest])
                (e,) = self.execs
                self.assertEqual(e["argv"], ["claude", *rest])
                self.assertIsNone(e["link"])
                self.assertFalse(e["resumed"])

    def test_a_resume_hands_the_link_back(self):
        state.record_harness_session(FID, "u-linked")
        state.record_conversation(FID, "/abs/t.jsonl")
        self.assertEqual(self._launch(rest=["--"], resume=True), 0)
        (e,) = self.execs
        self.assertEqual(e["argv"], ["claude", "--resume", "u-linked", "--name", FID])
        self.assertEqual((e["link"], e["conversation"], e["resumed"]),
                         ("u-linked", "/abs/t.jsonl", True))

    def test_a_resume_with_no_link_starts_fresh_and_says_so(self):
        said: list[str] = []
        with mock.patch.object(launcher.util, "err", side_effect=said.append):
            self.assertEqual(self._launch(rest=["--"], resume=True), 0)
        self.assertTrue(any("cannot be resumed here" in s for s in said), said)
        (e,) = self.execs
        self.assertEqual(e["argv"][1], "--session-id")
        self.assertFalse(e["resumed"])

    def test_a_fresh_codex_start_forgets_the_last_starts_link(self):
        state.record_identity(FID, {"CHARTER_HARNESS": "codex"})
        state.record_harness_session(FID, "s1")
        state.record_conversation(FID, "/abs/rollout.jsonl")
        self._launch(profile="codex", rest=["--"])
        self.assertEqual(self.execs[-1]["argv"], ["codex"])
        self.assertEqual((self.execs[-1]["link"], self.execs[-1]["conversation"]), (None, None))
        self.assertFalse(self.execs[-1]["resumed"])
        state.record_harness_session(FID, "s1")
        self._launch(profile="codex", rest=["--"], resume=True)
        self.assertEqual(self.execs[-1]["argv"], ["codex", "resume", "s1"])
        self.assertEqual(self.execs[-1]["link"], "s1")
        self.assertTrue(self.execs[-1]["resumed"])

    def test_a_codex_resume_with_no_link_is_a_fresh_start(self):
        state.record_identity(FID, {"CHARTER_HARNESS": "codex"})
        with mock.patch.object(launcher.util, "err"):
            self._launch(profile="codex", rest=["--"], resume=True)
        (e,) = self.execs
        self.assertEqual(e["argv"], ["codex"])
        self.assertFalse(e["resumed"])

    def test_opencode_resumes_by_its_reported_id(self):
        state.record_identity(FID, {"CHARTER_HARNESS": "opencode"})
        state.record_harness_session(FID, "ses_abc")
        self._launch(profile="opencode", rest=["--"], resume=True)
        self.assertEqual(self.execs[-1]["argv"], ["opencode", "-s", "ses_abc"])

    def test_a_harness_with_no_resume_starts_fresh(self):
        with mock.patch.dict(registry.KINDS, {"nosuch": _NoResume}):
            state.record_harness_session(FID, "s1")
            with mock.patch.object(launcher.util, "err") as err:
                got = launcher.session_argv(SimpleNamespace(harness="nosuch", name="p"), FID,
                                            resume=True, rest=[])
            self.assertEqual(got, ([], ""))
            err.assert_called_once()
            self.assertEqual(launcher.session_argv(
                SimpleNamespace(harness="never-registered", name="p"), FID, resume=True,
                rest=[]), ([], ""))

    def test_an_exec_that_raises_restores_everything(self):
        self.exec.side_effect = OSError(5, "Input/output error")
        state.record_harness_session(FID, "u0")
        state.record_conversation(FID, "/abs/c0")
        state.record_harness_pid(FID, 9)
        state.adopt_report(FID)
        undone: list[int] = []
        p = profiles.current().profiles["claude"]
        r = launcher.attempt(p, [], fid=FID, attended=False,
                             on_exec=lambda: lambda: undone.append(1))
        self.assertEqual(r.kind, launcher.KIND_EXEC)
        self.assertEqual(undone, [1], "the caller's own undo still runs")
        self.assertEqual((state.kept_harness_session(FID), state.conversation(FID),
                          state.harness_pid(FID), state.adopted(FID)),
                         ("u0", "/abs/c0", 9, True))

    def test_an_exec_that_raises_on_a_first_start_leaves_no_link(self):
        self.exec.side_effect = OSError(5, "Input/output error")
        p = profiles.current().profiles["claude"]
        launcher.attempt(p, [], fid=FID, attended=False)
        self.assertEqual((state.kept_harness_session(FID), state.harness_session(FID),
                          state.conversation(FID), state.harness_pid(FID),
                          state.adopted(FID)), (None, None, None, None, False))

    def test_a_refused_start_touches_no_link(self):
        state.record_harness_session(FID, "u0")
        state.adopt_report(FID)
        with mock.patch.object(launcher.shutil, "which", return_value=None), \
                mock.patch.object(launcher.util, "err"):
            self.assertEqual(self._launch(), launcher.MISSING_EXIT)
        self.assertEqual((state.kept_harness_session(FID), state.adopted(FID)), ("u0", True))

    def test_the_session_never_crosses_tmux(self):
        """Only the flag rides tmux's argv; the id, the name and the uuid are the launcher's
        own, read from the chat's record in the pane."""
        state.record_harness_session(FID, "e8962ccc-d263-4996-9881-c774dc586d3f")
        words = launcher.argv("claude", [], attended=True, resume=True)
        self.assertIn("--resume", words)
        self.assertFalse(any(_UUID_SHAPED.search(w) for w in words), words)
        self.assertNotIn("--resume", launcher.argv("claude", [], attended=True))

    def test_frame_launch_takes_the_resume_flag(self):
        from charter import cli
        ns = cli.build_parser().parse_args(
            ["frame-launch", "--profile", "claude", "--resume", "--", "-p"])
        self.assertTrue(ns.resume)
        ns = cli.build_parser().parse_args(["frame-launch", "--profile", "claude"])
        self.assertFalse(ns.resume)


class TheLinkTravelsThroughTheRecord(PersonaIso, unittest.TestCase):

    def test_the_record_carries_the_conversation_under_its_own_name(self):
        c = _recorded(resume="u1", conversation="/abs/t.jsonl")
        self.assertTrue(reopen.write([reopen.Frame(workspace="beta", chats=(c,))],
                                     focus="beta"))
        raw = json.loads(reopen.path().read_text())
        self.assertEqual(raw["frames"][0]["chats"][0]["conversation"], "/abs/t.jsonl")
        self.assertEqual(reopen.read().all_chats()[0].conversation, "/abs/t.jsonl")

    def test_a_record_from_0_62_reads_as_no_conversation(self):
        state._root().mkdir(parents=True, exist_ok=True)
        reopen.path().write_text(json.dumps({"version": 1, "focus": "beta", "frames": [
            {"workspace": "beta", "chats": [{"chat": "beta.1", "workspace": "beta",
                                             "harness": "claude-code", "resume": "u1"}]}]}))
        self.assertEqual(reopen.read().all_chats()[0].conversation, "")

    def test_a_conversation_that_is_not_text_reads_as_none(self):
        state._root().mkdir(parents=True, exist_ok=True)
        reopen.path().write_text(json.dumps({"version": 1, "focus": "beta", "frames": [
            {"workspace": "beta", "chats": [{"chat": "beta.1", "workspace": "beta",
                                             "conversation": 7}]}]}))
        self.assertEqual(reopen.read().all_chats()[0].conversation, "")

    def test_a_quit_writes_down_each_chats_conversation(self):
        n = commands_frame._record_the_plane(
            [_doomed(resume="u1", conversation="/abs/t.jsonl")], focus="beta",
            active=set(), windows={}, capture=False)
        self.assertEqual(n, 1)
        self.assertEqual(reopen.read().all_chats()[0].conversation, "/abs/t.jsonl")


if __name__ == "__main__":
    unittest.main()
