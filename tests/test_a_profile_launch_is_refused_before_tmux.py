"""A profile that may not run is refused before tmux is asked for anything.

Each guard runs before tmux wherever a terminal is attached — a refusal there is a `return`,
with no session, no window and no chat directory to tear down — and again in the pane
immediately before the exec, because the plane can move in between (spec, *What guards a
launch*). This module is the first half: what `_launch` refuses, what it hands tmux when it
does not, and what never crosses tmux at all.

**Every declared profile is refused here** (review B1). Nothing yet stands for the
operator's approval of a `command`, and `charter.local.toml` is a file a chat can write
with no diff to show for it — so between this merge and Task 3's, `main` runs no command
that file declares, through `charter <profile>`, `+`, a tab, reopen or a handoff. The three
cases that say so run without the stand-in the rest of the module uses.
"""

from __future__ import annotations

import io
import os
import subprocess
import unittest
from contextlib import redirect_stderr
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame, config, contain
from charter.frame import launcher, layout, reopen as reopen_state, state, tmuxctl
from tests import _gitguard, _tmuxsocket
from tests._isolation import PersonaIso, make_plane

_LOCAL = """
[harness.claude-work]
kind = "claude"
command = ["claude"]
env = { CLAUDE_CONFIG_DIR = "~/.cw" }

[harness.codex-pinned]
kind = "codex"
command = ["npx", "-y", "@openai/codex@0.140.0"]
"""

#: The operator's own tmux, as `tmuxctl.operator_server` reads it out of `$TMUX`: a socket
#: PATH, which is what `is_operator_socket` tells from charter's own `-L charter` name.
#: Computed the way tmux computes it (`tests/_tmuxsocket.py`) rather than written down —
#: a literal uid in a socket path is one developer's machine baked into the suite.
_OPERATOR = _tmuxsocket.OPERATOR_SOCKET


def _git(root: Path, *args: str) -> None:
    subprocess.run(["git", "-C", str(root), *args], check=True, capture_output=True,
                   env={**os.environ, **_gitguard.environment()})


