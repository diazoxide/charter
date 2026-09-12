"""A chat keeps its profile through `+`, a workspace tab, a handoff, a quit and a reopen.

Wherever charter showed a chat's harness it shows its profile, and wherever a press had to
answer "which harness" without being told — the `+`, a tab, a chat handed off by another
chat — the answer is the profile the chat it was pressed from is running. `CHARTER_HARNESS`
stays the KIND underneath, because hooks compare it to `claude-code`.

**A reopen never substitutes** (ruling 15). A chat whose profile is gone is skipped with a
line naming it and stays in the manifest for a retry, rather than being given another: a
profile may be another account, where the chat's resume id does not exist and its
workspace's code was never meant to go. The `[harness] default` fallback that used to move
such a chat is deleted here, and the two tests that pinned it are rewritten.
"""

from __future__ import annotations

import io
import json
import os
import subprocess
import unittest
from contextlib import redirect_stderr
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame, config
from charter.frame import chats, choose, launcher, leave, reopen as reopen_state, state
from tests import _gitguard
from tests._isolation import PersonaIso, declare_profiles, make_plane


#: The real `subprocess.run`, captured before any fixture below patches the module
#: attribute — see `_APressInAlpha._tmux` for what that patch really reaches.
_REAL_RUN = subprocess.run


def _completed(cmd, rc=0, out=""):
    return subprocess.CompletedProcess(list(cmd), rc, out, "")


def _a_chat(fid: str, *, ws: str, profile: str | None = None,
            harness: str = "claude-code", pane: str | None = "%1") -> None:
    """A chat directory on this plane, in the shape a launcher leaves one.

    *harness* is `$CHARTER_HARNESS` as the launch records it — `harness.base.name`, not the
    CLI word — and *profile* the record this task adds beside it.
    """
    state.frame_dir(fid, create=True)
    state.record_workspace(fid, ws)
    state.record_server(fid, commands_frame.SOCKET)
    state.record_identity(fid, {"CHARTER_HARNESS": harness})
    if profile is not None:
        state.record_profile(fid, profile)
    if pane is not None:
        state.record_harness_pane(fid, pane)


class _APressInAlpha(PersonaIso):
    """One chat in `alpha`, a fake launcher and a fake tmux — `test_the_chat_bars_plus_makes
    _a_chat.py`'s own fixture, with a profile on the chat."""

    FID = "alpha.1"

    def setUp(self) -> None:
        super().setUp()
        make_plane(self)
        self.enterContext(mock.patch.dict(
            os.environ,
            {"PATH": os.environ.get("PATH", ""),
             "GIT_CEILING_DIRECTORIES": str(self.tmp.resolve().parent),
             "CHARTER_ROOT": str(config.ROOT), **_gitguard.environment()}, clear=True))
        # After the clearing patch above, so its `$HOME` and `$GIT_CEILING_DIRECTORIES`
        # survive: a declared profile's launch asks git whether this file is ignored.
        self.local = declare_profiles(self)
        for ws in ("alpha", "beta"):
            (config.WORKSPACES_DIR / ws).mkdir(parents=True, exist_ok=True)
        _a_chat(self.FID, ws="alpha", profile="claude-work")
        self.said = self.enterContext(
            mock.patch("charter.commands_frame._say_on_screen"))
        self.launched: list = []

    def _no_approval_needed(self) -> None:
        """Task 2 refuses every declared profile (review B1), and these cases are about
        which profile is CARRIED. The two that are about the refusal reaching the presser do
        not stand it down."""
        self.enterContext(mock.patch.object(launcher, "_approval_refusal",
                                            return_value=None))

    def _tmux(self, cmd, **kw):
        """Answer the two tmux questions a press asks, and let everything else through.

        **`commands_frame.subprocess` IS the `subprocess` module**, so patching
        `charter.commands_frame.subprocess.run` replaces that attribute for the whole
        process — `charter.util.run` included. A fake that answered every command would
        therefore answer the profile ignore check's `git status` with rc 0 and no output,
        which `util.git_path_state` reads as *git tracks this file*: every profile in it
        refused, in a fixture that set out to describe the opposite.
        """
        if not cmd or cmd[0] != "tmux":
            return _REAL_RUN(cmd, **kw)
        if "display-message" in cmd:
            if "window_width" in cmd[-1]:
                return _completed(cmd, 0, "132:43")
            return _completed(cmd, 0, "$1\t@1")
        if "list-panes" in cmd:
            return _completed(cmd, 0, f"$1\t%1\t{config.STATE_DIR}\n")
        return _completed(cmd, 0)

    def _press(self, *, rc: int = 0):
        def fake_launch(args):
            self.launched.append(args)
            return rc

        with mock.patch("charter.commands_frame.subprocess.run", side_effect=self._tmux), \
                mock.patch("charter.commands_frame.cmd_launch", side_effect=fake_launch):
            return commands_frame.cmd_new_chat(mock.Mock(chat=self.FID))

    def _sentences(self) -> list[str]:
        return [c[0][1] for c in self.said.call_args_list]


