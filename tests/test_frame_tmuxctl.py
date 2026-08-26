"""Everything that touches the tmux binary, in one module, so the rest is testable.

The messages are asserted because `Deficit` already settled what an absent capability has
to read like: naming the limit and the command that closes it, never a guess, because "a
remedy that does not exist costs more than an honest gap".
"""

from __future__ import annotations

import os
import subprocess
import unittest
from unittest import mock

from charter.frame import tmuxctl


class Version(unittest.TestCase):
    def test_a_release_version_parses(self):
        with mock.patch.object(tmuxctl, "_probe", return_value="tmux 3.7c"):
            self.assertEqual(tmuxctl.version(), (3, 7))

    def test_a_two_part_version_parses(self):
        with mock.patch.object(tmuxctl, "_probe", return_value="tmux 3.2"):
            self.assertEqual(tmuxctl.version(), (3, 2))

    def test_an_absent_binary_is_none_not_zero(self):
        """None is 'charter has nothing to say', which reads differently from 'version
        0.0' — the distinction `registry.deficits` makes for an unknown harness."""
        with mock.patch.object(tmuxctl, "_probe", return_value=None):
            self.assertIsNone(tmuxctl.version())

    def test_unparseable_output_is_none_rather_than_a_crash(self):
        with mock.patch.object(tmuxctl, "_probe", return_value="tmux next-3.9"):
            self.assertIsNone(tmuxctl.version())


class Floor(unittest.TestCase):
    def test_the_floor_is_the_version_the_frame_is_checked_against(self):
        """The NUMBER is unchanged; its justification was wrong. `FLOOR` used to be
        documented as "the version `display-popup` needs" — `display-popup` appears
        nowhere in shipped code, in any form, only in two comments. What actually sits
        near 3.2 is the pane-scoped hook (`set-hook -p`) the exit-code mechanism rests
        on, and `run-shell -C` — the escape hatch's whole mechanism — which is 3.2
        exactly. (`display-menu`, the reason this comment used to give, is 3.0 and no
        longer exists in charter at all.) Kept at 3.2 deliberately
        rather than lowered on an unverified reading of tmux's CHANGES: no tmux older
        than 3.7c exists on this machine to test against, the cost of the floor being
        too HIGH is one accurate warning, and the cost of it being too LOW is a frame
        that starts and then declines to attach."""
        self.assertEqual(tmuxctl.FLOOR, (3, 2))

    def test_the_resize_hook_floor_stays_above_the_frames_own(self):
        """Two floors, two meanings — folding them into one would refuse (or warn
        about) the whole frame over a gap that only costs cosmetic resize drift."""
        self.assertGreater(tmuxctl.RESIZE_HOOK_FLOOR, tmuxctl.FLOOR)


class Messages(unittest.TestCase):
    def test_the_absent_message_names_the_command_that_fixes_it(self):
        msg = tmuxctl.absent_message()
        self.assertIn("tmux", msg)
        self.assertIn("--no-frame", msg)

    def test_the_below_floor_message_names_both_versions(self):
        msg = tmuxctl.below_floor_message((3, 0))
        self.assertIn("3.0", msg)
        self.assertIn("3.2", msg)

    def test_the_below_floor_message_does_not_claim_the_hotkey_is_disabled(self):
        """It said "the frame starts with the hotkey disabled". Nothing disables it:
        `cmd_launch` warns and continues, and `conf_text` still emits the bind. A
        message describing a mechanism that does not exist is worse than none, because
        it is the one place an operator looks to find out what they are losing."""
        msg = tmuxctl.below_floor_message((3, 0))
        self.assertNotIn("hotkey disabled", msg)
        self.assertIn("stays bound", msg)
        self.assertIn("exit code", msg)


class RunArgv(unittest.TestCase):
    def test_run_rejects_a_string_with_typeerror(self):
        """The argv guard survives `python -O`, so it raises TypeError, not AssertionError."""
        with self.assertRaises(TypeError):
            tmuxctl.run("starting the frame", "tmux new-session")

    def test_interact_rejects_a_string_too(self):
        """The same rule on the other door out of this module — a guard on one of two
        entry points is not a guard."""
        with self.assertRaises(TypeError):
            tmuxctl.interact("tmux attach -t x")


