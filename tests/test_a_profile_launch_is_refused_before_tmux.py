"""A profile that may not run is refused before tmux is asked for anything.

Each guard runs before tmux wherever a terminal is attached — a refusal there is a `return`,
with no session, no window and no chat directory to tear down — and again in the pane
immediately before the exec, because the plane can move in between (spec, *What guards a
launch*). This module is the first half: what `_launch` refuses, what it hands tmux when it
does not, and what never crosses tmux at all.

**No declared profile runs here until somebody has seen its command** (review B1, and
Task 3's ask in place of Task 2's flat refusal). `charter.local.toml` is a file a chat can
write with no diff to show for it, so a profile with no launch record shows what it would
run and asks first — through `charter <profile>`, `+`, a tab, reopen and a handoff alike.
The three cases that say so run without the record the rest of the module seeds, and every
other case here seeds one because it is about something else entirely.
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

from charter import commands_frame, config, contain, profiletrust
from charter.frame import launcher, layout, reopen as reopen_state, state, tmuxctl
from tests import _gitguard, _tmuxsocket
from tests._isolation import (APipe as _APipe, ATerminal as _ATerminal, PersonaIso,
                              Typed as _Typed, approve_every_profile, declare_profiles,
                              make_plane, wired_as_today)


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


#: The operator's own tmux, as `tmuxctl.operator_server` reads it out of `$TMUX`: a socket
#: PATH, which is what `is_operator_socket` tells from charter's own `-L charter` name.
#: Computed the way tmux computes it (`tests/_tmuxsocket.py`) rather than written down —
#: a literal uid in a socket path is one developer's machine baked into the suite.
_OPERATOR = _tmuxsocket.OPERATOR_SOCKET


class _ALaunchNamesAProfile(PersonaIso):
    """The real `_launch`, with tmux and everything that would start a process stood in —
    `tests/test_a_chat_opens_in_the_background_with_its_first_message.py`'s own patch set."""

    def setUp(self) -> None:
        super().setUp()
        make_plane(self)
        (config.WORKSPACES_DIR / "beta").mkdir(parents=True, exist_ok=True)
        self.enterContext(mock.patch.dict(
            os.environ,
            {"PATH": os.environ.get("PATH", ""),
             "CHARTER_ROOT": str(config.ROOT), **_gitguard.environment()}, clear=True))
        # AFTER the clearing patch, so its `$HOME` and `$GIT_CEILING_DIRECTORIES` survive:
        # the ignore check is a real `git status`, and both decide what it answers.
        self.local = declare_profiles(self)
        self.argvs: list[list[str]] = []
        self.execs: list[tuple] = []
        self.enterContext(mock.patch.object(launcher.os, "execvpe",
                                            side_effect=lambda *a: self.execs.append(a)))

    def _profile(self, name: str = "claude-work"):
        from charter import profiles

        return profiles.current().profiles[name]

    def _no_approval_needed(self) -> None:
        """Seed a launch record for every profile this plane declares.

        A declared profile charter has never run shows its command and asks (Task 3), so a
        case about something else — which argv crosses tmux, what a `PATH` refusal says —
        would otherwise be measuring that one question. This is what an operator who
        already said yes once leaves behind, which is the state those cases mean.

        Called again by a case that REWRITES `charter.local.toml`, because it records what
        is declared at the moment it runs.
        """
        approve_every_profile(self)

    def _tmux_answer(self, action, argv, **kw):
        self.argvs.append(list(argv))
        out = "%9\n"
        if "new-window" in argv and "-t" in argv and _OPERATOR in argv:
            out = "@1 %9\n"
        return subprocess.CompletedProcess(argv, 0, out, "")

    def _launch(self, *, which="/nowhere/claude", dead_status=None,
                verdict: tuple = (None, ""), operator=None, stdin=None, stdout=None,
                **ns) -> int:
        """The real `_launch` with tmux stood in — and every answer a case might care about
        as a PARAMETER rather than a patch applied inside here.

        The existing launcher tests record why (`_harness_binary_installed`): these patches
        are entered inside this helper, so they override anything a case set up outside it,
        and the case about a missing binary was answered "installed" along with everything
        else. A knob a case can state is the only way that stays honest.

        *stdin* and *stdout* are the same knob for the ask (Task 3): a stream given here
        REPLACES the interpreter's, which is how a case declares its terminal-ness to
        `tests/_ttyguard` and the only way to count what was read. Given neither, this is
        the open a press makes — a terminal for output, nothing to read from — which is
        what every case written before the ask existed assumed.
        """
        args = SimpleNamespace(**{"harness": "claude", "rest": [], "no_frame": False,
                                  "workspace": "beta", "pick": False, "size": (120, 40),
                                  **ns})
        streams = [mock.patch("sys.stdin", stdin) if stdin is not None
                   else mock.patch("sys.stdin.isatty", return_value=False),
                   mock.patch("sys.stdout", stdout) if stdout is not None
                   else mock.patch("sys.stdout.isatty", return_value=True)]
        for s in streams:
            self.enterContext(s)
        # *which* is one answer for every program, or a function of the program's name for
        # a case where the answer depends on WHICH program is asked (a wrapper and the
        # harness it wraps, `tests/test_a_harness_runs_through_the_command_you_name.py`).
        answer = {"side_effect": which} if callable(which) else {"return_value": which}
        with mock.patch("charter.commands_frame.shutil.which", **answer), \
                mock.patch.object(launcher.shutil, "which", **answer), \
                mock.patch.object(tmuxctl, "version", return_value=(3, 7)), \
                mock.patch.object(tmuxctl, "operator_server", return_value=operator), \
                mock.patch.object(commands_frame, "_live_sessions", return_value={"beta"}), \
                mock.patch.object(commands_frame, "_live_chats", return_value=set()), \
                mock.patch.object(tmuxctl, "run", side_effect=self._tmux_answer), \
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


