"""A wheel notch and a click on a REAL `repos` pane, in a REAL tmux, change what is drawn.

The report this answers is *"I tried to scroll the repo table and nothing happened"*, and
nothing about that report could have been settled by a unit test: every link in it is a
claim about tmux or about a separate process. `tests/test_the_repo_table_scrolls_and_selects
.py` says what the viewport does with an event and what the renderer does with an offset;
only this can say that a report injected into a client's terminal becomes an offset at all.

Everything here is real: a real tmux server, a real attached client on a real pty, two real
`charter panel` processes each holding their own pane, and the frame's own private config
sourced from `commands_frame.conf_text` rather than hand-written. What that buys over the
unit half, link by link:

* that `charter panel repos` builds a dispatcher for a BUILT-IN component at all — until
  this change that branch of `panel._run` built no registry, so charter's own components
  were the one kind that could declare `events` and never be delivered any;
* that the pane really asks its terminal to report (`overlay.MOUSE_ON` written to fd 1 by a
  process nobody in this test can monkey-patch);
* that tmux routes the report to the pane under the pointer, in that pane's own cells, and
  **leaves the keyboard on the harness** — which is what makes a clickable table something
  an operator can use rather than a trap;
* that the click's `state.record_selection` + `state.bump` reach the OTHER pane: the
  attention row is a third process, and the detail appearing there is the only proof that
  the two panels are talking through the frame's state rather than through a shared object
  that only exists in one interpreter.

**`mouse = true`, because that is the regime this feature is for.** With charter's flag off,
tmux asks the terminal to report from the ACTIVE pane's own request alone, and the active
pane here is a `cat` — so nothing would be sent, nothing would arrive, and a green test
would be measuring an empty room. `docs/frame.md` states that half to the operator; this
states the other half.

**And it is run over TWO plane shapes, which is #663.** The first version of this file was
built on a many-clones plane and went green while the wheel was a complete no-op on the
plane charter itself runs on — one clone with fourteen worktrees, where the bound was
computed from a repo count that is always 1. A harness that can only be pointed at one
shape is a harness that reports on the shape somebody happened to pick, so the frame is
built once in `_ARealFrame` and the CACHE is what a subclass supplies.
"""

from __future__ import annotations

import fcntl
import os
import shutil
import struct
import subprocess
import sys
import termios
import time
import unittest
from pathlib import Path

from charter import commands_frame, config, statusline, tui
from charter.frame import gather, layout, slots, state, tmuxctl

from tests import _tmuxreap
from tests._isolation import PersonaIso

_HAS_TMUX = shutil.which("tmux") is not None

_REPO_ROOT = Path(__file__).resolve().parents[1]

SOCKET = _tmuxreap.name("repo-point")
SOCKET_PATH = os.path.join(os.environ.get("TMUX_TMPDIR") or "/tmp",
                           f"tmux-{os.getuid()}", SOCKET)

_DEADLINE = 25.0

_TERM_CANDIDATES = tuple(dict.fromkeys(
    ([os.environ["TERM"]] if os.environ.get("TERM", "dumb") != "dumb" else [])
    + ["xterm-256color", "screen", "vt100"]))

#: Wide enough for the table — `statusline._LEFT_W` is 95 and the pane must clear it, or
#: `_repos` correctly refuses to draw a cut table and every case here would be measuring
#: that refusal instead.
COLS = 120

#: Rows the `repos` pane is split with: one heading and four rows of table. Four rows is
#: the STARVED shape for both plane shapes below — three rows of content and `…(N below)` —
#: which is the shape both reports were about. A pane sized to its content would have
#: nothing to scroll, which is the case the unit half pins and this one cannot reach
#: without a second frame.
REPOS_ROWS = 5