class RunGuardsTheTimeout(unittest.TestCase):
    """The defect this wrapper exists for: `cmd_launch` issued eleven
    `subprocess.run(..., timeout=15)` calls with nothing catching
    `subprocess.TimeoutExpired`, and TEN of them run after `new-session` has already
    started the harness detached. A wedged tmux server therefore gave the operator a
    traceback, a filed charter crash report, an orphaned agent session and no reattach
    line — for a condition that is not a charter bug at all."""

    def test_a_timeout_becomes_a_return_code_rather_than_an_exception(self):
        with mock.patch("subprocess.run",
                        side_effect=subprocess.TimeoutExpired(["tmux"], 15)), \
             mock.patch("charter.util.err") as err:
            proc = tmuxctl.run("installing the exit-status hook", ["tmux", "-V"])
        self.assertEqual(proc.returncode, tmuxctl.TIMED_OUT)
        self.assertNotEqual(proc.returncode, 0, "a timeout must never read as success")
        err.assert_called_once()

    def test_a_tmux_that_cannot_be_started_becomes_a_return_code_too(self):
        with mock.patch("subprocess.run", side_effect=OSError("no such binary")), \
             mock.patch("charter.util.err") as err:
            proc = tmuxctl.run("starting the frame", ["tmux", "-V"])
        self.assertEqual(proc.returncode, tmuxctl.COULD_NOT_RUN)
        err.assert_called_once()

    def test_a_failure_names_the_action_and_tmuxs_own_stderr(self):
        with mock.patch("subprocess.run", return_value=subprocess.CompletedProcess(
                ["tmux"], 1, stdout="", stderr="no such session: nope")), \
             mock.patch("charter.util.err", side_effect=lambda m: buf.append(m)):
            buf = []
            tmuxctl.run("ending the frame after an early death", ["tmux", "-V"])
        self.assertTrue(any("ending the frame" in m and "no such session" in m
                            for m in buf), buf)

    def test_report_false_stays_silent_but_still_reports_the_code(self):
        """`_live_sessions` asks against a socket no server has ever run on, where a
        non-zero return is the ORDINARY answer — reporting it would print an error on
        every first launch on a machine."""
        with mock.patch("subprocess.run", return_value=subprocess.CompletedProcess(
                ["tmux"], 1, stdout="", stderr="no server running")), \
             mock.patch("charter.util.err") as err:
            proc = tmuxctl.run("listing the frames already running", ["tmux", "-V"],
                               report=False)
        self.assertEqual(proc.returncode, 1)
        err.assert_not_called()

    def test_a_success_reports_nothing(self):
        with mock.patch("subprocess.run", return_value=subprocess.CompletedProcess(
                ["tmux"], 0, stdout="%7\n", stderr="")), \
             mock.patch("charter.util.err") as err:
            proc = tmuxctl.run("starting the frame", ["tmux", "-V"])
        self.assertEqual(proc.stdout.strip(), "%7")
        err.assert_not_called()

    def test_interact_neither_captures_nor_time_boxes(self):
        """`attach` IS the operator's terminal for as long as the harness runs, and
        `display-menu` waits for a keypress — capturing either would swallow the
        operator's own screen, and a timeout on either would end a frame for the crime
        of being used."""
        with mock.patch("subprocess.run",
                        return_value=subprocess.CompletedProcess(["tmux"], 0)) as run:
            tmuxctl.interact(["tmux", "attach"], env={"A": "b"})
        _, kwargs = run.call_args
        self.assertNotIn("timeout", kwargs)
        self.assertNotIn("capture_output", kwargs)
        self.assertEqual(kwargs["env"], {"A": "b"})


