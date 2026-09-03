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
import socket
import subprocess
import threading
import time
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame, config, root
from charter.frame import state

from tests import _tmuxreap, _tmuxsocket, _ttyguard
from tests._isolation import PersonaIso, make_plane, no_background_refresh

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

    def _run_watching_cwd(self, seen: list) -> None:
        """Run a click, recording the launcher's cwd at the moment it is called.

        The `chdir` is only in effect inside `cmd_launch`; the `finally` has restored it
        before the test could look, so the reading has to be taken from in there."""
        server = _Server()

        def fake_launch(args):
            seen.append(os.getcwd())
            self.launched.append(args)
            _a_chat("beta.1", ws="beta", pane="%9")
            server.opened.append("%9")
            return 0

        with mock.patch("charter.commands_frame.subprocess.run", side_effect=server), \
                mock.patch("charter.commands_frame.cmd_launch",
                           side_effect=fake_launch):
            commands_frame._switch_client(self.FID, "beta", said="workspace → beta")


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

    def test_nothing_is_opened_when_there_is_no_terminal_to_take_there(self):
        """**A switch that cannot happen must not spend first.** `_switch_client` already
        refuses when no client is attached to this chat's session — the operator detached,
        or an agent with no terminal is driving the frame — and that refusal used to come
        after a free `_plane_session` read. An open is not free: it starts a harness
        process and claims a chat ordinal. Asked BEFORE the open, so a frame nobody is
        looking at cannot be made to launch things into the dark.

        Deliberately inside the not-open branch rather than hoisted over the whole
        function: the order of the existing refusals is #793's and pinned by its own
        tests, and moving this one over `already in workspace` would change what a
        detached frame is told about a workspace it is already in."""
        s = self._run(_Server(clients=()))
        self.assertEqual(self.launched, [], "it started a harness for nobody")
        self.assertEqual(s.switched, [])
        self.assertIn("no terminal", self.said.call_args[0][1])

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

    def test_the_launch_runs_in_the_workspaces_own_directory(self):
        """Where `charter --workspace beta` typed in it would have run, and what
        `state.record_cwd` goes on to hold. Read INSIDE the fake launcher, because that is
        the only moment the `chdir` is in effect — the `finally` has put it back by the
        time the test could look."""
        seen = []
        self._run_watching_cwd(seen)
        # `os.path.realpath` on the expected side, never on the reading: `os.getcwd()`
        # hands back a RESOLVED path, and on macOS the plane lives under a `/tmp` that
        # is a symlink to `/private/tmp`. Comparing the constructed spelling would
        # fail on the platform that resolves and pass on the one that does not
        # (`tests/_tmuxsocket.py` measures the same trap for a socket path).
        self.assertEqual(seen, [os.path.realpath(config.WORKSPACES_DIR / "beta")])

    def test_a_workspace_with_no_directory_of_its_own_is_made_rather_than_stood_beside(self):
        """The other half of the same expression, and #850 changed which half. A workspace
        can be named by a chat record and by the bar without having a directory yet; the
        old answer was the plane root, which opened the tab into a directory that is not
        the workspace it names — the chat then recorded `workspace = beta` beside
        `cwd = <plane root>`, and stood outside the directory charter's per-workspace
        layer lives in. `workspace.ensure` makes it, which is what a `charter --workspace
        beta` typed in the plane would have done."""
        import shutil as _sh
        _sh.rmtree(config.WORKSPACES_DIR / "beta")
        seen = []
        self._run_watching_cwd(seen)
        self.assertEqual(seen, [os.path.realpath(config.WORKSPACES_DIR / "beta")])
        self.assertTrue((config.WORKSPACES_DIR / "beta").is_dir())

    def test_a_workspace_charter_cannot_create_still_falls_back_to_the_plane_root(self):
        """`ensure` raises for a `workspaces/` it cannot write, and this runs detached
        with its streams on `/dev/null` — so it degrades to the answer it always gave
        rather than taking the open down with a traceback nobody will ever see."""
        import shutil as _sh
        _sh.rmtree(config.WORKSPACES_DIR / "beta")
        seen = []
        with mock.patch("charter.commands_frame.workspace.ensure",
                        side_effect=OSError(13, "denied")):
            self._run_watching_cwd(seen)
        self.assertEqual(seen, [os.path.realpath(config.ROOT)])

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

    def test_the_refused_directory_is_contained_before_it_reaches_the_screen(self):
        """A path is the one value in that sentence charter did not choose the shape of,
        and the attention row is a pane charter paints — a newline in it writes a second
        line into the frame's own chrome. `contain.readable` is what stops that.

        The literal is spelled out by hand rather than round-tripped through the same
        constant the code interpolates, which would be green under any containment at
        all: the raw two-line form must not appear, and the message must stay one line.
        """
        # It has to be the path `_launch_root` HANDS BACK, and getting that wrong is what
        # this comment is for: the first version of this test replaced a path that was not
        # a real directory, which the fallback of the day swapped for the plane root, so
        # it passed against an uncontained message. `_launch_root` is `workspace.ensure`
        # since #850 — it creates rather than falling back — so this stubs the creator and
        # the path it returns is the one that reaches the sentence. A newline is legal in a
        # POSIX filename, so the directory is real and the `chdir` genuinely reaches it.
        bad = Path(str(config.WORKSPACES_DIR / "beta") + "\nworkspace → evil")
        bad.mkdir(parents=True, exist_ok=True)
        self.addCleanup(lambda: __import__("shutil").rmtree(bad, ignore_errors=True))
        self.assertTrue(bad.is_dir(), "the fixture's bad path must be a real directory")
        with mock.patch("charter.commands_frame.workspace.ensure",
                        return_value=bad), \
                mock.patch("charter.commands_frame.os.chdir",
                           side_effect=OSError(13, "denied")):
            self._run()
        said = self.said.call_args[0][1]
        self.assertNotIn("\nworkspace → evil", said)
        self.assertEqual(said.count("\n"), 0, said)

    def test_a_directory_that_cannot_be_returned_to_does_not_lose_the_open(self):
        """The `finally`'s own `except OSError`. Restoring the cwd is best effort — the
        directory this process started in can be gone by the time the launch returns
        (a sibling `reap` removed a chat directory, a worktree was deleted) — and a
        raise there would take down a switch whose workspace is already open and
        running, leaving the operator's client where it was with no message at all."""
        calls = []

        def chdir(path):
            calls.append(str(path))
            if len(calls) > 1:                 # the restore, not the way in
                raise OSError(2, "gone")

        with mock.patch("charter.commands_frame.os.chdir", side_effect=chdir):
            s = self._run()
        self.assertEqual(len(calls), 2, "the launcher never tried to go back")
        self.assertEqual(s.switched, [("/dev/ttys001", "$2")],
                         "a cwd charter could not return to lost the switch")

    def test_the_launcher_is_left_standing_where_it_started(self):
        """The `finally` half of the same `chdir`, and `cmd_reopen`'s own rule: this
        process goes on to re-lay-out two frames, and a launcher left in somebody else's
        directory is the silent wrongness §4e's cwd item exists to close."""
        was = os.getcwd()
        self._run()
        self.assertEqual(os.getcwd(), was)

    def test_a_plane_whose_harness_table_is_missing_entirely_is_not_a_crash(self):
        """`config.HARNESS` is ``None`` on a plane whose config has no `[harness]` table at
        all — a different state from one that has the table with no `default` in it, and
        the `or {}` is the only thing between the two and an `AttributeError` on the
        detached side, where a traceback goes to `/dev/null` and the click does nothing."""
        _a_chat(self.FID, ws="alpha", pane="%1", harness="")
        with mock.patch.object(config, "HARNESS", None):
            self._run()
        self.assertEqual(self.launched, [])
        self.assertIn("no harness", self.said.call_args[0][1])

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


