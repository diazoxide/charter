"""Starting a harness inside a frame, and the ways that must not go wrong.

The exit code is asserted because it was measured wrong once: an attached
`tmux new-session` returns 0 whatever its command exited with (pinned against 3.7c), so
the launcher waits and reads a recorded status instead of exec'ing and hoping. A second,
narrower race survives even that fix — the hooks that record the status and end the
session are not installed until a moment after the harness starts running, so a harness
that dies inside that window is never caught by them, and (verified by hand against a
real tmux 3.7c) nothing is left to end the session either, so `attach` would block
forever rather than merely return early. `Launch` below covers the EAGER
`display-message` check `cmd_launch` runs right after installing both hooks — before
ever calling `attach` — that closes this, and its harder cousin: a harness killed by a
signal reports `#{pane_dead_status}` EMPTY, not negative, and treating empty as "cannot
tell" reopens the same hang from a different angle (confirmed by hand:
`charter frame -- bash -c 'kill -9 $$'` hung under an earlier version of this fix).

**Every fake tmux command here returns a `subprocess.CompletedProcess`, never a real
tmux process** — the point of `charter/frame/tmuxctl.py` splitting the binary out of
everything else is that this module gets to keep that promise while still exercising the
FULL launch sequence: session creation, reading the captured pane id back off stdout,
loading the session-scoped config, carrying the exit-status path out of band, installing
the write and teardown hooks, drawing the panels, focusing the harness pane, and
attaching. The shell-quoting, hook-independence, and multi-session-server properties this
relies on were verified by hand against a real tmux 3.7c (see the fix-round sections of
the task report) — that verification cannot be a unit test without violating "never
start a real tmux server in this suite".
"""

from __future__ import annotations

