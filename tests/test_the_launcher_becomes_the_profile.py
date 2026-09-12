"""Every chat pane's first process is charter's launcher, and it becomes the harness.

The launcher runs the checks in the pane — the file is still ignored, the command is still
on `PATH`, the profile is approved — and then `os.execvpe`s the profile's command with the
profile's `env` applied. Three reasons, the first a constraint (spec, *How a harness
starts*): `env` reaches the harness without `layout.CARRIABLE` and without tmux's own
argument parser (#957, #961); one place runs the checks for every open; and `exec` keeps
the launcher's pid, so the pane id, `remain-on-exit` and the `pane-died` path see what they
see today. Measured before it was built on — `workspaces/harness-profiles/refs/task2-measure/`.

**A launcher is in a frame only when tmux says so** (rulings 29 and 33). Its own pid must be
the `#{pane_pid}` of a LIVE pane belonging to the chat it claims, read from tmux on that
chat's server: the window name on charter's own server, the `@charter_chat` option in the
operator's. `$TMUX_PANE` and `$CHARTER_SESSION_ID` are never proof — a model's own tool
shell inherits both from its chat, measured — so `charter frame-launch` run from one is a
launch with no frame that rewrites no chat's record.

**A refusal in the pane has to reach the operator without the dead pane** (ruling 42,
measured 2026-09-11): `_launch`'s eager dead-status ask completes 6-14 ms after the start
while a Python launcher's first line runs at 19-22 ms, and the teardown hook then kills the
window, so all 40 launcher refusals were missed. An attended launcher shows its refusal in
the pane and waits for a key; an unattended one writes it into the chat's state directory,
where the launch that opened the chat reads it back.
"""

from __future__ import annotations

import os
import subprocess
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame, config, profiles
from charter.frame import launcher, state, tmuxctl
from tests import _gitguard, _tmuxsocket
from tests._isolation import PersonaIso, declare_profiles, make_plane

#: The `list-panes -a` row `tmuxctl.live_pane_by_pid` parses: pid, dead, pane, session,
#: window, `@charter_chat`. Spelled here rather than built from the format, so a change to
#: either is a test that fails rather than two halves that move together.
def _row(pid: int, *, dead: str = "0", pane: str = "%1", session: str = "alpha",
         window: str = "alpha.1", chat: str = "alpha.1") -> str:
    return f"{pid}\t{dead}\t{pane}\t{session}\t{window}\t{chat}\n"


class _AProfileAndAPlane(PersonaIso):
    """A plane declaring `claude-work`, in a git repository that ignores the local file.

    The ignore check is a real `git status` — the one subprocess `profiles` runs — so the
    fixture is a real repository. `GIT_CEILING_DIRECTORIES` states where the walk stops, so
    a temp directory inside somebody's own checkout is not read as belonging to it.
    """

    def setUp(self) -> None:
        super().setUp()
        make_plane(self)
        self.local = declare_profiles(self)
        self.execs: list[tuple] = []
        self.exec = self.enterContext(mock.patch.object(
            launcher.os, "execvpe", side_effect=lambda *a: self.execs.append(a)))

    def _profile(self, name: str = "claude-work") -> profiles.Profile:
        return profiles.current().profiles[name]

    def _no_approval_needed(self) -> None:
        """Stand in for Task 2's B1 refusal, for a case that is about something else.

        Task 2 refuses every declared profile (review B1), so without this every case below
        would be measuring that one sentence. Task 3 replaces the body it stands in for.
        """
        self.enterContext(mock.patch.object(launcher, "_approval_refusal",
                                            return_value=None))