class TheLauncherAsksWhoseTmuxItIsIn(PersonaIso, unittest.TestCase):
    """`cmd_launch`'s guest branch, in the one form that runs everywhere (#812).

    :class:`ATabClickedInsideCharactersOwnTmux` is the truth of this and it needs a real
    tmux and a real attached client, so it SKIPS wherever no pty can be handed one — CI
    among them. This is the same decision asked with nothing running: does `$TMUX` naming
    charter's OWN socket send the launch down `_launch_in_operator_tmux`?

    It answers in milliseconds, and that matters twice over. `_launch_in_operator_tmux`
    stays awake for the life of the harness it starts, so the real-tmux class catches a
    regression here as a click that never returns — a 30-second deadline, and a deletion
    sweep that would rather kill a hanging suite than record the mutation. This case
    fails outright instead.
    """

    #: A socket name no server is running on, so the fall-through path below reaches tmux
    #: and finds nothing rather than reading the operator's own live `charter` server.
    SOCKET = _tmuxreap.name("whose-tmux")

    def _guest_path_taken(self, tmux_socket: str) -> bool:
        """Whether a launch whose ``$TMUX`` names *tmux_socket* builds a guest window."""
        (config.WORKSPACES_DIR / "beta").mkdir(parents=True, exist_ok=True)
        _ttyguard.no_terminal()
        args = SimpleNamespace(harness="frame", rest=["--", "true"], no_frame=False,
                               workspace="beta", pick=False, attach=False,
                               size=(120, 40))
        empty = subprocess.CompletedProcess([], 0, "", "")
        with mock.patch.object(commands_frame, "SOCKET", self.SOCKET), \
                mock.patch.dict(os.environ,
                                {"TMUX": _tmuxsocket.tmux_env(tmux_socket)}), \
                mock.patch.object(commands_frame.tmuxctl, "version",
                                  return_value=(3, 7)), \
                mock.patch.object(commands_frame.tmuxctl, "run", return_value=empty), \
                mock.patch.object(commands_frame.state, "new_chat_id",
                                  return_value=None), \
                mock.patch.object(commands_frame, "_launch_in_operator_tmux",
                                  return_value=0) as guest:
            commands_frame.cmd_launch(args)
        return guest.called

    def test_charters_own_socket_read_out_of_tmux_is_not_somebody_elses_server(self):
        """**#812's launch half.** A tab click runs `cmd_launch` in-process from a panel
        of charter's own frame (`_open_workspace`), and tmux exports its socket into that
        process as an absolute path. Read as a guest, the chat for the workspace being
        opened is built as a window inside the session the click came FROM — so the
        workspace never becomes a session and the switch has nothing to switch to."""
        self.assertFalse(self._guest_path_taken(_tmuxsocket.socket_path(self.SOCKET)))

    def test_a_tmux_charter_did_not_start_still_builds_a_window_in_it(self):
        """The branch is narrowed, not removed: ADR 0018's whole point is that a frame
        inside somebody else's tmux is a WINDOW on their server, with no second tmux under
        it and no second prefix key on top."""
        self.assertTrue(self._guest_path_taken(_tmuxsocket.OPERATOR_SOCKET))