class ADeclaredProfileAsksBeforeItRuns(_ALaunchNamesAProfile, unittest.TestCase):
    """Review B1, now that Task 3 has put the ask where Task 2's flat refusal stood: the
    three cases that run WITHOUT a launch record. Nothing a chat wrote into
    `charter.local.toml` starts until somebody at the keyboard has seen the command.
    """

    def test_a_declared_profile_with_no_record_asks_before_it_runs(self):
        typed = _Typed("n\n")
        rc = self._launch(profile="claude-work", stdin=typed,
                          stdout=(screen := _ATerminal()))
        self.assertEqual(rc, profiletrust.DECLINED_EXIT)
        self.assertIn("run this? [y/N]", screen.getvalue())
        self.assertIn("command  claude", screen.getvalue())
        self.assertEqual(typed.reads, 1)
        self.assertFalse(self._started(), self.argvs)
        self.assertEqual(self.execs, [])

    def test_a_declared_replacement_of_a_built_in_asks_before_it_runs(self):
        """`[harness.claude]` is the file speaking, whatever the name on the table is —
        and the command on it is the file's, which is the whole question."""
        self.local.write_text('[harness.claude]\nkind = "claude"\n'
                              'command = ["/opt/claude"]\n')
        rc = self._launch(stdin=_Typed("n\n"), stdout=(screen := _ATerminal()))
        self.assertEqual(rc, profiletrust.DECLINED_EXIT)
        self.assertIn("command  /opt/claude", screen.getvalue())
        self.assertFalse(self._started(), self.argvs)

    def test_a_built_in_the_file_does_not_replace_still_launches(self):
        self.assertEqual(self._launch(harness="codex", stdin=_Typed()), 0)
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

    def test_the_name_that_refusal_repeats_back_is_bounded(self):
        """`profiles.NAME_RE` bounds a declared name's shape and not its length, so this is
        the one value in the sentence that arrives printable and as long as the file liked.
        The sentence ends in the remedy — `Run it by its own name: charter <name>` — and an
        unbounded name in the middle of it is a remedy nobody gets to."""
        long_name = "c" * 200
        self.local.write_text(f'[harness.{long_name}]\nkind = "codex"\n'
                              'command = ["codex"]\n')
        rc, said = self._said(profile=long_name)
        self.assertEqual(rc, 2)
        self.assertIn("c" * 160 + "...", said)
        self.assertNotIn(long_name, said)

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

    def test_a_no_frame_launch_of_a_declared_profile_is_asked_about_like_any_other(self):
        """The `--no-frame` harness gets the same checks — no flag launches a profile
        unguarded — and here the question is put on this process's own terminal, because
        this process is the one that becomes the harness."""
        # The record seeded in `setUp` is what a case about something else wants; this one
        # is about the ask, so it starts from a plane that has never run this profile.
        (config.STATE_DIR / profiletrust.RECORD).unlink()
        rc = self._launch(profile="claude-work", no_frame=True, stdin=_Typed("n\n"),
                          stdout=_ATerminal())
        self.assertEqual(rc, profiletrust.DECLINED_EXIT)
        self.assertEqual(self.execs, [])

    def test_a_no_frame_launch_with_nowhere_to_ask_refuses_and_reads_nothing(self):
        """Review 3: `charter claude-work --no-frame > log` has somebody at the keyboard
        and no terminal to put the question on. It says so — rather than reading a pipe,
        which returns at once and looks exactly like a question that was answered."""
        (config.STATE_DIR / profiletrust.RECORD).unlink()
        rc, said = self._said(profile="claude-work", no_frame=True, stdin=_APipe())
        self.assertEqual(rc, launcher.REFUSED_EXIT)
        self.assertIn("no terminal here to ask in", said)
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


