"""**A one-chat confirmation gives the window back; the palette and the whole-plane one
keep it — #921, narrowed by #927.**

*"no any modal/drawer for confirmation"*, reported about a surface that has had one since
it was written. `overlay.modal_argvs` ends in `resize-pane -Z`, so every surface the
overlay pane holds takes the entire window — and a two-row question about one chat arriving
full-screen does not read as something that opened over the frame you were looking at. It
reads as somewhere else.

**The size of the surface matches the size of the consequence, and that rule has two
directions to hold — so this module pins both.** `chat: close` is a drawer
(:meth:`TheConfirmationGivesTheWindowBack.test_the_close_doorway_unzooms_the_pane_it_is
_drawn_in`). `charter: quit` is not
(:meth:`TheConfirmationGivesTheWindowBack.test_the_quit_doorway_keeps_the_whole_pane`),
because it stops EVERY harness on the plane and its confirmation is the one place an
operator sees that — every chat, in every workspace, and which of them can resume. In five
rows that is two at a time, scrolled, and scrolling a destructive list is how you answer it
without reading it. A module asserting only one direction would leave the other free to
drift into it silently, which is the whole shape of #927: #921 pinned quit as a drawer, and
that assertion was the thing to change.

**Dropping the zoom does not hide it.** `overlay.open_argv` splits `-v -l 5`, so the pane
goes back to five rows at the bottom of the window with the frame drawn above it, still the
ACTIVE pane and still the one whose mouse request the terminal follows.

**#739's five-row pane is not this one, and the difference is `active`.** That defect was a
second overlay ORPHANED by a double `F2` — blank, unfocused, holding a live Python process
nothing could reach. This pane is the focused one. :class:`ARealTmuxUnzoomsTheDrawer`
measures both halves on a real server rather than arguing them: the pane keeps its five
rows and its focus, and the outer terminal still receives this pane's own
``\\x1b[?1006h\\x1b[?1000h``.

**Only the one-chat confirmation.** The palette lists every action, every doorway and every
name an operator types towards; it scrolls, has no row cap by design, and earns the rows.
So does a picker, which is a list of names of unbounded length. So does quit's warning.
What gives the window up is the one surface that is a single question about a single chat,
with the answer at the top and two lines under it.

**And both routes to THAT, through one call.** `F2 → chat: close` and a right press on a
tab open the same `leave.confirm_rows` over the same plan; a route that unzoomed and a
route that did not would be two confirmations wearing one name. The tab strip only ever
opens close, so it only ever gets a drawer — and it gets one by inheriting the rule rather
than by holding a copy of it.
"""

from __future__ import annotations

import fcntl
import os
import shutil
import struct
import subprocess
import termios
import time
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame
from charter.frame import choose, leave, overlay, palette, state, tabmenu, tmuxctl

from tests import _tmuxreap
from tests._isolation import PersonaIso

_HAS_TMUX = shutil.which("tmux") is not None

#: The socket this module's real server runs on. `_tmuxreap.name` and never a spelling of
#: its own: a socket the reaper does not recognise is a socket that leaks for the life of
#: the machine, and that module records 497 of them in two days.
SOCKET_NAME = _tmuxreap.name("i921-drawer")


