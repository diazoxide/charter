"""Starting a harness inside a frame, and the three ways that must not go wrong.

The exit code is asserted because it was measured wrong once: an attached
`tmux new-session` returns 0 whatever its command exited with (pinned against 3.7c), so
the launcher waits and reads a recorded status instead of exec'ing and hoping.

**Every fake tmux command here returns a `subprocess.CompletedProcess`, never a real
tmux process** — the point of `charter/frame/tmuxctl.py` splitting the binary out of
everything else is that this module gets to keep that promise while still exercising the
FULL launch sequence: session creation, reading the captured pane id back off stdout,
loading the pane-scoped config, drawing the panels, and attaching.
"""

from __future__ import annotations

import subprocess
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from tests._isolation import PersonaIso
from charter import commands_frame
from charter.frame import state


class Bypass(unittest.TestCase):
    def test_a_pipe_gets_no_frame(self):
        """`charter claude -p "…" | jq` must be `claude -p "…" | jq`. A frame around a
        pipe is wrong, and exec keeps the exit code without any help."""
        with mock.patch("sys.stdout.isatty", return_value=False), \
             mock.patch("os.execvp") as ex:
            commands_frame.bypass(["claude", "-p", "hi"])
        ex.assert_called_once_with("claude", ["claude", "-p", "hi"])


class Conf(unittest.TestCase):
    def test_the_pane_died_hook_is_scoped_to_the_harness_pane(self):
        """pane-died fires for ANY pane. Unscoped, a dead panel would be reported as the
        agent's exit code."""
        text = commands_frame.conf_text(hotkey="F2", mouse=False, history_limit=50000,
                                        status_path="/tmp/f/exit", harness_pane="%3")
        self.assertIn("%3", text)
        self.assertIn("pane_dead_status", text)

    def test_remain_on_exit_is_set_or_the_status_never_exists(self):
        text = commands_frame.conf_text(hotkey="F2", mouse=False, history_limit=50000,
                                        status_path="/tmp/f/exit", harness_pane="%3")
        self.assertIn("remain-on-exit on", text)

    def test_mouse_is_off_unless_asked_for(self):
        off = commands_frame.conf_text(hotkey="F2", mouse=False, history_limit=1,
                                       status_path="/x", harness_pane="%0")
        self.assertIn("set -g mouse off", off)
        on = commands_frame.conf_text(hotkey="F2", mouse=True, history_limit=1,
                                      status_path="/x", harness_pane="%0")
        self.assertIn("set -g mouse on", on)

    def test_history_limit_is_raised_above_the_tmux_default(self):
        """tmux ships 2000 lines, which becomes the harness's entire scrollback."""
        text = commands_frame.conf_text(hotkey="F2", mouse=False, history_limit=50000,
                                        status_path="/x", harness_pane="%0")
        self.assertIn("history-limit 50000", text)


class MissingTmux(unittest.TestCase):
    def test_an_absent_tmux_names_the_remedy_and_does_not_start_a_frame(self):
        """Adapted from the task brief's own draft, which mocked `tmuxctl.available()`.
        Correction 5 requires `cmd_launch` to call `tmuxctl.version()` exactly ONCE and
        branch on it — `available()` is never called at all in the corrected launcher —
        so a test that only patches `available()` would exercise the REAL `tmux -V` here
        (whatever happens to be installed on the machine running the suite) instead of
        the absent-tmux path it claims to test. Patching `version()` to return `None` (=
        "absent", per `tmuxctl.version`'s own docstring) is what actually simulates it."""
        args = SimpleNamespace(harness="claude", rest=[], no_frame=False)
        with mock.patch("charter.frame.tmuxctl.version", return_value=None), \
             mock.patch("sys.stdout.isatty", return_value=True), \
             mock.patch("charter.commands_frame.subprocess.run") as run, \
             mock.patch("builtins.print") as p:
            rc = commands_frame.cmd_launch(args)
        self.assertNotEqual(rc, 0)
        printed = " ".join(str(c) for c in p.call_args_list)
        self.assertIn("--no-frame", printed)
        # "does not start a frame": no tmux command of any kind was even attempted.
        run.assert_not_called()


