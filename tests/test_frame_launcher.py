"""Starting a harness inside a frame, and the ways that must not go wrong.

The exit code is asserted because it was measured wrong once: an attached
`tmux new-session` returns 0 whatever its command exited with (pinned against 3.7c), so
the launcher waits and reads a recorded status instead of exec'ing and hoping. A second,
narrower race survives even that fix — the hook that records the status is not installed
until a moment after the harness starts running, so a harness that dies inside that
window is never caught by it, and (verified by hand against a real tmux 3.7c) nothing is
left to end the session either, so `attach` would block forever rather than merely return
early. `Launch` below covers the EAGER `display-message` check `cmd_launch` runs right
after installing the hook — before ever calling `attach` — that closes this.

**Every fake tmux command here returns a `subprocess.CompletedProcess`, never a real
tmux process** — the point of `charter/frame/tmuxctl.py` splitting the binary out of
everything else is that this module gets to keep that promise while still exercising the
FULL launch sequence: session creation, reading the captured pane id back off stdout,
loading the session-scoped config, installing the exit-code hook, drawing the panels,
and attaching. The shell-quoting and multi-session-server properties `_pane_died_hook_argv`
and `source-file` rely on were verified by hand against a real tmux 3.7c (see the
fix-round section of the task report) — that verification cannot be a unit test without
violating "never start a real tmux server in this suite".
"""

from __future__ import annotations

import shlex
import subprocess
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from tests._isolation import PersonaIso
from charter import commands_frame
from charter.frame import state


def _os_terminal_size(cols, rows):
    import os as _os
    return _os.terminal_size((cols, rows))


class Bypass(unittest.TestCase):
    def test_a_pipe_gets_no_frame(self):
        """`charter claude -p "…" | jq` must be `claude -p "…" | jq`. A frame around a
        pipe is wrong, and exec keeps the exit code without any help."""
        with mock.patch("sys.stdout.isatty", return_value=False), \
             mock.patch("os.execvp") as ex:
            commands_frame.bypass(["claude", "-p", "hi"])
        ex.assert_called_once_with("claude", ["claude", "-p", "hi"])


class Conf(unittest.TestCase):
    """`conf_text` no longer carries the `pane-died` hook (see `PaneDiedHook` below) —
    only the settings that are safe to bake into a text file `source-file` loads: no
    untrusted path ever reaches this function."""

    def test_status_mouse_and_history_limit_are_session_scoped(self):
        """Finding 4: `source-file` applies this text to the ONE shared server every
        frame runs on. `-g` (global) here would rewrite every OTHER live frame's mouse
        and scrollback the moment this frame's config loads — session-scoped (`-t
        <session>`, no `-g`) is what keeps frame N's settings from becoming frame
        (N-1)'s."""
        text = commands_frame.conf_text(hotkey="F2", mouse=True, history_limit=999,
                                        session="demo-42")
        self.assertIn("set -t demo-42 status off", text)
        self.assertIn("set -t demo-42 mouse on", text)
        self.assertIn("set -t demo-42 history-limit 999", text)
        self.assertNotIn("set -g mouse", text)
        self.assertNotIn("set -g history-limit", text)
        self.assertNotIn("set -g status", text)

    def test_escape_time_and_remain_on_exit_stay_global(self):
        """`escape-time` is a genuine SERVER option in tmux (verified by hand: it shows
        under `show-options -s`, never `-g`/session scope), so there is no per-session
        form to move it to. `remain-on-exit` is deliberately left global too — every
        frame wants the identical value, so sharing it leaks nothing frame-specific."""
        text = commands_frame.conf_text(hotkey="F2", mouse=False, history_limit=1,
                                        session="demo-42")
        self.assertIn("set -g escape-time 0", text)
        self.assertIn("set -g remain-on-exit on", text)

    def test_mouse_is_off_unless_asked_for(self):
        off = commands_frame.conf_text(hotkey="F2", mouse=False, history_limit=1,
                                       session="x")
        self.assertIn("mouse off", off)
        on = commands_frame.conf_text(hotkey="F2", mouse=True, history_limit=1,
                                      session="x")
        self.assertIn("mouse on", on)

    def test_history_limit_is_raised_above_the_tmux_default(self):
        """tmux ships 2000 lines, which becomes the harness's entire scrollback."""
        text = commands_frame.conf_text(hotkey="F2", mouse=False, history_limit=50000,
                                        session="x")
        self.assertIn("history-limit 50000", text)


