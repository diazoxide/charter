"""`F2` pressed on a real client, against a real frame, opening a real palette.

**The whole of Task 4's step 1, end to end.** `charter frame-palette` is fired by tmux's
own root key table, splits a real pane off a real harness, draws real rows built from the
real action registry, filters on real keystrokes, and starts real work that outlives the
pane it was started from. Nothing here is a stand-in for the production path: the bind is
`commands_frame.conf_text`'s own text, `source-file`d for real, and the palette's pane
program is `charter frame-palette --pane` reached through `overlay.open_argv`.

**Why a real tmux and a real attached client.** `tmux send-keys` feeds a PANE's own input
queue and never touches the key table, so it cannot exercise a `bind -n` at all — the same
fact `test_frame_overlay_escape_hatch.py` records for the escape hatch and
`test_frame_tmux_integration.py` recorded for `display-menu` before it. The palette's whole
entry point is a root-table bind, so the only honest test is a client on a pty with the key
written to it as a terminal would.

**No `charter` on `$PATH` is needed and no shim is installed**, unlike the menu integration
this replaces. `commands_frame.SOCKET` was a hardcoded constant every one of those tests had
to monkeypatch around; `cmd_palette` resolves its server from `state.frame_server(fid)`
instead, so recording this class's own socket in the frame's own state is the entire
substitution. That is not a testing convenience — it is the same "resolve the frame at the
moment the key fires" rule the bind text itself depends on.

Skipped, never failed, where the machine cannot supply what this needs: no tmux, a tmux too
old for the frame's own floor, or a tmux that will not attach a client (a headless CI step's
`TERM=dumb`). Each skip names what was missing.
"""

from __future__ import annotations

import os
import pty
import shutil
import subprocess
import sys
import time
import unittest
from pathlib import Path

from charter import commands_frame, config, util
from charter.frame import overlay, state, tmuxctl

from tests import _tmuxreap
from tests._isolation import PersonaIso

_HAS_TMUX = shutil.which("tmux") is not None

#: This checkout, for `$PYTHONPATH`. The palette's pane is spawned with
#: `util.self_relaunch_argv`, whose `-P` strips the cwd entry `-m` would otherwise
#: prepend — that flag is #390's fix and must not be weakened to make a test pass, so the
#: checkout is reached the way `tests/test_frame_tmux_integration._importable_env` reaches
#: it: through `$PYTHONPATH`, which `-P` leaves alone.
_REPO_ROOT = Path(__file__).resolve().parents[1]

#: This module's own server, unique per test PROCESS so an interrupted earlier run's socket
#: can never be mistaken for this one's, and so the next run can reap it —
#: `test_frame_overlay_escape_hatch.py`'s rule, now `tests._tmuxreap.name`'s (#564).
SOCKET = _tmuxreap.name("palette-integ")

#: Where tmux puts :data:`SOCKET`'s FILE, computed the way tmux computes it, and only as
#: the FALLBACK for it: :meth:`_ThePalette._teardown_socket` asks the live server first,
#: because tmux is the authority on its own path and this is a COPY of a rule that lives in
#: tmux's source. Nothing is ever asserted about it.
SOCKET_PATH = os.path.join(os.environ.get("TMUX_TMPDIR") or "/tmp",
                           f"tmux-{os.getuid()}", SOCKET)

#: How long a tmux state change gets before this gives up on it. Generous, and spent only
#: on the way to a failure: every wait below returns the instant the state is right.
_DEADLINE = 25.0

#: What a terminal sends for `F2`. SS3, xterm's own form — confirmed against tmux 3.7c on
#: an attached pty that BOTH this and the `\x1b[12~` CSI form reach the root key table, so
#: neither depends on which `TERM` the client attached with. Asserted against
#: `config.FRAME["hotkey"]`'s default by
#: :meth:`_ThePalette.test_the_key_this_module_presses_is_the_key_charter_binds`, so the two
#: cannot drift into a test pressing a key charter does not bind.
_F2 = b"\x1bOQ"

#: What Escape is on the wire, spelled the way :data:`_F2` is. A lone `\x1b` in a
#: source file is a byte the next reader cannot see.
_ESCAPE = b"\x1b"

#: What `overlay.HATCH_KEY` is on the wire. F12 in the same xterm form :data:`_F2` uses —
#: `\x1b[24~` — spelled beside it so a reader can see the two are the same kind of thing.
_HATCH = b"\x1b[24~"