class _FakeTmux:
    """Every tmux invocation `cmd_launch` makes, keyed by which subcommand it carries.

    Standing in for the real binary (never started in this suite — see the module
    docstring) while still letting a test drive the FULL sequence: the launcher reads a
    pane id off `new-session`'s stdout, and this fake hands one back the same way real
    tmux does, so a test can assert what the launcher does with THAT VALUE rather than
    with a hardcoded stand-in like `"%0"` (the exact bug correction 3 exists to catch).

    `exit_code`, if given, is written via `state.record_exit` at the moment the fake
    `attach` command runs — mimicking the real timing: `attach` blocks until the
    session ends, and by the time it returns, the `pane-died` hook (fired on the real
    server while attach was blocked) has already recorded the code.
    """

    def __init__(self, *, pane_id="%7", exit_code=None,
                session_rc=0, source_rc=0, panel_rc=0, attach_rc=0):
        self.pane_id = pane_id
        self.exit_code = exit_code
        self.session_rc = session_rc
        self.source_rc = source_rc
        self.panel_rc = panel_rc
        self.attach_rc = attach_rc
        self.calls: list[list[str]] = []
        self.fid = None
        self.sourced_conf_text = None

    def __call__(self, cmd, **kwargs):
        self.calls.append(list(cmd))
        if "new-session" in cmd:
            self.fid = cmd[cmd.index("-s") + 1]
            return subprocess.CompletedProcess(cmd, self.session_rc,
                                               stdout=(self.pane_id + "\n") if self.session_rc == 0 else "",
                                               stderr="" if self.session_rc == 0 else "no space for a new pane")
        if "source-file" in cmd:
            conf_path = cmd[cmd.index("source-file") + 1]
            self.sourced_conf_text = Path(conf_path).read_text()
            return subprocess.CompletedProcess(cmd, self.source_rc, stdout="",
                                               stderr="" if self.source_rc == 0 else "bad config line")
        if "split-window" in cmd:
            return subprocess.CompletedProcess(cmd, self.panel_rc, stdout="",
                                               stderr="" if self.panel_rc == 0 else "no space for a new pane")
        if "attach" in cmd:
            if self.exit_code is not None and self.fid:
                state.record_exit(self.fid, self.exit_code)
            return subprocess.CompletedProcess(cmd, self.attach_rc, stdout="", stderr="")
        if "list-sessions" in cmd:
            return subprocess.CompletedProcess(cmd, 0, stdout="", stderr="")
        raise AssertionError(f"unexpected tmux command in test: {cmd}")


def _launch(fake: _FakeTmux, *, cols=200, rows=50, version=(3, 7), harness="claude"):
    args = SimpleNamespace(harness=harness, rest=[], no_frame=False)
    with mock.patch("charter.commands_frame.subprocess.run", side_effect=fake), \
         mock.patch("sys.stdout.isatty", return_value=True), \
         mock.patch("charter.frame.tmuxctl.version", return_value=version), \
         mock.patch("charter.workspace.resolve", return_value="demo"), \
         mock.patch("os.get_terminal_size", return_value=os_terminal_size(cols, rows)):
        return commands_frame.cmd_launch(args)


def os_terminal_size(cols, rows):
    import os as _os
    return _os.terminal_size((cols, rows))