class TheUnzoomIsOneGuardedCommand(unittest.TestCase):
    """`overlay.unzoom_argv` — what it sends, and what it refuses to send."""

    def test_it_asks_before_it_toggles(self):
        """**`resize-pane -Z` is a TOGGLE and there is no unzoom flag in any tmux.** A
        bare toggle aimed at a pane that had already been unzoomed — an operator's own
        prefix key on charter's shared private server — would zoom the confirmation back
        over the whole window, which is the state this exists to leave."""
        argv = overlay.unzoom_argv("charter", overlay_pane="%7")
        self.assertIsNotNone(argv)
        self.assertIn("if-shell", argv)
        self.assertIn("-F", argv)
        self.assertIn(overlay.ZOOMED_FLAG, argv)
        self.assertEqual(argv[-1], "resize-pane -Z -t %7")

    def test_it_names_the_pane_on_both_halves(self):
        """The condition is read `-t` the pane and the command targets it too. A
        `resize-pane -Z` with no `-t` acts on the CURRENT pane, which `overlay.sweep_argv`
        records the cost of: on this server that is the operator's harness."""
        argv = overlay.unzoom_argv("charter", overlay_pane="%12")
        self.assertEqual(argv.count("%12"), 1, argv)
        self.assertIn("%12", argv[-1])

    def test_a_pane_it_cannot_spell_gets_no_command_at_all(self):
        """Every builder in `frame/overlay.py` degrades this way, and this one has the
        sharpest reason to: a `-t` tmux cannot resolve is a command that resizes something
        else."""
        for bad in ("", "0", "%", "pane", "%1;kill-server", "%1 %2", None):
            self.assertIsNone(overlay.unzoom_argv("charter", overlay_pane=bad or ""),
                              repr(bad))

    def test_the_palette_still_takes_the_whole_window(self):
        """The control, and the half of #921 that is deliberately NOT changed. If the zoom
        left `modal_argvs`, every surface would be a drawer and the palette would lose the
        rows it scrolls."""
        cmds = overlay.modal_argvs("charter", harness="%0", overlay_pane="%7")
        self.assertEqual(cmds[-1][-3:], ["-Z", "-t", "%7"])


class _AFrameWithChatsToClose(PersonaIso):
    """One frame on an isolated plane, with the tmux round trips observable."""

    FID = "alpha.1"
    SERVER = "charter-i921"

    def setUp(self) -> None:
        super().setUp()
        for fid, ws in ((self.FID, "alpha"), ("alpha.2", "alpha")):
            state.frame_dir(fid, create=True)
            state.record_server(fid, self.SERVER)
            state.record_workspace(fid, ws)
            state.record_harness_pane(fid, "%1")
            state.record_identity(fid, {"CHARTER_HARNESS": "claude-code",
                                        "CHARTER_WORKSPACE": "", "CHARTER_PERSONA": ""})
        self.live = ({self.FID, "alpha.2"},
                     {self.SERVER: {self.FID: "@0", "alpha.2": "@1"}},
                     set())
        self.ran = []
        self.enterContext(mock.patch.object(
            commands_frame, "_plane_live", return_value=self.live))
        self.enterContext(mock.patch.object(
            commands_frame, "_plane_servers", return_value=(self.SERVER,)))
        self.enterContext(mock.patch.object(
            tmuxctl, "run",
            side_effect=lambda _what, argv, **kw: self.ran.append(argv)
            or SimpleNamespace(stdout="", stderr="", returncode=0)))

    def _resizes(self):
        """Every `resize-pane -Z` this test asked tmux for, whichever command carries it."""
        return [a for a in self.ran if any("resize-pane -Z" in w or w == "-Z" for w in a)]


