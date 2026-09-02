"""Everything that touches the tmux binary, in one module, so the rest is testable.

The messages are asserted because `Deficit` already settled what an absent capability has
to read like: naming the limit and the command that closes it, never a guess, because "a
remedy that does not exist costs more than an honest gap".
"""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from charter.frame import tmuxctl
from tests._tmuxsocket import OPERATOR_SOCKET, OPERATOR_TMUX, socket_path


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


#: A child that writes one byte no UTF-8 decoder can read, between two markers that survive
#: it. Driven as a REAL subprocess and not a `mock.patch("subprocess.run")`, because the
#: raise this class is about happens **inside** `subprocess.run` — in the decode of the pipe
#: it opened — so a stub standing in for it is a stub standing in for the defect. Written
#: with this interpreter rather than `sh -c "printf ..."` so nothing rests on which shell
#: `/bin/sh` is or how its `printf` reads an octal escape.
_PRINTS_A_BYTE_UTF8_CANNOT_READ = [
    sys.executable, "-c", r"import sys; sys.stdout.buffer.write(b'ONE\xffTWO')"]

#: The same, on the other pipe and with a non-zero exit — `report_failure` reads `stderr`,
#: so the two streams are two separate ways for one decode to take a launch down.
_FAILS_SAYING_A_BYTE_UTF8_CANNOT_READ = [
    sys.executable, "-c",
    r"import sys; sys.stderr.buffer.write(b'no such session: \xff'); sys.exit(1)"]