class Launch(PersonaIso, unittest.TestCase):
    def test_the_pane_scoped_hook_uses_the_pane_tmux_actually_reported(self):
        """The regression correction 3 names directly: `conf_text`'s `harness_pane` must
        be the id READ OFF tmux's stdout, never the literal `"%0"` that happens to be
        right only for the very first pane ever created on a fresh server. `_FakeTmux`
        reports `%7` — a value `"%0"` could never produce by accident — so this fails if
        the launcher ever falls back to a hardcoded pane id."""
        fake = _FakeTmux(pane_id="%7", exit_code=0)
        rc = _launch(fake)
        self.assertEqual(rc, 0)
        self.assertIsNotNone(fake.sourced_conf_text)
        self.assertIn("%7", fake.sourced_conf_text)
        self.assertNotIn("%0", fake.sourced_conf_text)

    def test_the_config_is_loaded_with_source_file_not_only_dash_f(self):
        """Measured against tmux 3.7c (module docstring of commands_frame.py): `-f` on
        `new-session` is read only when that call starts a brand-new server, so a SECOND
        frame sharing `SOCKET` would never get its own pane-scoped hook installed if the
        launcher relied on `-f` alone. `source-file`, run after the pane id is known,
        applies live regardless of whether the server was already running."""
        fake = _FakeTmux(exit_code=0)
        _launch(fake)
        self.assertTrue(any("source-file" in c for c in fake.calls),
                        "the real config was never loaded via `source-file`")

    def test_the_recorded_exit_code_wins_over_a_zero_from_attach(self):
        """The whole module's reason to exist: an attached `tmux attach` (like
        `new-session`) returns 0 for the CLIENT's own detach, not for what ran inside —
        so a real crash must still surface even though `attach`'s own returncode is 0."""
        fake = _FakeTmux(exit_code=17, attach_rc=0)
        rc = _launch(fake)
        self.assertEqual(rc, 17)

    def test_a_failed_session_start_is_fatal_and_reported(self):
        """No harness pane exists — nothing to attach to, so this must not proceed to
        try drawing panels or attaching, and correction 2 requires the failure to name
        the command and tmux's stderr rather than vanish."""
        fake = _FakeTmux(session_rc=1)
        buf = []
        with mock.patch("charter.util.err", side_effect=lambda m: buf.append(m)):
            rc = _launch(fake)
        self.assertNotEqual(rc, 0)
        self.assertTrue(any("no space for a new pane" in m for m in buf),
                        f"tmux's own stderr was never surfaced: {buf}")
        self.assertFalse(any("split-window" in c for c in fake.calls))
        self.assertFalse(any("attach" in c for c in fake.calls))

    def test_a_failed_panel_split_is_reported_but_not_fatal(self):
        """Correction 2's target bug: the layout split that silently failed and shipped
        a frame with a missing panel. Here it must be REPORTED (command + tmux's
        stderr) — but the harness pane already exists and is already running, so the
        launch must still finish and return the harness's real exit code."""
        fake = _FakeTmux(panel_rc=1, exit_code=3)
        buf = []
        with mock.patch("charter.util.err", side_effect=lambda m: buf.append(m)):
            rc = _launch(fake)
        self.assertEqual(rc, 3)
        self.assertTrue(any("no space for a new pane" in m for m in buf),
                        f"the panel failure's tmux stderr was never surfaced: {buf}")
        self.assertTrue(any("attach" in c for c in fake.calls),
                        "a decorative panel failing must not cancel the attach")

    def test_below_the_tmux_floor_degrades_instead_of_refusing(self):
        """Correction 5: a version below `tmuxctl.FLOOR` prints `below_floor_message`
        and the frame still launches — only the (not-yet-wired) hotkey menu is
        affected, not the frame itself."""
        fake = _FakeTmux(exit_code=0)
        buf = []
        with mock.patch("charter.util.warn", side_effect=lambda m: buf.append(m)):
            rc = _launch(fake, version=(3, 0))
        self.assertEqual(rc, 0)
        self.assertTrue(any("3.0" in m and "3.2" in m for m in buf),
                        f"the below-floor message was never printed: {buf}")
        self.assertTrue(any("new-session" in c for c in fake.calls),
                        "below-floor must degrade, not refuse to launch")

    def test_a_terminal_size_os_cannot_report_falls_back_rather_than_crashing(self):
        """`os.get_terminal_size()` raises `OSError` even on a tty that passes
        `isatty()` — a documented trap, not a hypothetical. Must not propagate."""
        fake = _FakeTmux(exit_code=0)
        args = SimpleNamespace(harness="claude", rest=[], no_frame=False)
        with mock.patch("charter.commands_frame.subprocess.run", side_effect=fake), \
             mock.patch("sys.stdout.isatty", return_value=True), \
             mock.patch("charter.frame.tmuxctl.version", return_value=(3, 7)), \
             mock.patch("charter.workspace.resolve", return_value="demo"), \
             mock.patch("os.get_terminal_size", side_effect=OSError("no tty size")):
            rc = commands_frame.cmd_launch(args)
        self.assertEqual(rc, 0)
        session_cmd = next(c for c in fake.calls if "new-session" in c)
        self.assertEqual(session_cmd[session_cmd.index("-x") + 1], "80")
        self.assertEqual(session_cmd[session_cmd.index("-y") + 1], "24")

    def test_a_double_dash_separator_is_stripped_before_reaching_the_harness(self):
        """`nargs=argparse.REMAINDER` keeps a literal leading `--` when the operator
        typed one (`charter frame -- claude -p hi`); it is the token that told argparse
        to stop parsing, not part of what should run."""
        fake = _FakeTmux(exit_code=0)
        args = SimpleNamespace(harness="", rest=["--", "echo", "hi"], no_frame=False)
        with mock.patch("charter.commands_frame.subprocess.run", side_effect=fake), \
             mock.patch("sys.stdout.isatty", return_value=True), \
             mock.patch("charter.frame.tmuxctl.version", return_value=(3, 7)), \
             mock.patch("charter.workspace.resolve", return_value="demo"), \
             mock.patch("os.get_terminal_size", return_value=os_terminal_size(200, 50)):
            commands_frame.cmd_launch(args)
        session_cmd = next(c for c in fake.calls if "new-session" in c)
        self.assertEqual(session_cmd[session_cmd.index("--") + 1:], ["echo", "hi"])


