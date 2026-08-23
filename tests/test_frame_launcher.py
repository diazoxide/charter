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

import contextlib
import os
import shutil
import subprocess
import sys
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from tests._isolation import PersonaIso
from charter import commands_frame, config, util
from charter.frame import gather, layout, menu, slots, state, tmuxctl

#: The plane this test PROCESS was started in, captured at IMPORT — before any `setUp`
#: has had a chance to repoint `config`, so it is unavoidably the developer's REAL
#: `.charter/`. Only ever compared against; never read from, never written to.
_REAL_STATE_DIR = Path(config.STATE_DIR)


def _refuse_the_real_plane() -> None:
    """Refuse to run the real `cmd_launch` against the developer's own control plane.

    Every tmux call in this module is faked, which made it easy to read the whole file
    as harmless — but `cmd_launch` is REAL, and its first act on either path is
    `state.reap`: an `rmtree` of every directory under `config.STATE_DIR/frame/` that
    the server does not report live. `_FakeTmux` reports none, and a frame directory
    with no `server` marker matches every server, so an unisolated call here deletes the
    state of whatever frames the developer has open. Measured, not supposed: three tests
    in this module were creating `frame/demo-<pid>` in the real `.charter/` on every
    suite run, which means they had already run that `rmtree` to get there.

    Called by the two helpers that reach `cmd_launch` rather than left to each class's
    base list, because the base list is exactly what a new class copies from its
    neighbour without re-deriving. A class that forgets `PersonaIso` fails here, loudly,
    before anything is deleted.
    """
    if Path(config.STATE_DIR) == _REAL_STATE_DIR:
        raise AssertionError(
            "this test is about to run the real `cmd_launch` — `state.reap` and all — "
            f"against the developer's own control plane ({_REAL_STATE_DIR}). The class "
            "must derive from `PersonaIso` (see `tests/_isolation.py`).")


def _harness_binary_installed(resolver=None):
    """Make "is the harness's binary installed?" deterministic for the framed path.

    `cmd_launch` refuses to reach tmux for a REGISTERED harness whose binary is not on
    `$PATH` (it hands off to `bypass`, which names it and exits 127). Every framed-path
    test below therefore has to answer that question itself: left to the real
    `shutil.which`, the entire `Launch` class would take the bypass branch on any machine
    without `claude` installed — and `bypass` calls `os.execvp`, so a suite run there
    would try to REPLACE THE TEST PROCESS rather than merely fail. Machine-dependent and
    destructive, which is a worse combination than either alone.

    `side_effect` over `return_value`: `shutil.which` is patched on the module object, so
    this is global for the duration — `tmuxctl._probe`'s own `which("tmux")` goes through
    it too — and answering every name with a plausible path keeps that honest rather than
    special-casing one string.

    *resolver* exists because the blanket "everything is installed" answer silently made a
    test vacuous the first time this helper was written: `_launch` applies this patch
    INSIDE its own `with`, so it overrides anything a caller set up outside, and
    `test_the_frame_escape_hatch_is_not_narrowed_by_that_check` — whose entire point is a
    binary that is NOT on `$PATH` — was answered "installed" along with everything else.
    Caught by the mutation that widens the guard to `argv[0]`, which stayed green. A
    caller that cares what `which` says must therefore say so.
    """
    return mock.patch("charter.commands_frame.shutil.which",
                      side_effect=resolver or (lambda name, *a, **k: f"/usr/bin/{name}"))


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


class MissingHarnessBinary(PersonaIso, unittest.TestCase):
    """`charter claude` before `claude` is installed — the most likely FIRST-RUN state
    of this whole feature.

    `PersonaIso` because one test below (`test_an_installed_harness_is_not_short_circuited`)
    runs a FULL `_launch`, and `cmd_launch` reads `config.STATE_DIR` — the developer's
    own `.charter/` without it. Writing there was the smaller half: `cmd_launch` also
    calls `state.reap(live_before)`, and `_FakeTmux` answers the pre-`new-session`
    `list-sessions` with an EMPTY set, so every framed test here ran `reap(set())`
    against the real plane and deleted the operator's own frame directories — the
    unread-`exit`-file state #383 exists to protect, on a machine that has live frames
    on it. Verified by watching a real `.charter/frame/` across a suite run.

    It stayed invisible because the litter cleaned itself up: `cmd_launch`'s closing
    `reap()` took this launch's own directory back out again. Since #383 a launcher no
    longer reaps its OWN directory (its pid is necessarily still alive), so the
    accident stopped covering for the leak and a `demo-<pid>` directory survived every
    suite run — which is how this was found.

    A base list is not where that guarantee is enforced, though: the next class copies
    one from its neighbour without re-deriving it. `_launch` calls
    `_refuse_the_real_plane()` so a class that forgets this base fails loudly, before
    anything is deleted.

    `bypass` called `os.execvp` raw, so `FileNotFoundError` reached `cli.main`'s
    `except Exception`, which files a charter crash report and re-raises a traceback.
    `cli.main` already carves out this exact class twice (`contain.Refused`,
    `util.ProcTimeout`), both noting that filing such a condition as a bug "sends
    whoever reads it looking in the wrong repository"."""

    def test_a_missing_binary_exits_127_and_names_what_is_missing(self):
        buf = []
        with mock.patch("os.execvp", side_effect=FileNotFoundError(2, "No such file")), \
             mock.patch("charter.util.err", side_effect=lambda m: buf.append(m)):
            rc = commands_frame.bypass(["claude", "-p", "hi"])
        self.assertEqual(rc, 127, "127 is the shell's own 'command not found', so "
                                  "`charter claude && ...` behaves like `claude && ...`")
        self.assertTrue(any("claude" in m for m in buf),
                        f"the message must name the binary charter could not find: {buf}")

    def test_a_non_executable_binary_exits_126_rather_than_crashing(self):
        buf = []
        with mock.patch("os.execvp", side_effect=PermissionError(13, "Permission denied")), \
             mock.patch("charter.util.err", side_effect=lambda m: buf.append(m)):
            rc = commands_frame.bypass(["claude"])
        self.assertEqual(rc, 126)
        self.assertTrue(any("claude" in m for m in buf), buf)

    def test_the_framed_path_refuses_before_tmux_and_says_why(self):
        """The defect the `bypass` fix alone did not reach, and the one that mattered
        most: `charter claude` in a TERMINAL — the normal path, the first thing a new
        operator types — printed nothing at all.

        `new-session` starts, the exec fails instantly, the eager
        `_query_pane_dead_status` catches the dead pane and runs `kill-session`, and from
        that moment `code is not None`, so the whole `if code is None:` block — panels,
        `select-pane`, and `attach` — is skipped. No pane, no attach, nothing drawn.
        Measured under a pty against a real tmux 3.7c with `claude` genuinely off
        `$PATH`: **zero bytes** of output, exit 127, no alternate-screen switch. The
        irony this test also pins: `--no-frame` and piped output, the two paths that
        skip the frame, printed the right thing all along.

        `run.assert_not_called()` is the half that says "before tmux": returning 127 by
        going all the way through `new-session` and reading a dead pane's status would
        satisfy the exit code while leaving the operator exactly as uninformed."""
        args = SimpleNamespace(harness="claude", rest=[], no_frame=False)
        buf = []
        with mock.patch("charter.commands_frame.shutil.which", return_value=None), \
             mock.patch("sys.stdout.isatty", return_value=True), \
             mock.patch("charter.frame.tmuxctl.version", return_value=(3, 7)), \
             mock.patch("os.execvp", side_effect=FileNotFoundError(2, "No such file")), \
             mock.patch("charter.commands_frame.subprocess.run") as run, \
             mock.patch("charter.util.err", side_effect=lambda m: buf.append(m)):
            rc = commands_frame.cmd_launch(args)
        self.assertEqual(rc, 127)
        self.assertTrue(any("claude" in m for m in buf),
                        f"the operator must be told what is missing: {buf}")
        run.assert_not_called()

    def test_the_frame_escape_hatch_is_not_narrowed_by_that_check(self):
        """`charter frame -- <cmd>` must behave exactly as it did. `argv[0]` there is the
        operator's own verbatim word and is allowed to be a shell builtin, a relative
        path, or anything else tmux's own resolution accepts — so the check is scoped to
        `if h`, a REGISTERED harness whose binary charter itself chose
        (`harness.base.binary`), and never to `argv[0]`.

        This is what fails if anyone "simplifies" the guard to `not
        shutil.which(argv[0])`: a command charter has never met, provably not on `$PATH`,
        must still reach `new-session`."""
        fake = _FakeTmux(exit_code=0)
        self.assertIsNone(shutil.which("charter-definitely-not-a-real-binary-xyz"))
        # NOTHING resolves on `$PATH` for the duration — the strongest form of the
        # question, and it has to be said explicitly: `_launch`'s default answers
        # "installed" for every name, which made the first version of this test vacuous
        # (see `_harness_binary_installed`).
        with mock.patch("os.execvp", side_effect=AssertionError("bypassed the frame")):
            rc = _launch(fake, harness="",
                         rest=["--", "charter-definitely-not-a-real-binary-xyz"],
                         which=lambda name, *a, **k: None)
        self.assertEqual(rc, 0)
        self.assertTrue(any("new-session" in c for c in fake.calls),
                        "the `frame --` escape hatch must still reach tmux for a command "
                        "that is not on $PATH")

    def test_an_installed_harness_is_not_short_circuited(self):
        """The other direction, and what stops the guard from being "always bypass": with
        the binary present the launch proceeds into tmux exactly as before."""
        fake = _FakeTmux(exit_code=0)
        with mock.patch("os.execvp", side_effect=AssertionError("bypassed the frame")):
            rc = _launch(fake, harness="claude")
        self.assertEqual(rc, 0)
        self.assertTrue(any("new-session" in c for c in fake.calls))

    def test_no_crash_report_is_filed_for_a_missing_binary(self):
        """The half that actually mattered: not merely "does not traceback", but "does
        not file a bug against charter". Driven through `cli.main`, because that is
        where `_record_crash` lives — a test calling `bypass` directly could never
        observe it."""
        from charter import cli
        with mock.patch("os.execvp", side_effect=FileNotFoundError(2, "No such file")), \
             mock.patch("sys.stdout.isatty", return_value=True), \
             mock.patch("charter.cli._record_crash") as crash, \
             mock.patch("charter.util.err"):
            rc = cli.main(["claude", "--no-frame"])
        self.assertEqual(rc, 127)
        crash.assert_not_called()


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

    def test_the_hotkey_opens_the_menu_via_frame_menu_not_frame_space_menu(self):
        """`charter frame menu` (a space) would never be reached: `cli._split_frame_argv`
        treats `argv[0] == "frame"` as the launcher's own escape hatch and grafts
        everything past it onto the harness's own verbatim argv before `argparse` ever
        sees a subcommand to route — the exact reason `charter panel` is already a
        top-level sibling of `frame` rather than nested under it. `charter frame-menu`
        (a different literal token) is what `cli.py` actually registers."""
        text = commands_frame.conf_text(hotkey="F2", mouse=False, history_limit=1,
                                        session="x")
        self.assertIn('bind -n F2 run-shell \'"$CHARTER_PY" -m charter '
                      'frame-menu "#{client_name}"\'', text)
        self.assertNotIn("frame menu", text)

    def test_the_hotkey_never_invokes_a_bare_charter_off_the_path(self):
        """The panels already launch `[sys.executable, "-m", "charter"]`; this bind kept
        a bare `charter`. With charter not on the tmux server's own `$PATH` — a `uv
        tool` shim, a checkout run as `python -m charter` — pressing the hotkey makes
        `run-shell` print `'charter frame-menu "/dev/ttys020"' returned 127` INTO THE
        HARNESS PANE and drop it into copy-mode: charter drawing in the one rectangle
        ADR 0018 says it never draws.

        Asserts the absence of the old shape, not merely the presence of the new one —
        a fix that added `$CHARTER_PY` while leaving a second bare invocation behind
        would satisfy an `assertIn` alone. `run-shell 'charter` is the exact byte
        sequence that puts `charter` in the shell's own command position."""
        text = commands_frame.conf_text(hotkey="F2", mouse=False, history_limit=1,
                                        session="x")
        self.assertNotIn("run-shell 'charter", text)
        self.assertIn('"$CHARTER_PY" -m charter frame-menu', text)

    def test_the_interpreter_is_carried_out_of_band_not_baked_into_the_bind(self):
        """The same reasoning `_exit_path_env_argv`'s own docstring records for
        `status_path`: an absolute path re-embedded inside this nested tmux-quote layer
        is one apostrophe away from silent corruption (verified against 3.7c that
        `set-hook` still returns 0 while the stored action is mangled). So the bind
        text must carry the VARIABLE, never the value."""
        text = commands_frame.conf_text(hotkey="F2", mouse=False, history_limit=1,
                                        session="x")
        self.assertNotIn(sys.executable, text)

    def test_the_hotkey_bind_is_global_not_session_scoped(self):
        """Key tables have no per-session form in tmux (unlike `status`/`mouse`/
        `history-limit` above) — this is `-n`, not `-t <session>`, on purpose; see the
        `conf_text` docstring for why sharing one bind text across every frame on
        `SOCKET` is still safe."""
        text = commands_frame.conf_text(hotkey="F2", mouse=False, history_limit=1,
                                        session="demo-42")
        self.assertNotIn("bind -t demo-42", text)
        self.assertIn("bind -n F2", text)

    def test_a_different_hotkey_is_honoured(self):
        text = commands_frame.conf_text(hotkey="M-m", mouse=False, history_limit=1,
                                        session="x")
        self.assertIn("bind -n M-m run-shell", text)


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

    def test_the_frame_id_is_carried_to_its_own_hotkey_menu(self):
        """Without this, `run-shell` fired from a LATER frame sharing `SOCKET` falls
        back to the SERVER's own starting environment — the FIRST frame's
        `CHARTER_SESSION_ID`, not this one's (verified by hand against tmux 3.7c: a
        second session on an already-running server, `run-shell`'d with no override of
        its own, reported the first frame's id). The value carried is the session's own
        name — `_session_id_env_argv` hands a session its own id back, unlike
        `_exit_path_env_argv`, which carries a SEPARATE value (`status_path`)."""
        cmd = commands_frame._session_id_env_argv(socket="charter", session="demo-1")
        self.assertEqual(cmd, ["tmux", "-L", "charter", "set-environment", "-t", "demo-1",
                              "CHARTER_SESSION_ID", "demo-1"])


