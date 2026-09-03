"""`charter frame-new-chat` — what the chat bar's `+` runs, and the four ways it stops.

*"`+` button not working for creating new session."*

The affordance was a SENTENCE — `+ charter <harness> opens another` — which is true, which
names the command that does it, and which sits at the end of a row of clickable tabs
beginning with a `+`. Every terminal an operator has used puts a `+` there and every one of
them means *new*, so it was pressed. `slots.ADD_CHAT` is a `+` now and this is the command
behind it.

**Why a command at all, rather than the panel spawning `charter <harness>`.**
`builtin_actions._spawn` hands its child all three streams on `/dev/null`, and `cmd_launch`
reads a non-tty stdout as *this process cannot be the operator's terminal* and `os.execvp`s
the bare harness into the void — which is not a wrong frame, it is no frame at all and no
process left to report it. `attach=False` is the seam that says *build the frame, do not
become the terminal*; it has existed since `_open_workspace` needed it for §4k, and until
now it was reachable only in-process. `charter frame-new-chat` is the spelling.

**Every stop puts a line on the frame's own attention row**, and that is the whole point of
the command existing rather than the panel calling the launcher directly: this runs
detached with its streams on `/dev/null`, so a refusal it did not say is a `+` that did
nothing — which is the complaint this change is answering, one release later.

`tests/test_a_click_on_a_tab_bar_switches.APressOnThePlusMakesAChat` is which press starts
this; `tests/test_a_real_click_on_a_real_tab_bar_switches.py` is the real tmux that says a
chat really appears.
"""

from __future__ import annotations

import os
import subprocess
import unittest
from unittest import mock

from charter import commands_frame, config
from charter.frame import state

from tests._isolation import PersonaIso


def _completed(cmd, rc=0, out=""):
    return subprocess.CompletedProcess(list(cmd), rc, out, "")


def _a_chat(fid: str, *, ws: str, pane: str | None = "%1",
            harness: str = "claude-code", socket: str | None = None) -> None:
    """A chat directory on THIS plane, in the shape a launcher leaves one.

    *harness* is `$CHARTER_HARNESS` as `cmd_launch` records it — `harness.base.name`, not
    the CLI word — because that is the record `cmd_new_chat` reads to decide which harness
    the new chat runs. `tests/test_a_workspace_tab_opens_what_it_names.py` keeps the same
    helper for the same reason, one noun over.
    """
    state.frame_dir(fid, create=True)
    state.record_workspace(fid, ws)
    state.record_server(fid, socket or commands_frame.SOCKET)
    state.record_identity(fid, {"CHARTER_HARNESS": harness})
    if pane is not None:
        state.record_harness_pane(fid, pane)


class _APlusOnAFrameInAlpha(PersonaIso, unittest.TestCase):
    """One chat in `alpha`, on charter's own server, with a fake launcher and a fake tmux.

    The tmux stand-in answers the two questions `cmd_new_chat` asks before it launches —
    where this plane's `alpha` session is (`_plane_session`, through `list-panes`) and how
    big the window is — and nothing else. What is being asked here is which namespace the
    launcher is handed and which sentences the refusals say; whether tmux really makes a
    window is `tests/test_a_real_click_on_a_real_tab_bar_switches.py`'s.
    """

    FID = "alpha.1"

    def setUp(self):
        super().setUp()
        self.enterContext(mock.patch.dict(os.environ, {}, clear=True))
        (config.WORKSPACES_DIR / "alpha").mkdir(parents=True, exist_ok=True)
        _a_chat(self.FID, ws="alpha")
        self.said = self.enterContext(
            mock.patch("charter.commands_frame._say_on_screen"))
        self.launched: list = []
        self.calls: list[list[str]] = []

    def _tmux(self, cmd, **kw):
        self.calls.append(list(cmd))
        if "display-message" in cmd:
            # `_WINDOW_SIZE_FORMAT`'s own separator — a colon, not a space — read off the
            # constant rather than re-spelled, because a stand-in that answered in the
            # wrong shape would silently hand the launcher `_FALLBACK_SIZE` and the case
            # about the size would be measuring this fixture's typo.
            if "window_width" in cmd[-1]:
                return _completed(cmd, 0, "132:43")
            return _completed(cmd, 0, "$1\t@1")
        if "list-panes" in cmd:
            return _completed(cmd, 0, f"$1\t%1\t{config.STATE_DIR}\n")
        return _completed(cmd, 0)

    def _press(self, *, chat: str | None = None, rc: int = 0):
        """Run the command the `+` runs, with the launcher faked."""
        def fake_launch(args):
            self.launched.append(args)
            return rc

        with mock.patch("charter.commands_frame.subprocess.run", side_effect=self._tmux), \
                mock.patch("charter.commands_frame.cmd_launch", side_effect=fake_launch):
            return commands_frame.cmd_new_chat(
                mock.Mock(chat=self.FID if chat is None else chat))

    def _sentences(self) -> list[str]:
        return [c[0][1] for c in self.said.call_args_list]