class CollisionGuard(unittest.TestCase):
    def test_a_harness_cli_name_colliding_with_a_core_command_is_refused_at_build_time(self):
        """The additional requirement carried from Task 1: a harness registered with a
        `cli_name` that already names a `charter` command must be refused LOUDLY, at
        parser-construction time — never silently letting the harness shadow it.

        Patches `charter.harness.registry.KINDS` (the dict `harness.all()` actually
        reads inside its own module — `charter.harness.KINDS` is a re-exported ALIAS of
        the same object, but reassigning that alias via `mock.patch` would not touch
        what `registry.all()` reads from its own globals; `mock.patch.dict` mutating the
        real dict in place is what makes `harness.all()` see the fake entry).

        Asserts on the GUARD'S OWN wording (`clashing-harness`), not merely "a
        `ValueError` mentioning `status`" — on Python 3.12+, `argparse`'s own
        `add_parser` already raises `ValueError: conflicting subparser: status` for a
        second parser of the same name, which would make a weaker assertion pass even
        with this module's own guard deleted entirely (verified by hand: disabling the
        `if h.cli_name in sub.choices` check here still left this test green on 3.14,
        for exactly that reason — the guard exists for 3.11, where `add_parser` accepts
        a duplicate name silently). Checking for this guard's specific message is what
        makes the test depend on THIS code rather than on which interpreter runs it."""
        from charter import cli
        from charter.harness import registry
        from charter.harness.base import Harness

        class _CollidingHarness(Harness):
            name = "clashing-harness"
            cli_name = "status"  # already a core `charter` command (see cli.py)
            binary = "clashing"

        with mock.patch.dict("charter.harness.registry.KINDS",
                             {"clashing-harness": _CollidingHarness}, clear=True):
            with self.assertRaises(ValueError) as ctx:
                cli.build_parser()
        self.assertIn("clashing-harness", str(ctx.exception))
        self.assertIn("charter status", str(ctx.exception))


if __name__ == "__main__":
    unittest.main()