class PaneDiedHook(unittest.TestCase):
    """`_pane_died_hook_argv` — issued as its OWN tmux command (clean argv), never text
    baked into `conf_text`'s output. See the module docstring in `commands_frame.py` for
    why: nesting a shell-quoted path inside a SECOND, text-file level of tmux quoting
    (what `source-file`'s config syntax requires) breaks `shlex.quote`'s own escaping for
    a path containing a literal single quote — verified by hand against tmux 3.7c."""

    def test_the_hook_targets_the_harness_pane(self):
        cmd = commands_frame._pane_died_hook_argv(socket="charter", harness_pane="%9",
                                                   status_path="/tmp/f/exit")
        self.assertEqual(cmd[cmd.index("-t") + 1], "%9")
        self.assertIn("pane-died", cmd)

    def test_it_is_a_clean_argv_list_naming_the_socket(self):
        cmd = commands_frame._pane_died_hook_argv(socket="charter", harness_pane="%0",
                                                   status_path="/x")
        self.assertIsInstance(cmd, list)
        for part in cmd:
            self.assertIsInstance(part, str)
        self.assertEqual(cmd[:3], ["tmux", "-L", "charter"])

    def test_the_status_path_is_shell_quoted_against_a_space(self):
        """Finding 5: `status_path` reaches `run-shell`, which hands it to `/bin/sh -c`.
        A plane root (or `$CHARTER_HOME`) containing a space — ordinary on macOS — must
        not silently truncate the write. `shlex.quote`'s exact output is asserted, not
        merely "no error", because a test that only checks `set-hook` didn't crash would
        pass even if the path were embedded completely unquoted."""
        path = "/tmp/My Plane/exit"
        cmd = commands_frame._pane_died_hook_argv(socket="charter", harness_pane="%0",
                                                   status_path=path)
        action = cmd[-1]
        self.assertIn(shlex.quote(path), action)
        # The raw, unquoted path must not appear on its own — only inside the quoted form.
        self.assertNotIn(f"> {path}\"", action)

    def test_a_dollar_paren_injection_attempt_is_neutralized(self):
        """The sharper form of the same finding: a path shaped like a command
        substitution must reach the shell as an inert literal, not run."""
        path = "/tmp/inj $(touch pwned)/exit"
        cmd = commands_frame._pane_died_hook_argv(socket="charter", harness_pane="%0",
                                                   status_path=path)
        action = cmd[-1]
        self.assertIn(shlex.quote(path), action)
        # Unquoted, "$(touch pwned)" would sit outside any quotes in the action string;
        # quoted, it is wrapped in the shell-single-quoted form `shlex.quote` produces.
        self.assertNotIn("> /tmp/inj $(touch pwned)/exit\"", action)


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