def _row(name, *, repo=None, worktree_count=0) -> dict:
    d = {"name": name, "branch": "main", "dirty": False, "tracked_dirty": False,
         "ahead": 0, "behind": 0, "ci": None, "change": None, "sigil": "",
         "current": False, "worktree_count": worktree_count}
    if repo is not None:
        # A piece row's parent clone — `gather.scan` writes it on every `worktrees` entry,
        # and it is what makes a nested row resolve to a repo when it is clicked.
        d["repo"] = repo
    return d


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


def _fork_pty(rows: int, cols: int) -> tuple[int, int]:
    """`pty.fork` with the window size set BEFORE the exec — #648's fix, borrowed.

    `tests/test_frame_input_reaches_a_component.py` carries the measurement: a client born
    at the kernel's 80x24 default attaching to a wider session makes tmux resize the window,
    which is a SIGWINCH in every pane. Here that would repaint the table for a reason no
    case asked for, which is exactly the noise these cases have to be free of.
    """
    master, slave = os.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
    pid = os.fork()
    if pid == 0:
        os.close(master)
        os.login_tty(slave)
        return 0, -1
    os.close(slave)
    return pid, master


class _ARealFrame(PersonaIso):
    """One frame with charter's OWN `repos` and `attention` panels, and a real pointer.

    **A base, and the two shapes below are what it exists for.** #663 is the measurement
    that the shape a case is built on decides whether the feature is even switched on: a
    scroll harness built only on many-clones planes went green while the plane it shipped
    to — one clone, many worktrees — could not move a row. So the frame is built once here
    and the CACHE is what a subclass supplies (:meth:`_seed`), which makes "a different
    plane shape" a two-line subclass rather than a reason not to test the other one.
    """

    #: Rows the `repos` pane is split with, heading included. A subclass overrides it to
    #: starve the table by however much its own shape needs.
    ROWS = REPOS_ROWS

    def _seed(self) -> dict:
        """The gather cache this frame's `repos` pane draws from."""
        raise NotImplementedError

    def setUp(self) -> None:
        super().setUp()
        v = tmuxctl.version()
        if v is None or v < tmuxctl.FLOOR:
            self.skipTest(f"the frame's floor is tmux {tmuxctl.FLOOR[0]}."
                          f"{tmuxctl.FLOOR[1]}; this machine has {v}")
        self.addCleanup(self._teardown_socket)
        (self.tmp / "charter.toml").write_text("")

        self.fid = state.frame_id("repo-point", os.getpid())
        state.frame_dir(self.fid, create=True)
        # Written through `gather.save` — the cache a real scan writes and `slots._repos`
        # reads — rather than by running a scan, which would need real clones on disk to
        # say the same thing about tmux.
        self.cache = self._seed()
        self.repos = [r["name"] for r in self.cache["repos"]]
        gather.save(self.fid, self.cache)

        existing = os.environ.get("PYTHONPATH", "")
        parts = [str(_REPO_ROOT)] + ([existing] if existing else [])
        self.env = dict(os.environ, CHARTER_ROOT=str(self.tmp),
                        PYTHONPATH=os.pathsep.join(parts))
        self.env.pop("CHARTER_HOME", None)

        started = _tmux("-f", "/dev/null", "new-session", "-d", "-s", self.fid,
                        "-x", str(COLS), "-y", "24", "-P", "-F", "#{pane_id}", "cat",
                        env=self.env)
        self.assertEqual(started.returncode, 0, started.stderr)
        self.harness = started.stdout.strip()
        state.record_server(self.fid, SOCKET)
        state.record_harness_pane(self.fid, self.harness)

        # **`mouse=True`, sourced from charter's own `conf_text`.** Both halves matter:
        # the flag is the regime this feature is for, and asking `conf_text` for the text
        # rather than writing `set mouse on` by hand is what makes the `MouseDown1Pane`
        # rebind (#634) part of what is under test — a hand-written config would leave
        # tmux's default binding in place and the "keyboard stays on the harness" case
        # below would be measuring a config this file invented.
        conf = str(self.tmp / "frame.conf")
        with open(conf, "w") as fh:
            fh.write(commands_frame.conf_text(hotkey=config.FRAME["hotkey"], mouse=True,
                                              history_limit=100, session=self.fid))
        sourced = _tmux("source-file", conf)
        self.assertEqual(sourced.returncode, 0, sourced.stderr)

        self.attention = self._split("bottom", rows=1)
        self.repos_pane = self._split("repos", rows=self.ROWS)
        # **The harness is put back in front, and that is the regime, not tidiness.**
        # `split-window` selects the pane it made, and a frame whose repo table was the
        # ACTIVE pane would be reporting the mouse because THAT pane asked — which is the
        # `mouse = false` route `docs/frame.md` describes and would make every case below
        # pass with charter's flag off. With the harness active, the only thing that can
        # be making the terminal report is `[frame] mouse`, which is what these cases are
        # about. `commands_frame._split_panels` ends the same way for the same reason.
        selected = _tmux("select-pane", "-t", self.harness)
        self.assertEqual(selected.returncode, 0, selected.stderr)

        size = self._window_size()
        self.assertNotEqual(size, (-1, -1), "tmux would not say how big its window is")
        self.fd = self._attach(size)
        self.assertTrue(_await(lambda: self._window_size() == size),
                        "attaching the client resized the window, so every panel took a "
                        "SIGWINCH the cases below would read as an event")

        head = f"▪ repos {len(self.repos)}"
        self.assertTrue(_await(lambda: head in self._shown(self.repos_pane)),
                        f"the table never painted: {self._shown(self.repos_pane)!r}")
        self.assertTrue(_await(lambda: self._shown(self.attention).strip() != ""),
                        "the attention row never painted")
        self.assertEqual(self._active(), self.harness,
                         "the harness is not the active pane, so a report reaching a "
                         "panel would prove nothing about `[frame] mouse`")

    # -- the frame ---------------------------------------------------------- #

    def _split(self, slot: str, *, rows: int) -> str:
        """One of charter's OWN panels, run through the argv its launcher would run and
        marked the way its launcher marks it (`commands_frame._panel_mark_argv`, called
        rather than re-spelled — that mark is what the `MouseDown1Pane` bind asks about)."""
        argv = layout.panel_command(slot=slot, session=self.fid)
        r = _tmux("split-window", "-t", self.harness, "-v", "-l", str(rows),
                  "-P", "-F", "#{pane_id}", "--", *argv, env=self.env)
        self.assertEqual(r.returncode, 0, r.stderr)
        pane = r.stdout.strip()
        marked = subprocess.run(
            commands_frame._panel_mark_argv(socket=SOCKET, pane_id=pane),
            capture_output=True, text=True, timeout=20)
        self.assertEqual(marked.returncode, 0, marked.stderr)
        return pane

    def _attach(self, size: tuple[int, int]) -> int:
        cols, rows = size
        refusals = []
        for term in _TERM_CANDIDATES:
            pid, fd = _fork_pty(rows=rows, cols=cols)
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
        self.skipTest("no tmux client will attach on this machine, and a pointer needs "
                      "one — tried TERM=" + ", ".join(refusals))

    @staticmethod
    def _reap(pid: int, fd: int) -> None:
        for call in (lambda: os.kill(pid, 9), lambda: os.waitpid(pid, 0),
                     lambda: os.close(fd)):
            try:
                call()
            except OSError:
                pass

    def _teardown_socket(self) -> None:
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
        self.assertTrue(path.startswith("/"), f"tmux would not say where its socket is")
        self.assertFalse(os.path.exists(path), f"{path} survived this test's teardown")

    # -- reading the frame back --------------------------------------------- #

    def _shown(self, pane: str) -> str:
        return _tmux("capture-pane", "-p", "-t", pane).stdout

    def _painted(self, pane: str) -> str:
        """The pane WITH its escapes (`capture-pane -e`), which is the only way to ask
        whether a row is highlighted rather than merely present."""
        return _tmux("capture-pane", "-p", "-e", "-t", pane).stdout

    def _table(self) -> list[str]:
        """The table's rows as plain text, heading dropped, blank tail dropped."""
        rows = [ln.rstrip() for ln in self._shown(self.repos_pane).split("\n")]
        return [ln for ln in rows[1:] if ln.strip()]

    def _window_size(self) -> tuple[int, int]:
        """The WINDOW's columns and rows, asked of tmux rather than re-spelled from the
        `new-session` above (#514's rule). A pair no window has when the request fails, so
        `setUp`'s comparison fails rather than something unpacking a traceback."""
        r = _tmux("display-message", "-p", "-t", self.fid,
                  "#{window_width} #{window_height}")
        said = r.stdout.split()
        if r.returncode != 0 or len(said) != 2:
            return (-1, -1)
        return int(said[0]), int(said[1])

    def _rect(self, pane: str) -> tuple[int, int, int, int]:
        r = _tmux("display-message", "-p", "-t", pane,
                  "#{pane_left} #{pane_top} #{pane_width} #{pane_height}")
        self.assertEqual(r.returncode, 0, r.stderr)
        left, top, w, h = (int(n) for n in r.stdout.split())
        return left, top, w, h

    def _active(self) -> str:
        return _tmux("display-message", "-p", "#{pane_id}").stdout.strip()

    def _inject(self, payload: bytes) -> None:
        os.write(self.fd, payload)

    def _point(self, pane: str, *, row: int, col: int = 2, button: int = 0,
               release: bool = True) -> None:
        left, top, _w, _h = self._rect(pane)
        wcol, wrow = left + col + 1, top + row + 1
        self._inject(b"\x1b[<%d;%d;%dM" % (button, wcol, wrow))
        if release:
            self._inject(b"\x1b[<%d;%d;%dm" % (button, wcol, wrow))

    def _wheel(self, pane: str, *, down: bool, row: int = 1, col: int = 2) -> None:
        """One notch. 64 is the wheel with bit 6 set, 65 the other direction — the
        numbering `overlay._SGR_WHEEL` names, sent the way a reporting terminal sends it
        (a press with no release, which is the second reason that module keeps no press
        state)."""
        left, top, _w, _h = self._rect(pane)
        self._inject(b"\x1b[<%d;%d;%dM" % (64 + (1 if down else 0),
                                           left + col + 1, top + row + 1))


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class ARealPointerOnTheRealRepoTable(_ARealFrame):
    """**Many clones, no pieces** — the shape #658 was built and measured on.

    Six repos into four rows of table: three repo rows and `…(3 below)`, so there is
    somewhere below to scroll TO. A pane sized to its content would have nothing to
    scroll, which is the case the unit half pins and this one cannot reach.
    """

    def _seed(self) -> dict:
        return {"gathered_at": 0.0, "workspace": "w", "current_repo": None,
                "repos": [_row(f"repo{i}") for i in range(6)], "worktrees": []}

    def test_a_click_on_a_row_selects_that_repo_and_highlights_it(self):
        """The report, answered end to end. Nothing here bumps the frame's version by
        hand and nothing resizes anything, so a table that changed changed because of the
        click."""
        first = self._table()[0]
        name = first.split()[1]
        self.assertIn(name, self.repos, f"the first row is not a repo row: {first!r}")
        self._point(self.repos_pane, row=1)
        self.assertTrue(_await(lambda: state.selection(self.fid) == name),
                        f"the click never reached the panel: "
                        f"{state.selection(self.fid)!r}")
        self.assertTrue(
            _await(lambda: "\x1b[7m" in self._painted(self.repos_pane).split("\n")[1]),
            f"the selected row was never highlighted: "
            f"{self._painted(self.repos_pane)!r}")

    def test_the_click_leaves_the_keyboard_on_the_harness(self):
        """The property `mouse = true` used to take away and #634 gave back, asserted
        here against charter's OWN panel rather than a fixture's. A table you cannot click
        without losing your prompt is not a table an operator will click twice."""
        self.assertEqual(self._active(), self.harness)
        self._point(self.repos_pane, row=1)
        self.assertTrue(_await(lambda: state.selection(self.fid) is not None))
        self.assertEqual(self._active(), self.harness,
                         "clicking the repo table moved the keyboard off the harness")

    def test_the_attention_row_in_its_own_pane_learns_what_was_clicked(self):
        """**Two processes.** The click lands in the `repos` panel and the detail is drawn
        by the `bottom` panel, which has its own tty, its own loop and no memory in common
        with the other. The only thing between them is `state.record_selection` and the
        `state.bump` that wakes the poll — so this passing is the only proof that the
        cross-pane half is real and not an artefact of one interpreter."""
        name = self._table()[0].split()[1]
        self._point(self.repos_pane, row=1)
        self.assertTrue(_await(
            lambda: f"▪ {name}" in self._shown(self.attention)),
            f"the attention row never learned the selection: "
            f"{self._shown(self.attention)!r}")

    def test_a_click_on_the_heading_selects_nothing(self):
        """Row 0 of the pane is `▪ repos 6`, which is about no repo. It is a row the
        operator can see and click, so "nothing happens" has to be what happens rather
        than the nearest row being picked for them."""
        self._point(self.repos_pane, row=0)
        time.sleep(1.0)
        self.assertIsNone(state.selection(self.fid))

    def test_the_wheel_moves_the_window_over_the_repos(self):
        """The literal report. Six repos into four rows: three rows and `…(3 below)`, so
        there is somewhere below to go, and one notch must change WHICH repos are drawn."""
        before = self._table()
        self.assertIn("(3 below", "\n".join(before), before)
        self._wheel(self.repos_pane, down=True)
        self.assertTrue(_await(lambda: self._table() != before),
                        f"a wheel notch changed nothing: {before!r}")
        after = self._table()
        self.assertNotEqual(
            [ln.split()[1] for ln in after if ln.split()[1] in self.repos],
            [ln.split()[1] for ln in before if ln.split()[1] in self.repos])

    def test_the_wheel_comes_back_to_the_table_it_started_on(self):
        """Offset zero is the table that was always there, which is the compatibility
        claim every plane that never scrolls depends on — asserted here against the real
        rendered pane rather than against the function that composes it."""
        before = self._table()
        self._wheel(self.repos_pane, down=True)
        self.assertTrue(_await(lambda: self._table() != before))
        self._wheel(self.repos_pane, down=False)
        self.assertTrue(_await(lambda: self._table() == before),
                        f"scrolling back did not land on the table it left: "
                        f"{self._table()!r} != {before!r}")

    def test_the_wheel_never_selects(self):
        """Two gestures, two pieces of state — and the one that would be a hover if
        charter had asked for motion reporting. It did not (SGR 1000, not 1003), and a
        wheel that moved the selection would be that hover arriving by the back door."""
        self._wheel(self.repos_pane, down=True)
        time.sleep(1.0)
        self.assertIsNone(state.selection(self.fid))

    def test_the_table_says_the_same_thing_it_would_with_no_pointer_at_all(self):
        """The paint is not disturbed by the mode `events.Dispatcher.open` installs.
        cbreak and never raw is the reason (`frame/events.py` measured the staircase
        `tty.setraw` draws through `panel._write`'s `\\n`-joined rows), and a `repos` pane
        is the first of charter's own to run in that mode — so the rows must still start
        in column 0, every one of them."""
        rows = [tui.strip_ansi(ln) for ln in self._shown(self.repos_pane).split("\n")
                if ln.strip()]
        self.assertGreater(len(rows), 1, rows)
        for ln in rows:
            indent = len(ln) - len(ln.lstrip(" "))
            # The heading sits in column 0 and a table row is inset by `slots.INSET`. A
            # raw-mode staircase indents each row by the WIDTH OF THE ONE ABOVE — ninety
            # columns here, not four — so this bound is nowhere near the failure it names
            # and cannot be met by accident.
            self.assertLessEqual(indent, slots.INSET,
                                 f"a row was shifted right — the pane is in raw mode, "
                                 f"not cbreak: {ln!r}")
        self.assertTrue(rows[0].startswith("▪ repos"), rows[0])


