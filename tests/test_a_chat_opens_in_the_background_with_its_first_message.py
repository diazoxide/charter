"""A chat opens in a named workspace in the background, already told its first message.

This is the seam `charter handoff` (chat-handoff plan, Task 2) drives: `open_in_background`
runs the ordinary launcher — `cmd_launch` with `attach=False`, the `_open_workspace` and
`cmd_new_chat` precedent — carrying an `Opening`, `Reopening`'s sibling for a chat nobody
attaches to. Three promises, each pinned here:

* **Everything it refuses, it refuses before it starts anything** (`background_refusal`), so
  Task 2 can ask the same question before any write of its own.
* **Nobody moves.** The new window is not selected and the chat the operator is on keeps its
  panels; nothing attaches.
* **A pin the CALLING chat carries does not reach the new one.** #936 made a framed chat's
  launch record its workspace lock, but only for an unpinned launcher: a non-empty
  `$CHARTER_WORKSPACE` in the launching process wins that rung instead. The calling chat is
  another chat, so its pins are emptied for the length of the launch and put back exactly.

Real tmux — that the window really does not move, and that the whole message arrives — is
`tests/test_a_background_chat_really_starts_on_its_brief.py`.
"""

from __future__ import annotations

import os
import subprocess
import unittest
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame, config, persona, workspace
from charter.frame import state
from charter.harness import claude_code

from tests import _tmuxsocket
from tests._isolation import PersonaIso


def _completed(cmd, rc=0, out=""):
    return subprocess.CompletedProcess(list(cmd), rc, out, "")


def _a_chat(fid: str, *, ws: str, pane: str | None = "%1", harness: str = "claude-code",
            socket: str | None = None) -> None:
    """A chat directory on THIS plane, in the shape a launcher leaves one — the same helper
    `tests/test_the_chat_bars_plus_makes_a_chat.py` keeps, for the same reason."""
    state.frame_dir(fid, create=True)
    state.record_workspace(fid, ws)
    state.record_server(fid, socket or commands_frame.SOCKET)
    state.record_identity(fid, {"CHARTER_HARNESS": harness})
    if pane is not None:
        state.record_harness_pane(fid, pane)


class _AChatInAlpha(PersonaIso, unittest.TestCase):
    """`alpha.1` on charter's own server, a `beta` workspace on disk, a stand-in tmux and a
    stand-in launcher that records how it was called and what it could see."""

    CALLER = "alpha.1"

    def setUp(self):
        super().setUp()
        self.enterContext(mock.patch.dict(os.environ, {}, clear=True))
        for ws in ("alpha", "beta"):
            (config.WORKSPACES_DIR / ws).mkdir(parents=True, exist_ok=True)
        _a_chat(self.CALLER, ws="alpha")
        #: Session names `list-sessions` reports.
        self.sessions: set[str] = set()
        #: A live `beta` harness pane `list-panes` reports as this plane's, or ``None``.
        self.beta_seat: str | None = None
        #: What the stand-in launcher returns, which chat it says it made, and whether that
        #: chat recorded a harness pane.
        self.rc = 0
        self.gives_back = "beta.1"
        self.records_pane = True
        self.launched: list = []
        self.seen: list[dict] = []
        #: Every tmux command the open asked, in order.
        self.calls: list[list[str]] = []

    def _tmux(self, cmd, **kw):
        self.calls.append(list(cmd))
        if "display-message" in cmd:
            if "window_width" in cmd[-1]:
                return _completed(cmd, 0, "132:43")
            return _completed(cmd, 0, "$1\t@1")
        if "list-panes" in cmd:
            rows = [f"$1\t%1\t{config.STATE_DIR}"]
            if self.beta_seat:
                rows.append(f"$2\t{self.beta_seat}\t{config.STATE_DIR}")
            return _completed(cmd, 0, "".join(r + "\n" for r in rows))
        if "list-sessions" in cmd:
            return _completed(cmd, 0, "".join(s + "\n" for s in sorted(self.sessions)))
        return _completed(cmd, 0)

    def _fake_launch(self, args):
        self.launched.append(args)
        self.seen.append({"cwd": os.getcwd(),
                          "CHARTER_WORKSPACE": os.environ.get("CHARTER_WORKSPACE"),
                          "CHARTER_PERSONA": os.environ.get("CHARTER_PERSONA")})
        if self.gives_back:
            _a_chat(self.gives_back, ws="beta", pane="%9" if self.records_pane else None)
            args.opening.fid = self.gives_back
        return self.rc

    def _open(self, ws="beta", text="fix the widget please", persona=""):
        with mock.patch("charter.commands_frame.subprocess.run", side_effect=self._tmux), \
                mock.patch("charter.commands_frame.cmd_launch",
                           side_effect=self._fake_launch):
            return commands_frame.open_in_background(ws, caller=self.CALLER,
                                                     first_message=text, persona=persona)

    def _refusal(self, ws="beta", text="fix the widget please"):
        with mock.patch("charter.commands_frame.subprocess.run", side_effect=self._tmux):
            return commands_frame.background_refusal(ws, caller=self.CALLER,
                                                     first_message=text)