class TheLauncherBecomesTheProfile(_AProfileAndAPlane, unittest.TestCase):
    def test_the_pane_refuses_a_declared_profile_until_approval_exists(self):
        """Review B1: `main` must never run an unapproved declared command between two
        merges, so Task 2 refuses every declared profile and Task 3 lifts it."""
        rc = launcher.start(self._profile(), [], fid="beta.1", attended=False)
        self.assertEqual(rc, launcher.REFUSED_EXIT)
        self.assertEqual(self.execs, [])

    def test_a_built_in_the_file_does_not_replace_still_launches(self):
        rc = launcher.start(self._profile("claude"), [], fid=None, attended=False)
        self.assertEqual(rc, 0)
        self.assertEqual(self.execs[0][0], "claude")

    def test_an_unframed_environment_drops_only_the_session_id(self):
        """Review 8: the defect is the PAIR — an inherited `$CHARTER_SESSION_ID` beside the
        launched `CHARTER_HARNESS` makes `hooks._record_harness_session` write a harness's
        session id into another chat's state. The other three are pins an operator means."""
        base = {"CHARTER_SESSION_ID": "a.1", "CHARTER_ROOT": "/p",
                "CHARTER_PERSONA": "forge", "CHARTER_HARNESS": "codex"}
        env = launcher.environment(self._profile(), base, framed=False)
        self.assertNotIn("CHARTER_SESSION_ID", env)
        self.assertEqual(env["CHARTER_ROOT"], "/p")
        self.assertEqual(env["CHARTER_PERSONA"], "forge")
        self.assertEqual(env["CHARTER_HARNESS"], "claude-code")
        framed = launcher.environment(self._profile(), base, framed=True)
        self.assertEqual(framed["CHARTER_SESSION_ID"], "a.1")

    def test_the_env_is_the_profiles_over_this_process(self):
        env = launcher.environment(self._profile(),
                                   {"PATH": "/x", "CLAUDE_CONFIG_DIR": "/old"}, framed=True)
        self.assertEqual(env["PATH"], "/x")
        self.assertEqual(env["CLAUDE_CONFIG_DIR"], str(self.home / ".cw"))
        self.assertEqual(env["CHARTER_HARNESS_PROFILE"], "claude-work")

    def test_the_profile_and_kind_ride_the_exec_not_tmux(self):
        self._no_approval_needed()
        launcher.start(self._profile(), ["--resume", "s1"], fid="beta.1", attended=False)
        (program, argv, env), = self.execs
        self.assertEqual(program, "claude")
        self.assertEqual(argv, ["claude", "--resume", "s1"])
        self.assertEqual(env["CHARTER_HARNESS_PROFILE"], "claude-work")
        self.assertEqual(env["CHARTER_HARNESS"], "claude-code")
        self.assertEqual(env["CLAUDE_CONFIG_DIR"], str(self.home / ".cw"))
        self.assertEqual(state.profile("beta.1"), "claude-work")

    def test_a_file_that_became_committable_since_tmux_is_refused_in_the_pane(self):
        """The pane re-runs the chain because the plane can move between the pre-tmux check
        and the exec — which is the whole reason the checks run twice."""
        self._no_approval_needed()
        (config.ROOT / ".gitignore").write_text("")
        r = launcher.refusal(self._profile(), root=config.ROOT, attended=False,
                             env=dict(os.environ))
        self.assertEqual(r.kind, launcher.KIND_IGNORED)
        self.assertIn("charter reinit", r.text)
        self.assertEqual(launcher.start(self._profile(), [], fid=None, attended=False),
                         launcher.REFUSED_EXIT)
        self.assertEqual(self.execs, [])

    def test_a_built_in_is_not_refused_over_the_local_file(self):
        """Ruling 19 applies to what the file DECLARES. A built-in no declared profile
        replaces runs whatever git would do with a file it does not come from."""
        self._no_approval_needed()
        (config.ROOT / ".gitignore").write_text("")
        self.assertIsNone(launcher.refusal(self._profile("claude"), root=config.ROOT,
                                           attended=False, env=dict(os.environ)))

    def test_a_command_removed_since_tmux_is_refused_in_the_pane(self):
        self._no_approval_needed()
        with mock.patch.object(launcher.shutil, "which", return_value=None):
            rc = launcher.start(self._profile(), [], fid=None, attended=False)
        self.assertEqual(rc, launcher.MISSING_EXIT)
        self.assertEqual(self.execs, [])

    def test_a_command_that_vanished_between_which_and_exec_is_127(self):
        self._no_approval_needed()
        self.exec.side_effect = FileNotFoundError(2, "No such file or directory")
        r = launcher.attempt(self._profile(), [], fid=None, attended=False)
        self.assertEqual(r.kind, launcher.KIND_EXEC)
        self.assertEqual(r.exit, 127)

    def test_a_command_that_cannot_be_executed_is_126(self):
        self._no_approval_needed()
        self.exec.side_effect = PermissionError(13, "Permission denied")
        self.assertEqual(launcher.attempt(self._profile(), [], fid=None,
                                          attended=False).exit, 126)

    def test_an_exec_that_fails_after_on_exec_undoes_it(self):
        """N7's nit: what `on_exec` recorded is undone, so a pane that is no longer running
        anything does not also claim to be a chat."""
        self._no_approval_needed()
        self.exec.side_effect = OSError(5, "Input/output error")
        undone: list[int] = []
        r = launcher.attempt(self._profile(), [], fid=None, attended=False,
                             on_exec=lambda: lambda: undone.append(1))
        self.assertEqual(undone, [1])
        self.assertEqual(r.kind, launcher.KIND_EXEC)
        self.assertEqual(r.exit, launcher.REFUSED_EXIT)
        self.assertIn("Input/output error", r.text)

    def test_a_refusal_exits_three_and_the_number_is_the_point(self):
        """**3, written out**, because every neighbouring number already means something
        else to whoever reads `$?`: 127 and 126 are the shell's own words for a command
        that could not be found or could not be run (`MISSING_EXIT`, `NOT_EXECUTABLE_EXIT`,
        and `bypass` returns those same two), 2 is charter's own usage error, 1 is a
        harness that ran and failed, and 0 is a harness that ran and did not. A refusal
        started nothing at all, and `charter <profile> || …` can only tell that apart if
        the number is pinned rather than merely consistent with itself.
        """
        self.assertEqual(launcher.start(self._profile(), [], fid=None, attended=False), 3)
        self.assertEqual(launcher.REFUSED_EXIT, 3)
        self.assertEqual(launcher.MISSING_EXIT, 127)
        self.assertEqual(launcher.NOT_EXECUTABLE_EXIT, 126)

    def test_the_kinds_are_a_written_down_vocabulary(self):
        """Ruling 27: a caller branches on the KIND and never on the text, so the kinds are
        an interface — Task 3 matches `KIND_NOT_YET` to decide which refusal a pane defers
        to instead of prints. Spelled out here so a rename is a red test rather than a
        branch that quietly stops matching, and asserted DISTINCT because two kinds that
        collided would make that branch answer for both.
        """
        kinds = (launcher.KIND_IGNORED, launcher.KIND_PATH, launcher.KIND_NOT_YET,
                 launcher.KIND_EXEC)
        self.assertEqual(kinds, ("ignored", "path", "not-yet", "exec"))
        self.assertEqual(len(set(kinds)), len(kinds))

    def test_the_path_the_exec_will_use_is_the_path_that_is_asked(self):
        """**`env["PATH"]`, not this process's**, and the difference is a whole launch: a
        profile whose `env` puts its own directory on `PATH` runs a command charter itself
        cannot see. The command here exists ONLY in the profile's `PATH`, so a check asking
        any other question refuses a profile that is about to start perfectly well.
        """
        self._no_approval_needed()
        elsewhere = self.tmp / "bin"
        elsewhere.mkdir()
        program = elsewhere / "harness-only-here"
        program.write_text("#!/bin/sh\nexit 0\n")
        program.chmod(0o755)
        self.local.write_text('[harness.own-path]\nkind = "claude"\n'
                              'command = ["harness-only-here"]\n')
        p = self._profile("own-path")
        self.assertIsNone(launcher.refusal(p, root=config.ROOT, attended=False,
                                           env={"PATH": str(elsewhere)}))
        # And the other half of the same fact: with that directory gone from `PATH` the
        # very same profile is refused, so the answer above came from the `env` asked for.
        self.assertEqual(launcher.refusal(p, root=config.ROOT, attended=False,
                                          env={"PATH": str(self.tmp)}).kind,
                         launcher.KIND_PATH)