class RunNeverRaisesOnWhatItCannotDecode(unittest.TestCase):
    """#828: `run` caught two ways for a tmux command to end badly and there were three.

    `capture_output=True, text=True` with no `errors=` decodes both pipes **strictly**, so a
    child whose output is not valid UTF-8 raised `UnicodeDecodeError` — a `ValueError`, which
    neither the `TimeoutExpired` clause nor the `OSError` one sees — out of the one function
    whose whole contract is that a misbehaving tmux degrades down the path a refusal does.

    **It does land in a quit — through the window listing, not through the capture.** #828
    files the sharp caller as `commands_frame._capture_transcript` reading a harness pane, and
    tmux sanitises that one: measured on 3.7c, a pane that prints `\\377` is stored in tmux's
    own screen as U+FFFD and `capture-pane` hands back valid UTF-8. What does not get
    sanitised is a user OPTION, which round-trips its bytes untouched out of `list-windows -a
    -F '#{@charter_chat}'` — `_chat_seats`, which `cmd_quit` asks before it kills anything.
    The real-tmux half of that is in
    `tests/test_quit_and_reopen_on_a_real_tmux.ARealQuitStopsRealChats`; the cases here drive
    the decode itself.

    Charter now decodes with :data:`tmuxctl.DECODE_ERRORS`, and the assertions here spell the
    substitute character by hand: reading it off the constant would agree with any value the
    constant took, including `strict`.
    """

    def test_an_answer_charter_cannot_decode_comes_back_rather_than_raising(self):
        with self.assertRaises(UnicodeDecodeError):
            b"ONE\xffTWO".decode("utf-8")   # or this case is not about what it says it is

        proc = tmuxctl.run("listing the chats this plane has open",
                           _PRINTS_A_BYTE_UTF8_CANNOT_READ, timeout=20)

        self.assertEqual(proc.returncode, 0,
                         "the child succeeded; only charter could not read it")
        self.assertEqual(proc.stdout, "ONE\ufffdTWO")

    def test_the_text_around_the_byte_is_kept_rather_than_thrown_away(self):
        """The reason this is not a third invented return code beside `TIMED_OUT`.

        tmux answered — rc 0, the whole listing — so calling it a refusal would blame tmux
        for a command it ran correctly, and would throw away every row charter CAN read over
        one it cannot. What charter cannot read is one codepoint wide; what it can read is
        the rest of the answer.
        """
        proc = tmuxctl.run("listing the chats this plane has open",
                           _PRINTS_A_BYTE_UTF8_CANNOT_READ, timeout=20)

        self.assertTrue(proc.stdout.startswith("ONE"))
        self.assertTrue(proc.stdout.endswith("TWO"))

    def test_the_value_handed_back_is_one_a_caller_can_print(self):
        """`errors="surrogateescape"` would also stop the raise HERE, and move it.

        A lone surrogate has no UTF-8 encoding at all, so it raises `UnicodeEncodeError` on
        any strict encode downstream — `sys.stdout` under a normal `LANG=en_US.UTF-8` is
        exactly that (measured: `sys.stdout.errors == "strict"`, and writing one raises). A
        function documented as never raising has to hand back a value that does not raise
        either, or the promise is only about its own frame.
        """
        proc = tmuxctl.run("reading what the harness printed before it died",
                           _PRINTS_A_BYTE_UTF8_CANNOT_READ, timeout=20)

        proc.stdout.encode("utf-8")        # raises if a surrogate got through

    def test_a_failure_charter_cannot_decode_is_still_reported(self):
        """The failure REPORT is why this wrapper exists, and it reads `stderr`."""
        said = []
        with mock.patch("charter.util.err", side_effect=said.append):
            proc = tmuxctl.run("ending the frame after an early death",
                               _FAILS_SAYING_A_BYTE_UTF8_CANNOT_READ, timeout=20)

        self.assertEqual(proc.returncode, 1)
        self.assertEqual(proc.stderr, "no such session: \ufffd")
        self.assertTrue(any("ending the frame" in m and "no such session" in m for m in said),
                        said)

    def test_a_version_charter_cannot_decode_is_unreadable_rather_than_a_crash(self):
        """`_probe` is the module's other captured child, and it caught the same two things.

        `version()` gates the whole launch and is asked before anything is drawn, so a raise
        here is a traceback in place of a frame. It already has an answer for a `tmux -V` it
        cannot parse — `None`, which reads as "charter could not find out" — and a `tmux` on
        `$PATH` that is a wrapper script answering in some other encoding is the same fact.
        """
        d = Path(self.enterContext(tempfile.TemporaryDirectory()))
        shim = d / "tmux"
        shim.write_text(f"#!{sys.executable}\n"
                        r"import sys; sys.stdout.buffer.write(b'tmux 3.\xffc')" + "\n")
        shim.chmod(0o755)
        self.enterContext(mock.patch.dict(os.environ,
                                          {"PATH": f"{d}{os.pathsep}{os.environ['PATH']}"}))

        self.assertIsNone(tmuxctl.version())


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
        env = {"TMUX": OPERATOR_TMUX}
        self.assertEqual(tmuxctl.operator_server(env),
                         (OPERATOR_SOCKET, "$1"))

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
            tmuxctl.server_argv(OPERATOR_SOCKET, "list-windows", "-a"),
            ["tmux", "-S", OPERATOR_SOCKET, "list-windows", "-a"])

    def test_nothing_is_ever_joined(self):
        """The argv rule, at the one place every tmux command in charter now passes
        through: a joined string is shell-interpreted by tmux and a separate argv is
        not (pinned against 3.7c)."""
        argv = tmuxctl.server_argv("charter", "new-window", "--", "claude", "-p", "a;b")
        self.assertEqual(argv[-1], "a;b")
        self.assertTrue(all(isinstance(a, str) for a in argv))

    def test_charters_own_socket_spelled_as_a_path_still_gets_dash_s(self):
        """**The spelling test is unchanged by #812 and this is the case that proves
        it.** `-S` is the only flag that can reach a socket named by path — a `-L` may
        not contain a separator at all — so a value that happens to be charter's own
        server written the long way must still be aimed with `-S`, however the
        OWNERSHIP question (`is_operator_socket`) now answers for it."""
        own = socket_path("charter")
        self.assertFalse(tmuxctl.is_operator_socket(own, own="charter"))
        self.assertEqual(tmuxctl.server_argv(own, "list-sessions"),
                         ["tmux", "-S", own, "list-sessions"])