#: Pieces on the one-clone plane. Nine into three rows of content is the shape #663
#: reported and the shape the control plane this ships from actually has — and the number
#: is over `statusline._MAX_REPO_LINES`-shaped starvation rather than under it, so nothing
#: here is measuring a pane that happens to fit.
PIECES = 9


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class ARealWheelOverAPlaneWithOneCloneAndManyPieces(_ARealFrame):
    """**One clone, many worktrees** — the plane #658 shipped to and could not scroll.

    Every case above is on a many-clones plane, which is why #658 went green while the
    feature was a no-op for its own operator: `_scroll_limit` counted REPOS, and one repo
    always fits. This class is the same real frame with the same real pointer over the
    other shape, and it is deliberately the shape with no second clone in it — a case that
    passed because there were two repos to move between would say nothing about #663.

    Ten rows of content (one clone plus nine pieces) into four rows of table, so seven
    rows of the plane are off screen at the top of the scroll.
    """

    def _seed(self) -> dict:
        return {"gathered_at": 0.0, "workspace": "w", "current_repo": None,
                "repos": [_row("solo", worktree_count=PIECES)],
                "worktrees": [_row(f"piece{i}", repo="solo") for i in range(PIECES)]}

    def _shown_names(self) -> list[str]:
        """The names on the table's rows, the overflow line dropped.

        Read as *the token after the tree glyph*, because a piece row carries one more
        glyph than a repo row (`│  ├─` against `├─`) and a fixed column index therefore
        reads the wrong word on one of the two shapes — which is exactly the kind of
        near-miss that would let a case go green on a table that never moved. The
        overflow line has no glyph and drops out on its own.
        """
        glyphs = (statusline._TREE_MID.strip(), statusline._TREE_WT.strip(),
                  statusline._TREE_END.strip())
        out = []
        for ln in self._table():
            parts = ln.split()
            for i, tok in enumerate(parts[:-1]):
                if tok in glyphs:
                    out.append(parts[i + 1])
                    break
        return out

    def test_the_wheel_moves_the_window_over_the_worktrees(self):
        """**#663, literally.** A wheel notch into this pane's own pty used to move
        nothing at all: the pane renders ROWS and the bound was computed from the repo
        count, so a plane with one clone answered 0 however many pieces hung off it."""
        before = self._table()
        self._wheel(self.repos_pane, down=True)
        self.assertTrue(_await(lambda: self._table() != before),
                        f"a wheel notch changed nothing: {before!r}")

    def test_the_last_worktree_is_reachable(self):
        """Not merely "something moved": the row at the BOTTOM of the plane has to come
        into view, which is what the overflow line's reserved row costs and what a limit
        that forgot to subtract it would leave permanently one notch out of reach."""
        last = f"piece{PIECES - 1}"
        self.assertNotIn(last, self._shown_names(), "the fixture already fits")
        for _ in range(PIECES + 2):
            self._wheel(self.repos_pane, down=True)
        self.assertTrue(_await(lambda: last in self._shown_names()),
                        f"the bottom of the plane never came into view: "
                        f"{self._table()!r}")

    def test_the_pieces_that_do_not_fit_are_admitted_rather_than_dropped(self):
        """The other half of the report — "the worktrees past the pane's height are
        simply dropped". A table that shows three of ten rows and says nothing is the
        false-clean reading this module refuses everywhere else, so the count on the
        overflow line has to be a count of ROWS and include the pieces."""
        self.assertIn("(7 below", "\n".join(self._table()), self._table())

    def test_a_click_on_a_worktree_row_selects_its_clone(self):
        """**No drawn row answers nobody.** Asserted with the table SCROLLED, so the only
        row that could answer `solo` is a piece row — at offset zero the clone has a row
        of its own and a click that silently resolved to it would pass either way."""
        self._wheel(self.repos_pane, down=True)
        self.assertTrue(_await(lambda: "solo" not in self._shown_names()),
                        f"the clone's own row is still on screen: {self._table()!r}")
        self._point(self.repos_pane, row=1)
        self.assertTrue(_await(lambda: state.selection(self.fid) == "solo"),
                        f"a piece row selected nothing: {state.selection(self.fid)!r}")

    def test_the_wheel_comes_back_to_the_table_it_started_on(self):
        """Offset zero is where scrolling back lands, on this shape too."""
        before = self._table()
        self._wheel(self.repos_pane, down=True)
        self.assertTrue(_await(lambda: self._table() != before))
        self._wheel(self.repos_pane, down=False)
        self.assertTrue(_await(lambda: self._table() == before),
                        f"scrolling back did not land on the table it left: "
                        f"{self._table()!r} != {before!r}")