class TheOpenRunsTheLauncherForTheNamedWorkspace(_AChatInAlpha):

    def test_the_launch_names_the_workspace_and_never_attaches(self):
        self._open()
        self.assertEqual(len(self.launched), 1)
        launched = self.launched[0]
        self.assertEqual(launched.workspace, "beta")
        self.assertIs(launched.attach, False)
        self.assertIs(launched.pick, False)
        self.assertIs(launched.no_frame, False)

    def test_the_first_message_rides_the_harnesses_own_argv(self):
        for name, rest in (("claude-code", ["fix the widget please"]),
                           ("opencode", ["--prompt", "fix the widget please"]),
                           ("codex", ["fix the widget please"])):
            with self.subTest(harness=name):
                self.launched.clear()
                state.record_identity(self.CALLER, {"CHARTER_HARNESS": name})
                self._open()
                self.assertEqual(self.launched[0].rest, rest)

    def test_the_opening_carries_the_new_chat_id_back(self):
        self.assertEqual(self._open(), commands_frame.Opened(True, "beta.1", ""))

    def test_the_opening_carries_the_message_and_the_persona_it_was_given(self):
        self._open(text="fix the widget please", persona="forge")
        opening = self.launched[0].opening
        self.assertIsInstance(opening, commands_frame.Opening)
        self.assertEqual(opening.first_message, "fix the widget please")
        self.assertEqual(opening.persona, "forge")

    def test_it_is_sized_for_the_window_the_calling_chat_is_on(self):
        self._open()
        self.assertEqual(self.launched[0].size, (132, 43))

    def test_a_caller_with_no_recorded_pane_is_sized_off_the_target_workspaces_live_chat(self):
        """`cmd_new_chat`'s second reading: with the calling chat's own pane not on record,
        the window is measured off the pane `_plane_session` proves is live in the target."""
        (state.frame_dir(self.CALLER) / "harness").unlink()
        self.beta_seat = "%5"
        _a_chat("beta.2", ws="beta", pane="%5")
        self.sessions = {"beta"}
        self._open()
        self.assertEqual(self.launched[0].size, (132, 43))
        self.assertEqual([c[c.index("-t") + 1] for c in self.calls if "display-message" in c],
                         ["%5"])

    def test_with_no_pane_to_measure_it_aims_no_target_and_hands_no_size(self):
        """Neither pane is on record, so no `-t` is aimed at all — an empty target resolves
        to the tmux server's CURRENT window, very likely another plane's — and the launcher
        is handed ``None``, which `_launch_size` reads as "measure your own terminal"."""
        (state.frame_dir(self.CALLER) / "harness").unlink()
        self._open()
        self.assertIsNone(self.launched[0].size)
        self.assertEqual([c for c in self.calls if "display-message" in c], [])

    def test_a_directory_it_cannot_return_to_does_not_lose_the_chat_it_opened(self):
        """The launch ran in the target's directory, and this process could not change back
        (the directory it started in was removed meanwhile, say). The chat is open, and
        reporting that it is not would be the one wrong answer left — `cmd_new_chat`'s own
        arrangement."""
        was = os.getcwd()
        self.addCleanup(os.chdir, was)
        real = os.chdir
        target = os.path.realpath(workspace.workspace_dir("beta"))

        def chdir(path):
            if os.path.realpath(path) != target:
                raise OSError("the directory it started in is gone")
            return real(path)

        with mock.patch("charter.commands_frame.os.chdir", side_effect=chdir):
            got = self._open()
        self.assertEqual(got, commands_frame.Opened(True, "beta.1", ""))

    def test_the_launch_runs_in_the_target_workspaces_own_directory(self):
        was = os.getcwd()
        self._open()
        self.assertEqual(os.path.realpath(self.seen[0]["cwd"]),
                         os.path.realpath(workspace.workspace_dir("beta")))
        self.assertEqual(os.getcwd(), was)

    def test_a_pin_the_calling_chat_carries_does_not_reach_the_new_chat(self):
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "alpha",
                                          "CHARTER_PERSONA": "forge"}):
            self._open()
            self.assertEqual(self.seen[0]["CHARTER_WORKSPACE"], "")
            self.assertEqual(self.seen[0]["CHARTER_PERSONA"], "")
            self.assertEqual(os.environ["CHARTER_WORKSPACE"], "alpha")
            self.assertEqual(os.environ["CHARTER_PERSONA"], "forge")

    def test_a_name_the_calling_chat_did_not_have_is_absent_again_afterwards(self):
        self._open()
        self.assertEqual(self.seen[0]["CHARTER_WORKSPACE"], "")
        self.assertEqual(self.seen[0]["CHARTER_PERSONA"], "")
        self.assertNotIn("CHARTER_WORKSPACE", os.environ)
        self.assertNotIn("CHARTER_PERSONA", os.environ)