class _ALaunchNamesAProfile(PersonaIso):
    """The real `_launch`, with tmux and everything that would start a process stood in —
    `tests/test_a_chat_opens_in_the_background_with_its_first_message.py`'s own patch set."""

    def setUp(self) -> None:
        super().setUp()
        make_plane(self)
        self.local = config.ROOT / "charter.local.toml"
        self.local.write_text(_LOCAL)
        (config.ROOT / ".gitignore").write_text("/charter.local.toml\n")
        _git(config.ROOT, "init", "-q")
        (config.WORKSPACES_DIR / "beta").mkdir(parents=True, exist_ok=True)
        self.enterContext(mock.patch.dict(
            os.environ,
            {"PATH": os.environ.get("PATH", ""),
             "GIT_CEILING_DIRECTORIES": str(self.tmp.resolve().parent),
             "CHARTER_ROOT": str(config.ROOT), **_gitguard.environment()}, clear=True))
        self.argvs: list[list[str]] = []
        self.execs: list[tuple] = []
        self.enterContext(mock.patch.object(launcher.os, "execvpe",
                                            side_effect=lambda *a: self.execs.append(a)))

    def _no_approval_needed(self) -> None:
        """Task 2 refuses every declared profile (review B1). A case that is about something
        else stands that one refusal down; the three that are about it do not."""
        self.enterContext(mock.patch.object(launcher, "_approval_refusal",
                                            return_value=None))

    def _run(self, action, argv, **kw):
        self.argvs.append(list(argv))
        out = "%9\n"
        if "new-window" in argv and "-t" in argv and _OPERATOR in argv:
            out = "@1 %9\n"
        return subprocess.CompletedProcess(argv, 0, out, "")

    def _launch(self, *, which: str | None = "/nowhere/claude", dead_status=None,
                verdict: tuple = (None, ""), operator=None, **ns) -> int:
        """The real `_launch` with tmux stood in — and every answer a case might care about
        as a PARAMETER rather than a patch applied inside here.

        The existing launcher tests record why (`_harness_binary_installed`): these patches
        are entered inside this helper, so they override anything a case set up outside it,
        and the case about a missing binary was answered "installed" along with everything
        else. A knob a case can state is the only way that stays honest.
        """
        args = SimpleNamespace(**{"harness": "claude", "rest": [], "no_frame": False,
                                  "workspace": "beta", "pick": False, "size": (120, 40),
                                  **ns})
        with mock.patch("charter.commands_frame.shutil.which", return_value=which), \
                mock.patch.object(launcher.shutil, "which", return_value=which), \
                mock.patch("sys.stdout.isatty", return_value=True), \
                mock.patch("sys.stdin.isatty", return_value=False), \
                mock.patch.object(tmuxctl, "version", return_value=(3, 7)), \
                mock.patch.object(tmuxctl, "operator_server", return_value=operator), \
                mock.patch.object(commands_frame, "_live_sessions", return_value={"beta"}), \
                mock.patch.object(commands_frame, "_live_chats", return_value=set()), \
                mock.patch.object(tmuxctl, "run", side_effect=self._run), \
                mock.patch.object(tmuxctl, "write_all",
                                  side_effect=lambda joint, writes, **kw: [
                                      subprocess.CompletedProcess(w.argv, 0, "", "")
                                      for w in writes]), \
                mock.patch.object(tmuxctl, "interact",
                                  return_value=subprocess.CompletedProcess([], 0)), \
                mock.patch.object(commands_frame, "_query_pane_dead_status",
                                  return_value=dead_status), \
                mock.patch.object(commands_frame, "_await_the_launcher",
                                  return_value=verdict) as awaited, \
                mock.patch.object(commands_frame, "_draw_panels", return_value={}), \
                mock.patch.object(commands_frame, "_arm_panel_respawn"), \
                mock.patch.object(commands_frame, "_spawn_gather"), \
                mock.patch.object(commands_frame, "_chat_being_left", return_value=""), \
                mock.patch.object(commands_frame, "_drop_panels"), \
                mock.patch.object(commands_frame.state, "reap"):
            rc = commands_frame._launch(args)
        self.awaited = awaited
        return rc

    def _said(self, **ns) -> tuple[int, str]:
        err = io.StringIO()
        with redirect_stderr(err):
            rc = self._launch(**ns)
        return rc, err.getvalue()

    def _tmux_verbs(self) -> list[str]:
        return [a[3] for a in self.argvs if len(a) > 3]

    def _started(self) -> bool:
        return any("new-window" in a or "new-session" in a for a in self.argvs)


class EveryDeclaredProfileIsRefusedUntilApprovalExists(_ALaunchNamesAProfile,
                                                       unittest.TestCase):
    """Review B1, and the three cases that run WITHOUT the stand-in. Delete the refusal and
    these go red: `main` would run whatever a chat wrote into `charter.local.toml`."""

    def test_every_declared_profile_is_refused_until_approval_exists(self):
        rc, said = self._said(profile="claude-work")
        self.assertEqual(rc, launcher.REFUSED_EXIT)
        self.assertIn("cannot yet ask before a declared command runs", said)
        self.assertFalse(self._started(), self.argvs)
        self.assertEqual(self.execs, [])

    def test_a_declared_replacement_of_a_built_in_is_refused_until_approval_exists(self):
        self.local.write_text('[harness.claude]\nkind = "claude"\n'
                              'command = ["/opt/claude"]\n')
        rc, said = self._said()
        self.assertEqual(rc, launcher.REFUSED_EXIT)
        self.assertIn("cannot yet ask before a declared command runs", said)
        self.assertFalse(self._started(), self.argvs)

    def test_a_built_in_the_file_does_not_replace_still_launches(self):
        self.assertEqual(self._launch(harness="codex"), 0)
        self.assertTrue(self._started(), self.argvs)