class QueryPaneDeadStatus(unittest.TestCase):
    """`_query_pane_dead_status` is the SAME function `cmd_launch` calls both eagerly
    (immediately after installing the hook, to close the install race) and again as a
    fallback after `attach` returns — pinned directly here, not only indirectly through
    `Launch`'s race-recovery test, since either call site reuses this one function."""

    def _dm(self, stdout, rc=0):
        return mock.patch(
            "charter.commands_frame.subprocess.run",
            return_value=subprocess.CompletedProcess([], rc, stdout=stdout, stderr=""))

    def test_a_dead_pane_returns_its_status(self):
        with self._dm("1:42"):
            self.assertEqual(commands_frame._query_pane_dead_status("charter", "%0"), 42)

    def test_a_live_pane_returns_none(self):
        with self._dm("0:"):
            self.assertIsNone(commands_frame._query_pane_dead_status("charter", "%0"))

    def test_a_failed_query_returns_none_not_zero(self):
        """A query that fails outright (pane already gone, tmux error) must never be
        read as "alive" OR as a fabricated "exit 0" — `None` is "cannot tell", not
        either kind of answer. `stdout` here is deliberately made to LOOK like a dead
        pane (`"1:99"`) alongside the nonzero return code: a test that failed the query
        with empty stdout would pass even if the returncode check were deleted, since
        empty stdout also fails the separate `dead != "1"` parse — this is what actually
        pins the returncode check as its own, distinct guard."""
        with self._dm("1:99", rc=1):
            self.assertIsNone(commands_frame._query_pane_dead_status("charter", "%0"))

    def test_unparseable_status_text_returns_none_rather_than_raising(self):
        """`#{pane_dead_status}` is tmux's own format variable, not operator input, but
        this function must not assume its shape: a malformed or unexpected value must
        degrade to "cannot tell", never raise `ValueError` out of `int(status)`."""
        with self._dm("1:not-a-number"):
            self.assertIsNone(commands_frame._query_pane_dead_status("charter", "%0"))

    def test_a_negative_status_is_parsed(self):
        """tmux reports a negative `pane_dead_status` for a process killed by a signal."""
        with self._dm("1:-15"):
            self.assertEqual(commands_frame._query_pane_dead_status("charter", "%0"), -15)


class _FakeTmux:
    """Every tmux invocation `cmd_launch` makes, keyed by which subcommand it carries.

    Standing in for the real binary (never started in this suite — see the module
    docstring) while still letting a test drive the FULL sequence: the launcher reads a
    pane id off `new-session`'s stdout, and this fake hands one back the same way real
    tmux does, so a test can assert what the launcher does with THAT VALUE rather than
    with a hardcoded stand-in like `"%0"` (the exact bug correction 3 exists to catch).

    Three ways an exit code can reach `cmd_launch`, each selected by which constructor
    argument is set (at most one at a time in any single test):

    - `exit_code`: the ORDINARY path. Written via `state.record_exit` at the moment the
      fake `attach` command runs — mimicking real timing, where `attach` blocks until the
      session ends and by the time it returns, the hook (fired on the real server while
      attach was blocked) has already recorded the code.
    - `race_death_status`: the RACE `cmd_launch`'s eager `display-message` check exists
      to close. Nothing is written via `state.record_exit` up front (the hook never got
      the chance to fire), but a fake `display-message` query answers `1:<status>` —
      what tmux would actually report for a pane that died before any hook existed but
      is still there to ask, thanks to `remain-on-exit`. `cmd_launch` asks this BEFORE
      ever calling `attach` (verified by hand against a real tmux 3.7c that a `attach`
      against exactly this state blocks forever otherwise, `remain-on-exit` legitimately
      keeping the session alive with nothing left to end it), so a correct launcher
      never reaches the fake `attach` handler at all in this scenario.
    - `still_live` (with both of the above left `None`): the session is still running
      when `attach` returns (an operator detach, not a finish) — `list-sessions` reports
      the frame's own session id as still live.
    """

    def __init__(self, *, pane_id="%7", exit_code=None, race_death_status=None,
                still_live=False, session_rc=0, source_rc=0, hook_rc=0, panel_rc=0,
                attach_rc=0, dm_rc=0):
        self.pane_id = pane_id
        self.exit_code = exit_code
        self.race_death_status = race_death_status
        self.still_live = still_live
        self.session_rc = session_rc
        self.source_rc = source_rc
        self.hook_rc = hook_rc
        self.panel_rc = panel_rc
        self.attach_rc = attach_rc
        self.dm_rc = dm_rc
        self.calls: list[list[str]] = []
        self.fid = None
        self.sourced_conf_text = None
        self.new_session_env = None
        self.kill_session_called = False

    def __call__(self, cmd, **kwargs):
        self.calls.append(list(cmd))
        if "new-session" in cmd:
            self.fid = cmd[cmd.index("-s") + 1]
            self.new_session_env = kwargs.get("env")
            return subprocess.CompletedProcess(cmd, self.session_rc,
                                               stdout=(self.pane_id + "\n") if self.session_rc == 0 else "",
                                               stderr="" if self.session_rc == 0 else "no space for a new pane")
        if "source-file" in cmd:
            conf_path = cmd[cmd.index("source-file") + 1]
            self.sourced_conf_text = Path(conf_path).read_text()
            return subprocess.CompletedProcess(cmd, self.source_rc, stdout="",
                                               stderr="" if self.source_rc == 0 else "bad config line")
        if "set-hook" in cmd:
            return subprocess.CompletedProcess(cmd, self.hook_rc, stdout="",
                                               stderr="" if self.hook_rc == 0 else "bad hook target")
        if "split-window" in cmd:
            return subprocess.CompletedProcess(cmd, self.panel_rc, stdout="",
                                               stderr="" if self.panel_rc == 0 else "no space for a new pane")
        if "display-message" in cmd:
            out = f"1:{self.race_death_status}" if self.race_death_status is not None else "0:"
            return subprocess.CompletedProcess(cmd, self.dm_rc, stdout=out, stderr="")
        if "kill-session" in cmd:
            self.kill_session_called = True
            return subprocess.CompletedProcess(cmd, 0, stdout="", stderr="")
        if "attach" in cmd:
            if self.exit_code is not None and self.fid:
                state.record_exit(self.fid, self.exit_code)
            return subprocess.CompletedProcess(cmd, self.attach_rc, stdout="", stderr="")
        if "list-sessions" in cmd:
            live = {self.fid} if (self.still_live and self.fid) else set()
            return subprocess.CompletedProcess(cmd, 0, stdout="\n".join(live), stderr="")
        raise AssertionError(f"unexpected tmux command in test: {cmd}")