_TERM_CANDIDATES = tuple(dict.fromkeys(
    ([os.environ["TERM"]] if os.environ.get("TERM", "dumb") != "dumb" else [])
    + ["xterm-256color", "screen", "vt100"]))


def _tmux(*args: str, env=None) -> subprocess.CompletedProcess:
    return subprocess.run(["tmux", "-L", SOCKET, *args], capture_output=True, text=True,
                          timeout=20, env=env)


def _await(predicate, timeout: float = _DEADLINE) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(0.05)
    return predicate()


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class _ThePalette(PersonaIso):
    """One frame, one attached client, and the key that opens the palette."""

    #: Recorded into the frame's identity, so a subclass can pin the frame and watch the
    #: workspace rows arrive with their reason instead of arriving available.
    PIN = ""

    def setUp(self) -> None:
        super().setUp()
        v = tmuxctl.version()
        if v is None or v < tmuxctl.FLOOR:
            self.skipTest(f"the frame's floor is tmux {tmuxctl.FLOOR[0]}."
                          f"{tmuxctl.FLOOR[1]}; this machine has {v}")
        # Registered FIRST so it runs LAST — `addCleanup` is LIFO, and every client this
        # test forks onto a pty must be reaped before the server it is attached to goes.
        self.addCleanup(self._teardown_socket)
        (self.tmp / "charter.toml").write_text("")
        for name in ("alpha", "zebra"):
            (config.WORKSPACES_DIR / name).mkdir(parents=True, exist_ok=True)

        self.fid = state.frame_id("palette-integ", os.getpid())
        # `CHARTER_ROOT` has to be in the SERVER's own starting environment: the hotkey
        # bind's `run-shell` carries no `-t`, so what it inherits is whatever started the
        # server (`commands_frame._session_id_env_argv`'s docstring measures this).
        existing = os.environ.get("PYTHONPATH", "")
        self.env = dict(os.environ, CHARTER_ROOT=str(self.tmp),
                        PYTHONPATH=(f"{_REPO_ROOT}{os.pathsep}{existing}" if existing
                                    else str(_REPO_ROOT)))
        self.env.pop("CHARTER_HOME", None)
        started = _tmux("-f", "/dev/null", "new-session", "-d", "-s", self.fid,
                        "-x", "110", "-y", "32", "-P", "-F", "#{pane_id}", "cat",
                        env=self.env)
        self.assertEqual(started.returncode, 0, started.stderr)
        self.harness = started.stdout.strip()

        state.frame_dir(self.fid, create=True)
        state.record_server(self.fid, SOCKET)
        state.record_harness_pane(self.fid, self.harness)
        state.record_identity(self.fid, {"CHARTER_SESSION_ID": self.fid,
                                         "CHARTER_ROOT": str(self.tmp),
                                         "CHARTER_WORKSPACE": self.PIN,
                                         "CHARTER_PERSONA": "",
                                         "CHARTER_HARNESS": ""})
        state.record_workspace(self.fid, "alpha")

        for name, value in (("CHARTER_SESSION_ID", self.fid),
                            (tmuxctl.CHARTER_PY_ENV, sys.executable),
                            ("PYTHONSAFEPATH", "1")):
            r = _tmux("set-environment", "-t", self.fid, name, value)
            self.assertEqual(r.returncode, 0, r.stderr)

        # Charter's OWN frame config, sourced the way a launch sources it — not a
        # hand-written `bind` line standing in for it. A test that re-spelled the bind
        # would be measuring its own copy of charter's answer (#547), and the quoting
        # inside that line is exactly the part worth putting through tmux's real parser.
        conf = str(self.tmp / "frame.conf")
        with open(conf, "w") as fh:
            fh.write(commands_frame.conf_text(hotkey=config.FRAME["hotkey"], mouse=False,
                                              history_limit=100, session=self.fid))
        sourced = _tmux("source-file", conf)
        self.assertEqual(sourced.returncode, 0, sourced.stderr)
        armed = overlay.arm_hatch_argv(SOCKET, harness=self.harness)
        self.assertIsNotNone(armed)
        self.assertEqual(subprocess.run(armed, capture_output=True, timeout=20).returncode,
                         0)

    def _teardown_socket(self) -> None:
        """End the server and take its socket FILE with it, in that order and in ONE
        cleanup — `test_frame_overlay_escape_hatch.py::TheHatch._teardown_socket` verbatim.

        `kill-server` is a signal, not a wait: tmux's `cmd_kill_server_exec` signals ITSELF
        and then answers this client normally, so the listening socket keeps accepting for
        a measured median of 0.4 ms (max 1.3 ms) afterwards. A client that connects in that
        window reads EOF where a reply should be and reports `server exited unexpectedly` —
        the whole of the CI failure at `c735efc`. Unlinking closes the window rather than
        waiting it out.
        """
        said = _tmux("display-message", "-p", "#{socket_path}")
        was_running = said.returncode == 0
        path = said.stdout.strip()
        _tmux("kill-server")
        for candidate in {SOCKET_PATH, path if path.startswith("/") else SOCKET_PATH}:
            try:
                os.unlink(candidate)
            except OSError:
                pass
        if not was_running:
            return
        self.assertTrue(
            path.startswith("/"),
            f"tmux would not say where its socket is — `#{{socket_path}}` gave {path!r}, "
            f"so this teardown is unlinking a guess ({SOCKET_PATH})")
        self.assertFalse(
            os.path.exists(path),
            f"{path} survived this test's teardown, so the next test's `new-session` can "
            f"still reach a server that is exiting")

    def _attach(self) -> int:
        """A real `tmux attach` on a pty. Returns the master fd; the client is this
        process's own child to reap."""
        refusals = []
        for term in _TERM_CANDIDATES:
            pid, fd = pty.fork()
            if pid == 0:
                try:
                    os.environ["TERM"] = term
                    os.environ["CHARTER_ROOT"] = str(self.tmp)
                    os.execvp("tmux", ["tmux", "-L", SOCKET, "attach", "-t", self.fid])
                finally:
                    os._exit(127)
            if _await(lambda: bool(_tmux("list-clients", "-t", self.fid,
                                         "-F", "#{client_name}").stdout.strip()),
                      timeout=10.0):
                self.addCleanup(self._reap, pid, fd)
                return fd
            refusals.append(term)
            self._reap(pid, fd)
        self.skipTest("no tmux client will attach on this machine, and a root key table "
                      "entry needs one — tried TERM=" + ", ".join(refusals))

    @staticmethod
    def _reap(pid: int, fd: int) -> None:
        for call in (lambda: os.kill(pid, 9), lambda: os.waitpid(pid, 0),
                     lambda: os.close(fd)):
            try:
                call()
            except OSError:
                pass

    def _panes(self) -> dict[str, str]:
        out = _tmux("list-panes", "-t", self.fid,
                    "-F", "#{pane_id} #{pane_active}").stdout
        return dict(line.split() for line in out.splitlines() if " " in line)

    def _palette_pane(self) -> str | None:
        """The one pane in this window that is not the harness, or ``None``."""
        others = [p for p in self._panes() if p != self.harness]
        return others[0] if len(others) == 1 else None

    def _open(self) -> tuple[int, str]:
        """Press the hotkey on a real client and wait for the palette's pane to draw."""
        fd = self._attach()
        os.write(fd, _F2)
        self.assertTrue(_await(lambda: self._palette_pane() is not None),
                        f"the hotkey opened no pane: {self._panes()}")
        pane = self._palette_pane()
        self.assertTrue(
            _await(lambda: "detach" in _tmux("capture-pane", "-p", "-t", pane).stdout),
            f"the palette's pane never drew its rows:\n"
            f"{_tmux('capture-pane', '-p', '-t', pane).stdout}")
        return fd, pane

    def _screen(self, pane: str) -> str:
        return _tmux("capture-pane", "-p", "-t", pane).stdout

    def _await_screen(self, pane: str, text: str, *, present: bool = True) -> None:
        """Wait for *text* to be (or stop being) on *pane*, and say what was there if not.

        **Every content assertion here waits, and that is not flake-proofing.** The pane
        is split five rows tall and zoomed on the very next tmux command
        (`overlay.modal_argvs`), so the palette's FIRST paint is a five-row paint and the
        rows past the fourth genuinely are not on screen yet. What redraws it is the
        resize the surface notices on its next tick (`palette.TICK`) — the same mechanism
        that makes `window-resized` work at all, exercised here by construction rather
        than by a separate test. Asserting without waiting would be asserting against a
        paint charter has already replaced.
        """
        self.assertTrue(_await(lambda: (text in self._screen(pane)) == present),
                        f"{text!r} {'never appeared on' if present else 'never left'} "
                        f"the palette:\n{self._screen(pane)}")