class AQuotedCommandIsShownEscaped(_AProfileAndAPlane, unittest.TestCase):
    """Ruling 35, and the fourth review's nit: a refusal that quotes a command shows it
    escaped. The file is one a chat can write, and a `\\r` or an ESC in a command could
    otherwise redraw the line to show a harmless command while another one runs."""

    def setUp(self) -> None:
        super().setUp()
        self.local.write_text(
            '[harness.sneaky]\nkind = "claude"\n'
            'command = ["cl\\raude\\u001b[2Kharmless"]\n')
        self._no_approval_needed()

    def test_a_command_not_on_path_is_quoted_escaped(self):
        with mock.patch.object(launcher.shutil, "which", return_value=None):
            r = launcher.refusal(self._profile("sneaky"), root=config.ROOT,
                                 attended=False, env=dict(os.environ))
        self.assertNotIn("\r", r.text)
        self.assertNotIn("\x1b", r.text)
        # `contain.readable`'s own spelling for a byte that is not printable ASCII.
        self.assertIn("\\u000d", r.text)

    def test_an_exec_that_failed_quotes_it_escaped(self):
        self.exec.side_effect = FileNotFoundError(2, "No such file")
        with mock.patch.object(launcher.shutil, "which", return_value="/nowhere"):
            r = launcher.attempt(self._profile("sneaky"), [], fid=None, attended=False)
        self.assertNotIn("\r", r.text)
        self.assertNotIn("\x1b", r.text)
        self.assertIn("\\u000d", r.text)

    def test_what_the_system_said_about_the_failure_is_escaped_too(self):
        """The `strerror` is not charter's own words: it comes back from the kernel about a
        path the FILE chose, and on a filesystem where that name is the error message it is
        the file's text arriving on the operator's terminal by another route."""
        self.exec.side_effect = OSError(5, "cannot run cl\raude\x1b[2K")
        with mock.patch.object(launcher.shutil, "which", return_value="/nowhere"):
            r = launcher.attempt(self._profile("sneaky"), [], fid=None, attended=False)
        self.assertNotIn("\r", r.text)
        self.assertNotIn("\x1b", r.text)
        self.assertIn("\\u000d", r.text)

    def test_the_command_charter_shows_for_a_launch_is_escaped(self):
        """`display_command` is what an early death and a tmux that refused the argv both
        quote back, so it is a third way the file's own bytes reach a terminal."""
        shown = launcher.display_command(self._profile("sneaky"), ["--resume", "s1"])
        self.assertEqual(shown[-2:], ["--resume", "s1"])
        self.assertNotIn("\r", " ".join(shown))
        self.assertNotIn("\x1b", " ".join(shown))
        self.assertIn("\\u000d", shown[0])