def _launch(fake: _FakeTmux, *, cols=200, rows=50, version=(3, 7), harness="claude",
           rest=()):
    args = SimpleNamespace(harness=harness, rest=list(rest), no_frame=False)
    with mock.patch("charter.commands_frame.subprocess.run", side_effect=fake), \
         mock.patch("sys.stdout.isatty", return_value=True), \
         mock.patch("charter.frame.tmuxctl.version", return_value=version), \
         mock.patch("charter.workspace.resolve", return_value="demo"), \
         mock.patch("os.get_terminal_size", return_value=_os_terminal_size(cols, rows)):
        return commands_frame.cmd_launch(args)


class Launch(PersonaIso, unittest.TestCase):
    def test_the_pane_died_hook_targets_the_pane_tmux_actually_reported(self):
        """The regression correction 3 names directly: the hook's target must be the id
        READ OFF tmux's stdout, never the literal `"%0"` that happens to be right only
        for the very first pane ever created on a fresh server. `_FakeTmux` reports
        `%7` — a value `"%0"` could never produce by accident — so this fails if the
        launcher ever falls back to a hardcoded pane id."""
        fake = _FakeTmux(pane_id="%7", exit_code=0)
        rc = _launch(fake)
        self.assertEqual(rc, 0)
        hook_cmd = next(c for c in fake.calls if "set-hook" in c)
        self.assertEqual(hook_cmd[hook_cmd.index("-t") + 1], "%7")

    def test_the_config_is_loaded_with_source_file_not_only_dash_f(self):
        """Measured against tmux 3.7c (module docstring of commands_frame.py): `-f` on
        `new-session` is read only when that call starts a brand-new server, so a SECOND
        frame sharing `SOCKET` would never get its own settings applied if the launcher
        relied on `-f` alone. `source-file`, run after the pane id is known, applies
        live regardless of whether the server was already running."""
        fake = _FakeTmux(exit_code=0)
        _launch(fake)
        self.assertTrue(any("source-file" in c for c in fake.calls),
                        "the real config was never loaded via `source-file`")
        self.assertIn("mouse", fake.sourced_conf_text)

    def test_the_pane_died_hook_is_installed_as_its_own_command(self):
        """Companion to the `source-file` test: the hook must not be missing from BOTH
        places (baked into neither the sourced config nor issued separately)."""
        fake = _FakeTmux(exit_code=0)
        _launch(fake)
        self.assertTrue(any("set-hook" in c and "pane-died" in " ".join(c)
                           for c in fake.calls),
                        "the pane-died hook was never installed")
        # And NOT duplicated into the text file `source-file` loads (see `Conf`/
        # `PaneDiedHook`'s own docstrings for why it moved out entirely).
        self.assertNotIn("pane-died", fake.sourced_conf_text or "")

    def test_the_recorded_exit_code_wins_over_a_zero_from_attach(self):
        """The whole module's reason to exist: an attached `tmux attach` (like
        `new-session`) returns 0 for the CLIENT's own detach, not for what ran inside —
        so a real crash must still surface even though `attach`'s own returncode is 0."""
        fake = _FakeTmux(exit_code=17, attach_rc=0)
        rc = _launch(fake)
        self.assertEqual(rc, 17)

    def test_a_death_that_races_the_hooks_own_install_is_recovered(self):
        """Critical 1: `new-session` starts the harness immediately; the hook that would
        record its exit code is not installed until a `set-hook` call some time later. A
        harness that dies inside that window is never caught by the hook — nothing was
        listening yet, and hooks do not fire retroactively.

        The check for this is EAGER — run immediately after `set-hook` returns, before
        `attach` is ever called — not merely a fallback after `attach` returns. Verified
        by hand against a real tmux 3.7c: with nothing left to run the hook's own
        `kill-session`, `remain-on-exit` legitimately keeps the session alive forever,
        so an `attach` reaching this state BLOCKS FOREVER rather than returning 0 early.
        A correct launcher must therefore finish the hook's own job itself (record the
        code, run `kill-session`) and skip `attach` entirely — this test fails if
        `attach` is ever reached in this scenario, not only if the wrong code comes
        back."""
        fake = _FakeTmux(race_death_status=42)
        # `state.record_exit` is asserted on directly, mid-call, rather than by reading
        # `state.exit_code` back after `_launch` returns: `reap()` legitimately deletes
        # a frame's directory once its session is truly gone (which it is here, once
        # `kill-session` runs), and this test's own `_FakeTmux` reports nothing live —
        # so a post-hoc read would see the sentinel `None` regardless of whether the
        # code was ever written, for a reason that has nothing to do with this test.
        with mock.patch("charter.frame.state.record_exit",
                        side_effect=state.record_exit) as rec:
            rc = _launch(fake)
        self.assertEqual(rc, 42)
        rec.assert_called_once_with(fake.fid, 42)
        self.assertTrue(any("display-message" in c for c in fake.calls))
        self.assertTrue(fake.kill_session_called,
                        "nothing else was ever going to end this session")
        self.assertFalse(any("attach" in c for c in fake.calls),
                         "attach must never be reached against a session already known "
                         "to be over — reaching it here would hang against real tmux")

    def test_remain_on_exit_is_armed_before_the_pane_id_is_even_known(self):
        """The other half of Critical 1: the placeholder loaded via `new-session`'s own
        `-f` — written and on disk BEFORE that command even runs — must already carry
        `remain-on-exit on`, because a harness that dies in the opening milliseconds
        (a missing binary is the sharpest case) needs its pane to survive before this
        launcher has done anything else at all, config or hook alike."""
        conf_snapshots = []

        def _peek(cmd, **kwargs):
            if "new-session" in cmd:
                conf_path = Path(cmd[cmd.index("-f") + 1])
                conf_snapshots.append(conf_path.read_text())
            return fake_call(cmd, **kwargs)

        fake = _FakeTmux(exit_code=0)
        fake_call = fake.__call__
        args = SimpleNamespace(harness="claude", rest=[], no_frame=False)
        with mock.patch("charter.commands_frame.subprocess.run", side_effect=_peek), \
             mock.patch("sys.stdout.isatty", return_value=True), \
             mock.patch("charter.frame.tmuxctl.version", return_value=(3, 7)), \
             mock.patch("charter.workspace.resolve", return_value="demo"), \
             mock.patch("os.get_terminal_size", return_value=_os_terminal_size(200, 50)):
            commands_frame.cmd_launch(args)
        self.assertEqual(len(conf_snapshots), 1)
        self.assertIn("remain-on-exit on", conf_snapshots[0])

    def test_a_still_live_session_after_attach_is_a_detach_not_a_silent_zero(self):
        """Finding 6, and the spec's own words: "Detach is allowed and prints how to
        reattach... returning silently to a shell with it still running is not." A
        session `list-sessions` still reports after `attach` returns is this frame's
        own — the harness is still running, so this must not read as success by
        accident and must not stay silent about it."""
        fake = _FakeTmux(still_live=True)
        buf = []
        with mock.patch("charter.util.info", side_effect=lambda m: buf.append(m)):
            rc = _launch(fake)
        self.assertEqual(rc, 0)
        self.assertTrue(any("detach" in m.lower() and fake.fid in m for m in buf),
                        f"no reattach message was printed: {buf}")
        # The frame's own directory must not be reaped while the session is still live.
        self.assertTrue(state.frame_dir(fake.fid).exists())

    def test_a_failed_hook_install_is_reported_but_not_fatal(self):
        """A harness pane already exists and is already running by the time the hook is
        installed — losing exit-code tracking for this one frame is a real degradation,
        but killing an already-live pane over it would be worse."""
        fake = _FakeTmux(hook_rc=1, exit_code=5)
        buf = []
        with mock.patch("charter.util.err", side_effect=lambda m: buf.append(m)):
            rc = _launch(fake)
        self.assertEqual(rc, 5)
        self.assertTrue(any("bad hook target" in m for m in buf),
                        f"the hook-install failure's tmux stderr was never surfaced: {buf}")
        self.assertTrue(any("attach" in c for c in fake.calls))

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
        rc = _launch(fake, harness="", rest=["--", "echo", "hi"])
        self.assertEqual(rc, 0)
        session_cmd = next(c for c in fake.calls if "new-session" in c)
        self.assertEqual(session_cmd[session_cmd.index("--") + 1:], ["echo", "hi"])

    def test_charter_session_id_reaches_the_harness_environment(self):
        """Task 8's `notify.plane_changed` reads `$CHARTER_SESSION_ID` back out of the
        running harness's own environment — this is the one place it is ever set."""
        fake = _FakeTmux(exit_code=0)
        rc = _launch(fake)
        self.assertEqual(rc, 0)
        self.assertIsNotNone(fake.new_session_env)
        self.assertEqual(fake.new_session_env.get("CHARTER_SESSION_ID"), fake.fid)

    def test_a_frame_dir_the_state_module_refuses_does_not_crash(self):
        """`frame_dir` returns `None` rather than a `Path` for an id it cannot shape
        into a directory (see charter/frame/state.py) — this launcher must treat that as
        data, not assume it always gets a `Path` back."""
        args = SimpleNamespace(harness="claude", rest=[], no_frame=False)
        with mock.patch("charter.commands_frame.subprocess.run") as run, \
             mock.patch("sys.stdout.isatty", return_value=True), \
             mock.patch("charter.frame.tmuxctl.version", return_value=(3, 7)), \
             mock.patch("charter.workspace.resolve", return_value="demo"), \
             mock.patch("charter.frame.state.frame_dir", return_value=None), \
             mock.patch("os.get_terminal_size", return_value=_os_terminal_size(200, 50)):
            rc = commands_frame.cmd_launch(args)
        self.assertNotEqual(rc, 0)
        # `run` is reached once, for the pre-create `reap()` pass (see the reap-ordering
        # test below) — but never for anything that needs a working state directory.
        self.assertFalse(any("new-session" in c.args[0] for c in run.call_args_list))

    def test_reap_runs_before_this_frames_own_directory_is_created(self):
        """Finding 12: `state.reap` compares live tmux sessions against directories
        already on disk. Creating THIS frame's own directory first — before its tmux
        session exists — would make it look exactly like an abandoned one, and the very
        next `reap()` call would delete a directory this launch had not even started
        using yet."""
        order = []
        real_reap = state.reap
        real_frame_dir = state.frame_dir

        def _track_reap(live):
            order.append("reap")
            return real_reap(live)

        def _track_frame_dir(fid, **kw):
            if kw.get("create"):
                order.append("frame_dir_create")
            return real_frame_dir(fid, **kw)

        fake = _FakeTmux(exit_code=0)
        with mock.patch("charter.frame.state.reap", side_effect=_track_reap), \
             mock.patch("charter.frame.state.frame_dir", side_effect=_track_frame_dir):
            _launch(fake)
        self.assertIn("reap", order)
        self.assertIn("frame_dir_create", order)
        self.assertLess(order.index("reap"), order.index("frame_dir_create"),
                        f"reap must run before this frame's own directory is created: {order}")


