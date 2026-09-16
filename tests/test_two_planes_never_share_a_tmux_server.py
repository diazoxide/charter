"""Two projects run at once, and neither one's frame is in the other's tmux — on real tmux.

**The report (ruling 46).** The operator ran charter in two projects at once. Their frames
mixed, and one project's harness profiles showed in the other. Every plane shared tmux socket
`charter`, a frame's session is named by its bare workspace, and a launch decided whether to
start a session or join one by that NAME — so a second plane opening `default`, a workspace
every plane has, became a window in the first plane's `default` session, under the first
plane's panels, hooks, key bindings and palette.

**Measured before the fix, with this module's first version.** Two throwaway planes, both on
workspace `default`, each launched with its own real `_launch` against one shared socket:
`list-panes -a` reported `$0 %0 | $0 %1` — both planes' harness panes in ONE session. That
is the defect as a reading, and every class below is the shape of it that the fix has to
make impossible.

**What is real and what is not.** Real tmux, the real `_launch`, a real
`charter frame-launch` in each pane resolving each plane's own `charter.local.toml`. The
harness is a recorder first on the pane's `PATH`, which writes down its environment. The
one seam is `tmuxctl.plane_socket`, aimed at a reapable socket per plane
(`tests._tmuxreap.name`) and still derived from WHICHEVER plane is asking — a map keyed on the
plane's state directory — so a launch that asked the wrong plane lands on the wrong socket
here exactly as it would in production. `tests._planeguard` refuses the real
`charter-plane-*` names, so nothing here can reach an operator's frame.

**The legacy class** stands a reapable socket in for `tmuxctl.LEGACY_SOCKET`: frames started
before the upgrade are found there by their record, or by having none.

**The operator's-tmux class** is the other place two planes still meet: charter launched
inside a tmux the operator runs opens each chat as a window on THEIR server, so two planes
there put two `default.1` windows side by side.
"""

from __future__ import annotations

import itertools
import json
import os
import shlex
import shutil
import subprocess
import sys
import time
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame, config
from charter.frame import reopen, state, tmuxctl

from tests import _envguard, _gitguard, _tmuxreap, _tmuxsocket, _ttyguard
from tests._isolation import (PersonaIso, approve_profile, make_plane,
                              no_background_refresh, wired_as_today)

_HAS_TMUX = shutil.which("tmux") is not None

#: A fresh socket per CASE: a server told to exit still accepts on its socket for a few
#: milliseconds, and the next case's `new-session` draws that race
#: (`test_a_background_chat_really_starts_on_its_brief.py` records the CI run).
_SERVERS = itertools.count()

#: This checkout, for the `$PYTHONPATH` each pane's own launcher imports charter from.
_REPO_ROOT = Path(__file__).resolve().parents[1]

WS = "default"
CHAT = "default.1"

_WIRED = None


def setUpModule():
    global _WIRED
    _WIRED = wired_as_today()
    _WIRED.start()


def tearDownModule():
    if _WIRED is not None:
        _WIRED.stop()


def _eventually(predicate, timeout: float = 20.0) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(0.05)
    return bool(predicate())


def _tmux(socket: str, *args: str) -> subprocess.CompletedProcess:
    # `-u` for #984's reason: a client with no UTF-8 locale gets a format's TAB back as `_`.
    return subprocess.run(["tmux", "-u", "-L", socket, *args], capture_output=True,
                          text=True, timeout=20)


def _kill(socket: str) -> None:
    subprocess.run(["tmux", "-L", socket, "kill-server"], capture_output=True, timeout=20)
    try:
        os.unlink(_tmuxsocket.socket_path(socket))
    except OSError:
        pass