class PanelRespawnHook(unittest.TestCase):
    """`pane-died`, scoped to a PANEL pane — the other half of #382.

    `remain-on-exit` keeps a dead panel visible with its error, which is the half that
    already worked. This is the half that was specced and never built: a panel that dies
    stays dead for the frame's whole life. `charter/frame/panel.py` now holds its pane
    open for every failure charter's own Python can SEE, so what is left for this hook
    is exactly the kind it cannot — the interpreter failing to start, a SIGKILL, an OOM.

    Every property below was verified by hand against a real tmux 3.7c before being
    written down here, because none of it can be from inside this suite (see the module
    docstring's "never start a real tmux server" rule):

    * a pane-scoped `pane-died` hook on a PANEL pane fires when that panel's process
      dies, and does NOT fire the harness pane's own hooks;
    * installing it leaves the harness pane's `pane-died[0]`/`[1]` array byte-identical
      (`show-hooks -p` before and after) — pane options are per pane, so the array
      replacement `_pane_died_teardown_hook_argv`'s docstring measures cannot reach
      across panes;
    * `run-shell -b` (backgrounded, unlike `conf_text`'s hotkey bind) prints NOTHING
      into the harness pane and does not drop it into copy-mode even when the command
      it runs exits non-zero — the exact failure `conf_text`'s docstring records for
      the un-backgrounded form;
    * the single-quoted inner command reaches `/bin/sh` with `$CHARTER_PY` unexpanded by
      tmux, so the SHELL expands it, and `$CHARTER_SESSION_ID` is in that shell's
      environment (`set-environment -t <session>`, already issued by `cmd_launch`);
    * the hook SURVIVES the `respawn-pane` it triggers, which is why an attempt count on
      disk is the only thing bounding the loop.
    """

    def test_it_is_scoped_to_the_panel_pane_never_the_harness_pane(self):
        """`-p -t <panel pane>`: an unscoped (or session-scoped) `pane-died` hook fires
        for ANY pane, so it would respawn a panel every time the HARNESS died — and,
        worse, would be a second writer of the same option array the exit-code hooks
        live in."""
        cmd = commands_frame._panel_died_hook_argv(socket="charter", panel_pane="%11",
                                                   slot="top")
        self.assertIn("-p", cmd)
        self.assertEqual(cmd[cmd.index("-t") + 1], "%11")
        self.assertEqual(cmd[cmd.index("set-hook") + 1], "-p")

    def test_the_action_names_the_slot_and_the_pane_it_must_bring_back(self):
        action = commands_frame._panel_died_hook_argv(socket="charter", panel_pane="%11",
                                                      slot="bottom")[-1]
        self.assertIn("frame-respawn", action)
        self.assertIn("bottom", action)
        self.assertIn("%11", action)

    def test_the_action_never_names_a_bare_charter_off_the_path(self):
        """The same trap `conf_text`'s own docstring measures for the hotkey bind: a
        bare `charter` resolves against the tmux SERVER's `$PATH`, which need not have
        charter on it at all. The interpreter is carried out of band in
        `$CHARTER_PY` (`_charter_py_env_argv`) and expanded by the shell this action
        spawns — single-quoted, so tmux's own parsing leaves the `$` alone (verified
        against 3.7c: `show-hooks` reads the action back with the dollar escaped and
        the spawned shell receives the expanded path)."""
        action = commands_frame._panel_died_hook_argv(socket="charter", panel_pane="%11",
                                                      slot="top")[-1]
        self.assertIn(f'"${tmuxctl.CHARTER_PY_ENV}" -m charter', action)

    def test_the_action_is_backgrounded_so_it_cannot_draw_in_the_harness_pane(self):
        """`run-shell -b`, not a bare `run-shell`. Un-backgrounded, tmux prints a
        non-zero command's own `'…' returned N` into the HARNESS pane and drops it into
        copy-mode — charter drawing in the one rectangle ADR 0018 says it never draws
        (`conf_text`'s docstring records that happening for real). Backgrounded, the
        same failing command produced no output there at all (measured against 3.7c).
        This also matters because the command deliberately SLEEPS for its backoff: a
        blocking `run-shell` would stall tmux's command queue for seconds."""
        action = commands_frame._panel_died_hook_argv(socket="charter", panel_pane="%11",
                                                      slot="top")[-1]
        self.assertTrue(action.startswith("run-shell -b "), action)

    def test_it_is_a_clean_argv_list_naming_charters_own_socket(self):
        cmd = commands_frame._panel_died_hook_argv(socket="charter", panel_pane="%11",
                                                   slot="top")
        self.assertTrue(all(isinstance(a, str) for a in cmd))
        self.assertEqual(cmd[:4], ["tmux", "-L", "charter", "set-hook"])


class _RespawnTmux:
    """A tmux that answers only what `cmd_respawn` asks: is the session still live, and
    did the pane come back."""

    def __init__(self, *, live=("f-1",), respawn_rc=0):
        self.live = list(live)
        self.respawn_rc = respawn_rc
        self.calls: list[list[str]] = []

    def __call__(self, cmd, **kwargs):
        self.calls.append(list(cmd))
        if "list-sessions" in cmd:
            return subprocess.CompletedProcess(cmd, 0, stdout="\n".join(self.live),
                                               stderr="")
        if "respawn-pane" in cmd:
            return subprocess.CompletedProcess(cmd, self.respawn_rc, stdout="",
                                               stderr="" if self.respawn_rc == 0
                                               else "no such pane")
        raise AssertionError(f"unexpected tmux command in test: {cmd}")

    @property
    def respawns(self):
        return [c for c in self.calls if "respawn-pane" in c]


def _respawn(fake, *, slot="bottom", pane="%11", fid="f-1", slept=None):
    args = SimpleNamespace(slot=slot, pane=pane)
    env = {"CHARTER_SESSION_ID": fid} if fid is not None else {}
    with mock.patch("charter.commands_frame.subprocess.run", side_effect=fake), \
         mock.patch.dict(os.environ, env, clear=True), \
         mock.patch("charter.commands_frame.time.sleep",
                    side_effect=(slept.append if slept is not None else lambda _d: None)):
        return commands_frame.cmd_respawn(args)


class Respawn(PersonaIso, unittest.TestCase):
    """`charter frame-respawn` — what the panel pane's `pane-died` hook actually runs.

    The spec's own sentence, in full: "A dead panel stays visible with its error,
    respawns with backoff, and gives up after 3 attempts. A panel must never be able to
    take the agent down with it." The first clause and the last already held; these are
    the middle two.
    """

    def setUp(self) -> None:
        super().setUp()
        # A live frame always has its own state directory: `cmd_launch` creates and
        # bumps it before the first pane is split. `state.respawn_attempt` deliberately
        # does not create one (it would resurrect a directory `reap` just deleted at
        # teardown), so a test standing in for a running frame has to stand it up.
        state.bump("f-1")

    def test_the_first_death_brings_the_panel_back_with_its_own_argv(self):
        """Not an approximation of the panel's command — the SAME list
        `layout.panel_command` gave `panel_argvs` at launch, so a respawned panel cannot
        drift from the one the launcher spawned."""
        fake = _RespawnTmux()
        rc = _respawn(fake)
        self.assertEqual(rc, 0)
        self.assertEqual(len(fake.respawns), 1, fake.calls)
        cmd = fake.respawns[0]
        self.assertEqual(cmd[cmd.index("-t") + 1], "%11")
        self.assertEqual(cmd[cmd.index("--") + 1:],
                         layout.panel_command(slot="bottom", session="f-1",
                                              charter_argv=[sys.executable, "-m", "charter"]))

    def test_it_gives_up_after_three_attempts_and_leaves_the_pane_dead(self):
        """The cap the spec names. Without it, a panel that dies instantly on every
        start respawns forever — the hook survives each respawn (measured against tmux
        3.7c), so nothing else would ever stop it. The fourth death must leave the pane
        exactly as tmux left it: dead, with its own `Pane is dead (status N)` still
        readable, which is the outcome the spec asks for rather than a failure."""
        fake = _RespawnTmux()
        for _ in range(commands_frame._RESPAWN_ATTEMPTS):
            _respawn(fake)
        self.assertEqual(len(fake.respawns), commands_frame._RESPAWN_ATTEMPTS)
        _respawn(fake)
        self.assertEqual(len(fake.respawns), commands_frame._RESPAWN_ATTEMPTS,
                         "a fourth death was respawned — the cap is not holding")

    def test_each_attempt_waits_longer_than_the_one_before(self):
        """"With backoff", and the reason is a real one rather than politeness: a panel
        that fails at startup fails in milliseconds, so three back-to-back respawns
        would all land inside the same broken condition and burn the whole budget before
        anything transient (a filesystem hiccup, a plane mid-upgrade) could clear."""
        slept = []
        fake = _RespawnTmux()
        for _ in range(commands_frame._RESPAWN_ATTEMPTS):
            _respawn(fake, slept=slept)
        self.assertEqual(len(slept), commands_frame._RESPAWN_ATTEMPTS)
        self.assertEqual(slept, sorted(slept))
        self.assertLess(slept[0], slept[-1], slept)

    def test_a_frame_whose_session_has_already_ended_is_not_respawned(self):
        """Every panel pane dies when the frame is torn down, so every panel's hook
        fires on the way out — the ORDINARY case, not an edge one. Respawning into a
        session that no longer exists would fail once per panel and report each failure
        as if something had gone wrong. Checked AFTER the backoff sleep, not before:
        that is the window in which the teardown actually completes."""
        fake = _RespawnTmux(live=())
        rc = _respawn(fake)
        self.assertEqual(rc, 0)
        self.assertEqual(fake.respawns, [], "a finished frame's panel was respawned")

    def test_a_slot_with_no_renderer_is_refused_rather_than_respawned(self):
        """Same rule `cmd_launch` already applies when it declines to split a pane for
        an unimplemented slot: bringing one back would recreate exactly the permanently
        dead pane that filter exists to prevent."""
        fake = _RespawnTmux()
        rc = _respawn(fake, slot="sideways")
        self.assertEqual(rc, 0)
        self.assertEqual(fake.respawns, [])

    def test_a_pane_id_of_the_wrong_shape_is_never_used_as_a_target(self):
        """The value arrives via a tmux hook action, which is text tmux re-parsed — the
        same reason `_PANE_ID_RE` guards `_resize_hook_argv`. A target that is not
        `%<digits>` is treated as no target at all."""
        fake = _RespawnTmux()
        rc = _respawn(fake, pane="-t;kill-server")
        self.assertEqual(rc, 0)
        self.assertEqual(fake.respawns, [])

    def test_no_frame_id_in_the_environment_is_a_quiet_no_op(self):
        """`$CHARTER_SESSION_ID` is how every `run-shell`-spawned charter command
        resolves its own frame (`cmd_menu` does the same). Arriving without one means
        this was not fired by a frame at all; there is nothing to respawn and nowhere to
        report it."""
        fake = _RespawnTmux()
        rc = _respawn(fake, fid=None)
        self.assertEqual(rc, 0)
        self.assertEqual(fake.respawns, [])

    def test_a_panel_of_an_already_reaped_frame_is_not_respawned(self):
        """The frame is over and its state is gone — `state.respawn_attempt` refuses to
        count for a directory `reap` removed, and refusing to count is refusing to
        respawn. Belt to the liveness check's braces: that check happens after the
        backoff, so a `reap` that lands in between still has to leave a panel unrespawned
        rather than counted from zero.

        The pid at the end of the id is load-bearing since #383: `reap` keeps a directory
        while the launcher named in it is still running, and this class's own `f-1`
        fixture ends in pid 1 — `launchd`/`init`, which never exits — so `reap(set())`
        would rightly keep it and the test would prove the opposite of its name. A pid
        that has genuinely exited is a fact about this machine rather than a guess about
        it (the same reason `test_frame_state._a_dead_pid` exists)."""
        dead = subprocess.Popen([sys.executable, "-c", "pass"])
        dead.wait()
        fid = f"f-{dead.pid}"
        state.bump(fid)
        fake = _RespawnTmux(live=(fid,))
        state.reap(set(), server=commands_frame.SOCKET)
        self.assertFalse(state.frame_dir(fid).exists(), "the fixture was not reaped")

        rc = _respawn(fake, fid=fid)
        self.assertEqual(rc, 0)
        self.assertEqual(fake.respawns, [])

    def test_a_count_that_cannot_be_recorded_gives_up_rather_than_looping(self):
        """`state.respawn_attempt` answers `None` when it cannot record the attempt at
        all. Treating that as "attempt 1" would respawn forever — the one outcome the
        cap exists to prevent — so it degrades the same way exceeding the cap does."""
        fake = _RespawnTmux()
        with mock.patch("charter.frame.state.respawn_attempt", return_value=None):
            rc = _respawn(fake)
        self.assertEqual(rc, 0)
        self.assertEqual(fake.respawns, [])


class MissingTmux(unittest.TestCase):
    def test_an_absent_tmux_names_the_remedy_and_does_not_start_a_frame(self):
        """Adapted from the task brief's own draft, which mocked `tmuxctl.available()`.
        Correction 5 requires `cmd_launch` to call `tmuxctl.version()` exactly ONCE and
        branch on it, so a test that patched `available()` would exercise the REAL `tmux
        -V` here (whatever happens to be installed on the machine running the suite)
        instead of the absent-tmux path it claims to test. Patching `version()` to return
        `None` (= "absent", per `tmuxctl.version`'s own docstring) is what actually
        simulates it. `available()` has since been deleted outright — it never had a
        production caller, which is exactly why the brief's draft could name it."""
        args = SimpleNamespace(harness="claude", rest=[], no_frame=False)
        with mock.patch("charter.frame.tmuxctl.version", return_value=None), \
             mock.patch("sys.stdout.isatty", return_value=True), \
             _harness_binary_installed(), \
             mock.patch("charter.commands_frame.subprocess.run") as run, \
             mock.patch("builtins.print") as p:
            rc = commands_frame.cmd_launch(args)
        self.assertNotEqual(rc, 0)
        printed = " ".join(str(c) for c in p.call_args_list)
        self.assertIn("--no-frame", printed)
        # "does not start a frame": no tmux command of any kind was even attempted.
        run.assert_not_called()


