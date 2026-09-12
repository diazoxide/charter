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
from charter.frame import chats, choose, leave, reopen as reopen_state, state
from tests import _gitguard
from tests._isolation import (APipe as _APipe, PersonaIso,
                              approve_every_profile, approve_profile,
                              declare_profiles, make_plane, wired_as_today)


#: Ruling 10: a profile whose config folder does not carry charter's guard refuses to
#: launch, and that applies to the built-ins every launch test here starts. In-process the
#: suite's `claude` guard makes detection read UNKNOWN, so every one of them would refuse
#: over a fact none of them is about. One fixture for the module, because no test in it is
#: about wiring; `tests/test_a_profile_is_wired_or_refuses.py` is where that is the subject.
_WIRED = None


def setUpModule():
    global _WIRED
    _WIRED = wired_as_today()
    _WIRED.start()


def tearDownModule():
    if _WIRED is not None:
        _WIRED.stop()


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
        # **A press has no terminal**, stated rather than inherited (`tests/_ttyguard`).
        # `builtin_actions._spawn` runs `cmd_new_chat` detached with all three streams on
        # `/dev/null`, which is what decides that the one question a profile can raise is
        # deferred to the pane (`_the_pane_will_ask`) rather than asked here.
        self.enterContext(mock.patch("sys.stdin", _APipe()))
        self.said = self.enterContext(
            mock.patch("charter.commands_frame._say_on_screen"))
        self.launched: list = []

    def _no_approval_needed(self) -> None:
        """Seed a launch record for every profile this plane declares.

        These cases are about which profile is CARRIED by a press, a tab or a handoff, and
        a profile charter has never run asks its own question first (Task 3). The cases
        that are about that question do not seed one.
        """
        approve_every_profile(self)

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


class ANewChatStartsOnThePressersProfile(_APressInAlpha, unittest.TestCase):
    """**A `+` opens a chat at the selector, and the pressing chat's profile is the row it
    opens ON.** It used to launch that profile outright and refuse the press whenever it
    could not resolve one, which is a `+` that does nothing with a sentence on the
    attention row. The selector answers every one of those cases, so the rungs below are
    about which ROW the cursor lands on — and a press charter cannot answer that for still
    opens a chat.

    `_same_profile_as`'s own refusals are unchanged and still tested where they are decided
    (`tests/test_a_chat_carries_its_profile.AHandoffTakesTheCallingChatsProfile`, and the
    reopen cases below): a handoff still names its profile, because nobody is at that one.
    """

    def setUp(self) -> None:
        super().setUp()
        self._no_approval_needed()

    def test_the_plus_starts_on_the_profile_this_chat_records(self):
        self._press()
        self.assertEqual(len(self.launched), 1, self._sentences())
        self.assertTrue(self.launched[0].select)
        self.assertEqual(self.launched[0].start, "claude-work")

    def test_a_chat_from_before_profiles_starts_on_the_built_in_of_its_kind(self):
        _a_chat(self.FID, ws="alpha", harness="codex")
        (config.STATE_DIR / "frame" / self.FID / "profile").unlink(missing_ok=True)
        self._press()
        self.assertEqual(self.launched[0].start, "codex")

    def test_a_chat_whose_profile_is_gone_starts_on_no_row_rather_than_the_default(self):
        """Ruling 15's reason, one surface over: another profile may be another account, so
        a profile the plane no longer declares is never silently swapped for the default.
        What changed is that this is no longer a refusal — the chat opens, on no row, and
        the selector lists what this machine actually has."""
        _a_chat(self.FID, ws="alpha", profile="gone")
        self.local.write_text(self.local.read_text()
                              + '\n[harness]\ndefault = "claude"\n')
        self._press()
        self.assertEqual(len(self.launched), 1, self._sentences())
        self.assertEqual(self.launched[0].start, "")
        self.assertEqual(self._sentences(), [], self._sentences())

    def test_a_refused_replacement_of_a_built_in_starts_on_no_row(self):
        """Rung 2 and ruling 37: the built-in named after this chat's kind is one the file
        replaced and charter refused, so the press never falls back to the command the
        operator replaced. The row it would have opened on is simply not offered — and the
        selector lists that name refused, with the file's own reason on it."""
        self.local.write_text('[harness.codex]\nkind = "codex"\ncommand = "codex --x"\n')
        _a_chat(self.FID, ws="alpha", harness="codex")
        (config.STATE_DIR / "frame" / self.FID / "profile").unlink(missing_ok=True)
        self._press()
        self.assertEqual(self.launched[0].start, "")

    def test_with_no_record_and_no_default_the_press_still_opens_a_chat(self):
        _a_chat(self.FID, ws="alpha", harness="zzz-nothing")
        (config.STATE_DIR / "frame" / self.FID / "profile").unlink(missing_ok=True)
        self.local.write_text("")
        self._press()
        self.assertEqual(len(self.launched), 1, self._sentences())
        self.assertTrue(self.launched[0].select)
        self.assertEqual(self.launched[0].start, "")
        self.assertEqual(self._sentences(), [], self._sentences())


