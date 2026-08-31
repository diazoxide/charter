"""#730: a workspace switch repaints the repo strip without re-sizing the pane it draws in.

The strip's height is a function of its CONTENT — `layout.repos_rows` over
`frame_slots.repos_rows_wanted`, which counts the frame's own workspace — and it is
recomputed at launch and on every `window-resized`. A workspace switch changes the content
and nothing re-runs the arithmetic, so the pane keeps the height of the workspace you left
and the difference comes out of the agent's session:

```
▪ repos 2
  ├─ api                               main
  └─ ledger                            main
                                       <- five blank rows, taken from the harness
```

It cuts the other way too: switching from a 1-clone workspace to an 8-clone one draws the
table into the pane the small one wanted, so two real repos are replaced by `…(+2 more)`
on a terminal with room for all of them.

**This is not #714's defect one workspace over, and the distinction is the fix.** #714 was
about WHICH panes exist and answered it by asking tmux. This is about how tall an existing
pane is, and charter already has the answer in one place: `_reassert_sizes` recomputes
every height from the frame's current content and applies it, and `cmd_resize` calls it on
every terminal drag. `switch.to_workspace` re-gathers the cache and bumps, and never makes
that call. So the fix is a call, not a mechanism — which is why what is asserted below is
mostly *the same* call, made from a second place, with the same refusals in front of it.

Four properties, one class each:

**The switch re-asserts the heights, and it does it with the NEW workspace's count**
(`ASwitchResizesTheStripForTheWorkspaceItArrivesAt`). Sizing from the count charter had
before the gather would be the defect with an extra function in front of it.

**Before the bump, not after** (`TheOrderIsTheOneTheCacheAlreadyEstablished`). A panel's
poll loop reads the version and then repaints; the bump is what makes it repaint. Resizing
afterwards means the panel has already drawn its table into the old rectangle, and nothing
bumps again — the same argument `to_workspace` already makes for refreshing the cache
before the bump, one fact over.

**Every refusal `cmd_resize` makes is made here too**
(`AFrameThatCannotBeMeasuredIsLeftAlone`). This runs from a keypress against a frame that
may have no panes recorded, no readable harness pane, or a window tmux will not report a
size for — and re-asserting a layout onto a window whose size charter had to guess is the
destructive move #501 removed from the resize path. Shared code, so shared refusals.

**And the pane really changes height** (`TheStripReallyChangesHeightOnARealServer`).
`resize-pane` is a command charter sends; that the pane ends up shorter is a fact about
tmux, and the issue is a screenshot of pane heights. Measured on tmux 3.7c and at
`tmuxctl.FLOOR`.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import unittest
from pathlib import Path
from unittest import mock

from charter import commands_frame, config, workspace as ws_mod
from charter.frame import gather, layout, state, switch, tmuxctl

from tests import _tmuxreap
from tests._isolation import PersonaIso

_HAS_TMUX = shutil.which("tmux") is not None

SOCKET = _tmuxreap.name("switch-resize")

_REPO_ROOT = Path(__file__).resolve().parent.parent
_REAL_STATE_DIR = Path(config.STATE_DIR)


def _clones(ws: str, n: int) -> None:
    """*n* real clones in workspace *ws* — a directory whose `.git` is a DIRECTORY, which
    is the line `workspace.is_clone` draws and the only thing `gather.row_count`'s
    no-cache path needs. No git, no subprocess: the count is a filesystem fact, and this
    module is about what charter does with the count rather than how it arrives at one."""
    for i in range(n):
        (config.WORKSPACES_DIR / ws / f"r{i}" / ".git").mkdir(parents=True, exist_ok=True)


class _Tmux:
    """A recording stand-in for `tmuxctl.run` that answers the two things a re-assertion
    reads: the window's size, and — since #714 — which panes the window holds."""

    def __init__(self, *, size="200:50", window=None):
        self.size = size
        self.window = window or {}
        self.calls: list[list[str]] = []

    def __call__(self, action, argv, *, env=None, timeout=None, report=True):
        self.calls.append(list(argv))
        out = ""
        if "display-message" in argv:
            out = self.size
        elif "list-panes" in argv:
            harness = argv[argv.index("-t") + 1]
            rows = [f"{harness}  "] + [
                f"{p} {commands_frame._PANEL_MARK} {s}"
                for p, s in self.window.items()]
            out = "\n".join(rows)
        return subprocess.CompletedProcess(argv, 0, stdout=out, stderr="")

    def heights(self) -> dict[str, int]:
        """`{pane id: -y}` for every `resize-pane` that set a height."""
        return {c[c.index("-t") + 1]: int(c[c.index("-y") + 1])
                for c in self.calls if "resize-pane" in c and "-y" in c}