class ThePaletteOpensAndRuns(_ThePalette, unittest.TestCase):
    """Step 1, whole: `F2` opens a palette listing actions, typing filters, Enter runs."""

    def test_the_key_this_module_presses_is_the_key_charter_binds(self):
        """`\\x1bOQ` is F2. If `[frame] hotkey`'s default ever moves, this test must move
        with it — otherwise every assertion below would be pressing a key charter does not
        bind and measuring a palette that simply never opened."""
        self.assertEqual(config.FRAME["hotkey"], "F2")

    def test_the_hotkey_opens_a_palette_listing_the_frames_actions(self):
        _, pane = self._open()
        for expected in ("detach", "density: minimal", "workspace: alpha — pick another",
                         "persona — pick one"):
            self._await_screen(pane, expected)

    def test_typing_narrows_the_list_to_what_matches(self):
        fd, pane = self._open()
        self._await_screen(pane, "workspace: alpha")
        os.write(fd, b"workspace")
        self._await_screen(pane, "detach", present=False)
        screen = self._screen(pane)
        self.assertIn("workspace: alpha", screen, screen)
        self.assertIn("workspace", screen.splitlines()[0],
                      "what the operator typed is not on the header")

    def test_enter_opens_the_picker_in_this_same_pane_and_choosing_moves_the_frame(self):
        """**Task 6's step 1, end to end, with real keys.** The palette's workspace row is
        a doorway: Enter replaces the surface in the pane the operator is already looking
        at — no second pane, no second charter process, nothing to race
        `_close_palette` — and the name chosen there switches the frame.

        Asserted on the frame's own record rather than on anything drawn, because "the
        frame moved" is `state.workspace_for`'s rungs (#411) and not a repaint.
        """
        fd, pane = self._open()
        self._await_screen(pane, "workspace: alpha")
        self.assertEqual(state.frame_workspace(self.fid), "alpha")
        self.assertEqual(self._palette_pane(), pane)
        os.write(fd, b"workspace\r")
        # The picker: the names alone, no `workspace:` prefix and no `detach` beside them.
        self._await_screen(pane, "detach", present=False)
        self._await_screen(pane, "zebra")
        self.assertEqual(self._palette_pane(), pane,
                         "the picker opened a second pane instead of reusing this one")
        os.write(fd, b"zebra\r")
        self.assertTrue(_await(lambda: state.frame_workspace(self.fid) == "zebra"),
                        f"the chosen name never switched the frame: workspace is still "
                        f"{state.frame_workspace(self.fid)!r}")

    def test_typing_a_name_switches_the_frame_with_no_doorway_in_between(self):
        """**Task 8, end to end, with real keys, and the keystroke claim is the point.**

        The case above spends `F2`, Enter on the doorway, the name, Enter. This one spends
        `F2`, the name, Enter — one keypress fewer, on the thing an operator does most —
        and it is the same pane, the same surface and the same switch underneath.

        The kind is asserted on the drawn row rather than on the `Row`: `workspace` beside
        `zebra` is what tells it from a persona of the same name, and it is the note
        column, which `overlay.Surface.render` lays out against the pane's real width.
        """
        fd, pane = self._open()
        self._await_screen(pane, "workspace: alpha")
        self.assertEqual(state.frame_workspace(self.fid), "alpha")
        was = state.version(self.fid)
        os.write(fd, b"zebra")
        self._await_screen(pane, "detach", present=False)
        screen = self._screen(pane)
        # Past the header, which carries what was typed and would answer for the row.
        row = next((ln for ln in screen.splitlines()[1:] if "zebra" in ln), "")
        self.assertIn("workspace", row,
                      f"the row does not say what kind of thing `zebra` is:\n{screen}")
        self.assertEqual(self._palette_pane(), pane,
                         "typing a name opened a second pane")
        os.write(fd, b"\r")
        self.assertTrue(_await(lambda: state.frame_workspace(self.fid) == "zebra"),
                        f"the typed name never switched the frame: workspace is still "
                        f"{state.frame_workspace(self.fid)!r}")
        self.assertTrue(_await(lambda: state.version(self.fid) != was),
                        "the frame was never bumped, so no panel repaints")

    def test_nothing_typed_lists_no_names_at_all(self):
        """The other half, and the one a display test cannot fake: the palette that just
        opened is the doorways and the actions, with `zebra` nowhere on it. A palette that
        listed every name up front would draw it here."""
        _, pane = self._open()
        self._await_screen(pane, "density: minimal")
        screen = self._screen(pane)
        self.assertNotIn("zebra", screen, screen)
        self.assertIn("workspace: alpha", screen, screen)

    def test_the_picker_bumps_the_frame_so_every_panel_repaints(self):
        """#411/#412's half of step 1. A pointer some panels may read is the bug; the
        version moving is what makes a panel's poll loop redraw against the new plane."""
        fd, pane = self._open()
        self._await_screen(pane, "workspace: alpha")
        was = state.version(self.fid)
        os.write(fd, b"workspace\r")
        self._await_screen(pane, "zebra")
        os.write(fd, b"zebra\r")
        self.assertTrue(_await(lambda: state.version(self.fid) != was),
                        "the frame was never bumped, so no panel repaints")

    def test_the_pane_comes_back_to_the_harness_when_the_palette_closes(self):
        fd, pane = self._open()
        self.assertTrue(_await(lambda: self._panes().get(pane) == "1"),
                        "the palette's pane never became the active one")
        os.write(fd, b"\x1b")
        self.assertTrue(_await(lambda: self._palette_pane() is None),
                        f"the palette's pane outlived it: {self._panes()}")
        self.assertEqual(self._panes().get(self.harness), "1",
                         "the harness is not the active pane again")

    def test_the_hatch_is_disarmed_when_the_palette_closes(self):
        """`close_argvs`' third command, which only runs because the three go out as ONE
        tmux invocation — this process is what the second one kills. Sent separately it
        never ran, measured, and the window option went on naming a pane that is gone."""
        fd, pane = self._open()
        self.assertTrue(_await(
            lambda: pane in _tmux("show-options", "-w", "-t", self.harness, "-v",
                                  overlay.HATCH_OPTION).stdout))
        os.write(fd, b"\x1b")
        self.assertTrue(_await(lambda: self._palette_pane() is None))
        self.assertEqual(
            _tmux("show-options", "-w", "-t", self.harness, "-v",
                  overlay.HATCH_OPTION).stdout.strip(),
            overlay.hatch_command(harness=self.harness),
            "the hatch still names the pane the palette was drawn in")