class TwoSpellingsOfOneSocket(unittest.TestCase):
    """#812: `<name>` and `<tmpdir>/tmux-<uid>/<name>` are ONE server, and the guest test
    could not tell them apart.

    The operator's own ``$TMUX`` reads ``/private/tmp/tmux-<uid>/charter,18923,83`` — the
    socket charter started itself, spelled absolute because tmux writes it that way into
    every pane it opens. A chat launched from inside one of those panes recorded that
    spelling, and `is_operator_socket` — a leading-slash test — answered "somebody else's
    tmux" for charter's own private server. Every workspace tab in that chat, the one
    back included, was then refused by name.

    **The socket paths here come from `tests/_tmuxsocket.py`, which computes tmux's rule
    independently of the module under test** (#601: never a spelled uid, and never
    the production function asked to confirm itself). If the two implementations of "where
    tmux puts a socket" ever disagree, that is this class going red rather than a round
    trip agreeing with itself.
    """

    def test_a_name_and_its_socket_file_are_one_server(self):
        self.assertTrue(tmuxctl.same_server("charter", socket_path("charter")))

    def test_two_names_are_two_servers(self):
        self.assertFalse(tmuxctl.same_server("charter", "default"))

    def test_two_socket_files_are_two_servers(self):
        self.assertFalse(tmuxctl.same_server(socket_path("charter"),
                                             socket_path("default")))

    def test_the_symlinked_spelling_of_one_file_is_one_server(self):
        """``/tmp`` is a symlink to ``/private/tmp`` on macOS, and BOTH spellings reach
        the same running server — `test_frame_tmux_integration.OP_SOCKET_PATH` builds the
        ``/tmp`` form and talks to a server tmux reports at the ``/private/tmp`` one. A
        comparison that read those as two servers would answer #812 on Linux and not on
        the platform the report came from."""
        unresolved = os.path.join("/tmp", f"tmux-{os.getuid()}", "charter")
        self.assertTrue(tmuxctl.same_server(unresolved, "charter"),
                        f"{unresolved} and `charter` are the same socket file")

    def test_a_socket_file_asked_for_by_its_own_path_is_that_path(self):
        """**Why `_resolved` has no branch in it, pinned where a change would be caught.**

        `os.path.join` throws every earlier component away when the tail is absolute, so
        `socket_path` handed a socket PATH answers with that path and handed a `-L` NAME
        builds the file for it — one expression covering both spellings. The deletion
        sweep found the `if` that used to stand in `_resolved` to be exactly equivalent to
        its else, which is this property being true; the line went, and the property is
        asserted here instead of being relied on silently.

        Both spellings of the same file, because the second is the one that carries a
        symlink: `socket_path` resolves the DIRECTORY it builds and does not touch a path
        handed to it, and `_resolved` is what resolves the whole thing afterwards."""
        for spelled in (f"/tmp/tmux-{os.getuid()}/charter",
                        os.path.realpath(f"/tmp/tmux-{os.getuid()}") + "/charter"):
            with self.subTest(spelled=spelled):
                self.assertEqual(tmuxctl.socket_path(spelled), spelled)

    def test_an_unknown_side_is_never_the_same_server_as_anything(self):
        """``""`` is `state.frame_server` reading a marker that is not there — the absence
        of an answer, not a spelling of one. Two absences are not a match either, which is
        the case a bare equality would have got wrong."""
        self.assertFalse(tmuxctl.same_server("", "charter"))
        self.assertFalse(tmuxctl.same_server("charter", ""))
        self.assertFalse(tmuxctl.same_server("", ""))
        self.assertFalse(tmuxctl.same_server(None, None))

    def test_the_socket_directory_is_tmuxs_own_rule_and_not_dollar_tmpdir(self):
        """``$TMUX_TMPDIR`` or tmux's ``_PATH_TMP`` literal — **never** ``$TMPDIR``.

        Measured on tmux 3.7c and at the 3.2 floor, on the machine this was written on: a
        server started with ``TMPDIR`` pointed at a scratch directory reported its own
        ``#{socket_path}`` as ``/private/tmp/tmux-<uid>/<name>`` and left that directory
        empty, while the same run with ``TMUX_TMPDIR`` set built ``<it>/tmux-<uid>/``. On
        macOS `tempfile.gettempdir()` answers a per-user ``/var/folders/…`` that tmux
        never puts a socket in, so reading the wrong variable would put every socket path
        charter computes somewhere no server has ever listened."""
        with mock.patch.dict(os.environ, {"TMPDIR": "/nowhere-tmpdir"}, clear=False):
            os.environ.pop("TMUX_TMPDIR", None)
            self.assertEqual(tmuxctl.socket_path("charter"),
                             os.path.realpath(f"/tmp/tmux-{os.getuid()}") + "/charter")
        with mock.patch.dict(os.environ, {"TMUX_TMPDIR": "/nowhere-tmux"}, clear=False):
            self.assertEqual(tmuxctl.socket_path("charter"),
                             f"/nowhere-tmux/tmux-{os.getuid()}/charter")