class _Frame(PersonaIso, unittest.TestCase):
    """One frame, two workspaces of different sizes, and the pane map a launch records."""

    FID = "sw-resize"
    HARNESS = "%1"
    PANES = {"top": "%2", "bottom": "%3", "repos": "%4"}

    #: Eight clones against one. Far enough apart that `layout.repos_rows`' floor and cap
    #: cannot make the two answers equal, which would be a test that passes because the
    #: arithmetic collapsed rather than because the switch re-ran it.
    BIG, SMALL = 8, 1

    def setUp(self):
        super().setUp()
        # The three identity names are STATED rather than the environment CLEARED, and
        # that distinction is this fixture's own bill: `to_workspace` calls
        # `gather.refresh`, which runs a real `git status` per clone, and a cleared
        # environment takes `$GIT_CONFIG_GLOBAL` and `$PATH` with it — `tests/_gitguard.py`
        # refuses the first and the second simply cannot find `git`. Empty is what every
        # charter reader treats as absent, so `workspace.resolve` still falls through to
        # the pointer this frame writes.
        self.enterContext(mock.patch.dict(
            os.environ, {"CHARTER_WORKSPACE": "", "CHARTER_PERSONA": "",
                         "CHARTER_SESSION_ID": ""}))
        for name in ("alpha", "beta"):
            (config.WORKSPACES_DIR / name).mkdir(parents=True, exist_ok=True)
        _clones("alpha", self.BIG)
        _clones("beta", self.SMALL)
        state.frame_dir(self.FID, create=True)
        state.record_identity(self.FID, {"CHARTER_SESSION_ID": self.FID,
                                         "CHARTER_WORKSPACE": "", "CHARTER_PERSONA": ""})
        state.record_workspace(self.FID, "alpha")
        ws_mod.set_active("alpha", session_id=self.FID, force=True, terminal_id="")
        state.record_harness_pane(self.FID, self.HARNESS)
        state.record_panes(self.FID, panels=dict(self.PANES))
        self.enterContext(mock.patch.object(tmuxctl, "version", return_value=(3, 7)))

    def _switch(self, name="beta", *, fake=None):
        fake = fake or _Tmux(window={p: s for s, p in self.PANES.items()})
        with mock.patch.object(commands_frame.tmuxctl, "run", fake):
            out = switch.to_workspace(self.FID, name)
        return out, fake

    def _sizes(self, ws: str) -> dict[str, int]:
        """What each slot should be for workspace *ws*, computed the way production
        computes it — through `commands_frame._slot_sizes`, not by this test doing the
        arithmetic a second way. Leaves the frame recorded on *ws*, so a caller that wants
        the OTHER workspace's number has to say so."""
        state.record_workspace(self.FID, ws)
        gather.refresh(self.FID, workspace=ws)
        return commands_frame._slot_sizes(
            self.FID, list(self.PANES), window_rows=50,
            pane_cols=layout.repos_cols(list(self.PANES), window_cols=200))

    def _harness_for(self, ws: str) -> int:
        """The harness height a re-assertion computes for *ws* — which is where the
        strip's height actually shows up. See
        `TheStripIsTheRemainderAndIsNeverNamed`."""
        return layout.harness_rows(self._sizes(ws), window_rows=50)