class ANewChatTakesThePressersProfile(_APressInAlpha, unittest.TestCase):
    def setUp(self) -> None:
        super().setUp()
        self._no_approval_needed()
        # **The profile's command is on `PATH`, said rather than inherited.** These cases
        # name `codex` as well as `claude`, and a press now runs the launcher's own checks
        # before it launches — so on a machine without codex installed (CI is one; the
        # machine this was written on is not) every one of them would be refused for a
        # reason none of them is about. `charter.commands_frame.shutil` is the `shutil`
        # module, so this is the answer the launcher's own check gets too.
        self.enterContext(mock.patch("charter.commands_frame.shutil.which",
                                     return_value="/nowhere/harness"))

    def test_the_plus_launches_the_profile_this_chat_records(self):
        self._press()
        self.assertEqual(len(self.launched), 1, self._sentences())
        self.assertEqual(self.launched[0].harness, "claude")
        self.assertEqual(self.launched[0].profile, "claude-work")

    def test_a_chat_from_before_profiles_takes_the_built_in_of_its_kind(self):
        _a_chat(self.FID, ws="alpha", harness="codex")
        (config.STATE_DIR / "frame" / self.FID / "profile").unlink(missing_ok=True)
        self._press()
        self.assertEqual(self.launched[0].profile, "codex")

    def test_a_declared_replacement_of_the_built_in_is_what_that_chat_gets(self):
        """A command that really resolves, because the launcher's PATH guard runs here for
        real: a replacement pinning a path this machine does not have is a different case
        (`…refused_before_tmux`), and it would pass this one for the wrong reason."""
        self.local.write_text('[harness.codex]\nkind = "codex"\n'
                              'command = ["/bin/sh"]\n')
        _a_chat(self.FID, ws="alpha", harness="codex")
        (config.STATE_DIR / "frame" / self.FID / "profile").unlink(missing_ok=True)
        self._press()
        self.assertEqual(self.launched[0].profile, "codex")
        from charter import profiles
        self.assertEqual(profiles.current().profiles["codex"].command, ("/bin/sh",))

    def test_a_chat_whose_profile_is_gone_is_refused_not_given_the_default(self):
        """Ruling 15's reason, one surface over: another profile may be another account."""
        _a_chat(self.FID, ws="alpha", profile="gone")
        self.local.write_text(self.local.read_text()
                              + '\n[harness]\ndefault = "claude"\n')
        self._press()
        self.assertEqual(self.launched, [])
        self.assertTrue(any("no longer declares" in s for s in self._sentences()),
                        self._sentences())

    def test_the_gone_profiles_name_is_repeated_back_escaped(self):
        """The record is a file under `.charter/`, not something charter minted this run —
        and this sentence goes to the frame, where an ESC would repaint the line around it.
        (A `\\r` cannot make the trip: `read_text` translates it to `\\n` on the way back,
        which is a fact about the record and not about the containment.)"""
        _a_chat(self.FID, ws="alpha", profile="go\x1bne")
        self._press()
        self.assertEqual(self.launched, [])
        self.assertTrue(any("\\u001b" in s for s in self._sentences()),
                        self._sentences())

    def test_a_chat_from_before_profiles_says_why_its_kinds_profile_was_refused(self):
        """Rung 2, and ruling 37 on the way down it: the built-in named after this chat's
        kind is one the file replaced and charter refused, so the press says THAT reason
        rather than running the command the operator replaced. Losing the reason here is
        the silent half — a `+` that does nothing, with nothing said about why."""
        self.local.write_text('[harness.codex]\nkind = "codex"\ncommand = "codex --x"\n')
        _a_chat(self.FID, ws="alpha", harness="codex")
        (config.STATE_DIR / "frame" / self.FID / "profile").unlink(missing_ok=True)
        self._press()
        self.assertEqual(self.launched, [])
        self.assertTrue(any("never a shell string" in s for s in self._sentences()),
                        self._sentences())

    def test_with_no_record_and_no_default_the_press_is_refused_by_name(self):
        _a_chat(self.FID, ws="alpha", harness="zzz-nothing")
        (config.STATE_DIR / "frame" / self.FID / "profile").unlink(missing_ok=True)
        self.local.write_text("")
        self._press()
        self.assertEqual(self.launched, [])
        self.assertTrue(any("no profile" in s for s in self._sentences()),
                        self._sentences())


