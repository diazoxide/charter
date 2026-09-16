"""A real chat pane starts as charter's launcher and becomes the harness — on real tmux.

The claim this file holds is the one the whole feature rests on: every chat pane's first
process is `charter frame-launch`, and after its `exec` the pane is indistinguishable from
one the harness was started in directly. The pane id charter recorded, the exit code, the
`pane-died` hooks, `$TMUX_PANE` and the liveness list all answer what they answered before
profiles existed — measured on tmux 3.7c and at the 3.2 floor, on charter's own server and
in an operator's, before any of it was built
(`workspaces/harness-profiles/refs/task2-measure/`, and `docs/frame.md`'s *How a harness
starts*). This is that measurement as a test, on whatever tmux the runner has.

**And the two halves of ruling 42**, which the measurement is the reason for: a refusal the
pane prints and exits on is read by nobody — the eager dead-status ask completes 6-14 ms
after the start while the launcher's first line runs at 19-22 ms, and the teardown hook
then kills the window. So an attended pane shows its refusal and WAITS for a key, and an
unattended one records it where the launch that opened the chat reads it back.

**The recorder is the harness here**, first on the `PATH` the tmux client is started with:
a launcher resolves the profile in its own process, so patching `ClaudeCodeHarness.binary`
in this one would not reach the pane at all.
"""

from __future__ import annotations

import itertools
import json
import os
import shutil
import subprocess
import sys
import time
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame, config
from charter.frame import launcher, state, tmuxctl

from tests import _envguard, _gitguard, _tmuxreap, _tmuxsocket, _ttyguard
from tests._isolation import (PersonaIso, approve_profile, assert_approved,
                              declare_profiles, make_plane, no_background_refresh,
                              wired_as_today)


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


_HAS_TMUX = shutil.which("tmux") is not None

#: A fresh socket per CASE, never per class: a server told to exit is still accepting on
#: its socket for a few milliseconds, and the next case's `new-session` draws that race
#: (`test_a_background_chat_really_starts_on_its_brief.py` records the CI run that showed
#: it).
_SERVERS = itertools.count()

#: This checkout, for the `$PYTHONPATH` the pane's own launcher needs. `-P` keeps the
#: child's cwd off `sys.path` (#390) and its cwd is a workspace directory, so without this
#: `python -P -m charter frame-launch` cannot import the charter under test — it would exit
#: at once and take its window with it, which looks exactly like a launch that never
#: happened.
_REPO_ROOT = Path(__file__).resolve().parents[1]

#: One declared profile whose command really resolves here — the recorder on this fixture's
#: own `PATH` is `claude` — so a pane that starts it really becomes it.
_LOCAL = """
[harness.claude-work]
kind = "claude"
command = ["claude"]
env = { CLAUDE_CONFIG_DIR = "~/.cw" }
"""

#: And one whose command exists nowhere, for the pane that has to REFUSE. Since Task 3 an
#: unapproved profile in an attended pane asks rather than refuses, so a refusal that
#: reaches a real pane has to come from another link in the chain: this is `NOT_ON_PATH`,
#: which is the one an operator actually meets (a harness uninstalled, a path typo).
_NOWHERE = """
[harness.claude-work]
kind = "claude"
command = ["charter-has-no-such-harness-here"]
"""


def _eventually(predicate, timeout: float = 15.0) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(0.05)
    return bool(predicate())