class AnUnattendedLaunchReadsTheLaunchersVerdict(_ALaunchNamesAProfile, unittest.TestCase):
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

    def test_a_pane_that_already_died_is_not_asked_a_second_time(self):
        """Two answers about one pane, and the eager one is the answer: tmux itself reported
        `#{pane_dead_status}`, so the harness RAN and then exited on that number. Reading a
        launcher record over the top of it would report a refusal for a chat that started
        perfectly well and failed afterwards — and on the wrong exit code."""
        with mock.patch.object(commands_frame, "_pane_last_words", return_value=[]):
            rc, said = self._said(profile="claude-work", attach=False,
                                  opening=commands_frame.Opening("fix it please"),
                                  dead_status=7,
                                  verdict=(3, "a refusal from some other launch"))
        self.assertEqual(rc, 7)
        self.assertIn("7", said)
        self.assertNotIn("some other launch", said)

    def test_a_launch_with_no_profile_has_no_launcher_to_ask(self):
        """`charter frame -- <cmd>` starts the command itself — no charter launcher runs in
        that pane, so nothing there can have recorded a verdict. A record left under that
        chat id by an earlier chat is not this launch's answer, and waiting for one would
        cost the whole budget on every escape-hatch open."""
        rc, said = self._said(harness="frame", rest=["--", "true"], attach=False,
                              opening=commands_frame.Opening("fix it please"),
                              verdict=(3, "a refusal from some other launch"))
        self.assertEqual(rc, 0)
        self.assertNotIn("some other launch", said)