class ThereIsNeverMoreThanOnePalettePane(_ThePalette, unittest.TestCase):
    """#739, and the invariant is stated as a COUNT rather than per route.

    The report is a double `F2`: the second press split a second overlay off the harness,
    Escape closed the one holding the keyboard, and the other stayed — a blank five-row
    pane holding a live Python process, six rows off the harness, for the life of the
    frame. `F12` is `select-pane` and did not clear it; a resize did not; no palette row
    did. Only tmux's own prefix + `x`, which `charter frame` neither binds nor documents.

    **Counted, not route-checked.** Making the double press safe and calling the class
    closed is how this repository has three times ended up with an assertion sitting on
    the path that already satisfied it (#682, #683, #689) — and the double press is not
    the only route: any second open against a live palette does it. The cases below press
    the key, run the command by hand, and press the key three times in a row, and each one
    asks the same question of tmux: how many panes on this window carry the overlay mark.
    """

    def _overlays(self) -> list[str]:
        """Every pane on the frame's window that charter marked as an overlay.

        Read back through `overlay.live_argv`'s own format, so this asks the question the
        production sweep asks. A test that counted "panes that are not the harness" would
        also count a panel, and would go green over a sweep that killed the mark and left
        the pane.
        """
        argv = overlay.live_argv(SOCKET, harness=self.harness)
        self.assertIsNotNone(argv)
        return list(overlay.live_panes(
            subprocess.run(argv, capture_output=True, text=True, timeout=20).stdout))

    def _await_drawn(self, *, replacing: str = "") -> str:
        """Wait for exactly one overlay pane — a NEW one when *replacing* names the old —
        drawn, and answer its id.

        Three halves, and each one cost a red run to learn.

        **Exactly one** is the invariant every case here is about.

        **Not the one before it.** `F2` fires a `run-shell`, so the press returns long
        before charter has swept, split and drawn; for a measured moment the answer to
        "which panes are overlays" is still the FIRST palette, alone and correct-looking.
        A wait that only counted was satisfied by that moment, and the Escape that followed
        went to a palette charter was in the middle of replacing — which read as a leak and
        was a race in this test. Measured, with the panes dumped at each step: after the
        second press the listing still said `%1`, and `%2` did not appear until after the
        Escape had already been sent.

        **Drawn.** The palette puts its own tty in raw mode on the way in
        (`palette.own_the_tty`), and until it has, a byte written to the client sits in
        canonical mode waiting for a newline — so an Escape sent to a pane that exists but
        has not drawn looks exactly like a palette that ignored Escape.
        """
        def replaced() -> bool:
            found = self._overlays()
            return len(found) == 1 and found[0] != replacing

        self.assertTrue(_await(replaced),
                        f"{self._overlays()} — not exactly one palette"
                        + (f" other than {replacing}" if replacing else ""))
        pane = self._overlays()[0]
        self.assertTrue(
            _await(lambda: "detach" in self._screen(pane)),
            f"the palette's pane never drew its rows:\n{self._screen(pane)}")
        return pane

    def test_one_press_marks_exactly_one_pane(self):
        """The control, and it is load-bearing: every count below is zero for a palette
        that is never marked at all, so a sweep that found nothing would pass them."""
        _, pane = self._open()
        self.assertEqual(self._overlays(), [pane])

    def test_a_second_press_replaces_the_palette_instead_of_stacking_one(self):
        fd, first = self._open()
        os.write(fd, _F2)
        second = self._await_drawn(replacing=first)
        self.assertNotEqual(second, first, "the first palette is the one still open")

    def test_the_harness_gets_its_rows_back_after_ONE_open_and_close(self):
        """The control for the case below, and the one that says whose defect a row leak
        is. If a single open/close already loses rows, the reopen is not what lost them."""
        def rows() -> int:
            return int(_tmux("display-message", "-p", "-t", self.harness,
                             "#{pane_height}").stdout.strip() or 0)

        fd = self._attach()
        self.assertTrue(_await(lambda: rows() > 0))
        before = rows()
        os.write(fd, _F2)
        self._await_drawn()
        os.write(fd, _ESCAPE)
        self.assertTrue(_await(lambda: self._overlays() == []))
        self.assertTrue(_await(lambda: rows() == before),
                        f"the harness kept {rows()} rows of the {before} it had")

    def test_the_harness_gets_its_rows_back_when_the_reopened_palette_closes(self):
        """The half an operator actually sees. The leak was invisible — a blank gap above
        the repo table's rule reads as empty terminal — so what it cost was rows: the
        harness went from 24 to 18 and stayed there. Escape must give all of them back.
        """
        def rows() -> int:
            return int(_tmux("display-message", "-p", "-t", self.harness,
                             "#{pane_height}").stdout.strip() or 0)

        fd = self._attach()
        self.assertTrue(_await(lambda: rows() > 0))
        before = rows()
        os.write(fd, _F2)
        first = self._await_drawn()
        os.write(fd, _F2)
        self._await_drawn(replacing=first)
        os.write(fd, _ESCAPE)
        self.assertTrue(_await(lambda: self._overlays() == []),
                        f"a palette survived Escape: {self._panes()}")
        self.assertTrue(_await(lambda: rows() == before),
                        f"the harness kept {rows()} rows of the {before} it had")

    def test_three_presses_still_leave_one(self):
        """The count is an invariant and not a special case for two. Each press closes
        what is open and draws a fresh one, so the answer is the same however many times
        an operator leans on a key that seems not to have registered."""
        fd, pane = self._open()
        for _ in range(2):
            os.write(fd, _F2)
            pane = self._await_drawn(replacing=pane)
        self.assertEqual(self._overlays(), [pane])

    def test_the_escape_hatch_still_closes_the_palette_a_reopen_drew(self):
        """The hatch is armed per palette, and a reopen must leave the LIVE one armed.

        The worry is a swept process running `_close_palette` from its `finally` on the
        way out: that re-arms the hatch with no overlay, and `F12` would then select the
        harness and leave the new palette standing — the very leak this is fixing, one
        keypress later. It cannot happen, because `kill-pane` terminates a Python process
        without running its `finally` (measured directly, against a process whose
        `finally` writes a file: the file is never written). This is that fact asserted
        through the surface rather than restated as a comment.
        """
        fd, first = self._open()
        os.write(fd, _F2)
        self._await_drawn(replacing=first)
        os.write(fd, _HATCH)
        self.assertTrue(_await(lambda: self._overlays() == []),
                        f"the hatch left the reopened palette standing: {self._panes()}")
        self.assertTrue(
            _await(lambda: self._panes().get(self.harness) == "1"),
            f"the hatch did not put the operator back in the harness: {self._panes()}")

    def test_the_sweep_cannot_reach_another_chats_palette(self):
        """Scoped to the frame's own window, and this is the case that says so.

        Every chat on charter's socket is a window on one server, and each has its own
        harness and its own palette. A sweep that asked `list-panes -a` would find a
        palette another chat has open — a different operator's screen, in the ordinary
        two-agent case — and kill it. Measured on 3.7c and on the 3.2 floor,
        `list-panes -t <a pane id>` resolves to that pane's window and lists only its
        panes; this is that property asserted through the production call rather than
        through the format string it is spelled with.

        The pane in the other window is marked BY HAND, because what is under test is the
        reach of the sweep and not how a real palette gets there. A second real frame
        would put a second `charter frame-palette` between the test and the one question
        it is asking.
        """
        other = _tmux("new-window", "-t", self.fid, "-d", "-P", "-F", "#{pane_id}",
                      "cat", env=self.env)
        self.assertEqual(other.returncode, 0, other.stderr)
        elsewhere = other.stdout.strip()
        self.addCleanup(lambda: _tmux("kill-pane", "-t", elsewhere))
        marked = _tmux("set-option", "-p", "-t", elsewhere,
                       overlay.OVERLAY_OPTION, "1")
        self.assertEqual(marked.returncode, 0, marked.stderr)

        fd, first = self._open()
        os.write(fd, _F2)
        self._await_drawn(replacing=first)
        alive = _tmux("list-panes", "-a", "-F", "#{pane_id}").stdout.split()
        self.assertIn(elsewhere, alive,
                      "the sweep killed a palette open on another chat's window")

    def test_the_route_is_the_open_and_not_the_key(self):
        """`F2` is the reachable route and it is not the only one: `charter frame-palette`
        typed in the harness, and a second client attached to the same session pressing
        the key, open a palette by the same call. Asserted against that call, so the
        invariant belongs to opening rather than to one key.
        """
        _, first = self._open()
        r = subprocess.run(
            [sys.executable, "-m", "charter", "frame-palette"],
            capture_output=True, text=True, timeout=60,
            env=dict(self.env, CHARTER_SESSION_ID=self.fid,
                     **{tmuxctl.CHARTER_PY_ENV: sys.executable}))
        self.assertEqual(r.returncode, 0, r.stderr)
        second = self._await_drawn(replacing=first)
        self.assertNotEqual(second, first,
                            "a hand-run palette stacked on the key's")