class Probe(unittest.TestCase):
    """`--probe` (on `cmd_launch`) and `charter frame-probe` (`cmd_probe`) share one
    read-only gate, `commands_frame.frame_ready` — mirroring `cmd_launch`'s OWN behaviour
    below `tmuxctl.FLOOR` (warn, still runs), not a stricter refusal, is the whole point:
    a probe that refused there would report a frame this same launcher goes on to draw.
    """

    def test_present_and_at_the_floor_exits_zero(self):
        with mock.patch("charter.frame.tmuxctl.version", return_value=(3, 7)), \
             mock.patch("charter.commands_frame.subprocess.run") as run, \
             mock.patch("builtins.print") as p:
            rc = commands_frame.cmd_probe()
        self.assertEqual(rc, 0)
        run.assert_not_called()
        printed = " ".join(str(c) for c in p.call_args_list)
        self.assertIn("3.7", printed)

    def test_absent_tmux_exits_nonzero_and_names_it(self):
        with mock.patch("charter.frame.tmuxctl.version", return_value=None), \
             mock.patch("charter.commands_frame.subprocess.run") as run, \
             mock.patch("builtins.print") as p:
            rc = commands_frame.cmd_probe()
        self.assertNotEqual(rc, 0)
        run.assert_not_called()
        printed = " ".join(str(c) for c in p.call_args_list)
        self.assertIn("tmux", printed)

    def test_below_the_floor_still_exits_zero(self):
        """The mutation this pins: a naive `--probe` written as "refuse below the floor"
        would flip this to non-zero — wrong, because `cmd_launch` itself still runs a
        frame there (only the hotkey menu is disabled). A probe stricter than the thing
        it is asked about lies about that thing."""
        with mock.patch("charter.frame.tmuxctl.version", return_value=(3, 0)), \
             mock.patch("charter.commands_frame.subprocess.run") as run, \
             mock.patch("builtins.print"):
            rc = commands_frame.cmd_probe()
        self.assertEqual(rc, 0)
        run.assert_not_called()

    def test_below_the_floor_says_what_is_at_risk(self):
        """Where the below-floor message went when it stopped printing at launch (see
        `Launch.test_below_the_tmux_floor_degrades_instead_of_refusing`). This is the
        surface an operator can actually read: nothing has switched their terminal to
        tmux's alternate screen by the time it prints."""
        with mock.patch("charter.frame.tmuxctl.version", return_value=(3, 0)), \
             mock.patch("charter.commands_frame.subprocess.run") as run, \
             mock.patch("builtins.print") as p:
            rc = commands_frame.cmd_probe()
        self.assertEqual(rc, 0)
        run.assert_not_called()
        printed = " ".join(str(c) for c in p.call_args_list)
        self.assertIn("3.0", printed)
        self.assertIn("3.2", printed)

    def test_a_slot_with_no_renderer_is_named_by_probe(self):
        """The second standing ceiling that moved off the launch path: `[frame] slots`
        accepts a slot charter sizes but `frame.slots.SLOTS` has no renderer for, so
        the harness pane silently keeps that space. Read from `config.FRAME` with
        nothing started — the probe's own read-only promise.

        `left`/`right` shipped renderers in Task 3 (#385) — both are removed from the
        registry here (restored after, via `mock.patch.dict`) to simulate the one
        still-standing case the same way, rather than asserting against a pair that
        no longer names it."""
        with mock.patch("charter.frame.tmuxctl.version", return_value=(3, 7)), \
             mock.patch.dict(config.FRAME, {"slots": ["top", "left", "right"]}), \
             mock.patch.dict(slots.SLOTS), \
             mock.patch("charter.commands_frame.subprocess.run") as run, \
             mock.patch("builtins.print") as p:
            del slots.SLOTS["left"]
            del slots.SLOTS["right"]
            rc = commands_frame.cmd_probe()
        self.assertEqual(rc, 0, "an unimplemented slot is a ceiling, not a failure")
        run.assert_not_called()
        printed = " ".join(str(c) for c in p.call_args_list)
        self.assertIn("left", printed)
        self.assertIn("right", printed)

    def test_an_ordinary_machine_gets_no_ceilings_at_all(self):
        """The other direction, and what stops the two tests above from passing against
        a probe that always warns: with a new-enough tmux and only implemented slots
        configured, `frame_ready` reports `ok` and names no ceiling."""
        with mock.patch("charter.frame.tmuxctl.version", return_value=(3, 7)), \
             mock.patch.dict(config.FRAME, {"slots": ["top", "bottom"]}):
            code, level, line = commands_frame.frame_ready()
        self.assertEqual((code, level), (0, "ok"))
        self.assertNotIn("\n", line)

    def test_cmd_launch_short_circuits_on_probe_before_touching_anything(self):
        """`args.probe` is checked before `harness.all()`, `workspace.resolve()`, or any
        `subprocess.run` — the read-only promise `charter/news.py` requires of a `check:`,
        proven here by making every one of those raise if reached at all."""
        args = SimpleNamespace(harness="claude", rest=["-p", "hi"], no_frame=False,
                               probe=True)
        with mock.patch("charter.frame.tmuxctl.version", return_value=(3, 7)), \
             mock.patch("charter.harness.all", side_effect=AssertionError("harness "
                        "resolved")), \
             mock.patch("charter.workspace.resolve",
                        side_effect=AssertionError("workspace touched")), \
             mock.patch("charter.commands_frame.subprocess.run") as run, \
             mock.patch("builtins.print"):
            rc = commands_frame.cmd_launch(args)
        self.assertEqual(rc, 0)
        run.assert_not_called()

    def test_a_missing_probe_attribute_means_launch_not_probe(self):
        """`getattr(args, "probe", False)`, not `args.probe`: every OTHER `cmd_launch`
        call in this file builds its own `args` with no `probe` field at all, and none of
        them means "probe" — a bare `AttributeError` here would crash every one of them.

        Distinguished from the probe path by what it REACHES, not by its return code —
        with tmux present, `--probe` returns before `workspace.resolve()`
        (`test_cmd_launch_short_circuits_on_probe_before_touching_anything` above); an
        `args` with no `probe` field at all must reach it, exactly like an ordinary
        launch always has.
        """
        args = SimpleNamespace(harness="claude", rest=[], no_frame=False)
        self.assertFalse(hasattr(args, "probe"))
        sentinel = AssertionError("workspace resolved — the launch path was reached")
        with mock.patch("charter.frame.tmuxctl.version", return_value=(3, 7)), \
             mock.patch("sys.stdout.isatty", return_value=True), \
             _harness_binary_installed(), \
             mock.patch("charter.workspace.resolve", side_effect=sentinel):
            with self.assertRaises(AssertionError) as ctx:
                commands_frame.cmd_launch(args)
        self.assertIs(ctx.exception, sentinel)


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


class PaneState(unittest.TestCase):
    """The three answers `_pane_state` has to tell apart, one of which is not obvious.

    `_query_pane_dead_status` (above) folds two of them into `None` and is tested on its
    own terms; the wait loop inside an operator's tmux cannot, because "alive, ask again"
    and "gone, stop asking" are opposite instructions and confusing them is a spin that
    never ends.
    """

    def _dm(self, stdout, rc=0):
        return mock.patch(
            "charter.commands_frame.subprocess.run",
            return_value=subprocess.CompletedProcess([], rc, stdout=stdout, stderr=""))

    def test_a_running_pane_is_alive(self):
        with self._dm("0:"):
            self.assertEqual(commands_frame._pane_state("charter", "%0"),
                             (commands_frame._ALIVE, None))

    def test_a_dead_pane_carries_its_status(self):
        with self._dm("1:42"):
            self.assertEqual(commands_frame._pane_state("charter", "%0"),
                             (commands_frame._DEAD, 42))

    def test_a_pane_that_no_longer_exists_is_gone_not_alive(self):
        """Measured against a real tmux 3.7c, and the shape a guard written from memory
        gets wrong: `display-message -p -t <a pane that is gone>` returns 0 and expands
        both variables to NOTHING — so what comes back is the format's own literal `:`,
        not an empty line. Read as "alive", `_wait_for_harness` polls a window the
        operator already closed for as long as their shell stays open.

        `tests/test_frame_tmux_integration.py` asks a real server the same question; this
        is what pins it on a machine with no tmux at all."""
        with self._dm(":"):
            self.assertEqual(commands_frame._pane_state("charter", "%0"),
                             (commands_frame._GONE, None))

    def test_an_answer_with_nothing_in_it_at_all_is_gone_too(self):
        """The whole-line case, for a format that ever carries no literal of its own."""
        with self._dm(""):
            self.assertEqual(commands_frame._pane_state("charter", "%0"),
                             (commands_frame._GONE, None))

    def test_a_query_that_fails_is_gone_rather_than_alive(self):
        """A wedged or vanished server (`tmuxctl.run` folds all three of its failure
        shapes into a return code) means the same thing to a waiter: there is nothing
        left here to wait for. Stdout is made to LOOK alive so the returncode check is
        what this depends on."""
        with self._dm("0:", rc=1):
            self.assertEqual(commands_frame._pane_state("charter", "%0"),
                             (commands_frame._GONE, None))


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
                panel_pane_ids=None, pane_capture="",
                session_rc=0, source_rc=0, env_set_rc=0, write_hook_rc=0,
                teardown_hook_rc=0, panel_rc=0, select_rc=0, attach_rc=0, dm_rc=0,
                kill_rc=0, arm_rc=0, resize_hook_rc=0, capture_rc=0,
                respawn_hook_rc=0,
                resize_hook_stderr="bad resize hook target"):
        self.pane_id = pane_id
        self.exit_code = exit_code
        self.race_death_status = race_death_status
        # What `capture-pane` reports the dead pane still had in it — the ONLY place a
        # command that died before `attach` ever got the chance to say anything (#384).
        self.pane_capture = pane_capture
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
        self.capture_rc = capture_rc
        self.respawn_hook_rc = respawn_hook_rc
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
            if any("frame-respawn" in a for a in cmd):
                # A PANEL pane's own respawn hook (#382) — a different pane's option
                # array from the harness pane's two exit-code hooks, and given its own
                # knob here so a test can fail it without touching those.
                return subprocess.CompletedProcess(cmd, self.respawn_hook_rc, stdout="",
                                                   stderr="" if self.respawn_hook_rc == 0 else "bad panel target")
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
        if "capture-pane" in cmd:
            return subprocess.CompletedProcess(cmd, self.capture_rc,
                                               stdout=self.pane_capture if self.capture_rc == 0 else "",
                                               stderr="" if self.capture_rc == 0 else "no such pane")
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


def _outside_tmux():
    """Say, explicitly, that this launch is NOT happening inside somebody's tmux.

    Not decoration. `cmd_launch` reads `$TMUX` to decide whether to start its own
    private server or open a window in one that already exists, and THIS SUITE IS
    ORDINARILY RUN INSIDE A CHARTER FRAME — where `$TMUX` is set by tmux itself. Left to
    the ambient environment, every test in `Launch` below would take the inside-tmux
    branch on a developer's machine and the private-server branch in CI, which is the
    same class of never-fails test `test_the_harness_name_reaches_the_harness_environment`
    documents for `$CHARTER_HARNESS`. `TMUX_PANE` goes with it for the same reason.
    """
    stripped = {k: v for k, v in os.environ.items() if k not in ("TMUX", "TMUX_PANE")}
    return mock.patch.dict(os.environ, stripped, clear=True)


