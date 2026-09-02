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
        self.assertEqual(self._sentences(), [commands_frame.NO_CHAT_HERE])

    def test_a_session_this_plane_cannot_prove_is_its_own_is_refused(self):
        """**§3.3 at a new door.** One tmux server serves every plane on this machine, and
        `cmd_launch` decides between starting a session and joining one on a NAME. A `+`
        that ran the launcher without this check could add a window to another project's
        live frame, across every isolation boundary charter has."""
        with mock.patch("charter.commands_frame._plane_session", return_value=None):
            self.assertEqual(self._press(), 0)
        self.assertEqual(self.launched, [])
        self.assertEqual(self._sentences(), [commands_frame.NO_SESSION_HERE])

    def test_a_chat_recording_no_launchable_harness_is_refused_by_name(self):
        with mock.patch.dict(config.HARNESS, {"default": "nothing-installed"}):
            state.record_identity(self.FID, {"CHARTER_HARNESS": "also-nothing"})
            self.assertEqual(self._press(), 0)
        self.assertEqual(self.launched, [])
        self.assertEqual(len(self._sentences()), 1)
        self.assertIn("harness", self._sentences()[0])

    def test_a_workspace_directory_charter_cannot_enter_is_refused_by_name(self):
        with mock.patch("charter.commands_frame.os.chdir", side_effect=OSError("nope")):
            self.assertEqual(self._press(), 0)
        self.assertEqual(self.launched, [])
        self.assertEqual(len(self._sentences()), 1)
        self.assertIn("cannot enter", self._sentences()[0])

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
