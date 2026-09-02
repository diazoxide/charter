"""A workspace tab for a workspace that is not open yet OPENS it, and takes you there.

The operator's report, which is the whole requirement:

> *"when I want to switch to another workspace it should switch and keep my current
> sessions open in background, but I can't switch — I get `charter: workspace 'default'
> is not open on this plane — open it with charter <harness> --workspace default`"*

Thirteen of their fifteen workspace tabs did nothing. #793 shipped the half where the
workspace is already open (`switch-client`, and nothing dies); this is the other half.

**The refusal it replaces gave a reason, and the reason was measured to be false.** It
read: *"Opening one is `cmd_launch` — a directory, an ordinal, a harness process and an
`attach` — and this runs detached with its streams on `/dev/null`
(`builtin_actions._spawn`), with no terminal to attach anything to."* A detached process
with no controlling terminal creates a tmux session, splits panes and sets session options
perfectly well; what needs a terminal is the `attach`, and the way to arrive somewhere
without one is `switch-client`, which is what #793 already does. Measured on tmux 3.7c and
at the 3.2 floor, identically — :class:`ADetachedProcessCanBuildAWorkspace` is that
measurement, and it is the foundation the rest of this module stands on.

**What actually needed a terminal was somewhere else entirely**, and this is the finding
that matters for anybody reading the old comment: `cmd_launch`'s second guard is ``if
args.no_frame or not sys.stdout.isatty(): return bypass(argv)``, and `bypass` **execs**.
Reached from a detached switch it would have replaced the switching process with a bare
harness on `/dev/null` — no frame, no session, no switch, and nothing on screen to say so.
:class:`TheLauncherCanBuildAFrameWithNoTerminalOfItsOwn` pins that gate open only for a
launch that was never going to be a terminal anyway.

**#518 is not what the old comment said it was.** #518 asks that `charter <harness>` stop
resolving a workspace *silently*, and its "creating is not free" paragraph is about
`charter workspace create` making a **workspace directory** on a name the operator TYPED:
*"a picker that creates on a typo leaves litter, so the create path needs confirmation or
a validation pass."* A tab click types nothing. The name comes off a list charter itself
drew from `switch.workspaces()` — every directory already under `workspaces/` — so there
is no typo to create on and no workspace is created at all. What a click makes is a
**chat** in a workspace that already exists, which is precisely what §4k
(*"if `foo` is live, attach to it; if not, open it and leave the others"*) says
`charter -w foo` does, and precisely what the old refusal told the operator to go and type
by hand. :class:`NoWorkspaceIsEverCreatedByAClick` pins the half of #518 that does apply.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame, config
from charter.frame import state

from tests import _tmuxreap, _ttyguard
from tests._isolation import PersonaIso, no_background_refresh

_HAS_TMUX = shutil.which("tmux") is not None

#: What `_switch_client` used to say instead of opening. Spelled by hand rather than
#: imported, because its ABSENCE is what several cases here assert and a constant read out
#: of the module under test would follow a reworded sentence into a green run.
_OLD_REFUSAL = "is not open on this plane"


def _completed(cmd, rc=0, out=""):
    return subprocess.CompletedProcess(list(cmd), rc, out, "")


def _a_chat(fid: str, *, ws: str, pane: str | None, harness="claude-code") -> None:
    """A chat directory on THIS plane, in the shape a launcher leaves one.

    *harness* is `$CHARTER_HARNESS` as `cmd_launch` records it — `harness.base.name`, not
    the CLI word — because that is the record :func:`_open_workspace` reads to decide what
    to open the next workspace with. ``""`` is a chat launched by a charter that predates
    `state.record_identity`.
    """
    state.frame_dir(fid, create=True)
    state.record_workspace(fid, ws)
    state.record_server(fid, commands_frame.SOCKET)
    state.record_identity(fid, {"CHARTER_HARNESS": harness})
    if pane is not None:
        state.record_harness_pane(fid, pane)


class _Server:
    """The tmux a switch reads, with the target workspace appearing only once opened.

    *opened_pane* is the pane id `cmd_launch` is pretended to have created. Until the
    fake launcher runs, `list-panes` reports only this plane's `alpha` chat — which is
    exactly the state the old refusal fired on.
    """

    def __init__(self, *, size="132:43", clients=("/dev/ttys001",)):
        self.size = size
        self.clients = list(clients)
        self.opened: list[str] = []
        self.calls: list[list[str]] = []
        self.switched: list[tuple[str, str]] = []

    def _seats(self) -> str:
        rows = [f"$1\t%1\t{config.STATE_DIR}"]
        rows += [f"$2\t{p}\t{config.STATE_DIR}" for p in self.opened]
        return "".join(r + "\n" for r in rows)

    def __call__(self, cmd, **kw):
        self.calls.append(list(cmd))
        if "display-message" in cmd:
            fmt = cmd[-1]
            if "window_width" in fmt:
                return _completed(cmd, 0, self.size)
            return _completed(cmd, 0, "$1\t@1")
        if "list-panes" in cmd:
            return _completed(cmd, 0, self._seats())
        if "list-clients" in cmd:
            target = cmd[cmd.index("-t") + 1]
            moved = [c for c, t in self.switched if t == target]
            return _completed(cmd, 0, "".join(c + "\n" for c in
                                              (moved if target == "$2"
                                               else [c for c in self.clients
                                                     if c not in moved])))
        if "switch-client" in cmd:
            self.switched.append((cmd[cmd.index("-c") + 1], cmd[cmd.index("-t") + 1]))
            return _completed(cmd, 0)
        if "list-windows" in cmd:
            return _completed(cmd, 0, "$2\t1\tbeta.1\n")
        return _completed(cmd, 0)


class _OpensBeta(PersonaIso, unittest.TestCase):
    """A frame in `alpha`, a `beta` that exists on disk and is not open, and a fake
    launcher that records how it was called."""

    FID = "alpha.1"

    def setUp(self):
        super().setUp()
        self.enterContext(mock.patch.dict(os.environ, {}, clear=True))
        for n in ("alpha", "beta"):
            (config.WORKSPACES_DIR / n).mkdir(parents=True, exist_ok=True)
        _a_chat(self.FID, ws="alpha", pane="%1")
        self.said = self.enterContext(
            mock.patch("charter.commands_frame._say_on_screen"))
        self.enterContext(mock.patch("charter.commands_frame._apply_arrangement"))
        self.enterContext(
            mock.patch("charter.commands_frame._relayout_target", return_value=None))
        self.launched: list = []

    def _run(self, server=None, *, opens=True):
        server = server or _Server()

        def fake_launch(args):
            self.launched.append(args)
            if opens:
                _a_chat("beta.1", ws="beta", pane="%9")
                server.opened.append("%9")
            return 0

        with mock.patch("charter.commands_frame.subprocess.run", side_effect=server), \
                mock.patch("charter.commands_frame.cmd_launch",
                           side_effect=fake_launch):
            commands_frame._switch_client(self.FID, "beta", said="workspace → beta")
        return server


class TheTabOpensTheWorkspaceItNames(_OpensBeta):
    """The report, answered. A tab for a workspace with no session builds one and moves
    the terminal onto it, instead of printing a command for the operator to type."""

    def test_a_workspace_with_no_session_is_opened_instead_of_refused(self):
        self._run()
        self.assertEqual(len(self.launched), 1,
                         "the click did not open the workspace it named")
        for call in self.said.call_args_list:
            self.assertNotIn(_OLD_REFUSAL, call[0][1])

    def test_the_launch_names_the_workspace_that_was_clicked(self):
        """`--workspace` outright, never a resolution: the launcher runs detached in a
        process whose cwd is the chat the operator clicked FROM, and `workspace.resolve`
        would answer `alpha` from it."""
        self._run()
        self.assertEqual(self.launched[0].workspace, "beta")

    def test_the_launch_does_not_attach(self):
        """The whole seam. `attach` is what needs a terminal and there is none; arriving
        is `switch-client`, which needs none."""
        self._run()
        self.assertFalse(commands_frame._wants_attach(self.launched[0]))

    def test_the_launch_carries_no_command_of_its_own(self):
        """`rest` empty, so `cmd_launch`'s §4k open-or-focus gate stays reachable and the
        harness starts at its own prompt with nothing sent to it."""
        self._run()
        self.assertEqual(list(self.launched[0].rest), [])

    def test_the_client_is_moved_onto_the_session_the_open_created(self):
        s = self._run()
        self.assertEqual(s.switched, [("/dev/ttys001", "$2")])

    def test_the_operator_is_told_they_arrived(self):
        self._run()
        self.assertTrue(self.said.called)
        self.assertEqual(self.said.call_args[0][1], "workspace → beta")

    def test_an_open_that_starts_no_session_moves_nothing_and_says_so(self):
        """A launch can fail — a harness that is not installed, a state directory that
        cannot be made. The switch must then leave this chat exactly as it was rather
        than report a move that did not happen (#411's shape through a success)."""
        s = self._run(opens=False)
        self.assertEqual(len(self.launched), 1, "it never tried to open")
        self.assertEqual(s.switched, [])
        said = self.said.call_args[0][1]
        self.assertIn("beta", said)
        self.assertIn("could not open", said)


class TheOpenIsSizedForTheTerminalThatWillSeeIt(_OpensBeta):
    """A detached launcher cannot measure a terminal, and the number decides the frame.

    `os.get_terminal_size()` raises `OSError` in a process with its streams on
    `/dev/null` — measured on both tmux versions — so `cmd_launch` would fall back to
    `_FALLBACK_SIZE` (80x24) and `_drawable_slots` would drop every panel a real terminal
    has room for. The size that matters is the size of the client about to be switched,
    and that client is looking at THIS chat's window right now, so tmux already knows it.
    """

    def test_the_launch_is_given_the_size_of_the_window_the_client_is_on(self):
        self._run(_Server(size="132:43"))
        self.assertEqual(self.launched[0].size, (132, 43))

    def test_it_is_not_the_eighty_by_twenty_four_fallback(self):
        """Pinned as its own case because 80x24 is what a silent failure looks like: a
        frame that came up with one panel where the operator has room for four."""
        self._run(_Server(size="200:60"))
        self.assertNotEqual(self.launched[0].size, commands_frame._FALLBACK_SIZE)
        self.assertEqual(self.launched[0].size, (200, 60))


class NoWorkspaceIsEverCreatedByAClick(_OpensBeta):
    """#518's rule, kept where it actually applies.

    #518 is about `charter workspace create` making a directory on a name somebody typed
    wrong. A tab carries a name charter drew from the directories that already exist, so
    the click cannot name one that is not there — and this asserts the listing is
    unchanged rather than trusting that argument.
    """

    def test_the_set_of_workspaces_is_the_same_after_a_click_as_before(self):
        before = sorted(p.name for p in config.WORKSPACES_DIR.iterdir())
        self._run()
        self.assertEqual(sorted(p.name for p in config.WORKSPACES_DIR.iterdir()),
                         before)

    def test_no_pointer_is_written_for_a_workspace_nobody_picked(self):
        """`_pin_workspace` writes only when the operator PICKED at the launcher's own
        prompt (#518: a launch that resolved silently must write no pointer). A click is
        not that prompt, so `picked` must stay false."""
        self._run()
        self.assertFalse(getattr(self.launched[0], "pick", False))


class AnOpenNeverLandsInAnotherPlanesSession(_OpensBeta):
    """The cross-plane guarantee #793 built, kept across the new door into `cmd_launch`.

    **This is the hazard the open introduces, and it is not hypothetical.** One tmux server
    serves every plane on the machine — eleven sessions from three projects on the
    operator's own socket the week this was written — and `cmd_launch` decides whether to
    start a session or add a window to one with ``if session in live_sessions:``, which is
    a **name** test over that whole machine. `_plane_session` having just answered ``None``
    does not mean the name is free; it means *this plane cannot prove the session is its
    own*, and those are different facts whenever two planes share a workspace name.

    Left unguarded, a click on `shared` in plane B would have added a chat window to plane
    A's live `shared` session — another project's frame, across every isolation boundary
    charter has — and then failed the `@charter_plane` veto on the way back out, so the
    operator would have been told the open failed while a window sat in somebody else's
    session. §3.3 names exactly this: *"Open-or-focus must match on this plane's chat
    directories, never on a live session name."*
    """

    def _run_with_live_name(self, names, **kw):
        with mock.patch("charter.commands_frame._live_sessions",
                        return_value=set(names)):
            return self._run(**kw)

    def test_a_name_already_live_on_the_machine_is_not_opened(self):
        s = self._run_with_live_name({"beta"})
        self.assertEqual(self.launched, [], "it launched into a session it does not own")
        self.assertEqual(s.switched, [])

    def test_it_says_the_name_is_taken_rather_than_that_the_open_failed(self):
        """The operator can act on this one — it is their own machine and their own other
        project — so the sentence has to say which fact stopped it."""
        self._run_with_live_name({"beta"})
        said = self.said.call_args[0][1]
        self.assertIn("beta", said)
        self.assertIn("another plane", said)

    def test_an_unrelated_live_session_does_not_block_the_open(self):
        """The guard is the workspace's OWN session name and nothing broader: a machine
        with other charter frames running on it is the ordinary case, not a refusal."""
        self._run_with_live_name({"alpha", "something-else"})
        self.assertEqual(len(self.launched), 1)


class TheOpenUsesTheHarnessTheOperatorIsAlreadyIn(_OpensBeta):
    """Which harness a tab opens with — the one question a click cannot carry.

    A tab names a workspace and nothing else. The chat it was clicked FROM recorded its own
    harness, and using that makes the click mean "another workspace, same tool" — the only
    answer available that the operator actually expressed.
    """

    def test_it_is_the_harness_this_chat_records(self):
        _a_chat(self.FID, ws="alpha", pane="%1", harness="codex")
        self._run()
        self.assertEqual(self.launched[0].harness, "codex")

    def test_a_chat_with_no_recorded_harness_falls_back_to_the_planes_default(self):
        """A chat launched by a charter that predates `state.record_identity`. The plane's
        `[harness] default` is a thing somebody chose, so it is a better answer than
        refusing — and the fallback is named in `cmd_reopen`'s own words."""
        _a_chat(self.FID, ws="alpha", pane="%1", harness="")
        with mock.patch.dict(config.HARNESS, {"default": "codex"}):
            self._run()
        self.assertEqual(self.launched[0].harness, "codex")

    def test_a_directory_charter_cannot_enter_is_refused_before_the_launch(self):
        """`cmd_launch` reads its cwd with `os.getcwd()`, so the open has to `os.chdir`
        into the workspace — and a `chdir` that fails must stop the launch rather than
        run it from wherever this process happened to be standing, which is a chat in
        ANOTHER workspace. Refused before `cmd_launch` is called at all."""
        with mock.patch("charter.commands_frame.os.chdir",
                        side_effect=OSError(13, "denied")):
            self._run()
        self.assertEqual(self.launched, [])
        self.assertIn("cannot enter", self.said.call_args[0][1])

    def test_the_launcher_is_left_standing_where_it_started(self):
        """The `finally` half of the same `chdir`, and `cmd_reopen`'s own rule: this
        process goes on to re-lay-out two frames, and a launcher left in somebody else's
        directory is the silent wrongness §4e's cwd item exists to close."""
        was = os.getcwd()
        self._run()
        self.assertEqual(os.getcwd(), was)

    def test_with_neither_it_refuses_by_name_rather_than_guessing(self):
        _a_chat(self.FID, ws="alpha", pane="%1", harness="")
        with mock.patch.dict(config.HARNESS, {"default": ""}):
            self._run()
        self.assertEqual(self.launched, [])
        self.assertIn("no harness", self.said.call_args[0][1])


class TheLauncherCanBuildAFrameWithNoTerminalOfItsOwn(PersonaIso, unittest.TestCase):
    """`cmd_launch`'s two tty-shaped assumptions, opened exactly as far as the seam needs.

    Both are correct for every launch that IS the operator's terminal, and neither is
    correct for one that will never attach.

    **`shutil.which` is pinned in both cases, and that is not decoration.** `cmd_launch`
    has an EARLIER `bypass` for a registered harness whose binary is not installed, so
    without the pin these cases ask whether the developer running them happens to have
    `claude` on `$PATH` — green on a laptop, red on CI, and measuring the machine rather
    than the repo either way. Caught exactly that way: both passed locally and failed on
    all four CI Pythons.
    """

    def test_a_launch_that_will_not_attach_is_not_execd_away_by_the_non_tty_guard(self):
        """**The guard that actually blocked this**, and the reason it is dangerous
        rather than merely wrong: `bypass` calls `os.execvp`, so a detached switch that
        reached it would have been REPLACED by a bare harness with its streams on
        `/dev/null` — no frame, no session, no switch, and no surface left to report it
        on. Gated on `_wants_attach`, because a non-tty stdout only means "this process
        cannot be the operator's terminal", which such a launch was never going to be.
        """
        args = SimpleNamespace(harness="claude", rest=[], no_frame=False,
                               workspace="beta", pick=False, attach=False,
                               size=(120, 40))
        with mock.patch("charter.commands_frame.sys.stdout") as out, \
                mock.patch("charter.commands_frame.bypass") as byp, \
                mock.patch("charter.commands_frame.shutil.which",
                           return_value="/nowhere/claude"), \
                mock.patch("charter.commands_frame.tmuxctl.version", return_value=None):
            out.isatty.return_value = False
            commands_frame.cmd_launch(args)
        byp.assert_not_called()

    def test_a_launch_that_wants_a_terminal_is_still_bypassed_without_one(self):
        """The guard is opened for one case and left standing for every other: a piped
        `charter claude > file` must still run the harness with no frame."""
        args = SimpleNamespace(harness="claude", rest=[], no_frame=False,
                               workspace=None, pick=False)
        with mock.patch("charter.commands_frame.sys.stdout") as out, \
                mock.patch("charter.commands_frame.shutil.which",
                           return_value="/nowhere/claude"), \
                mock.patch("charter.commands_frame.bypass",
                           return_value=0) as byp:
            out.isatty.return_value = False
            commands_frame.cmd_launch(args)
        byp.assert_called_once()

    def test_a_size_that_is_not_two_positive_integers_is_measured_instead(self):
        """The handed-in size reaches `layout.session_argv` as `-x`/`-y`, where a
        malformed pair is a tmux parse error that would take the whole launch down with
        nothing on screen to say why — so it is validated rather than trusted, and an
        unusable one falls back to the terminal reading `cmd_launch` always did.

        Each shape is its own row because each is a separate `and`/`isinstance` in the
        guard, and a single case would leave the rest of them free to be deleted."""
        for bad in (None, (80,), (80, 24, 3), "80x24", (80.0, 24), ("80", 24),
                    (0, 24), (80, 0), (-1, 24), (80, -1)):
            with self.subTest(size=bad):
                self.assertIsNone(
                    commands_frame._launch_size(SimpleNamespace(size=bad)))

    def test_a_size_of_two_positive_integers_is_taken(self):
        self.assertEqual(
            commands_frame._launch_size(SimpleNamespace(size=(132, 43))), (132, 43))

    def test_wants_attach_is_false_when_a_caller_says_so(self):
        """`_wants_attach`'s own docstring anticipated this: *"the day a third caller
        wants one without the other, a shared expression would have to be unpicked at two
        call sites at once."* This is that caller — not restoring, and not a terminal."""
        self.assertFalse(commands_frame._wants_attach(
            SimpleNamespace(attach=False)))

    def test_every_existing_caller_still_attaches(self):
        """An `args` with no `attach` field is every production and test caller that
        predates the seam, and each of them is the operator's terminal."""
        self.assertTrue(commands_frame._wants_attach(SimpleNamespace()))


@unittest.skipUnless(_HAS_TMUX, "needs a real tmux")
class ADetachedProcessCanBuildAWorkspace(unittest.TestCase):
    """The measurement the old refusal's reason was wrong about, against a real server.

    Nothing charter is in here on purpose: this is a claim about **tmux**, and it is the
    one the design rests on. A process with `start_new_session=True` and all three streams
    on `/dev/null` — `builtin_actions._spawn`'s exact shape — creates a session, splits it,
    marks it, and moves an attached client onto it.

    Run against tmux 3.7c and against tmux 3.2 (`tmuxctl.FLOOR`), identically on both.
    CI installs no tmux, so this runs by hand.
    """

    def test_a_detached_creator_builds_a_session_and_switches_a_client_onto_it(self):
        import pty
        import time

        sock = "openws-t-%d" % os.getpid()
        self.assertNotEqual(sock, "charter")

        def tm(*a):
            return subprocess.run(["tmux", "-L", sock, *a],
                                  capture_output=True, text=True)

        self.addCleanup(tm, "kill-server")
        tm("new-session", "-d", "-s", "home", "sh")

        # **Several `TERM`s, then a skip — never an assertion.** CI has tmux but the
        # terminal it offers a client is not the developer's: measured, a client that
        # attaches on a laptop refuses on all four CI Pythons, and a bare `assertTrue`
        # there reports "charter is broken" for "this machine has no terminal tmux will
        # talk to". The sibling real-tmux modules all spell this ladder; so does this one.
        client = ""
        for term in ("xterm-256color", "screen", "vt100"):
            pid, _fd = pty.fork()
            if pid == 0:                                # pragma: no cover - the child
                # `os._exit` in a `finally`, which every other `pty.fork` in this suite
                # spells for the same reason: an `execvp` that RAISES leaves the child
                # running the test framework, and a second runner that goes on to fork is
                # how one failed exec becomes a machine full of them. Never `sys.exit` —
                # that unwinds, and unittest catches `SystemExit`.
                try:
                    os.environ["TERM"] = term
                    os.execvp("tmux", ["tmux", "-L", sock, "attach", "-t", "home"])
                finally:
                    os._exit(127)
            self.addCleanup(lambda p=pid: os.waitpid(p, os.WNOHANG))
            deadline = time.time() + 10
            while time.time() < deadline and not client:
                client = tm("list-clients", "-F", "#{client_tty}").stdout.strip()
                time.sleep(0.05)
            if client:
                break
            try:
                os.kill(pid, 9)
            except OSError:
                pass
        if not client:
            self.skipTest("no tmux client can attach on this machine, and the claim "
                          "under test is that a detached process can move a real one")

        script = (
            '{ [ -t 0 ] || [ -t 1 ] || [ -t 2 ]; } && exit 91; '
            f'tmux -L {sock} new-session -d -s ws || exit 92; '
            f'tmux -L {sock} split-window -t ws -d || exit 93; '
            f'tmux -L {sock} set -t ws @charter_plane /a/plane || exit 94; '
            f'tmux -L {sock} switch-client -c "{client}" -t ws || exit 95; exit 0')
        rc = subprocess.Popen(["/bin/sh", "-c", script], start_new_session=True,
                              stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                              stderr=subprocess.DEVNULL).wait(timeout=30)
        self.assertEqual(rc, 0, "a detached, tty-less process could not build a session")

        deadline = time.time() + 10
        while time.time() < deadline:
            if tm("display-message", "-p", "-t", client,
                  "#{client_session}").stdout.strip() == "ws":
                break
            time.sleep(0.05)
        self.assertEqual(tm("display-message", "-p", "-t", client,
                            "#{client_session}").stdout.strip(), "ws")
        self.assertEqual(
            len(tm("list-panes", "-t", "ws", "-F", "#{pane_id}").stdout.split()), 2)
        self.assertEqual(tm("show-options", "-t", "ws", "-v",
                            "@charter_plane").stdout.strip(), "/a/plane")
        self.assertEqual(
            len(tm("list-panes", "-t", "home", "-F", "#{pane_id}").stdout.split()), 1,
            "building a workspace killed something")


@unittest.skipUnless(_HAS_TMUX, "needs a real tmux")
class ARealTabOpensARealWorkspace(PersonaIso, unittest.TestCase):
    """**The operator's report, end to end, with nothing faked but the harness binary.**

    Every class above stops somewhere: the unit cases stop at a mock `cmd_launch`, and
    :class:`ADetachedProcessCanBuildAWorkspace` measures tmux without charter in the room.
    This one runs the whole thing — a real tmux server, a real client on a real pty, a real
    `_switch_client`, a real `cmd_launch` underneath it building a real session with real
    panel processes, and a real `switch-client` landing the client on it.

    **The harness is a script that sleeps**, and that is the only substitution. A test may
    not start a real `claude`: it costs money, it needs credentials, and it would make this
    file's result depend on a network. What the script has to be is a real executable on
    `$PATH` that tmux really runs, because the claim being tested is that a detached
    process can start one at all.

    Verified on tmux 3.7c and at the 3.2 floor, identically on both.
    """

    HERE, THERE = "alpha", "beta"

    def setUp(self):
        super().setUp()
        # **The declaration is the point of the fixture, not paperwork around it.** The
        # process this stands in for is a `charter frame-switch` started by
        # `builtin_actions._spawn`: `start_new_session=True`, all three streams on
        # `/dev/null`. Saying so out loud is what makes the launch below take the branches
        # a real click takes — no #518 picker, and `os.get_terminal_size()` raising, which
        # is exactly the condition the size seam exists for.
        _ttyguard.no_terminal()
        # The repo scan and the version/forge pollers are real detached charter children
        # that `cmd_launch` kicks off and nothing here waits for. This test is about the
        # SESSION the launch builds, not about them, so the spawners are stopped rather
        # than the forks allowed — `tests/_planeguard.py`'s own first suggestion.
        no_background_refresh(self)
        self.enterContext(mock.patch("charter.commands_frame._spawn_gather"))
        self.socket = _tmuxreap.name("open-ws-tab")
        self.enterContext(mock.patch.object(commands_frame, "SOCKET", self.socket))
        self.addCleanup(self._teardown)
        self.kids: list[tuple[int, int]] = []

        # A harness that is a real binary and starts no conversation.
        binroot = Path(self.enterContext(__import__("tempfile").TemporaryDirectory()))
        fake = binroot / "codex"
        fake.write_text("#!/bin/sh\nexec sleep 300\n")
        fake.chmod(0o755)
        self.enterContext(mock.patch.dict(
            os.environ, {"PATH": f"{binroot}{os.pathsep}{os.environ.get('PATH', '')}"}))

        for n in (self.HERE, self.THERE):
            (config.WORKSPACES_DIR / n).mkdir(parents=True, exist_ok=True)

        # The chat the operator is standing in, built by hand so the fixture does not
        # depend on the code path under test.
        self.fid = f"{self.HERE}.1"
        started = self._tmux("new-session", "-d", "-s", self.HERE, "-n", self.fid,
                             "-x", "132", "-y", "43", "-P", "-F", "#{pane_id}",
                             "sleep 300")
        self.assertEqual(started.returncode, 0, started.stderr)
        self.pane = started.stdout.strip()
        _a_chat(self.fid, ws=self.HERE, pane=self.pane, harness="codex")
        self.assertEqual(self._tmux("set-option", "-t", self.pane,
                                    commands_frame._PLANE_OPTION,
                                    str(config.STATE_DIR)).returncode, 0)

    def _teardown(self):
        for pid, fd in self.kids:
            try:
                os.kill(pid, 9)
                os.waitpid(pid, 0)
            except OSError:
                pass
            try:
                os.close(fd)
            except OSError:
                pass
        self._tmux("kill-server")

    def _tmux(self, *a):
        return subprocess.run(["tmux", "-L", self.socket, *a],
                              capture_output=True, text=True)

    def _clients(self, session):
        return [c for c in self._tmux("list-clients", "-t", session,
                                      "-F", "#{client_name}").stdout.split() if c]

    #: The terminal this fixture's operator is sitting at. Deliberately NOT 80x24: the
    #: whole point of the size seam is that the frame is built for the real terminal, and
    #: a fixture at the fallback size could not tell the two apart.
    CLIENT_SIZE = (132, 43)

    def _attach(self, session):
        import fcntl
        import pty
        import struct
        import termios
        import time
        for term in ("xterm-256color", "screen", "vt100"):
            pid, fd = pty.fork()
            if pid == 0:                                # pragma: no cover - the child
                try:
                    os.environ["TERM"] = term
                    os.execvp("tmux", ["tmux", "-L", self.socket, "attach",
                                       "-t", session])
                finally:
                    os._exit(127)
            # Sized BEFORE the client is waited for: a pty is born 80x24, and tmux resizes
            # the session it attaches to to whatever the client is. Left at the default
            # this fixture would be measuring `_FALLBACK_SIZE` against itself.
            cols, rows = self.CLIENT_SIZE
            fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
            end = time.time() + 10
            while time.time() < end and not self._clients(session):
                time.sleep(0.05)
            if self._clients(session):
                self.kids.append((pid, fd))
                return self._clients(session)[-1]
            try:
                os.kill(pid, 9)
                os.waitpid(pid, 0)
            except OSError:
                pass
        self.skipTest("no tmux client can attach on this machine, and opening a "
                      "workspace to switch into is a decision about an ATTACHED client")

    def _panes(self):
        """Every pane on the server with its pid and whether tmux calls it dead."""
        return sorted(self._tmux("list-panes", "-a", "-F",
                                 "#{pane_id} #{pane_pid} #{pane_dead}").stdout.split("\n"))

    def _click_the_tab(self):
        """What the panel's handler does, minus the pointer: `charter frame-switch
        --workspace <name>` in a process of its own. Called in-process so the assertions
        can be made after it has finished rather than polled for."""
        import time
        commands_frame._switch_client(self.fid, self.THERE,
                                      said=f"workspace → {self.THERE}")
        time.sleep(0.3)

    def test_the_workspace_opens_and_the_terminal_arrives_in_it(self):
        """**The report, answered.** A tab for a workspace that was not open now opens it
        and takes the operator there — which is what the refusal it replaces told them to
        go and type by hand."""
        client = self._attach(self.HERE)
        self._click_the_tab()
        self.assertIn(self.THERE,
                      self._tmux("list-sessions", "-F", "#{session_name}").stdout.split(),
                      "the click did not open the workspace it named")
        self.assertEqual(self._clients(self.THERE), [client],
                         "the workspace opened and the terminal never arrived in it")
        self.assertEqual(self._clients(self.HERE), [])

    def test_the_chat_left_behind_keeps_running(self):
        """The operator's own sentence — *"keep my current sessions open in background"* —
        measured rather than argued: the pane they left has the same pid afterwards and
        tmux does not call it dead."""
        self._attach(self.HERE)
        before = [p for p in self._panes() if p.startswith(self.pane + " ")]
        self._click_the_tab()
        self.assertEqual([p for p in self._panes() if p.startswith(self.pane + " ")],
                         before, "opening a workspace killed the chat it left")

    def test_the_opened_workspace_is_marked_with_this_plane(self):
        """§4b's marker, written by the launch that CREATED the session — so the next
        switch can tell this session from another plane's of the same name."""
        self._attach(self.HERE)
        self._click_the_tab()
        pane = self._tmux("list-panes", "-t", self.THERE,
                          "-F", "#{pane_id}").stdout.split()[0]
        self.assertEqual(
            self._tmux("display-message", "-p", "-t", pane,
                       "#{%s}" % commands_frame._PLANE_OPTION).stdout.strip(),
            str(config.STATE_DIR))

    def test_the_new_workspace_is_laid_out_for_the_terminal_and_not_for_eighty_columns(
            self):
        """The size seam, measured where it matters. A detached launcher cannot read a
        terminal, so without a size handed in this window would have been built at
        `_FALLBACK_SIZE` — and `_drawable_slots` reads 80x24 as room for almost nothing."""
        self._attach(self.HERE)
        self._click_the_tab()
        width = self._tmux("display-message", "-p", "-t", self.THERE,
                           "#{window_width}").stdout.strip()
        self.assertEqual(width, str(self.CLIENT_SIZE[0]),
                         "the opened workspace was laid out for the wrong terminal")


if __name__ == "__main__":
    unittest.main()