def _launch(fake: _FakeTmux, *, cols=200, rows=50, version=(3, 7), harness="claude",
           rest=(), which=None):
    _refuse_the_real_plane()
    args = SimpleNamespace(harness=harness, rest=list(rest), no_frame=False)
    with _outside_tmux(), \
         mock.patch("charter.commands_frame.subprocess.run", side_effect=fake), \
         mock.patch("sys.stdout.isatty", return_value=True), \
         _harness_binary_installed(which), \
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
        time the hook could possibly fire.

        Filtered on `CHARTER_FRAME_EXIT`, and that filter is the test. `cmd_launch`
        issues THREE `set-environment` calls now (the exit path, the frame id, the
        interpreter), and an unfiltered `next(... if "set-environment" in c)` was
        satisfied by whichever landed first — so deleting the `_exit_path_env_argv`
        block outright left this assertion, and the whole suite, green. No test
        anywhere asserted the launcher issued the exit-path call at all. Copies the
        shape `test_the_frame_id_is_carried_to_its_own_menu_before_the_write_hook`
        below already had right: filter, assert exactly one, assert the VALUE."""
        fake = _FakeTmux(exit_code=0)
        _launch(fake)
        exit_calls = [c for c in fake.calls
                      if "set-environment" in c and "CHARTER_FRAME_EXIT" in c]
        self.assertEqual(len(exit_calls), 1,
                         "the exit-status path must be carried out of band exactly "
                         f"once: {fake.calls}")
        cmd = exit_calls[0]
        self.assertEqual(cmd[cmd.index("-t") + 1], fake.fid)
        self.assertEqual(cmd[-1], str(state.frame_dir(fake.fid) / "exit"),
                         "the path carried must be THIS frame's own `exit` file — the "
                         "one `state.exit_code` reads back")
        hook_idx = next(i for i, c in enumerate(fake.calls)
                       if "set-hook" in c and "pane-died[1]" not in c)
        self.assertLess(fake.calls.index(cmd), hook_idx)

    def test_the_frame_id_is_carried_to_its_own_menu_before_the_write_hook(self):
        """Companion to the test above: `charter frame-menu`/`charter frame-action`
        (fired later, from a live keypress) both resolve `$CHARTER_SESSION_ID` from a
        `run-shell` environment the same way the write hook resolves
        `$CHARTER_FRAME_EXIT` — so this `set-environment` call needs to exist by the
        time ANYTHING could plausibly fire, same reasoning, different variable."""
        fake = _FakeTmux(exit_code=0)
        _launch(fake)
        sid_calls = [c for c in fake.calls if "set-environment" in c and "CHARTER_SESSION_ID" in c]
        self.assertEqual(len(sid_calls), 1,
                         "the frame's own id must be carried out of band exactly once")
        self.assertEqual(sid_calls[0][sid_calls[0].index("-t") + 1], fake.fid)
        self.assertEqual(sid_calls[0][-1], fake.fid)
        sid_idx = fake.calls.index(sid_calls[0])
        hook_idx = next(i for i, c in enumerate(fake.calls)
                       if "set-hook" in c and "pane-died[1]" not in c)
        self.assertLess(sid_idx, hook_idx)

    def test_charters_own_interpreter_is_carried_to_the_hotkey_menu(self):
        """The delivery half of the bare-`charter` fix (`Conf` above pins the bind text
        that READS it). Without this call the bind expands `$CHARTER_PY` to nothing and
        `run-shell` runs ` -m charter frame-menu ...`, whose failure tmux prints into
        the harness pane. Filtered and counted the way the `CHARTER_SESSION_ID` test
        below is — an unfiltered "some set-environment happened" assertion is exactly
        what let the exit-path call be deleted with the suite green."""
        fake = _FakeTmux(exit_code=0)
        _launch(fake)
        py_calls = [c for c in fake.calls
                    if "set-environment" in c and "CHARTER_PY" in c]
        self.assertEqual(len(py_calls), 1,
                         f"the interpreter must be carried exactly once: {fake.calls}")
        cmd = py_calls[0]
        self.assertEqual(cmd[cmd.index("-t") + 1], fake.fid,
                         "session-scoped: two planes on one laptop can be two different "
                         "charter installs, so a `-g` write would hand frame N's "
                         "interpreter to frame N-1")
        self.assertEqual(cmd[-1], sys.executable)

    def test_pythonsafepath_is_carried_to_the_hotkey_menu_too(self):
        """#390: `"$CHARTER_PY" -m charter ...` (the bind `Conf` pins, and every menu
        item's own action — `frame.menu.menu_argv`) is a shell TEMPLATE shared by every
        session on `SOCKET`, so there is nowhere in it to put a per-invocation `-P` the
        way the panel argv gets one (see `test_panels_are_launched_via_self_relaunch_argv`
        below) without re-embedding per-machine text `conf_text`'s own docstring already
        bans. `PYTHONSAFEPATH=1`, carried the same session-scoped way as `CHARTER_PY`
        itself, is `-P`'s equivalent for exactly this case: an interpreter env var every
        `"$CHARTER_PY" -m charter` the hotkey or a menu item runs inherits, immune to
        whatever `charter/` package happens to sit under the pane's own cwd."""
        fake = _FakeTmux(exit_code=0)
        _launch(fake)
        safepath_calls = [c for c in fake.calls
                          if "set-environment" in c and "PYTHONSAFEPATH" in c]
        self.assertEqual(len(safepath_calls), 1,
                         f"PYTHONSAFEPATH must be carried exactly once: {fake.calls}")
        cmd = safepath_calls[0]
        self.assertEqual(cmd[cmd.index("-t") + 1], fake.fid,
                         "session-scoped, same reasoning as CHARTER_PY: two planes on "
                         "one laptop can be two different charter installs")
        self.assertEqual(cmd[-1], "1")

    def test_panels_are_launched_via_self_relaunch_argv(self):
        """#390's visible failure: the panel argv (`layout.panel_argvs`'s `charter_argv`)
        used to be a hand-built `[sys.executable, "-m", "charter"]` with no `-P`. Spawned
        with the pane's cwd set to wherever the operator launched from, a charter checkout
        cwd made the child import THAT tree instead of the installed one — on a tree
        without a `panel` command, argparse exits 2 before charter ever runs, and both
        panels came up dead. `tests/test_self_relaunch_shadowing.py` proves the mechanism
        against a real decoy package; this pins that the production call site actually
        uses it."""
        fake = _FakeTmux(exit_code=0)
        _launch(fake)
        panel_calls = [c for c in fake.calls if "split-window" in c]
        self.assertTrue(panel_calls, "no panel was drawn — this test would be vacuous")
        for c in panel_calls:
            dd = c.index("--")
            self.assertEqual(c[dd + 1:dd + 5], util.self_relaunch_argv(),
                             f"panel argv was not built via self_relaunch_argv: {c}")

    def test_the_harness_name_reaches_the_harness_environment(self):
        """`harness.current()` reads `$CHARTER_HARNESS` FIRST, before any native
        detection — so this export decides what every hook running inside the frame
        thinks it is running in. Nothing asserted it: removing the two lines that set
        it survived the entire suite.

        The ambient value is deliberately overwritten with a sentinel first, and that is
        the whole test. `cmd_launch` builds its environment as `dict(os.environ, ...)`,
        and this suite is frequently RUN inside a charter frame, where the real
        `$CHARTER_HARNESS` is already `claude-code` — so a straight assertion on the
        expected value passes whether or not the launcher exports anything at all
        (confirmed: the mutation that deletes the export left the first version of this
        test green). Starting from a wrong value means only an actual export can make
        this pass.

        `claude` (`cli_name`, what a hand types) and `claude-code` (`name`, the harness's
        own identity) are also different strings on purpose, so a launcher exporting the
        wrong one of the two fails rather than passes."""
        fake = _FakeTmux(exit_code=0)
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "stale-sentinel"}):
            _launch(fake, harness="claude")
        self.assertIsNotNone(fake.new_session_env)
        self.assertEqual(fake.new_session_env.get("CHARTER_HARNESS"), "claude-code")

    def test_a_bare_frame_command_leaves_the_harness_name_alone(self):
        """`charter frame -- <cmd>` runs something charter has never met, so there is no
        harness identity for the launcher to claim — it must not invent one. Same
        sentinel trick, read the other way: whatever the launching environment already
        said is what the harness inherits, and nothing charter made up."""
        fake = _FakeTmux(exit_code=0)
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "stale-sentinel"}):
            _launch(fake, harness="", rest=["--", "echo", "hi"])
        self.assertIsNotNone(fake.new_session_env)
        self.assertEqual(fake.new_session_env.get("CHARTER_HARNESS"), "stale-sentinel",
                         "a bare `charter frame --` must not claim a harness identity")

    def test_the_menu_is_populated_with_a_detach_action(self):
        """`cmd_launch` must not merely bind the hotkey — the menu it opens has to have
        something real in it, or the mechanism this whole task exists to wire up is
        reachable but empty. "Detach" is the spec's own words ("Detach is allowed and
        prints how to reattach").

        `still_live=True`, not `exit_code=0`: a launch that ends normally has its own
        frame directory reaped by `cmd_launch` itself before returning (same reasoning
        `test_a_still_live_session_after_attach_is_a_detach_not_a_silent_zero` already
        documents for `state.frame_dir(...).exists()`) — this test needs the menu table
        to still be there to read back, so it has to look at a still-running frame."""
        fake = _FakeTmux(still_live=True)
        _launch(fake)
        entries = menu.build(fake.fid)
        self.assertEqual(len(entries), 1)
        label, action_id = entries[0]
        self.assertEqual(label, "Detach")
        argv = menu.resolve(fake.fid, action_id)
        self.assertEqual(argv, ["tmux", "-L", "charter", "detach-client", "-s", fake.fid])

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
        with _outside_tmux(), \
             mock.patch("charter.commands_frame.subprocess.run", side_effect=_peek), \
             mock.patch("sys.stdout.isatty", return_value=True), \
             _harness_binary_installed(), \
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
        # Weak evidence for the live-session rule specifically, and deliberately not
        # dressed up as more: since #383 this directory is kept by EITHER rule, because
        # `fid` ends in the launcher's pid and the launcher is this test process. That
        # is inherent — a launch's own id always names a live pid — so `Reap`'s fixtures
        # in tests/test_frame_state.py are where the live-session rule is pinned alone,
        # deliberately named after dead pids so nothing else can keep them.
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
        (measured by hand: `%2 active=1, %0 active=0` after two splits).

        The ORDER is the property, and it was the half nothing pinned: this test's own
        name and docstring were about ordering while it asserted only the target, so
        moving the `select-pane` call ABOVE the panel loop — which reintroduces exactly
        the "the harness never receives a keystroke" defect, since every later split
        steals the focus back — left the suite green. Compares indices, the way
        `test_the_write_hook_is_installed_before_the_teardown_hook` already does."""
        fake = _FakeTmux(exit_code=0)
        _launch(fake)
        select_idx = next(i for i, c in enumerate(fake.calls) if "select-pane" in c)
        select_cmd = fake.calls[select_idx]
        self.assertEqual(select_cmd[select_cmd.index("-t") + 1], fake.pane_id)

        split_idxs = [i for i, c in enumerate(fake.calls) if "split-window" in c]
        self.assertTrue(split_idxs, "no panel was drawn — this test would be vacuous")
        self.assertGreater(select_idx, max(split_idxs),
                           "the harness pane must be selected AFTER every split, or "
                           "the last panel drawn keeps the focus and the harness never "
                           "receives a keystroke")
        attach_idx = next(i for i, c in enumerate(fake.calls) if "attach" in c)
        self.assertLess(select_idx, attach_idx,
                        "selecting after `attach` returns is selecting after the "
                        "operator has already stopped using the frame")

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

    def test_every_drawn_panel_is_armed_for_its_own_respawn(self):
        """#382's second half at the launcher: a panel that dies must be brought back,
        and nothing can bring it back unless a hook was installed on ITS pane while it
        was alive. One hook per drawn panel, each targeting the pane tmux actually
        reported for that slot and naming that slot."""
        fake = _FakeTmux(exit_code=0, panel_pane_ids={"top": "%11", "bottom": "%12"})
        rc = _launch(fake)
        self.assertEqual(rc, 0)
        hooks = [c for c in fake.calls
                 if "set-hook" in c and any("frame-respawn" in a for a in c)]
        self.assertEqual(len(hooks), 2, fake.calls)
        by_pane = {c[c.index("-t") + 1]: c[-1] for c in hooks}
        self.assertIn("top", by_pane["%11"])
        self.assertIn("bottom", by_pane["%12"])

    def test_a_panel_whose_pane_id_was_never_learned_is_not_armed(self):
        """Same guard the resize hook already has, for the same reason: without a valid
        `%<digits>` there is no target, and a hook installed against a guessed one would
        either fail outright or arm the wrong pane. `top` reports a value of the wrong
        shape and `bottom` a real one — only `bottom` may be armed."""
        fake = _FakeTmux(exit_code=0,
                         panel_pane_ids={"top": "not-a-pane-id", "bottom": "%12"})
        rc = _launch(fake)
        self.assertEqual(rc, 0)
        hooks = [c for c in fake.calls
                 if "set-hook" in c and any("frame-respawn" in a for a in c)]
        self.assertEqual([c[c.index("-t") + 1] for c in hooks], ["%12"])

    def test_arming_the_panels_never_touches_the_harness_panes_own_hooks(self):
        """The regression this issue explicitly forbids. The harness pane's `pane-died`
        array holds the exit-code write hook at `[0]` and `kill-session` at `[1]`, and
        an unindexed `set-hook pane-died` REPLACES a whole array — so a third
        `pane-died` writer is exactly the shape that deletes teardown and brings back
        the hang. It is safe only because it targets a DIFFERENT PANE, which is a
        property of the argv and therefore testable here: no respawn hook may name the
        harness pane, and the harness pane's own two hooks must still be exactly two,
        write before teardown. (Verified against real tmux 3.7c as well: `show-hooks -p`
        on the harness pane reads back identically before and after a panel pane is
        armed, and a panel dying does not fire `kill-session`.)"""
        fake = _FakeTmux(exit_code=0, panel_pane_ids={"top": "%11", "bottom": "%12"})
        _launch(fake)
        harness_hooks = [c for c in fake.calls
                         if "set-hook" in c and "pane-died" in " ".join(c)
                         and c[c.index("-t") + 1] == fake.pane_id]
        self.assertEqual(len(harness_hooks), 2,
                         f"the harness pane's hook array gained a writer: {harness_hooks}")
        self.assertNotIn("pane-died[1]", harness_hooks[0])
        self.assertIn("pane-died[1]", harness_hooks[1])
        for c in fake.calls:
            if any("frame-respawn" in a for a in c):
                self.assertNotEqual(c[c.index("-t") + 1], fake.pane_id,
                                    "a respawn hook was installed on the harness pane")

    def test_a_failed_respawn_arming_is_reported_but_not_fatal(self):
        """Same treatment every other decorative tmux command in this launcher gets: a
        panel that cannot be armed for respawn is still a panel that came up, and the
        harness pane is already running."""
        fake = _FakeTmux(exit_code=0, panel_pane_ids={"top": "%11"}, respawn_hook_rc=1)
        buf = []
        with mock.patch("charter.util.err", side_effect=lambda m: buf.append(m)):
            rc = _launch(fake)
        self.assertEqual(rc, 0)
        self.assertTrue(any("respawn" in m for m in buf), buf)
        self.assertTrue(any("attach" in c for c in fake.calls))

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
        """Fix round 2, item 4: `tmuxctl.report_failure` prints the command and tmux's
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
        `util.err`/`tmuxctl.report_failure`'s command-and-stderr dump."""
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
        must still get the loud, specific `tmuxctl.report_failure` treatment; degrading
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
        """Correction 5: a version below `tmuxctl.FLOOR` still launches — the frame
        itself works there.

        And it launches SILENTLY. The below-floor message used to be a `util.warn` on
        this path; measured, `util.warn` lands 86 bytes before tmux's own
        `\x1b[?1049h`, so the operator's terminal switches to the alternate screen
        milliseconds later and the line is restored to view only when the frame EXITS.
        It is a standing capability ceiling — true on every launch on this machine —
        so it moved to the two surfaces built to answer on demand: `frame_ready`
        (`--probe`, `charter frame-probe`) and `doctor.check_frame`. See
        `Probe.test_below_the_floor_says_what_is_at_risk` for where it is asserted now."""
        fake = _FakeTmux(exit_code=0)
        buf = []
        with mock.patch("charter.util.warn", side_effect=lambda m: buf.append(m)):
            rc = _launch(fake, version=(3, 0))
        self.assertEqual(rc, 0)
        self.assertTrue(any("new-session" in c for c in fake.calls),
                        "below-floor must degrade, not refuse to launch")
        self.assertEqual(buf, [], "a standing ceiling must not be warned about into a "
                                 "terminal that is about to switch to tmux's alternate "
                                 "screen — see the docstring")

    def test_a_terminal_size_os_cannot_report_falls_back_rather_than_crashing(self):
        """`os.get_terminal_size()` raises `OSError` even on a tty that passes
        `isatty()` — a documented trap, not a hypothetical. Must not propagate."""
        fake = _FakeTmux(exit_code=0)
        args = SimpleNamespace(harness="claude", rest=[], no_frame=False)
        with _outside_tmux(), \
             mock.patch("charter.commands_frame.subprocess.run", side_effect=fake), \
             mock.patch("sys.stdout.isatty", return_value=True), \
             _harness_binary_installed(), \
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
        """`[frame] slots` accepts a slot (`instance.FRAME_SLOTS`, sized by
        `layout.SLOT_SIZE`) that `frame.slots.SLOTS` — the renderer registry — has no
        renderer for. Spawning a real pane for one anyway would leave the operator a
        permanently dead, wrapped-error pane under `remain-on-exit on` (`panel.run`
        correctly refuses it, exit 2, but nothing then explains why at the point the
        frame actually comes up). This pins that no such pane is even attempted, one
        warning names it, and the implemented slots still draw.

        `left` shipped a renderer in Task 3 (#385) — removed from the registry here
        (restored after, via `mock.patch.dict`) to keep simulating the one
        still-standing case, rather than asserting against a slot that now draws."""
        fake = _FakeTmux(exit_code=0)
        buf = []
        with mock.patch.dict(config.FRAME, {"slots": ["top", "bottom", "left"]}), \
             mock.patch.dict(slots.SLOTS), \
             mock.patch("charter.util.warn", side_effect=lambda m: buf.append(m)):
            del slots.SLOTS["left"]
            rc = _launch(fake)
        self.assertEqual(rc, 0)
        self.assertFalse(any("panel" in c and "left" in c for c in fake.calls),
                         "no pane may be spawned for a slot with no renderer")
        self.assertTrue(any("panel" in c and "bottom" in c for c in fake.calls),
                        "an IMPLEMENTED slot must still be drawn")
        # Silently: which slots have a renderer is a property of this BUILD, not of
        # this launch, and the warning it used to print landed 86 bytes before tmux
        # switched the terminal to the alternate screen. `--probe` and `charter doctor`
        # name it instead — see `Probe.test_a_slot_with_no_renderer_is_named_by_probe`.
        self.assertEqual(buf, [], f"nothing standing may be warned at launch: {buf}")

    def test_a_frame_dir_the_state_module_refuses_does_not_crash(self):
        """`frame_dir` returns `None` rather than a `Path` for an id it cannot shape
        into a directory (see charter/frame/state.py) — this launcher must treat that as
        data, not assume it always gets a `Path` back."""
        args = SimpleNamespace(harness="claude", rest=[], no_frame=False)
        with _outside_tmux(), \
             mock.patch("charter.commands_frame.subprocess.run") as run, \
             mock.patch("sys.stdout.isatty", return_value=True), \
             _harness_binary_installed(), \
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

        def _track_reap(live, **kw):
            order.append("reap")
            return real_reap(live, **kw)

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

    def test_a_launch_does_not_eat_a_sibling_frames_unread_exit_code(self):
        """#383, at the altitude where it costs something. Ordering the reap first (the
        test above) NARROWS the window in which a sibling's `exit` file can be deleted
        out from under its own launcher; it cannot close it, because the sibling's tmux
        session is genuinely gone by then and so is genuinely absent from `live`. What
        closes it is that the sibling's LAUNCHER is still running, and the frame id says
        so: `frame_id` puts that launcher's pid at the end of the name.

        The stand-in is a real child process, deliberately not this test process: this
        launch's own frame id is `frame_id("demo", os.getpid())`, so borrowing our pid
        would test the launcher not reaping ITSELF — a different property, and one that
        would still pass with the sibling case broken. The child sleeps far longer than
        the launch and is killed on cleanup."""
        child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(300)"])
        self.addCleanup(child.wait)
        self.addCleanup(child.kill)
        sibling = state.frame_id("other-workspace", child.pid)
        state.record_exit(sibling, 42)

        _launch(_FakeTmux(exit_code=0))

        self.assertEqual(state.exit_code(sibling), 42,
                         "a launch reaped a sibling frame whose launcher was still "
                         "alive — that launcher now returns a fabricated 0 for a "
                         "harness that exited 42")

    def test_a_launch_does_not_inherit_an_exit_code_from_an_earlier_life_of_its_pid(self):
        """The bill for #383's fix, paid here rather than left to be discovered. A frame
        id is `<workspace>-<launcher pid>` and pids are recycled — Linux wraps at
        `kernel.pid_max`, 32768 by default — so a launcher for the same workspace really
        does land on a pid an earlier launcher already used. `reap` now keeps a directory
        for as long as the pid in its name is live, and on THIS launch that pid is live
        because it is ours: the earlier frame's directory, `exit` file and all, is still
        there when this launch adopts the same id.

        Read back, that stale code becomes this launch's own return value. Asserted on
        the DETACH path, where nothing new is ever recorded and the stale file is
        therefore the only thing `state.exit_code` can find — a harness running perfectly
        well, detached, would be reported as having failed with a dead frame's number and
        the reattach line would never print."""
        stale = state.frame_id("demo", os.getpid())   # the id `_launch` is about to mint
        state.record_exit(stale, 99)

        fake = _FakeTmux(still_live=True)
        buf = []
        with mock.patch("charter.util.info", side_effect=lambda m: buf.append(m)):
            rc = _launch(fake)

        self.assertEqual(fake.fid, stale, "the fixture stopped colliding — proves nothing")
        self.assertEqual(rc, 0, "a previous frame's exit code was returned as this one's")
        self.assertTrue(any("detach" in m.lower() for m in buf),
                        f"no reattach message — the stale code suppressed it: {buf}")

    def test_a_launch_does_not_inherit_a_cached_scan_from_an_earlier_life_of_its_pid(self):
        """The second file the recycled directory carries, and the second reader that
        inherits it. `exit` is read once, by this launcher; `gather.json` is read by
        every PANEL, on every repaint, and `gather.read` has no freshness check by
        design — it is a panel's hot path, kept current by `notify.plane_changed`.

        So a launch adopting a dead frame's directory draws that frame's repos,
        branches and CI, and nothing corrects it: a panel repaints only on a
        `state.version` bump, so a scan from another day sits on screen until the
        session's first hook fires. Before #383 this was unreachable — `reap` had
        deleted the directory and `read` fell through to a live `scan` — and clearing
        the file on the launch path is what puts it back on exactly that path.

        `scan` is mocked to a sentinel, so this fails if the stale cache is served AND
        distinguishes that from "a live gather happened to agree"."""
        stale = state.frame_id("demo", os.getpid())   # the id `_launch` is about to mint
        gather.save(stale, {"gathered_at": 1.0, "workspace": "from-a-dead-frame",
                            "current_repo": None, "repos": [], "worktrees": []})

        fake = _FakeTmux(still_live=True)
        _launch(fake)

        self.assertEqual(fake.fid, stale, "the fixture stopped colliding — proves nothing")
        fresh = {"gathered_at": 0.0, "workspace": "sentinel", "current_repo": None,
                 "repos": [], "worktrees": []}
        with mock.patch.object(gather, "scan", return_value=fresh):
            self.assertEqual(gather.read(fake.fid), fresh,
                             "a panel of this frame would draw a dead frame's scan "
                             "until the session's first hook bump")

    def test_a_launch_does_not_inherit_a_spent_respawn_budget_from_its_pid(self):
        """The third thing the recycled directory carries (#382 meeting #383), and the
        one whose inheritance is not merely stale but already exhausted.

        `state.respawn_attempt` never resets — three deaths across a frame's whole life,
        deliberately — and every panel's `pane-died` hook fires during its own frame's
        TEARDOWN, so a directory left behind by a finished frame always has counts in it
        and may well be at `_RESPAWN_ATTEMPTS` already. Adopted by a new frame, those
        counts are charged to panels that have not died once: the first real death of
        this frame's `top` panel would be refused as attempt 4, and a panel that charter
        promises to bring back would simply stay dead, with nothing anywhere saying it
        was another frame's budget that ran out."""
        stale = state.frame_id("demo", os.getpid())   # the id `_launch` is about to mint
        state.bump(stale)
        for _ in range(commands_frame._RESPAWN_ATTEMPTS + 1):
            state.respawn_attempt(stale, "top")

        fake = _FakeTmux(still_live=True)
        _launch(fake)

        self.assertEqual(fake.fid, stale, "the fixture stopped colliding — proves nothing")
        self.assertEqual(state.respawn_attempt(fake.fid, "top"), 1,
                         "this frame's first panel death was charged to a dead frame's "
                         "budget and would never be respawned")