class APressThatCannotLaunchSaysWhyOnTheFrame(_APressInAlpha, unittest.TestCase):
    """`cmd_new_chat` runs detached with its three streams on `/dev/null`, so a refusal
    `_launch` printed is read by nobody: without this the `+` would report only "the
    launcher returned 3" for a profile the operator could have fixed. Review B1 makes this
    reachable today — every declared profile is refused until Task 3."""

    def test_the_press_says_the_launchers_own_refusal(self):
        self._press()
        self.assertEqual(self.launched, [])
        self.assertTrue(any("cannot yet ask before a declared command runs" in s
                            for s in self._sentences()), self._sentences())


class AWorkspaceTabOpensTheSameProfile(_APressInAlpha, unittest.TestCase):
    def setUp(self) -> None:
        super().setUp()
        self._no_approval_needed()

    def _open(self):
        def fake_launch(args):
            self.launched.append(args)
            return 0

        with mock.patch("charter.commands_frame.subprocess.run", side_effect=self._tmux), \
                mock.patch("charter.commands_frame.cmd_launch", side_effect=fake_launch), \
                mock.patch.object(commands_frame, "_live_sessions", return_value=set()), \
                mock.patch.object(commands_frame, "_plane_session",
                                  return_value=("$1", "beta.1")), \
                mock.patch.object(commands_frame, "_window_size", return_value=(120, 40)):
            return commands_frame._open_workspace(self.FID, "beta",
                                                  socket=commands_frame.SOCKET,
                                                  window="@1")

    def test_a_workspace_tab_opens_the_profile_the_presser_is_running(self):
        self._open()
        self.assertEqual(len(self.launched), 1, self._sentences())
        self.assertEqual(self.launched[0].profile, "claude-work")
        self.assertEqual(self.launched[0].harness, "claude")


class AHandoffTakesTheCallingChatsProfile(_APressInAlpha, unittest.TestCase):
    def _refusal(self, ws: str = "beta") -> str:
        with mock.patch("charter.commands_frame.subprocess.run", side_effect=self._tmux), \
                mock.patch.object(commands_frame, "_live_sessions", return_value=set()), \
                mock.patch.object(commands_frame, "_plane_session",
                                  return_value=("$1", "beta.1")):
            return commands_frame.background_refusal(ws, caller=self.FID,
                                                     first_message="fix the widget please")

    def test_a_handoff_takes_the_calling_chats_profile(self):
        self._no_approval_needed()
        self.assertEqual(self._refusal(), "")
        with mock.patch("charter.commands_frame.subprocess.run", side_effect=self._tmux), \
                mock.patch.object(commands_frame, "_live_sessions", return_value=set()), \
                mock.patch.object(commands_frame, "_plane_session",
                                  return_value=("$1", "beta.1")), \
                mock.patch.object(commands_frame, "_window_size", return_value=(120, 40)), \
                mock.patch("charter.commands_frame.cmd_launch",
                           side_effect=lambda args: self.launched.append(args)):
            commands_frame.open_in_background("beta", caller=self.FID,
                                              first_message="fix the widget please")
        self.assertEqual(self.launched[0].profile, "claude-work")
        self.assertEqual(self.launched[0].harness, "claude")

    def test_a_handoff_to_a_declared_profile_refuses_before_anything_is_written(self):
        """Review B1: the refusal is the whole answer a handoff gets, before a chat
        directory, a window or a first message exists anywhere."""
        self.assertIn("cannot yet ask before a declared command runs", self._refusal())

    def test_a_profile_whose_kind_this_charter_cannot_place_is_refused_not_raised(self):
        """The guard `_same_harness_as` carried before profiles, restored.

        Every profile names a kind this charter registers, so nothing reachable produces
        this — but `_harness_of` is typed `| None`, and what is on the other side of a
        missing guard here is an `AttributeError` inside a process whose three streams are
        `/dev/null`: a handoff that answers nothing at all, to a caller waiting for a
        sentence. It fails closed, in the words this refusal already has."""
        # Task 2's own B1 refusal comes earlier in the chain and would answer this case for
        # a reason it is not about.
        self._no_approval_needed()
        with mock.patch.object(commands_frame, "_harness_of", return_value=None):
            said = self._refusal()
        self.assertIn("cannot open a chat in 'beta'", said)
        self.assertIn("no profile this charter can launch", said)

    def test_a_handoff_from_a_chat_whose_profile_is_gone_is_refused(self):
        _a_chat(self.FID, ws="alpha", profile="gone")
        self.assertIn("no longer declares", self._refusal())