import os
import subprocess
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from tests._isolation import PersonaIso
from charter import commands_frame, config
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
    """Neither `pane-died` hook lives in `conf_text` — see `PaneDiedHooks` below — only
    the settings that are safe to bake into a text file `source-file` loads."""

    def test_status_mouse_and_history_limit_are_session_scoped(self):
        """`source-file` applies this text to the ONE shared server every frame runs
        on. `-g` (global) here would rewrite every OTHER live frame's mouse and
        scrollback the moment this frame's config loads — session-scoped (`-t
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


class PaneDiedHooks(unittest.TestCase):
    """The exit-status hook is now TWO separate tmux commands, not one — see
    `commands_frame.py`'s module docstring ("Teardown is its own hook") for why a
    write hook (`pane-died[0]`, carries the real status) and a teardown hook
    (`pane-died[1]`, a constant `kill-session`) are installed independently: a bug in
    the write hook's own construction must never be able to take teardown down with it.
    The status path itself is delivered out of band (`_exit_path_env_argv`,
    `set-environment`), never embedded in either hook's action text."""

    def test_the_write_hook_targets_the_harness_pane(self):
        cmd = commands_frame._pane_died_write_hook_argv(socket="charter", harness_pane="%9")
        self.assertEqual(cmd[cmd.index("-t") + 1], "%9")
        self.assertIn("pane-died", cmd)
        self.assertNotIn("pane-died[1]", cmd)

    def test_the_write_hook_is_scoped_to_the_pane_not_the_whole_session(self):
        """`-p` is what makes this a PANE hook rather than a session-wide one — verified
        against tmux 3.7c what dropping it costs: without `-p`, `pane-died` fires for
        EVERY pane in the session, so a decorative PANEL exiting (77, say) writes 77
        into the harness's own exit-status file, and the operator sees a panel's exit
        status reported as their agent's."""
        cmd = commands_frame._pane_died_write_hook_argv(socket="charter", harness_pane="%9")
        self.assertIn("-p", cmd)

    def test_the_write_hook_action_is_constant(self):
        """No operator- or plane-derived string is ever embedded in this text — the
        exact bug that let a plane root with a literal `'` in it hang silently
        (`set-hook` reported success while the stored action was corrupted). Two calls
        with different `harness_pane`s produce IDENTICAL action text; only the `-t`
        target differs."""
        a = commands_frame._pane_died_write_hook_argv(socket="charter", harness_pane="%1")
        b = commands_frame._pane_died_write_hook_argv(socket="charter", harness_pane="%2")
        self.assertEqual(a[-1], b[-1])

    def test_the_write_hook_falls_back_to_the_unknown_death_sentinel_when_status_is_empty(self):
        """Measured against tmux 3.7c: a harness killed by a signal
        (SIGKILL/SIGTERM/SIGSEGV) reports `#{pane_dead_status}` EMPTY, not a negative
        number. An unqualified `echo #{pane_dead_status} > ...` would write an empty,
        unparseable line — `state.exit_code` cannot parse it and reads back `None`,
        exactly like nothing was ever recorded. Confirmed by hand: `charter frame --
        bash -c 'sleep 1.5; kill -9 $$'` (well past the install race, so the hooks ARE
        installed) returned 0 — silent success for a crashed harness — before this
        fix. `${v:-N}` in the shell fragment closes it at the point of writing."""
        cmd = commands_frame._pane_died_write_hook_argv(socket="charter", harness_pane="%0")
        action = cmd[-1]
        self.assertIn(f"${{v:-{commands_frame._UNKNOWN_DEATH_CODE}}}", action)

    def test_the_teardown_hook_is_a_constant_kill_session_at_index_1(self):
        cmd = commands_frame._pane_died_teardown_hook_argv(socket="charter", harness_pane="%9")
        self.assertEqual(cmd[cmd.index("-t") + 1], "%9")
        self.assertIn("pane-died[1]", cmd)
        self.assertEqual(cmd[-1], "kill-session")

    def test_both_hooks_are_clean_argv_lists_naming_the_socket(self):
        for cmd in (commands_frame._pane_died_write_hook_argv(socket="charter", harness_pane="%0"),
                   commands_frame._pane_died_teardown_hook_argv(socket="charter", harness_pane="%0")):
            self.assertIsInstance(cmd, list)
            for part in cmd:
                self.assertIsInstance(part, str)
            self.assertEqual(cmd[:3], ["tmux", "-L", "charter"])

    def test_the_status_path_is_carried_out_of_band(self):
        """`set-environment` takes the path as ONE argv value — no shell, no tmux
        text-command parsing of it at all — verified by hand to round-trip a space, a
        literal `'`, and a `$(...)` injection attempt correctly precisely because
        nothing here ever re-parses it as text."""
        path = "/tmp/My Plane's exit $(touch pwned)/exit"
        cmd = commands_frame._exit_path_env_argv(socket="charter", session="demo-1",
                                                 status_path=path)
        self.assertEqual(cmd, ["tmux", "-L", "charter", "set-environment", "-t", "demo-1",
                              "CHARTER_FRAME_EXIT", path])


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
    (immediately after installing the hooks, to close the install race) and again as a
    fallback after `attach` returns — pinned directly here, not only indirectly through
    `Launch`'s race-recovery test, since either call site reuses this one function."""

    def _dm(self, stdout, rc=0):
        return mock.patch(
            "charter.commands_frame.subprocess.run",
            return_value=subprocess.CompletedProcess([], rc, stdout=stdout, stderr=""))

    def test_the_unknown_death_sentinel_is_not_zero(self):
        """Every other test in this class refers to `_UNKNOWN_DEATH_CODE` symbolically,
        which pins nothing about its actual VALUE — and `0` is precisely the fabricated
        silent success this whole module exists to rule out. A pane confirmed dead with
        no readable status must never be reported as if it exited cleanly."""
        self.assertNotEqual(commands_frame._UNKNOWN_DEATH_CODE, 0)

    def test_a_dead_pane_returns_its_status(self):
        with self._dm("1:42"):
            self.assertEqual(commands_frame._query_pane_dead_status("charter", "%0"), 42)

    def test_a_subprocess_error_returns_none_rather_than_raising(self):
        """Correction 2's counterpart for a query rather than an action: `_live_sessions`
        already wraps its `subprocess.run` call this way, and this function must too — a
        `TimeoutExpired`/`OSError` escaping here would crash `cmd_launch` AFTER the
        harness is already running, which is a worse failure than the one this whole
        function exists to prevent (a fabricated answer)."""
        with mock.patch("charter.commands_frame.subprocess.run",
                        side_effect=OSError("no such tmux")):
            self.assertIsNone(commands_frame._query_pane_dead_status("charter", "%0"))

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

    def test_unparseable_status_text_degrades_to_the_sentinel_rather_than_raising(self):
        """`#{pane_dead_status}` is tmux's own format variable, not operator input, but
        this function must not assume its shape: a malformed value must never raise
        `ValueError` out of `int(status)`. It degrades to `_UNKNOWN_DEATH_CODE`, not
        `None` — by the time `status` is even inspected, `dead == "1"` already confirmed
        the pane IS dead (see `test_a_failed_query_returns_none_not_zero` above for the
        one case that still answers `None`: the query itself failing, before `dead` is
        even known). `None` would tell `cmd_launch` to go ahead and attach a pane
        already known to be gone."""
        with self._dm("1:not-a-number"):
            self.assertEqual(commands_frame._query_pane_dead_status("charter", "%0"),
                             commands_frame._UNKNOWN_DEATH_CODE)

    def test_an_empty_status_is_dead_with_unknown_status_not_cannot_tell(self):
        """Fix round 2, item 1/2: measured against tmux 3.7c, a harness killed by a
        signal (SIGKILL/SIGTERM/SIGSEGV) reports `pane_dead=1` with an EMPTY
        `pane_dead_status` — replaces an earlier version of this test class that wrongly
        assumed a signal death produced a NEGATIVE number (see the test below, reworded
        rather than deleted, for the parser's own handling of that shape). Treating
        empty as `None` ("cannot tell") told `cmd_launch` "alive, go ahead and attach" —
        confirmed by hand that this hung `charter frame -- bash -c 'kill -9 $$'`
        forever, against a session `remain-on-exit` was correctly keeping alive with
        nothing left to end it."""
        with self._dm("1:"):
            self.assertEqual(commands_frame._query_pane_dead_status("charter", "%0"),
                             commands_frame._UNKNOWN_DEATH_CODE)

    def test_a_negative_status_would_still_be_parsed_if_one_were_ever_reported(self):
        """NOT a claim that tmux reports negative statuses for a signal death — verified
        it does not (see the empty-status test above, which is what actually happens
        and is what this test used to wrongly assert). Kept, reworded, to pin the
        parser's own `-` handling defensively in case some other tmux version or
        platform ever does report one."""
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
      session ends and by the time it returns, the write hook (fired on the real server
      while attach was blocked) has already recorded the code.
    - `race_death_status`: the RACE `cmd_launch`'s eager `display-message` check exists
      to close. Nothing is written via `state.record_exit` up front (the hooks never got
      the chance to fire), but a fake `display-message` query answers `1:<status>` —
      what tmux would actually report for a pane that died before any hook existed but
      is still there to ask, thanks to `remain-on-exit`. `cmd_launch` asks this BEFORE
      ever calling `attach` (verified by hand against a real tmux 3.7c that `attach`
      against exactly this state blocks forever otherwise), so a correct launcher never
      reaches the fake `attach` handler at all in this scenario.
    - `still_live` (with both of the above left `None`): the session is still running
      when `attach` returns (an operator detach, not a finish) — `list-sessions` reports
      the frame's own session id as still live.

    `pre_existing_sessions` answers the FIRST `list-sessions` call (before this frame's
    own session exists, i.e. `self.fid` is still `None`) — a separate knob from
    `still_live`, which only answers the SECOND call (after this frame's session was
    created), because they model two different questions: "was anything else already
    running before this launch" versus "is THIS frame still running after `attach`".
    """

    def __init__(self, *, pane_id="%7", exit_code=None, race_death_status=None,
                still_live=False, pre_existing_sessions=frozenset(),
                panel_pane_ids=None,
                session_rc=0, source_rc=0, env_set_rc=0, write_hook_rc=0,
                teardown_hook_rc=0, panel_rc=0, select_rc=0, attach_rc=0, dm_rc=0,
                kill_rc=0, arm_rc=0, resize_hook_rc=0,
                resize_hook_stderr="bad resize hook target"):
        self.pane_id = pane_id
        self.exit_code = exit_code
        self.race_death_status = race_death_status
        self.still_live = still_live
        self.pre_existing_sessions = pre_existing_sessions
        # slot -> pane id `split-window` reports for it. Empty by default (see
        # `split-window`'s own handler below for why that matters for every other test
        # in this class).
        self.panel_pane_ids = panel_pane_ids or {}
        self.session_rc = session_rc
        self.source_rc = source_rc
        self.env_set_rc = env_set_rc
        self.write_hook_rc = write_hook_rc
        self.teardown_hook_rc = teardown_hook_rc
        self.panel_rc = panel_rc
        self.select_rc = select_rc
        self.attach_rc = attach_rc
        self.dm_rc = dm_rc
        self.kill_rc = kill_rc
        self.arm_rc = arm_rc
        self.resize_hook_rc = resize_hook_rc
        # Distinct from every other stderr string in this fake — a test needs to
        # control it independently to exercise the "invalid option" degrade (fix
        # round 3, item 2) separately from an ordinary resize-hook failure.
        self.resize_hook_stderr = resize_hook_stderr
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
        if "remain-on-exit" in cmd:
            # The direct "arm ahead of an already-running server" command — distinct
            # from `source-file` (which carries `remain-on-exit` as text in a config
            # file, never as a bare tmux argv command).
            return subprocess.CompletedProcess(cmd, self.arm_rc, stdout="",
                                               stderr="" if self.arm_rc == 0 else "cannot set")
        if "set-environment" in cmd:
            return subprocess.CompletedProcess(cmd, self.env_set_rc, stdout="",
                                               stderr="" if self.env_set_rc == 0 else "bad session")
        if "set-hook" in cmd:
            if "pane-died[1]" in cmd:
                return subprocess.CompletedProcess(cmd, self.teardown_hook_rc, stdout="",
                                                   stderr="" if self.teardown_hook_rc == 0 else "bad teardown target")
            if "window-resized" in cmd:
                return subprocess.CompletedProcess(cmd, self.resize_hook_rc, stdout="",
                                                   stderr="" if self.resize_hook_rc == 0 else self.resize_hook_stderr)
            return subprocess.CompletedProcess(cmd, self.write_hook_rc, stdout="",
                                               stderr="" if self.write_hook_rc == 0 else "bad hook target")
        if "select-pane" in cmd:
            return subprocess.CompletedProcess(cmd, self.select_rc, stdout="",
                                               stderr="" if self.select_rc == 0 else "no such pane")
        if "split-window" in cmd:
            # Real tmux, with `-P -F '#{pane_id}'` now always on this argv (see
            # `layout.panel_argvs`), prints the new pane's id. Most tests leave
            # `panel_pane_ids` empty (its default) and get the SAME "reports nothing"
            # behaviour this fake always had — deliberately, so this addition doesn't
            # ripple through every other test in this class, which are not ABOUT the
            # resize hook. Keyed by slot (`cmd[cmd.index("panel") + 1]`, the literal
            # argv element `panel_argvs` always emits right after `--`), not by call
            # order, so a test can name exactly which slot(s) it wants a real id for.
            slot = cmd[cmd.index("panel") + 1] if "panel" in cmd else None
            pane_id = self.panel_pane_ids.get(slot, "") if self.panel_rc == 0 else ""
            return subprocess.CompletedProcess(cmd, self.panel_rc,
                                               stdout=f"{pane_id}\n" if pane_id else "",
                                               stderr="" if self.panel_rc == 0 else "no space for a new pane")
        if "display-message" in cmd:
            out = f"1:{self.race_death_status}" if self.race_death_status is not None else "0:"
            return subprocess.CompletedProcess(cmd, self.dm_rc, stdout=out, stderr="")
        if "kill-session" in cmd:
            self.kill_session_called = True
            return subprocess.CompletedProcess(cmd, self.kill_rc, stdout="",
                                               stderr="" if self.kill_rc == 0 else "no such session")
        if "attach" in cmd:
            if self.exit_code is not None and self.fid:
                state.record_exit(self.fid, self.exit_code)
            return subprocess.CompletedProcess(cmd, self.attach_rc, stdout="", stderr="")
        if "list-sessions" in cmd:
            if self.fid is None:
                live = set(self.pre_existing_sessions)
            else:
                live = {self.fid} if self.still_live else set()
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
    def test_the_write_hook_targets_the_pane_tmux_actually_reported(self):
        """The regression correction 3 names directly: the hook's target must be the id
        READ OFF tmux's stdout, never the literal `"%0"` that happens to be right only
        for the very first pane ever created on a fresh server. `_FakeTmux` reports
        `%7` — a value `"%0"` could never produce by accident — so this fails if the
        launcher ever falls back to a hardcoded pane id."""
        fake = _FakeTmux(pane_id="%7", exit_code=0)
        rc = _launch(fake)
        self.assertEqual(rc, 0)
        write_hook_cmd = next(c for c in fake.calls
                              if "set-hook" in c and "pane-died[1]" not in c)
        self.assertEqual(write_hook_cmd[write_hook_cmd.index("-t") + 1], "%7")

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

    def test_both_hooks_are_installed_as_their_own_commands(self):
        """Companion to the `source-file` test: neither hook is missing from BOTH
        places (baked into the sourced config nor issued separately)."""
        fake = _FakeTmux(exit_code=0)
        _launch(fake)
        write = [c for c in fake.calls if "set-hook" in c and "pane-died[1]" not in c]
        teardown = [c for c in fake.calls if "pane-died[1]" in c]
        self.assertEqual(len(write), 1, "the write hook was not installed exactly once")
        self.assertEqual(len(teardown), 1, "the teardown hook was not installed exactly once")
        # And NOT duplicated into the text file `source-file` loads.
        self.assertNotIn("pane-died", fake.sourced_conf_text or "")

    def test_the_exit_status_path_is_carried_before_the_write_hook_is_installed(self):
        """Ordering matters: the write hook's action reads `$CHARTER_FRAME_EXIT` back
        from the session's own environment, so `set-environment` must land before the
        hook that depends on it — otherwise the shell sees an unset variable the first
        time the hook could possibly fire."""
        fake = _FakeTmux(exit_code=0)
        _launch(fake)
        env_idx = next(i for i, c in enumerate(fake.calls) if "set-environment" in c)
        hook_idx = next(i for i, c in enumerate(fake.calls)
                       if "set-hook" in c and "pane-died[1]" not in c)
        self.assertLess(env_idx, hook_idx)

    def test_the_write_hook_is_installed_before_the_teardown_hook(self):
        """Load-bearing, not incidental — see `_pane_died_teardown_hook_argv`'s own
        docstring. Verified against tmux 3.7c: an UNINDEXED `set-hook pane-died ...`
        call does not merely overwrite index 0 of an existing hook array, it REPLACES
        THE WHOLE ARRAY — so installing the write hook (unindexed) AFTER the teardown
        hook (`pane-died[1]`) silently deletes the teardown hook the moment it lands,
        and the hang both hooks together exist to close comes back. Reproduced end to
        end by swapping the two `cmd_launch` calls: `RESULT: TIMEOUT`, session left
        attached. This test pins the ORDER `cmd_launch` issues the two commands in;
        `tests/test_frame_tmux_integration.py` pins the real-tmux array-replacement
        behaviour the order exists to work around."""
        fake = _FakeTmux(exit_code=0)
        _launch(fake)
        write_idx = next(i for i, c in enumerate(fake.calls)
                         if "set-hook" in c and "pane-died[1]" not in c)
        teardown_idx = next(i for i, c in enumerate(fake.calls) if "pane-died[1]" in c)
        self.assertLess(write_idx, teardown_idx,
                        "the write hook must be installed BEFORE the teardown hook, or "
                        "installing teardown wipes it out — see the docstring above")

    def test_the_recorded_exit_code_wins_over_a_zero_from_attach(self):
        """The whole module's reason to exist: an attached `tmux attach` (like
        `new-session`) returns 0 for the CLIENT's own detach, not for what ran inside —
        so a real crash must still surface even though `attach`'s own returncode is 0."""
        fake = _FakeTmux(exit_code=17, attach_rc=0)
        rc = _launch(fake)
        self.assertEqual(rc, 17)

    def test_a_death_that_races_the_hooks_own_install_is_recovered(self):
        """Critical 1: `new-session` starts the harness immediately; the hooks that
        would record its exit code and end the session are not installed until a
        `set-hook` call some time later. A harness that dies inside that window is
        never caught by them — nothing was listening yet, and hooks do not fire
        retroactively.

        The check for this is EAGER — run immediately after both hooks are installed,
        before `attach` is ever called — not merely a fallback after `attach` returns.
        Verified by hand against a real tmux 3.7c: with nothing left to run
        `kill-session`, `remain-on-exit` legitimately keeps the session alive forever,
        so an `attach` reaching this state BLOCKS FOREVER rather than returning 0 early.
        A correct launcher must therefore finish the hooks' own job itself (record the
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

    def test_a_failed_teardown_after_an_early_death_is_reported(self):
        """`kill-session`, run directly after the eager check recovers an early death,
        was the module's own last unchecked tmux return code — correction 2 applies to
        it too."""
        fake = _FakeTmux(race_death_status=9, kill_rc=1)
        buf = []
        with mock.patch("charter.util.err", side_effect=lambda m: buf.append(m)):
            rc = _launch(fake)
        self.assertEqual(rc, 9)
        self.assertTrue(any("ending the frame" in m for m in buf), buf)

    def test_remain_on_exit_is_armed_before_the_pane_id_is_even_known(self):
        """The other half of Critical 1: the placeholder loaded via `new-session`'s own
        `-f` — written and on disk BEFORE that command even runs — must already carry
        `remain-on-exit on`, because a harness that dies in the opening milliseconds
        (a missing binary is the sharpest case) needs its pane to survive before this
        launcher has done anything else at all, config or hooks alike."""
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

    def test_remain_on_exit_is_armed_directly_when_a_server_is_already_running(self):
        """The placeholder's `-f` is silently ignored once a server is already up
        (measured; see the module docstring), regardless of who started it — an
        operator's own `tmux -L charter new-session` included. Confirmed by hand:
        without arming it directly in that case, a race-window death against such a
        server returns the WRONG code (1, not its own) deterministically, rather than
        hanging — still wrong, just a quieter failure than the hang."""
        fake = _FakeTmux(exit_code=0, pre_existing_sessions=frozenset({"someone-elses"}))
        rc = _launch(fake)
        self.assertEqual(rc, 0)
        self.assertTrue(any("remain-on-exit" in c for c in fake.calls),
                        "remain-on-exit was not armed directly ahead of a running server")

    def test_remain_on_exit_is_not_armed_a_second_way_on_a_fresh_server(self):
        """Companion: when nothing else is running, the placeholder `-f` is sufficient
        (it IS what starts the server), so the direct arm command should not run —
        pinning that the condition is actually checked, not that running it twice would
        itself be wrong."""
        fake = _FakeTmux(exit_code=0)  # pre_existing_sessions defaults to empty
        _launch(fake)
        self.assertFalse(any("remain-on-exit" in c for c in fake.calls))

    def test_a_still_live_session_after_attach_is_a_detach_not_a_silent_zero(self):
        """The spec's own words: "Detach is allowed and prints how to reattach...
        returning silently to a shell with it still running is not." A session
        `list-sessions` still reports after `attach` returns is this frame's own — the
        harness is still running, so this must not read as success by accident and must
        not stay silent about it."""
        fake = _FakeTmux(still_live=True)
        buf = []
        with mock.patch("charter.util.info", side_effect=lambda m: buf.append(m)):
            rc = _launch(fake)
        self.assertEqual(rc, 0)
        self.assertTrue(any("detach" in m.lower() and fake.fid in m for m in buf),
                        f"no reattach message was printed: {buf}")
        # The frame's own directory must not be reaped while the session is still live.
        self.assertTrue(state.frame_dir(fake.fid).exists())

    def test_refuses_to_attach_when_the_teardown_hook_fails_to_install(self):
        """Without the teardown hook, a crash ANY time later in the harness's life
        would leave `attach` blocked forever with nothing to end the session — the same
        hang the eager check exists to close, just moved later and made permanent for
        this frame. Refusing to attach is the safe choice; the harness keeps running,
        detached.

        The return code must be NONZERO, not the `0` a deliberate operator detach
        returns below: the harness never ran interactively and charter has no way to
        learn its real exit code, so a script or `&&` chain must see this launch as
        having failed, not quietly succeeded."""
        fake = _FakeTmux(teardown_hook_rc=1, still_live=True)
        buf = []
        with mock.patch("charter.util.err", side_effect=lambda m: buf.append(m)):
            rc = _launch(fake)
        self.assertNotEqual(rc, 0)
        self.assertTrue(any("refusing to attach" in m for m in buf),
                        f"no refusal message: {buf}")
        self.assertFalse(any("attach" in c for c in fake.calls),
                         "attach must never be reached once teardown cannot be trusted")
        self.assertTrue(state.frame_dir(fake.fid).exists())

    def test_the_harness_pane_is_selected_after_the_splits(self):
        """`split-window` makes the newly created pane the ACTIVE one by default, so
        after every slot has been drawn, the LAST panel drawn — not the harness — has
        focus, and an interactive harness never receives a keystroke without this
        (measured by hand: `%2 active=1, %0 active=0` after two splits)."""
        fake = _FakeTmux(exit_code=0)
        _launch(fake)
        select_cmd = next(c for c in fake.calls if "select-pane" in c)
        self.assertEqual(select_cmd[select_cmd.index("-t") + 1], fake.pane_id)

    def test_a_failed_pane_selection_is_reported_but_not_fatal(self):
        fake = _FakeTmux(exit_code=0, select_rc=1)
        buf = []
        with mock.patch("charter.util.err", side_effect=lambda m: buf.append(m)):
            rc = _launch(fake)
        self.assertEqual(rc, 0)
        self.assertTrue(any("focusing the harness pane" in m for m in buf), buf)
        self.assertTrue(any("attach" in c for c in fake.calls))

    def test_a_failed_write_hook_install_is_reported_but_not_fatal(self):
        """A harness pane already exists and is already running by the time the hook is
        installed — losing exit-code tracking for this one frame is a real degradation,
        but killing an already-live pane over it would be worse."""
        fake = _FakeTmux(write_hook_rc=1, exit_code=5)
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

    def test_a_resize_hook_is_installed_reasserting_every_drawn_panels_size(self):
        """Cross-task fix round, item 3: tmux's own layout engine redistributes EVERY
        pane proportionally on any resize, `-l size` notwithstanding — verified by hand
        against real tmux 3.7c (`commands_frame._resize_hook_argv`'s own docstring): a
        120x30 frame grown to 200x50 stretched two one-row panels to 8 and 7 rows.
        `cmd_launch` must install a `window-resized` hook re-asserting each DRAWN
        panel's fixed dimension, targeting the REAL pane id tmux reported for it — a
        slot name alone is not a valid `resize-pane` target."""
        fake = _FakeTmux(exit_code=0, panel_pane_ids={"top": "%11", "bottom": "%12"})
        rc = _launch(fake)
        self.assertEqual(rc, 0)
        resize_cmd = next(c for c in fake.calls if "window-resized" in c)
        self.assertEqual(resize_cmd[resize_cmd.index("-t") + 1], fake.pane_id,
                         "the hook must be scoped to the harness pane's own window")
        action = resize_cmd[-1]
        self.assertIn("%11", action)
        self.assertIn("%12", action)
        self.assertIn("-y 1", action)

    def test_no_resize_hook_is_installed_when_no_panel_pane_id_was_learned(self):
        """Companion: every OTHER test in this class leaves `split-window` reporting no
        pane id (the fake's own default). With nothing valid to target, installing a
        resize hook anyway would either fail outright or silently target nothing — this
        pins that `cmd_launch` does not even attempt it in that case."""
        fake = _FakeTmux(exit_code=0)
        _launch(fake)
        self.assertFalse(any("window-resized" in c for c in fake.calls))

    def test_a_pane_id_of_the_wrong_shape_is_never_interpolated_into_the_resize_hook(self):
        """Fix round 2, item 3: `_resize_hook_argv` interpolates the pane id directly
        into a hook ACTION STRING tmux later re-parses as a command line — the exact
        construction the module docstring's "constant string" section bans for
        `status_path`, for the same reason: something interpolated into an action must
        be safe BY CONSTRUCTION, not merely safe because tmux happens to always report
        `%<digits>` today. A value of any other shape must be treated the same as no id
        at all — that one slot gets no resize-hook entry, and every OTHER (validly
        shaped) slot still does."""
        fake = _FakeTmux(exit_code=0,
                         panel_pane_ids={"top": "not-a-pane-id", "bottom": "%12"})
        rc = _launch(fake)
        self.assertEqual(rc, 0)
        resize_cmd = next(c for c in fake.calls if "window-resized" in c)
        action = resize_cmd[-1]
        self.assertNotIn("not-a-pane-id", action)
        self.assertIn("%12", action)

    def test_a_failed_resize_hook_install_is_reported_but_not_fatal(self):
        """Same "report, don't kill an already-running pane" treatment every other
        cosmetic tmux command in this launcher gets (correction 2) — every pane already
        measures its own tty on every repaint regardless (the module docstring's "belt
        and braces" framing), so a launch's correctness never depended on this hook."""
        fake = _FakeTmux(exit_code=0, panel_pane_ids={"top": "%11", "bottom": "%12"},
                         resize_hook_rc=1)
        buf = []
        with mock.patch("charter.util.err", side_effect=lambda m: buf.append(m)):
            rc = _launch(fake)
        self.assertEqual(rc, 0)
        self.assertTrue(any("resize hook" in m for m in buf), buf)
        self.assertTrue(any("attach" in c for c in fake.calls))

    def test_a_failed_resize_hook_install_names_its_consequence(self):
        """Fix round 2, item 4: `_report_tmux_failure` prints the command and tmux's
        own stderr, but not what the failure COSTS the operator — every other degrade
        in this launcher (`source-file`, `set-environment`) pairs its failure report
        with a `util.warn` naming the consequence; the resize hook's own failure was
        missing that second half."""
        fake = _FakeTmux(exit_code=0, panel_pane_ids={"top": "%11", "bottom": "%12"},
                         resize_hook_rc=1)
        buf = []
        with mock.patch("charter.util.warn", side_effect=lambda m: buf.append(m)):
            rc = _launch(fake)
        self.assertEqual(rc, 0)
        self.assertTrue(any("continuing without it" in m and "drift" in m for m in buf),
                        buf)

    def test_an_invalid_option_resize_hook_failure_warns_rather_than_errors(self):
        """Fix round 3, item 2: `RESIZE_HOOK_FLOOR` is a fast path to skip a version
        already KNOWN too old, not the only thing standing between an operator and a
        LOUD, RECURRING error if that constant is ever wrong — safe by construction,
        not by the constant being right. A failed install whose stderr is tmux's own
        `invalid option: <name>` (confirmed by hand: generic `set-hook`
        argument-parsing text, not specific to this one hook) must degrade the SAME
        quiet way a known-too-old version already does: `util.warn`, never
        `util.err`/`_report_tmux_failure`'s command-and-stderr dump."""
        fake = _FakeTmux(exit_code=0, panel_pane_ids={"top": "%11", "bottom": "%12"},
                         resize_hook_rc=1,
                         resize_hook_stderr="invalid option: window-resized")
        warned, errored = [], []
        with mock.patch("charter.util.warn", side_effect=lambda m: warned.append(m)), \
             mock.patch("charter.util.err", side_effect=lambda m: errored.append(m)):
            rc = _launch(fake)
        self.assertEqual(rc, 0)
        self.assertEqual(errored, [], f"an unsupported hook name must never reach "
                                      f"util.err: {errored}")
        self.assertTrue(any("resize" in m and "drift" in m for m in warned), warned)

    def test_a_non_invalid_option_resize_hook_failure_still_errors(self):
        """Companion to the test above — the two paths must not collapse into each
        other. A resize-hook failure for some OTHER reason (a real bug, a permissions
        problem, anything that isn't tmux saying the hook name itself is unrecognised)
        must still get the loud, specific `_report_tmux_failure` treatment; degrading
        every failure to a quiet warning would hide an actual regression behind the
        same wording a known compatibility gap uses."""
        fake = _FakeTmux(exit_code=0, panel_pane_ids={"top": "%11", "bottom": "%12"},
                         resize_hook_rc=1,
                         resize_hook_stderr="no space for a new pane")
        warned, errored = [], []
        with mock.patch("charter.util.warn", side_effect=lambda m: warned.append(m)), \
             mock.patch("charter.util.err", side_effect=lambda m: errored.append(m)):
            rc = _launch(fake)
        self.assertEqual(rc, 0)
        self.assertTrue(any("resize hook" in m for m in errored), errored)
        self.assertTrue(any("continuing without it" in m and "drift" in m
                            for m in warned), warned)

    def test_the_resize_hook_is_skipped_quietly_below_its_own_version_floor(self):
        """Fix round 2, item 5: `window-resized` was added in tmux 3.3
        (`tmuxctl.RESIZE_HOOK_FLOOR`) — ABOVE `tmuxctl.FLOOR` (3.2), a version this
        launcher explicitly still allows to launch (degraded, not refused — see
        `test_below_the_tmux_floor_degrades_instead_of_refusing`). Below
        RESIZE_HOOK_FLOOR the hook must never even be ATTEMPTED — confirmed by hand
        that installing it on an unrecognised hook name fails with `invalid option:
        window-resized` — one quiet note instead, naming the real version gap."""
        fake = _FakeTmux(exit_code=0, panel_pane_ids={"top": "%11", "bottom": "%12"})
        buf = []
        with mock.patch("charter.util.warn", side_effect=lambda m: buf.append(m)):
            rc = _launch(fake, version=(3, 2))
        self.assertEqual(rc, 0)
        self.assertFalse(any("window-resized" in c for c in fake.calls),
                         "the hook must not even be attempted below its own floor")
        self.assertTrue(any("3.2" in m and "resize" in m for m in buf), buf)

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

    def test_columns_and_lines_are_stripped_from_the_environment_handed_to_tmux(self):
        """Belt and braces alongside every pane measuring its own tty (`frame/slots.py`,
        `frame/panel.py`): `env` here is inherited WHOLE by every process tmux starts on
        this launch, and `$COLUMNS`/`$LINES` describe the LAUNCHING terminal, not any
        pane this frame creates — verified against a real panel pane by hand (`ps -E`
        showed both inherited whole before this fix)."""
        fake = _FakeTmux(exit_code=0)
        with mock.patch.dict(os.environ, {"COLUMNS": "500", "LINES": "70"}):
            rc = _launch(fake)
        self.assertEqual(rc, 0)
        self.assertIsNotNone(fake.new_session_env)
        self.assertNotIn("COLUMNS", fake.new_session_env)
        self.assertNotIn("LINES", fake.new_session_env)

    def test_a_configured_slot_with_no_renderer_is_skipped_not_spawned_dead(self):
        """`[frame] slots` accepts `left`/`right` (`instance.FRAME_SLOTS`, sized by
        `layout.SLOT_SIZE`) even though `frame.slots.SLOTS` — the renderer registry —
        has no renderer for either yet. Spawning a real pane for one anyway would leave
        the operator a permanently dead, wrapped-error pane under `remain-on-exit on`
        (`panel.run` correctly refuses it, exit 2, but nothing then explains why at the
        point the frame actually comes up). This pins that no such pane is even
        attempted, one warning names it, and the implemented slots still draw."""
        fake = _FakeTmux(exit_code=0)
        buf = []
        with mock.patch.dict(config.FRAME, {"slots": ["top", "bottom", "left"]}), \
             mock.patch("charter.util.warn", side_effect=lambda m: buf.append(m)):
            rc = _launch(fake)
        self.assertEqual(rc, 0)
        self.assertFalse(any("panel" in c and "left" in c for c in fake.calls),
                         "no pane may be spawned for a slot with no renderer")
        self.assertTrue(any("left" in m for m in buf), buf)
        self.assertTrue(any("panel" in c and "bottom" in c for c in fake.calls),
                        "an IMPLEMENTED slot must still be drawn")

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
        # `run` is reached at most for the pre-create `reap()`/arm pass (see the
        # reap-ordering test below) — but never for anything that needs a working state
        # directory.
        self.assertFalse(any("new-session" in c.args[0] for c in run.call_args_list))

    def test_reap_runs_before_this_frames_own_directory_is_created(self):
        """`state.reap` compares live tmux sessions against directories already on
        disk. Creating THIS frame's own directory first — before its tmux session
        exists — would make it look exactly like an abandoned one, and the very next
        `reap()` call would delete a directory this launch had not even started using
        yet."""
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
    it with `if False:`, left the full suite green before this class existed). These
    test the DECISION, not what `bypass()` does once reached."""

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
        """The loop that registers every harness runs BEFORE `sub.add_parser
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

    def test_a_harness_named_panel_is_refused_too(self):
        """`charter/frame/panel.py` (Task 7) is internal, spawned only by
        `layout.panel_argvs` — never typed by an operator — but it is registered the
        same way `frame` is: AFTER the harness loop, in `_add_frame_parsers`. A harness
        claiming `cli_name == "panel"` would otherwise pass `sub.choices` (nothing is
        named `panel` yet when the loop checks) and only collide once `charter/cli.py`'s
        own `sub.add_parser("panel")` call runs — where a version-3.11 argparse would
        silently let the harness shadow it, and every panel pane would then fail at
        startup against a parser that no longer expects `--session`."""
        from charter import cli
        from charter.harness.base import Harness

        class _PanelHarness(Harness):
            name = "panel-harness"
            cli_name = "panel"
            binary = "panel-harness"

        with mock.patch.dict("charter.harness.registry.KINDS",
                             {"panel-harness": _PanelHarness}, clear=True):
            with self.assertRaises(ValueError) as ctx:
                cli.build_parser()
        self.assertIn("panel-harness", str(ctx.exception))


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


class MainDeliversFrameRest(unittest.TestCase):
    """The DELIVERY half of Critical 2. `FrameArgvSplit` above tests `_split_frame_argv`
    in isolation, re-implementing in its own `_parse` helper the graft `cli.main` itself
    performs (`args.rest = frame_rest`) — a test that only exercises the helper cannot
    catch that graft line being deleted from `main`. Confirmed: deleting `args.rest =
    frame_rest` from `main()` left the full suite green, while `charter claude -p hi`
    silently ran with `rest=[]`, dropping `-p hi` entirely."""

    def test_charter_claude_dash_p_hi_reaches_bypass_via_main(self):
        from charter import cli
        with mock.patch("charter.commands_frame.bypass", return_value=0) as byp:
            rc = cli.main(["claude", "--no-frame", "-p", "hi"])
        byp.assert_called_once_with(["claude", "-p", "hi"])
        self.assertEqual(rc, 0)


if __name__ == "__main__":
    unittest.main()