class OperatorServer(unittest.TestCase):
    """Reading `$TMUX` — the one fact that decides whether charter nests or not.

    Measured against tmux 3.7c by printing `$TMUX` inside a real pane: the value is
    `<socket path>,<server pid>,<session id>`, where the session id is the NUMBER off
    tmux's own `#{session_id}` (`$1` -> `1`). Both halves matter to the launcher — the
    socket says WHICH server the operator is already talking to, and the session id says
    which session of theirs the new window belongs in — so both are parsed here rather
    than one being re-queried from tmux later.
    """

    def test_a_real_tmux_environment_yields_the_socket_and_the_session(self):
        env = {"TMUX": "/private/tmp/tmux-502/default,70029,1"}
        self.assertEqual(tmuxctl.operator_server(env),
                         ("/private/tmp/tmux-502/default", "$1"))

    def test_no_tmux_variable_means_charter_is_not_inside_one(self):
        self.assertIsNone(tmuxctl.operator_server({}))

    def test_an_empty_tmux_variable_is_not_inside_one_either(self):
        """An exported-but-empty `$TMUX` is what a shell that once ran `unset`-less
        cleanup leaves behind; treating it as "inside tmux" would send the launcher at a
        socket path of `""`."""
        self.assertIsNone(tmuxctl.operator_server({"TMUX": ""}))

    def test_a_value_charter_cannot_read_is_refused_rather_than_guessed(self):
        """Two commas and a numeric third field is the whole contract. Anything else —
        a truncated value, a non-numeric session id — is not shaped into a target that
        would then be interpolated into `new-window -t`; charter falls back to its own
        private server, which nests but is never wrong about what it is talking to."""
        for bad in ("/tmp/sock", "/tmp/sock,70029", "/tmp/sock,70029,abc",
                    ",70029,1", "/tmp/sock,70029,1,extra"):
            with self.subTest(bad=bad):
                self.assertIsNone(tmuxctl.operator_server({"TMUX": bad}))

    def test_a_relative_socket_path_is_refused(self):
        """`server_argv` picks `-S` over `-L` on a leading slash alone, so a socket
        path that is not absolute would silently be sent to tmux as a `-L` SERVER NAME
        and start a brand-new server — the nesting this whole path exists to stop,
        reached by a different route."""
        self.assertIsNone(tmuxctl.operator_server({"TMUX": "sock,70029,1"}))

    def test_the_default_source_is_the_real_environment(self):
        with mock.patch.dict(os.environ, {"TMUX": "/tmp/s,1,2"}):
            self.assertEqual(tmuxctl.operator_server(), ("/tmp/s", "$2"))
        with mock.patch.dict(os.environ, {}, clear=True):
            self.assertIsNone(tmuxctl.operator_server())


class ServerArgv(unittest.TestCase):
    """`tmux -L <name>` and `tmux -S <path>` are the same command against two different
    servers, and every argv charter builds has to pick one.

    A leading `/` is the whole discriminator, and it is total rather than heuristic: a
    socket path only ever reaches charter from `$TMUX`, which tmux itself writes
    absolute (measured), and a `-L` name may not contain a separator at all — tmux joins
    it onto its own socket directory to make the path.
    """

    def test_a_plain_name_selects_charters_own_private_server(self):
        self.assertEqual(tmuxctl.server_argv("charter", "list-sessions"),
                         ["tmux", "-L", "charter", "list-sessions"])

    def test_an_absolute_path_selects_the_operators_existing_server(self):
        self.assertEqual(
            tmuxctl.server_argv("/private/tmp/tmux-502/default", "list-windows", "-a"),
            ["tmux", "-S", "/private/tmp/tmux-502/default", "list-windows", "-a"])

    def test_nothing_is_ever_joined(self):
        """The argv rule, at the one place every tmux command in charter now passes
        through: a joined string is shell-interpreted by tmux and a separate argv is
        not (pinned against 3.7c)."""
        argv = tmuxctl.server_argv("charter", "new-window", "--", "claude", "-p", "a;b")
        self.assertEqual(argv[-1], "a;b")
        self.assertTrue(all(isinstance(a, str) for a in argv))