#: A name `profiles.NAME_RE` accepts and `contain.readable` still has work to do on. The
#: regex bounds a profile name's SHAPE — ASCII letters, digits, `_` and `-` — and says
#: nothing at all about its LENGTH, so a declared name is the one value in these sentences
#: that arrives printable and unbounded. 200 is over `contain.DISPLAY_LIMIT` and under
#: nothing in particular.
LONG_NAME = "w" * 200


class ARefusalNamesTheProfileWithoutBecomingIt(_AProfileAndAPlane, unittest.TestCase):
    """Every refusal quotes the profile's name back, and the quote is BOUNDED.

    `profiles.NAME_RE` is why this is about length rather than about escapes: a name with a
    `\\r` in it never resolves at all, so what reaches these four sentences is always
    printable — and always as long as the file cared to make it. A 200-character name
    unbounded is a refusal that scrolls its own remedy off the screen, and each of these is
    a sentence whose whole job is to be read to the end (CONTEXT.md, *A refusal is the rule
    working*). One fixture, four sites, because it is one property.
    """

    #: What `contain.readable` leaves of :data:`LONG_NAME` — its own clip, spelled out
    #: rather than recomputed, so this test disagrees with the code instead of agreeing
    #: with whatever it happens to do.
    CLIPPED = "w" * 160 + "..."

    def setUp(self) -> None:
        super().setUp()
        self.local.write_text(f'[harness.{LONG_NAME}]\nkind = "claude"\n'
                              'command = ["claude"]\n')

    def _refusal(self, **kw):
        return launcher.refusal(self._profile(LONG_NAME), root=config.ROOT,
                                attended=False, env=dict(os.environ), **kw)

    def test_the_not_yet_refusal_clips_it(self):
        r = self._refusal()
        self.assertEqual(r.kind, launcher.KIND_NOT_YET)
        self.assertIn(self.CLIPPED, r.text)
        self.assertNotIn(LONG_NAME, r.text)

    def test_the_ignored_refusal_clips_it(self):
        (config.ROOT / ".gitignore").write_text("")
        r = self._refusal()
        self.assertEqual(r.kind, launcher.KIND_IGNORED)
        self.assertIn(self.CLIPPED, r.text)
        self.assertNotIn(LONG_NAME, r.text)

    def test_the_not_on_path_refusal_clips_it(self):
        self._no_approval_needed()
        with mock.patch.object(launcher.shutil, "which", return_value=None):
            r = self._refusal()
        self.assertEqual(r.kind, launcher.KIND_PATH)
        self.assertIn(self.CLIPPED, r.text)
        self.assertNotIn(LONG_NAME, r.text)

    def test_the_exec_that_failed_clips_it(self):
        self._no_approval_needed()
        self.exec.side_effect = FileNotFoundError(2, "No such file")
        r = launcher.attempt(self._profile(LONG_NAME), [], fid=None, attended=False)
        self.assertEqual(r.kind, launcher.KIND_EXEC)
        self.assertIn(self.CLIPPED, r.text)
        self.assertNotIn(LONG_NAME, r.text)

    def test_the_not_yet_refusal_offers_only_profiles_the_file_does_not_declare(self):
        """The other half of :data:`launcher.DECLARED_NOT_YET`: it names what the operator
        CAN launch, and a list that repeated the declared profiles back would name the very
        commands this refusal exists to not run."""
        self.local.write_text(f'[harness.{LONG_NAME}]\nkind = "claude"\n'
                              'command = ["claude"]\n\n'
                              '[harness.claude]\nkind = "claude"\ncommand = ["elsewhere"]\n')
        offered = self._refusal().text.rsplit(": ", 1)[1].rstrip(".").split(", ")
        read = profiles.current()
        self.assertTrue(offered)
        for name in offered:
            self.assertEqual(read.profiles[name].source, profiles.BUILTIN, name)