class ALaunchRefusesBeforeTmux(_ALaunchNamesAProfile, unittest.TestCase):
    def setUp(self) -> None:
        super().setUp()
        self._no_approval_needed()

    def test_an_unknown_profile_is_refused_and_nothing_is_allocated(self):
        rc, said = self._said(profile="nope")
        self.assertEqual(rc, 2)
        self.assertIn("no profile named 'nope'", said)
        self.assertEqual(self.argvs, [])
        self.assertFalse((config.STATE_DIR / "frame" / "beta.1").exists())

    def test_a_refused_profile_says_its_own_reason(self):
        self.local.write_text('[harness.bad]\nkind = "claude"\ncommand = "claude -x"\n')
        rc, said = self._said(profile="bad")
        self.assertEqual(rc, 2)
        self.assertIn("never a shell string", said)
        self.assertEqual(self.argvs, [])

    def test_a_profile_of_another_kind_is_refused_by_its_name(self):
        rc, said = self._said(profile="codex-pinned")
        self.assertEqual(rc, 2)
        self.assertIn("is a codex profile", said)
        self.assertIn("charter codex-pinned", said)

    def test_a_declared_profile_from_a_committable_file_is_refused(self):
        (config.ROOT / ".gitignore").write_text("")
        rc, said = self._said(profile="claude-work")
        self.assertEqual(rc, launcher.REFUSED_EXIT)
        self.assertIn("charter reinit", said)
        self.assertFalse(self._started(), self.argvs)

    def test_a_built_in_is_not_refused_over_the_local_file(self):
        (config.ROOT / ".gitignore").write_text("")
        self.assertEqual(self._launch(), 0)
        self.assertTrue(self._started(), self.argvs)

    def test_a_declared_replacement_of_a_built_in_in_a_committable_file_refuses_that_name(
            self):
        """Ruling 19: that name refuses rather than falling back to the built-in, which
        would run the command the operator replaced."""
        self.local.write_text('[harness.claude]\nkind = "claude"\n'
                              'command = ["/opt/claude"]\n')
        (config.ROOT / ".gitignore").write_text("")
        rc, said = self._said()
        self.assertEqual(rc, launcher.REFUSED_EXIT)
        self.assertIn("charter reinit", said)
        self.assertFalse(self._started(), self.argvs)
        self.assertEqual(self.execs, [])

    def test_a_command_not_on_path_is_refused_by_profile_name(self):
        rc, said = self._said(profile="claude-work", which=None)
        self.assertEqual(rc, launcher.MISSING_EXIT)
        self.assertIn("profile 'claude-work' runs claude, which is not on PATH", said)

    def test_each_refusal_says_something_different(self):
        """A reader has to be able to tell which rule fired. Four refusals, four sentences,
        and the same fixture — so a copied message is a red test rather than a shrug."""
        said = {self._said(profile="nope")[1],
                self._said(profile="codex-pinned")[1]}
        self.local.write_text('[harness.bad]\nkind = "claude"\ncommand = "claude -x"\n')
        said.add(self._said(profile="bad")[1])
        said.add(self._said(which=None)[1])
        self.assertEqual(len(said), 4, said)