class _TwoPlanes(PersonaIso):
    """Plane A at the case's own root, plane B beside it, and a map from each plane's state
    directory to the reapable socket that stands in for its own server."""

    SLUG = "two-planes"

    def setUp(self) -> None:
        super().setUp()
        v = tmuxctl.version()
        if v is None or v < tmuxctl.FLOOR:
            self.skipTest(f"the frame's floor is tmux {tmuxctl.FLOOR[0]}.{tmuxctl.FLOOR[1]}"
                          f"; this machine has {v}")
        make_plane(self)
        no_background_refresh(self)
        # A launch with no terminal of its own: `attach=False`, the way a tab or a handoff
        # drives it.
        _ttyguard.no_terminal()
        self.a_root = self.tmp
        self.b_root = self.tmp.parent / f"{self.tmp.name}-plane-b"
        self.b_root.mkdir()
        (self.b_root / "charter.toml").write_text("schema = 1\n")
        self.addCleanup(shutil.rmtree, self.b_root, True)

        n = next(_SERVERS)
        self.sockets: dict[str, str] = {}
        for label, root in (("a", self.a_root), ("b", self.b_root)):
            config.use(root)
            self.sockets[os.path.realpath(str(config.STATE_DIR))] = _tmuxreap.name(
                f"{self.SLUG}-{label}-{n}")
        config.use(self.a_root)
        for socket in self.sockets.values():
            self.addCleanup(_kill, socket)
        self.enterContext(mock.patch.object(
            tmuxctl, "plane_socket",
            side_effect=lambda: self.sockets[os.path.realpath(str(config.STATE_DIR))]))
        # A detached `charter frame-gather` outlives the case and is refused by
        # `tests/_planeguard`, and no panel is what this file is about.
        self.enterContext(mock.patch.object(commands_frame, "_spawn_gather"))
        self.enterContext(mock.patch.object(commands_frame, "_drawable_slots",
                                            return_value=[]))

        self.records = self.tmp / "records"
        self.records.mkdir()
        bindir = self.tmp / "bin"
        bindir.mkdir()
        (bindir / "claude").write_text(
            f"#!{sys.executable}\n"
            "import json, os, sys, time\n"
            # The wiring probe (ruling 10) is answered, not recorded.
            "if sys.argv[1:3] == ['plugin', 'list']:\n"
            "    json.dump([{'id': 'charter@charter', 'scope': 'user', 'enabled': True,\n"
            "                'installedAt': '2026-09-12T00:00:00Z'}], sys.stdout)\n"
            "    sys.exit(0)\n"
            "out = os.path.join(os.environ['RECORD_DIR'], f'{os.getpid()}.json')\n"
            "with open(out + '.tmp', 'w') as f:\n"
            "    json.dump({'env': dict(os.environ), 'pid': os.getpid(),\n"
            "               'argv': sys.argv[1:]}, f)\n"
            "os.replace(out + '.tmp', out)\n"
            "time.sleep(300)\n")
        (bindir / "claude").chmod(0o755)
        home = self.tmp / "home"
        home.mkdir()
        self.enterContext(mock.patch.dict(os.environ, {
            "PATH": f"{bindir}{os.pathsep}{os.environ.get('PATH', '')}",
            "RECORD_DIR": str(self.records),
            "HOME": str(home),
            "GIT_CEILING_DIRECTORIES": str(Path(self.tmp).resolve().parent),
            "PYTHONPATH": os.pathsep.join(
                [str(_REPO_ROOT), os.environ.get("PYTHONPATH", "")]).rstrip(os.pathsep),
            **_gitguard.environment(),
            **_envguard.stated(),
        }, clear=True))
        # **A different profile in each plane's own `charter.local.toml`**, so a pane that
        # read the other plane's file would refuse a profile it does not have — and one that
        # read its own carries that plane's `CLAUDE_CONFIG_DIR` into the harness.
        for label, root in (("a", self.a_root), ("b", self.b_root)):
            config.use(root)
            (root / "charter.local.toml").write_text(
                f'[harness.claude-{label}]\nkind = "claude"\ncommand = ["claude"]\n'
                f'env = {{ CLAUDE_CONFIG_DIR = "{root / ("cfg-" + label)}" }}\n')
            (root / ".gitignore").write_text("/charter.local.toml\n")
            subprocess.run(["git", "-C", str(root), "init", "-q"], check=True,
                           capture_output=True, env={**os.environ, **_gitguard.environment()})
            (config.WORKSPACES_DIR / WS).mkdir(parents=True, exist_ok=True)
            approve_profile(self, f"claude-{label}")
        config.use(self.a_root)

    # -- helpers --------------------------------------------------------------------------

    def _in(self, root: Path) -> None:
        config.use(root)

    def _socket_of(self, root: Path) -> str:
        self._in(root)
        return tmuxctl.plane_socket()

    def _launch_in(self, root: Path, label: str, chat: str = CHAT) -> str:
        """Plane *root*'s own real `_launch` of its own profile; the chat's harness pane."""
        self._in(root)
        with mock.patch.dict(os.environ, {"CHARTER_ROOT": str(root)}):
            rc = commands_frame._launch(SimpleNamespace(
                harness="claude", profile=f"claude-{label}", rest=[], no_frame=False,
                workspace=WS, pick=False, attach=False, size=(120, 40)))
        self.assertEqual(rc, 0, f"plane {label}'s launch failed")
        pane = state.harness_pane(chat)
        self.assertTrue(pane, f"plane {label}'s launch recorded no harness pane")
        return pane

    def _harness_in(self, socket: str, pane: str) -> dict:
        """What the recorder in *pane* wrote down, found by that pane's own pid."""
        pid = _tmux(socket, "display-message", "-p", "-t", pane, "#{pane_pid}").stdout.strip()
        record = self.records / f"{pid}.json"
        self.assertTrue(_eventually(record.is_file),
                        f"no harness ran in {pane} on {socket} (pid {pid!r})")
        return json.loads(record.read_text())

    def _alive(self, socket: str, pane: str) -> bool:
        out = _tmux(socket, "display-message", "-p", "-t", pane, "#{pane_dead}")
        return out.returncode == 0 and out.stdout.strip() == "0"

    def _both(self) -> tuple[str, str, str, str]:
        a_pane = self._launch_in(self.a_root, "a")
        b_pane = self._launch_in(self.b_root, "b")
        return (self._socket_of(self.a_root), a_pane, self._socket_of(self.b_root), b_pane)


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class TwoPlanesLaunchedOneAfterTheOther(_TwoPlanes, unittest.TestCase):
    """Both planes open `default`; each is a session on its own server and nothing else."""

    def test_each_plane_holds_its_own_default_session_on_a_server_of_its_own(self):
        a_socket, a_pane, b_socket, b_pane = self._both()

        self.assertNotEqual(a_socket, b_socket)
        for socket, pane in ((a_socket, a_pane), (b_socket, b_pane)):
            with self.subTest(socket=socket):
                self.assertEqual(_tmux(socket, "list-sessions", "-F",
                                       "#{session_name}").stdout.split(), [WS])
                self.assertEqual(_tmux(socket, "list-panes", "-a", "-F",
                                       "#{pane_id}").stdout.split(), [pane])

    def test_no_window_of_one_plane_is_in_the_other_planes_server(self):
        """The reading the defect was: a window whose plane marker names the OTHER plane.
        One window per server, carrying its own plane's chat and its own plane's marker."""
        a_socket, _a, b_socket, _b = self._both()

        for socket, root in ((a_socket, self.a_root), (b_socket, self.b_root)):
            with self.subTest(root=root):
                self._in(root)
                rows = _tmux(socket, "list-windows", "-a", "-F",
                             "#{@charter_chat}\t#{@charter_plane}").stdout.splitlines()
                self.assertEqual(rows, [f"{CHAT}\t{config.STATE_DIR}"])

    def test_each_pane_runs_its_own_planes_root_and_its_own_planes_profile(self):
        """The half the operator SAW: one project's profiles in the other. Each pane's
        launcher read its own plane's `charter.local.toml`, so each harness carries its own
        plane's root, profile name and profile environment."""
        a_socket, a_pane, b_socket, b_pane = self._both()

        for socket, pane, root, label in ((a_socket, a_pane, self.a_root, "a"),
                                          (b_socket, b_pane, self.b_root, "b")):
            with self.subTest(plane=label):
                env = self._harness_in(socket, pane)["env"]
                self.assertEqual(env.get("CHARTER_ROOT"), str(root))
                self.assertEqual(env.get("CHARTER_HARNESS_PROFILE"), f"claude-{label}")
                self.assertEqual(env.get("CLAUDE_CONFIG_DIR"), str(root / f"cfg-{label}"))
                self.assertEqual(env.get("CHARTER_SESSION_ID"), CHAT)

    def test_closing_one_planes_chat_leaves_the_other_planes_frame_standing(self):
        a_socket, a_pane, b_socket, b_pane = self._both()

        self._in(self.a_root)
        self.assertEqual(commands_frame.cmd_close(SimpleNamespace(chat="", chat_id=CHAT)), 0)

        self.assertTrue(_eventually(lambda: not self._alive(a_socket, a_pane)),
                        "plane A's own chat was not stopped")
        self.assertTrue(self._alive(b_socket, b_pane), "plane B's frame was stopped")

    def test_quitting_one_plane_leaves_the_other_planes_frame_standing(self):
        a_socket, a_pane, b_socket, b_pane = self._both()

        self._in(self.a_root)
        self.assertEqual(commands_frame.cmd_quit(SimpleNamespace(chat=CHAT)), 0)

        self.assertTrue(_eventually(lambda: not self._alive(a_socket, a_pane)),
                        "plane A's own chat was not stopped")
        self.assertTrue(self._alive(b_socket, b_pane), "plane B's frame was stopped")
        self.assertEqual([c.chat for c in reopen.read().all_chats()], [CHAT])
        self._in(self.b_root)
        self.assertIsNone(reopen.read(), "plane B was never quit and has nothing recorded")