class EarlyDeathIsLegible(PersonaIso, unittest.TestCase):
    """#384: a command that dies before the frame is drawn must not die in silence.

    The exact shape of that silence, measured under a pty against a real tmux 3.7c:
    `new-session` starts the command, it dies instantly, the eager
    `_query_pane_dead_status` catches the dead pane, `kill-session` runs, and because
    `code is not None` from that moment the ENTIRE `if code is None:` block — panels,
    `select-pane`, `attach` — is skipped. There is no pane and no attach, so **zero
    bytes** reach the operator and the launch returns a number with no explanation.

    `MissingHarnessBinary` above closes this for a REGISTERED harness by refusing before
    tmux is ever reached — charter chose that binary's name, so `shutil.which` is asking
    about charter's own claim. The escape hatch cannot be closed that way, and that is
    the design call #384 left open: `charter frame -- <cmd>` runs whatever TMUX runs, and
    tmux's own rule (verified against 3.7c) is that ONE argument goes to a shell — so
    `charter frame -- 'ulimit -n; exit 3'` is a whole shell command line, builtin and
    all — while TWO OR MORE are `execvp`'d directly. A `shutil.which(argv[0])` pre-check
    answers neither question: it would refuse the shell form outright, and for the
    `execvp` form it would still be a prediction where an answer is available for free a
    few milliseconds later. So nothing is refused up front; the failure is reported
    afterwards, out of what tmux actually did.

    Charter has to say two different things because tmux leaves two different residues
    (both measured against 3.7c, both pinned by
    `tests/test_frame_tmux_integration.py::EarlyDeathIntegration`):

    * ONE argument, missing command — the shell runs, prints `command not found`, exits
      **127**. The accurate words already exist; they are just in a pane nobody ever
      attached to. Charter repeats them back.
    * TWO OR MORE arguments, missing command — `execvp` fails inside tmux's own child,
      which exits **1** with the pane completely EMPTY. Nothing to repeat, and a bare 1
      is indistinguishable from a program that ran and failed. Only there does charter
      answer the resolution question itself.
    """

    def test_a_command_that_dies_before_the_frame_is_drawn_says_so(self):
        """The headline defect. Nothing on `$PATH` resolves for the duration — the
        strongest form of the question and the one that pins the CONTRACT at the same
        time: the launch must still reach `new-session` (refusing here would narrow what
        the escape hatch accepts) and must still come back with charter's own account of
        what happened.

        Asserts charter's OWN framing — the phrase and the exit code — rather than the
        command's name, which also appears in the pane text
        `test_the_dead_panes_own_last_words_are_repeated_back` covers. Deleting only the
        `capture-pane` call leaves this test green and that one red, and vice versa;
        asserting on the name alone would have collapsed the two into one."""
        fake = _FakeTmux(race_death_status=127,
                         pane_capture="zsh:1: command not found: nosuchthing-xyz\n")
        buf = []
        with mock.patch("charter.util.err", side_effect=lambda m: buf.append(m)):
            rc = _launch(fake, harness="", rest=["--", "nosuchthing-xyz"],
                         which=lambda name, *a, **k: None)
        self.assertEqual(rc, 127)
        self.assertTrue(any("new-session" in c for c in fake.calls),
                        "the escape hatch must still run a command that is not on $PATH")
        self.assertTrue(any("charter frame:" in m and "127" in m for m in buf),
                        f"the operator was told nothing about a dead frame: {buf}")

    def test_the_dead_panes_own_last_words_are_repeated_back(self):
        """The half charter cannot write itself. A lone `nosuchthing-xyz` reaches a
        SHELL (tmux's one-argument rule), and the shell's own `command not found` names
        the word, the interpreter and the line — all of it accurate, all of it in a pane
        that is about to be killed without ever having been drawn. Reading it back out
        before `kill-session` is what turns tmux's silence into a report."""
        fake = _FakeTmux(race_death_status=127,
                         pane_capture="zsh:1: command not found: nosuchthing-xyz\n")
        buf = []
        with mock.patch("charter.util.err", side_effect=lambda m: buf.append(m)):
            _launch(fake, harness="", rest=["--", "nosuchthing-xyz"])
        self.assertTrue(any("command not found: nosuchthing-xyz" in m for m in buf),
                        f"the pane's own last words never reached the operator: {buf}")

    def test_the_pane_is_read_before_the_session_is_killed(self):
        """Ordering, and load-bearing rather than incidental — `kill-session` destroys
        the pane, so a capture issued after it can only ever come back empty. Pinned by
        comparing indices, the same way
        `test_the_write_hook_is_installed_before_the_teardown_hook` pins the other
        ordering this launcher depends on.

        `"set-hook" not in c` is not decoration: `_pane_died_teardown_hook_argv`'s ACTION
        is the literal string `kill-session`, so an unfiltered search finds the hook
        INSTALL — issued long before either the capture or the real teardown — and this
        test failed against a correct implementation until it stopped matching that."""
        fake = _FakeTmux(race_death_status=127, pane_capture="boom\n")
        with mock.patch("charter.util.err"):
            _launch(fake, harness="", rest=["--", "nosuchthing-xyz"])
        capture_idx = next(i for i, c in enumerate(fake.calls) if "capture-pane" in c)
        kill_idx = next(i for i, c in enumerate(fake.calls)
                        if "kill-session" in c and "set-hook" not in c)
        self.assertLess(capture_idx, kill_idx,
                        "the pane must be read BEFORE it is destroyed, or there is "
                        "nothing left to read")

    def test_a_clean_early_exit_is_not_reported_as_a_failure(self):
        """`charter frame -- true` finishes before the frame is drawn too, and reaches
        this exact path with status 0. Nothing is wrong, so nothing is said — and the
        pane is not even read: whatever a SUCCESSFUL command wrote was its stdout, and
        charter repeating it onto stderr would be inventing output on the wrong stream.
        The exit code is the whole message."""
        fake = _FakeTmux(race_death_status=0, pane_capture="hi\n")
        buf = []
        with mock.patch("charter.util.err", side_effect=lambda m: buf.append(m)):
            rc = _launch(fake, harness="", rest=["--", "true"])
        self.assertEqual(rc, 0)
        self.assertEqual(buf, [], f"a clean early exit must say nothing: {buf}")
        self.assertFalse(any("capture-pane" in c for c in fake.calls),
                         "a successful command's own output is not charter's to reprint")

    def test_a_launch_that_reaches_attach_never_reads_the_pane(self):
        """The ordinary path: the operator WAS attached and watched the harness live and
        die on their own screen. Reading the pane back there and printing it would
        replay the entire session onto stderr after the fact. The report belongs to the
        one path where nothing was ever shown."""
        fake = _FakeTmux(exit_code=3, pane_capture="a whole session\n")
        buf = []
        with mock.patch("charter.util.err", side_effect=lambda m: buf.append(m)):
            rc = _launch(fake)
        self.assertEqual(rc, 3)
        self.assertFalse(any("capture-pane" in c for c in fake.calls),
                         "a frame the operator actually saw must not be replayed at them")
        self.assertEqual(buf, [])

    def test_tmuxs_own_dead_pane_trailer_is_not_repeated_back(self):
        """`remain-on-exit` draws `Pane is dead (status 127, <date>)` INTO the pane —
        measured against tmux 3.7c, and it is the last line every capture of a dead pane
        comes back with. Echoing it would be charter reporting in tmux's words, at
        length, immediately after saying the same thing in its own; and the timestamp
        makes the line different on every single run. Filtered out — and the filter is
        safe by construction rather than by tmux's wording never changing: a trailer that
        stopped matching would be repeated back, which is merely noisy, never silent."""
        fake = _FakeTmux(
            race_death_status=127,
            pane_capture="zsh:1: command not found: nope\n\n"
                         "Pane is dead (status 127, Sun Aug 23 18:15:16 2026)\n")
        buf = []
        with mock.patch("charter.util.err", side_effect=lambda m: buf.append(m)):
            _launch(fake, harness="", rest=["--", "nope"])
        self.assertTrue(any("command not found: nope" in m for m in buf), buf)
        self.assertFalse(any("Pane is dead" in m for m in buf),
                         f"tmux's own trailer was repeated back at the operator: {buf}")

    def test_a_capture_that_fails_still_leaves_charters_own_account(self):
        """The capture is additive, never the report itself. A `capture-pane` that fails
        outright (the pane raced away, a wedged server) must not take the message down
        with it — that would put the launch straight back to the zero bytes #384 is
        about. Nor is the failed capture itself reported: the operator is already being
        told about a real failure, and a second line about the diagnostic charter tried
        to run buries it."""
        fake = _FakeTmux(race_death_status=127, capture_rc=1)
        errored, warned = [], []
        with mock.patch("charter.util.err", side_effect=lambda m: errored.append(m)), \
             mock.patch("charter.util.warn", side_effect=lambda m: warned.append(m)):
            rc = _launch(fake, harness="", rest=["--", "nosuchthing-xyz"])
        self.assertEqual(rc, 127)
        self.assertEqual(len(errored), 1,
                         f"exactly one account of the failure, not two: {errored}")
        self.assertIn("127", errored[0])
        self.assertEqual(warned, [])

    def test_a_multi_word_command_that_printed_nothing_is_diagnosed_against_the_path(self):
        """The case tmux leaves charter nothing to work with. Measured against 3.7c: two
        or more arguments are `execvp`'d directly, and a failed `execvp` exits tmux's own
        child with **1** and an EMPTY pane — no shell was ever involved, so nothing said
        `command not found`. A bare 1 is indistinguishable from a program that ran and
        failed, so this is the one place charter answers the resolution question itself.

        Sound, not a guess: with no shell in the way, a word that resolves to nothing
        provably could not have run."""
        with mock.patch("charter.commands_frame.shutil.which", return_value=None):
            msg = commands_frame.early_death_message(["nosuchthing-xyz", "--flag"], 1, [])
        self.assertIn("nosuchthing-xyz", msg)
        self.assertIn("$PATH", msg)

    def test_a_lone_word_is_never_diagnosed_against_the_path(self):
        """The contract, stated as a message rather than as a refusal — and the test
        that fails if anyone "simplifies" the diagnosis into a blanket check of
        `argv[0]`. tmux hands a SINGLE argument to a shell, so that argument is a shell
        command line: `ulimit` is a builtin no `$PATH` will ever hold, and the text here
        is not even one word. `shutil.which` has nothing to say about it, so charter
        does not pretend otherwise — the shell already spoke, and
        `test_the_dead_panes_own_last_words_are_repeated_back` is how that reaches the
        operator."""
        with mock.patch("charter.commands_frame.shutil.which", return_value=None):
            msg = commands_frame.early_death_message(["ulimit -n; exit 3"], 3, [])
        self.assertNotIn("$PATH", msg)
        self.assertIn("3", msg)

    def test_a_pane_that_spoke_for_itself_is_never_second_guessed(self):
        """The second half of the same rule: charter fills SILENCE, it does not argue
        with a pane that already said what went wrong. A command whose own words came
        back gets those words and no `$PATH` speculation layered on top — which would be
        wrong as often as not, since a program that printed and then failed plainly did
        run."""
        with mock.patch("charter.commands_frame.shutil.which", return_value=None):
            msg = commands_frame.early_death_message(
                ["nosuchthing-xyz", "--flag"], 1, ["config: no such profile 'x'"])
        self.assertIn("config: no such profile 'x'", msg)
        self.assertNotIn("$PATH", msg)

    def test_an_existing_but_unexecutable_path_is_told_apart_from_a_missing_one(self):
        """Three states, not two — "resolvable on `$PATH`", "a path that exists", and
        "neither" — because they need three different remedies and `execvp` fails
        identically for the last two (exit 1, empty pane). Telling an operator to check
        their `$PATH` for a script sitting right there, missing only `chmod +x`, sends
        them looking in the wrong place."""
        script = self.tmp / "not-executable.sh"
        script.write_text("#!/bin/sh\necho hi\n")  # deliberately never chmod +x'd
        with mock.patch("charter.commands_frame.shutil.which", return_value=None):
            msg = commands_frame.early_death_message([str(script), "--flag"], 1, [])
        self.assertIn("not executable", msg)
        self.assertNotIn("$PATH", msg,
                         "a file that is right there is not a $PATH problem")


