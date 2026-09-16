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

from charter import commands_frame, config, contain, hooks, profiles
from charter import workspace as ws_mod
from charter.frame import launcher, leave, reopen, state, tmuxctl
from charter.harness import claude_code, codex, opencode, registry
from charter.harness.base import Harness
from tests import _gitguard
from tests._isolation import PersonaIso, PlaneIso, make_plane, run_hook, wired_as_today

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

    def test_an_id_that_names_no_directory_records_and_reads_nothing_and_never_raises(self):
        """`frame_dir` answers ``None`` for an id that is a path; a hook hands these whatever
        `$CHARTER_SESSION_ID` holds, and a hook must never break a turn."""
        for bad in ("../escape", "a/b"):
            with self.subTest(fid=bad):
                self.assertFalse(state.record_conversation(bad, "/abs/t"))
                self.assertFalse(state.adopt_report(bad))
                self.assertFalse(state.resumed_start(bad))
                # The clears too: each is reached from a start and from a hook.
                self.assertIsNone(state.clear_conversation(bad))
                self.assertIsNone(state.clear_adoption(bad))
                self.assertIsNone(state.clear_harness_session(bad))
                self.assertIsNone(state.record_harness_pid(bad, 7))
                self.assertIsNone(state.record_start(bad, resumed=True))

    def test_a_record_that_cannot_be_written_does_not_raise(self):
        """Both writers run from a start or a hook, where raising costs a turn."""
        with mock.patch.object(state.config, "replace_for", side_effect=OSError(28, "full")):
            self.assertIsNone(state.record_harness_pid(FID, 4242))
            self.assertIsNone(state.record_start(FID, resumed=True))
        self.assertIsNone(state.harness_pid(FID))
        self.assertFalse(state.resumed_start(FID))

    def test_a_clear_that_meets_a_directory_in_its_place_does_not_raise(self):
        """`unlink` refuses a directory (EISDIR on Linux, EPERM on macOS), and each clear goes
        on to the next file rather than stopping at the first it cannot remove."""
        (self.d / "session").mkdir()
        (self.d / "session.durable").write_text("u1\n")
        state.clear_harness_session(FID)
        self.assertFalse((self.d / "session.durable").exists())
        (self.d / "conversation").mkdir()
        state.clear_conversation(FID)
        self.assertTrue((self.d / "conversation").is_dir())
        (self.d / "session.adopted").mkdir()
        state.record_harness_pid(FID, 9)
        state.clear_adoption(FID)
        self.assertIsNone(state.harness_pid(FID))

    def test_the_link_files_are_named_as_a_running_launcher_wrote_them(self):
        """A chat started under one charter is reported on by the next one's hooks once the
        plugin updates, so these names are read across a version while the chat runs —
        spelled here by hand, not imported, so a rename cannot move both halves at once."""
        state.record_harness_session(FID, "u1")
        state.record_conversation(FID, "/abs/t")
        state.record_harness_pid(FID, 9)
        state.adopt_report(FID)
        state.record_start(FID, resumed=True)
        for name, body in (("session.durable", "u1"), ("conversation", "/abs/t"),
                           ("harness.pid", "9"), ("session.adopted", ""),
                           ("session.start", "resumed")):
            with self.subTest(file=name):
                self.assertEqual((self.d / name).read_text().strip(), body)

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


#: Codex's SessionStart input, key for key, as `codex-rs/hooks/src/schema.rs:486-497`
#: declares it at `rust-v0.147.0` with `deny_unknown_fields` (read from source, not run).
_CODEX_0_147_SESSIONSTART = {"session_id": "", "transcript_path": "", "cwd": "/w",
                             "hook_event_name": "SessionStart", "model": "gpt-5.1-codex",
                             "permission_mode": "default", "source": "startup"}


def _codex_report(sid: str, path="/abs/rollout.jsonl") -> dict:
    return {**_CODEX_0_147_SESSIONSTART, "session_id": sid, "transcript_path": path}