class WhoseServerIsIt(unittest.TestCase):
    """`is_operator_socket` — the question `frame/slots.py` and
    `commands_frame._switch_workspace` ask, and the one #812 was answered wrongly.

    The refusal it gates is not the defect and is asserted still to fire below: inside a
    genuine operator tmux a workspace really is not a session, and `switch-client` has
    nothing correct to do there.
    """

    def test_charters_own_socket_by_name_is_not_a_guests(self):
        self.assertFalse(tmuxctl.is_operator_socket("charter", own="charter"))

    def test_charters_own_socket_by_absolute_path_is_not_a_guests_either(self):
        """**#812 itself, in one line.** This answered True, and every workspace tab in a
        chat that recorded this spelling refused."""
        self.assertFalse(tmuxctl.is_operator_socket(socket_path("charter"),
                                                    own="charter"))

    def test_a_socket_charter_did_not_start_is_still_a_guests(self):
        """The other half, and the reason this is a comparison rather than a deletion:
        ``default`` is the socket an operator's own `tmux` starts, in the same directory
        as charter's, and a frame there is a WINDOW in their session."""
        self.assertTrue(tmuxctl.is_operator_socket(OPERATOR_SOCKET, own="charter"))
        self.assertNotEqual(OPERATOR_SOCKET, socket_path("charter"))

    def test_another_planes_private_socket_is_a_guests_too(self):
        """Not merely "is it in tmux's socket directory": a second charter-shaped socket
        that is not the one THIS charter starts is somebody else's server."""
        self.assertTrue(tmuxctl.is_operator_socket(socket_path("charter-elsewhere"),
                                                   own="charter"))

    def test_nothing_recorded_is_not_a_guests_server(self):
        """`state.frame_server` answers ``None`` for a frame launched by a charter that
        predates the marker, and the fallback beside every call is charter's own socket."""
        self.assertFalse(tmuxctl.is_operator_socket(None, own="charter"))
        self.assertFalse(tmuxctl.is_operator_socket("", own="charter"))

    def test_the_socket_it_compares_against_is_the_one_charter_launches_on(self):
        """*own* defaults to `commands_frame.SOCKET`, read at CALL time.

        Bound at import instead, this would answer for the socket the suite was loaded
        with rather than the throwaway one a real-tmux test patches in — so every such
        test would be measuring ownership of a server it never touched, which is the
        shape of mistake #812 is."""
        from charter import commands_frame
        self.assertEqual(commands_frame.SOCKET, "charter")
        self.assertFalse(tmuxctl.is_operator_socket(socket_path("charter")))
        with mock.patch.object(commands_frame, "SOCKET", "charter-somewhere-else"):
            self.assertTrue(tmuxctl.is_operator_socket(socket_path("charter")))
            self.assertFalse(
                tmuxctl.is_operator_socket(socket_path("charter-somewhere-else")))




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
            tmuxctl.server_argv(OPERATOR_SOCKET, "kill-pane", "-t", "%2")]))

    def test_nothing_to_chain_is_nothing_to_run(self):
        """`overlay.close_argvs` answers `[]` when it will not build a close at all, and
        that must not become a bare `tmux` invocation with no command in it."""
        self.assertIsNone(tmuxctl.chain([]))


if __name__ == "__main__":
    unittest.main()