class TheRecordCarriesTheProfile(PersonaIso, unittest.TestCase):
    """What a quit writes down, and what a reopen reads back. The key and the file name are
    spelled literally: a round trip through charter's own writer and reader cannot pin a
    name that both halves would rename together."""

    def setUp(self) -> None:
        super().setUp()
        make_plane(self)

    def _doomed(self, **kw):
        fields = dict(chat="alpha.1", workspace="alpha", persona="", harness="claude-code",
                      cwd="", resume="", server=commands_frame.SOCKET, live=True,
                      active=True, exit_code=None, closed=False, homeless=False,
                      cwd_gone=False, cwd_outside=False, profile="claude-work")
        return leave.Doomed(**{**fields, **kw})

    def test_a_quit_records_each_chats_profile(self):
        commands_frame._record_the_plane([self._doomed()], focus="alpha",
                                         active={"alpha.1"}, windows={}, capture=False)
        raw = json.loads((config.STATE_DIR / "frame" / "reopen.json").read_text())
        (chat,) = raw["frames"][0]["chats"]
        self.assertEqual(chat["profile"], "claude-work")

    def test_a_manifest_written_before_profiles_reads_as_no_profile(self):
        commands_frame._record_the_plane([self._doomed()], focus="alpha",
                                         active=set(), windows={}, capture=False)
        path = config.STATE_DIR / "frame" / "reopen.json"
        raw = json.loads(path.read_text())
        del raw["frames"][0]["chats"][0]["profile"]
        path.write_text(json.dumps(raw))
        self.assertEqual(reopen_state.read().all_chats()[0].profile, "")

    def test_a_profile_that_is_not_text_reads_as_no_profile(self):
        commands_frame._record_the_plane([self._doomed()], focus="alpha",
                                         active=set(), windows={}, capture=False)
        path = config.STATE_DIR / "frame" / "reopen.json"
        raw = json.loads(path.read_text())
        raw["frames"][0]["chats"][0]["profile"] = 3
        path.write_text(json.dumps(raw))
        self.assertEqual(reopen_state.read().all_chats()[0].profile, "")

    def test_a_quit_reads_the_profile_off_the_chat_it_is_stopping(self):
        _a_chat("alpha.1", ws="alpha", profile="claude-work")
        plan = leave.plan(live={"alpha.1"}, focus="alpha")
        self.assertEqual(plan.chats[0].profile, "claude-work")