class APressRunsNoGuardForAProfileNobodyHasPicked(_APressInAlpha, unittest.TestCase):
    """The `+` used to run the launcher's whole refusal chain before it launched, so its
    reason could reach the frame's attention row rather than `/dev/null`. Nothing is being
    launched now: the pane runs that chain, fresh, for whichever profile is picked minutes
    from now — so asking here would be asking about a profile nobody has chosen.

    Review B1 is what makes this visible: every declared profile is refused until Task 3,
    and the press opens a chat regardless.
    """

    def test_the_press_opens_a_chat_over_a_profile_the_launcher_would_refuse(self):
        self._press()
        self.assertEqual(len(self.launched), 1, self._sentences())
        self.assertTrue(self.launched[0].select)
        self.assertNotIn("cannot yet ask", " ".join(self._sentences()))


class AWorkspaceTabStartsOnTheSameProfile(_APressInAlpha, unittest.TestCase):
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

    def test_a_workspace_tab_starts_on_the_profile_the_presser_is_running(self):
        self._open()
        self.assertEqual(len(self.launched), 1, self._sentences())
        self.assertTrue(self.launched[0].select)
        self.assertEqual(self.launched[0].start, "claude-work")


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

    def test_a_handoff_to_an_unapproved_profile_refuses_before_anything_is_written(self):
        """Nobody is at a chat a handoff opens, so the question cannot be put and the
        refusal is the whole answer — before a chat directory, a window or a first message
        exists anywhere. It names the one command that would approve it."""
        said = self._refusal()
        self.assertIn("nobody is at this open to approve it", said)
        self.assertIn("charter claude-work", said)

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

    def test_the_gone_profiles_name_is_repeated_back_escaped(self):
        """Ruling 35. The record is a file under `.charter/`, not something charter minted
        this run, and this sentence reaches a model's tool result — where an ESC would
        repaint the line around it. (A `\\r` cannot make the trip: `read_text` translates it
        to `\\n` on the way back, which is a fact about the record rather than about the
        containment.)

        Asked HERE rather than at the `+`, and that is where it moved to: a press no longer
        surfaces this sentence at all, because it opens the selector instead of refusing.
        A handoff still names its profile — nobody is at that open to be asked."""
        _a_chat(self.FID, ws="alpha", profile="go\x1bne")
        said = self._refusal()
        self.assertIn("\\u001b", said)
        self.assertNotIn("\x1b", said)


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
        # What an operator who has already run this profile once leaves behind: these cases
        # are about which profile comes back, not about the question a new one asks.
        approve_profile(self)

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

    def test_a_record_that_cannot_be_written_is_not_raised_out_of_a_hook(self):
        """**A disk that is full is not this function's to report.** Both writers are
        reached from a launcher mid-`exec` and from hook paths that have nowhere to print,
        and the caller's own answer to "the record is missing" is already "not yet" — which
        is exactly what a write that failed leaves behind. So the failure is swallowed
        HERE, and the reader above is what says so."""
        state.frame_dir("alpha.1", create=True)
        with mock.patch.object(state.config, "replace_for",
                               side_effect=OSError(28, "No space left on device")):
            state.record_profile("alpha.1", "claude-work")
            state.record_launch("alpha.1", 3, "refused")
        self.assertIsNone(state.profile("alpha.1"))
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