class TheLaunchHandsTmuxTheLauncherAndNeverTheEnv(_ALaunchNamesAProfile, unittest.TestCase):
    def setUp(self) -> None:
        super().setUp()
        self._no_approval_needed()

    def test_the_launch_hands_tmux_the_launcher_and_never_the_env(self):
        self.assertEqual(self._launch(profile="claude-work"), 0)
        start = next(a for a in self.argvs if "new-window" in a)
        self.assertEqual(start[-9:], [os.sys.executable, "-P", "-m", "charter",
                                      "frame-launch", "--profile", "claude-work",
                                      "--attended", "--"])
        flat = " ".join(" ".join(a) for a in self.argvs)
        self.assertNotIn("~/.cw", flat)
        self.assertNotIn(os.path.expanduser("~/.cw"), flat)
        self.assertNotIn("CLAUDE_CONFIG_DIR", flat)

    def test_every_environment_name_that_crosses_tmux_is_one_of_the_five(self):
        """Spelled here rather than imported: the promise is the LIST, and a test that read
        it off the same constant would agree with any change to it."""
        self._launch(profile="claude-work")
        carried = {a[i + 1].split("=")[0]
                   for a in self.argvs for i, tok in enumerate(a) if tok == "-e"}
        self.assertTrue(carried)
        self.assertLessEqual(carried, {"CHARTER_SESSION_ID", "CHARTER_HARNESS",
                                       "CHARTER_ROOT", "CHARTER_WORKSPACE",
                                       "CHARTER_PERSONA", "PATH"})

    def test_carriable_is_unchanged(self):
        """A pin: nothing is added to it, and a profile's `env` never travels this way."""
        self.assertEqual(layout.CARRIABLE,
                         frozenset({"CHARTER_SESSION_ID", "CHARTER_HARNESS", "CHARTER_ROOT",
                                    "CHARTER_WORKSPACE", "CHARTER_PERSONA", "PATH"}))

    def test_the_chat_records_its_profile_and_keeps_the_kind_as_the_harness(self):
        self._launch(profile="claude-work")
        self.assertEqual(state.profile("beta.1"), "claude-work")
        self.assertEqual(state.identity("beta.1")["CHARTER_HARNESS"], "claude-code")

    def test_a_frame_escape_hatch_launch_records_no_profile(self):
        self.assertEqual(self._launch(harness="frame", rest=["--", "true"]), 0)
        start = next(a for a in self.argvs if "new-window" in a)
        self.assertEqual(start[-2:], ["--", "true"])
        self.assertNotIn("frame-launch", " ".join(start))
        self.assertIsNone(state.profile("beta.1"))

    def test_a_reopen_launch_is_unattended(self):
        recorded = reopen_state.Chat(chat="beta.1", workspace="beta", persona="",
                                     harness="claude-code", cwd="", resume="",
                                     transcript="", active=False)
        self._launch(profile="claude-work",
                     reopening=commands_frame.Reopening(recorded))
        start = next(a for a in self.argvs if "new-window" in a)
        self.assertNotIn("--attended", start)

    def test_a_background_open_launch_is_unattended(self):
        self._launch(profile="claude-work", attach=False,
                     opening=commands_frame.Opening("fix it please"))
        start = next(a for a in self.argvs if "new-window" in a)
        self.assertNotIn("--attended", start)


class ALaunchWithNoFrameExecsTheProfile(_ALaunchNamesAProfile, unittest.TestCase):
    def setUp(self) -> None:
        super().setUp()
        self._no_approval_needed()

    def test_no_frame_execs_the_profile_with_its_env(self):
        self.assertEqual(self._launch(profile="claude-work", no_frame=True), 0)
        (program, argv, env), = self.execs
        self.assertEqual((program, argv), ("claude", ["claude"]))
        self.assertEqual(env["CLAUDE_CONFIG_DIR"], os.path.expanduser("~/.cw"))
        self.assertEqual(env["CHARTER_HARNESS_PROFILE"], "claude-work")
        self.assertEqual(env["CHARTER_HARNESS"], "claude-code")
        self.assertEqual(self.argvs, [], "a bare harness asks tmux for nothing")

    def test_no_frame_from_a_chat_does_not_carry_that_chats_session_id(self):
        """Review 8: the pair is the defect — `hooks._record_harness_session` acts on a
        session id beside the launched `CHARTER_HARNESS`, so a bare harness typed inside a
        chat would write its own harness session into that chat's state."""
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "alpha.1",
                                          "CHARTER_HARNESS": "codex"}):
            self._launch(profile="claude-work", no_frame=True)
        (_program, _argv, env), = self.execs
        self.assertNotIn("CHARTER_SESSION_ID", env)
        self.assertEqual(env["CHARTER_HARNESS"], "claude-code")

    def test_a_persona_pin_typed_before_no_frame_still_reaches_the_harness(self):
        """A pin: `bypass` carried these before profiles, and they are pins an operator
        means — `CHARTER_PERSONA=forge charter claude --no-frame` says what it says."""
        with mock.patch.dict(os.environ, {"CHARTER_PERSONA": "forge",
                                          "CHARTER_WORKSPACE": "alpha",
                                          "CHARTER_SESSION_ID": "alpha.1"}):
            self._launch(profile="claude-work", no_frame=True)
        (_program, _argv, env), = self.execs
        self.assertEqual(env["CHARTER_PERSONA"], "forge")
        self.assertEqual(env["CHARTER_WORKSPACE"], "alpha")
        self.assertEqual(env["CHARTER_ROOT"], str(config.ROOT))

    def test_a_no_frame_launch_of_a_declared_profile_is_refused_like_any_other(self):
        """The `--no-frame` harness gets the same checks — no flag launches a profile
        unguarded."""
        with mock.patch.object(launcher, "_approval_refusal",
                               return_value=launcher.Refusal(launcher.KIND_NOT_YET, "no",
                                                             launcher.REFUSED_EXIT)):
            rc = self._launch(profile="claude-work", no_frame=True)
        self.assertEqual(rc, launcher.REFUSED_EXIT)
        self.assertEqual(self.execs, [])