class AnUnavailableActionIsDrawnWithItsReason(_ThePalette, unittest.TestCase):
    """Step 4, on the plan's own example, against a real frame.

    `$CHARTER_WORKSPACE` was set at launch, so it is in every panel pane's environment and
    nothing charter writes can outrank it. The rows stay, and what they carry is the
    sentence `switch.to_workspace` would have refused with.
    """

    PIN = "alpha"

    def test_the_row_is_listed_and_says_why_it_cannot_run(self):
        _, pane = self._open()
        self._await_screen(pane, "workspace: alpha")
        self._await_screen(pane, "cannot switch: $CHARTER_WORKSPACE")

    def test_choosing_it_opens_no_picker_changes_nothing_and_the_frame_says_so(self):
        """A pinned frame does not get a list of names it cannot switch to. The row is
        still there, still says why, and Enter on it closes the palette having moved
        nothing — the reason goes to the operator's own screen on the way out."""
        fd, pane = self._open()
        self._await_screen(pane, "workspace: alpha")
        os.write(fd, b"workspace\r")
        self.assertTrue(_await(lambda: self._palette_pane() is None),
                        "the palette never closed")
        time.sleep(0.5)
        self.assertEqual(state.frame_workspace(self.fid), "alpha",
                         "a refused switch must not move the frame")