class TheVerdictWaitStopsOnItsOwnTerms(PersonaIso, unittest.TestCase):
    """`_await_the_launcher` itself: it ends on the verdict, on the pane dying, or on its
    own budget — and never on a sleep nobody bounded."""

    def setUp(self) -> None:
        super().setUp()
        make_plane(self)
        state.frame_dir("beta.1", create=True)
        self.slept: list[float] = []
        self.enterContext(mock.patch.object(commands_frame.time, "sleep",
                                            side_effect=self._sleep))

    #: A bound rather than a recorder, because the failure this class is about is a WAIT
    #: that does not end: a loop with its deadline gone would spin here forever and a
    #: hanging test is not a verdict (`tests/_ttyguard.py` records the same lesson). Well
    #: above what any case below asks for, so only a genuinely unbounded loop reaches it.
    _POLL_CEILING = 100

    def _sleep(self, seconds: float) -> None:
        self.slept.append(seconds)
        if len(self.slept) > self._POLL_CEILING:
            raise AssertionError(
                f"the wait polled {len(self.slept)} times: its budget is not bounding it")

    def _verdict(self, alive: bool = True):
        """What `_await_the_launcher` answers with the pane in the state named."""
        status = commands_frame._ALIVE if alive else commands_frame._GONE
        with mock.patch.object(commands_frame, "_pane_state", return_value=(status, None)):
            return commands_frame._await_the_launcher("sock", "beta.1", "%1")

    def test_a_launcher_that_handed_the_pane_over_is_not_waited_on(self):
        state.record_launch("beta.1")
        self.assertEqual(self._verdict(), (None, ""))
        self.assertEqual(self.slept, [], "a chat that started fine must not cost a wait")

    def test_a_refusal_comes_back_with_its_own_exit_code(self):
        state.record_launch("beta.1", launcher.MISSING_EXIT, "not on PATH")
        self.assertEqual(self._verdict(), (launcher.MISSING_EXIT, "not on PATH"))

    def test_a_pane_that_died_without_a_verdict_is_not_waited_on_either(self):
        self.assertEqual(self._verdict(alive=False), (None, ""))
        self.assertEqual(self.slept, [])

    def test_a_launcher_that_never_answers_is_given_up_on(self):
        """A budget, not a hang: a pane charter could not prove a chat for records nothing,
        and a reopen of four chats must not stop on it."""
        ticks = iter([0.0, 0.0, commands_frame._LAUNCHER_SECONDS + 1])
        with mock.patch.object(commands_frame.time, "monotonic",
                               side_effect=lambda: next(ticks)):
            self.assertEqual(self._verdict(), (None, ""))

    def test_the_budget_is_spent_at_the_deadline_and_not_one_poll_later(self):
        """The clock is asked twice — once to set the deadline, once to find it reached —
        and that second reading ends the wait, because the budget is spent AT it and not
        after it. The third tick is left in the iterator on purpose: a wait that consumed
        it polled a pane, and slept, on time it had already been told it did not have.
        """
        ticks = iter([0.0, float(commands_frame._LAUNCHER_SECONDS), 99.0])
        with mock.patch.object(commands_frame.time, "monotonic",
                               side_effect=lambda: next(ticks)):
            self.assertEqual(self._verdict(), (None, ""))
        self.assertEqual(next(ticks), 99.0)
        self.assertEqual(self.slept, [])