class AnEarlyDeathNamesTheProfilesCommand(_ALaunchNamesAProfile, unittest.TestCase):
    def setUp(self) -> None:
        super().setUp()
        self._no_approval_needed()

    def test_an_early_death_names_the_profiles_command_not_the_launcher(self):
        """What the operator reads has to be the command they wrote down, not
        `python -P -m charter frame-launch --profile …`."""
        with mock.patch.object(commands_frame, "_pane_last_words", return_value=[]):
            rc, said = self._said(profile="claude-work", dead_status=3)
        self.assertEqual(rc, 3)
        self.assertIn("claude", said)
        self.assertNotIn("frame-launch", said)


class AnUnattendedLaunchReadsTheLauncherssVerdict(_ALaunchNamesAProfile, unittest.TestCase):
    """Ruling 42's second half. Nobody is at a reopened or handed-off chat to press a key,
    so the pane records what it did and the launch that opened it reports that."""

    def setUp(self) -> None:
        super().setUp()
        self._no_approval_needed()

    def test_a_refusal_the_pane_recorded_is_what_the_operator_is_told(self):
        rc, said = self._said(
            profile="claude-work", attach=False,
            opening=commands_frame.Opening("fix it please"),
            verdict=(launcher.REFUSED_EXIT, "profile 'claude-work' is refused — nope"))
        self.assertEqual(rc, launcher.REFUSED_EXIT)
        self.assertIn("is refused — nope", said)

    def test_an_attended_launch_waits_for_nothing_because_the_pane_holds_its_own(self):
        """An attended pane shows its refusal and waits for a key, so the launch that opened
        it must not also stop to read a verdict — it is about to attach."""
        self._launch(profile="claude-work")
        self.awaited.assert_not_called()

    def test_an_unattended_launch_is_the_one_that_asks(self):
        self._launch(profile="claude-work", attach=False,
                     opening=commands_frame.Opening("fix it please"))
        self.awaited.assert_called_once()