@unittest.skipUnless(_HAS_TMUX, "needs a real tmux")
class ADetachedProcessCanBuildAWorkspace(unittest.TestCase):
    """The measurement the old refusal's reason was wrong about, against a real server.

    Nothing charter is in here on purpose: this is a claim about **tmux**, and it is the
    one the design rests on. A process with `start_new_session=True` and all three streams
    on `/dev/null` — `builtin_actions._spawn`'s exact shape — creates a session, splits it,
    marks it, and moves an attached client onto it.

    Run against tmux 3.7c and against tmux 3.2 (`tmuxctl.FLOOR`), identically on both.
    CI has tmux and really runs this; what it does not have is a terminal tmux will hand a
    CLIENT, so the case skips there and the `switch-client` half is verified by hand. See
    the `TERM` ladder below, which is the measurement that established that.
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
class AClientHungUpOnSaysTheServerExitedUnexpectedly(unittest.TestCase):
    """The tmux fact :meth:`ARealTabOpensARealWorkspace._hold_the_server` is shaped
    around, measured rather than inherited — and measured **deterministically**, which is
    the point of doing it here rather than trusting #713's argument a second time.

    #839 is a flake nobody has made fail on demand: it wants a runner loaded enough that a
    tmux server which has already decided to exit is descheduled between "stopped serving"
    and "closed its listening fd", and a client whose `connect` lands inside that window.
    What that client then reports is not a race, though — it is a fact, and this pins it
    with no server, no signals, no load and no `sleep`:

    * a socket file at the path a `-L <name>` client looks at,
    * a listener that **accepts** — which is the synchronisation, read back from the
      kernel: the client really did connect, so it is past the point where it would have
      built a server of its own,
    * and then a hang-up instead of an answer, which is what a retiring tmux gives a
      client it will never serve.

    The answer is rc 1 and the three words #839's stack traces end with. So the message
    that names charter names a client that reached a socket with nothing behind it, and
    the way to stop drawing it is to stop leaving sockets like that between tests.

    If a later tmux ever answered something else here, `_hold_the_server`'s argument
    should be re-made rather than inherited — this is the assertion that would say so.
    """

    def test_a_client_that_connects_and_is_never_answered_says_it_lost_the_server(self):
        name = _tmuxreap.name("hung-up-on")
        path = Path(_tmuxsocket.socket_path(name))
        # `S_IRWXU`, the mode tmux makes this directory with itself: it refuses a socket
        # directory that anyone else can reach into, and a fresh machine may not have one.
        path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        # LIFO, so the listener is closed first and the file is only then removed — and
        # removed only if nothing is bound to it, which is `_tmuxreap.reap`'s rule and
        # matters on the one path where this test fails: a client that built a real server
        # here would be holding this path, and a file removed from under it is a resident
        # tmux nothing can find (#564).
        self.addCleanup(self._drop, path)
        self.addCleanup(listener.close)
        listener.bind(str(path))
        listener.listen(1)
        client = subprocess.Popen(
            ["tmux", "-L", name, "new-session", "-d", "-s", "s", "--", "cat"],
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.addCleanup(client.kill)
        was = path.stat().st_ino
        conn, _ = listener.accept()
        conn.close()
        out, err = client.communicate(timeout=30)
        self.assertEqual(client.returncode, 1, f"{out!r} {err!r}")
        self.assertEqual(
            err.strip(), "server exited unexpectedly",
            "this is the sentence #839's failures are made of, and a tmux that spells it "
            "differently makes every argument built on it worth re-reading")
        # A client that ends up building a server unlinks what it found and binds its own,
        # which is what makes a fresh inode one server birth and what `_hold_the_server`
        # counted twelve of. This one connected instead, so the socket it connected to is
        # still the one bound above — and if it were not, nothing here measured a hang-up.
        self.assertEqual(path.stat().st_ino, was,
                         "the socket was rebuilt, so this client started a server of its "
                         "own rather than being hung up on by one")

    @staticmethod
    def _drop(path: Path) -> None:
        """Remove *path* unless something is still bound to it."""
        if not _tmuxreap._listening(path):
            path.unlink(missing_ok=True)


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

    **The server is HELD, and #839 is the bill for the version that rebuilt it per test**
    — see :meth:`_hold_the_server`, which is #713's construction applied to this fixture.
    """

    HERE, THERE = "alpha", "beta"

    #: This module's own server, unique per test PROCESS — `tests/_tmuxreap.py`'s
    #: namespace, so a run killed before its cleanup is reaped by the next one rather than
    #: collided with. A class attribute rather than a `setUp` local because
    #: :meth:`tearDownClass` has to name the same socket, and `_tmuxreap.name` answers the
    #: same string every time in one process.
    SOCKET = _tmuxreap.name("open-ws-tab")

    #: The session :meth:`_hold_the_server` opens and nothing else here ever touches. One
    #: per server, the same name `_TmuxServerFixture.KEEPER` uses in
    #: `tests/test_frame_tmux_integration.py`.
    #:
    #: It is never a target and never a subject: everything this class writes is `-t`'d at
    #: a session or pane it created itself, the two cases that read `list-panes -a` filter
    #: on :attr:`pane`, and `list-sessions` is only ever read with `assertIn` or against
    #: :meth:`_sessions`, which states the keeper by name.
    #:
    #: **What charter makes of it, stated rather than assumed.** `_live_windows` — the
    #: read behind `_reap_this_server` and every "is this chat still running" question —
    #: asks for `@charter_chat` and a `cat` in a session nothing marked carries none, so
    #: the keeper is invisible to it. `_live_sessions` is a different read and DOES see it:
    #: it lists every session name on the socket, and `cmd_launch`'s §4k open-or-focus gate
    #: asks whether the workspace it was given is in that set. So the one rule this name
    #: has to obey is that it is not a workspace name — :attr:`HERE` and :attr:`THERE` are
    #: `alpha` and `beta`, and a keeper called `alpha` would make the open under test
    #: focus a `cat`.
    KEEPER = "keep"

    @classmethod
    def _server(cls, *a):
        """One tmux command on this class's socket, without needing an instance —
        :meth:`tearDownClass` runs after the last one is gone."""
        return subprocess.run(["tmux", "-L", cls.SOCKET, *a],
                              capture_output=True, text=True)

    @classmethod
    def tearDownClass(cls):
        """The held server goes here, and its socket FILE goes with it.

        Held for the class rather than for a test means nothing else will empty it, so
        this is the only thing that ends it — and a `kill-server` that leaves the file
        behind is the leak `tests/_tmuxreap.py` was written to count (14 resident servers
        and 658 socket files, #564). It is also what makes the NEXT class on this socket
        — this one's subclass — build a server rather than connect to one:
        :meth:`_hold_the_server` explains why those are different.
        """
        cls._server("kill-server")
        cls._drop_the_socket_file()

    @classmethod
    def _drop_the_socket_file(cls):
        """Unlink this class's socket file, once nothing is bound to it any more.

        **Gated, and `reap`'s own reasoning is the gate.** tmux does not unlink its socket
        on the way out, so removing it is what stops the next client finding a path to
        connect to. But removing one from under a server that did NOT die is #564 with
        the evidence destroyed — a resident tmux holding a session, with no file left
        naming it, invisible to every later reap. So this waits for the socket to stop
        accepting and leaves the file where it is if it never does.

        Waits on a fact read back from the socket, never for a duration: the deadline
        only bounds how long "it would not die" takes to give up.

        `_tmuxreap._listening` rather than a second `AF_UNIX` connect written here, and
        rather than a `tmux` invocation that would have to be told apart from the very
        race this is about: it is the suite's one spelling of "is anybody bound to this
        path", and `tests/test_the_suite_reaps_its_own_tmux_servers.py` already asks it by
        that name from outside the module.
        """
        path = Path(_tmuxsocket.socket_path(cls.SOCKET))
        deadline = time.monotonic() + 10
        while _tmuxreap._listening(path) and time.monotonic() < deadline:
            time.sleep(0.02)
        if not _tmuxreap._listening(path):
            path.unlink(missing_ok=True)

    def _hold_the_server(self):
        """One session on this class's socket that no test ever kills, so the server is
        born ONCE per class instead of once per test.

        **#713's construction, and #839 is what this fixture cost without it.** `tmux
        new-session` on a socket whose file exists is a client that **connects**, not one
        that builds; and `kill-server` ends the server while leaving that file behind.
        So a fixture that killed the server at the end of every test handed the next
        test's `new-session` a socket with a process on its way out at the other end of
        it. Reach that process after it has stopped accepting and the client gets
        `ECONNREFUSED`, unlinks, and starts a fresh server — the healthy path, and the
        only one an idle machine ever sees. Reach it while it still holds its listening
        fd and the `connect` SUCCEEDS, the command is handed to a server that will never
        run it, and the client is hung up on instead of answered: tmux's own words for
        that are **`server exited unexpectedly`**, rc 1
        (:class:`AClientHungUpOnSaysTheServerExitedUnexpectedly` measures exactly that,
        deterministically).

        That is why it flaked rather than failed — nothing varies but whether a process
        that has already decided to exit gets there before the next `connect` lands, and
        on a loaded runner it does not. #839 caught three different methods across this
        class and its subclass doing it in two days, all in `setUp`, all on the fixture's
        own `new-session`, all cleared by a re-run at the same sha.

        **How many draws that is, counted rather than guessed.** A rebuilding client
        unlinks the stale socket and binds a new one, so a fresh inode at the socket path
        is one server birth. Instrumented on the version this fixed, one run of this
        module: **12 births** on this socket — one per test across this class and its
        subclass. Held, it is one per class, and each of those two is a client meeting no
        socket file at all (:meth:`_drop_the_socket_file`), which is a build and cannot be
        a race.

        **A construction, not a wait** (#650's rule): no `sleep`, no retry, no widened
        assertion. Every `new-session` here still has its return code asserted, and a
        `new-session` that fails for any real reason still fails this test with tmux's own
        sentence.

        Idempotent, and that is not tidiness either: `ATabClickedInsideCharactersOwnTmux`
        kills this server outright when a click hangs, which is a failure rather than a
        cleanup. The next test rebuilds from a socket path with nothing at it rather than
        inheriting that failure's race.
        """
        self.assertNotIn(self.KEEPER, (self.HERE, self.THERE),
                         "the keeper is a session on the socket under test, so a keeper "
                         "named after a workspace would make `cmd_launch`'s open-or-focus "
                         "gate focus it (`_live_sessions`) instead of opening anything")
        if self._tmux("has-session", "-t", self.KEEPER).returncode == 0:
            return
        self._tmux("kill-server")
        self._drop_the_socket_file()
        held = self._tmux("new-session", "-d", "-s", self.KEEPER,
                          "-x", "80", "-y", "24", "--", "cat")
        self.assertEqual(held.returncode, 0, held.stderr)

    def _sessions(self):
        """Every session standing on this class's server, sorted."""
        return sorted(s for s in self._tmux("list-sessions", "-F", "#{session_name}")
                      .stdout.split() if s)

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
        # **`bypass` is stubbed so this case can FAIL rather than vanish, and that is a
        # measurement fix rather than a convenience.** `bypass` calls `os.execvp`: on the
        # unmutated code it is never reached from here, but the deletion sweep's whole job
        # is to reach it — and when it did, the exec replaced this test runner with the
        # fake harness below, so the shard reported `no tests ran` and TWO mutations of
        # `_wants_attach` came back "not measured" instead of killed. Returning 127 keeps
        # the meaning exactly (a launch that bypassed the frame started no session, so
        # every assertion below fails) while leaving a process alive to say so.
        self.enterContext(mock.patch("charter.commands_frame.bypass", return_value=127))
        self.socket = self.SOCKET
        self.enterContext(mock.patch.object(commands_frame, "SOCKET", self.socket))
        # `kids` first: `_teardown` reads it, and a `setUp` that dies between the two
        # would turn one failure into an `AttributeError` in the cleanup that reports it.
        self.kids: list[tuple[int, int]] = []
        self.addCleanup(self._teardown)
        # The server, standing before this test's first `new-session` rather than built
        # by it. See `_hold_the_server` — this is the whole of #839.
        self._hold_the_server()
        # **Isolation, asserted where it can still be read.** Holding the server means the
        # sessions a test builds outlive its own `new-session`s unless something clears
        # them, and this class's session names are its WORKSPACE names — fixed, and
        # therefore refused a second time (`duplicate session`). `_teardown` clears every
        # session but the keeper; if it ever stopped doing so, the next test would meet a
        # `beta` full of the previous test's chats and `_live_chats` would read them as
        # this plane's. That is a far worse failure than a red `new-session`, so it is
        # caught here rather than left to be diagnosed.
        self.assertEqual(
            self._sessions(), [self.KEEPER],
            f"the test before this one left a session standing on {self.socket}, so this "
            f"one starts on a server that is not empty of it")

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
        # **The sessions this test built, and not the server under them** — #839. Every
        # session but the keeper, so the fixed `alpha`/`beta` names are free for the next
        # test and nothing of this one's is left for it to read, while the server itself
        # never becomes a process on its way out for the next `new-session` to connect to
        # (`_hold_the_server`). `tearDownClass` is what ends the server.
        self._tmux("kill-session", "-a", "-t", self.KEEPER)
        # Idempotent, so a test may run this by hand to measure what the next one meets
        # (`test_the_teardown_between_two_tests_leaves_the_server_standing`) without the
        # registered cleanup then killing a pty client twice.
        self.kids = []

    def _tmux(self, *a):
        return self._server(*a)

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

    def test_the_teardown_between_two_tests_leaves_the_server_standing(self):
        """**#839's fix, asserted rather than trusted, and red on the version it fixed.**

        The thing that flaked is not anything a test here asserts — it is the state one
        test hands the next one, which no assertion could see. So this runs the cleanup
        every test in this class ends with and looks at what is left: the same server
        process, holding the keeper and nothing else.

        Both halves fail on the version this fixed, and both are the defect. That
        `_teardown` ended in `kill-server`, so the pid read back afterwards is `''` — the
        next test's `new-session` was a client with no server to talk to, meeting the
        socket FILE that `kill-server` leaves behind and connecting to whatever was still
        at the other end of it. `_hold_the_server` is the argument at length; twelve
        server births per run of this module were twelve draws in that race.

        Inherited by :class:`ATabClickedInsideCharactersOwnTmux`, which shares this socket
        and this teardown and had two of #839's three reported failures.
        """
        born = self._tmux("display-message", "-p", "#{pid}").stdout.strip()
        self.assertTrue(born.isdigit(),
                        f"this class's server was not standing before its own test "
                        f"({born!r}) — `display-message` does not start one")
        self._teardown()
        self.assertEqual(
            self._tmux("display-message", "-p", "#{pid}").stdout.strip(), born,
            "the teardown took the server down with the sessions, so the next test on "
            "this socket has to rebuild one — and a client that connects to a server "
            "already on its way out is answered `server exited unexpectedly` (#839)")
        self.assertEqual(
            self._sessions(), [self.KEEPER],
            "the teardown left one of this test's own sessions standing, so the next "
            "test's `new-session` would be refused the name it needs")

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