class OnlyThePanesOwnFirstProcessIsFramed(_AProfileAndAPlane, unittest.TestCase):
    """Ruling 29, revised: the pid proof, and what it refuses.

    Measured 2026-09-11 in a real framed chat: its Bash tool shell (pid 4707, ppid 53118)
    saw `TMUX_PANE=%3195`, the harness pane of the chat whose `pane_pid` was 53118 — so a
    model running `charter frame-launch` from its tool would have passed a pane-id proof and
    rewritten that chat's profile record, which reopen follows.
    """

    def setUp(self) -> None:
        super().setUp()
        state.frame_dir("alpha.1", create=True)
        state.record_server("alpha.1", commands_frame.SOCKET)
        state.record_profile("alpha.1", "codex")
        self.rows = _row(53118)
        self.asked: list[list[str]] = []
        self.enterContext(mock.patch.object(
            tmuxctl, "run",
            side_effect=lambda action, argv, **kw: (
                self.asked.append(list(argv)),
                subprocess.CompletedProcess(argv, 0, self.rows, ""))[1]))
        self.enterContext(mock.patch.dict(
            os.environ, {"CHARTER_SESSION_ID": "alpha.1", "TMUX_PANE": "%1",
                         "TMUX": "/tmp/somebody/else,1,0",
                         "CHARTER_ROOT": str(config.ROOT),
                         "PATH": os.environ.get("PATH", ""),
                         **_gitguard.environment()}, clear=True))

    def test_the_panes_own_first_process_is_framed(self):
        with mock.patch.object(launcher.os, "getpid", return_value=53118):
            self.assertEqual(launcher.framed_chat(), "alpha.1")

    def test_a_process_that_is_not_the_panes_first_process_is_not_framed(self):
        with mock.patch.object(launcher.os, "getpid", return_value=4707):
            self.assertIsNone(launcher.framed_chat())

    def test_the_pane_is_asked_of_the_chats_own_server_never_of_tmux(self):
        """`$TMUX` names the socket this process is inside, which is not the same question
        as which server the chat is on (#812). The chat's own record decides."""
        with mock.patch.object(launcher.os, "getpid", return_value=53118):
            launcher.framed_chat()
        self.assertTrue(self.asked)
        for argv in self.asked:
            self.assertIn(commands_frame.SOCKET, argv)
            self.assertNotIn("/tmp/somebody/else", argv)

    def test_a_pane_whose_window_is_not_named_for_the_chat_is_not_framed(self):
        self.rows = _row(53118, window="alpha.2")
        with mock.patch.object(launcher.os, "getpid", return_value=53118):
            self.assertIsNone(launcher.framed_chat())

    def test_a_dead_pane_whose_pid_matches_does_not_prove_the_chat(self):
        """A dead `remain-on-exit` pane keeps its old pid, which the system may have reused
        (ruling 33)."""
        self.rows = _row(53118, dead="1")
        with mock.patch.object(launcher.os, "getpid", return_value=53118):
            self.assertIsNone(launcher.framed_chat())

    def test_a_server_that_does_not_answer_is_no_frame(self):
        with mock.patch.object(tmuxctl, "run", return_value=subprocess.CompletedProcess(
                [], 1, "", "no server running")), \
                mock.patch.object(launcher.os, "getpid", return_value=53118):
            self.assertIsNone(launcher.framed_chat())

    def test_a_tmux_that_failed_is_not_read_even_though_it_printed(self):
        """A `list-panes` that exited non-zero listed SOME panes at best — a server shutting
        down answers for the sessions it has left — and a partial listing is not evidence
        about which pane this process is. The rows here would prove the chat if the code
        read them, which is the whole point: what decides is the return code."""
        with mock.patch.object(tmuxctl, "run", return_value=subprocess.CompletedProcess(
                [], 1, _row(53118), "server exiting")), \
                mock.patch.object(launcher.os, "getpid", return_value=53118):
            self.assertIsNone(launcher.framed_chat())

    def test_a_row_with_more_fields_than_the_format_is_not_read(self):
        """A window NAME may hold a tab and `list-panes` does not quote it, so a row can
        arrive with seven fields where the format asks for six. A row charter cannot assign
        is not a pane it will prove a chat with — and unpacking it would raise inside the
        one function whose whole job is to answer yes or no."""
        self.rows = "53118\t0\t%1\talpha\talpha.1\tstray\talpha.1\n"
        with mock.patch.object(launcher.os, "getpid", return_value=53118):
            self.assertIsNone(launcher.framed_chat())

    def test_an_unprovable_chat_is_repeated_back_escaped(self):
        """`$CHARTER_SESSION_ID` is inherited from whatever started this process, so it is
        the one value in this sentence charter did not mint — and this sentence goes
        straight to a terminal."""
        said: list[str] = []
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "alpha\r.1"}), \
                mock.patch.object(launcher.util, "err", side_effect=said.append), \
                mock.patch.object(launcher.os, "getpid", return_value=4707):
            self.assertIsNone(launcher.framed_chat())
        self.assertEqual(len(said), 1, said)
        self.assertIn("\\u000d", said[0])
        self.assertNotIn("\r", said[0])

    def test_a_claimed_chat_that_cannot_be_proven_says_so_in_one_line(self):
        said: list[str] = []
        with mock.patch.object(launcher.util, "err", side_effect=said.append), \
                mock.patch.object(launcher.os, "getpid", return_value=4707):
            launcher.framed_chat()
        self.assertEqual(len(said), 1, said)
        self.assertIn("alpha.1", said[0])

    def test_a_launch_that_claims_no_chat_says_nothing_about_one(self):
        said: list[str] = []
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": ""}), \
                mock.patch.object(launcher.util, "err", side_effect=said.append):
            self.assertIsNone(launcher.framed_chat())
        self.assertEqual(said, [])