class TheConfirmationGivesTheWindowBack(_AFrameWithChatsToClose, unittest.TestCase):
    """`commands_frame._picker` — the `F2` route."""

    def _open(self, row):
        return commands_frame._picker(row, self.FID, [], socket=self.SERVER, pane="%7")

    def test_the_close_doorway_unzooms_the_pane_it_is_drawn_in(self):
        """**Half of #927's rule, and the half #921 was reported for.** `chat: close` is
        two rows about one chat — the confirming row and the chat `leave.plan(only=…)`
        names — so the whole consequence is on screen in five rows, and taking the window
        for it read as somewhere else."""
        surface = self._open(leave.open_rows(self.FID)[1])
        self.assertIsNotNone(surface, "the close doorway opened nothing")
        self.assertEqual(self._resizes(),
                         [overlay.unzoom_argv(self.SERVER, overlay_pane="%7")])

    def test_the_quit_doorway_keeps_the_whole_pane(self):
        """**The other half, and the one that has to be asserted separately — #927.**

        The size of the surface matches the size of the consequence. `charter: quit` stops
        every harness on the plane, and this surface is the one place an operator sees the
        whole blast radius: every chat, in every workspace, and which of them can resume.
        Five rows draws two of them at a time under a heading saying how many there are —
        and scrolling a destructive list is how an operator answers it without reading it.

        Pinned facing the opposite way from
        :meth:`test_the_close_doorway_unzooms_the_pane_it_is_drawn_in` on purpose. #921
        asserted quit was a drawer too and the assertion was what made that a decision
        rather than an accident; with only one direction pinned, the other is free to drift
        into it and no test says so.
        """
        surface = self._open(leave.open_rows(self.FID)[0])
        self.assertIsNotNone(surface, "the quit doorway opened nothing")
        self.assertEqual(self._resizes(), [], self.ran)

    def test_the_two_doorways_are_told_apart_by_the_verb_and_not_by_the_row_count(self):
        """**A plane with one chat on it does not make quit a drawer.** The rule is what
        the verb CAN reach, not what it happens to reach on this plane at this moment: an
        operator who learns that quit takes the pane has learned something that is true
        every time, and a surface that changed shape with the plane's size would teach them
        nothing they could rely on. `_as_a_drawer` reads the verb off the surface's label,
        which is why."""
        self.live[0].discard("alpha.2")
        self.assertIsNotNone(self._open(leave.open_rows(self.FID)[0]))
        self.assertEqual(self._resizes(), [], self.ran)

    def test_a_name_picker_keeps_the_whole_pane(self):
        """A picker is a list of names of unbounded length — forty workspaces on this
        project's own plane — and it scrolls. Five rows would be three names at a time."""
        row = choose.open_rows(self.FID)[0]
        self.assertIsNotNone(self._open(row), "the picker doorway opened nothing")
        self.assertEqual(self._resizes(), [], self.ran)

    def test_a_row_that_opens_nothing_sends_no_command(self):
        """`_as_a_drawer` takes the ``None`` too, which is what makes it one call rather
        than an `if` at two call sites — and what stops an ordinary action row paying a
        tmux round trip on its way past."""
        self.assertIsNone(self._open(
            palette.overlay.Row(id="frame.detach", title="detach — leave it running")))
        self.assertEqual(self.ran, [])

    def test_a_pane_charter_cannot_name_leaves_the_surface_zoomed(self):
        """Never a refusal: a hand-typed `charter frame-palette --pane` has no
        `$TMUX_PANE`, and what it gets is the surface this shipped as rather than a
        `resize-pane` aimed at whatever pane tmux thinks is current."""
        surface = commands_frame._picker(leave.open_rows(self.FID)[1], self.FID, [],
                                         socket=self.SERVER, pane="")
        self.assertIsNotNone(surface)
        self.assertEqual(self.ran, [])