class BypassRouting(PersonaIso, unittest.TestCase):
    """`Bypass.test_a_pipe_gets_no_frame` pins `bypass()`'s OWN argv shape but never
    calls `cmd_launch` at all — it cannot catch a `cmd_launch` that stopped routing to
    `bypass()` in the first place (confirmed: deleting the routing check, or replacing
    it with `if False:`, left the full suite green before this class existed). These
    test the DECISION, not what `bypass()` does once reached.

    `PersonaIso` for the same reason `MissingHarnessBinary` above carries it: the
    negative case here (`test_a_tty_without_no_frame_does_not_bypass`) proves the launch
    was NOT bypassed by letting a full `_launch` run — the whole real launcher,
    `state.reap` and all — which writes frame state under `config.STATE_DIR` and reaps
    everything beside it. That was the developer's real `.charter/` until this."""

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

    def test_a_harness_named_frame_menu_is_refused_too(self):
        """Same class of collision as `frame`/`panel` above, for the two commands this
        task added: a harness claiming `cli_name == "frame-menu"` would pass the loop's
        own `sub.choices` check (nothing is named that yet) and only collide once
        `_add_frame_parsers`'s own `sub.add_parser("frame-menu")` call runs — silently
        shadowing the hotkey menu's own handler on a 3.11 floor, with nothing telling
        the operator why the hotkey stopped doing anything."""
        from charter import cli
        from charter.harness.base import Harness

        class _FrameMenuHarness(Harness):
            name = "frame-menu-harness"
            cli_name = "frame-menu"
            binary = "frame-menu-harness"

        with mock.patch.dict("charter.harness.registry.KINDS",
                             {"frame-menu-harness": _FrameMenuHarness}, clear=True):
            with self.assertRaises(ValueError) as ctx:
                cli.build_parser()
        self.assertIn("frame-menu-harness", str(ctx.exception))

    def test_a_harness_named_frame_action_is_refused_too(self):
        from charter import cli
        from charter.harness.base import Harness

        class _FrameActionHarness(Harness):
            name = "frame-action-harness"
            cli_name = "frame-action"
            binary = "frame-action-harness"

        with mock.patch.dict("charter.harness.registry.KINDS",
                             {"frame-action-harness": _FrameActionHarness}, clear=True):
            with self.assertRaises(ValueError) as ctx:
                cli.build_parser()
        self.assertIn("frame-action-harness", str(ctx.exception))

    def test_a_harness_named_frame_probe_is_refused_too(self):
        """`frame-probe` (this task) is reserved the same way, though for a different
        reason than `frame-menu`/`frame-action`: it exists because `news._PROBEABLE`
        cannot list `("frame",)` at all — every parser `_wire` builds carries a
        pass-through `rest` — not because `_split_frame_argv` eats anything past
        `frame`. A harness silently shadowing it would break the one command a news
        `check:` can safely name for the frame feature at all."""
        from charter import cli
        from charter.harness.base import Harness

        class _FrameProbeHarness(Harness):
            name = "frame-probe-harness"
            cli_name = "frame-probe"
            binary = "frame-probe-harness"

        with mock.patch.dict("charter.harness.registry.KINDS",
                             {"frame-probe-harness": _FrameProbeHarness}, clear=True):
            with self.assertRaises(ValueError) as ctx:
                cli.build_parser()
        self.assertIn("frame-probe-harness", str(ctx.exception))


class TwoHarnessesCanShareAWord(unittest.TestCase):
    """The genuine flaw a rebase onto charter 0.49.0 surfaced: `registry.all()`
    instantiates by dict VALUE (`[cls() for cls in KINDS.values()]`), so two KEYS mapped
    to the same class produce two harness objects sharing a `cli_name` —
    `tests/test_guard_claims_its_reach.py::test_the_names_are_derived_not_written_down`
    reaches exactly this shape (`KINDS["zzz-fictional"] = KINDS[CLAUDE_CODE]`) from a
    module that owns no code here at all. `_add_frame_parsers` used to raise
    `ValueError` for it — indistinguishable, until this fix, from a harness shadowing a
    CORE command — which took down `build_parser()`, and with it every `charter`
    command, over a registry mistake in a different module entirely. A harness-vs-
    harness collision costs the LATER harness one launcher; it must not cost the whole
    CLI, which is the whole reason `_add_frame_parsers` now tells the two apart.
    """

    def test_a_harness_colliding_with_a_core_command_still_raises(self):
        """Enumerated against the REAL parser's own core commands, not one hardcoded
        name (`CollisionGuard` above already pins `status` alone) — the rule has to hold
        for core commands in general, not just the one example that happened to be
        picked first."""
        from charter import cli, harness
        from charter.harness.base import Harness

        core = cli._subcommand_names(cli.build_parser())
        reserved = ({h.cli_name for h in harness.all() if h.cli_name} |
                   {"frame", "panel", "frame-menu", "frame-action", "frame-probe"})
        # Only genuinely CORE commands — colliding with the frame family itself is
        # `CollisionGuard`'s own job, one dedicated test per reserved name.
        sample = sorted(core - reserved)
        self.assertTrue(sample, "no core (non-harness) command found to collide with")
        for core_name in sample[:4]:
            with self.subTest(core_command=core_name):
                # Named `core_name`, not `name`: a class body assigning its OWN `name`
                # attribute shadows an enclosing `name` when Python resolves names
                # inside the class suite, which turns `name = f"...{name}"` into a
                # `NameError` from inside the class body itself (confirmed by hand).
                class _Colliding(Harness):
                    name = f"colliding-with-{core_name}"
                    cli_name = core_name
                    binary = "colliding"

                with mock.patch.dict("charter.harness.registry.KINDS",
                                     {f"colliding-with-{core_name}": _Colliding},
                                     clear=True):
                    with self.assertRaises(ValueError) as ctx:
                        cli.build_parser()
                self.assertIn(f"colliding-with-{core_name}", str(ctx.exception))

    def test_two_harnesses_sharing_a_cli_name_do_not_raise_and_the_first_wins(self):
        """`build_parser()` must succeed, exactly one subcommand must exist for the
        shared word, and running it must resolve to a REAL harness — the one registered
        FIRST, `KINDS`'s own dict-insertion order (`registry.all`'s own docstring: "in
        registration order"), asserted directly here rather than assumed.

        This is the OUTCOME an operator would see, not a direct pin of
        `_add_frame_parsers`'s own loop order: `_wire` stores only the plain STRING
        `h.cli_name` on the parser (`set_defaults(harness=name, ...)`), so `cmd_launch`
        re-resolves it against a FRESH `harness.all()` at run time regardless of which of
        the two colliding classes happened to call `add_parser` first — confirmed by hand
        by reversing `_add_frame_parsers`'s own iteration order and watching this test
        stay green while `test_the_second_claimant_is_reported_not_silent` below (which
        checks the warning's own wording) goes red. That test is what actually pins the
        loop's choice of "first"; this one pins that the choice is never silently wrong
        from an operator's seat.
        """
        from charter import cli, harness
        from charter.harness import registry
        from charter.harness.base import Harness

        class _First(Harness):
            name = "first-claimant"
            cli_name = "shared-word"
            binary = "first"

        class _Second(Harness):
            name = "second-claimant"
            cli_name = "shared-word"   # same word, a DIFFERENT class
            binary = "second"

        kinds = {"first-claimant": _First, "second-claimant": _Second}
        with mock.patch.dict("charter.harness.registry.KINDS", kinds, clear=True):
            # What "first" means, pinned rather than assumed: dict-insertion order.
            self.assertEqual([h.name for h in registry.all()],
                             ["first-claimant", "second-claimant"])
            parser = cli.build_parser()  # must NOT raise
            names = cli._subcommand_names(parser)
            self.assertIn("shared-word", names)
            args = parser.parse_args(["shared-word", "--", "hi"])
            self.assertEqual(args.harness, "shared-word")
            # `cmd_launch` resolves `args.harness` back to a `Harness` this same way
            # (`next((x for x in harness.all() if x.cli_name == args.harness), None)`)
            # — the harness that actually RUNS for this word is the one that got the
            # launcher, not merely the one this test expects by name.
            resolved = next((h for h in harness.all() if h.cli_name == args.harness), None)
        self.assertIsNotNone(resolved)
        self.assertEqual(resolved.name, "first-claimant")

    def test_the_second_claimant_is_reported_not_silent(self):
        """"Make the skip observable": a second harness wanting a claimed word must say
        so, not merely fail to get a launcher with nothing printed anywhere.

        Roles are checked precisely, not just "both names appear somewhere": it is
        `second-claimant` that must be named as the one left without a launcher, and
        `first-claimant` as who already holds it — the same distinction
        `test_two_harnesses_sharing_a_cli_name_do_not_raise_and_the_first_wins` pins from
        the parser's own side. A version of this loop that iterated `harness.all()`
        reversed would still call `util.warn` exactly once, mentioning both names, and
        still pass a test that only checked for their presence — confirmed by hand: only
        the STARTSWITH/`already claimed by` checks below actually go red under that
        mutation.
        """
        from charter import cli
        from charter.harness.base import Harness

        class _First(Harness):
            name = "first-claimant"
            cli_name = "shared-word"
            binary = "first"

        class _Second(Harness):
            name = "second-claimant"
            cli_name = "shared-word"
            binary = "second"

        kinds = {"first-claimant": _First, "second-claimant": _Second}
        with mock.patch.dict("charter.harness.registry.KINDS", kinds, clear=True), \
             mock.patch("charter.util.warn") as warn:
            cli.build_parser()
        warn.assert_called_once()
        said = warn.call_args[0][0]
        self.assertTrue(said.startswith("harness 'second-claimant'"), said)
        self.assertIn("already claimed by 'first-claimant'", said)
        self.assertIn("shared-word", said)


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

    def test_a_bare_probe_is_still_recognized(self):
        """The exact trap this task's brief named: without `--probe` in `_OWN_FLAGS`,
        `_split_frame_argv` grafts it onto `args.rest` instead of leaving it for
        `argparse`, and `cmd_launch` (finding no harness named `""` for bare `frame`)
        hands `["--probe"]` to `bypass`, which `os.execvp("--probe", ...)` turns into a
        real `FileNotFoundError` — confirmed by hand by removing the entry and running
        `charter frame --probe`."""
        ns = self._parse(["frame", "--probe"])
        self.assertTrue(ns.probe)
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

    def test_frame_action_and_frame_menu_are_not_swallowed_by_the_frame_split(self):
        """The bug this pins: `argv[0] == "frame"` is what `_split_frame_argv` matches
        on, so `charter frame action a1` (a SPACE, as the task brief's own draft text
        proposed) would have `"action"` and `"a1"` grafted onto `args.rest` — the SAME
        path `charter frame -- <cmd>` uses — and `cmd_launch` would try to LAUNCH A NEW
        FRAME running `["action", "a1"]` as a harness, never reaching `cmd_action` at
        all. `frame-action`/`frame-menu` are different literal tokens `argv[0]` never
        matches, so `_split_frame_argv` leaves them alone — the same reason `panel` is
        already a top-level sibling of `frame` rather than nested under it."""
        from charter.cli import _split_frame_argv
        for argv in (["frame-action", "a1"], ["frame-menu"], ["frame-probe"]):
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

    def test_charter_frame_action_reaches_cmd_action_via_main(self):
        """The delivery half of the `_split_frame_argv` fix above: a test that only
        proves `_split_frame_argv` leaves `frame-action` alone cannot catch `cli.py`
        never having registered a parser for it in the first place."""
        from charter import cli
        with mock.patch("charter.commands_frame.cmd_action", return_value=0) as ca:
            rc = cli.main(["frame-action", "a3"])
        ca.assert_called_once()
        self.assertEqual(ca.call_args[0][0].action_id, "a3")
        self.assertEqual(rc, 0)

    def test_charter_frame_menu_reaches_cmd_menu_via_main(self):
        """`client` (`#{client_name}`, expanded by tmux inside the bind's own text —
        see `conf_text`'s docstring) is a required positional now, not queried after
        the fact."""
        from charter import cli
        with mock.patch("charter.commands_frame.cmd_menu", return_value=0) as cm:
            rc = cli.main(["frame-menu", "/dev/ttys7"])
        cm.assert_called_once()
        self.assertEqual(cm.call_args[0][0].client, "/dev/ttys7")
        self.assertEqual(rc, 0)

    def test_charter_frame_probe_reaches_cmd_probe_via_main(self):
        """The delivery half of `frame-probe`'s own registration: a test that only
        proves `_split_frame_argv` leaves it alone (`FrameArgvSplit` above) cannot catch
        `_add_frame_parsers` never having registered a parser for it."""
        from charter import cli
        with mock.patch("charter.commands_frame.cmd_probe", return_value=0) as cp:
            rc = cli.main(["frame-probe"])
        cp.assert_called_once()
        self.assertEqual(rc, 0)

    def test_charter_frame_dash_dash_probe_reaches_cmd_launch_via_main(self):
        """The delivery half of the `_OWN_FLAGS` fix: a test that only exercises
        `_split_frame_argv` in isolation (`test_a_bare_probe_is_still_recognized` above)
        cannot catch `main` failing to graft `args.probe` through, or `cmd_launch`
        itself never checking it."""
        from charter import cli
        with mock.patch("charter.frame.tmuxctl.version", return_value=(3, 7)), \
             mock.patch("charter.commands_frame.subprocess.run") as run, \
             mock.patch("builtins.print"):
            rc = cli.main(["frame", "--probe"])
        run.assert_not_called()
        self.assertEqual(rc, 0)


class MenuCommands(PersonaIso, unittest.TestCase):
    """`cmd_menu`/`cmd_action` — the handlers `charter frame-menu`/`charter frame-action`
    dispatch to. `cmd_menu`'s `args.client` is `#{client_name}`, expanded by tmux INSIDE
    the bind's own text before this process ever starts (see `conf_text`'s docstring) —
    never queried here, so these tests supply it directly rather than mocking a
    `list-clients` call that no longer exists. `fid` is still resolved purely from
    `$CHARTER_SESSION_ID`; neither function ever takes a frame id as an argument (see
    `menu.py`'s own docstring for why the id stays out of the bind's own text)."""

    def test_cmd_menu_opens_the_current_frames_own_menu_on_its_own_client(self):
        """`-c` — not merely `-t` — is what the fix for IMPORTANT-1 added: `-t fid`
        alone does not choose WHICH terminal sees the menu (verified by hand: it
        rendered frame B's menu on frame A's screen). `cmd_menu` must pass
        `args.client` straight through to `-c`, unmodified."""
        menu.record(fid="f-menu", entries=[("Detach", ["true"])])
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "f-menu"}), \
             mock.patch("charter.commands_frame.subprocess.run") as run:
            run.return_value = SimpleNamespace(returncode=0)
            rc = commands_frame.cmd_menu(SimpleNamespace(client="/dev/ttys7"))
        self.assertEqual(rc, 0)
        run.assert_called_once()
        menu_cmd = run.call_args[0][0]
        self.assertEqual(menu_cmd[:9],
                         ["tmux", "-L", "charter", "display-menu", "-t", "f-menu",
                          "-c", "/dev/ttys7", "-T"])

    def test_cmd_menu_with_an_empty_client_is_a_quiet_no_op(self):
        """`#{client_name}` failing to expand (or `charter frame-menu` invoked some
        other way with nothing supplied) leaves no screen to draw on and none to
        report that on either — `display-menu` must never even be attempted."""
        menu.record(fid="f-menu", entries=[("Detach", ["true"])])
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "f-menu"}), \
             mock.patch("charter.commands_frame.subprocess.run") as run:
            rc = commands_frame.cmd_menu(SimpleNamespace(client=""))
        self.assertEqual(rc, 0)
        run.assert_not_called()

    def test_cmd_menu_with_no_session_id_is_a_quiet_no_op_not_a_failed_display_menu(self):
        """Real tmux 3.7c: `list-clients -t ""` (the query an EARLIER version of this
        module made) does NOT report zero clients — with a client attached it reports
        the CURRENT session's client, same as an unscoped query would. The prior test
        for this case asserted the opposite and passed anyway, because its mock hard
        -coded the convenient wrong answer — the exact "wrong invariant pinned by a
        passing test" this fix round exists to remove. `cmd_menu` no longer queries
        `list-clients` at all, so that specific fiction cannot recur, but the SAME
        real hazard remains from a different angle: `menu.build("")` is always empty
        (`state.frame_dir` refuses an empty id), so a real `display-menu` call with no
        `$CHARTER_SESSION_ID` would still have ZERO items and fail outright ("too few
        arguments") if `cmd_menu` did not check for that deliberately."""
        with mock.patch.dict(os.environ, {}, clear=True), \
             mock.patch("charter.commands_frame.subprocess.run") as run:
            rc = commands_frame.cmd_menu(SimpleNamespace(client="/dev/ttys7"))
        self.assertEqual(rc, 0)
        run.assert_not_called()

    def test_cmd_menu_with_a_real_session_but_an_empty_menu_is_also_a_quiet_no_op(self):
        """The same empty-menu guard, reached a different way: a genuine
        `$CHARTER_SESSION_ID` whose frame has recorded no entries yet (nothing has
        called `menu.record` for it, or `cmd_launch` has not reached that point)."""
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "f-empty"}), \
             mock.patch("charter.commands_frame.subprocess.run") as run:
            rc = commands_frame.cmd_menu(SimpleNamespace(client="/dev/ttys7"))
        self.assertEqual(rc, 0)
        run.assert_not_called()

    def test_cmd_action_runs_the_resolved_argv_as_a_list_never_a_shell(self):
        menu.record(fid="f-act", entries=[("Detach", ["tmux", "-L", "charter",
                                                       "detach-client", "-s", "f-act"])])
        entries = menu.build("f-act")
        action_id = entries[0][1]
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "f-act"}), \
             mock.patch("charter.commands_frame.subprocess.run") as run:
            run.return_value = SimpleNamespace(returncode=0)
            rc = commands_frame.cmd_action(SimpleNamespace(action_id=action_id))
        self.assertEqual(rc, 0)
        run.assert_called_once_with(["tmux", "-L", "charter", "detach-client",
                                     "-s", "f-act"])

    def test_cmd_action_on_an_unknown_id_is_a_clean_failure_not_a_crash(self):
        buf = []
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "f-act"}), \
             mock.patch("charter.util.err", side_effect=lambda m: buf.append(m)):
            rc = commands_frame.cmd_action(SimpleNamespace(action_id="a99"))
        self.assertEqual(rc, 2)
        self.assertTrue(any("a99" in m for m in buf), buf)