def _claude_report(sid, path="/abs/t.jsonl", source="startup") -> dict:
    """What Claude Code 2.1.272 hands a SessionStart hook (C1, C7): Codex's keys and more."""
    return {"session_id": sid, "transcript_path": path, "cwd": "/w",
            "hook_event_name": "SessionStart", "source": source, "model": "claude-x",
            "session_title": "t · beta.1", "scratchpad_dir": "/tmp/s"}


class ALinkFollowsOnlyTheChatsOwnHarness(PlaneIso, unittest.TestCase):
    """Which harness sent a report comes from the report, never from inherited environment,
    and only the chat's own harness may change its link (controller's rulings, C5–C7).

    Every case states its whole environment (`clear=True`): a nested harness inherits every
    `CHARTER_*` variable, `CLAUDE_PID` and `TMUX_PANE`, which is the point."""

    def setUp(self) -> None:
        super().setUp()
        _chat(FID, kind="claude-code")

    def _hook(self, data: dict, **env) -> None:
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": FID, **env}, clear=True):
            hooks._record_harness_session(data)

    def _claude(self, sid, *, pid="100", env_sid=None, **kw) -> None:
        env = {"CHARTER_HARNESS": "claude-code"}
        if pid is not None:
            env["CLAUDE_PID"] = pid
        if env_sid is not False:
            env["CLAUDE_CODE_SESSION_ID"] = sid if env_sid is None else env_sid
        self._hook(_claude_report(sid, **kw), **env)

    def _link(self):
        return (state.kept_harness_session(FID), state.conversation(FID),
                state.harness_pid(FID))

    def _a_start(self, *, resumed=False) -> None:
        """What the launcher's wrapper does as a harness starts."""
        state.clear_adoption(FID)
        state.record_start(FID, resumed=resumed)

    def test_the_first_report_of_the_chosen_id_adopts_its_pid(self):
        state.record_harness_session(FID, "u")
        self._claude("u", path="/abs/t")
        self.assertEqual(self._link(), ("u", "/abs/t", 100))
        self.assertTrue(state.adopted(FID))

    def test_a_resume_reporting_the_same_id_re_adopts(self):
        state.record_harness_session(FID, "u")
        self._claude("u")
        self._a_start(resumed=True)
        self._claude("u", pid="200", source="resume")
        self.assertEqual(state.harness_pid(FID), 200)

    def test_clear_from_the_adopted_pid_moves_the_link(self):
        state.record_harness_session(FID, "u")
        self._claude("u", path="/abs/u")
        self._claude("v", path="/abs/v", source="clear")
        self.assertEqual(self._link(), ("v", "/abs/v", 100))

    def test_a_followed_link_forgets_the_old_conversation_until_its_own_is_named(self):
        state.record_harness_session(FID, "u")
        self._claude("u", path="/abs/u")
        self._claude("v", path="relative.jsonl", source="clear")
        self.assertEqual(self._link(), ("v", None, 100))

    def test_a_nested_harness_is_ignored(self):
        state.record_harness_session(FID, "u")
        self._claude("u", path="/abs/u")
        self._claude("w", pid="300", path="/abs/w")
        self.assertEqual(self._link(), ("u", "/abs/u", 100))

    def test_a_report_of_the_linked_id_from_the_adopted_pid_renames_its_conversation(self):
        state.record_harness_session(FID, "u")
        self._claude("u", path="/abs/u")
        self._claude("u", path="/abs/u2", source="compact")
        self.assertEqual(self._link(), ("u", "/abs/u2", 100))

    def test_a_report_of_the_linked_id_from_another_pid_is_ignored(self):
        state.record_harness_session(FID, "u")
        self._claude("u", path="/abs/u")
        self._claude("u", pid="300", path="/abs/elsewhere")
        self.assertEqual(self._link(), ("u", "/abs/u", 100))

    def test_a_report_without_claude_pid_after_adoption_is_ignored(self):
        state.record_harness_session(FID, "u")
        self._claude("u", path="/abs/u")
        for sid in ("v", "u"):
            with self.subTest(sid=sid):
                self._claude(sid, pid=None, path="/abs/other")
                self.assertEqual(self._link(), ("u", "/abs/u", 100))

    def test_a_report_without_claude_pid_never_adopts(self):
        for pid in (None, "0", "-5", "x", "²"):
            with self.subTest(pid=pid):
                self._claude("x", pid=pid)
                self.assertEqual(self._link(), (None, None, None))
                self.assertFalse(state.adopted(FID))

    def test_a_report_of_another_id_before_adoption_is_ignored(self):
        state.record_harness_session(FID, "u")
        self._claude("w")
        self.assertEqual(self._link(), ("u", None, None))
        self.assertFalse(state.adopted(FID))

    def test_a_start_that_chose_no_id_adopts_its_first_report(self):
        self._claude("x")
        self.assertEqual(self._link(), ("x", "/abs/t.jsonl", 100))

    def test_codex_adopts_the_first_report_of_a_start_only(self):
        state.record_identity(FID, {"CHARTER_HARNESS": "codex"})
        env = {"CHARTER_HARNESS": "codex"}
        self._hook(_codex_report("s1", "/abs/r1"), **env)
        self.assertEqual(self._link(), ("s1", "/abs/r1", None))
        self._hook(_codex_report("s2", "/abs/r2"), **env)
        self.assertEqual(self._link(), ("s1", "/abs/r1", None))
        self._a_start()
        self._hook(_codex_report("s3", "/abs/r3"), **env)
        self.assertEqual(self._link(), ("s3", "/abs/r3", None))

    def test_a_codex_rollout_codex_names_null_records_the_link_and_no_file(self):
        state.record_identity(FID, {"CHARTER_HARNESS": "codex"})
        self._hook(_codex_report("s1", None))
        self.assertEqual(self._link(), ("s1", None, None))

    def test_a_malformed_first_report_does_not_take_the_starts_adoption(self):
        state.record_identity(FID, {"CHARTER_HARNESS": "codex"})
        self._hook(_codex_report("-rf"))
        self.assertFalse(state.adopted(FID))
        self._hook(_codex_report("s1"))
        self.assertEqual(state.kept_harness_session(FID), "s1")

    def test_a_resumed_codex_start_keeps_the_link_it_resumed(self):
        """X3, read from source: a resumed root session reports the id it had. The rule
        holds for either answer — a report naming another id is never adopted."""
        state.record_identity(FID, {"CHARTER_HARNESS": "codex"})
        state.record_harness_session(FID, "s1")
        self._a_start(resumed=True)
        self._hook(_codex_report("s9", "/abs/r9"))
        self.assertEqual(self._link(), ("s1", None, None))
        self._hook(_codex_report("s1", "/abs/r1"))
        self.assertEqual(self._link(), ("s1", "/abs/r1", None))
        self.assertTrue(state.adopted(FID), "the resumed start has adopted its report")

    def test_a_nested_non_claude_report_is_ignored(self):
        """C7, inferred for `codex` and not measured: a harness nested in a Claude chat
        inherits the outer `CLAUDE_PID` and `CLAUDE_CODE_SESSION_ID`, and reports its own
        id — so it is not a Claude Code report, and it is not this chat's kind."""
        state.record_harness_session(FID, "u")
        self._claude("u", path="/abs/u")
        inherited = {"CHARTER_HARNESS": "claude-code", "CLAUDE_PID": "100",
                     "CLAUDE_CODE_SESSION_ID": "u"}
        for payload in (_codex_report("z"), _claude_report("z")):
            with self.subTest(keys=sorted(payload)):
                self._hook(payload, **inherited)
                self.assertEqual(self._link(), ("u", "/abs/u", 100))

    def test_a_session_id_variable_unequal_to_the_payload_is_not_a_claude_report(self):
        self._claude("u2", env_sid="u")
        self.assertEqual(self._link(), (None, None, None))
        self._claude("u", env_sid="u")
        self.assertEqual(self._link(), ("u", "/abs/t.jsonl", 100))

    def test_a_missing_session_id_variable_never_adopts_and_is_ignored_after_adoption(self):
        self._claude("u", env_sid=False)
        self.assertEqual(self._link(), (None, None, None))
        self._claude("u")
        self._claude("v", env_sid=False, source="clear")
        self.assertEqual(self._link(), ("u", "/abs/t.jsonl", 100))

    def test_the_parent_pid_is_never_consulted(self):
        """C7: `os.getppid() == CLAUDE_PID` held only for a lone hook command, because Claude
        runs hooks under `/bin/sh -c` — so it is not a proof, and is not asked."""
        with mock.patch.object(hooks.os, "getppid", side_effect=AssertionError("asked")):
            self._claude("u")
        self.assertEqual(self._link(), ("u", "/abs/t.jsonl", 100))

    def test_charter_harness_never_decides_the_sender(self):
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "codex",
                                          "CLAUDE_CODE_SESSION_ID": "u"}, clear=True):
            self.assertEqual(hooks.sender(_claude_report("u")).name, claude_code.NAME)
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "claude-code"}, clear=True):
            self.assertEqual(hooks.sender(_codex_report("s")).name, codex.NAME)
            self.assertIsNone(hooks.sender(_claude_report("u")))
            self.assertIsNone(hooks.sender({}), "no id and no variable is no report")

    def test_a_codex_payload_with_an_unknown_key_is_no_report(self):
        with mock.patch.dict(os.environ, {}, clear=True):
            self.assertIsNone(hooks.sender({**_codex_report("s"), "turn_id": "t1"}))
            missing = _codex_report("s")
            del missing["permission_mode"]
            self.assertIsNone(hooks.sender(missing))

    def test_a_codex_0_147_sessionstart_payload_is_a_codex_report(self):
        """Pins `CODEX_SESSIONSTART_KEYS` against the source it was read from: a set edited
        without re-reading `schema.rs` goes red here, not silently on the plane."""
        self.assertEqual(hooks.CODEX_SESSIONSTART_KEYS, frozenset(_CODEX_0_147_SESSIONSTART))
        with mock.patch.dict(os.environ, {}, clear=True):
            self.assertEqual(hooks.sender(_codex_report("s")).name, codex.NAME)

    def test_the_chats_recorded_kind_must_be_the_sender(self):
        for kind in ("opencode", "codex", ""):
            with self.subTest(kind=kind):
                state.record_identity(FID, {"CHARTER_HARNESS": kind})
                self._claude("u")
                self.assertEqual(self._link(), (None, None, None))

    def test_an_id_that_could_be_read_as_a_flag_is_refused(self):
        for sid in ("-rf", "a b", "x" * 200, ""):
            with self.subTest(sid=sid):
                self._claude(sid)
                self.assertEqual(self._link(), (None, None, None))
                self.assertFalse(state.adopted(FID))
        self._claude(7, env_sid="7")
        self.assertEqual(self._link(), (None, None, None))

    def test_a_relative_or_nul_path_is_refused(self):
        for path in ("t.jsonl", "/a\x00b", "/" + "x" * 5000):
            with self.subTest(path=path):
                state.clear_adoption(FID)
                self._claude("u", path=path)
                self.assertEqual(self._link(), ("u", None, 100))

    def test_a_link_that_moves_wakes_the_frame(self):
        before = state.version(FID)
        self._claude("u")
        moved = state.version(FID)
        self.assertNotEqual(moved, before)
        self._claude("u", path="/abs/other", source="compact")
        self.assertEqual(state.version(FID), moved, "a report that moves nothing repaints nothing")

    def test_outside_a_frame_or_a_plane_nothing_is_recorded(self):
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "claude-code", "CLAUDE_PID": "1",
                                          "CLAUDE_CODE_SESSION_ID": "u"}, clear=True):
            hooks._record_harness_session(_claude_report("u"))
        self.assertEqual(self._link(), (None, None, None))
        with mock.patch.object(hooks, "_in_a_plane", return_value=False):
            self._claude("u")
        self.assertEqual(self._link(), (None, None, None))

    def test_the_hook_never_raises(self):
        with mock.patch.object(state, "record_harness_session", side_effect=RuntimeError):
            self.assertIsNone(self._claude("u"))
            self.assertIsNone(self._opencode("ses_abc"))

    # -- opencode: the tool hook, found by its pane (open question 1, ruled (A)) ---------- #

    def _opencode(self, sid, *, pane="%4", tmux=None, harness="opencode", data=None):
        env = {"CHARTER_HARNESS": harness, "CHARTER_SESSION_ID": sid, "TMUX_PANE": pane,
               "TMUX": f"{tmuxctl.socket_path('srv')},1,0" if tmux is None else tmux}
        with mock.patch.dict(os.environ, env, clear=True):
            return hooks._record_reported_session(
                data if data is not None else {"hook_event_name": "PreToolUse",
                                               "session_id": sid, "cwd": "/w",
                                               "tool_name": "Read", "tool_input": {}})

    def _an_opencode_chat(self, fid="beta.2", pane="%4"):
        _chat(fid, kind="opencode", pane=pane, server="srv")
        return fid

    def test_opencode_is_recorded_from_its_tool_hook_by_its_pane(self):
        fid = self._an_opencode_chat()
        self._opencode("ses_abcdef012345ABCDEFGHIJklmn")
        self.assertEqual(state.kept_harness_session(fid), "ses_abcdef012345ABCDEFGHIJklmn")
        self._opencode("ses_def")
        self.assertEqual(state.kept_harness_session(fid), "ses_abcdef012345ABCDEFGHIJklmn")
        self.assertIsNone(state.conversation(fid), "opencode names no transcript")

    def test_a_pane_two_chats_record_resolves_to_none(self):
        self._an_opencode_chat("beta.2")
        self._an_opencode_chat("beta.3")
        self._opencode("ses_abc")
        for fid in ("beta.2", "beta.3"):
            self.assertIsNone(state.kept_harness_session(fid))
            self.assertFalse(state.adopted(fid))

    def test_a_pane_charter_did_not_record_writes_nothing(self):
        fid = self._an_opencode_chat()
        for pane, tmux in (("%9", None), ("%4", f"{tmuxctl.socket_path('other')},1,0"),
                           ("", None), ("%4", "")):
            with self.subTest(pane=pane, tmux=tmux):
                self._opencode("ses_abc", pane=pane, tmux=tmux)
                self.assertIsNone(state.kept_harness_session(fid))

    def test_only_a_chat_whose_harness_reports_at_a_tool_hook_is_recorded(self):
        """The kind is the CHAT's own record (`state.identity`), not `$CHARTER_HARNESS`,
        which a harness nested in the pane inherits. A chat of a kind that reports at
        SessionStart is never linked from a tool hook."""
        self._an_opencode_chat()
        for kind in ("claude-code", "codex", "", "nosuch"):
            with self.subTest(kind=kind):
                state.record_identity("beta.2", {"CHARTER_HARNESS": kind})
                self._opencode("ses_abc")
                self.assertIsNone(state.kept_harness_session("beta.2"))
                self.assertFalse(state.adopted("beta.2"))

    def test_a_claude_code_tool_hook_in_its_own_chat_writes_nothing(self):
        """Claude Code reports at SessionStart. Its tool hooks run in its own chat's pane,
        of its own kind, carrying its own `session_id` — and still move nothing, or every
        tool call after `/clear` would drag the link back to whichever id that call named."""
        _chat("beta.2", kind="claude-code", pane="%4", server="srv")
        self._opencode("u-from-a-tool-call", harness="claude-code")
        self.assertIsNone(state.kept_harness_session("beta.2"))
        self.assertFalse(state.adopted("beta.2"))

    def test_an_opencode_nested_in_another_harnesss_chat_writes_nothing(self):
        _chat("beta.2", kind="claude-code", pane="%4", server="srv")
        self._opencode("ses_abc")
        self.assertIsNone(state.kept_harness_session("beta.2"))
        self.assertFalse(state.adopted("beta.2"))

    def test_a_malformed_opencode_id_does_not_take_the_starts_adoption(self):
        fid = self._an_opencode_chat()
        self._opencode("-rf")
        self._opencode("x y")
        self.assertFalse(state.adopted(fid))
        self._opencode("ses_ok")
        self.assertEqual(state.kept_harness_session(fid), "ses_ok")

    def test_the_recorded_id_is_the_payloads_and_the_variable_must_agree(self):
        """The shim sets `$CHARTER_SESSION_ID` and the payload's `session_id` from ONE value,
        so a genuine report has them equal — and that equality is the proof this report is
        the pane's own harness (the controller's ruling). The id charter keeps is still the
        payload's; a report whose variable names something else is ignored."""
        fid = self._an_opencode_chat()
        env = {"CHARTER_HARNESS": "opencode", "CHARTER_SESSION_ID": "ses_var",
               "TMUX_PANE": "%4", "TMUX": f"{tmuxctl.socket_path('srv')},1,0"}
        with mock.patch.dict(os.environ, env, clear=True):
            hooks._record_reported_session({"session_id": "ses_payload"})
        self.assertIsNone(state.kept_harness_session(fid))
        self.assertFalse(state.adopted(fid))
        with mock.patch.dict(os.environ, {**env, "CHARTER_SESSION_ID": "ses_payload"},
                             clear=True):
            hooks._record_reported_session({"session_id": "ses_payload"})
        self.assertEqual(state.kept_harness_session(fid), "ses_payload")

    def test_a_nested_claude_in_an_unwired_opencode_chat_records_nothing(self):
        """The scenario `$CHARTER_HARNESS` could not refuse. opencode's plugin is not wired,
        so its shim fails open and opencode has reported nothing; the operator runs `claude`
        in that pane. The nested harness inherits `CHARTER_HARNESS=opencode`, `$TMUX_PANE` and
        the chat's own `$CHARTER_SESSION_ID`, and reports a uuid of its own — which is not
        that variable, so it proves nothing and the link stays empty. Recorded, a reopen would
        run `opencode -s <claude uuid>`, which opencode refuses (O2): the chat would come back
        dead rather than empty."""
        fid = self._an_opencode_chat()
        env = {"CHARTER_HARNESS": "opencode", "CHARTER_SESSION_ID": fid,
               "CLAUDE_PID": "4242", "TMUX_PANE": "%4",
               "TMUX": f"{tmuxctl.socket_path('srv')},1,0"}
        with mock.patch.dict(os.environ, env, clear=True):
            hooks._record_reported_session(
                {"session_id": "e8962ccc-d263-4996-9881-c774dc586d3f",
                 "hook_event_name": "PreToolUse", "cwd": "/w", "tool_name": "Bash"})
        self.assertIsNone(state.kept_harness_session(fid))
        self.assertFalse(state.adopted(fid))

    def test_a_pane_that_is_not_a_pane_id_is_never_looked_up(self):
        """`$TMUX_PANE` is held to `tmuxctl.PANE_ID_RE` BEFORE it reaches `chat_in_pane` —
        asserted on the lookup rather than on the outcome, because a malformed pane matches
        no recorded one either way, so only the call says whether the shape was asked."""
        fid = self._an_opencode_chat()
        for pane in ("4", "%4;kill-server", "%", "%4 %5", ""):
            with self.subTest(pane=pane):
                with mock.patch.object(state, "chat_in_pane") as looked_up:
                    self._opencode("ses_abc", pane=pane)
                looked_up.assert_not_called()
                self.assertIsNone(state.kept_harness_session(fid))
        with mock.patch.object(state, "chat_in_pane", return_value=fid) as looked_up:
            self._opencode("ses_abc")
        looked_up.assert_called_once()

    def test_outside_a_plane_opencode_records_nothing(self):
        fid = self._an_opencode_chat()
        with mock.patch.object(hooks, "_in_a_plane", return_value=False):
            self._opencode("ses_abc")
        self.assertIsNone(state.kept_harness_session(fid))