class TheOperatorsTmuxProvesTheChatByItsOption(_AProfileAndAPlane, unittest.TestCase):
    """Ruling 33: there a window name is only a label — an operator hook, a plugin or
    `allow-rename on` output can change it — so the proof is the `@charter_chat` option
    charter sets before `respawn-pane`, which pane output cannot touch."""

    #: Computed the way tmux computes it, never written down: a literal uid in a socket
    #: path is one developer's machine baked into the suite.
    OPERATOR = _tmuxsocket.OPERATOR_SOCKET

    def setUp(self) -> None:
        super().setUp()
        state.frame_dir("alpha.1", create=True)
        state.record_server("alpha.1", self.OPERATOR)
        self.rows = _row(53118, session="op", window="PWNED", chat="alpha.1")
        self.enterContext(mock.patch.object(
            tmuxctl, "run",
            side_effect=lambda action, argv, **kw: subprocess.CompletedProcess(
                argv, 0, self.rows, "")))
        self.enterContext(mock.patch.dict(
            os.environ, {"CHARTER_SESSION_ID": "alpha.1",
                         "CHARTER_ROOT": str(config.ROOT),
                         "PATH": os.environ.get("PATH", ""),
                         **_gitguard.environment()}, clear=True))

    def test_a_renamed_window_still_proves_the_chat_by_its_option(self):
        with mock.patch.object(launcher.os, "getpid", return_value=53118):
            self.assertEqual(launcher.framed_chat(), "alpha.1")

    def test_a_window_named_like_the_chat_is_not_proof_without_the_option(self):
        self.rows = _row(53118, session="op", window="alpha.1", chat="")
        with mock.patch.object(launcher.os, "getpid", return_value=53118):
            self.assertIsNone(launcher.framed_chat())