class InertFormat(unittest.TestCase):
    """`tmuxctl.inert_format` — carried whole from the deleted `tests/test_frame_menu.py`
    when the palette replaced `display-menu`.

    **The function moved; the hole it closes did not.** A `display-menu` item NAME was a
    tmux format, and so is a `display-message` argument — `display-message`'s own docs say
    so — which is where charter still puts a workspace name, a persona name and an action's
    refusal (`commands_frame._say_on_screen`). Deleting these along with the menu would
    have deleted the only tests behind the guard that closed a CRITICAL finding, leaving
    correct code with nothing pinning it: exactly the shape the deletion sweep exists to
    catch.
    """

    def test_a_shell_job_is_escaped_hash_by_hash(self):
        """`#(cmd)` runs `cmd` the instant tmux draws it (confirmed by hand through the
        real production path). `##` is tmux's own escape for a literal `#`; doubling
        every occurrence is what closes it — checked here as an exact string match, not
        merely "the substring #( is gone", since a partial escape (only the first #, say)
        would still leave a working `#(...)` job one character later."""
        self.assertEqual(tmuxctl.inert_format("#(touch /tmp/pwned)"),
                         "##(touch /tmp/pwned)")

    def test_a_format_variable_is_escaped_too(self):
        """`#{session_name}` substitutes a value rather than running a job, but it is the
        SAME construction (an unescaped `#`) and the SAME fix closes it — no separate
        mechanism needed for the two forms tmux's FORMATS section documents."""
        self.assertEqual(tmuxctl.inert_format("#{session_name}"), "##{session_name}")

    def test_every_hash_is_doubled_not_only_the_first(self):
        raw = "##(a)##(b)#"
        self.assertEqual(tmuxctl.inert_format(raw).count("#"), 2 * raw.count("#"))

    def test_a_leading_hyphen_never_reaches_tmuxs_flag_position(self):
        """Measured against a real attached client: tmux reads a value beginning with `-`
        as an unrecognised FLAG of its own and refuses the whole command, rc 1 — worse
        than its own docs suggest. A leading space keeps the text and closes it."""
        rendered = tmuxctl.inert_format("-my-branch")
        self.assertFalse(rendered.startswith("-"))
        self.assertIn("my-branch", rendered)

    def test_a_trailing_hash_gets_a_trailing_space(self):
        """Cosmetic, not a safety hole (the escape above already makes it inert either
        way) — but a value doubled from a single trailing `#` collides with the
        style-reset sequence tmux appends after it, rendering literal `x#[default]`
        garbage (confirmed by hand). A trailing space breaks the adjacency."""
        self.assertEqual(tmuxctl.inert_format("trailing#"), "trailing## ")

    def test_text_with_none_of_the_special_shapes_is_unchanged(self):
        self.assertEqual(tmuxctl.inert_format("ordinary text"), "ordinary text")


class ChainedCommands(unittest.TestCase):
    """`tmuxctl.chain` — several commands in ONE invocation, because the palette's own
    close list kills the process sending it.

    Measured against tmux 3.7c from inside the pane being killed: sent one at a time, the
    first command returned 0 and the process was gone before the second answered, so the
    third never ran. Sent as one command line, all three ran, 3 times out of 3.
    """

    def test_the_commands_are_separated_by_tmuxs_own_separator(self):
        argv = tmuxctl.chain([tmuxctl.server_argv("charter", "select-pane", "-t", "%1"),
                              tmuxctl.server_argv("charter", "kill-pane", "-t", "%2")])
        self.assertEqual(argv, ["tmux", "-L", "charter", "select-pane", "-t", "%1",
                                ";", "kill-pane", "-t", "%2"])

    def test_the_server_is_named_once_and_only_once(self):
        argv = tmuxctl.chain([tmuxctl.server_argv("charter", "a"),
                              tmuxctl.server_argv("charter", "b"),
                              tmuxctl.server_argv("charter", "c")])
        self.assertEqual(argv.count("-L"), 1)
        self.assertEqual(argv.count("charter"), 1)

    def test_a_semicolon_inside_one_argument_is_not_a_separator(self):
        """`@charter_hatch`'s own value IS `select-pane -t %1 ; kill-pane -t %2`, and it
        travels as ONE argv element. Measured against tmux 3.7c: an argument merely
        containing a `;` is passed through whole while a standalone `;` separates — so
        the value must come back out of `chain` unsplit."""
        value = "select-pane -t %1 ; kill-pane -t %2"
        argv = tmuxctl.chain([tmuxctl.server_argv("charter", "set-option", "-w", "-t",
                                                  "%1", "@charter_hatch", value),
                              tmuxctl.server_argv("charter", "kill-pane", "-t", "%2")])
        self.assertIn(value, argv)
        self.assertEqual(argv.count(";"), 1, argv)

    def test_two_servers_are_refused_rather_than_sent_to_one_of_them(self):
        """The head is what selects WHICH tmux this reaches. A chain built from charter's
        private socket and an operator's own would send one server's commands to the
        other — so it is refused, not guessed at."""
        self.assertIsNone(tmuxctl.chain([
            tmuxctl.server_argv("charter", "kill-pane", "-t", "%1"),
            tmuxctl.server_argv("/private/tmp/tmux-502/default", "kill-pane", "-t", "%2")]))

    def test_nothing_to_chain_is_nothing_to_run(self):
        """`overlay.close_argvs` answers `[]` when it will not build a close at all, and
        that must not become a bare `tmux` invocation with no command in it."""
        self.assertIsNone(tmuxctl.chain([]))


if __name__ == "__main__":
    unittest.main()