class ATmuxFormatInAMessageIsInert(_ThePalette, unittest.TestCase):
    """The CRITICAL finding, carried onto the surface that still has it.

    `display-menu`'s item names were formats, and so is a `display-message` argument —
    tmux's own docs say so. charter no longer puts a switch outcome through either
    (#729: it goes to the frame's own attention row, where no tmux parser is in the path),
    so what this class proves is no longer "the escaping charter applies is necessary" but
    the fact underneath it: **an unescaped `#` in a tmux message argument really is read by
    tmux's parser rather than drawn.** That is what makes `inert_format` the right thing
    for the next surface that hands tmux a string built from a name, and what makes the
    narrower per-value guards charter uses elsewhere load-bearing rather than decorative.
    `tests/test_frame_menu.py::LabelSafety` proved the ESCAPING at argv level and moved
    whole to `test_frame_tmuxctl.py::InertFormat`; this is the half a unit test cannot
    show, and it is asserted against tmux rather than against charter.

    **`#{...}`, not `#(...)`, and that is a measurement rather than a preference.** On tmux
    3.7c a `#(shell command)` in a `display-message` substitutes to EMPTY and the job was
    never observed to run — with a client attached, without one, and from `status-right`
    too — so a `#(...)` canary here would go green whether or not the guard existed, which
    is the "passes for the wrong reason" shape this whole file is written against.
    `#{...}` substitution is real and immediate: measured below, in both directions, in the
    same run.

    What that leaves is still the whole property. An unescaped `#` in a name charter puts
    on screen is read by tmux's parser rather than drawn, and both spellings share one
    escape; the guard is `#` -> `##` either way, and `#{...}` is what can be demonstrated
    to be doing something today.
    """

    def _said(self, text: str) -> str:
        """*text* through tmux's own format parser, exactly as `display-message` reads it.

        `-p` so the expansion comes back rather than landing on a status line nothing can
        read — the parser is the same one; only the destination differs.
        """
        r = _tmux("display-message", "-p", "-t", self.fid, text)
        self.assertEqual(r.returncode, 0, r.stderr)
        return r.stdout

    def test_an_unescaped_format_really_is_expanded_by_tmux(self):
        """The control. Without it the test below proves only that `inert_format` changes
        a string, not that the string it changes would have been read as a format."""
        said = self._said("x #{session_name} y")
        self.assertIn(self.fid, said, said)
        self.assertNotIn("#{session_name}", said, said)

    def test_what_say_on_screen_sends_is_drawn_and_never_read(self):
        said = self._said(tmuxctl.inert_format("x #{session_name} y"))
        self.assertIn("#{session_name}", said, said)
        self.assertNotIn(self.fid, said, said)

    def test_a_leading_hyphen_does_not_take_the_whole_command_with_it(self):
        """Measured against a real attached client on the menu this replaces: tmux reads a
        value beginning with `-` as an unrecognised FLAG of its own and refuses the whole
        command, rc 1. So the guard is asserted against tmux itself, not against its docs."""
        refused = _tmux("display-message", "-p", "-t", self.fid, "-my-branch")
        self.assertNotEqual(refused.returncode, 0,
                            "tmux accepted a leading `-` — this guard is now pinned to "
                            "a behaviour tmux no longer has")
        ok = self._said(tmuxctl.inert_format("-my-branch"))
        self.assertIn("my-branch", ok, ok)