class ThePressRunsTheLauncherForThisWorkspace(_APlusOnAFrameInAlpha):
    """The ordinary path: one launch, in this workspace, not attaching."""

    def test_a_press_launches_exactly_once(self):
        self.assertEqual(self._press(), 0)
        self.assertEqual(len(self.launched), 1,
                         f"the press started {len(self.launched)} launches")
        self.assertEqual(self._sentences(), [],
                         f"a successful press said something: {self._sentences()}")

    def test_the_launch_names_the_workspace_this_chat_is_in(self):
        """Outright, never a resolution. This runs detached in a process whose environment
        is a panel's, and `workspace.resolve()` there would answer for whatever that
        process happened to inherit — the trap `state.record_identity` measures, one field
        over. §4j is the other half: a chat belongs to its workspace for life, so there is
        no other workspace this press could mean."""
        self._press()
        self.assertEqual(self.launched[0].workspace, "alpha")

    def test_the_launch_does_not_attach(self):
        """**The seam this command exists on the far side of.** Left at the `getattr`
        default this reaches `bypass`, which `os.execvp`s the bare harness over this
        process with its output on `/dev/null` — no frame, no chat, and nothing left to
        say so."""
        self._press()
        self.assertIs(self.launched[0].attach, False)

    def test_the_launch_carries_no_command_and_asks_no_picker(self):
        """`rest` empty so the harness starts at its own prompt with nothing sent to it,
        and `pick` false so `_picker_wanted` can never raise a prompt on a path with no
        operator waiting — `_open_workspace`'s two fields for its two reasons."""
        self._press()
        self.assertEqual(self.launched[0].rest, [])
        self.assertFalse(self.launched[0].pick)
        self.assertFalse(self.launched[0].no_frame)

    def test_the_launch_uses_the_harness_this_chat_records(self):
        """The one question a `+` cannot carry, answered the only way the operator has
        expressed: the tool they are already in."""
        self._press()
        self.assertEqual(self.launched[0].harness, "claude")

    def test_a_chat_with_no_recorded_harness_falls_back_to_the_planes_default(self):
        """The migration case — a chat launched by a charter that predates
        `state.record_identity`."""
        state.record_identity(self.FID, {"CHARTER_HARNESS": ""})
        with mock.patch.dict(config.HARNESS, {"default": "claude"}):
            self._press()
        self.assertEqual(self.launched[0].harness, "claude")

    def test_the_launch_is_sized_for_the_window_the_chat_is_on(self):
        """A launcher with no terminal of its own measures nothing, and `cmd_launch`'s own
        fallback is 80x24 — which `_drawable_slots` reads as room for almost nothing. The
        new chat is about to be shown on the terminal looking at THIS chat, so that window
        is what it is sized for (`_launch_size`)."""
        self._press()
        self.assertEqual(self.launched[0].size, (132, 43))

    def test_no_tmux_target_is_ever_the_empty_string(self):
        """**An empty `-t` is not a failed read — it is the SERVER's current window**, and
        on a socket serving eleven sessions from three projects that is very likely another
        plane's. `_open_workspace` carried a `state.harness_pane(fid) or ""` into
        `_window_size`, the deletion sweep survived the `or ""` because that caller had
        already proved a pane, and asking why is what found this.

        Asked of every argv this command sends rather than of the one line that could get
        it wrong, so a second target added later is covered on the day it is added — and
        asked in the state that can REACH it. With this chat's pane recorded, no fallback
        runs and the check is vacuous; the row that matters is the chat whose pane record
        is gone while a SIBLING's is not — the migration case, and the one an `or ""` would
        send to the server's current window. A chat with no sibling either never gets that
        far: `_plane_session` has nothing to match and refuses before it asks tmux
        anything, which is why the sibling is planted rather than assumed.
        """
        _a_chat("alpha.9", ws="alpha", pane="%1")
        for pane in ("%1", ""):
            with self.subTest(recorded=pane or "<none>"):
                state.record_harness_pane(self.FID, pane)
                self.calls.clear()
                self._press()
                self.assertTrue(self.calls, "the press sent tmux nothing at all")
                for cmd in self.calls:
                    self.assertNotIn("", cmd[1:],
                                     f"an empty argument reached tmux: {cmd}")

    def test_a_chat_with_no_pane_of_its_own_is_sized_off_the_live_one(self):
        """**Reachable, unlike the fallback it replaces**: a chat launched by a charter
        that predates `state.record_harness_pane`, or one whose state directory was
        truncated, has no pane. What it is sized for is then the pane `_plane_session` has
        just proved is live in this workspace — the window a client in this workspace is
        looking at, which is `_open_workspace`'s own answer to the same question."""
        _a_chat("alpha.9", ws="alpha", pane="%1")
        state.record_harness_pane(self.FID, "")
        self._press()
        self.assertEqual(self.launched[0].size, (132, 43))
        self.assertTrue(any("%1" in cmd for cmd in self.calls),
                        f"nothing was measured off the live pane: {self.calls}")

    def test_a_workspace_whose_panes_are_all_gone_measures_nothing_at_all(self):
        """The end of the chain: no pane anywhere means charter has no window to lay the
        new frame out for, and `_launch_size` reads `None` as *measure your own terminal*
        — which for a detached process is the documented 80x24. A frame charter could not
        measure comes up small; one measured off a stranger's terminal comes up wrong, and
        only one of those is recoverable by resizing the window."""
        with mock.patch("charter.frame.chats.pane_of", return_value=None):
            self._press()
        self.assertIsNone(self.launched[0].size)
        self.assertFalse(any("display-message" in cmd and "window_width" in cmd[-1]
                             for cmd in self.calls),
                         f"a window was measured with no pane to measure it from: "
                         f"{self.calls}")

    def test_a_workspace_with_no_directory_yet_is_made_before_the_chat_goes_in_it(self):
        """**A real state, not a defensive one.** `switch.workspaces` folds
        `config.DEFAULT_WORKSPACE` into its list whether or not its directory exists —
        so a plane that has never made one still draws a tab for it and still offers a
        `+` in it.

        It used to run the launcher from `config.ROOT`, and the chat then recorded
        `workspace = alpha` beside `cwd = <plane root>` — a disagreement created at the
        moment of launch, which also put the chat outside the directory #850's harness
        layer lives in. `_launch_root` calls `workspace.ensure`, so the boundary exists
        before a chat is put inside it.
        """
        seen: list[str] = []

        def fake_launch(args):
            seen.append(os.getcwd())
            self.launched.append(args)
            return 0

        (config.WORKSPACES_DIR / "alpha").rmdir()
        with mock.patch("charter.commands_frame.subprocess.run", side_effect=self._tmux), \
                mock.patch("charter.commands_frame.cmd_launch", side_effect=fake_launch):
            commands_frame.cmd_new_chat(mock.Mock(chat=self.FID))
        self.assertEqual(seen, [str((config.WORKSPACES_DIR / "alpha").resolve())],
                         "the launch did not run in the workspace the tab names")
        self.assertTrue((config.WORKSPACES_DIR / "alpha").is_dir())

    def test_the_launch_runs_in_the_workspaces_own_directory(self):
        """`cmd_launch` reads `os.getcwd()` for the frame's cwd, and a panel process is
        standing wherever its pane was started. Restored in a `finally` — this process goes
        on to draw — so the reading has to be taken from inside the launcher."""
        seen: list[str] = []

        def fake_launch(args):
            seen.append(os.getcwd())
            self.launched.append(args)
            return 0

        here = os.getcwd()
        with mock.patch("charter.commands_frame.subprocess.run", side_effect=self._tmux), \
                mock.patch("charter.commands_frame.cmd_launch", side_effect=fake_launch):
            commands_frame.cmd_new_chat(mock.Mock(chat=self.FID))
        self.assertEqual(seen, [str((config.WORKSPACES_DIR / "alpha").resolve())])
        self.assertEqual(os.getcwd(), here, "the launcher was left in another directory")

    def test_it_answers_zero_even_when_the_launcher_did_not(self):
        """`cmd_palette`'s rule: a non-zero from a `run-shell` child is printed INTO THE
        HARNESS PANE and drops it into copy-mode — charter drawing in the one rectangle
        ADR 0018 says it never draws. So every outcome here is 0 and the report goes on
        the attention row."""
        self.assertEqual(self._press(rc=2), 0)
        self.assertTrue(any("2" in s for s in self._sentences()),
                        f"a launcher failure was swallowed: {self._sentences()}")