class ASwitchResizesTheStripForTheWorkspaceItArrivesAt(_Frame):
    """The defect, and the one thing that makes the fix a fix rather than a call: the
    height is the ARRIVING workspace's."""

    def test_the_two_workspaces_really_do_want_different_heights(self):
        """The control, and it runs first on purpose. If the eight-clone and one-clone
        workspaces asked for the same strip, every assertion below would pass on a switch
        that resized nothing."""
        big, small = self._sizes("alpha")["repos"], self._sizes("beta")["repos"]
        self.assertGreater(big, small,
                           "the fixture's two workspaces want the same pane, so nothing "
                           "below can tell a re-assertion from its absence")

    def test_the_harness_is_told_a_height_computed_from_the_new_workspaces_content(self):
        """Where the strip's height actually lands. `_reassert_sizes` asserts every slot
        with a CONSTANT size and the harness, and lets the strip take the remainder — so
        "the strip is two rows now" is said by telling the harness it may have the other
        forty-three."""
        want = self._harness_for("beta")
        state.record_workspace(self.FID, "alpha")
        _, fake = self._switch("beta")
        self.assertEqual(fake.heights().get(self.HARNESS), want,
                         f"the harness kept a height that is not beta's: "
                         f"{fake.heights()}")

    def test_a_switch_to_a_bigger_workspace_gives_the_strip_more_rows(self):
        """The other direction, which the issue reports as `…(+2 more)` on a terminal with
        room for every repo. Asserted separately because a fix that only ever shrank would
        pass every case above — and asserted as a smaller HARNESS, which is the same
        sentence from the other end of the boundary `resize-pane -y` moves."""
        big_harness = self._harness_for("alpha")
        small_harness = self._harness_for("beta")
        self.assertLess(big_harness, small_harness,
                        "the fixture cannot tell the two directions apart")
        ws_mod.set_active("beta", session_id=self.FID, force=True, terminal_id="")
        state.record_workspace(self.FID, "beta")
        _, fake = self._switch("alpha")
        self.assertEqual(fake.heights().get(self.HARNESS), big_harness)

    def test_the_strip_itself_is_never_named(self):
        """It is `layout.VARIABLE_ROW_SLOTS` — the stack's dependent pane — and asserting
        it is what `_reassert_sizes`' own #515 measurement forbids: in a stack of N panes
        only N-1 heights are free, and naming all N made the result depend on the order
        the panes happened to be in. Stated here because a reader looking for "the strip
        was resized" will not find it, and would otherwise conclude the fix does nothing."""
        _, fake = self._switch("beta")
        self.assertNotIn(self.PANES["repos"], fake.heights(),
                         "the variable-row slot was given an explicit height, so the "
                         "layout now depends on which pane tmux resized first")
        self.assertEqual(fake.heights().get(self.PANES["top"]), 1)
        self.assertEqual(fake.heights().get(self.PANES["bottom"]), 1)

    def test_a_refused_switch_resizes_nothing(self):
        """The frame did not move, so its content did not change. A re-assertion here
        would be charter acting on a switch it just declined."""
        _, fake = self._switch("nosuchworkspace")
        self.assertEqual(fake.heights(), {},
                         "charter re-laid-out a frame whose switch it refused")


class TheOrderIsTheOneTheCacheAlreadyEstablished(_Frame):
    """Before the bump, for the reason `to_workspace` already refreshes the cache before
    the bump: the bump is what makes a panel repaint, and a panel that repaints into a
    rectangle charter is about to change has drawn its table at the wrong height with
    nothing left to make it draw again."""

    def test_the_resize_is_issued_before_the_frame_is_bumped(self):
        seen: list[str] = []
        fake = _Tmux(window={p: s for s, p in self.PANES.items()})

        def note_bump(fid):
            seen.append("bump")

        def run(action, argv, **kw):
            if "resize-pane" in argv and "-y" in argv:
                seen.append("resize")
            return fake(action, argv, **kw)

        with mock.patch.object(commands_frame.tmuxctl, "run", run), \
                mock.patch.object(switch.state, "bump", note_bump):
            switch.to_workspace(self.FID, "beta")
        self.assertIn("resize", seen, "nothing was resized at all")
        self.assertIn("bump", seen)
        self.assertLess(seen.index("resize"), seen.index("bump"),
                        "the panels were told to repaint before the panes were the size "
                        "they were going to repaint into")

    def test_the_cache_is_refreshed_before_the_height_is_computed(self):
        """`gather.refresh` writes the count the height is computed FROM, so a resize
        ahead of it would size the pane for the workspace being left — the defect with an
        extra call in front of it."""
        seen: list[str] = []
        fake = _Tmux(window={p: s for s, p in self.PANES.items()})

        real_refresh = gather.refresh

        def refresh(fid, **kw):
            seen.append("refresh")
            return real_refresh(fid, **kw)

        def run(action, argv, **kw):
            if "resize-pane" in argv and "-y" in argv:
                seen.append("resize")
            return fake(action, argv, **kw)

        with mock.patch.object(commands_frame.tmuxctl, "run", run), \
                mock.patch.object(gather, "refresh", refresh):
            switch.to_workspace(self.FID, "beta")
        self.assertLess(seen.index("refresh"), seen.index("resize"))