class AReopenNeverSubstitutes(PersonaIso, unittest.TestCase):
    def setUp(self) -> None:
        super().setUp()
        make_plane(self)
        self.enterContext(mock.patch.dict(
            os.environ,
            {"PATH": os.environ.get("PATH", ""),
             "GIT_CEILING_DIRECTORIES": str(self.tmp.resolve().parent),
             "CHARTER_ROOT": str(config.ROOT), **_gitguard.environment()}, clear=True))
        self.local = declare_profiles(self)
        (config.WORKSPACES_DIR / "alpha").mkdir(parents=True, exist_ok=True)
        self.launched: list = []
        self.enterContext(mock.patch.object(launcher, "_approval_refusal",
                                            return_value=None))

    def _chat(self, **kw):
        fields = dict(chat="alpha.1", workspace="alpha", persona="", harness="claude-code",
                      cwd="", resume="", transcript="", active=True, profile="claude-work")
        return reopen_state.Chat(**{**fields, **kw})

    def _reopen(self, c):
        def fake_launch(args):
            self.launched.append(args)
            args.reopening.fid = "alpha.1"
            return 0

        err = io.StringIO()
        with redirect_stderr(err), \
                mock.patch("charter.commands_frame.cmd_launch", side_effect=fake_launch):
            r = commands_frame._reopen_one(c)
        return r, err.getvalue()

    def test_a_chat_comes_back_on_its_own_profile(self):
        r, _said = self._reopen(self._chat())
        self.assertIsNotNone(r)
        self.assertEqual(self.launched[0].profile, "claude-work")
        self.assertEqual(self.launched[0].harness, "claude")

    def test_a_chat_whose_profile_is_gone_is_skipped_with_its_name(self):
        r, said = self._reopen(self._chat(profile="gone"))
        self.assertIsNone(r)
        self.assertEqual(self.launched, [])
        self.assertIn("gone", said)
        self.assertIn("charter reopen", said)

    def test_a_chat_whose_profile_is_refused_says_the_refusal_rather_than_gone(self):
        """Declared and refused is not the same fact as no longer declared, and the two
        remedies differ: one is a line to fix, the other a profile to declare again."""
        (config.ROOT / "charter.local.toml").write_text(
            '[harness.claude-work]\nkind = "claude"\ncommand = "claude -x"\n')
        r, said = self._reopen(self._chat())
        self.assertIsNone(r)
        self.assertIn("never a shell string", said)
        self.assertNotIn("no longer declares", said)

    def test_a_chat_whose_kind_is_unregistered_is_skipped_not_moved_to_the_default(self):
        """This replaces the case that pinned the `[harness] default` fallback: a chat is
        never reopened under something nobody chose."""
        self.local.write_text(self.local.read_text()
                              + '\n[harness]\ndefault = "claude"\n')
        r, said = self._reopen(self._chat(harness="zzz", profile=""))
        self.assertIsNone(r)
        self.assertEqual(self.launched, [])
        self.assertNotIn("reopening it under", said)

    def test_a_chat_with_no_profile_to_name_asks_the_file_nothing(self):
        """A chat that never had a profile, whose kind nothing registers, has no NAME to
        resolve — so the file is not asked, and this chat is skipped under its own kind.

        `[harness.""]` is what makes that visible rather than merely tidy: TOML allows the
        empty key, charter refuses it for its shape, and the refusal is filed under the
        empty name. A resolve that ran anyway would find it and report somebody else's bad
        line as the reason this chat did not come back — sending the operator to fix a
        profile the manifest never mentions.
        """
        self.local.write_text('[harness.""]\nkind = "claude"\ncommand = ["claude"]\n')
        r, said = self._reopen(self._chat(harness="zzz", profile=""))
        self.assertIsNone(r)
        self.assertEqual(self.launched, [])
        self.assertIn("zzz", said)

    def test_a_chat_from_before_profiles_comes_back_on_its_kinds_built_in(self):
        r, _said = self._reopen(self._chat(profile=""))
        self.assertIsNotNone(r)
        self.assertEqual(self.launched[0].profile, "claude")

    def test_a_skipped_chat_is_still_recorded_for_a_retry(self):
        """`_consume` keeps what did not come back, so declaring the profile again and
        running `charter reopen` brings it back."""
        kept = self._chat(chat="alpha.2", profile="gone")
        m = reopen_state.Manifest(
            at=0, focus="alpha",
            frames=(reopen_state.Frame(workspace="alpha", chats=(self._chat(), kept)),))
        with redirect_stderr(io.StringIO()):
            commands_frame._consume(m, {"alpha.1"})
        left = reopen_state.read().all_chats()
        self.assertEqual([c.chat for c in left], ["alpha.2"])
        self.assertEqual(left[0].profile, "gone")