class TheTabMenuRouteIsTheSameDrawer(_AFrameWithChatsToClose, unittest.TestCase):
    """`frame/tabmenu.py` — the right-press route, and now the `-` on the strip.

    Two routes to `leave.confirm_rows` that must not look different, which is the same
    reason `tabmenu.confirm_rows` builds `leave`'s own rows rather than enumerating its
    own.
    """

    def _drive(self, pick):
        chosen = []

        def _own(surface, *, then=None, **kw):
            row = next(r for r in surface.rows if pick(r))
            nxt = then(row) if then is not None else None
            chosen.append(nxt)
            return None

        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": self.FID,
                                          "TMUX_PANE": "%7"}), \
                mock.patch.object(palette, "own_the_tty", side_effect=_own), \
                mock.patch.object(commands_frame, "_close_palette"), \
                mock.patch.object(commands_frame, "_say_on_screen"):
            tabmenu.draw(SimpleNamespace(tab="alpha.2"))
        return chosen[0]

    def test_the_menus_close_doorway_unzooms(self):
        nxt = self._drive(lambda r: r.id == tabmenu.CLOSE_ID)
        self.assertIsNotNone(nxt, "the doorway opened nothing")
        self.assertEqual(self._resizes(),
                         [overlay.unzoom_argv(self.SERVER, overlay_pane="%7")])

    def test_the_transcript_row_does_not(self):
        """`_as_a_drawer`'s ``None`` passthrough, at the one call site where a row that
        opens nothing is the ordinary case."""
        self.assertIsNone(self._drive(lambda r: r.id == tabmenu.TRANSCRIPT_ID))
        self.assertEqual(self._resizes(), [], self.ran)


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class ARealTmuxUnzoomsTheDrawer(unittest.TestCase):
    """**The two facts #739's counter-evidence turns on, measured on a real server.**

    A five-row overlay pane was a defect once, and this is what says it is not one here:
    that pane was orphaned and unfocused, and this one is neither. What is asserted is what
    the operator would see — five rows, the frame above them — and what the pointer needs:
    the pane is still `active`, so its own mouse request is still the one tmux propagates.
    """

    ROWS, COLS = 30, 100

    def setUp(self) -> None:
        self.addCleanup(self._teardown)
        self._tmux("new-session", "-d", "-s", "t",
                   "-x", str(self.COLS), "-y", str(self.ROWS), "sleep 600")
        self.harness = self._tmux("list-panes", "-t", "t",
                                  "-F", "#{pane_id}").stdout.strip()
        self.overlay = self._tmux(
            "split-window", "-t", self.harness, "-v", "-l", str(overlay._SPLIT_ROWS),
            "-P", "-F", "#{pane_id}", "--", "sleep", "600").stdout.strip()

    def _tmux(self, *args) -> subprocess.CompletedProcess:
        return subprocess.run(["tmux", "-L", SOCKET_NAME, *args],
                              capture_output=True, text=True, timeout=30)

    def _teardown(self) -> None:
        """`kill-server` on this socket by NAME, then the socket file — never a sweep over
        anything matching a pattern. `tests/_tmuxreap` records why the file has to go too:
        `kill-server` leaves it behind, one per run."""
        self._tmux("kill-server")
        (Path("/tmp") / f"tmux-{os.getuid()}" / SOCKET_NAME).unlink(missing_ok=True)

    def _panes(self) -> dict:
        out = self._tmux("list-panes", "-t", "t", "-F",
                         "#{pane_id} #{pane_height} #{pane_active} "
                         "#{window_zoomed_flag}").stdout
        return {f[0]: tuple(f[1:]) for f in (line.split() for line in out.splitlines())}

    def _run(self, argv) -> None:
        self.assertIsNotNone(argv)
        subprocess.run(argv, capture_output=True, text=True, timeout=30, check=True)

    def test_the_pane_keeps_its_five_rows_and_its_focus(self):
        for cmd in overlay.modal_argvs(SOCKET_NAME, harness=self.harness,
                                       overlay_pane=self.overlay):
            self._run(cmd)
        zoomed = self._panes()
        self.assertEqual(zoomed[self.overlay], (str(self.ROWS), "1", "1"),
                         "the palette is not zoomed over the window")

        self._run(overlay.unzoom_argv(SOCKET_NAME, overlay_pane=self.overlay))
        drawer = self._panes()
        self.assertEqual(drawer[self.overlay],
                         (str(overlay._SPLIT_ROWS), "1", "0"),
                         "the confirmation did not become a five-row drawer")
        height, active, _zoom = drawer[self.harness]
        self.assertEqual(active, "0", "the drawer gave the keyboard back to the harness")
        self.assertGreater(int(height), 1,
                           "#739's pane: the frame above the drawer is not drawn")

    def test_it_is_idempotent_because_the_toggle_is_guarded(self):
        """`resize-pane -Z` is a toggle, so an unguarded second send would zoom the
        confirmation back over the window — the exact state it exists to leave."""
        for cmd in overlay.modal_argvs(SOCKET_NAME, harness=self.harness,
                                       overlay_pane=self.overlay):
            self._run(cmd)
        for _ in range(3):
            self._run(overlay.unzoom_argv(SOCKET_NAME, overlay_pane=self.overlay))
        self.assertEqual(self._panes()[self.overlay],
                         (str(overlay._SPLIT_ROWS), "1", "0"))

    #: What `TERM` the measuring client attaches under. A tmux client refuses to start on
    #: a terminal it cannot look up — `open terminal failed` — and a CI runner's `TERM` is
    #: often `dumb` or absent, which is what turned this measurement into an empty read on
    #: the first CI run. The operator's own comes first where it is usable;
    #: `tests/test_a_real_click_on_a_real_tab_bar_switches` keeps the identical ladder for
    #: the identical reason.
    TERMS = tuple(dict.fromkeys(
        ([os.environ["TERM"]] if os.environ.get("TERM", "dumb") != "dumb" else [])
        + ["xterm-256color", "screen", "vt100"]))

    def _attached(self):
        """A real client on a real pty, and the master end to read it from.

        ``None`` when no `TERM` this machine has lets a client start at all — which is not
        a failure of the property below, it is the absence of anything to measure it with.
        """
        for term in self.TERMS:
            master, slave = os.openpty()
            fcntl.ioctl(slave, termios.TIOCSWINSZ,
                        struct.pack("HHHH", self.ROWS, self.COLS, 0, 0))
            client = subprocess.Popen(
                ["tmux", "-L", SOCKET_NAME, "attach", "-t", "t"],
                stdin=slave, stdout=slave, stderr=slave, start_new_session=True,
                env=dict(os.environ, TERM=term))
            os.close(slave)
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline:
                if self._tmux("list-clients", "-t", "t").stdout.strip():
                    def _stop():
                        client.terminate()
                        client.wait(timeout=10)
                    self.addCleanup(_stop)
                    self.addCleanup(os.close, master)
                    os.set_blocking(master, False)
                    return master
                time.sleep(0.05)
            client.terminate()
            client.wait(timeout=10)
            os.close(master)
        return None

    @staticmethod
    def _drain(master: int, seconds: float = 0.8) -> bytes:
        out, end = b"", time.monotonic() + seconds
        while time.monotonic() < end:
            try:
                out += os.read(master, 65536)
            except BlockingIOError:
                time.sleep(0.02)
            except OSError:
                break
        return out

    def _ask_for_mouse(self, master: int) -> bytes:
        """Make the overlay pane withdraw and re-request mouse reporting, and answer with
        whatever the client's terminal received.

        The withdrawal first because a mode tmux is already in produces no new bytes: what
        travels to the outer terminal is the CHANGE, so a measurement that only ever asked
        would be reading the transition it happened to catch.
        """
        tty = self._tmux("display-message", "-p", "-t", self.overlay,
                         "#{pane_tty}").stdout.strip()
        with open(tty, "wb", buffering=0) as f:
            f.write(overlay.MOUSE_OFF.encode())
            self._drain(master, 0.4)
            f.write(overlay.MOUSE_ON.encode())
        return self._drain(master)

    def test_the_unzoomed_drawer_still_owns_the_terminals_mouse_mode(self):
        """**The half of #739 that mattered and the half that does not.**

        The zoom was never what made the pointer work — `select-pane` is, because the outer
        terminal's mouse mode follows the ACTIVE pane (§4i) — and `modal_argvs` keeps it.
        Measured through a real client on a real pty rather than argued.

        **With its own positive control**, which is this repo's rule for a measurement that
        depends on the machine (`test_the_hotkey_injection_this_guards_against_is_live_on
        _this_tmux`): the ZOOMED overlay is the state charter ships, so if its request does
        not reach the client either, this environment cannot show the property at all and
        there is nothing here for the unzoom to break. That case skips and says so, rather
        than reporting a defect in charter for a `TERM` a runner did not have.
        """
        master = self._attached()
        if master is None:
            self.skipTest("no client would attach on any of "
                          f"{self.TERMS!r} — nothing to measure the request against")
        self._drain(master, 1.0)
        for cmd in overlay.modal_argvs(SOCKET_NAME, harness=self.harness,
                                       overlay_pane=self.overlay):
            self._run(cmd)
        control = self._ask_for_mouse(master)
        if b"1006h" not in control or b"1000h" not in control:
            self.skipTest(
                "this tmux/terminal does not propagate a pane's mouse request to the "
                f"client even ZOOMED ({control!r}), so the unzoom cannot be measured "
                "against it")

        self._run(overlay.unzoom_argv(SOCKET_NAME, overlay_pane=self.overlay))
        self.assertEqual(self._panes()[self.overlay],
                         (str(overlay._SPLIT_ROWS), "1", "0"))
        saw = self._ask_for_mouse(master)
        self.assertIn(b"1006h", saw, "the drawer's SGR request never left tmux")
        self.assertIn(b"1000h", saw, "the drawer's mouse request never left tmux")


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