class AFrameThatCannotBeMeasuredIsLeftAlone(_Frame):
    """`cmd_resize`'s refusals, reached from a keypress instead of from a hook — and each
    of them is an input that reaches only that arm, because a switch that resizes nothing
    and a switch that resizes wrongly look identical from the return code (`Outcome.ok` is
    about the SWITCH, and the switch succeeded in every case here)."""

    def test_a_frame_with_no_panes_recorded_resizes_nothing(self):
        """A frame whose panels all failed to draw has nothing to re-assert, and
        `_reassert_sizes` would go on to tell the HARNESS a height derived from an empty
        map."""
        state.record_panes(self.FID, panels={})
        out, fake = self._switch("beta")
        self.assertTrue(out.ok, "the switch itself must still happen")
        self.assertEqual(fake.heights(), {})

    def test_a_harness_pane_that_is_not_tmuxs_own_shape_resizes_nothing(self):
        """#475's rule, on the value this reads off disk before using it as a
        `resize-pane -t` target."""
        state.record_harness_pane(self.FID, "%1;kill-server")
        out, fake = self._switch("beta")
        self.assertTrue(out.ok)
        self.assertEqual(fake.calls, [])

    def test_a_window_tmux_will_not_size_is_left_at_the_size_it_has(self):
        """#501: taking a fallback here and re-asserting an 80x24 layout over a window
        that is very probably not 80x24 is the destructive move, and a keypress has the
        same option a hook does — do nothing, and let the next resize try again."""
        fake = _Tmux(size="", window={p: s for s, p in self.PANES.items()})
        out, fake = self._switch("beta", fake=fake)
        self.assertTrue(out.ok)
        self.assertEqual(fake.heights(), {})

    def test_a_window_that_moved_while_this_was_measuring_is_left_alone(self):
        """#501's second half, and the one a keypress can hit as easily as a drag: the
        size is read, the heights are computed from it, and it is read again before
        anything is applied. A layout applied from a measurement the window has already
        left is a stale layout asserted with confidence."""
        sizes = iter(("200:50", "120:30"))
        fake = _Tmux(window={p: s for s, p in self.PANES.items()})

        def run(action, argv, **kw):
            if "display-message" in argv:
                fake.size = next(sizes, "120:30")
            return fake(action, argv, **kw)

        with mock.patch.object(commands_frame.tmuxctl, "run", run):
            out = switch.to_workspace(self.FID, "beta")
        self.assertTrue(out.ok)
        self.assertEqual(fake.heights(), {},
                         "charter applied a layout computed for a window size the "
                         "window had already left")