class _ARealChatOnARealServer(PersonaIso):
    """One workspace, a real private server, and a recorder standing in for `claude`."""

    WS = "beta"
    SLUG = "pane-becomes-harness"
    #: What the recorder exits with once it has written its file. `None` keeps it alive.
    EXIT_WITH: int | None = None
    #: How long a launch may sit in `_wait_for_harness` here before it is a failure. Twice
    #: the longest wait any case in this module makes on purpose, and two orders of
    #: magnitude under the sweep's 900 s subset timeout, which is the bound it replaces.
    HARNESS_WAIT_SECONDS = 30.0

    def setUp(self) -> None:
        super().setUp()
        v = tmuxctl.version()
        if v is None or v < tmuxctl.FLOOR:
            self.skipTest(f"the frame's floor is tmux {tmuxctl.FLOOR[0]}.{tmuxctl.FLOOR[1]}"
                          f"; this machine has {v}")
        make_plane(self)
        no_background_refresh(self)
        self._bound_the_wait_for_harness()
        # A launch with no terminal of its own: `attach=False`, the way a tab or a handoff
        # drives it, and the state `os.get_terminal_size()` raising is the seam for.
        _ttyguard.no_terminal()
        self.tmux = shutil.which("tmux")
        self.socket = _tmuxreap.name(f"{self.SLUG}-{next(_SERVERS)}")
        self.enterContext(mock.patch.object(tmuxctl, "plane_socket", return_value=self.socket))
        self.addCleanup(self._reap_the_server)
        # A detached `charter frame-gather` outlives the case and is refused by
        # `tests/_planeguard`; what is asked here is what tmux does.
        self.enterContext(mock.patch.object(commands_frame, "_spawn_gather"))
        # No panel panes: every one is another `-m charter` child, and none of them is what
        # this file is about.
        self.enterContext(mock.patch.object(commands_frame, "_drawable_slots",
                                            return_value=[]))
        (config.WORKSPACES_DIR / self.WS).mkdir(parents=True, exist_ok=True)

        self.records = self.tmp / "records"
        self.records.mkdir()
        bindir = self.tmp / "bin"
        bindir.mkdir()
        # **The harness**: it writes down the argv, the environment and the pid it was
        # given — which is the whole measurement — and then either stays or exits with a
        # number the frame has to carry out.
        (bindir / "claude").write_text(
            f"#!{sys.executable}\n"
            "import json, os, sys, time\n"
            # **The wiring probe is answered first** (ruling 10). The launcher asks this
            # binary `plugin list --json` before it execs it, and a recorder that recorded
            # THAT call would both overwrite the measurement and leave the launch refusing.
            # `user` scope covers every directory (`plugincache.covers`).
            "if sys.argv[1:3] == ['plugin', 'list']:\n"
            "    json.dump([{'id': 'charter@charter', 'scope': 'user', 'enabled': True,\n"
            "                'installedAt': '2026-09-12T00:00:00Z'}], sys.stdout)\n"
            "    sys.exit(0)\n"
            "out = os.path.join(os.environ['RECORD_DIR'], 'harness.json')\n"
            "with open(out + '.tmp', 'w') as f:\n"
            "    json.dump({'argv': sys.argv[1:], 'env': dict(os.environ),\n"
            "               'pid': os.getpid()}, f)\n"
            "os.replace(out + '.tmp', out)\n"
            # A beat before the exit, and it is not decoration. `_launch` reports an EARLY
            # DEATH — the harness's own code, rather than 0 — for a pane that dies while it
            # is still setting the frame up, and a harness that exits the instant it is
            # exec'd races that window. Measured on CI (3.12, 2026-09-12): the launch
            # answered 7 instead of 0 on a loaded runner, having answered 0 on four
            # interpreters an hour earlier. What this class is about is that the code
            # TRAVELS, which the assertions below read off the chat's own state; whether it
            # also arrives through the early-death path is a different test's subject.
            f"{'time.sleep(1); sys.exit(' + str(self.EXIT_WITH) + ')' if self.EXIT_WITH is not None else 'time.sleep(300)'}\n")
        (bindir / "claude").chmod(0o755)
        self.enterContext(mock.patch.dict(os.environ, {
            # First on the client's `PATH`, ahead of `tests/_claudeguard`'s own fake: the
            # launcher resolves the profile's command in the PANE, so this is the only way
            # to decide what really runs there.
            "PATH": f"{bindir}{os.pathsep}{os.environ.get('PATH', '')}",
            "RECORD_DIR": str(self.records),
            "CHARTER_ROOT": str(config.ROOT),
            "PYTHONPATH": os.pathsep.join(
                [str(_REPO_ROOT), os.environ.get("PYTHONPATH", "")]).rstrip(os.pathsep),
            **_gitguard.environment(),
            # The server this case starts hands its environment to every `charter panel`
            # in the frame, and a panel on a fresh plane forks a PyPI check without it (#945).
            **_envguard.stated(),
        }, clear=True))

    def _bound_the_wait_for_harness(self) -> None:
        """`_wait_for_harness` with a deadline, because the real one has none by design.

        Inside an operator's tmux the launch stays awake for the life of the frame
        (`_launch_in_operator_tmux`), and `_wait_for_harness` is `while True` until the
        pane dies or goes. Right for `charter claude` in somebody's tmux; a hang in this
        module for any pane that never ends. **Measured by the deletion sweep** on PR #998
        (run 34732660388, `69cd3a9`): its `disable-branch` on `_launch`'s `if selecting:` —
        every launch opens the profile selector — put a selector waiting for a key into
        `ARefusalInsideTheOperatorsOwnTmux`'s pane, and the shard sat in this wait for its
        whole 900 s subset timeout, `ran: 0`, twice. A mutation the suite cannot answer in
        bounded time is a line the sweep cannot pin; the thirteen other modules the map
        lists for `_launch` all answered that one in 2-32 s.

        The real function still gives the answer — called the moment the pane is not alive,
        which is where it returns at once — so what every case asserts is unchanged. Only
        the alive spin is bounded, and it ends the way this module's other waits do: a
        failure that says what was on the pane.
        """
        real = commands_frame._wait_for_harness

        def bounded(socket: str, harness_pane: str) -> int | None:
            deadline = time.monotonic() + self.HARNESS_WAIT_SECONDS
            while time.monotonic() < deadline:
                if commands_frame._pane_state(socket, harness_pane)[0] != commands_frame._ALIVE:
                    return real(socket, harness_pane)
                time.sleep(commands_frame._POLL_SECONDS)
            self.fail(f"the harness pane was still running after "
                      f"{self.HARNESS_WAIT_SECONDS:g}s, and nothing in this module should "
                      f"outlive that: {commands_frame._pane_last_words(socket, harness_pane)!r}")

        self.enterContext(mock.patch.object(commands_frame, "_wait_for_harness",
                                            side_effect=bounded))

    def _reap_the_server(self) -> None:
        subprocess.run([self.tmux, "-L", self.socket, "kill-server"],
                       capture_output=True, timeout=20)
        try:
            os.unlink(_tmuxsocket.socket_path(self.socket))
        except OSError:
            pass

    def _tmux(self, *args: str) -> subprocess.CompletedProcess:
        return subprocess.run([self.tmux, "-L", self.socket, *args], capture_output=True,
                              text=True, timeout=20)

    def _launch(self, *, unapproved: bool = False, **ns) -> int:
        """The real `_launch`, against the real server this case started.

        **A declared profile is checked for its launch record first** (Global Constraint,
        N6): `_launch` passes `--attended` for an open somebody is in front of, and a real
        pane has a terminal on both ends — so a profile with no record stops that pane at
        `run this? [y/N]` and waits until the suite's own 600 s watchdog. That is a hang
        rather than a failure, and a hang costs the whole run its verdict, so it is caught
        here, before tmux, where it names the fixture's mistake. *unapproved* is how the
        one case that is ABOUT that question says so.
        """
        name = ns.get("profile")
        if name and not unapproved:
            assert_approved(name)
        args = SimpleNamespace(**{"harness": "claude", "rest": [], "no_frame": False,
                                  "workspace": self.WS, "pick": False, "attach": False,
                                  "size": (120, 40), **ns})
        return commands_frame._launch(args)

    def _harness(self) -> dict:
        """What the exec'd harness wrote down, once it has run."""
        record = self.records / "harness.json"
        self.assertTrue(_eventually(record.is_file),
                        f"the harness never ran in the pane: {self._pane_text()}")
        return json.loads(record.read_text())

    def _alive(self, pane: str) -> bool:
        """Is *pane*'s own process still running?

        **The reading a refusal in the pane is asserted on, rather than the exit code the
        `pane-died` hook records** — and the difference is measured rather than assumed. A
        launcher that refuses exits without ever `exec`ing a harness, and on the Linux
        runner that pane comes back dead with an EMPTY `#{pane_dead_status}` and no exit
        file written, where the same pane on macOS carries the number; a pane whose HARNESS
        exits records its code on both (`TheHarnessExitCodeTravelsAsItDid`, which is where
        that contract belongs). What ruling 42 is about is whether the sentence survives
        long enough to be read and whether a keypress then releases it, and that is exactly
        what this answers.

        A pane tmux no longer lists at all is not alive either — the teardown hook closes
        the window once the chat is over.
        """
        out = self._tmux("display-message", "-p", "-t", pane, "#{pane_dead}")
        return out.returncode == 0 and out.stdout.strip() == "0"

    def _pane_text(self, pane: str | None = None) -> str:
        """What is on the pane, with tmux's own hard wrap taken out.

        `capture-pane` returns the pane as it is DRAWN, so a sentence longer than the pane
        is wide comes back broken across lines mid-word. Joining is what makes an assertion
        about the sentence rather than about the width of the window it landed in.
        """
        pane = pane or (state.harness_pane("beta.1") or "")
        out = self._tmux("capture-pane", "-p", "-S", "-", "-t", pane).stdout
        return "".join(out.splitlines())


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class ThePaneCharterStartsIsThePaneTheHarnessRunsIn(_ARealChatOnARealServer,
                                                    unittest.TestCase):
    """L1, L5 and L6 of the measurement, as one launch: the pane charter recorded is the
    pane the harness is in, it is that pane to every status reader, and the profile's
    environment arrived without crossing tmux."""

    def test_the_pane_charter_recorded_is_the_one_the_harness_is_running_in(self):
        self.assertEqual(self._launch(), 0)
        fid = "beta.1"
        pane = state.harness_pane(fid)
        self.assertTrue(pane, "the launch recorded no harness pane")
        harness = self._harness()
        pid = self._tmux("display-message", "-p", "-t", pane, "#{pane_pid}").stdout.strip()
        self.assertEqual(pid, str(harness["pid"]),
                         "the `exec` did not keep the launcher's pid for the harness, so "
                         "the pane charter recorded is not the harness's own")

    def test_the_harness_is_still_this_chat_to_every_status_reader(self):
        """L5: `$TMUX_PANE` inside the harness is the pane charter recorded, and the chat
        is in the server's own liveness list — which is what `state.reap` spares."""
        self.assertEqual(self._launch(), 0)
        fid = "beta.1"
        harness = self._harness()
        self.assertEqual(harness["env"].get("TMUX_PANE"), state.harness_pane(fid))
        self.assertTrue(_eventually(lambda: fid in (commands_frame._live_chats(self.socket)
                                               or set())),
                        "the chat is not in the server's liveness list")

    def test_the_profile_reaches_the_harness_and_never_tmux(self):
        """L6, and the constraint the launcher exists for: `CHARTER_HARNESS_PROFILE` is set
        at the `exec`, so it reaches the harness without joining `layout.CARRIABLE` and
        without passing through tmux's own argument parser."""
        self.assertEqual(self._launch(), 0)
        harness = self._harness()
        self.assertEqual(harness["env"].get("CHARTER_HARNESS_PROFILE"), "claude")
        self.assertEqual(harness["env"].get("CHARTER_HARNESS"), "claude-code")
        for scope in (("show-environment", "-g"),
                      ("show-environment", "-t", state.workspace_prefix(self.WS))):
            said = self._tmux(*scope).stdout
            self.assertNotIn("CHARTER_HARNESS_PROFILE", said,
                             "the profile crossed tmux, where only its name may go")

    def test_a_declared_profiles_env_reaches_the_harness_and_not_tmux(self):
        """L6 for a profile the FILE declares, which is the case the constraint exists for:
        `env` is set at the `exec` in the pane, so nothing of it joins `layout.CARRIABLE`
        or crosses tmux's own argument parser — and tmux's environment holds no trace of
        either the variable or its value."""
        (self.tmp / "alt").mkdir()
        declare_profiles(self, f'[harness.claude-work]\nkind = "claude"\n'
                               f'command = ["claude"]\n'
                               f'env = {{ CLAUDE_CONFIG_DIR = "{self.tmp / "alt"}" }}\n')
        approve_profile(self)
        self.assertEqual(self._launch(profile="claude-work"), 0)
        harness = self._harness()
        self.assertEqual(harness["env"].get("CLAUDE_CONFIG_DIR"), str(self.tmp / "alt"))
        self.assertEqual(harness["env"].get("CHARTER_HARNESS_PROFILE"), "claude-work")
        for scope in (("show-environment", "-g"),
                      ("show-environment", "-t", state.workspace_prefix(self.WS))):
            said = self._tmux(*scope).stdout
            self.assertNotIn("CLAUDE_CONFIG_DIR", said)
            self.assertNotIn(str(self.tmp / "alt"), said)

    def test_the_chat_is_framed_because_the_pane_proved_it(self):
        """Rulings 29 and 33 end to end: the launcher asked tmux whether its own pid was
        the `#{pane_pid}` of a live pane whose window is named for this chat, and kept the
        chat's session id because it was. A launcher that could not prove it drops that id
        — which is what stops a `charter frame-launch` run from a model's tool shell
        rewriting a chat's record."""
        self.assertEqual(self._launch(), 0)
        self.assertEqual(self._harness()["env"].get("CHARTER_SESSION_ID"), "beta.1")


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class TheHarnessExitCodeTravelsAsItDid(_ARealChatOnARealServer, unittest.TestCase):
    """L2: the `pane-died` hooks are installed against the launcher's pane and fire for the
    harness that replaced it, so the code the harness chose is the code charter records.

    **And the window STAYS, which is the exit gate's whole change** (#1112, task 2). This
    case used to assert the opposite — that `pane-died[1]`'s `kill-window` took the chat's
    window the moment its harness exited — and that is exactly what decision 4 overturns: no
    harness exit destroys a chat. The exit code still travels; what changed is that the tab
    is still there to hold it, and the operator is offered something.

    7 is a crash, so this drives the crash presentation end to end on a real server: the
    dead pane is left exactly as tmux keeps it, and the choice opens in a drawer marked as
    this chat's.
    """

    SLUG = "pane-exit-code"
    EXIT_WITH = 7

    def _why_not_ended(self, fid: str) -> str:
        """Every gate `ended.present` refuses at, read back off the real server.

        A bare "it never claimed this exit" says nothing about WHICH of the six conditions
        declined, and the answer is in six different places — three files under the chat's
        directory, the chat's own record, and two pane options tmux holds. Assembling them
        here is what makes a red run readable instead of the start of a hunt.
        """
        proof = self._tmux("list-panes", "-a", "-F",
                           "#{pane_id}|#{pane_dead}|#{@charter_chat}|#{@charter_plane}"
                           "|#{@charter_drawer}").stdout.strip()
        hooks = self._tmux("show-hooks", "-p", "-t",
                           state.harness_pane(fid) or "").stdout.strip()
        return (f"\n  profile={state.profile(fid)!r}"
                f"\n  drawn={state.was_drawn(fid)!r} closed={state.was_closed(fid)!r}"
                f"\n  exit={state.exit_code(fid)!r} server={state.frame_server(fid)!r}"
                f"\n  recorded pane={state.harness_pane(fid)!r}"
                f"\n  this plane={commands_frame._this_plane()!r}"
                f"\n  this plane, resolved={os.path.realpath(commands_frame._this_plane())!r}"
                f"\n  list-panes:\n    " + proof.replace("\n", "\n    ") +
                f"\n  hooks on the pane:\n    " + hooks.replace("\n", "\n    "))

    def test_a_harness_exit_code_travels_as_it_did(self):
        self.assertEqual(self._launch(), 0)
        fid = "beta.1"
        self.assertTrue(_eventually(lambda: state.exit_code(fid) == 7),
                        f"the exit code did not reach the chat's state: "
                        f"{state.exit_code(fid)!r}")
        self.assertTrue(_eventually(lambda: state.is_ended(fid)),
                        "the ended step never claimed this exit:"
                        + self._why_not_ended(fid))
        self.assertIn(fid, self._tmux("list-windows", "-a", "-F",
                                      "#{window_name}").stdout.split(),
                      "a harness exiting closed its chat's window")

    def test_a_crash_leaves_the_pane_alone_and_opens_a_marked_drawer(self):
        """The harness's last lines are what a crashed tab is FOR, so charter draws nothing
        in that pane — tmux keeps it dead under `remain-on-exit` — and splits the choice
        beneath it. One listing proves the drawer is this chat's, which is the same listing
        every kill in `frame/ended.py` is aimed by."""
        self.assertEqual(self._launch(), 0)
        fid = "beta.1"
        self.assertTrue(_eventually(lambda: state.is_ended(fid)),
                        "the ended step never claimed this exit")

        pane = state.harness_pane(fid)
        self.assertEqual(
            self._tmux("display-message", "-p", "-t", pane, "#{pane_dead}").stdout.strip(),
            "1", "the crashed pane was respawned instead of being left alone")
        # **`|` and not a TAB, because this call is a raw `tmux` without `-u`.** #984:
        # a client whose environment names no UTF-8 locale gets a literal tab back in a
        # `-F` format as `_`, so a tab-separated row read here splits into one field and
        # the assertion can never match. `tmuxctl.server_argv` adds `-u` for exactly this;
        # the fixture's own `_tmux` does not, and production's `ended.PROOF_FORMAT` goes
        # through `server_argv`, so only the test has to choose another separator.
        self.assertTrue(_eventually(lambda: any(
            row.split("|")[-1] == fid
            for row in self._tmux("list-panes", "-a", "-F",
                                  "#{pane_id}|#{@charter_drawer}").stdout.splitlines())),
            f"no drawer was opened for the crashed chat: "
            f"{self._tmux('list-panes', '-a', '-F', '#{pane_id}|#{@charter_drawer}').stdout!r}")


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class ARefusalInThePaneReachesTheOperator(_ARealChatOnARealServer, unittest.TestCase):
    """Ruling 42, both halves, against a real pane.

    The plane declares a profile whose command exists nowhere, so the pane's own launcher
    refuses it — while the check this process makes before tmux is stood down, which is
    exactly the state the two checks exist for: the plane can move between them.
    """

    SLUG = "pane-refusal"

    def setUp(self) -> None:
        super().setUp()
        # The shared fixture: the file, a repository that ignores it, `$HOME`, and the
        # ceiling that stops git's walk above this throwaway plane. It carries
        # `tests._gitguard.environment()` into the git it runs, which is what every other
        # module here does and what `tests._planeguard.AmbientGitConfig` holds them to.
        self.local = declare_profiles(self, _NOWHERE)
        # Approved, so what the pane refuses is the missing command rather than the
        # question `ThePaneAsksBeforeItRunsANewCommand` below is about.
        approve_profile(self)
        # Only the pre-tmux call, in THIS process. The pane's launcher is another process
        # and runs the whole chain for itself, which is the half under test.
        self.enterContext(mock.patch.object(launcher, "refusal", return_value=None))

    def test_an_attended_pane_holds_its_refusal_until_somebody_reads_it(self):
        self.assertEqual(self._launch(profile="claude-work"), 0)
        fid = "beta.1"
        pane = state.harness_pane(fid)
        self.assertTrue(_eventually(
            lambda: "which is not on PATH" in self._pane_text(pane)),
            f"the pane never showed the refusal: {self._pane_text(pane)!r}")
        # **The property ruling 42 is about**: it is still there to be read. A launcher that
        # printed and exited would have had its window killed by the teardown hook 6-14 ms
        # after the start, and all 40 measured refusals were lost that way.
        self.assertEqual(
            self._tmux("display-message", "-p", "-t", pane, "#{pane_dead}").stdout.strip(),
            "0", "the pane died with the refusal on it instead of waiting")
        sent = self._tmux("send-keys", "-t", pane, "Enter")
        self.assertEqual(sent.returncode, 0, sent.stderr)
        self.assertTrue(_eventually(lambda: not self._alive(pane), 30),
                        f"the keypress did not let the launcher go: "
                        f"{self._pane_text(pane)!r}")

    def test_a_key_pressed_before_the_pane_is_listening_is_not_lost(self):
        """**The window between the refusal appearing and the read starting is where an
        operator actually presses.** `tty.setraw`'s own default is `TCSAFLUSH`, which
        discards input that arrived before the mode change — so a key pressed in that
        window was thrown away and the pane waited for a second one nobody knew to send.
        Measured on CI (Linux, tmux 3.4), where the case above went red for exactly this;
        on the machine it was written on the timing happened to fall the other way, which
        is why this case sends the key as early as it possibly can."""
        self.assertEqual(self._launch(profile="claude-work"), 0)
        pane = state.harness_pane("beta.1")
        self._tmux("send-keys", "-t", pane, "Enter")
        self.assertTrue(_eventually(lambda: not self._alive(pane), 30),
                        f"the keypress was swallowed: {self._pane_text(pane)!r}")

    def test_an_unattended_refusal_is_read_back_by_the_launch_that_opened_the_chat(self):
        """Nobody is at a chat a handoff opens, so there is no key to wait for: the pane
        records what it did and this launch — which has a caller listening — reports it and
        carries the refusal's own exit code out.

        Asserted on what was SAID rather than on the record it was read from: the launch
        ends by reaping the chat that never started, so the record has done its job and
        gone by the time anything could look at it. What has to survive is the sentence.
        """
        said: list[str] = []
        with mock.patch.object(commands_frame.util, "err", side_effect=said.append):
            rc = self._launch(profile="claude-work",
                              opening=commands_frame.Opening("fix the widget please"))
        # 127, the shell's own number for a command that is not there, and the launcher's
        # for it — carried out of the pane by the record rather than reconstructed.
        self.assertEqual(rc, launcher.MISSING_EXIT)
        self.assertTrue(any("which is not on PATH" in s for s in said),
                        f"the launch did not report what the pane refused: {said}")