class ThePaneCommandResolvesTheProfileByName(_AProfileAndAPlane, unittest.TestCase):
    """`charter frame-launch --profile <name> -- <rest>` — what tmux starts in every chat
    pane, and the only thing about a profile that crosses tmux."""

    def setUp(self) -> None:
        super().setUp()
        self.enterContext(mock.patch.dict(
            os.environ, {"CHARTER_ROOT": str(config.ROOT),
                         "PATH": os.environ.get("PATH", ""),
                         **_gitguard.environment()}, clear=True))
        self.enterContext(mock.patch.object(launcher, "framed_chat", return_value=None))
        self.enterContext(mock.patch.object(launcher, "_wait_for_the_operator"))

    def _run(self, **ns) -> int:
        args = SimpleNamespace(**{"profile": "claude-work", "attended": False,
                                  "rest": ["--"], **ns})
        return launcher.cmd_frame_launch(args)

    def test_the_pane_execs_the_profile_it_was_named(self):
        self._no_approval_needed()
        self.assertEqual(self._run(rest=["--", "-p", "x"]), 0)
        (_program, argv, _env), = self.execs
        self.assertEqual(argv, ["claude", "-p", "x"])

    def test_a_name_nothing_declares_is_repeated_back_escaped(self):
        """The name in this sentence came off a command line or out of a chat's own
        record, and the sentence goes to a terminal (ruling 35)."""
        said = launcher.unknown_profile("cl\raude")
        self.assertIn("\\u000d", said)
        self.assertNotIn("\r", said)

    def test_a_pane_asked_for_a_profile_that_is_gone_says_so(self):
        said: list[str] = []
        with mock.patch.object(launcher.util, "err", side_effect=said.append):
            rc = self._run(profile="gone")
        self.assertEqual(rc, launcher.REFUSED_EXIT)
        self.assertTrue(any("no profile named 'gone'" in s for s in said), said)
        self.assertEqual(self.execs, [])

    def test_a_pane_asked_for_a_refused_profile_says_its_own_reason(self):
        self.local.write_text('[harness.bad]\nkind = "claude"\ncommand = "claude -x"\n')
        said: list[str] = []
        with mock.patch.object(launcher.util, "err", side_effect=said.append):
            rc = self._run(profile="bad")
        self.assertEqual(rc, launcher.REFUSED_EXIT)
        self.assertTrue(any("never a shell string" in s for s in said), said)

    def test_each_refused_profile_answers_with_its_own_reason(self):
        """Two refused profiles, two different reasons: a resolve that handed back the
        first refusal it found would tell an operator to fix the wrong line of their own
        file. The refusals are a LIST, and which entry answers is the question here."""
        self.local.write_text('[harness.bad-command]\nkind = "claude"\n'
                              'command = "claude -x"\n\n'
                              '[harness.bad-kind]\nkind = "nosuchharness"\n'
                              'command = ["claude"]\n')
        self.assertIn("never a shell string", launcher.resolve("bad-command")[1])
        self.assertIn("nosuchharness", launcher.resolve("bad-kind")[1])

    def test_a_refused_name_is_looked_up_the_way_it_was_written_down(self):
        """`profiles` records a refused name CONTAINED, because a refusal quotes it; so the
        lookup here has to ask the same question the record answered. A name with a `\\r` in
        it is refused for its shape — and it is exactly the name whose reason goes missing
        if the two halves stop agreeing, which is the silent failure, not a loud one."""
        self.local.write_text('[harness."a\\rb"]\nkind = "claude"\ncommand = ["claude"]\n')
        p, why = launcher.resolve("a\rb")
        self.assertIsNone(p)
        self.assertIn("\\u000d", why)

    def test_the_frame_proof_runs_before_anything_is_printed(self):
        """Ruling 35: `framed_chat()` is the first thing this does — before claiming,
        before drawing, before any output. The name proof holds only while the pane has
        printed nothing."""
        log: list[str] = []
        self._no_approval_needed()
        with mock.patch.object(launcher, "framed_chat",
                               side_effect=lambda: log.append("proof")), \
                mock.patch.object(launcher.util, "err",
                                  side_effect=lambda m: log.append("err")), \
                mock.patch.object(launcher.util, "info",
                                  side_effect=lambda m: log.append("info")), \
                mock.patch("sys.stdout.write", side_effect=lambda m: log.append("out")), \
                mock.patch("sys.stderr.write", side_effect=lambda m: log.append("err")):
            self._run(profile="gone")
        self.assertEqual(log[0], "proof", log)