#: The `$TMUX` a real tmux 3.7c exports into every pane it starts —
#: `<socket path>,<server pid>,<session id>` (measured by printing it from inside one).
OPERATOR_TMUX = "/private/tmp/tmux-502/default,70029,1"
OPERATOR_SOCKET = "/private/tmp/tmux-502/default"

#: How many "is the harness still there?" asks `_FakeOperatorTmux` answers before it
#: calls the launcher stuck. Every test here drives the wait loop through at most a
#: handful, so anything past this is a launcher that stopped noticing an answer.
_SPIN_LIMIT = 50


class _FakeOperatorTmux:
    """Every tmux command the inside-a-tmux launch makes, against a server it did not
    start.

    A separate fake from `_FakeTmux` on purpose: the two paths issue almost disjoint
    command sets (`new-window`/`respawn-pane`/`kill-window` against
    `new-session`/`attach`/`kill-session`), and folding both into one fake would let a
    launcher that took the WRONG branch still find every command it asked for. Anything
    unexpected raises, so a test fails loudly rather than quietly exercising the other
    path.

    *polls_alive* is how many status queries report the harness still running before it
    dies, so a test can drive the wait loop rather than mock it away — the loop is the
    piece that replaces `attach` here, and `attach` is what carried the exit code out on
    the other path.
    """

    def __init__(self, *, window_id="@3", pane_id="%7", polls_alive=1, exit_code=0,
                 pane_vanishes=False, window_size=(200, 50),
                 pre_existing_windows=("zsh",), list_windows_rc=0,
                 new_window_rc=0, arm_rc=0, respawn_rc=0, panel_rc=0,
                 panel_pane_ids=None, resize_hook_rc=0, select_rc=0, kill_rc=0,
                 pane_capture="", capture_rc=0):
        self.window_id = window_id
        self.pane_id = pane_id
        self.polls_alive = polls_alive
        self.exit_code = exit_code
        self.pane_vanishes = pane_vanishes
        # What `capture-pane` reports the dead pane still held. #384 reaches this path
        # too: a harness that dies before the frame is drawn is never switched to, so
        # the operator's screen never changes and the pane they never saw is the only
        # place the reason exists.
        self.pane_capture = pane_capture
        self.capture_rc = capture_rc
        self.window_size = window_size
        self.pre_existing_windows = list(pre_existing_windows)
        self.list_windows_rc = list_windows_rc
        self.new_window_rc = new_window_rc
        self.arm_rc = arm_rc
        self.respawn_rc = respawn_rc
        self.panel_rc = panel_rc
        self.panel_pane_ids = panel_pane_ids or {}
        self.resize_hook_rc = resize_hook_rc
        self.select_rc = select_rc
        self.kill_rc = kill_rc
        self.calls: list[list[str]] = []
        self.status_queries = 0
        self.window_killed = False
        self.respawn_env = None

    def _ok(self, cmd, stdout=""):
        return subprocess.CompletedProcess(cmd, 0, stdout=stdout, stderr="")

    def __call__(self, cmd, **kwargs):
        self.calls.append(list(cmd))
        if "list-windows" in cmd:
            if self.list_windows_rc != 0:
                return subprocess.CompletedProcess(
                    cmd, self.list_windows_rc, stdout="",
                    stderr=f"error connecting to {OPERATOR_SOCKET} (No such file)")
            live = list(self.pre_existing_windows)
            if self.window_killed is False and any("new-window" in c for c in self.calls):
                live.append(_frame_id())
            return self._ok(cmd, stdout="\n".join(live))
        if "new-window" in cmd:
            if self.new_window_rc != 0:
                return subprocess.CompletedProcess(cmd, self.new_window_rc, stdout="",
                                                   stderr="no space")
            return self._ok(cmd, stdout=f"{self.window_id} {self.pane_id}\n")
        if "remain-on-exit" in cmd:
            return subprocess.CompletedProcess(cmd, self.arm_rc, stdout="",
                                               stderr="" if self.arm_rc == 0 else "cannot set")
        if "respawn-pane" in cmd:
            self.respawn_env = {a.split("=", 1)[0]: a.split("=", 1)[1]
                                for i, a in enumerate(cmd)
                                if i and cmd[i - 1] == "-e"}
            return subprocess.CompletedProcess(cmd, self.respawn_rc, stdout="",
                                               stderr="" if self.respawn_rc == 0 else "no pane")
        if "display-message" in cmd:
            fmt = cmd[-1]
            if "window_width" in fmt:
                return self._ok(cmd, stdout="%d:%d\n" % self.window_size)
            self.status_queries += 1
            if self.status_queries > _SPIN_LIMIT:
                # A spin rather than a hang: `_wait_for_harness` is deliberately
                # unbounded (an agent session runs for hours and charter waits for all
                # of it), so a launcher that never stops asking would otherwise wedge
                # the whole suite instead of failing it. Reached only by a defect —
                # nothing here answers "alive" more than a handful of times.
                raise AssertionError(
                    f"charter asked whether the harness pane was still there "
                    f"{self.status_queries} times — it never stopped waiting for a "
                    f"pane that is gone")
            if self.status_queries <= self.polls_alive:
                return self._ok(cmd, stdout="0:\n")
            if self.pane_vanishes:
                # Measured against a real tmux 3.7c (see
                # `tests/test_frame_tmux_integration.py`): a `-t` naming a pane that no
                # longer exists is NOT an error. `display-message -p` returns 0 and
                # expands both variables to nothing — leaving the format's OWN literal
                # `:` behind, not an empty line. Getting this wrong here is what let a
                # launcher that spun forever on a closed window pass every unit test.
                return self._ok(cmd, stdout=":\n")
            return self._ok(cmd, stdout=f"1:{self.exit_code}\n")
        if "capture-pane" in cmd:
            return subprocess.CompletedProcess(
                cmd, self.capture_rc,
                stdout=self.pane_capture if self.capture_rc == 0 else "",
                stderr="" if self.capture_rc == 0 else "no such pane")
        if "split-window" in cmd:
            slot = cmd[cmd.index("panel") + 1] if "panel" in cmd else None
            pane = self.panel_pane_ids.get(slot, "") if self.panel_rc == 0 else ""
            return subprocess.CompletedProcess(cmd, self.panel_rc,
                                               stdout=f"{pane}\n" if pane else "",
                                               stderr="" if self.panel_rc == 0 else "no space")
        if "set-hook" in cmd:
            return subprocess.CompletedProcess(cmd, self.resize_hook_rc, stdout="",
                                               stderr="" if self.resize_hook_rc == 0
                                               else "bad hook target")
        if "select-pane" in cmd or "select-window" in cmd:
            return subprocess.CompletedProcess(cmd, self.select_rc, stdout="",
                                               stderr="" if self.select_rc == 0 else "no such")
        if "kill-window" in cmd:
            self.window_killed = True
            return subprocess.CompletedProcess(cmd, self.kill_rc, stdout="",
                                               stderr="" if self.kill_rc == 0 else "no window")
        raise AssertionError(f"unexpected tmux command on the operator's server: {cmd}")


def _frame_id():
    return state.frame_id("demo", os.getpid())


def _launch_inside(fake: _FakeOperatorTmux, *, version=(3, 7), harness="claude",
                   rest=(), tmux_env=OPERATOR_TMUX, slots=None):
    """Run `cmd_launch` as if the operator typed it inside their own tmux."""
    _refuse_the_real_plane()
    args = SimpleNamespace(harness=harness, rest=list(rest), no_frame=False)
    env = dict(os.environ, TMUX=tmux_env, TMUX_PANE="%0")
    ctx = [mock.patch.dict(os.environ, env, clear=True),
           mock.patch("charter.commands_frame.subprocess.run", side_effect=fake),
           mock.patch("charter.commands_frame.time.sleep"),
           mock.patch("sys.stdout.isatty", return_value=True),
           _harness_binary_installed(),
           mock.patch("charter.frame.tmuxctl.version", return_value=version),
           mock.patch("charter.workspace.resolve", return_value="demo")]
    with contextlib.ExitStack() as stack:
        for c in ctx:
            stack.enter_context(c)
        return commands_frame.cmd_launch(args)