class TheTwoRecordsAChatsLauncherWrites(PersonaIso, unittest.TestCase):
    """`state.record_profile` and `state.record_launch` — what the pane writes down, read
    back by the launch that opened it and by every later `+`, tab, quit and reopen.

    **A chat id is not charter's to trust.** It reaches these four functions from
    `$CHARTER_SESSION_ID` as often as from `frame_id`, so `frame_dir` answers `None` for one
    that names no directory under the state root — and `None` has to be handled here rather
    than raised out of a hook that cannot afford it. A test that only ever passes a
    well-formed id measures none of that: the writes are `try`-wrapped and would look
    identical.
    """

    #: `contain.child` refuses it, so `frame_dir` has no directory to answer with. Written
    #: as the id an inherited `$CHARTER_SESSION_ID` could really carry.
    NOT_A_CHAT = "../elsewhere"

    def setUp(self) -> None:
        super().setUp()
        make_plane(self)

    def test_a_chat_id_that_names_no_directory_is_written_nowhere(self):
        state.record_profile(self.NOT_A_CHAT, "claude-work")
        state.record_launch(self.NOT_A_CHAT, 3, "refused")
        self.assertIsNone(state.profile(self.NOT_A_CHAT))
        self.assertIsNone(state.launch(self.NOT_A_CHAT))
        # And nothing was written beside the state root under that name either.
        self.assertEqual(list((config.STATE_DIR / "frame").glob("*elsewhere*")), [])

    def test_a_refusal_of_several_lines_comes_back_whole(self):
        """The record is `<code>\\n<text>`, and the text is a refusal that may carry its own
        newline — `press Enter to close this chat.` is a second line of one. Only the FIRST
        newline separates the two fields; every later one belongs to the sentence."""
        state.record_launch("alpha.1", 3, "profile 'x' is refused\n  press Enter.\n")
        self.assertEqual(state.launch("alpha.1"),
                         (3, "profile 'x' is refused\n  press Enter.\n"))

    def test_a_half_written_record_is_read_as_no_answer_yet(self):
        """The launcher writes this file while the launch that opened the chat is reading
        it, and a hand edit is always possible. A code that is not a number is not a
        verdict — and "not yet" is the reading that lets the launch carry on."""
        state.record_launch("alpha.1", 0, "")
        d = state.frame_dir("alpha.1")
        (d / "launch").write_text("not-a-number\nhalf a sentence")
        self.assertIsNone(state.launch("alpha.1"))

    def test_a_launch_that_has_not_answered_yet_says_so(self):
        state.frame_dir("alpha.1", create=True)
        self.assertIsNone(state.launch("alpha.1"))
        state.record_launch("alpha.1")
        self.assertEqual(state.launch("alpha.1"), (0, ""))


class WhereTheHarnessWasShownTheProfileIs(PersonaIso, unittest.TestCase):
    def setUp(self) -> None:
        super().setUp()
        make_plane(self)

    def _doomed(self, **kw):
        fields = dict(chat="alpha.1", workspace="alpha", persona="", harness="claude-code",
                      cwd="", resume="", server=commands_frame.SOCKET, live=True,
                      active=True, exit_code=None, closed=False, homeless=False,
                      cwd_gone=False, cwd_outside=False, profile="claude-work")
        return leave.Doomed(**{**fields, **kw})

    def test_a_quit_row_names_the_profile(self):
        self.assertEqual(leave.title(self._doomed()), "alpha.1 · claude-work")

    def test_a_chat_row_from_before_profiles_still_names_its_harness(self):
        self.assertEqual(leave.title(self._doomed(profile="")), "alpha.1 · claude-code")

    def test_a_chat_that_says_nothing_at_all_is_its_own_id(self):
        self.assertEqual(leave.title(self._doomed(profile="", harness="")), "alpha.1")

    def test_the_chat_pickers_note_names_the_profile(self):
        _a_chat("alpha.1", ws="alpha", profile="claude-work")
        self.assertEqual(choose._note(choose.CHAT, "alpha.1"), "claude-work")

    def test_a_chat_from_before_profiles_still_notes_its_harness(self):
        _a_chat("alpha.1", ws="alpha")
        self.assertEqual(choose._note(choose.CHAT, "alpha.1"), "claude-code")

    def test_the_profile_a_chat_records_is_what_chats_reports(self):
        _a_chat("alpha.1", ws="alpha", profile="claude-work")
        self.assertEqual(chats.profile_of("alpha.1"), "claude-work")
        self.assertEqual(chats.profile_of("alpha.2"), "")


if __name__ == "__main__":
    unittest.main()