class ThePressSaysWhyWhenItWillNotMakeAChat(_APlusOnAFrameInAlpha):
    """The four stops, each with its own sentence. A `+` that silently failed would be the
    same complaint this change is answering, one release later."""

    def test_a_frame_in_a_tmux_you_already_had_is_refused_by_name(self):
        """On charter's own server a workspace IS a session and a chat is a window in it.
        Inside a tmux the operator already has, charter writes nothing session-scoped at
        all and a chat there is a window in a session charter does not own — so the `+`
        names the command that works there instead of guessing.

        The socket is a bare absolute path rather than a real one, because
        `tmuxctl.is_operator_socket` discriminates on the leading `/` and
        `tests/test_no_test_bakes_a_uid_into_a_socket_path.py` refuses a `tmux-<uid>`
        spelt into a test file."""
        state.record_server(self.FID, "/nowhere/someone-elses")
        self.assertEqual(self._press(), 0)
        self.assertEqual(self.launched, [], "a chat was made in somebody else's tmux")
        # **The sentence spelled by hand, not read off the constant.** A case built from
        # the constant it is about agrees with any value that constant takes — the
        # survivor the deletion sweep reported for exactly this line. What the operator has
        # to be told is *that charter will not do it here* and *what does work there*, so
        # both halves are written out.
        said = self._sentences()
        self.assertEqual(len(said), 1, said)
        self.assertIn("window in a tmux you already had", said[0])
        self.assertIn("charter <harness>", said[0],
                      "the refusal does not name the command that DOES work there")
        self.assertEqual(said, [commands_frame.NO_CHAT_HERE],
                         "the constant and the sentence have come apart")

    def test_a_session_this_plane_cannot_prove_is_its_own_is_refused(self):
        """**§3.3 at a new door.** One tmux server serves every plane on this machine, and
        `cmd_launch` decides between starting a session and joining one on a NAME. A `+`
        that ran the launcher without this check could add a window to another project's
        live frame, across every isolation boundary charter has."""
        with mock.patch("charter.commands_frame._plane_session", return_value=None):
            self.assertEqual(self._press(), 0)
        self.assertEqual(self.launched, [])
        # Spelled by hand for the case above's reason. The load-bearing half is *cannot
        # prove* — charter is not saying the session is somebody else's, it is saying it
        # cannot show it is this plane's, and those are different claims.
        said = self._sentences()
        self.assertEqual(len(said), 1, said)
        self.assertIn("cannot prove", said[0])
        self.assertIn("another plane's", said[0])
        self.assertEqual(said, [commands_frame.NO_SESSION_HERE],
                         "the constant and the sentence have come apart")

    def test_a_chat_recording_no_launchable_harness_is_refused_by_name(self):
        with mock.patch.dict(config.HARNESS, {"default": "nothing-installed"}):
            state.record_identity(self.FID, {"CHARTER_HARNESS": "also-nothing"})
            self.assertEqual(self._press(), 0)
        self.assertEqual(self.launched, [])
        said = self._sentences()
        self.assertEqual(len(said), 1, said)
        self.assertIn("records no harness this charter can launch", said[0])
        self.assertIn("[harness] default", said[0],
                      "the refusal does not name the key that would fix it")
        self.assertTrue(said[0].startswith("cannot open another chat:"),
                        f"the refusal does not name its own subject: {said[0]!r}")

    def test_a_workspace_directory_charter_cannot_enter_is_refused_by_name(self):
        with mock.patch("charter.commands_frame.os.chdir", side_effect=OSError("nope")):
            self.assertEqual(self._press(), 0)
        self.assertEqual(self.launched, [])
        said = self._sentences()
        self.assertEqual(len(said), 1, said)
        self.assertEqual(said[0], "cannot open another chat: charter cannot enter this "
                                  "workspace's directory")

    def test_a_cwd_that_vanished_while_the_launcher_ran_costs_nothing(self):
        """**The `except OSError` on the way BACK, which the deletion sweep asked about.**

        The launcher is run from the workspace's directory and this process is put back
        where it was in a `finally`. That restore can fail for the same reasons the first
        `chdir` can — the panel's own cwd removed while the launch was in flight — and it
        happens after the work is done, so it must not turn a chat that WAS made into a
        traceback in a process whose streams are `/dev/null`.

        The second call is the one that raises, so the launch really runs; the first is the
        one that must not, or this would be measuring the refusal above.
        """
        calls = {"n": 0}
        real = os.chdir
        # **The restore this case breaks is the one that puts THIS process back**, so the
        # case has to do it instead — otherwise the interpreter is left standing in the
        # workspace directory and every later case in this file resolves its plane from
        # there. Registered before the failure is arranged, so it runs whatever happens.
        self.addCleanup(real, os.getcwd())

        def flaky(path):
            calls["n"] += 1
            if calls["n"] > 1:
                raise OSError("the directory this panel was standing in is gone")
            real(path)

        with mock.patch("charter.commands_frame.os.chdir", side_effect=flaky):
            self.assertEqual(self._press(), 0)
        self.assertEqual(calls["n"], 2, "the restore was never attempted")
        self.assertEqual(len(self.launched), 1,
                         "the launch did not happen, so this measures nothing")
        self.assertEqual(self._sentences(), [],
                         f"a failed restore was reported as a failure: "
                         f"{self._sentences()}")

    def test_no_frame_at_all_is_a_message_on_stderr_and_nothing_else(self):
        """Typed by hand outside a frame. There is no attention row to draw on — that is
        what "no frame" means — so this one goes to the stream a shell can read, and still
        answers 0 for `cmd_palette`'s reason."""
        with mock.patch("charter.util.err") as err:
            self.assertEqual(self._press(chat=""), 0)
        self.assertEqual(self.launched, [])
        self.assertEqual(self._sentences(), [])
        self.assertEqual(err.call_count, 1)

    def test_each_refusal_says_a_different_thing(self):
        """The set, asked as a set. Four stops sharing one sentence would tell the operator
        that the `+` does not work and nothing about why — which is the state before this
        change wearing a message."""
        said = {commands_frame.NO_CHAT_HERE, commands_frame.NO_SESSION_HERE}
        self.assertEqual(len(said), 2)
        for text in said:
            self.assertNotIn("\n", text, f"a refusal spans two rows: {text!r}")