class EveryRefusalComesBeforeAnythingStarts(_AChatInAlpha):

    def test_an_empty_first_message_is_refused(self):
        got = self._open(text="  \n")
        self.assertIs(got.ok, False)
        self.assertIn("empty first message", got.message)
        self.assertEqual(self.launched, [])

    def test_a_first_message_starting_with_a_dash_is_refused(self):
        got = self._open(text="--dangerously-skip-permissions now")
        self.assertIn("starts with `-`", got.message)
        self.assertEqual(self.launched, [])

    def test_a_single_word_first_message_is_refused(self):
        got = self._open(text="login")
        self.assertIn("single word 'login'", got.message)
        self.assertEqual(self.launched, [])

    def test_a_single_word_is_named_without_the_whitespace_around_it(self):
        self.assertIn("single word 'login'", self._refusal(text="  login\n"))

    def test_the_word_it_names_cannot_forge_a_line_or_a_colour(self):
        """The word is the caller's own text, echoed into a sentence an operator reads, so
        an escape sequence in it arrives escaped (`contain.one_line`)."""
        said = self._refusal(text="log\x1b[31min")
        self.assertNotIn("\x1b", said)
        self.assertIn("log\\x1b[31min", said)

    def test_a_nul_byte_is_refused(self):
        got = self._open(text="fix\x00 it")
        self.assertIn("NUL byte", got.message)
        self.assertEqual(self.launched, [])

    def test_a_first_message_past_the_bound_is_refused(self):
        bound = commands_frame.FIRST_MESSAGE_MAX_BYTES
        got = self._open(text="x " * (bound // 2 + 1))
        self.assertIn("past", got.message)
        self.assertIn(str(bound), got.message)
        self.assertEqual(self.launched, [])

    def test_a_first_message_exactly_at_the_bound_is_taken(self):
        """Pins `>` against `>=`."""
        bound = commands_frame.FIRST_MESSAGE_MAX_BYTES
        text = "x" * (bound - 2) + " y"
        self.assertEqual(len(text.encode()), bound)
        got = self._open(text=text)
        self.assertIs(got.ok, True, got.message)
        self.assertEqual(len(self.launched), 1)

    def test_the_bound_is_the_ruled_twelve_thousand_two_hundred_and_eighty_eight_bytes(self):
        """The ruled number, spelled by hand rather than read off the constant. Every other
        case here reads `FIRST_MESSAGE_MAX_BYTES` and follows whatever value it holds — the
        deletion sweep retuned it and they all stayed green — while `docs/frame.md` and the
        refusal promise 12,288."""
        taken = self._open(text="x" * 12286 + " y")
        self.assertIs(taken.ok, True, taken.message)
        self.assertIn("12289-byte", self._refusal(text="x" * 12287 + " y"))

    def test_a_first_message_is_measured_in_bytes_not_characters(self):
        bound = commands_frame.FIRST_MESSAGE_MAX_BYTES
        text = "é " * (bound // 3 + 1)
        self.assertLess(len(text), bound)
        got = self._open(text=text)
        self.assertIs(got.ok, False)
        self.assertEqual(self.launched, [])

    def test_a_byte_that_is_not_utf8_is_counted_not_crashed_on(self):
        """A first message can carry a surrogate escape (a byte that is not UTF-8, as
        `sys.argv` or a surrogateescape stdin hands it over). The bound counts the bytes
        `exec` is handed; a strict `str.encode` would raise instead of answering (#959)."""
        self.assertEqual(self._refusal(text="fix \udcff it"), "")

    def test_a_chat_in_a_tmux_you_already_had_is_refused(self):
        state.record_server(self.CALLER, _tmuxsocket.OPERATOR_SOCKET)
        got = self._open()
        self.assertIn("a tmux you already had", got.message)
        self.assertEqual(self.launched, [])

    def test_a_chat_with_no_launchable_harness_is_refused(self):
        state.record_identity(self.CALLER, {"CHARTER_HARNESS": ""})
        with mock.patch.object(config, "HARNESS", None):
            got = self._open()
        self.assertIn(commands_frame.NO_HARNESS, got.message)
        self.assertEqual(self.launched, [])

    def test_a_harness_charter_has_not_measured_is_refused(self):
        with mock.patch.object(claude_code.ClaudeCodeHarness, "first_message_argv",
                               return_value=None):
            got = self._open()
        self.assertIn("has not measured how claude-code", got.message)
        self.assertEqual(self.launched, [])

    def test_a_session_this_plane_cannot_prove_is_its_own_is_refused(self):
        self.sessions = {"beta"}
        got = self._open()
        self.assertIn("probably another plane's", got.message)
        self.assertEqual(self.launched, [])

    def test_a_session_this_plane_can_prove_is_its_own_is_joined(self):
        self.beta_seat = "%5"
        _a_chat("beta.2", ws="beta", pane="%5")
        self.sessions = {"beta"}
        got = self._open()
        self.assertIs(got.ok, True, got.message)

    def test_a_workspace_directory_charter_cannot_enter_is_refused(self):
        real = os.chdir
        target = os.path.realpath(workspace.workspace_dir("beta"))

        def chdir(path):
            if os.path.realpath(path) == target:
                raise OSError("permission denied")
            return real(path)

        with mock.patch("charter.commands_frame.os.chdir", side_effect=chdir):
            got = self._open()
        self.assertIn("cannot enter its directory", got.message)
        self.assertEqual(self.launched, [])

    def test_a_launcher_that_fails_is_said(self):
        self.rc, self.gives_back = 1, ""
        got = self._open()
        self.assertIs(got.ok, False)
        self.assertIn("returned 1", got.message)

    def test_a_launcher_that_answers_zero_with_no_chat_is_not_success(self):
        """The chat id is the only proof a chat exists; a 0 without one proves nothing."""
        self.gives_back = ""
        self.assertIs(self._open().ok, False)

    def test_a_chat_whose_harness_died_on_start_is_not_success(self):
        """`_launch` records the harness pane the moment tmux reports one; a harness that
        then dies in its first moments leaves that record behind while the launcher returns
        its exit code (the early-death path). A recorded pane proves a chat was started, not
        that it is running, so the return code is asked as well."""
        self.rc = 1
        got = self._open()
        self.assertIs(got.ok, False)
        self.assertIn("returned 1", got.message)

    def test_a_chat_id_with_no_harness_pane_is_not_success(self):
        self.records_pane = False
        self.assertIs(self._open().ok, False)

    def test_the_refusal_check_writes_nothing(self):
        """Task 2 asks this before any write of its own, so it must make none itself."""
        self.sessions = {"beta"}

        def listings():
            return (sorted(os.listdir(state._root())),
                    {ws: sorted(os.listdir(config.WORKSPACES_DIR / ws))
                     for ws in sorted(os.listdir(config.WORKSPACES_DIR))})

        before = listings()
        self.assertIn("probably another plane's", self._refusal())
        self.assertEqual(listings(), before)

    def test_each_refusal_says_a_different_thing(self):
        bound = commands_frame.FIRST_MESSAGE_MAX_BYTES
        said = [self._refusal(text=t) for t in (
            "  \n", "--dangerously-skip-permissions now", "login", "fix\x00 it",
            "x " * (bound // 2 + 1))]
        state.record_server(self.CALLER, _tmuxsocket.OPERATOR_SOCKET)
        said.append(self._refusal())
        state.record_server(self.CALLER, commands_frame.SOCKET)
        state.record_identity(self.CALLER, {"CHARTER_HARNESS": ""})
        with mock.patch.object(config, "HARNESS", None):
            said.append(self._refusal())
        state.record_identity(self.CALLER, {"CHARTER_HARNESS": "claude-code"})
        with mock.patch.object(claude_code.ClaudeCodeHarness, "first_message_argv",
                               return_value=None):
            said.append(self._refusal())
        self.sessions = {"beta"}
        said.append(self._refusal())
        self.assertTrue(all(said), said)
        self.assertEqual(len(set(said)), 9, said)


class OnlyAnOpeningIsReadAsOne(unittest.TestCase):
    """`_opening`'s `isinstance`, for `_reopening`'s reason. `args` is whatever namespace a
    caller built: one that happens to carry an unrelated `opening` attribute, or a `Mock`
    that answers every attribute, must not be read as a background open — whose "persona"
    would then be handed to `persona.set_active`."""

    def test_an_unrelated_opening_attribute_is_not_an_opening(self):
        self.assertIsNone(commands_frame._opening(SimpleNamespace(opening="beta")))

    def test_a_mock_that_answers_every_attribute_is_not_an_opening(self):
        self.assertIsNone(commands_frame._opening(mock.Mock()))

    def test_an_opening_is_read_as_itself(self):
        opening = commands_frame.Opening("fix it please")
        self.assertIs(commands_frame._opening(SimpleNamespace(opening=opening)), opening)


class TheLaunchOpensWithoutMovingAnyone(PersonaIso, unittest.TestCase):
    """The real `_launch`, with tmux and everything that would start a process stood in."""

    def setUp(self):
        super().setUp()
        self.enterContext(mock.patch.dict(os.environ, {}, clear=True))
        self.make_persona("forge")
        self.argvs: list[list[str]] = []
        #: ``(argv, the brief on disk for beta.1 at that moment)`` per tmux call, so a test
        #: can ask what existed BEFORE the window that starts the harness was created.
        self.brief_at: list[tuple[list[str], str | None]] = []

    def _launch(self, *, opening=None, attach=False, reopening=None):
        args = SimpleNamespace(harness="claude", rest=["fix it please"], no_frame=False,
                               workspace="beta", pick=False, size=(120, 40))
        if attach is False:
            args.attach = False
        if opening is not None:
            args.opening = opening
        if reopening is not None:
            args.reopening = reopening

        def run(action, argv, **kw):
            self.argvs.append(list(argv))
            self.brief_at.append((list(argv), state.brief("beta.1")))
            return subprocess.CompletedProcess(
                argv, 0, "%9\n" if "new-window" in argv else "", "")

        def write_all(joint, writes, **kw):
            return [subprocess.CompletedProcess(w.argv, 0, "", "") for w in writes]

        with mock.patch("charter.commands_frame.shutil.which",
                        return_value="/nowhere/claude"), \
                mock.patch("sys.stdout.isatty", return_value=True), \
                mock.patch("sys.stdin.isatty", return_value=False), \
                mock.patch.object(commands_frame.tmuxctl, "version", return_value=(3, 7)), \
                mock.patch.object(commands_frame.tmuxctl, "operator_server",
                                  return_value=None), \
                mock.patch.object(commands_frame, "_live_sessions", return_value={"beta"}), \
                mock.patch.object(commands_frame, "_live_chats", return_value=set()), \
                mock.patch.object(commands_frame.tmuxctl, "run", side_effect=run), \
                mock.patch.object(commands_frame.tmuxctl, "write_all",
                                  side_effect=write_all), \
                mock.patch.object(commands_frame.tmuxctl, "interact",
                                  return_value=subprocess.CompletedProcess([], 0)) as interact, \
                mock.patch.object(commands_frame, "_query_pane_dead_status",
                                  return_value=None), \
                mock.patch.object(commands_frame, "_draw_panels", return_value={}) as panels, \
                mock.patch.object(commands_frame, "_arm_panel_respawn"), \
                mock.patch.object(commands_frame, "_spawn_gather"), \
                mock.patch.object(commands_frame, "_chat_being_left",
                                  return_value="beta.0") as left, \
                mock.patch.object(commands_frame, "_drop_panels") as drop, \
                mock.patch.object(commands_frame.state, "reap"):
            rc = commands_frame._launch(args)
        self.interact, self.left, self.drop, self.panels = interact, left, drop, panels
        return rc

    def test_an_opening_selects_no_window(self):
        self._launch(opening=commands_frame.Opening("fix it please"))
        self.assertFalse([a for a in self.argvs if "select-window" in a], self.argvs)

    def test_an_opening_drops_no_one_elses_panels(self):
        self._launch(opening=commands_frame.Opening("fix it please"))
        self.drop.assert_not_called()
        self.left.assert_not_called()

    def test_an_opening_never_attaches(self):
        self._launch(opening=commands_frame.Opening("fix it please"))
        self.interact.assert_not_called()

    def test_an_opening_still_gets_its_panels_before_anyone_looks(self):
        """Open question 15, as ruled: the new window gets its panel processes the way every
        chat a reopen builds does. Only the select and the teardown are skipped, so a later
        gate that also skipped the panels would leave a chat nobody has looked at yet with
        none."""
        self._launch(opening=commands_frame.Opening("fix it please"))
        self.panels.assert_called_once()

    def test_an_opening_learns_the_chat_id_the_launcher_allocated(self):
        opening = commands_frame.Opening("fix it please")
        self._launch(opening=opening)
        self.assertEqual(opening.fid, "beta.1")
        self.assertTrue((config.STATE_DIR / "frame" / "beta.1").is_dir())

    def test_an_opening_writes_the_persona_it_was_given_under_the_new_chat(self):
        self._launch(opening=commands_frame.Opening("fix it please", persona="forge"))
        self.assertEqual(persona.for_session("beta.1"), "forge")

    def test_an_opening_with_no_persona_writes_no_pointer(self):
        self._launch(opening=commands_frame.Opening("fix it please"))
        self.assertIsNone(persona.for_session("beta.1"))

    def test_an_openings_brief_is_on_disk_before_the_window_that_starts_the_harness(self):
        """The brief is what the new chat exists to read, and `charter reopen`'s SessionStart
        block (Task 5) reads it as the harness starts. Written by the CALLER after
        `cmd_launch` returned, it appeared only once the harness was already running — a race
        that reproduces on nobody's machine. So it is written where the persona pointer is,
        at id allocation, and the window that starts the harness comes after."""
        self._launch(opening=commands_frame.Opening("fix it please", brief="fix it\nplease\n"))
        made = [(argv, brief) for argv, brief in self.brief_at if "new-window" in argv]
        self.assertTrue(made, self.argvs)
        self.assertEqual(made[0][1], "fix it\nplease\n")

    def test_an_opening_with_no_brief_leaves_no_brief_file(self):
        self._launch(opening=commands_frame.Opening("fix it please"))
        self.assertIsNone(state.brief("beta.1"))
        self.assertFalse((config.STATE_DIR / "frame" / "beta.1" / "brief").exists())

    def test_an_ordinary_launch_still_selects_its_window(self):
        """The gate is an `and`, not a replacement: a launch with no opening, whose `attach`
        is left absent, is still the operator's terminal and still lands on its chat."""
        self._launch(attach=None)
        self.assertTrue([a for a in self.argvs if "select-window" in a], self.argvs)

    def test_a_reopen_still_selects_no_window_and_drops_no_panels(self):
        """The `_reopening` half of the gate this change re-spelled — the deletion sweep
        showed nothing pinned it. A reopen builds several chats with nobody attached: a
        select would move a client that does not exist yet, and a teardown would strip the
        panels off the sibling the same reopen had just drawn (`_reopening`)."""
        recorded = SimpleNamespace(chat="beta.9", persona="")
        self._launch(reopening=commands_frame.Reopening(recorded), attach=None)
        self.assertFalse([a for a in self.argvs if "select-window" in a], self.argvs)
        self.drop.assert_not_called()


if __name__ == "__main__":
    unittest.main()