class _TheFirstLookFindsThePaneAlreadyDead:
    """The race #1067 lost on CI 3.11, with the race taken out.

    `_launch` asks tmux whether the harness pane is dead the moment its hooks are in, and a
    launcher that refused had usually not exited yet — the ask lands at 110-185 ms, the
    record at 210-300 ms, and every one of 48 CI passes took at least 289 ms. The failing run
    took 212 ms: the launcher had already recorded, exited, and left a dead pane whose
    `#{pane_dead_status}` Linux reports EMPTY, which that ask reads as
    `commands_frame._UNKNOWN_DEATH_CODE`.

    So that first ask is made to lose, every time: it waits until the pane really has
    stopped, then answers what Linux answers. Every later ask is tmux's own.
    """

    def _first_look_after_the_pane_died(self) -> None:
        real = commands_frame._query_pane_dead_status
        asked: list[str] = []

        def too_late(socket: str, pane: str) -> int | None:
            asked.append(pane)
            if len(asked) > 1:
                return real(socket, pane)
            self.assertTrue(_eventually(lambda: not self._alive(pane), 30),
                            f"the pane never stopped: {self._pane_text(pane)!r}")
            return commands_frame._UNKNOWN_DEATH_CODE

        self.enterContext(mock.patch.object(commands_frame, "_query_pane_dead_status",
                                            side_effect=too_late))


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class ARefusalIsReadEvenWhenThePaneIsAlreadyGone(_TheFirstLookFindsThePaneAlreadyDead,
                                                _ARealChatOnARealServer, unittest.TestCase):
    """#1067: a launcher that recorded a refusal has said why it exited, and a dead pane
    is not evidence against that — however early the launch first looks."""

    SLUG = "pane-refusal-read-first"

    def setUp(self) -> None:
        super().setUp()
        self.local = declare_profiles(self, _NOWHERE)
        approve_profile(self)
        self.enterContext(mock.patch.object(launcher, "refusal", return_value=None))
        self._first_look_after_the_pane_died()

    def test_the_recorded_refusal_and_its_number_reach_the_operator(self):
        said: list[str] = []
        with mock.patch.object(commands_frame.util, "err", side_effect=said.append):
            rc = self._launch(profile="claude-work",
                              opening=commands_frame.Opening("fix the widget please"))
        self.assertTrue(any("which is not on PATH" in s for s in said),
                        f"the launch did not report what the pane refused: {said}")
        self.assertEqual(rc, launcher.MISSING_EXIT, said)


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class AHarnessThatTookThePaneIsStillReportedFromThePane(_TheFirstLookFindsThePaneAlreadyDead,
                                                       _ARealChatOnARealServer,
                                                       unittest.TestCase):
    """The other side of #1067's fix: what a launcher records when it hands the pane over
    is not a refusal, and it must not stand in for a pane that has since died. A harness
    that started and then exited before the first look is the early death it was before
    the record was read first — reported from the pane, with the pane's own reading."""

    SLUG = "pane-handed-over-then-died"
    EXIT_WITH = 7

    def setUp(self) -> None:
        super().setUp()
        declare_profiles(self, _LOCAL)
        approve_profile(self)
        self._first_look_after_the_pane_died()

    def test_the_dead_panes_reading_is_what_the_launch_reports(self):
        with mock.patch.object(commands_frame.util, "err"):
            rc = self._launch(profile="claude-work",
                              opening=commands_frame.Opening("fix the widget please"))
        self._harness()
        self.assertEqual(rc, commands_frame._UNKNOWN_DEATH_CODE)


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class ThePaneAsksBeforeItRunsANewCommand(_ARealChatOnARealServer, unittest.TestCase):
    """Task 3's question, in the place the plan says it is asked: the chat's own pane.

    A `+`, a workspace tab and the palette's new chat are all a single press by a process
    with `/dev/null` for streams, so charter cannot put the question where the press
    happened — but the pane it opens has a terminal on both of its ends. This is that pane,
    on a real server, with a real `y` sent to it.
    """

    SLUG = "pane-asks"

    def setUp(self) -> None:
        super().setUp()
        self.local = declare_profiles(self, _LOCAL)
        # No `approve_profile`: this is the one case that is ABOUT the missing record, and
        # `_launch(unapproved=True)` is how it says so to the guard that would fail fast.
        self.enterContext(mock.patch.object(launcher, "refusal", return_value=None))

    def test_the_pane_shows_the_command_and_waits_for_an_answer(self):
        self.assertEqual(self._launch(profile="claude-work", unapproved=True), 0)
        pane = state.harness_pane("beta.1")
        self.assertTrue(_eventually(lambda: "run this? [y/N]" in self._pane_text(pane)),
                        f"the pane never asked: {self._pane_text(pane)!r}")
        drawn = self._pane_text(pane)
        self.assertIn("command  claude", drawn)
        self.assertIn("CLAUDE_CONFIG_DIR=~/.cw", drawn)
        self.assertFalse((self.records / "harness.json").is_file(),
                         "the command ran before anybody answered the question")

    def test_a_yes_starts_the_harness_and_is_not_asked_again(self):
        """And the record it leaves is what makes it *once*: the second launch of the same
        profile runs the command with nothing to answer, which is the whole promise of the
        feature's own title."""
        self.assertEqual(self._launch(profile="claude-work", unapproved=True), 0)
        pane = state.harness_pane("beta.1")
        self.assertTrue(_eventually(lambda: "run this? [y/N]" in self._pane_text(pane)),
                        f"the pane never asked: {self._pane_text(pane)!r}")
        sent = self._tmux("send-keys", "-t", pane, "y", "Enter")
        self.assertEqual(sent.returncode, 0, sent.stderr)
        harness = self._harness()
        self.assertEqual(harness["env"].get("CHARTER_HARNESS_PROFILE"), "claude-work")
        self.assertEqual(harness["env"].get("CLAUDE_CONFIG_DIR"),
                         str(self.home / ".cw"))
        # Written by the PANE's own process, into this plane's state directory — which is
        # what `assert_approved` reads and what every later open of this profile finds.
        assert_approved("claude-work")


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class ARefusalInsideTheOperatorsOwnTmux(_ARealChatOnARealServer, unittest.TestCase):
    """Ruling 42 on the path that needs it most — a real window in a tmux charter does not
    own, end to end.

    **Nothing here ever looks at that pane.** Charter's launcher on this path stays awake
    for the life of the frame and closes the window itself, and an open nobody is watching
    is never switched to — so a refusal printed in the pane is gone with the window before
    anything reads it, which is what `_pane_last_words` measured empty. The chat's own
    record is the only copy, and the process holding the terminal is the one that must say
    it.

    The pre-tmux check is stood down in THIS process only: the pane's launcher is another
    process and runs the whole chain for itself, which is the half under test.
    """

    SLUG = "pane-refusal-operator"

    def setUp(self) -> None:
        super().setUp()
        self.local = declare_profiles(self, _NOWHERE)
        approve_profile(self)
        self.enterContext(mock.patch.object(launcher, "refusal", return_value=None))
        # An operator's own server: started by NAME so `tests._tmuxreap` can collect it,
        # and reached by PATH, which is what `tmuxctl.is_operator_socket` reads as "a tmux
        # charter did not start" (#812 is what happens when those two are compared as
        # strings). `test_frame_tmux_integration.OP_SOCKET_PATH` is the same pairing.
        self.op_name = _tmuxreap.name(f"{self.SLUG}-{next(_SERVERS)}")
        self.addCleanup(subprocess.run,
                        [self.tmux, "-L", self.op_name, "kill-server"],
                        capture_output=True, timeout=20)
        started = subprocess.run(
            [self.tmux, "-L", self.op_name, "-f", "/dev/null", "new-session", "-d",
             "-s", "op", "-x", "120", "-y", "40", "--", "cat"],
            capture_output=True, text=True, timeout=20, env=dict(os.environ))
        self.assertEqual(started.returncode, 0, started.stderr)
        self.op_path = _tmuxsocket.socket_path(self.op_name)
        session = subprocess.run(
            [self.tmux, "-L", self.op_name, "display-message", "-p", "#{session_id}"],
            capture_output=True, text=True, timeout=20).stdout.strip()
        self.enterContext(mock.patch.object(tmuxctl, "operator_server",
                                            return_value=(self.op_path, session)))

    def test_the_launch_says_what_the_pane_refused(self):
        """The sentence first, because it is the thing this path had no other copy of —
        and the number second, with what was said in its message: the two halves fail for
        different reasons, and a bare `1 != 3` said which half only by luck."""
        said: list[str] = []
        with mock.patch.object(commands_frame.util, "err", side_effect=said.append):
            rc = self._launch(profile="claude-work",
                              opening=commands_frame.Opening("fix the widget please"))
        self.assertTrue(any("which is not on PATH" in s for s in said),
                        f"the launch did not say what the pane refused: {said}")
        # The launcher's own number, not tmux's reading of a pane that is no longer there:
        # in somebody else's tmux `#{pane_dead_status}` is empty or absent for exactly this
        # pane, and both read as `_UNKNOWN_DEATH_CODE`.
        self.assertEqual(rc, launcher.MISSING_EXIT, said)

    def test_the_window_it_opened_is_taken_back(self):
        """A window the operator never asked for, holding a chat that never started, is not
        theirs to close."""
        with mock.patch.object(commands_frame.util, "err"):
            self._launch(profile="claude-work",
                         opening=commands_frame.Opening("fix the widget please"))
        windows = subprocess.run(
            [self.tmux, "-L", self.op_name, "list-windows", "-a", "-F", "#{window_name}"],
            capture_output=True, text=True, timeout=20).stdout.split()
        self.assertNotIn("beta.1", windows, f"the frame's window was left behind: {windows}")


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class APaneThatNeverEndsIsAFailureHereAndNotAHang(_ARealChatOnARealServer,
                                                    unittest.TestCase):
    """`_bound_the_wait_for_harness`, pinned — the guard that turns the sweep's 900 s
    "not measured" into a red with the pane on it. The pane here is a fake that is always
    alive, because the real thing this stands for is the selector the mutation put there,
    and that took 30 s per case to say the same."""

    SLUG = "pane-never-ends"
    HARNESS_WAIT_SECONDS = 0.5

    def test_the_wait_ends_in_its_budget_and_says_what_the_pane_shows(self):
        self.enterContext(mock.patch.object(commands_frame, "_pane_state",
                                            return_value=(commands_frame._ALIVE, None)))
        self.enterContext(mock.patch.object(commands_frame, "_pane_last_words",
                                            return_value=["charter · which profile?"]))
        started = time.monotonic()
        with self.assertRaises(AssertionError) as caught:
            commands_frame._wait_for_harness(self.socket, "%1")
        self.assertLess(time.monotonic() - started, 10.0)
        self.assertIn("still running after 0.5s", str(caught.exception))
        self.assertIn("which profile?", str(caught.exception))

    def test_a_pane_that_ends_gets_the_real_answer(self):
        """The bound is on the alive spin alone: a pane that dies is answered by
        `_wait_for_harness` itself, code included, so no case's assertion moved."""
        self.enterContext(mock.patch.object(commands_frame, "_pane_state",
                                            return_value=(commands_frame._DEAD, 7)))
        self.assertEqual(commands_frame._wait_for_harness(self.socket, "%1"), 7)


if __name__ == "__main__":
    unittest.main()