@unittest.skipUnless(_HAS_TMUX, "tmux is not installed on this machine")
class TheStripReallyChangesHeightOnARealServer(PersonaIso, unittest.TestCase):
    """`resize-pane` is a command charter sends; that the pane ends up shorter is a fact
    about tmux, and the issue is a screenshot of pane heights.

    Run against tmux 3.7c and against 3.2 — `tmuxctl.FLOOR` — by putting a 3.2 built from
    the release tarball first on `$PATH`. Identical on both, so nothing here is gated on a
    version.
    """

    FID = "sw-real"
    BIG, SMALL = 8, 1

    def setUp(self):
        super().setUp()
        self.assertNotEqual(
            Path(config.STATE_DIR), _REAL_STATE_DIR,
            "this test runs charter's real switch, whose state it would write into the "
            "developer's own control plane")
        self.addCleanup(self._teardown_socket)
        self.enterContext(mock.patch.dict(
            os.environ, {"CHARTER_WORKSPACE": "", "CHARTER_PERSONA": "",
                         "CHARTER_SESSION_ID": ""}))
        for name in ("alpha", "beta"):
            (config.WORKSPACES_DIR / name).mkdir(parents=True, exist_ok=True)
        _clones("alpha", self.BIG)
        _clones("beta", self.SMALL)
        made = self._srv("new-session", "-d", "-s", "w", "-x", "200", "-y", "50",
                         "-P", "-F", "#{pane_id}", "--", "sleep", "100000")
        self.assertEqual(made.returncode, 0, made.stderr)
        self.harness = made.stdout.strip()
        # Panes that stand in for the panels, split and marked exactly as
        # `_split_panels` does it, so what is re-asserted here is re-asserted against the
        # real geometry rather than against a map this test invented.
        # **The flags and the ORDER are production's, and getting them wrong is not a
        # cosmetic difference.** `layout.panel_argvs` gives `top` a `-b` and every other
        # slot a plain `-v`, which tmux places DIRECTLY below the harness — so a slot split
        # later sits ABOVE one split earlier, and the measured top-down order is `top`,
        # harness, `repos`, `bottom` (that function's own docstring, tmux 3.7c). Built the
        # naive way — three plain `-v` splits — this fixture put `top` at the BOTTOM of the
        # stack, and the rows the harness gave back landed in it instead of in the strip:
        # a real re-assertion looked like a no-op against geometry no frame has.
        self.panes = {}
        for slot, size, before in (("top", 1, ["-b"]), ("bottom", 1, []),
                                   ("repos", 9, [])):
            made = self._srv("split-window", "-d", "-t", self.harness, "-v", *before,
                             "-l", str(size), "-P", "-F", "#{pane_id}",
                             "--", "sleep", "100000")
            self.assertEqual(made.returncode, 0, made.stderr)
            self.panes[slot] = made.stdout.strip()
        state.record_harness_pane(self.FID, self.harness)
        state.record_panes(self.FID, panels=dict(self.panes))
        state.record_identity(self.FID, {"CHARTER_SESSION_ID": self.FID,
                                         "CHARTER_WORKSPACE": "", "CHARTER_PERSONA": ""})
        state.record_workspace(self.FID, "alpha")
        ws_mod.set_active("alpha", session_id=self.FID, force=True, terminal_id="")
        state.record_server(self.FID, SOCKET)

    def _teardown_socket(self) -> None:
        """`kill-server` FIRST, then unlink — `addCleanup` runs LIFO, so registering the
        two separately in that order would unlink the socket and then reconnect to
        nothing."""
        self._srv("kill-server")
        (Path("/tmp") / f"tmux-{os.getuid()}" / SOCKET).unlink(missing_ok=True)

    def _srv(self, *args: str) -> subprocess.CompletedProcess:
        return subprocess.run(["tmux", "-L", SOCKET, *args],
                              capture_output=True, text=True, timeout=15)

    def _height(self, pane: str) -> int:
        out = self._srv("display-message", "-p", "-t", pane, "#{pane_height}")
        self.assertEqual(out.returncode, 0, out.stderr)
        return int(out.stdout.strip())

    def test_switching_to_a_smaller_workspace_hands_the_rows_back(self):
        before = self._height(self.panes["repos"])
        harness_before = self._height(self.harness)
        with mock.patch.object(commands_frame, "SOCKET", SOCKET):
            out = switch.to_workspace(self.FID, "beta")
        self.assertTrue(out.ok, out.message)
        after = self._height(self.panes["repos"])
        self.assertLess(after, before,
                        f"the repo strip kept the 8-clone height ({before} rows) after "
                        f"a switch to a 1-clone workspace")
        self.assertGreater(self._height(self.harness), harness_before,
                           "the strip shrank and the harness did not grow, so the rows "
                           "it gave up are blank")

    def test_switching_to_a_bigger_workspace_takes_the_rows_it_needs(self):
        state.record_workspace(self.FID, "beta")
        ws_mod.set_active("beta", session_id=self.FID, force=True, terminal_id="")
        self._srv("resize-pane", "-t", self.panes["repos"], "-y", "2")
        before = self._height(self.panes["repos"])
        with mock.patch.object(commands_frame, "SOCKET", SOCKET):
            out = switch.to_workspace(self.FID, "alpha")
        self.assertTrue(out.ok, out.message)
        self.assertGreater(self._height(self.panes["repos"]), before,
                           "the table was drawn into the small workspace's pane, so "
                           "real repos are replaced by `…(+N more)`")

    def test_a_chat_switch_already_sizes_the_strip_for_the_chat_it_enters(self):
        """**#730's second section is wrong, and this is the measurement that says so.**

        The issue reports the same defect on a chat switch: "`cmd_chat` splits fresh panels
        into the target window, and the target chat's own workspace may have a different
        row count from the one whose height the window carries." It does not reproduce.
        `cmd_chat` step 3 goes through `_apply_arrangement` -> `_relayout`, which sizes its
        splits with `_slot_sizes(fid=<target>)` and then re-asserts every height with
        `_reassert_sizes(fid=<target>)` — both of which read the TARGET chat's own
        workspace, never the one being left.

        Measured on a real server rather than argued from the call graph: a chat on the
        one-clone workspace is given the eight-clone geometry, re-laid-out the way step 3
        does it, and lands on its own number.

        Pinned rather than left alone, because a property that holds by accident of one
        `fid=` argument is a property one refactor away from not holding — and the next
        person to read that issue section will look for the bug it describes.
        """
        other = f"{self.FID}-chat2"
        state.record_harness_pane(other, self.harness)
        state.record_panes(other, panels=dict(self.panes))
        state.record_workspace(other, "beta")
        state.record_server(other, SOCKET)
        ws_mod.set_active("beta", session_id=other, force=True, terminal_id="")
        gather.refresh(other, workspace="beta")
        # The window still carries the eight-clone chat's geometry, which is exactly the
        # state the issue describes.
        self._srv("resize-pane", "-t", self.panes["repos"], "-y", "9")
        before = self._height(self.panes["repos"])
        with mock.patch.object(commands_frame, "SOCKET", SOCKET):
            where = commands_frame._relayout_target(other)
            self.assertIsNotNone(where)
            commands_frame._apply_arrangement(
                other, where=where, want=["top", "bottom", "repos"])
        want = commands_frame._slot_sizes(
            other, list(self.panes), window_rows=50,
            pane_cols=layout.repos_cols(list(self.panes), window_cols=200))["repos"]
        self.assertLess(want, before, "the fixture cannot tell the two chats apart")
        self.assertEqual(self._height(self.panes["repos"]), want,
                         "a chat switch left the strip at the height of the chat that "
                         "was left, which would make #730's second section reproduce")

    def test_no_pane_the_switch_does_not_own_is_touched(self):
        """A pane the operator split for themselves is not part of the arrangement and
        `_reassert_sizes` only ever names slots it has a size for — but the assertion is
        cheap and the failure it catches is somebody's work disappearing."""
        theirs = self._srv("split-window", "-d", "-t", self.harness, "-l", "4",
                           "-P", "-F", "#{pane_id}", "--", "sleep", "100000")
        self.assertEqual(theirs.returncode, 0, theirs.stderr)
        mine = theirs.stdout.strip()
        with mock.patch.object(commands_frame, "SOCKET", SOCKET):
            switch.to_workspace(self.FID, "beta")
        alive = self._srv("list-panes", "-t", self.harness,
                          "-F", "#{pane_id}").stdout.split()
        self.assertIn(mine, alive, "charter closed a pane it did not split")
        self.assertIn(self.harness, alive)