class EveryToolHookHearsOpencodesReport(PlaneIso, unittest.TestCase):
    """opencode has no session-start event, so its first tool call is where its session is
    reported — through whichever of charter's tool hooks its shim routes that tool to."""

    HANDLERS = ("pretooluse", "pretooluse_read", "pretooluse_edit", "pretooluse_dispatch",
                "posttooluse", "posttooluse_bash", "posttooluse_skill",
                "posttooluse_message", "posttooluse_dispatch")

    def test_each_hands_its_payload_over(self):
        payload = {"hook_event_name": "PreToolUse", "session_id": "ses_abc", "cwd": str(self.tmp),
                   "tool_name": "Read", "tool_input": {"file_path": str(self.tmp / "x")},
                   "tool_response": {"output": ""}}
        for name in self.HANDLERS:
            with self.subTest(handler=name):
                seen: list[dict] = []
                with mock.patch.object(hooks, "_record_reported_session",
                                       side_effect=seen.append), \
                        mock.patch.dict(os.environ, {"PATH": os.environ.get("PATH", "")},
                                        clear=True):
                    run_hook(getattr(hooks, name), payload)
                self.assertEqual(seen, [payload])


class AReopenAsksForTheConversationBack(PersonaIso, unittest.TestCase):
    """`_reopen_one` asks the launcher for the conversation with `resume`, never by putting
    the harness's own flag in `rest` — the launcher spells it per harness, from the record."""

    def setUp(self) -> None:
        super().setUp()
        make_plane(self)
        wired_as_today(self)
        self.t = self.tmp / "t.jsonl"
        self.t.write_text("{}\n")
        self.inside = ws_mod.ensure("beta")
        self.elsewhere = self.tmp / "elsewhere"
        self.elsewhere.mkdir()
        self.launched: list = []

    def _reopen(self, **kw):
        c = _recorded(**{"cwd": str(self.inside), **kw})

        def launch(args):
            self.launched.append(args)
            args.reopening.fid = args.reopening.chat.chat
            return 0

        said: list[str] = []
        with mock.patch.object(commands_frame, "cmd_launch", side_effect=launch), \
                mock.patch.object(commands_frame.util, "warn", side_effect=said.append), \
                mock.patch.object(commands_frame.util, "info", side_effect=said.append):
            self.assertIsNotNone(commands_frame._reopen_one(c))
        (args,) = self.launched
        return args, " ".join(said)

    def test_a_reopen_hands_resume_to_the_launcher_not_rest(self):
        args, said = self._reopen(resume="u1", conversation=str(self.t))
        self.assertEqual((args.rest, args.resume), ([], True))
        self.assertIn(leave.RESUMES, said)

    def test_a_claude_chat_with_no_transcript_comes_back_empty(self):
        args, said = self._reopen(resume="u1", conversation=str(self.tmp / "never.jsonl"))
        self.assertEqual((args.rest, args.resume), ([], False))
        self.assertNotIn(leave.RESUMES, said)

    def test_a_harness_that_names_no_directory_resumes_wherever_it_comes_back(self):
        args, _said = self._reopen(resume="u1", conversation=str(self.t),
                                   cwd=str(self.elsewhere))
        self.assertTrue(args.resume)

    def test_opencode_resumes_in_its_recorded_directory(self):
        args, said = self._reopen(harness="opencode", resume="ses_x")
        self.assertTrue(args.resume)
        self.assertIn(leave.RESUMES, said)

    def test_an_opencode_chat_with_nothing_to_resume_is_not_told_it_moved(self):
        """The sentence is about a resume the move costs; a chat with no link loses none,
        and hearing that its conversation cannot be found elsewhere would be a false alarm."""
        args, said = self._reopen(harness="opencode", resume="", cwd=str(self.elsewhere))
        self.assertFalse(args.resume)
        self.assertNotIn("finds a conversation by", said)

    def test_opencode_resumes_only_in_its_recorded_directory(self):
        """O2, read from source: `opencode -s <id>` looks the id up in the working
        directory, so a chat a reopen moves into its workspace (#867) reopens empty — and
        says why."""
        args, said = self._reopen(harness="opencode", resume="ses_x", cwd=str(self.elsewhere))
        self.assertFalse(args.resume)
        self.assertIn("opencode", said)
        self.assertIn("reopens empty", said)
        self.assertNotIn(leave.RESUMES, said)


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