class BypassRouting(unittest.TestCase):
    """`Bypass.test_a_pipe_gets_no_frame` pins `bypass()`'s OWN argv shape but never
    calls `cmd_launch` at all — it cannot catch a `cmd_launch` that stopped routing to
    `bypass()` in the first place (confirmed: deleting the routing check, or replacing
    it with `if False:`, left the full 15-test suite green before this class existed).
    These test the DECISION, not what `bypass()` does once reached."""

    def test_the_no_frame_flag_routes_to_bypass(self):
        args = SimpleNamespace(harness="claude", rest=[], no_frame=True)
        with mock.patch("charter.commands_frame.bypass", return_value=0) as byp, \
             mock.patch("sys.stdout.isatty", return_value=True), \
             mock.patch("charter.commands_frame.subprocess.run") as run:
            rc = commands_frame.cmd_launch(args)
        byp.assert_called_once_with(["claude"])
        run.assert_not_called()
        self.assertEqual(rc, 0)

    def test_a_non_tty_stdout_routes_to_bypass_even_without_the_flag(self):
        args = SimpleNamespace(harness="claude", rest=[], no_frame=False)
        with mock.patch("charter.commands_frame.bypass", return_value=0) as byp, \
             mock.patch("sys.stdout.isatty", return_value=False), \
             mock.patch("charter.commands_frame.subprocess.run") as run:
            rc = commands_frame.cmd_launch(args)
        byp.assert_called_once_with(["claude"])
        run.assert_not_called()
        self.assertEqual(rc, 0)

    def test_a_tty_without_no_frame_does_not_bypass(self):
        fake = _FakeTmux(exit_code=0)
        with mock.patch("charter.commands_frame.bypass") as byp:
            _launch(fake)
        byp.assert_not_called()


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

    def test_a_harness_named_frame_is_refused_too(self):
        """Finding 11: the loop that registers every harness runs BEFORE `sub.add_parser
        ("frame", ...)` does, so a harness with `cli_name == "frame"` would pass a check
        against `sub.choices` alone (nothing is named `frame` there yet) and only
        collide once the escape hatch's own `add_parser` call runs a few lines later —
        where, on the 3.11 floor (`argparse` does not raise for a re-added name there —
        see `_split_frame_argv`'s docstring for the same version gap elsewhere), it
        would silently shadow the harness instead of the other way around."""
        from charter import cli
        from charter.harness.base import Harness

        class _FrameHarness(Harness):
            name = "frame-harness"
            cli_name = "frame"
            binary = "frame-harness"

        with mock.patch.dict("charter.harness.registry.KINDS",
                             {"frame-harness": _FrameHarness}, clear=True):
            with self.assertRaises(ValueError) as ctx:
                cli.build_parser()
        self.assertIn("frame-harness", str(ctx.exception))