#: Worktrees on the plane below — three, so one clone and three pieces is four rows in the
#: four rows of table this pane has. `layout.repos_rows` sizes the real pane to its content,
#: so this is the ORDINARY plane and the one the wheel must be inert on.
PIECES_THAT_FIT = 3


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class ARealWheelOverAPaneThatFitsItsPieces(_ARealFrame):
    """**The tested nothing, on a real frame** — the same plane shape, tall enough.

    The unit half asserts `slots._scroll_limit(15, 15) == 0` and one either side of it, and
    that is the claim that matters; this is the claim that a real pane, with a real panel
    process taking real SGR reports off its own pty, spends nothing on them either. The two
    zeros #663 is about — *the pieces fit* and *the pieces were never counted* — look the
    same from outside, and the class above is what tells them apart.
    """

    def _seed(self) -> dict:
        return {"gathered_at": 0.0, "workspace": "w", "current_repo": None,
                "repos": [_row("solo", worktree_count=PIECES_THAT_FIT)],
                "worktrees": [_row(f"piece{i}", repo="solo")
                              for i in range(PIECES_THAT_FIT)]}

    def test_every_row_of_the_plane_is_on_screen_and_nothing_admits_otherwise(self):
        """Four rows of content in four rows of table: no overflow line and no `⑂N`,
        because there is nothing this pane is not showing. The badge is the half that would
        go wrong in the other direction — counted from the pieces DRAWN, it must vanish
        here.

        Asserted on `…(`, the marker every shape of that line starts with, rather than on
        one wording of it: this case used to look for `more)` and #741 deleted that word,
        which would have left it passing against a line it had stopped being able to see."""
        rows = self._table()
        self.assertEqual(len(rows), 1 + PIECES_THAT_FIT, rows)
        self.assertNotIn("…(", "\n".join(rows))
        self.assertNotIn("⑂", "\n".join(rows))

    def test_the_wheel_moves_nothing_on_a_pane_that_fits(self):
        """**A negative measured against a panel proved to be awake.** A pane that never
        repaints would pass "nothing changed" for the wrong reason, which is the whole
        failure mode #663 is: a green measurement of an empty room. So a click is landed
        first and its highlight waited for — that IS this panel repainting on this pty —
        and only then is the wheel asserted to cost nothing."""
        before = self._table()
        self._point(self.repos_pane, row=1)
        self.assertTrue(_await(lambda: state.selection(self.fid) == "solo"),
                        "the panel never took the click, so a wheel that changed nothing "
                        "would prove nothing")
        self.assertTrue(
            _await(lambda: "\x1b[7m" in self._painted(self.repos_pane).split("\n")[1]),
            "the panel never repainted, so it is not awake to be measured")
        for _ in range(3):
            self._wheel(self.repos_pane, down=True)
        time.sleep(1.0)
        self.assertEqual(self._table(), before,
                         "a pane tall enough for its content scrolled")


if __name__ == "__main__":
    unittest.main()