class TheVerdictWaitStopsOnItsOwnTerms(PersonaIso, unittest.TestCase):
    """`_await_the_launcher` itself: it ends on the verdict, on the pane dying, or on its
    own budget — and never on a sleep nobody bounded."""

    def setUp(self) -> None:
        super().setUp()
        make_plane(self)
        state.frame_dir("beta.1", create=True)
        self.slept: list[float] = []
        self.enterContext(mock.patch.object(commands_frame.time, "sleep",
                                            side_effect=self.slept.append))

    def _await(self, alive: bool = True):
        status = commands_frame._ALIVE if alive else commands_frame._GONE
        with mock.patch.object(commands_frame, "_pane_state", return_value=(status, None)):
            return commands_frame._await_the_launcher("sock", "beta.1", "%1")

    def test_a_launcher_that_handed_the_pane_over_is_not_waited_on(self):
        state.record_launch("beta.1")
        self.assertEqual(self._await(), (None, ""))
        self.assertEqual(self.slept, [], "a chat that started fine must not cost a wait")

    def test_a_refusal_comes_back_with_its_own_exit_code(self):
        state.record_launch("beta.1", launcher.MISSING_EXIT, "not on PATH")
        self.assertEqual(self._await(), (launcher.MISSING_EXIT, "not on PATH"))

    def test_a_pane_that_died_without_a_verdict_is_not_waited_on_either(self):
        self.assertEqual(self._await(alive=False), (None, ""))
        self.assertEqual(self.slept, [])

    def test_a_launcher_that_never_answers_is_given_up_on(self):
        """A budget, not a hang: a pane charter could not prove a chat for records nothing,
        and a reopen of four chats must not stop on it."""
        ticks = iter([0.0, 0.0, commands_frame._LAUNCHER_SECONDS + 1])
        with mock.patch.object(commands_frame.time, "monotonic",
                               side_effect=lambda: next(ticks)):
            self.assertEqual(self._await(), (None, ""))


class TheOperatorsTmuxRespawnsTheLauncher(_ALaunchNamesAProfile, unittest.TestCase):
    """Ruling 4: inside a tmux the operator already has, the `cat` placeholder stays — it is
    what lets `remain-on-exit` be set before anything in the pane can exit (#384) — and the
    launcher is what `respawn-pane` starts, where the harness argv went."""

    def setUp(self) -> None:
        super().setUp()
        self._no_approval_needed()
        self.chat_option_rc = 0

    def _run(self, action, argv, **kw):
        self.argvs.append(list(argv))
        rc = 0
        if "set-option" in argv and commands_frame._CHAT_OPTION in argv:
            rc = self.chat_option_rc
        out = "@1 %9\n" if "new-window" in argv else "%9\n"
        return subprocess.CompletedProcess(argv, rc, out, "")

    def _in_operator_tmux(self, **ns) -> tuple[int, str]:
        ns.setdefault("operator", (_OPERATOR, "$1"))
        with mock.patch.object(commands_frame, "_pane_state",
                                  return_value=(commands_frame._ALIVE, None)), \
                mock.patch.object(commands_frame, "_wait_for_harness", return_value=0), \
                mock.patch.object(commands_frame, "_window_size", return_value=(120, 40)), \
                mock.patch.object(commands_frame, "_reap_this_server"), \
                mock.patch.object(commands_frame, "_live_windows", return_value=set()):
            return self._said(**ns)

    def test_the_window_starts_on_the_placeholder_and_respawns_the_launcher(self):
        rc, _said = self._in_operator_tmux(profile="claude-work")
        self.assertEqual(rc, 0)
        opened = next(a for a in self.argvs if "new-window" in a)
        self.assertEqual(opened[-2:], ["--", *layout.PLACEHOLDER])
        respawned = next(a for a in self.argvs if "respawn-pane" in a)
        self.assertEqual(respawned[-5:], ["frame-launch", "--profile", "claude-work",
                                          "--attended", "--"])
        self.assertLess(self.argvs.index(next(a for a in self.argvs
                                              if "remain-on-exit" in a)),
                        self.argvs.index(respawned))

    def test_it_fails_closed_and_loudly_when_the_chat_option_does_not_take(self):
        """The launcher proves its chat by `@charter_chat` on this server (ruling 33). A
        window that could not be told which chat it is would make every launcher in it
        unproven — running with no frame, recording nothing, and silently — so the launch
        stops instead."""
        self.chat_option_rc = 1
        rc, said = self._in_operator_tmux(profile="claude-work")
        self.assertNotEqual(rc, 0)
        self.assertIn("chat", said.lower())
        self.assertFalse([a for a in self.argvs if "respawn-pane" in a],
                         "nothing may be started in a window charter could not name")
        self.assertTrue([a for a in self.argvs if "kill-window" in a],
                        "the window the operator never asked for is taken back")


if __name__ == "__main__":
    unittest.main()