class TheCliAcceptsWhatTheBindSends(unittest.TestCase):
    """The bind stores an argv; a CLI that does not accept it is a hotkey that silently
    exits 2 inside a `run-shell`, with nothing anywhere to print the reason.

    Parsed from `conf_text`'s own text and from `util.self_relaunch_argv`'s own output,
    never from a hand-written copy of either.
    """

    def test_the_bind_dispatches_to_the_palette(self):
        from charter.cli import build_parser
        ns = build_parser().parse_args(["frame-palette", "/dev/ttys020"])
        self.assertEqual(ns.client, "/dev/ttys020")
        self.assertFalse(ns.pane)
        self.assertIs(ns.func, commands_frame.cmd_palette)

    def test_the_panes_own_argv_dispatches_to_the_palette_too(self):
        from charter.cli import build_parser
        argv = util.self_relaunch_argv("frame-palette", "/dev/ttys020", "--pane")
        ns = build_parser().parse_args(argv[argv.index("charter") + 1:])
        self.assertTrue(ns.pane)
        self.assertIs(ns.func, commands_frame.cmd_palette)

    def test_the_bind_text_names_the_command_the_cli_registers(self):
        """The two halves of the same fact, joined: whatever `conf_text` puts in the bind
        must be a word `build_parser` answers to."""
        from charter.cli import build_parser
        text = commands_frame.conf_text(hotkey="F2", mouse=False, history_limit=100,
                                        session="f-1")
        bind = [ln for ln in text.splitlines() if ln.startswith("bind -n F2 ")][0]
        word = bind.split("-m charter ")[1].split()[0]
        build_parser().parse_args([word, "x"])       # raises SystemExit if unregistered


if __name__ == "__main__":                          # pragma: no cover
    unittest.main()