class TheOperatorsTmuxRespawnsTheLauncher(_ALaunchNamesAProfile, unittest.TestCase):
    """Ruling 4: inside a tmux the operator already has, the `cat` placeholder stays — it is
    what lets `remain-on-exit` be set before anything in the pane can exit (#384) — and the
    launcher is what `respawn-pane` starts, where the harness argv went."""

    def setUp(self) -> None:
        super().setUp()
        self._no_approval_needed()
        self.chat_option_rc = 0

    def _tmux_answer(self, action, argv, **kw):
        self.argvs.append(list(argv))
        rc = 0
        if "set-option" in argv and commands_frame._CHAT_OPTION in argv:
            rc = self.chat_option_rc
        out = "@1 %9\n" if "new-window" in argv else "%9\n"
        return subprocess.CompletedProcess(argv, rc, out, "")

    def _in_operator_tmux(self, *, harness_exit: int | None = 0,
                          pane_state: tuple = (commands_frame._ALIVE, None),
                          **ns) -> tuple[int, str]:
        """*harness_exit* is what `_wait_for_harness` answers — ``None`` for a pane that is
        no longer there to be asked — and *pane_state* what the EAGER ask right after the
        respawn finds. Parameters rather than patches applied inside here, for the reason
        `_launch` gives about its own knobs: the two are different moments in this
        function's life and a case has to be able to name which one it is about."""
        ns.setdefault("operator", (_OPERATOR, "$1"))
        with mock.patch.object(commands_frame, "_pane_state", return_value=pane_state), \
                mock.patch.object(commands_frame, "_wait_for_harness",
                                  return_value=harness_exit), \
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

    def test_a_pane_that_refused_and_vanished_still_reports_its_own_number(self):
        """**The launcher's number, not tmux's reading of a pane that is not there.**
        `remain-on-exit` is armed on this path, and nothing guarantees it took — a pane
        that is gone answers no `#{pane_dead_status}` at all, and `_wait_for_harness`
        reports that as `None` exactly as it does for a window the operator closed. The
        launcher wrote down what it was exiting with BEFORE it exited, which is better
        evidence than anything reconstructed afterwards. Measured on a Linux runner, where
        this path answered `_UNKNOWN_DEATH_CODE` for the same refusal charter's own server
        reported as 3 — and the sentence went with it.
        """
        rc, said = self._in_operator_tmux(
            profile="claude-work", harness_exit=None,
            verdict=(launcher.REFUSED_EXIT, "profile 'claude-work' is refused — nope"))
        self.assertEqual(rc, launcher.REFUSED_EXIT)
        self.assertIn("is refused — nope", said)
        self.assertNotIn("is not something charter can know", said)

    def test_a_pane_already_dead_when_the_frame_is_built_reports_the_refusal(self):
        """The EAGER ask, one moment after the respawn: a launcher fast enough to refuse
        before this line runs leaves a pane that is already over, and nothing below this
        branch ever runs — no panels, no `select-window`, so the operator is never switched
        to the window at all. The chat's own record is the only copy of the sentence, and
        the number the launcher exited with is the launch's."""
        rc, said = self._in_operator_tmux(
            profile="claude-work",
            pane_state=(commands_frame._GONE, None),
            verdict=(launcher.REFUSED_EXIT, "profile 'claude-work' is refused — nope"))
        self.assertEqual(rc, launcher.REFUSED_EXIT)
        self.assertIn("is refused — nope", said)

    def test_a_pane_already_dead_with_no_refusal_keeps_tmuxs_own_number(self):
        """And the other way down the same branch: a harness that RAN and failed has no
        launcher record to prefer, so what tmux reported about the pane is the answer — and
        the operator is told what died, because nothing else on this path will."""
        with mock.patch.object(commands_frame, "_pane_last_words", return_value=[]):
            rc, said = self._in_operator_tmux(profile="claude-work",
                                              pane_state=(commands_frame._DEAD, 7))
        self.assertEqual(rc, 7)
        self.assertIn("7", said)

    def test_a_pane_that_vanished_with_nothing_recorded_says_nothing_it_cannot_know(self):
        """`code` is `None` and the launcher recorded nothing: charter knows the pane is
        gone and knows nothing else. An early-death sentence built on that would name an
        exit code nobody has — so the branch that writes one is asked whether there IS a
        number first, and this path says nothing at all."""
        rc, said = self._in_operator_tmux(profile="claude-work",
                                          pane_state=(commands_frame._GONE, None))
        self.assertEqual(rc, commands_frame._UNKNOWN_DEATH_CODE)
        self.assertEqual(said, "")

    def test_a_launch_with_no_profile_has_no_launcher_record_to_read_here_either(self):
        """`charter frame -- <cmd>` starts the command itself on this path too: no charter
        launcher runs in that pane, so a record left under that chat id by an earlier chat
        is not this launch's verdict."""
        rc, said = self._in_operator_tmux(
            harness="frame", rest=["--", "true"], pane_state=(commands_frame._GONE, None),
            verdict=(launcher.REFUSED_EXIT, "a refusal from some other launch"))
        self.assertEqual(rc, commands_frame._UNKNOWN_DEATH_CODE)
        self.assertNotIn("some other launch", said)

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