class TheCommandIsReachableAndReserved(unittest.TestCase):
    """The CLI half — a word charter owns, parsed to this function."""

    def test_the_word_parses_to_the_command(self):
        from charter import cli
        args = cli.build_parser().parse_args(["frame-new-chat", "--chat", "api.1"])
        self.assertIs(args.func, commands_frame.cmd_new_chat)
        self.assertEqual(args.chat, "api.1")

    def test_the_chat_option_is_optional(self):
        """A bind installed by an older charter fires with no `--chat` at all, and a
        hand-typed `charter frame-new-chat` inside a frame has none either — both resolve
        through `$CHARTER_SESSION_ID`, which is `_pressers_chat`'s fallback."""
        from charter import cli
        self.assertEqual(cli.build_parser().parse_args(["frame-new-chat"]).chat, "")

    def test_it_takes_no_name_to_get_wrong(self):
        """#518's line held. A chat's id is allocated and its workspace is fixed for life
        (§4j), so there is nothing here for an operator to type — which is also why the
        workspace bar has no `+`: a workspace IS a name, and creating one on a typo leaves
        litter."""
        from charter import cli
        with self.assertRaises(SystemExit):
            cli.build_parser().parse_args(["frame-new-chat", "somename"])

    def test_a_harness_cannot_claim_the_word(self):
        """`cli._core_commands` reserves it, and the reservation is asked for by BEHAVIOUR
        rather than by reading the source: a harness whose `cli_name` is this word must be
        refused when the parser is built.

        Without the reservation nothing raises — on this repo's 3.11 floor a second
        `add_parser` of the same name silently replaces the first — so the `+` would launch
        a harness instead of making a chat, and every other case in this file would still
        pass.
        """
        from charter import cli
        from charter.harness import base
        rogue = mock.Mock(spec=base.Harness, name="rogue")
        rogue.name = "rogue"
        rogue.cli_name = "frame-new-chat"
        with mock.patch("charter.harness.all", return_value=[rogue]):
            with self.assertRaises(ValueError) as caught:
                cli.build_parser()
        self.assertIn("frame-new-chat", str(caught.exception))