class ARefusalInThePaneReachesTheOperator(_AProfileAndAPlane, unittest.TestCase):
    """Ruling 42, measured: the eager dead-status ask completes 6-14 ms after the start and
    a Python launcher's first line runs at 19-22 ms, so a refusal printed and exited on is
    never read back off the pane — on charter's own server the teardown hook kills the
    window at once (`_pane_last_words` answered `[]` in all 40 runs), and in the operator's
    tmux `_launch_in_operator_tmux` closes the window before reading it."""

    def setUp(self) -> None:
        super().setUp()
        self.enterContext(mock.patch.dict(
            os.environ, {"CHARTER_ROOT": str(config.ROOT),
                         "PATH": os.environ.get("PATH", ""),
                         **_gitguard.environment()}, clear=True))
        state.frame_dir("beta.1", create=True)
        self.enterContext(mock.patch.object(launcher, "framed_chat",
                                            return_value="beta.1"))

    def test_an_attended_pane_waits_for_a_key_before_it_exits(self):
        waited: list[int] = []
        with mock.patch.object(launcher, "_wait_for_the_operator",
                               side_effect=lambda: waited.append(1)):
            rc = launcher.cmd_frame_launch(SimpleNamespace(
                profile="claude-work", attended=True, rest=[]))
        self.assertEqual(rc, launcher.REFUSED_EXIT)
        self.assertEqual(waited, [1], "an attended refusal must not vanish with the pane")

    def test_an_unattended_pane_waits_for_nobody(self):
        waited: list[int] = []
        with mock.patch.object(launcher, "_wait_for_the_operator",
                               side_effect=lambda: waited.append(1)):
            launcher.cmd_frame_launch(SimpleNamespace(
                profile="claude-work", attended=False, rest=[]))
        self.assertEqual(waited, [], "nobody is at this pane to press a key")

    def test_an_unattended_refusal_is_written_where_the_launch_can_read_it(self):
        with mock.patch.object(launcher, "_wait_for_the_operator"):
            launcher.cmd_frame_launch(SimpleNamespace(
                profile="claude-work", attended=False, rest=[]))
        code, said = state.launch("beta.1")
        self.assertEqual(code, launcher.REFUSED_EXIT)
        self.assertIn("claude-work", said)

    def test_handing_the_pane_over_is_recorded_too(self):
        """The other half of the same record: without it a launch that waits for a verdict
        would wait out its whole budget on every chat that started perfectly well."""
        self._no_approval_needed()
        launcher.cmd_frame_launch(SimpleNamespace(profile="claude-work", attended=False,
                                                  rest=[]))
        self.assertEqual(state.launch("beta.1"), (0, ""))

    def test_the_attended_pane_names_the_key_it_actually_waits_for(self):
        """Enter, spelled out, because it is the key the line read below really waits for:
        `_wait_for_the_operator` reads a LINE rather than a keystroke (a `tcsetattr` from a
        pane left the launcher killed by a signal on a Linux runner, and the refusal went
        with the window — the one thing ruling 42 exists to prevent). "Press any key" under
        a line read is a pane that looks hung."""
        said: list[str] = []
        with mock.patch.object(launcher, "_wait_for_the_operator"), \
                mock.patch.object(launcher.util, "err", side_effect=said.append):
            launcher.cmd_frame_launch(SimpleNamespace(
                profile="claude-work", attended=True, rest=[]))
        self.assertIn("  press Enter to close this chat.", said)

    def test_a_pane_with_nobody_at_its_keyboard_does_not_stop_to_ask(self):
        """`_wait_for_the_operator` is what an attended refusal ends on, and a pane whose
        stdin is not a terminal has nobody to press anything — so it must not READ, which
        is a stronger claim than "does not block": the suite's own stdin is `/dev/null`,
        where a read returns at once and a missing guard looks exactly like a present one.
        """
        class _APipe:
            def isatty(self) -> bool:
                return False

            def readline(self) -> str:
                raise AssertionError("nobody is here to press anything")

        with mock.patch("sys.stdin", _APipe()):
            launcher._wait_for_the_operator()

    def test_a_pane_with_somebody_at_its_keyboard_waits_for_their_line(self):
        """And the other direction, which is what keeps the guard above honest."""
        class _ATerminal:
            def __init__(self) -> None:
                self.lines = 0

            def isatty(self) -> bool:
                return True

            def readline(self) -> str:
                self.lines += 1
                return "\n"

        stdin = _ATerminal()
        with mock.patch("sys.stdin", stdin):
            launcher._wait_for_the_operator()
        self.assertEqual(stdin.lines, 1)


class TheLauncherIsWhatTmuxStarts(_AProfileAndAPlane, unittest.TestCase):
    def test_only_the_profiles_name_crosses_tmux(self):
        argv = launcher.argv("claude-work", ["-p", "hi"], attended=True)
        self.assertEqual(argv[-7:], ["frame-launch", "--profile", "claude-work",
                                     "--attended", "--", "-p", "hi"])
        self.assertNotIn(os.path.expanduser("~/.cw"), " ".join(argv))
        self.assertNotIn("CLAUDE_CONFIG_DIR", " ".join(argv))

    def test_an_unattended_launch_says_so_by_leaving_the_flag_out(self):
        self.assertNotIn("--attended", launcher.argv("claude-work", [], attended=False))

    def test_it_is_this_interpreters_own_charter(self):
        """`util.self_relaunch_argv`'s `-P`: `python -m charter` prepends the child's cwd to
        `sys.path`, and a chat's cwd is a workspace clone that may hold its own `charter/`
        package (#390)."""
        argv = launcher.argv("claude-work", [], attended=False)
        self.assertEqual(argv[:4], [os.sys.executable, "-P", "-m", "charter"])


if __name__ == "__main__":
    unittest.main()