class FrameArgvSplit(unittest.TestCase):
    """Critical 2: `charter claude -p hi` — the documented, spec-named invocation — was
    refused outright by `argparse` before this fix (`nargs=argparse.REMAINDER` cannot
    hold a positional whose first token looks like an option `argparse` itself does not
    recognize; confirmed identical on 3.9, 3.12, 3.14). `_split_frame_argv` peels the
    harness's own argv off before `argparse` ever parses it, the same shape
    `_split_exec_command` already uses for `secret exec`."""

    def _parse(self, argv):
        from charter.cli import _split_exec_command, _split_frame_argv, build_parser
        rest, tail = _split_exec_command(list(argv))
        rest2, frame_rest = _split_frame_argv(rest)
        ns = build_parser().parse_args(rest2)
        if tail is not None:
            ns.command = tail
        if frame_rest is not None:
            ns.rest = frame_rest
        return ns

    def test_a_leading_short_flag_reaches_the_harness(self):
        """The exact case the coordinator reported: `charter claude -p hi`."""
        ns = self._parse(["claude", "-p", "hi"])
        self.assertEqual(ns.rest, ["-p", "hi"])
        self.assertFalse(ns.no_frame)

    def test_a_leading_long_flag_reaches_the_harness(self):
        ns = self._parse(["claude", "--continue"])
        self.assertEqual(ns.rest, ["--continue"])

    def test_a_bare_no_frame_is_still_recognized(self):
        ns = self._parse(["claude", "--no-frame"])
        self.assertTrue(ns.no_frame)
        self.assertEqual(ns.rest, [])

    def test_no_frame_followed_by_harness_flags(self):
        ns = self._parse(["claude", "--no-frame", "-p", "hi"])
        self.assertTrue(ns.no_frame)
        self.assertEqual(ns.rest, ["-p", "hi"])

    def test_an_explicit_double_dash_still_works(self):
        ns = self._parse(["claude", "--", "-p", "hi"])
        self.assertEqual(ns.rest, ["--", "-p", "hi"])

    def test_the_frame_escape_hatch_is_covered_too(self):
        ns = self._parse(["frame", "--", "some-cmd", "--flag"])
        self.assertEqual(ns.rest, ["--", "some-cmd", "--flag"])

    def test_bare_help_still_shows_charters_own_help_not_a_crash(self):
        """The regression a first draft of this fix introduced: treating `--help` as
        just more harness argv left `charter claude --help` reaching the BYPASS path
        with `rest=["--help"]`, which `os.execvp("--help", ...)` turned into an actual
        `FileNotFoundError` — a crash, not merely a UX change. `-h`/`--help` are kept in
        the same fixed-leading-position allowlist as `--no-frame` for exactly this
        reason: `argparse` intercepts them itself and exits cleanly."""
        from charter.cli import _split_frame_argv
        rest, frame_rest = _split_frame_argv(["claude", "--help"])
        self.assertEqual(rest, ["claude", "--help"])
        self.assertEqual(frame_rest, [])
        with self.assertRaises(SystemExit) as ctx:
            self._parse(["claude", "--help"])
        self.assertEqual(ctx.exception.code, 0)

    def test_a_later_help_still_reaches_the_harness(self):
        """`--help` loses its special status once it is not the immediate next token —
        `charter claude --continue --help` means "run claude, tell IT to show its own
        help", not charter's."""
        ns = self._parse(["claude", "--continue", "--help"])
        self.assertEqual(ns.rest, ["--continue", "--help"])

    def test_unrelated_subcommands_are_untouched(self):
        from charter.cli import _split_frame_argv
        for argv in (["workspace", "list"], ["persona", "list"], ["status"]):
            with self.subTest(argv=argv):
                rest, frame_rest = _split_frame_argv(list(argv))
                self.assertEqual(rest, argv)
                self.assertIsNone(frame_rest)


if __name__ == "__main__":
    unittest.main()