class _OnTheSharedServer(_TwoPlanes):
    """`_TwoPlanes`, plus a reapable socket standing in for `tmuxctl.LEGACY_SOCKET` and a way
    to plant a chat on it the way a charter from before ruling 46 left one."""

    def setUp(self) -> None:
        super().setUp()
        self.legacy = _tmuxreap.name(f"legacy-shared-{next(_SERVERS)}")
        self.addCleanup(_kill, self.legacy)
        self.enterContext(mock.patch.object(tmuxctl, "LEGACY_SOCKET", self.legacy))

    def _old_chat(self, fid: str, *, first: bool, record: bool = True,
                  mark: bool = False, profile: str = "") -> str:
        """One chat the way a pre-upgrade launcher left it, for whichever plane `config` is
        on: a window on the shared server with its chat option — in a NEW `default` session
        when *first*, else as a window joining the one already there, which is exactly what
        a second plane's launch did — and a directory that records that server, or none.

        *mark* writes the session's plane marker through production's own builder, the way
        the launch that CREATED a session did; a joining launch never did."""
        cmd = "exec cat"
        if first:
            pane = _tmux(self.legacy, "new-session", "-d", "-s", WS, "-P", "-F",
                         "#{pane_id}", "-x", "80", "-y", "24", "sh", "-c", cmd).stdout.strip()
        else:
            pane = _tmux(self.legacy, "new-window", "-d", "-t", WS, "-P", "-F",
                         "#{pane_id}", "sh", "-c", cmd).stdout.strip()
        self.assertTrue(pane.startswith("%"), pane)
        _tmux(self.legacy, "set-option", "-w", "-t", pane, commands_frame._CHAT_OPTION, fid)
        if mark:
            argv = commands_frame._plane_option_argv(socket=self.legacy, harness_pane=pane)
            ran = subprocess.run(argv, capture_output=True, text=True, timeout=20)
            self.assertEqual(ran.returncode, 0, ran.stderr)
        state.frame_dir(fid, create=True)
        if record:
            state.record_server(fid, self.legacy)
        state.record_workspace(fid, WS)
        state.record_harness_pane(fid, pane)
        state.record_cwd(fid, str(config.ROOT))
        state.record_identity(fid, {"CHARTER_HARNESS": "claude-code",
                                    "CHARTER_WORKSPACE": "", "CHARTER_PERSONA": ""})
        if profile:
            state.record_profile(fid, profile)
        return pane


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class AFrameStartedBeforeTheUpgrade(_OnTheSharedServer, unittest.TestCase):
    """Ruling 46's legacy rule: a chat recorded on the shared socket, or recording no server
    at all, is found there by close, quit and the launch's own gates — while every NEW chat
    starts on the plane's own server."""

    SLUG = "legacy-planes"

    def test_a_chat_recorded_on_the_shared_socket_is_closed_there(self):
        pane = self._old_chat("default.1", first=True, record=True)
        keep = self._old_chat("default.2", first=False, record=True)

        self.assertEqual(commands_frame.cmd_close(
            SimpleNamespace(chat="", chat_id="default.1")), 0)

        self.assertTrue(_eventually(lambda: not self._alive(self.legacy, pane)))
        self.assertTrue(self._alive(self.legacy, keep))

    def test_a_chat_with_no_record_is_closed_on_the_shared_socket(self):
        pane = self._old_chat("default.1", first=True, record=False)
        self.assertIsNone(state.frame_server("default.1"))

        self.assertEqual(commands_frame.cmd_close(
            SimpleNamespace(chat="", chat_id="default.1")), 0)

        self.assertTrue(_eventually(lambda: not self._alive(self.legacy, pane)))

    def test_a_quit_records_and_stops_both_kinds_on_the_shared_socket(self):
        recorded = self._old_chat("default.1", first=True, record=True)
        unrecorded = self._old_chat("default.2", first=False, record=False)

        self.assertEqual(commands_frame.cmd_quit(SimpleNamespace(chat="")), 0)

        self.assertEqual(sorted(c.chat for c in reopen.read().all_chats()),
                         ["default.1", "default.2"])
        self.assertTrue(_eventually(lambda: not self._alive(self.legacy, recorded)))
        self.assertTrue(_eventually(lambda: not self._alive(self.legacy, unrecorded)))

    def test_a_reopen_while_an_old_frame_runs_is_refused_rather_than_doubled(self):
        """The plane is live on the shared server even though its own server has nothing,
        so `charter reopen` would put a second copy of the chat beside the first."""
        self._old_chat("default.1", first=True, record=False)
        reopen.write([reopen.Frame(workspace=WS, chats=(
            reopen.Chat(chat="default.1", workspace=WS, persona="", harness="claude-code",
                        cwd="", resume="", transcript="", active=True),))], focus=WS)

        said: list[str] = []
        with mock.patch("sys.stdout.isatty", return_value=True), \
                mock.patch.object(commands_frame.util, "err", side_effect=said.append), \
                mock.patch.object(commands_frame, "cmd_launch") as launch:
            self.assertEqual(commands_frame.cmd_reopen(SimpleNamespace()), 1)
        launch.assert_not_called()
        self.assertTrue(any("already running" in s for s in said), said)

    def test_a_reopen_with_one_dead_server_still_sees_the_chat_live_on_the_other(self):
        """#1088: the plane's own server killed by hand, a chat still running on the shared
        one. The dead server says nothing about the live chat, so the reopen is refused."""
        self._old_chat("default.1", first=True, record=True)
        own = tmuxctl.plane_socket()
        _tmux(own, "new-session", "-d", "-s", WS, "sh", "-c", "exec cat")
        state.frame_dir("default.2", create=True)
        state.record_server("default.2", own)
        state.record_workspace("default.2", WS)
        self.assertEqual(_tmux(own, "kill-server").returncode, 0)
        reopen.write([reopen.Frame(workspace=WS, chats=tuple(
            reopen.Chat(chat=fid, workspace=WS, persona="", harness="claude-code",
                        cwd="", resume="", transcript="", active=fid == "default.1")
            for fid in ("default.1", "default.2")))], focus=WS)

        said: list[str] = []
        with mock.patch("sys.stdout.isatty", return_value=True), \
                mock.patch.object(commands_frame.util, "err", side_effect=said.append), \
                mock.patch.object(commands_frame, "cmd_launch") as launch:
            self.assertEqual(commands_frame.cmd_reopen(SimpleNamespace()), 1)
        launch.assert_not_called()
        self.assertTrue(any("already running" in s for s in said), said)

    def test_a_new_launch_starts_on_the_planes_own_server_and_leaves_the_old_one(self):
        """Nothing new ever starts on the shared server — a session there is joined by its
        bare name, which is the defect — and the old frame keeps running beside it."""
        old = self._old_chat("default.1", first=True, record=True)

        pane = self._launch_in(self.a_root, "a", chat="default.2")
        own = tmuxctl.plane_socket()

        self.assertEqual(state.frame_server("default.2"), own)
        self.assertEqual(_tmux(own, "list-panes", "-a", "-F", "#{pane_id}").stdout.split(),
                         [pane])
        self.assertEqual(_tmux(self.legacy, "list-panes", "-a", "-F",
                               "#{pane_id}").stdout.split(), [old])
        self.assertTrue(self._alive(self.legacy, old))

    def test_a_launch_reaps_an_old_chat_that_has_ended_on_the_shared_server(self):
        """`state.reap` leaves a directory to a reap of its OWN server, and nothing asks the
        shared one but this — without it an old chat's state would outlive it for good."""
        pane = self._old_chat("default.7", first=True, record=False)
        _tmux(self.legacy, "kill-pane", "-t", pane)
        self.assertTrue(state.frame_dir("default.7").is_dir())

        self._launch_in(self.a_root, "a")

        self.assertFalse(state.frame_dir("default.7").exists(),
                         "the ended chat's directory outlived it")


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class TwoPlanesMixedInOneSessionBeforeTheUpgrade(_OnTheSharedServer, unittest.TestCase):
    """**The reporter's own machine, as the upgrade finds it.** Before ruling 46, plane A
    opened `default` on the shared server and plane B's `default` joined that session as a
    window. Both planes' first chat is `default.1`, and the session carries plane A's marker
    — the launch that created it wrote it, and B's joining launch did not.

    Two readings could tell whose window is whose there, and one of them is wrong in exactly
    this state: the SESSION marker names A for B's window too. What cannot be wrong is the
    pane — a server-minted id each plane's own launcher wrote down for its own chat."""

    SLUG = "mixed-planes"

    def setUp(self) -> None:
        super().setUp()
        self._in(self.a_root)
        self.a_pane = self._old_chat(CHAT, first=True, mark=True, profile="claude-a")
        state.record_harness_session(CHAT, "conv-plane-a")
        # The transcript its harness named, on disk: a restore asks for the conversation back
        # only where that file exists (#1101, `leave.conversation_exists`).
        (self.a_root / "conv-plane-a.jsonl").write_text("{}\n")
        state.record_conversation(CHAT, str(self.a_root / "conv-plane-a.jsonl"))
        self._in(self.b_root)
        self.b_pane = self._old_chat(CHAT, first=False, profile="claude-b")
        state.record_harness_session(CHAT, "conv-plane-b")
        self.assertEqual(
            _tmux(self.legacy, "list-panes", "-a", "-F", "#{session_id}").stdout.split(),
            ["$0", "$0"], "the fixture is not one mixed session")

    def test_the_plane_that_made_the_session_quits_its_own_window_and_not_the_other(self):
        """Both windows carry `default.1` and the listing names B's second, so a quit that
        aimed by chat id alone — last row wins — killed plane B's window and left A's."""
        self._in(self.a_root)
        self.assertEqual(commands_frame.cmd_quit(SimpleNamespace(chat=CHAT)), 0)

        self.assertTrue(_eventually(lambda: not self._alive(self.legacy, self.a_pane)),
                        "plane A's own window was not stopped")
        self.assertTrue(self._alive(self.legacy, self.b_pane),
                        "plane A's quit stopped plane B's window of the same id")

    def test_the_plane_that_joined_the_session_quits_its_own_window_there(self):
        """The session marker names plane A, so a veto on the marker alone skipped every one
        of plane B's windows: B's quit recorded its chat, stopped nothing, and said it had.

        Driven as the way out is spelled — `charter frame-quit` typed in plane B's own
        project, no chat of its own to stand in — because F2 on that shared server can
        belong to plane A."""
        self._in(self.b_root)
        self.assertEqual(commands_frame.cmd_quit(SimpleNamespace(chat="")), 0)

        self.assertTrue(_eventually(lambda: not self._alive(self.legacy, self.b_pane)),
                        "plane B's own window was not stopped")
        self.assertTrue(self._alive(self.legacy, self.a_pane),
                        "plane B's quit stopped plane A's window")
        self.assertEqual([c.chat for c in reopen.read().all_chats()], [CHAT])

    def test_the_plane_that_joined_the_session_closes_its_own_window_there(self):
        self._in(self.b_root)
        self.assertEqual(commands_frame.cmd_close(SimpleNamespace(chat="", chat_id=CHAT)), 0)

        self.assertTrue(_eventually(lambda: not self._alive(self.legacy, self.b_pane)))
        self.assertTrue(self._alive(self.legacy, self.a_pane))

    def test_the_documented_listing_shows_the_mixed_session_as_two_windows_of_one_chat(self):
        """`docs/frame.md` and the news entry hand the operator one command to see a mixed
        session with. It is run here off the docs' own line, against this fixture, so the
        docs cannot drift from what tmux prints. Per WINDOW and not per pane: a pane listing
        prints every panel as a row and its plane column reads the same, so it cannot show
        the one thing a mixed session is — two windows of one chat id under one session.
        The plane column is the session's, plane A's, on both rows."""
        docs = (_REPO_ROOT / "docs" / "frame.md").read_text()
        # By slug, not by version: a release stamps this entry's filename.
        news = next((_REPO_ROOT / "docs" / "news")
                    .glob("*-two-projects-never-share-a-tmux-server.md")).read_text()
        lines = [ln.strip() for ln in docs.splitlines()
                 if ln.strip().startswith("tmux -L charter list-")]
        self.assertEqual(len(lines), 1, lines)
        self.assertIn(lines[0], news, "the news entry spells a different command")
        argv = shlex.split(lines[0])
        argv[argv.index("charter")] = self.legacy

        out = subprocess.run(argv, capture_output=True, text=True, timeout=20)

        self.assertEqual(out.returncode, 0, out.stderr)
        rows = [ln.split(" ", 3) for ln in out.stdout.splitlines()]
        self.assertEqual(len(rows), 2, out.stdout)
        self.assertEqual({r[0] for r in rows}, {WS}, "one session")
        self.assertEqual(len({r[1] for r in rows}), 2, "two windows")
        self.assertEqual({r[2] for r in rows}, {CHAT}, "of one chat id")
        self._in(self.a_root)
        self.assertEqual({r[3] for r in rows}, {str(config.STATE_DIR)},
                         "the plane column is the session's, and names plane A for both")

    def test_charter_after_a_quit_restores_that_planes_chats_while_the_other_plane_runs(self):
        """**The upgrade route, end to end.** Plane A quits its old frame and types
        `charter`. Plane B's `default.1` is still live on the shared server — a chat of the
        same id — and must neither keep plane A's dead chat looking alive (which skipped the
        restore, opened a fresh chat and recorded over the quit, losing the resume id) nor
        be touched."""
        self._in(self.a_root)
        self.assertEqual(commands_frame.cmd_quit(SimpleNamespace(chat=CHAT)), 0)
        self.assertTrue(_eventually(lambda: not self._alive(self.legacy, self.a_pane)))

        with mock.patch.dict(os.environ, {"CHARTER_ROOT": str(self.a_root)}), \
                mock.patch("sys.stdout.isatty", return_value=True), \
                mock.patch("sys.stdin.isatty", return_value=False), \
                mock.patch.object(commands_frame, "_attach_after_reopen",
                                  return_value=0) as attached:
            rc = commands_frame.cmd_launch(SimpleNamespace(
                harness="claude", rest=[], no_frame=False, workspace=WS, pick=False,
                fresh=False))

        self.assertEqual(rc, 0)
        attached.assert_called_once()
        own = tmuxctl.plane_socket()
        panes = _tmux(own, "list-panes", "-a", "-F", "#{pane_id}").stdout.split()
        self.assertEqual(len(panes), 1, "the plane was not restored onto its own server")
        argv = self._harness_in(own, panes[0])["argv"]
        self.assertIn("--resume", argv)
        self.assertEqual(argv[argv.index("--resume") + 1], "conv-plane-a")
        self.assertTrue(self._alive(self.legacy, self.b_pane),
                        "restoring plane A touched plane B's old frame")


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class TwoPlanesInsideOneOperatorTmux(_TwoPlanes, unittest.TestCase):
    """Charter launched inside a tmux the operator runs opens each chat as a window on THEIR
    server — so two planes there are two `default.1` windows in one server, told apart only
    by the plane marker charter writes on its own window."""

    SLUG = "operator-planes"

    def setUp(self) -> None:
        super().setUp()
        self.operator = _tmuxreap.name(f"operator-tmux-{next(_SERVERS)}")
        self.addCleanup(_kill, self.operator)
        out = _tmux(self.operator, "new-session", "-d", "-s", "work", "-x", "120", "-y",
                    "40", "sh", "-c", "exec cat")
        self.assertEqual(out.returncode, 0, out.stderr)
        self.path = _tmuxsocket.socket_path(self.operator)

    def _chat_window(self, root: Path) -> str:
        """One chat window the way `_launch_in_operator_tmux` marks it — through the same
        argv builders — with this plane's records spelling the server as `$TMUX` does."""
        self._in(root)
        out = _tmux(self.operator, "new-window", "-d", "-t", "work", "-P", "-F",
                    "#{pane_id}", "sh", "-c", "exec cat")
        pane = out.stdout.strip()
        self.assertTrue(pane.startswith("%"), out.stderr)
        for argv in (commands_frame._chat_option_argv(socket=self.path, harness_pane=pane,
                                                      chat=CHAT),
                     commands_frame._plane_option_argv(socket=self.path, harness_pane=pane,
                                                       window=True)):
            ran = subprocess.run(argv, capture_output=True, text=True, timeout=20)
            self.assertEqual(ran.returncode, 0, ran.stderr)
        state.frame_dir(CHAT, create=True)
        state.record_server(CHAT, self.path)
        state.record_workspace(CHAT, WS)
        state.record_harness_pane(CHAT, pane)
        state.record_cwd(CHAT, str(root))
        state.record_identity(CHAT, {"CHARTER_HARNESS": "claude-code",
                                     "CHARTER_WORKSPACE": "", "CHARTER_PERSONA": ""})
        return pane

    def test_the_marker_is_on_the_window_and_the_operators_session_is_untouched(self):
        self._chat_window(self.a_root)
        session = _tmux(self.operator, "show-options", "-t", "work").stdout
        self.assertNotIn(commands_frame._PLANE_OPTION, session,
                         "a session option was written on the operator's own session")

    def test_closing_one_planes_chat_leaves_the_other_planes_window_of_the_same_id(self):
        """#933's hazard where it still lives: `_stop_chats` aims `kill-window` at one window
        per chat id, last row wins — and plane B's `default.1` is listed after plane A's."""
        a_pane = self._chat_window(self.a_root)
        b_pane = self._chat_window(self.b_root)

        self._in(self.a_root)
        self.assertEqual(commands_frame.cmd_close(SimpleNamespace(chat="", chat_id=CHAT)), 0)

        self.assertTrue(_eventually(lambda: not self._alive(self.operator, a_pane)),
                        "plane A's own window was not closed")
        self.assertTrue(self._alive(self.operator, b_pane),
                        "plane A's close killed plane B's window of the same id")

    def test_quitting_one_plane_leaves_the_other_planes_window_of_the_same_id(self):
        a_pane = self._chat_window(self.a_root)
        b_pane = self._chat_window(self.b_root)

        self._in(self.a_root)
        commands_frame.cmd_quit(SimpleNamespace(chat=CHAT))

        self.assertTrue(_eventually(lambda: not self._alive(self.operator, a_pane)))
        self.assertTrue(self._alive(self.operator, b_pane),
                        "plane A's quit killed plane B's window of the same id")

    def test_a_stale_record_matching_a_live_pane_here_is_vetoed_by_the_window_marker(self):
        """**The operator's tmux restarts and pane ids start again.** Plane A holds a chat
        directory from before the restart — this server, `%1` — and plane B opens the first
        chat in the fresh tmux, which gets `%1`. Matched on the pane alone, plane A's quit
        would kill plane B's window and record plane B's chat as its own. The marker charter
        writes on its OWN window says whose it is, and here it is the reading that decides:
        a window marker is written per chat, so unlike a mixed legacy session's SESSION
        marker it can never name the wrong plane for the window it is on."""
        b_pane = self._chat_window(self.b_root)
        self._in(self.a_root)
        state.frame_dir(CHAT, create=True)
        state.record_server(CHAT, self.path)
        state.record_workspace(CHAT, WS)
        state.record_harness_pane(CHAT, b_pane)
        state.record_identity(CHAT, {"CHARTER_HARNESS": "claude-code",
                                     "CHARTER_WORKSPACE": "", "CHARTER_PERSONA": ""})

        self.assertEqual(commands_frame.cmd_quit(SimpleNamespace(chat="")), 0)

        self.assertTrue(self._alive(self.operator, b_pane),
                        "plane A's quit killed plane B's window on a recycled pane id")
        recorded = reopen.read()
        for chat in (recorded.all_chats() if recorded else ()):
            self.assertEqual(chat.transcript, "",
                             "plane A's quit captured plane B's pane as its own chat")

    def test_a_stale_record_matching_a_live_pane_here_closes_nothing_of_the_others(self):
        b_pane = self._chat_window(self.b_root)
        self._in(self.a_root)
        state.frame_dir(CHAT, create=True)
        state.record_server(CHAT, self.path)
        state.record_workspace(CHAT, WS)
        state.record_harness_pane(CHAT, b_pane)

        commands_frame.cmd_close(SimpleNamespace(chat="", chat_id=CHAT))

        self.assertTrue(self._alive(self.operator, b_pane),
                        "plane A's close killed plane B's window on a recycled pane id")

    def test_a_pane_is_proven_by_its_own_pid_whichever_plane_holds_the_same_chat_id(self):
        """`launcher.framed_chat` and `_close_the_cancelled_chat` prove a chat by the pid of
        the process asking, and `@charter_chat` alone would not: both windows carry
        `default.1`. A pid is one pane on the machine."""
        a_pane = self._chat_window(self.a_root)
        b_pane = self._chat_window(self.b_root)
        for pane in (a_pane, b_pane):
            with self.subTest(pane=pane):
                pid = int(_tmux(self.operator, "display-message", "-p", "-t", pane,
                                "#{pane_pid}").stdout.strip())
                row = tmuxctl.live_pane_by_pid(self.path, pid)
                self.assertIsNotNone(row)
                self.assertEqual((row.pane, row.chat), (pane, CHAT))


if __name__ == "__main__":
    unittest.main()