class ATabClickedInsideCharactersOwnTmux(ARealTabOpensARealWorkspace):
    """#812: the same click, from the ``$TMUX`` a real one actually has.

    **The shape no case above had, and the gap #811 shipped through.** Every real-tmux
    test in this suite starts from a socket charter created and reaches it by the NAME it
    created it under, with ``$TMUX`` unset — which is what `tests/_envguard.py` leaves and
    what CI hands you. A click never happens there. It happens in a `charter frame-switch`
    started from a PANEL PANE of charter's own frame, and tmux exports ``$TMUX`` into
    every process it starts in a pane — as ``<socket path>,<server pid>,<session id>``,
    the socket ABSOLUTE. The operator's own read
    ``/private/tmp/tmux-<uid>/charter,18923,83``: charter's own private server, spelled the
    one way `is_operator_socket` used to call somebody else's.

    So this class is its parent with one thing changed — the environment the click runs in
    — and every test above is re-run through it. What that buys is stated rather than
    implied: on the code as it stood, `cmd_launch` read that variable, concluded it was a
    guest, and built the new workspace's chat as a `new-window` in the session the click
    came FROM. `test_the_workspace_opens_and_the_terminal_arrives_in_it` fails there
    (`list-sessions` never grows a `beta`), and the four cases below name what that cost:
    the workspace was not a session, the chat recorded the spelling rather than the
    socket, the way back was refused, and the round trip had to leave the chat it started
    in running.

    **The operator's report was the round trip, not the open.** *"I switched to `fleet`
    workspace, it switched with a new empty chat session, then when I want to switch back
    — I get an error."* :meth:`test_a_tab_in_the_opened_chat_takes_the_terminal_back` is
    that sentence, and it is the case that can only be written here: a chat opened from
    inside charter's own tmux is the only chat that ever recorded the absolute spelling,
    so it is the only one whose own tabs were refused.

    Verified on tmux 3.7c and at the 3.2 floor. Skipped where no tmux client can attach —
    a switch is a decision about an ATTACHED client, and CI has a tmux server but no pty
    it will hand one (`_attach`).
    """

    def setUp(self):
        super().setUp()
        # tmux is the authority on where its own socket is; `_tmuxsocket` computes the
        # same thing without asking. Both are here because the FIXTURE has to be built
        # before any server answers in the real case this stands in for, and because two
        # implementations of one rule that quietly drift apart is the defect underneath
        # #812 arriving in the suite instead of the product.
        self.socket_file = _tmuxsocket.socket_path(self.socket)
        self.assertEqual(
            self._tmux("display-message", "-p", "#{socket_path}").stdout.strip(),
            self.socket_file,
            "this tmux does not put its socket where charter computes it would")
        self.server_pid = self._tmux("display-message", "-p", "#{pid}").stdout.strip()
        # **The standing chat is marked the way a launcher marks one, and the round trip
        # is what needed it.** `_chat_option_argv` writes `@charter_chat` on every window
        # `cmd_launch` opens; this fixture builds the chat it starts in by hand and did
        # not. Nothing above notices — but a real `cmd_launch` reaps on its way in
        # (`_live_chats` reads exactly that option), so the open triggered by the FORWARD
        # click deleted `alpha.1`'s state directory as a chat with no live window, and the
        # way back could no longer prove the session it wanted was this plane's. The chat
        # left standing looks like a real one here because in production it always is.
        marked = self._tmux("set-option", "-w", "-t", self.pane,
                            commands_frame._CHAT_OPTION, self.fid)
        self.assertEqual(marked.returncode, 0, marked.stderr)
        # A plane a CHILD can find, not merely one this process believes in. The way back
        # re-dresses the chat it lands on (`_apply_arrangement`), and the chat this
        # fixture starts in was built by hand with no panels — so the round trip really
        # does split real `charter panel` processes, and each resolves its own plane. See
        # :meth:`_in_a_pane_of`.
        make_plane(self)

    def _in_a_pane_of(self, session: str):
        """The environment a `charter frame-switch` really runs in — the two variables
        that decide which tmux it is inside and which plane it belongs to.

        ``$TMUX`` is the subject: ``<socket path>,<server pid>,<session id>``, tmux's own
        spelling, exported into every process it starts in a pane.

        ``$CHARTER_ROOT`` is not, and it is here because leaving it out would have this
        fixture's panel children resolve a plane by walking up from the test runner's cwd
        — the checkout, which is the developer's REAL plane (`tests/_planeguard.py`
        refuses exactly that). A real one gets the answer from the frame's own cwd, which
        `_open_workspace` chdirs into and a switch does not; the pointer says the same
        thing across a process boundary. `make_plane` in `setUp` is what makes it a plane
        a child can actually find.
        """
        return {"TMUX": self._tmux_env(session), root.ENV_VAR: str(config.ROOT)}

    def _tmux_env(self, session: str) -> str:
        """``$TMUX`` exactly as tmux writes it into a pane of *session* on this server."""
        sid = self._tmux("display-message", "-p", "-t", session,
                         "#{session_id}").stdout.strip()
        self.assertTrue(sid.startswith("$"), sid)
        return f"{self.socket_file},{self.server_pid},{sid[1:]}"

    #: How long a click gets before this class calls it stuck. A click is `switch-client`
    #: plus, at most, one `cmd_launch` that does not attach — tenths of a second on both
    #: tmux versions, measured. Generous enough that a loaded CI runner is not called a
    #: hang, and finite because a click that never returns is exactly what regressing this
    #: looks like (see :meth:`_click_the_tab`).
    CLICK_DEADLINE = 30.0

    def _click_the_tab(self):
        """The click, on a thread, with a deadline — because the failure this class
        catches is a HANG and not a wrong answer.

        On the code #812 was filed against, `cmd_launch` read this `$TMUX`, decided it was
        a guest, and took `_launch_in_operator_tmux` — which does not install the
        `pane-died` hooks and instead stays awake for the whole life of the harness
        (`_wait_for_harness`). The harness here is `exec sleep 300`. So the click never
        returned at all: measured, this class run against `ff228a4` did not fail, it sat
        there until it was killed, which is not a pass, not a fail and not a report — and
        the deletion sweep kills a suite that hangs rather than recording the mutation.

        A daemon thread is what makes the deadline honest rather than cosmetic: the click
        that has not returned is left where it is, said so by name, and `_teardown`'s
        `kill-server` is what finally releases it.
        """
        out: list[BaseException | None] = []

        def go():
            try:
                with mock.patch.dict(os.environ, self._in_a_pane_of(self.HERE)):
                    ARealTabOpensARealWorkspace._click_the_tab(self)
                out.append(None)
            except BaseException as e:                  # noqa: BLE001 - reported below
                out.append(e)

        clicking = threading.Thread(target=go, daemon=True)
        clicking.start()
        clicking.join(self.CLICK_DEADLINE)
        if not out:
            # The server goes BEFORE the failure is raised, not in `_teardown` after it.
            # `_wait_for_harness` is watching a pane on it, so killing it is what lets the
            # stuck click unwind — and a click still inside `_open_workspace` when
            # `PersonaIso` removes this case's tmp tree takes the NEXT case down with an
            # `os.getcwd()` that has no directory to answer. Measured exactly that way
            # against `ff228a4`: one honest failure here, one unrelated `FileNotFoundError`
            # in the case after it.
            self._tmux("kill-server")
            clicking.join(10)
            self.fail(f"the click has not returned after {self.CLICK_DEADLINE:g}s — a "
                      f"workspace tab that opens its workspace as a window in the session "
                      f"it was clicked from blocks in `_wait_for_harness` for the life of "
                      f"the harness, and the switch never happens (#812)")
        if out[0] is not None:
            raise out[0]

    def _click_back(self, opened: str):
        """The second half of the operator's report: a tab in the chat they arrived at.

        Run from inside charter's tmux too, and naming the session it is standing in NOW
        — a panel in the chat they landed on is as much inside charter's server as the one
        they left, and a round trip that only set the variable on the way out would not be
        the trip that was reported.
        """
        with mock.patch.dict(os.environ, self._in_a_pane_of(self.THERE)):
            commands_frame._switch_client(opened, self.HERE,
                                          said=f"workspace → {self.HERE}")
        time.sleep(0.3)

    def _opened_chat(self) -> str:
        """The chat id the click's own launch claimed, read off the server."""
        chats = [c for c in self._tmux("list-windows", "-t", self.THERE,
                                       "-F", "#{@charter_chat}").stdout.split() if c]
        self.assertEqual(len(chats), 1, f"expected one chat in {self.THERE}: {chats}")
        return chats[0]

    def test_the_opened_workspace_is_a_session_of_its_own(self):
        """**The launch, not the switch, and this is what a fix to `is_operator_socket`
        alone would not have reached.** `cmd_launch` decided between "session on charter's
        own server" and "window in the session I am already in" on whether ``$TMUX``
        parsed at all. From a panel pane it always parses, so a tab click opened its chat
        inside the workspace it was leaving — measured before the fix as `list-sessions`
        reporting only `alpha` with `list-windows -a` reporting `[alpha] win=beta.1`."""
        self._attach(self.HERE)
        self._click_the_tab()
        self.assertIn(self.THERE,
                      self._tmux("list-sessions", "-F", "#{session_name}").stdout.split())
        self.assertEqual(
            [w for w in self._tmux("list-windows", "-t", self.HERE,
                                   "-F", "#{window_name}").stdout.split() if w],
            [self.fid],
            f"the chat for '{self.THERE}' was opened as a window inside '{self.HERE}'")

    def test_the_opened_chat_records_the_server_by_the_name_charter_launches_it_under(
            self):
        """The record every later tab reads (`state.frame_server`).

        Spelled by hand rather than compared to `commands_frame.SOCKET`, because the
        failure this pins is precisely a second, equal-but-different spelling of one
        socket: a round trip through the same constant would have agreed with itself
        while the operator was stuck."""
        self._attach(self.HERE)
        self._click_the_tab()
        recorded = state.frame_server(self._opened_chat())
        self.assertEqual(recorded, self.socket,
                         "the chat recorded a spelling of the socket, not the socket")
        self.assertNotEqual(recorded, self.socket_file)
        self.assertFalse(recorded.startswith("/"),
                         f"{recorded} is the absolute spelling #812 is about")

    def test_a_tab_in_the_opened_chat_takes_the_terminal_back(self):
        """**The operator's report, end to end.** Click `beta`, land there, click `alpha`,
        arrive — where the second click used to answer *"cannot switch: this chat is a
        window in your own tmux, where a workspace is not a session"* about a chat sitting
        on charter's private server.

        The way back runs with ``$TMUX`` set too, and to the session it is standing in
        now: a panel in the chat the operator arrived at is as much inside charter's tmux
        as the one they left, and a round trip that only set the variable on the way out
        would not be the trip that was reported."""
        client = self._attach(self.HERE)
        self._click_the_tab()
        self.assertEqual(self._clients(self.THERE), [client])
        opened = self._opened_chat()
        self._click_back(opened)
        self.assertEqual(self._clients(self.HERE), [client],
                         "the tab back did not take the terminal back")
        self.assertEqual(self._clients(self.THERE), [])
        self.assertNotIn("a window in your own tmux", state.notice(opened) or "")

    def test_the_chat_it_came_from_is_still_running_after_the_round_trip(self):
        """§4b's own requirement across both legs: switching keeps the other chat's
        harness alive, and a trip out and back must leave the pane it started in with the
        pid it started with."""
        self._attach(self.HERE)
        before = [p for p in self._panes() if p.startswith(self.pane + " ")]
        self._click_the_tab()
        opened = self._opened_chat()
        self._click_back(opened)
        self.assertEqual([p for p in self._panes() if p.startswith(self.pane + " ")],
                         before, "the round trip killed the chat it started in")


if __name__ == "__main__":
    unittest.main()