class LaunchInsideTmux(PersonaIso, unittest.TestCase):
    """`charter claude` typed inside a tmux the operator already has.

    The frame is built as a WINDOW in THEIR server — the layout is identical, but there
    is no second tmux, no second prefix key, and nothing of theirs is written to. That
    is the settled design (`docs/superpowers/specs/2026-08-21-harness-wrapper-design.md`,
    ADR 0018); what shipped before this was a private server started INSIDE their pane,
    which works but leaves two prefix layers stacked on one terminal.

    Every assertion below is about a boundary rather than a preference: charter's
    commands must reach their socket and not a new one, must not write a single option
    of theirs, and must never `kill-session` — the last of which would take down the
    operator's whole session, every window in it, over a harness exiting.
    """

    def test_no_second_tmux_server_is_started(self):
        """The bug itself. `new-session` on charter's own socket is exactly the nesting
        this path replaces, and `-L` anywhere would mean charter went looking for a
        server of its own instead of the one it is already inside."""
        fake = _FakeOperatorTmux(exit_code=0)
        self.assertEqual(_launch_inside(fake), 0)
        self.assertTrue(fake.calls, "no tmux command was issued at all")
        self.assertFalse([c for c in fake.calls if "new-session" in c],
                         f"a second server was started: {fake.calls}")
        for cmd in fake.calls:
            self.assertNotIn("-L", cmd[:3], f"not the operator's own server: {cmd}")
            self.assertEqual(cmd[:3], ["tmux", "-S", OPERATOR_SOCKET], cmd)

    def test_the_frame_is_a_window_in_the_operators_own_session(self):
        fake = _FakeOperatorTmux(exit_code=0)
        _launch_inside(fake)
        new_window = next(c for c in fake.calls if "new-window" in c)
        self.assertEqual(new_window[new_window.index("-t") + 1], "$1",
                         "the session id charter read out of `$TMUX`")
        self.assertEqual(new_window[new_window.index("-n") + 1], _frame_id(),
                         "the window is named for the frame, which is what liveness "
                         "and reaping are matched on here")

    def test_the_harness_is_never_what_the_window_is_created_with(self):
        """The placeholder-then-respawn ordering, seen from the launcher: if the harness
        were `new-window`'s own command it could exit before `remain-on-exit` was set on
        its pane, and its exit code would leave with the window."""
        fake = _FakeOperatorTmux(exit_code=0)
        _launch_inside(fake)
        new_window = next(c for c in fake.calls if "new-window" in c)
        self.assertNotIn("claude", new_window, f"the harness started too early: {new_window}")
        respawn = next(c for c in fake.calls if "respawn-pane" in c)
        self.assertEqual(respawn[respawn.index("--") + 1:], ["claude"])

    def test_the_pane_is_kept_askable_before_the_harness_can_exit(self):
        fake = _FakeOperatorTmux(exit_code=0)
        _launch_inside(fake)
        arm = next(i for i, c in enumerate(fake.calls) if "remain-on-exit" in c)
        respawn = next(i for i, c in enumerate(fake.calls) if "respawn-pane" in c)
        self.assertLess(arm, respawn,
                        f"remain-on-exit must be armed first: {fake.calls}")
        armed = fake.calls[arm]
        self.assertIn("-p", armed, "pane-scoped, never the operator's own -g or session")
        self.assertEqual(armed[armed.index("-t") + 1], "%7")

    def test_nothing_of_the_operators_is_written(self):
        """Their config untouched, in the spec's own words. Every one of these would
        reach beyond charter's own window: `source-file` and `set -g` are server- or
        session-wide, `set-environment` hands every new shell they open a frame id that
        is not theirs, and a key binding is server-wide with no per-window form at all."""
        fake = _FakeOperatorTmux(exit_code=0)
        _launch_inside(fake)
        for cmd in fake.calls:
            self.assertNotIn("source-file", cmd, cmd)
            self.assertNotIn("set-environment", cmd, cmd)
            self.assertNotIn("bind", cmd, cmd)
            self.assertNotIn("bind-key", cmd, cmd)
            self.assertNotIn("-g", cmd, f"a global write on somebody else's server: {cmd}")

    def test_the_operators_session_is_never_killed(self):
        """The one that is catastrophic rather than merely rude: `kill-session` on their
        server ends every window they have open, over a harness exiting."""
        fake = _FakeOperatorTmux(exit_code=3)
        self.assertEqual(_launch_inside(fake), 3)
        self.assertFalse([c for c in fake.calls if "kill-session" in c], fake.calls)
        kill = next(c for c in fake.calls if "kill-window" in c)
        self.assertEqual(kill[kill.index("-t") + 1], "@3",
                         "the window id tmux reported, never an index tmux renumbers")
        self.assertTrue(fake.window_killed)

    def test_the_operator_is_switched_to_the_frame_never_attached(self):
        """`attach` is what the private-server path blocks on; here the operator is
        already attached to their own server, so a second attach would be the nesting
        again. `select-window` moves the client they already have."""
        fake = _FakeOperatorTmux(exit_code=0)
        _launch_inside(fake)
        self.assertFalse([c for c in fake.calls if "attach" in c], fake.calls)
        select = next(c for c in fake.calls if "select-window" in c)
        self.assertEqual(select[select.index("-t") + 1], "@3")

    def test_the_harnesss_own_exit_code_comes_back(self):
        """The property the whole module exists for, on a path where `attach` is not
        there to be waited on: charter watches the pane instead and exits with what tmux
        reports the harness died with."""
        fake = _FakeOperatorTmux(exit_code=42, polls_alive=3)
        self.assertEqual(_launch_inside(fake), 42)
        self.assertGreater(fake.status_queries, 3,
                           "charter stopped watching before the harness had finished")

    def test_charters_own_variables_reach_the_harness_on_the_respawn(self):
        """`harness.current()` reads `$CHARTER_HARNESS` before any native detection, and
        every hook inside the frame resolves `$CHARTER_SESSION_ID`. On the private-server
        path both ride in the environment `new-session` hands the server it starts; there
        is no such moment here, so they ride on `respawn-pane -e` instead — which reaches
        this pane and nothing else of theirs.

        The ambient values are overwritten with a sentinel first: this suite is normally
        run inside a charter frame, where `$CHARTER_HARNESS` is already set, so a bare
        assertion on the expected value would pass whether or not the launcher carried
        anything."""
        fake = _FakeOperatorTmux(exit_code=0)
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "stale-sentinel",
                                          "CHARTER_SESSION_ID": "stale-sentinel"}):
            _launch_inside(fake)
        self.assertEqual(fake.respawn_env.get("CHARTER_SESSION_ID"), _frame_id())
        self.assertEqual(fake.respawn_env.get("CHARTER_HARNESS"), "claude-code")

    def test_the_launching_pane_is_never_impersonated(self):
        """`$TMUX`/`$TMUX_PANE` describe the pane charter was TYPED in, and tmux sets
        both itself for the pane it creates. Carrying the launcher's own values across
        would tell the harness it is running in a pane it is not — the identity
        collision `_PANE_ID_VARS` and `WINDOWID` were argued about for."""
        fake = _FakeOperatorTmux(exit_code=0)
        _launch_inside(fake)
        self.assertNotIn("TMUX", fake.respawn_env)
        self.assertNotIn("TMUX_PANE", fake.respawn_env)

    def test_the_frames_state_is_reaped_against_the_operators_server(self):
        """A frame here is a window on their socket and appears in no `tmux -L charter
        list-sessions` output at all. Reaping on the wrong server's list deletes a
        running frame's version file and its recorded exit code while it is still on
        screen — see `tests/test_frame_state.py`'s `ReapAcrossServers`."""
        fake = _FakeOperatorTmux(exit_code=0)
        seen = []
        marked = []
        real_reap, real_record = state.reap, state.record_server
        with mock.patch("charter.frame.state.reap",
                        side_effect=lambda live, **kw: (seen.append(kw.get("server")),
                                                        real_reap(live, **kw))[1]), \
             mock.patch("charter.frame.state.record_server",
                        side_effect=lambda fid, server: (marked.append((fid, server)),
                                                         real_record(fid, server))[1]):
            _launch_inside(fake)
        self.assertTrue(seen, "nothing was reaped at all")
        self.assertEqual(set(seen), {OPERATOR_SOCKET})
        # And the frame wrote down whose server it was on, which is the only thing that
        # lets a LATER launch on the other server leave it alone. Asserted on the call
        # rather than on the file: the last `reap` of a finished launch legitimately
        # removes this frame's own directory, marker and all.
        self.assertEqual(marked, [(_frame_id(), OPERATOR_SOCKET)])

    def test_a_launch_here_does_not_inherit_a_cached_scan_from_an_earlier_life_of_its_pid(self):
        """#383's bill, on this path. `Launch` pins it for charter's own server; the
        recycled pid is not a property of a server, it is a property of `frame_id`, so
        the launcher inside somebody else's tmux adopts the same directory in exactly
        the same circumstances — and `reap` keeps it for exactly the same reason (the
        pid at the end of the name is live, because it is this launcher's own).

        `gather.json` is what costs something here: `gather.read` has no freshness check
        by design and a panel repaints only on a version bump, so a dead frame's repos,
        branches and CI would sit beside a live harness in the operator's own window
        until the session's first hook fires. `scan` is mocked to a sentinel so a served
        cache fails, and "a live gather happened to agree" cannot pass by accident."""
        stale = _frame_id()
        gather.save(stale, {"gathered_at": 1.0, "workspace": "from-a-dead-frame",
                            "current_repo": None, "repos": [], "worktrees": []})

        fake = _FakeOperatorTmux(exit_code=0)
        _launch_inside(fake)

        fresh = {"gathered_at": 0.0, "workspace": "sentinel", "current_repo": None,
                 "repos": [], "worktrees": []}
        with mock.patch.object(gather, "scan", return_value=fresh):
            self.assertEqual(gather.read(stale), fresh,
                             "a panel of this frame would draw a dead frame's scan "
                             "until the session's first hook bump")

    def test_a_launch_here_clears_an_adopted_exit_code_before_it_records_its_own(self):
        """The other half of the adopted directory. The code returned on this path comes
        off the pane rather than out of the file, so a stale `exit` is not read back as
        this launch's result — but it IS charter's record of how the frame ended, and a
        launcher killed mid-run would leave a dead frame's number standing as this one's.

        Ordered against `bump` rather than checked after the launch, because a completed
        launch legitimately records an exit of its own and would overwrite the fixture
        either way — which is precisely the assertion that would still pass with
        `clear_exit` deleted."""
        order = []
        real_clear, real_discard, real_bump = state.clear_exit, gather.discard, state.bump
        with mock.patch("charter.frame.state.clear_exit",
                        side_effect=lambda fid: (order.append(("clear_exit", fid)),
                                                 real_clear(fid))[1]), \
             mock.patch("charter.commands_frame.gather.discard",
                        side_effect=lambda fid: (order.append(("discard", fid)),
                                                 real_discard(fid))[1]), \
             mock.patch("charter.frame.state.bump",
                        side_effect=lambda fid: (order.append(("bump", fid)),
                                                 real_bump(fid))[1]):
            _launch_inside(_FakeOperatorTmux(exit_code=0))

        fid = _frame_id()
        self.assertIn(("clear_exit", fid), order, f"nothing cleared the adopted exit: {order}")
        self.assertIn(("discard", fid), order, f"nothing discarded the adopted scan: {order}")
        self.assertLess(order.index(("clear_exit", fid)), order.index(("bump", fid)),
                        f"the adopted state outlived the bump panels poll: {order}")

    def test_a_server_that_stops_answering_is_not_treated_as_empty(self):
        """`state.reap` deletes every frame directory not in the list it is handed, so
        handing it an empty one for a server that did not answer wipes a SIBLING frame's
        version file and recorded exit code — while that frame is still on screen. An
        empty window list and no window list are opposite facts."""
        fake = _FakeOperatorTmux(exit_code=0)
        real_calls = []
        real_live = commands_frame._live_windows

        def _dies_at_the_end(socket):
            real_calls.append(socket)
            return real_live(socket) if len(real_calls) == 1 else None

        with mock.patch("charter.commands_frame._live_windows",
                        side_effect=_dies_at_the_end), \
             mock.patch("charter.frame.state.reap") as reap:
            _launch_inside(fake)
        self.assertEqual(reap.call_count, 1,
                         "the closing reap ran against a server that said nothing")

    def test_the_frame_is_sized_from_the_tmux_window_not_the_launching_pane(self):
        """`os.get_terminal_size()` here measures the pane charter was typed in, which
        is a fraction of the window the frame gets when the operator has splits open.
        Sizing the slots from it would drop panels a full-width window has room for."""
        fake = _FakeOperatorTmux(exit_code=0, window_size=(60, 8),
                                 panel_pane_ids={"top": "%8", "bottom": "%9"})
        with mock.patch("os.get_terminal_size",
                        return_value=_os_terminal_size(400, 100)) as size:
            _launch_inside(fake)
        size.assert_not_called()
        self.assertFalse([c for c in fake.calls if "split-window" in c],
                         "a 60x8 tmux window has no room for a panel")

    def test_the_panels_are_split_off_the_harness_pane(self):
        fake = _FakeOperatorTmux(exit_code=0,
                                 panel_pane_ids={"top": "%8", "bottom": "%9"})
        _launch_inside(fake)
        splits = [c for c in fake.calls if "split-window" in c]
        self.assertEqual(len(splits), 2, f"one per configured slot: {splits}")
        for cmd in splits:
            self.assertEqual(cmd[cmd.index("-t") + 1], "%7",
                             "every split targets the harness pane's id — tmux "
                             "renumbers indices on each one")

    def test_the_panels_get_charters_own_environment_too(self):
        """Not only the harness. A panel resolves the plane it draws — `config.STATE_DIR`
        among it — from its own environment, and a pane on the operator's server inherits
        THEIR tmux server's, which may predate this plane entirely. Left unfixed, a panel
        inside their tmux drew a different plane's numbers from the harness beside it,
        and (measured by hand) `frame/slots.py` could not find this frame's own server
        marker, so the bottom row went on advertising a hotkey charter had not bound."""
        fake = _FakeOperatorTmux(exit_code=0, panel_pane_ids={"top": "%8"})
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "stale-sentinel"}):
            _launch_inside(fake)
        split = next(c for c in fake.calls if "split-window" in c)
        carried = {c.split("=", 1)[0]: c.split("=", 1)[1]
                   for i, c in enumerate(split) if i and split[i - 1] == "-e"}
        self.assertEqual(carried.get("CHARTER_SESSION_ID"), _frame_id())
        self.assertNotIn("TMUX_PANE", carried)

    def test_a_harness_that_dies_at_once_is_never_switched_to(self):
        """The same eager check the private-server path makes, for the same reason: a
        harness that died before charter finished building the frame leaves nothing
        worth showing, and switching the operator's client to it would park them on a
        dead pane."""
        fake = _FakeOperatorTmux(exit_code=127, polls_alive=0)
        self.assertEqual(_launch_inside(fake), 127)
        self.assertFalse([c for c in fake.calls if "select-window" in c], fake.calls)
        self.assertFalse([c for c in fake.calls if "split-window" in c], fake.calls)
        self.assertTrue(fake.window_killed, "the empty window was left behind")

    def test_a_harness_that_dies_at_once_inside_their_tmux_still_says_so(self):
        """#384 on this path, where the silence is worse than the one it was written
        for. On charter's own server a dead-on-arrival harness at least never got an
        `attach`; here the operator is looking at the tmux the whole time and the frame's
        window is created, filled with a corpse and killed again without their client
        ever being switched to it — every one of `select-window`, `split-window` and the
        wait loop is skipped by the test above. Nothing changes on their screen, so the
        report is the only thing they get.

        Both halves are asserted: charter's own framing (the exit code, which the pane
        does not name) and the pane's own words (which charter cannot invent). Deleting
        either one of `early_death_message` and `_pane_last_words` from this path leaves
        exactly one of these two assertions red."""
        fake = _FakeOperatorTmux(
            exit_code=127, polls_alive=0,
            pane_capture="zsh:1: command not found: nosuchthing-xyz\n")
        buf = []
        with mock.patch("charter.util.err", side_effect=lambda m: buf.append(m)):
            rc = _launch_inside(fake, harness="", rest=["--", "nosuchthing-xyz"])
        self.assertEqual(rc, 127)
        self.assertTrue(any("charter frame:" in m and "127" in m for m in buf),
                        f"the operator was told nothing about a dead frame: {buf}")
        self.assertTrue(any("command not found: nosuchthing-xyz" in m for m in buf),
                        f"the dead pane's own words never reached the report: {buf}")

    def test_the_dead_panes_words_are_read_off_their_server_before_it_is_closed(self):
        """Two properties one call has to get right, and both are invisible in the
        message itself.

        `-S <their socket>`, never `-L charter`: `_pane_last_words` is shared with the
        private-server path, and a hand-built `-L` there would send this capture to
        charter's own server, find no such pane, and hand `early_death_message` an empty
        list. The message would still be printed — that is the trap — just without the
        one thing only the pane knows.

        And BEFORE `kill-window`: the pane is the only copy, so a capture ordered after
        the window is closed reads nothing, with the same silent, still-printing
        degrade."""
        fake = _FakeOperatorTmux(exit_code=127, polls_alive=0,
                                 pane_capture="boom\n")
        with mock.patch("charter.util.err"):
            _launch_inside(fake, harness="", rest=["--", "nosuchthing-xyz"])
        captures = [i for i, c in enumerate(fake.calls) if "capture-pane" in c]
        self.assertEqual(len(captures), 1, fake.calls)
        self.assertEqual(fake.calls[captures[0]][:3], ["tmux", "-S", OPERATOR_SOCKET],
                         "the capture went to a server that is not the one the dead "
                         "pane is on")
        kills = [i for i, c in enumerate(fake.calls) if "kill-window" in c]
        self.assertTrue(kills, fake.calls)
        self.assertLess(captures[0], kills[0],
                        "the pane was destroyed before charter read it")

    def test_a_command_that_finished_cleanly_before_the_frame_is_not_narrated(self):
        """The other side of the same rule the private-server path takes: `charter frame
        -- true` reaches this identical branch with 0, and whatever it wrote was its own
        STDOUT. Repeating that onto stderr would be charter inventing output on the wrong
        stream over a command that did exactly what it was asked."""
        fake = _FakeOperatorTmux(exit_code=0, polls_alive=0, pane_capture="hello\n")
        buf = []
        with mock.patch("charter.util.err", side_effect=lambda m: buf.append(m)):
            rc = _launch_inside(fake, harness="", rest=["--", "true"])
        self.assertEqual(rc, 0)
        self.assertEqual(buf, [], f"a clean early exit was narrated anyway: {buf}")
        self.assertFalse([c for c in fake.calls if "capture-pane" in c],
                         "a clean exit's pane must not even be read")

    def test_a_window_the_operator_closes_ends_the_wait_rather_than_spinning(self):
        """Measured against tmux 3.7c: `display-message -p -t <pane that is gone>`
        returns 0 and prints an EMPTY line rather than failing — so a wait loop that
        only stops on `#{pane_dead}` being `1` would poll a window nobody can bring back
        for as long as the shell it was typed in stays open.

        Nonzero, deliberately: charter cannot know what the harness would have exited
        with, and reporting a killed agent as a clean 0 is the fabricated success this
        module refuses everywhere else."""
        fake = _FakeOperatorTmux(polls_alive=2, pane_vanishes=True)
        buf = []
        with mock.patch("charter.util.err", side_effect=lambda m: buf.append(m)):
            rc = _launch_inside(fake)
        self.assertEqual(rc, commands_frame._UNKNOWN_DEATH_CODE)
        self.assertTrue(any("window" in m for m in buf), buf)

    def test_a_stale_tmux_variable_falls_back_to_charters_own_server(self):
        """`$TMUX` exported into a shell whose tmux is long gone (a detached `env`
        capture, a `tmux kill-server` while a script was running) names a socket nothing
        answers on. Charter is then NOT inside a tmux, whatever the variable says, and
        the private-server path is the correct one — checked against the server rather
        than believed, because every command after this would otherwise fail one by one
        against a socket that does not exist."""
        fake = _FakeOperatorTmux(list_windows_rc=1)
        args = SimpleNamespace(harness="claude", rest=[], no_frame=False)
        private = _FakeTmux(exit_code=0)

        def _route(cmd, **kw):
            if "-S" in cmd[:3]:
                return fake(cmd, **kw)
            return private(cmd, **kw)

        with mock.patch.dict(os.environ, dict(os.environ, TMUX=OPERATOR_TMUX)), \
             mock.patch("charter.commands_frame.subprocess.run", side_effect=_route), \
             mock.patch("charter.commands_frame.time.sleep"), \
             mock.patch("sys.stdout.isatty", return_value=True), \
             _harness_binary_installed(), \
             mock.patch("charter.frame.tmuxctl.version", return_value=(3, 7)), \
             mock.patch("charter.workspace.resolve", return_value="demo"), \
             mock.patch("os.get_terminal_size",
                        return_value=_os_terminal_size(200, 50)):
            rc = commands_frame.cmd_launch(args)
        self.assertEqual(rc, 0)
        self.assertTrue([c for c in private.calls if "new-session" in c],
                        "charter never fell back to its own server")

    def test_a_window_that_cannot_be_opened_is_a_clean_failure(self):
        fake = _FakeOperatorTmux(new_window_rc=1)
        buf = []
        with mock.patch("charter.util.err", side_effect=lambda m: buf.append(m)):
            rc = _launch_inside(fake)
        self.assertNotEqual(rc, 0)
        self.assertFalse([c for c in fake.calls if "respawn-pane" in c], fake.calls)

    def test_a_harness_that_cannot_be_started_does_not_leave_the_window_behind(self):
        """`respawn-pane` failing leaves the placeholder running in a window the
        operator never asked for — visible, in their own window list, forever."""
        fake = _FakeOperatorTmux(respawn_rc=1)
        rc = _launch_inside(fake)
        self.assertNotEqual(rc, 0)
        self.assertTrue(fake.window_killed, "the placeholder window was left behind")


if __name__ == "__main__":
    unittest.main()
